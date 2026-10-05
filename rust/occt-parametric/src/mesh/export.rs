use super::*;
use crate::assembly::{Rigid, compose, invert, rigid};
use serde_json::{Value, json};
use std::fmt::Write;

impl TaggedSurfaceMesh {
    /// Gmsh 2.2 ASCII surface mesh in mm. Near-coincident seam vertices weld
    /// within max(1e-9 mm, span*1e-12); collapsed triangles are rejected.
    pub fn to_msh(&self) -> Result<String, ModelError> {
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

    pub fn to_gltf(&self, appearance: MaterialAppearance) -> Result<String, ModelError> {
        let mut scene = Scene::default();
        scene.add(self, appearance, &self.id)?;
        scene.finish(1)
    }
}

pub(super) type IndexedTriangle = ([usize; 3], usize);
pub(super) fn welded(
    mesh: &TaggedSurfaceMesh,
) -> Result<(Vec<[f64; 3]>, Vec<IndexedTriangle>), ModelError> {
    let (minimum, maximum) = bounds(mesh)?;
    let tolerance = (0..3)
        .map(|axis| maximum[axis] - minimum[axis])
        .fold(0.0, f64::max)
        .mul_add(1e-12, 0.0)
        .max(1e-9);
    let mut buckets: HashMap<[i64; 3], Vec<usize>> = HashMap::new();
    let mut nodes: Vec<[f64; 3]> = Vec::new();
    let mut triangles = Vec::with_capacity(mesh.triangles.len());
    for triangle in &mesh.triangles {
        let mut indices = [0; 3];
        for (vertex, point) in triangle.points.iter().enumerate() {
            let point = xyz(*point);
            let cell = std::array::from_fn(|axis| {
                ((point[axis] - minimum[axis]) / tolerance).floor() as i64
            });
            let existing = neighboring_node(cell, point, tolerance, &buckets, &nodes);
            let index = existing.unwrap_or_else(|| {
                let index = nodes.len();
                nodes.push(point);
                buckets.entry(cell).or_default().push(index);
                index
            });
            indices[vertex] = index + 1;
        }
        if indices[0] == indices[1] || indices[1] == indices[2] || indices[0] == indices[2] {
            return Err(ModelError::new(
                "mesh welding collapses a triangle; use a larger model scale",
            ));
        }
        manufacturing::triangle_normal(&MeshTriangle {
            face_index: triangle.face_index,
            points: indices.map(|index| {
                let point = nodes[index - 1];
                Vec3::new(point[0], point[1], point[2])
            }),
        })
        .map_err(|error| error.context("mesh welding collapsed a triangle"))?;
        triangles.push((indices, triangle.face_index));
    }
    Ok((nodes, triangles))
}

fn neighboring_node(
    cell: [i64; 3],
    point: [f64; 3],
    tolerance: f64,
    buckets: &HashMap<[i64; 3], Vec<usize>>,
    nodes: &[[f64; 3]],
) -> Option<usize> {
    for x in -1..=1 {
        for y in -1..=1 {
            for z in -1..=1 {
                let key = [cell[0] + x, cell[1] + y, cell[2] + z];
                if let Some(index) = buckets.get(&key).into_iter().flatten().find(|index| {
                    (0..3).all(|axis| (nodes[**index][axis] - point[axis]).abs() <= tolerance)
                }) {
                    return Some(*index);
                }
            }
        }
    }
    None
}

pub(super) fn bounds(mesh: &TaggedSurfaceMesh) -> Result<([f64; 3], [f64; 3]), ModelError> {
    if mesh.triangles.is_empty() {
        return Err(ModelError::new("cannot export an empty mesh"));
    }
    let mut minimum = [f64::INFINITY; 3];
    let mut maximum = [f64::NEG_INFINITY; 3];
    for point in mesh
        .triangles
        .iter()
        .flat_map(|triangle| triangle.points)
        .map(xyz)
    {
        for axis in 0..3 {
            if !point[axis].is_finite() {
                return Err(ModelError::new("mesh contains nonfinite coordinates"));
            }
            minimum[axis] = minimum[axis].min(point[axis]);
            maximum[axis] = maximum[axis].max(point[axis]);
        }
    }
    if (0..3).any(|axis| !(maximum[axis] - minimum[axis]).is_finite()) {
        return Err(ModelError::new("mesh coordinate span overflows"));
    }
    Ok((minimum, maximum))
}

fn gltf_axis(point: [f64; 3]) -> [f64; 3] {
    [point[0], point[2], -point[1]]
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
    /// index and the center in millimeters.
    fn add_mesh(
        &mut self,
        mesh: &TaggedSurfaceMesh,
        appearance: MaterialAppearance,
    ) -> Result<(usize, [f64; 3]), ModelError> {
        let (minimum, maximum) = bounds(mesh)?;
        let center: [f64; 3] =
            std::array::from_fn(|axis| minimum[axis] * 0.5 + maximum[axis] * 0.5);
        let material = self.material(appearance)?;
        let count = mesh
            .triangles
            .len()
            .checked_mul(3)
            .ok_or_else(|| ModelError::new("mesh vertex count overflows"))?;
        let mut positions = Vec::with_capacity(count * 12);
        let mut normals = Vec::with_capacity(count * 12);
        let mut low = [f32::INFINITY; 3];
        let mut high = [f32::NEG_INFINITY; 3];
        for triangle in &mesh.triangles {
            let normal = gltf_axis(manufacturing::triangle_normal(triangle)?);
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
                    positions.extend_from_slice(&value.to_le_bytes());
                    normals.extend_from_slice(&(normal[axis] as f32).to_le_bytes());
                }
            }
            manufacturing::triangle_normal(&MeshTriangle {
                face_index: triangle.face_index,
                points: quantized.map(|point| {
                    Vec3::new(
                        f64::from(point[0]),
                        f64::from(point[1]),
                        f64::from(point[2]),
                    )
                }),
            })
            .map_err(|error| error.context("glTF float32 conversion collapses a triangle"))?;
        }
        let mut key = material.to_le_bytes().to_vec();
        key.extend_from_slice(&positions);
        key.extend_from_slice(&normals);
        let mesh_index = match self.geometry.get(&key) {
            Some(index) => *index,
            None => {
                let position = self.accessor(positions, count, Some((low, high)));
                let normal = self.accessor(normals, count, None);
                let index = self.meshes.len();
                self.meshes.push(json!({"primitives": [{"attributes": {"POSITION": position, "NORMAL": normal}, "material": material, "mode": 4}]}));
                self.geometry.insert(key, index);
                index
            }
        };
        Ok((mesh_index, center))
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
            // glTF axes are A·model with A = [[1,0,0],[0,0,1],[0,-1,0]], so the
            // rotation is A·R·Aᵀ; the matrix is column-major.
            let a = |row: usize, k: usize| match (row, k) {
                (0, 0) | (1, 2) => 1.0,
                (2, 1) => -1.0,
                _ => 0.0,
            };
            let turned = |row: usize, column: usize| {
                (0..3)
                    .flat_map(|i| (0..3).map(move |j| (i, j)))
                    .map(|(i, j)| a(row, i) * r(i, j) * a(column, j))
                    .sum::<f64>()
            };
            let mut matrix = [0.0; 16];
            for column in 0..3 {
                for row in 0..3 {
                    matrix[4 * column + row] = turned(row, column);
                }
            }
            matrix[12..15].copy_from_slice(&translation);
            matrix[15] = 1.0;
            node["matrix"] = json!(matrix);
        } else {
            node["translation"] = json!(translation);
        }
        self.nodes.push(node);
    }
    fn accessor(
        &mut self,
        bytes: Vec<u8>,
        count: usize,
        bounds: Option<([f32; 3], [f32; 3])>,
    ) -> usize {
        let view = self.views.len();
        self.views.push(json!({"buffer": 0, "byteOffset": self.binary.len(), "byteLength": bytes.len(), "target": 34962}));
        self.binary.extend(bytes);
        let mut accessor =
            json!({"bufferView": view, "componentType": 5126, "count": count, "type": "VEC3"});
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
        serde_json::to_string_pretty(&json!({"asset": {"version": "2.0", "generator": "opencascade-c-bridge"},
            "scene": 0, "scenes": [{"nodes": nodes}], "nodes": self.nodes, "meshes": self.meshes,
            "materials": self.materials, "bufferViews": self.views, "accessors": self.accessors,
            "buffers": [{"byteLength": self.binary.len(), "uri": format!("data:application/octet-stream;base64,{}", base64(&self.binary))}],
            "extras": {"units": "m", "sourceUnits": "mm", "sourceUpAxis": "Z", "generatedVariants": variants} }))
            .map_err(|error| ModelError::new(format!("encode glTF: {error}")))
    }
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

impl InstanceGraph<'_> {
    /// glTF of the named output of every unsuppressed instance whose family
    /// has it, nodes named by instance id: the mesh counterpart of
    /// `OutputSet::AllWithOutput`. O(instances log instances) to select, plus
    /// `export_gltf`, which shares meshes between identical variants.
    pub fn export_gltf_output(
        &self,
        session: &Session,
        output: &str,
        settings: MeshSettings,
    ) -> Result<String, ModelError> {
        settings.options()?;
        let mut context = ExportContext::new(self);
        let mut ids: Vec<&String> = self.nodes.keys().collect();
        ids.sort_unstable();
        let definitions: Vec<_> = ids
            .into_iter()
            .enumerate()
            .map(|(index, instance)| MeshExportDefinition {
                // Instance ids need not be safe mesh names; nodes keep the id.
                id: format!("part_{index}"),
                output: InstanceOutputRef {
                    instance: instance.clone(),
                    output: output.to_owned(),
                },
                settings,
                face_tags: Vec::new(),
                manufacturing: None,
            })
            .filter(|definition| definition.validate_cached(self, &mut context).is_ok())
            .collect();
        if definitions.is_empty() {
            return Err(ModelError::new(format!(
                "no unsuppressed instance has output '{output}'"
            )));
        }
        self.export_gltf(session, &definitions)
    }

    /// Regenerate selected instances together, preserve current poses, and emit
    /// glTF 2.0 with inherited material appearance. One final output per instance.
    /// Identical rebased geometry/material combinations share a glTF mesh.
    pub fn export_gltf(
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
            let world = world_transform(self, instance, &mut context)?;
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
                continue;
            }
            let mesh = definition.tagged_result(self, session, result, &mut context, false)?;
            let (mesh_index, center) = scene.add_mesh(&mesh, appearance)?;
            scene.add_node(mesh_index, center, None, instance, &mesh);
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
