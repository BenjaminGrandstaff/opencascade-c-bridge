use super::*;

pub fn welded(
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

pub fn bounds(mesh: &TaggedSurfaceMesh) -> Result<([f64; 3], [f64; 3]), ModelError> {
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

pub type IndexedTriangle = ([usize; 3], usize);

impl<'definition> InstanceGraph<'definition> {
    /// One untagged mesh definition per instance that `include`s and whose
    /// family has `output`, in instance-id order.
    pub fn output_definitions(
        &self,
        output: &str,
        settings: MeshSettings,
        context: &mut ExportContext<'definition>,
        include: impl Fn(&str) -> bool,
    ) -> Result<Vec<MeshExportDefinition>, ModelError> {
        settings.options()?;
        let mut ids: Vec<&String> = self.nodes.keys().filter(|id| include(id)).collect();
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
            .filter(|definition| definition.validate_cached(self, context).is_ok())
            .collect();
        if definitions.is_empty() {
            return Err(ModelError::new(format!(
                "no unsuppressed instance has output '{output}'"
            )));
        }
        Ok(definitions)
    }
}
