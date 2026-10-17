use super::*;
use crate::assembly::{Rigid, compose, invert, rigid};
#[allow(unused_imports)]
pub use occt_parametric_core::mesh::export::*;
use serde_json::{Value, json};
use std::fmt::Write;

fn gltf_axis(point: [f64; 3]) -> [f64; 3] {
    [point[0], point[2], -point[1]]
}

/// A family-local rigid placement in glTF axes/meters, independent of mesh
/// centering and the representative instance used to share tessellation.
fn family_local_matrix(m: Rigid) -> [f64; 16] {
    let r = |row: usize, column: usize| m[4 * row + column];
    let a = |row: usize, k: usize| match (row, k) {
        (0, 0) | (1, 2) => 1.0,
        (2, 1) => -1.0,
        _ => 0.0,
    };
    let mut matrix = [0.0; 16];
    for column in 0..3 {
        for row in 0..3 {
            matrix[4 * column + row] = (0..3)
                .flat_map(|i| (0..3).map(move |j| (i, j)))
                .map(|(i, j)| a(row, i) * r(i, j) * a(column, j))
                .sum();
        }
    }
    matrix[12..15].copy_from_slice(&gltf_axis([m[3] * 0.001, m[7] * 0.001, m[11] * 0.001]));
    matrix[15] = 1.0;
    matrix
}

/// Model-space rigid transform from an instance's local geometry to its
/// generated placement: its placement, then each enclosing frame and joint
/// motion, in the order regeneration applies them. O(frame depth).
fn world_transform<'definition>(
    graph: &InstanceGraph<'definition>,
    instance: &str,
    context: &mut ExportContext<'definition>,
) -> Result<Rigid, ModelError> {
    let resolved = graph.resolve_with_placement_cached(instance, &mut context.resolutions)?;
    let mut world = rigid(resolved.placement)?;
    for frame in resolved.frames {
        world = compose(&rigid(frame)?, &world);
    }
    Ok(world)
}

/// A tessellated variant: its scene mesh, center, the world transform of
/// the instance it was tessellated for, and its id and tag names.
struct Tessellated {
    mesh: usize,
    center: [f64; 3],
    world: Rigid,
    source: TaggedSurfaceMesh,
}
impl Scene {
    fn material(&mut self, appearance: MaterialAppearance) -> Result<usize, ModelError> {
        appearance.validate()?;
        let key =
            serde_json::to_vec(&appearance).map_err(|error| ModelError::new(error.to_string()))?;
        if let Some(index) = self.appearances.get(&key) {
            return Ok(*index);
        }
        let index = self.materials.len();
        self.materials.push(json!({ "pbrMetallicRoughness": { "baseColorFactor": appearance.base_color,
            "metallicFactor": appearance.metallic, "roughnessFactor": appearance.roughness },
            "doubleSided": appearance.double_sided, "alphaMode": if appearance.base_color[3] < 1.0 { "BLEND" } else { "OPAQUE" } }));
        self.appearances.insert(key, index);
        Ok(index)
    }
    fn add(
        &mut self,
        mesh: &TaggedSurfaceMesh,
        appearance: MaterialAppearance,
        name: &str,
    ) -> Result<(), ModelError> {
        let (mesh_index, center) = self.add_mesh(mesh, appearance)?;
        self.add_node(mesh_index, center, None, name, mesh);
        Ok(())
    }

    /// Adds (or finds an identical) mesh, centered on its bounds; returns its
    /// index and the center in millimeters. Vertices are welded by exact
    /// float32 position within each B-rep face, with area-weighted normals,
    /// so curved faces shade smoothly while edges between faces stay sharp;
    /// triangles are indexed. A second, line primitive holds the feature
    /// edges: triangle edges between two faces or on one triangle only (the
    /// B-rep edges and open boundaries). O(triangles).
    fn add_mesh(
        &mut self,
        mesh: &TaggedSurfaceMesh,
        appearance: MaterialAppearance,
    ) -> Result<(usize, [f64; 3]), ModelError> {
        let (minimum, maximum) = bounds(mesh)?;
        let center: [f64; 3] =
            std::array::from_fn(|axis| minimum[axis] * 0.5 + maximum[axis] * 0.5);
        let material = self.material(appearance)?;
        mesh.triangles
            .len()
            .checked_mul(3)
            .ok_or_else(|| ModelError::new("mesh vertex count overflows"))?;
        let mut vertex_of: HashMap<(usize, [u32; 3]), u32> = HashMap::new();
        let mut point_of: HashMap<[u32; 3], u32> = HashMap::new();
        let mut points: Vec<[f32; 3]> = Vec::new();
        let mut sums: Vec<[f64; 3]> = Vec::new();
        let mut indices: Vec<u32> = Vec::with_capacity(mesh.triangles.len() * 3);
        let mut edges: HashMap<(u32, u32), EdgeUse> = HashMap::new();
        let mut low = [f32::INFINITY; 3];
        let mut high = [f32::NEG_INFINITY; 3];
        for triangle in &mesh.triangles {
            let mut quantized = [[0.0_f32; 3]; 3];
            for (index, point) in triangle.points.iter().map(|point| xyz(*point)).enumerate() {
                let local = gltf_axis(std::array::from_fn(|axis| {
                    (point[axis] - center[axis]) * 0.001
                }));
                for axis in 0..3 {
                    let value = local[axis] as f32;
                    if !value.is_finite() {
                        return Err(ModelError::new("glTF coordinates exceed float32 range"));
                    }
                    quantized[index][axis] = value;
                    low[axis] = low[axis].min(value);
                    high[axis] = high[axis].max(value);
                }
            }
            let corners =
                quantized.map(|p| Vec3::new(f64::from(p[0]), f64::from(p[1]), f64::from(p[2])));
            manufacturing::triangle_normal(&MeshTriangle {
                face_index: triangle.face_index,
                points: corners,
            })
            .map_err(|error| error.context("glTF float32 conversion collapses a triangle"))?;
            // Area-weighted normal: the unnormalized cross product.
            let [a, b, c] = corners;
            let (u, v) = (subtract(b, a), subtract(c, a));
            let weight = [
                u.y * v.z - u.z * v.y,
                u.z * v.x - u.x * v.z,
                u.x * v.y - u.y * v.x,
            ];
            let mut corner = [0_u32; 3];
            let mut welded = [0_u32; 3];
            for (index, p) in quantized.iter().enumerate() {
                let bits = p.map(f32::to_bits);
                let count = u32::try_from(points.len())
                    .map_err(|_| ModelError::new("glTF mesh has too many vertices"))?;
                let vertex = *vertex_of
                    .entry((triangle.face_index, bits))
                    .or_insert_with(|| {
                        points.push(*p);
                        sums.push([0.0; 3]);
                        count
                    });
                let next = point_of.len() as u32;
                welded[index] = *point_of.entry(bits).or_insert(next);
                let sum = &mut sums[vertex as usize];
                for axis in 0..3 {
                    sum[axis] += weight[axis];
                }
                corner[index] = vertex;
            }
            indices.extend(corner);
            for (i, j) in [(0, 1), (1, 2), (2, 0)] {
                let key = (welded[i].min(welded[j]), welded[i].max(welded[j]));
                edges
                    .entry(key)
                    .and_modify(|edge| {
                        edge.uses += 1;
                        edge.mixed |= edge.face != triangle.face_index;
                    })
                    .or_insert(EdgeUse {
                        face: triangle.face_index,
                        vertices: [corner[i], corner[j]],
                        uses: 1,
                        mixed: false,
                    });
            }
        }
        let count = points.len();
        let mut positions = Vec::with_capacity(count * 12);
        let mut normals = Vec::with_capacity(count * 12);
        for (point, sum) in points.iter().zip(&sums) {
            let length = (sum[0] * sum[0] + sum[1] * sum[1] + sum[2] * sum[2]).sqrt();
            let normal = if length > 0.0 {
                sum.map(|v| v / length)
            } else {
                [0.0, 1.0, 0.0]
            };
            for axis in 0..3 {
                positions.extend_from_slice(&point[axis].to_le_bytes());
                normals.extend_from_slice(&(normal[axis] as f32).to_le_bytes());
            }
        }
        let mut lines: Vec<u32> = edges
            .values()
            .filter(|edge| edge.uses == 1 || edge.mixed)
            .flat_map(|edge| edge.vertices)
            .collect();
        // Stable output: edges in vertex order.
        let mut pairs: Vec<[u32; 2]> = lines.chunks(2).map(|c| [c[0], c[1]]).collect();
        pairs.sort_unstable();
        lines = pairs.into_iter().flatten().collect();
        let short = count <= usize::from(u16::MAX);
        let index_bytes = |values: &[u32]| -> Vec<u8> {
            if short {
                values
                    .iter()
                    .flat_map(|&v| (v as u16).to_le_bytes())
                    .collect()
            } else {
                values.iter().flat_map(|&v| v.to_le_bytes()).collect()
            }
        };
        let (triangle_bytes, line_bytes) = (index_bytes(&indices), index_bytes(&lines));
        let mut key = material.to_le_bytes().to_vec();
        for part in [&positions, &normals, &triangle_bytes, &line_bytes] {
            key.extend_from_slice(&(part.len() as u64).to_le_bytes());
            key.extend_from_slice(part);
        }
        let mesh_index = match self.geometry.get(&key) {
            Some(index) => *index,
            None => {
                let index_type = if short { 5123 } else { 5125 };
                let position =
                    self.accessor(positions, count, 5126, "VEC3", Some((low, high)), 34962);
                let normal = self.accessor(normals, count, 5126, "VEC3", None, 34962);
                let triangles = self.accessor(
                    triangle_bytes,
                    indices.len(),
                    index_type,
                    "SCALAR",
                    None,
                    34963,
                );
                let mut primitives = vec![
                    json!({"attributes": {"POSITION": position, "NORMAL": normal},
                    "indices": triangles, "material": material, "mode": 4}),
                ];
                if !lines.is_empty() {
                    let edge = self.edge_material();
                    let line =
                        self.accessor(line_bytes, lines.len(), index_type, "SCALAR", None, 34963);
                    primitives.push(json!({"attributes": {"POSITION": position},
                        "indices": line, "material": edge, "mode": 1}));
                }
                let index = self.meshes.len();
                self.meshes.push(json!({ "primitives": primitives }));
                self.geometry.insert(key, index);
                index
            }
        };
        Ok((mesh_index, center))
    }
    /// The shared dark material of feature-edge lines.
    fn edge_material(&mut self) -> usize {
        if let Some(index) = self.edge_material {
            return index;
        }
        let index = self.materials.len();
        self.materials.push(json!({"name": "edges", "pbrMetallicRoughness": {
            "baseColorFactor": [0.08, 0.09, 0.1, 1.0], "metallicFactor": 0.0, "roughnessFactor": 1.0 }}));
        self.edge_material = Some(index);
        index
    }
    /// A node drawing mesh `mesh_index` (centered on `center` mm) moved by the
    /// model-space rigid `transform` (mm), if any. Unrotated nodes keep a
    /// plain translation; rotated ones carry a glTF matrix.
    fn add_node(
        &mut self,
        mesh_index: usize,
        center: [f64; 3],
        transform: Option<Rigid>,
        name: &str,
        mesh: &TaggedSurfaceMesh,
    ) {
        let m = transform.unwrap_or([1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0]);
        let r = |row: usize, column: usize| m[4 * row + column];
        // Where the transform takes the mesh center, in model millimeters.
        let moved: [f64; 3] = std::array::from_fn(|row| {
            (0..3).map(|k| r(row, k) * center[k]).sum::<f64>() + r(row, 3)
        });
        let translation = gltf_axis(moved.map(|value| value * 0.001));
        let rotated = (0..3).any(|row| (0..3).any(|k| r(row, k) != f64::from(u8::from(row == k))));
        let mut node = json!({ "name": name, "mesh": mesh_index,
            "extras": {"sourceMesh": mesh.id, "faceTags": mesh.names} });
        if rotated {
            let mut matrix = family_local_matrix(m);
            matrix[12..15].copy_from_slice(&translation);
            node["matrix"] = json!(matrix);
        } else {
            node["translation"] = json!(translation);
        }
        self.nodes.push(node);
    }
    /// Appends `bytes` (4-byte aligned) as a buffer view and accessor of
    /// `count` elements of glTF `component` type and `kind`, for `target`.
    fn accessor(
        &mut self,
        bytes: Vec<u8>,
        count: usize,
        component: u32,
        kind: &str,
        bounds: Option<([f32; 3], [f32; 3])>,
        target: u32,
    ) -> usize {
        while !self.binary.len().is_multiple_of(4) {
            self.binary.push(0);
        }
        let view = self.views.len();
        self.views.push(json!({"buffer": 0, "byteOffset": self.binary.len(), "byteLength": bytes.len(), "target": target}));
        self.binary.extend(bytes);
        let mut accessor =
            json!({"bufferView": view, "componentType": component, "count": count, "type": kind});
        if let Some((minimum, maximum)) = bounds {
            accessor["min"] = json!(minimum);
            accessor["max"] = json!(maximum);
        }
        let index = self.accessors.len();
        self.accessors.push(accessor);
        index
    }
    fn finish(self, variants: usize) -> Result<String, ModelError> {
        let nodes: Vec<_> = (0..self.nodes.len()).collect();
        serde_json::to_string(&json!({"asset": {"version": "2.0", "generator": "opencascade-c-bridge"},
            "scene": 0, "scenes": [{"nodes": nodes}], "nodes": self.nodes, "meshes": self.meshes,
            "materials": self.materials, "bufferViews": self.views, "accessors": self.accessors,
            "buffers": [{"byteLength": self.binary.len(), "uri": format!("data:application/octet-stream;base64,{}", base64(&self.binary))}],
            "extras": {"units": "m", "sourceUnits": "mm", "sourceUpAxis": "Z", "generatedVariants": variants} }))
            .map_err(|error| ModelError::new(format!("encode glTF: {error}")))
    }
}

/// Binary glTF (GLB) of glTF JSON whose single buffer is an embedded base64
/// data URI, as [`InstanceGraphExport`] writes it: the JSON chunk with the
/// buffer's URI removed, then the decoded binary chunk, both padded to four
/// bytes. Browsers read it without decoding base64 or copying vertex data.
/// O(size).
pub fn gltf_to_glb(gltf: &str) -> Result<Vec<u8>, ModelError> {
    let mut value: Value = serde_json::from_str(gltf)
        .map_err(|error| ModelError::new(format!("parse glTF: {error}")))?;
    let binary = match value["buffers"].as_array_mut().map(|b| b.as_mut_slice()) {
        Some([buffer]) => {
            let uri = buffer["uri"].as_str().unwrap_or_default();
            let encoded = uri
                .strip_prefix("data:application/octet-stream;base64,")
                .ok_or_else(|| {
                    ModelError::new("glTF buffer must be an embedded base64 data URI")
                })?;
            let bytes = unbase64(encoded)?;
            buffer.as_object_mut().expect("buffer object").remove("uri");
            bytes
        }
        Some([]) | None => Vec::new(),
        Some(_) => return Err(ModelError::new("GLB holds one glTF buffer")),
    };
    let mut json =
        serde_json::to_vec(&value).map_err(|error| ModelError::new(error.to_string()))?;
    while !json.len().is_multiple_of(4) {
        json.push(b' ');
    }
    let mut bin = binary;
    while !bin.len().is_multiple_of(4) {
        bin.push(0);
    }
    let total = 12 + 8 + json.len() + if bin.is_empty() { 0 } else { 8 + bin.len() };
    let total = u32::try_from(total).map_err(|_| ModelError::new("GLB exceeds 4 GiB"))?;
    let mut out = Vec::with_capacity(total as usize);
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2_u32.to_le_bytes());
    out.extend_from_slice(&total.to_le_bytes());
    out.extend_from_slice(&(json.len() as u32).to_le_bytes());
    out.extend_from_slice(b"JSON");
    out.extend_from_slice(&json);
    if !bin.is_empty() {
        out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
        out.extend_from_slice(b"BIN\0");
        out.extend_from_slice(&bin);
    }
    Ok(out)
}

fn unbase64(text: &str) -> Result<Vec<u8>, ModelError> {
    let value = |c: u8| -> Result<u32, ModelError> {
        Ok(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return Err(ModelError::new("glTF buffer is not valid base64")),
        }
        .into())
    };
    let bytes = text.as_bytes();
    if !bytes.len().is_multiple_of(4) {
        return Err(ModelError::new("glTF buffer is not valid base64"));
    }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for chunk in bytes.chunks(4) {
        let padding = chunk.iter().rev().take_while(|&&c| c == b'=').count();
        let mut word = 0_u32;
        for &c in &chunk[..4 - padding] {
            word = (word << 6) | value(c)?;
        }
        word <<= 6 * padding as u32;
        let decoded = [(word >> 16) as u8, (word >> 8) as u8, word as u8];
        out.extend_from_slice(&decoded[..3 - padding]);
    }
    Ok(out)
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let value = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for shift in [18, 12] {
            result.push(ALPHABET[((value >> shift) & 63) as usize] as char);
        }
        result.push(if chunk.len() > 1 {
            ALPHABET[((value >> 6) & 63) as usize] as char
        } else {
            '='
        });
        result.push(if chunk.len() > 2 {
            ALPHABET[(value & 63) as usize] as char
        } else {
            '='
        });
    }
    result
}
pub trait InstanceGraphExport<'definition> {
    /// glTF of the named output of every unsuppressed instance whose family
    /// has it, nodes named by instance id: the mesh counterpart of
    /// `OutputSet::AllWithOutput`. O(instances log instances) to select, plus
    /// `export_gltf`, which shares meshes between identical variants.
    fn export_gltf_output(
        &self,
        session: &Session,
        output: &str,
        settings: MeshSettings,
    ) -> Result<String, ModelError>;

    /// Regenerate selected instances together, preserve current poses, and emit
    /// glTF 2.0 with inherited material appearance. One final output per instance.
    /// Identical rebased geometry/material combinations share a glTF mesh.
    fn export_gltf(
        &self,
        session: &Session,
        definitions: &[MeshExportDefinition],
    ) -> Result<String, ModelError>;

    /// glTF of the named output of every generated instance that has it, from
    /// an existing regeneration (such as `regenerate_all`, whose verification
    /// results the caller keeps), so geometry is not generated twice. Nodes
    /// are named by instance id. O(instances log instances) to select, plus
    /// one tessellation per shared variant.
    fn export_gltf_generated(
        &self,
        session: &Session,
        generation: &GraphRegeneration<'_>,
        output: &str,
        settings: MeshSettings,
    ) -> Result<String, ModelError>;
}

impl<'definition> InstanceGraphExport<'definition> for InstanceGraph<'definition> {
    fn export_gltf_output(
        &self,
        session: &Session,
        output: &str,
        settings: MeshSettings,
    ) -> Result<String, ModelError> {
        let mut context = ExportContext::new(self);
        let definitions = self.output_definitions(output, settings, &mut context, |_| true)?;
        self.export_gltf(session, &definitions)
    }

    fn export_gltf(
        &self,
        session: &Session,
        definitions: &[MeshExportDefinition],
    ) -> Result<String, ModelError> {
        if definitions.is_empty() {
            return Err(ModelError::new("glTF needs at least one selected mesh"));
        }
        self.validate_assembly()?;
        let mut context = ExportContext::new(self);
        let mut ids = HashSet::new();
        for definition in definitions {
            definition.validate_cached(self, &mut context)?;
            if !ids.insert(definition.output.instance.as_str()) {
                return Err(ModelError::new(
                    "glTF needs one output per distinct instance",
                ));
            }
        }
        let generation =
            self.regenerate_instances_current(session, &ids.into_iter().collect::<Vec<_>>())?;
        self.scene(session, &generation, definitions, &mut context)
    }

    fn export_gltf_generated(
        &self,
        session: &Session,
        generation: &GraphRegeneration<'_>,
        output: &str,
        settings: MeshSettings,
    ) -> Result<String, ModelError> {
        let mut context = ExportContext::new(self);
        let definitions = self.output_definitions(output, settings, &mut context, |id| {
            generation.result(id).is_some()
        })?;
        self.scene(session, generation, &definitions, &mut context)
    }
}

/// A regeneration and glTF built across worker threads (see
/// [`InstanceGraphParallelExport`]): the scene, each generated instance with
/// the representative whose variant it uses, and each variant's requirement
/// results.
#[derive(Debug)]
pub struct ParallelGltf {
    pub gltf: String,
    /// Generated instances, sorted, each with its variant's representative.
    pub instances: Vec<(String, String)>,
    /// Requirement results of each representative's variant.
    pub variants: HashMap<String, Vec<VerificationResult>>,
    pub generated_variants: usize,
}

pub trait InstanceGraphParallelExport {
    /// Regenerates every unsuppressed instance and returns the glTF of
    /// `output`, as `regenerate_all` and `export_gltf_generated` do, with the
    /// distinct parameter variants spread over up to `threads` worker
    /// threads, each with its own session. A worker generates a variant once
    /// in local coordinates, keeps its requirement results and tessellation,
    /// and releases its shapes; every instance of the variant becomes a node
    /// placed by its own world transform, so no shape is copied or placed in
    /// OCCT. Returns `None` when the assembly has requirements, which need
    /// every placed shape in one session; callers then regenerate serially.
    /// O(variants × generation / threads + instances).
    fn regenerate_gltf_parallel(
        &mut self,
        session: &Session,
        output: &str,
        settings: MeshSettings,
        threads: usize,
    ) -> Result<Option<ParallelGltf>, ModelError>;
}

impl<'definition> InstanceGraphParallelExport for InstanceGraph<'definition> {
    fn regenerate_gltf_parallel(
        &mut self,
        session: &Session,
        output: &str,
        settings: MeshSettings,
        threads: usize,
    ) -> Result<Option<ParallelGltf>, ModelError> {
        if !self.assembly.requirements.is_empty() {
            return Ok(None);
        }
        self.refresh_driven_patterns(session)?;
        let graph: &InstanceGraph<'definition> = self;
        let mut ids: Vec<&str> = graph
            .nodes
            .keys()
            .map(String::as_str)
            .filter(|id| !graph.is_suppressed(id))
            .collect();
        ids.sort_unstable();
        let groups = graph.group_by_parameters(&ids)?;
        let mut context = ExportContext::new(graph);
        let generated: HashSet<&str> = ids.iter().copied().collect();
        let definitions = graph
            .output_definitions(output, settings, &mut context, |id| generated.contains(id))?;
        let by_instance: HashMap<&str, &MeshExportDefinition> = definitions
            .iter()
            .map(|d| (d.output.instance.as_str(), d))
            .collect();
        type Outcome = Result<(Option<TaggedSurfaceMesh>, Vec<VerificationResult>), ModelError>;
        let next = std::sync::atomic::AtomicUsize::new(0);
        let failed = std::sync::atomic::AtomicBool::new(false);
        let workers = threads.clamp(1, groups.len().max(1));
        let mut outcomes: Vec<Option<Outcome>> = (0..groups.len()).map(|_| None).collect();
        std::thread::scope(|scope| -> Result<(), ModelError> {
            let handles: Vec<_> = (0..workers)
                .map(|_| {
                    scope.spawn(|| -> Result<Vec<(usize, Outcome)>, ModelError> {
                        let session = Session::new().map_err(|e| ModelError::new(e.to_string()))?;
                        let mut context = ExportContext::new(graph);
                        let mut done = Vec::new();
                        while !failed.load(std::sync::atomic::Ordering::Relaxed) {
                            let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            let Some(group) = groups.get(index) else {
                                break;
                            };
                            let (representative, resolved) = &group[0];
                            let outcome = resolved
                                .instance
                                .regenerate(&session)
                                .map_err(|error| instance_error(representative, error))
                                .and_then(|local| {
                                    let mesh = by_instance
                                        .get(representative)
                                        .map(|definition| {
                                            definition.tagged_result(
                                                graph,
                                                &session,
                                                &local,
                                                &mut context,
                                                false,
                                            )
                                        })
                                        .transpose();
                                    let verification = local.verification.clone();
                                    release_results(&session, [local]);
                                    mesh.map(|mesh| (mesh, verification))
                                });
                            if outcome.is_err() {
                                failed.store(true, std::sync::atomic::Ordering::Relaxed);
                            }
                            done.push((index, outcome));
                        }
                        Ok(done)
                    })
                })
                .collect();
            for handle in handles {
                let done = handle
                    .join()
                    .map_err(|_| ModelError::new("a regeneration worker panicked"))??;
                for (index, outcome) in done {
                    outcomes[index] = Some(outcome);
                }
            }
            Ok(())
        })?;
        // The first failure in group order, as serial regeneration reports.
        let mut variants = HashMap::new();
        let mut meshes = Vec::with_capacity(groups.len());
        let mut variant_of: HashMap<&str, (usize, &str)> = HashMap::new();
        let mut instances = Vec::with_capacity(ids.len());
        for (index, (group, outcome)) in groups.iter().zip(outcomes).enumerate() {
            let (mesh, verification) =
                outcome.ok_or_else(|| ModelError::new("a variant was not generated"))??;
            let representative = group[0].0;
            for (id, _) in group {
                variant_of.insert(id, (index, representative));
            }
            variants.insert(representative.to_owned(), verification);
            meshes.push(mesh);
        }
        for id in &ids {
            instances.push(((*id).to_owned(), variant_of[id].1.to_owned()));
        }
        let mut scene = Scene::default();
        let mut placed: HashMap<(usize, usize), (usize, [f64; 3])> = HashMap::new();
        for definition in &definitions {
            let instance = definition.output.instance.as_str();
            let (index, _) = variant_of[instance];
            let mesh = meshes[index]
                .as_ref()
                .ok_or_else(|| ModelError::new("glTF instance was not generated"))?;
            let material =
                change_impact::inherited_material(instance, graph, &mut context.material_ids);
            let appearance = material
                .as_ref()
                .and_then(|material| graph.assembly.material_appearances.get(material))
                .copied()
                .unwrap_or_default();
            let material = scene.material(appearance)?;
            let (mesh_index, center) = match placed.get(&(index, material)) {
                Some(found) => *found,
                None => {
                    let added = scene.add_mesh(mesh, appearance)?;
                    placed.insert((index, material), added);
                    added
                }
            };
            let world = world_transform(graph, instance, &mut context)?;
            scene.add_node(mesh_index, center, Some(world), instance, mesh);
            scene.nodes.last_mut().expect("added node")["extras"]["familyLocalMatrix"] =
                json!(family_local_matrix(world));
        }
        Ok(Some(ParallelGltf {
            gltf: scene.finish(groups.len())?,
            instances,
            variants,
            generated_variants: groups.len(),
        }))
    }
}

pub(crate) trait InstanceGraphExportInternal<'definition> {
    /// Writes the glTF scene for already generated `definitions`.
    fn scene(
        &self,
        session: &Session,
        generation: &GraphRegeneration<'_>,
        definitions: &[MeshExportDefinition],
        context: &mut ExportContext<'definition>,
    ) -> Result<String, ModelError>;
}

impl<'definition> InstanceGraphExportInternal<'definition> for InstanceGraph<'definition> {
    fn scene(
        &self,
        session: &Session,
        generation: &GraphRegeneration<'_>,
        definitions: &[MeshExportDefinition],
        context: &mut ExportContext<'definition>,
    ) -> Result<String, ModelError> {
        let mut scene = Scene::default();
        // Instances sharing a generated variant have the same local geometry,
        // so one tessellation serves all of them, moved by the rigid transform
        // between their world placements. Keyed by representative, output,
        // settings and material; face-tagged meshes keep their own names.
        let mut tessellated: HashMap<(String, String, Vec<u8>, usize), Tessellated> =
            HashMap::new();
        for definition in definitions {
            let instance = &definition.output.instance;
            let result = generation
                .result(instance)
                .ok_or_else(|| ModelError::new("glTF instance was not generated"))?;
            let material =
                change_impact::inherited_material(instance, self, &mut context.material_ids);
            let appearance = material
                .as_ref()
                .and_then(|material| self.assembly.material_appearances.get(material))
                .copied()
                .unwrap_or_default();
            let world = world_transform(self, instance, context)?;
            let key = definition.face_tags.is_empty().then(|| {
                (
                    generation
                        .shared_from(instance)
                        .unwrap_or(instance)
                        .to_owned(),
                    definition.output.output.clone(),
                    serde_json::to_vec(&definition.settings).unwrap_or_default(),
                    scene.material(appearance).unwrap_or(usize::MAX),
                )
            });
            if let Some(shared) = key.as_ref().and_then(|key| tessellated.get(key)) {
                let transform = compose(&world, &invert(&shared.world));
                scene.add_node(
                    shared.mesh,
                    shared.center,
                    Some(transform),
                    instance,
                    &shared.source,
                );
                scene.nodes.last_mut().expect("added node")["extras"]["familyLocalMatrix"] =
                    json!(family_local_matrix(world));
                continue;
            }
            let mesh = definition.tagged_result(self, session, result, context, false)?;
            let (mesh_index, center) = scene.add_mesh(&mesh, appearance)?;
            scene.add_node(mesh_index, center, None, instance, &mesh);
            scene.nodes.last_mut().expect("added node")["extras"]["familyLocalMatrix"] =
                json!(family_local_matrix(world));
            if let Some(key) = key {
                tessellated.insert(
                    key,
                    Tessellated {
                        mesh: mesh_index,
                        center,
                        world,
                        source: TaggedSurfaceMesh {
                            triangles: Vec::new(),
                            face_tags: Vec::new(),
                            faces: Vec::new(),
                            ..mesh
                        },
                    },
                );
            }
        }
        scene.finish(generation.generated_variants())
    }
}

/// A position-welded triangle edge: its first triangle's face and vertex
/// pair, how many triangles use it, and whether they lie on different faces.
struct EdgeUse {
    face: usize,
    vertices: [u32; 2],
    uses: u32,
    mixed: bool,
}

#[derive(Default)]
struct Scene {
    binary: Vec<u8>,
    views: Vec<Value>,
    accessors: Vec<Value>,
    meshes: Vec<Value>,
    materials: Vec<Value>,
    nodes: Vec<Value>,
    geometry: HashMap<Vec<u8>, usize>,
    appearances: HashMap<Vec<u8>, usize>,
    edge_material: Option<usize>,
}
pub trait TaggedSurfaceMeshExport {
    /// Gmsh 2.2 ASCII surface mesh in mm. Near-coincident seam vertices weld
    /// within max(1e-9 mm, span*1e-12); collapsed triangles are rejected.
    fn to_msh(&self) -> Result<String, ModelError>;

    fn to_gltf(&self, appearance: MaterialAppearance) -> Result<String, ModelError>;
}

impl TaggedSurfaceMeshExport for TaggedSurfaceMesh {
    fn to_msh(&self) -> Result<String, ModelError> {
        let (nodes, triangles) = welded(self)?;
        let mut output = String::from("$MeshFormat\n2.2 0 8\n$EndMeshFormat\n");
        if !self.names.is_empty() {
            writeln!(output, "$PhysicalNames\n{}", self.names.len()).expect("string write");
            for (index, name) in self.names.iter().enumerate() {
                if !valid_name(name) {
                    return Err(ModelError::new("invalid mesh physical name"));
                }
                writeln!(output, "2 {} \"{}\"", index + 1, name).expect("string write");
            }
            output.push_str("$EndPhysicalNames\n");
        }
        writeln!(output, "$Nodes\n{}", nodes.len()).expect("string write");
        for (index, node) in nodes.iter().enumerate() {
            writeln!(output, "{} {} {} {}", index + 1, node[0], node[1], node[2])
                .expect("string write");
        }
        writeln!(output, "$EndNodes\n$Elements\n{}", triangles.len()).expect("string write");
        for (index, (nodes, face)) in triangles.iter().enumerate() {
            let tag = self
                .face_tags
                .get(*face)
                .copied()
                .ok_or_else(|| ModelError::new("mesh triangle face index is invalid"))?;
            if tag as usize > self.names.len() {
                return Err(ModelError::new("mesh physical tag is invalid"));
            }
            writeln!(
                output,
                "{} 2 2 {} {} {} {} {}",
                index + 1,
                tag,
                face + 1,
                nodes[0],
                nodes[1],
                nodes[2]
            )
            .expect("string write");
        }
        output.push_str("$EndElements\n");
        Ok(output)
    }

    fn to_gltf(&self, appearance: MaterialAppearance) -> Result<String, ModelError> {
        let mut scene = Scene::default();
        scene.add(self, appearance, &self.id)?;
        scene.finish(1)
    }
}
