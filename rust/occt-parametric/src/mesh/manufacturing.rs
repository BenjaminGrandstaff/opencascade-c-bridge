//! Sampled screening on oriented triangles, not a certification of the BREP.
use super::*;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ManufacturingSettings {
    pub pull_direction: VectorQuantity,
    pub build_direction: VectorQuantity,
    pub minimum_draft_radians: f64,
    /// Maximum downward surface inclination from vertical, in [0, pi/2].
    pub maximum_overhang_radians: f64,
    pub minimum_wall: Quantity,
    pub maximum_wall_samples: usize,
}
impl Default for ManufacturingSettings {
    fn default() -> Self {
        Self {
            pull_direction: VectorQuantity::scalars(0.0, 0.0, 1.0),
            build_direction: VectorQuantity::scalars(0.0, 0.0, 1.0),
            minimum_draft_radians: std::f64::consts::PI / 180.0,
            maximum_overhang_radians: std::f64::consts::FRAC_PI_4,
            minimum_wall: Quantity::length(0.8, LengthUnit::Millimeter),
            maximum_wall_samples: 1000,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FaceDraftCheck {
    pub face: usize,
    pub minimum_signed_radians: f64,
    pub meets_minimum: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WallSample {
    pub triangle: usize,
    pub thickness_mm: Option<f64>,
    pub below_minimum: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ManufacturingReport {
    pub draft: Vec<FaceDraftCheck>,
    pub overhang_triangles: Vec<usize>,
    pub wall_samples: Vec<WallSample>,
    pub minimum_sampled_wall_mm: Option<f64>,
    pub unresolved_wall_samples: usize,
}

impl TaggedSurfaceMesh {
    /// Screen tessellated geometry. Signed draft excludes pull-normal caps.
    /// Overhang excludes facets touching the lowest build plane. Wall thickness
    /// is an inward normal ray at selected triangle centroids; it requires a
    /// closed, consistently oriented welded mesh. These sampled checks can
    /// miss thin regions, bridging behavior, and sub-triangle detail.
    pub fn check_manufacturability(
        &self,
        settings: ManufacturingSettings,
    ) -> Result<ManufacturingReport, ModelError> {
        self.screen(settings, true)
    }

    /// Draft and overhang always; the wall ray pass and its closed-mesh
    /// requirement only when `walls` is set.
    pub(crate) fn screen(
        &self,
        settings: ManufacturingSettings,
        walls: bool,
    ) -> Result<ManufacturingReport, ModelError> {
        let (pull, build, wall) = validate(settings)?;
        let normals = self
            .triangles
            .iter()
            .map(triangle_normal)
            .collect::<Result<Vec<_>, _>>()?;
        if walls {
            closed_mesh(self)?;
        }
        let span = mesh_span(self)?;
        let tolerance = (span * 1e-10).max(1e-9);
        let (draft, overhang_triangles) =
            surface_screen(self, &normals, pull, build, settings, tolerance)?;
        let wall = if walls {
            wall_screen(self, &normals, settings, span, tolerance, wall)
        } else {
            WallScreen {
                samples: Vec::new(),
                minimum: None,
                unresolved: 0,
            }
        };
        Ok(ManufacturingReport {
            draft,
            overhang_triangles,
            wall_samples: wall.samples,
            minimum_sampled_wall_mm: wall.minimum,
            unresolved_wall_samples: wall.unresolved,
        })
    }
}

fn surface_screen(
    mesh: &TaggedSurfaceMesh,
    normals: &[[f64; 3]],
    pull: [f64; 3],
    build: [f64; 3],
    settings: ManufacturingSettings,
    tolerance: f64,
) -> Result<(Vec<FaceDraftCheck>, Vec<usize>), ModelError> {
    let reference = xyz(mesh.triangles[0].points[0]);
    let floor = mesh
        .triangles
        .iter()
        .flat_map(|triangle| triangle.points)
        .map(|point| dot(subtract(xyz(point), reference), build))
        .try_fold(f64::INFINITY, |minimum, value| {
            if value.is_finite() {
                Ok(minimum.min(value))
            } else {
                Err(ModelError::new("build projection overflows"))
            }
        })?;
    let mut draft: BTreeMap<usize, f64> = BTreeMap::new();
    let mut overhang_triangles = Vec::new();
    for (index, (triangle, normal)) in mesh.triangles.iter().zip(normals).enumerate() {
        let projection = dot(*normal, pull).clamp(-1.0, 1.0);
        if projection.abs() < 1.0 - 1e-8 {
            let angle = projection.asin();
            draft
                .entry(triangle.face_index)
                .and_modify(|minimum| *minimum = minimum.min(angle))
                .or_insert(angle);
        }
        if dot(*normal, build) < -settings.maximum_overhang_radians.sin()
            && triangle
                .points
                .iter()
                .any(|point| dot(subtract(xyz(*point), reference), build) > floor + tolerance)
        {
            overhang_triangles.push(index);
        }
    }
    Ok((
        draft
            .into_iter()
            .map(|(face, angle)| FaceDraftCheck {
                face,
                minimum_signed_radians: angle,
                meets_minimum: angle >= settings.minimum_draft_radians,
            })
            .collect(),
        overhang_triangles,
    ))
}

struct WallScreen {
    samples: Vec<WallSample>,
    minimum: Option<f64>,
    unresolved: usize,
}
fn wall_screen(
    mesh: &TaggedSurfaceMesh,
    normals: &[[f64; 3]],
    settings: ManufacturingSettings,
    span: f64,
    tolerance: f64,
    wall: f64,
) -> WallScreen {
    let tree = Node::build(mesh, (0..mesh.triangles.len()).collect());
    let count = mesh.triangles.len().min(settings.maximum_wall_samples);
    let mut wall_samples = Vec::with_capacity(count);
    let mut minimum: Option<f64> = None;
    let mut unresolved = 0;
    for sample in 0..count {
        let index = ((sample as u128 * mesh.triangles.len() as u128) / count as u128) as usize;
        let triangle = &mesh.triangles[index];
        let origin = std::array::from_fn(|axis| {
            triangle
                .points
                .iter()
                .map(|point| xyz(*point)[axis] / 3.0)
                .sum()
        });
        let direction = normals[index].map(|value| -value);
        let mut nearest = f64::INFINITY;
        tree.nearest(
            mesh,
            normals,
            index,
            origin,
            direction,
            span,
            tolerance,
            &mut nearest,
        );
        let thickness = nearest.is_finite().then_some(nearest);
        if let Some(distance) = thickness {
            minimum = Some(minimum.map_or(distance, |value| value.min(distance)));
        } else {
            unresolved += 1;
        }
        wall_samples.push(WallSample {
            triangle: index,
            thickness_mm: thickness,
            below_minimum: thickness.is_some_and(|value| value < wall),
        });
    }
    WallScreen {
        samples: wall_samples,
        minimum,
        unresolved,
    }
}

pub(super) fn validate(
    settings: ManufacturingSettings,
) -> Result<([f64; 3], [f64; 3], f64), ModelError> {
    if !settings.minimum_draft_radians.is_finite()
        || !(0.0..std::f64::consts::FRAC_PI_2).contains(&settings.minimum_draft_radians)
        || !settings.maximum_overhang_radians.is_finite()
        || !(0.0..=std::f64::consts::FRAC_PI_2).contains(&settings.maximum_overhang_radians)
        || settings.minimum_wall.dimension != Dimension::Length
        || !(1..=20_000).contains(&settings.maximum_wall_samples)
    {
        return Err(ModelError::new(
            "invalid manufacturing angles, wall units, or sample budget",
        ));
    }
    let wall = settings.minimum_wall.normalized()?;
    if !wall.is_finite() || wall <= 0.0 {
        return Err(ModelError::new("minimum wall must be finite and positive"));
    }
    Ok((
        unit(xyz(settings
            .pull_direction
            .normalized(Dimension::Scalar)?))?,
        unit(xyz(settings
            .build_direction
            .normalized(Dimension::Scalar)?))?,
        wall,
    ))
}

fn unit(vector: [f64; 3]) -> Result<[f64; 3], ModelError> {
    if vector.iter().any(|value| !value.is_finite()) {
        return Err(ModelError::new("direction must be finite and nonzero"));
    }
    let maximum = vector.into_iter().map(f64::abs).fold(0.0, f64::max);
    if !maximum.is_finite() || maximum == 0.0 {
        return Err(ModelError::new("direction must be finite and nonzero"));
    }
    let scaled = vector.map(|value| value / maximum);
    let length = dot(scaled, scaled).sqrt();
    Ok(scaled.map(|value| value / length))
}
fn dot(left: [f64; 3], right: [f64; 3]) -> f64 {
    (0..3).map(|axis| left[axis] * right[axis]).sum()
}
fn cross(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [
        left[1] * right[2] - left[2] * right[1],
        left[2] * right[0] - left[0] * right[2],
        left[0] * right[1] - left[1] * right[0],
    ]
}
fn subtract(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|axis| left[axis] - right[axis])
}

pub(super) fn triangle_normal(triangle: &MeshTriangle) -> Result<[f64; 3], ModelError> {
    let [a, b, c] = triangle.points.map(xyz);
    let ab = unit(subtract(b, a))?;
    let ac = unit(subtract(c, a))?;
    unit(cross(ab, ac))
}

fn mesh_span(mesh: &TaggedSurfaceMesh) -> Result<f64, ModelError> {
    let (minimum, maximum) = export::bounds(mesh)?;
    Ok((0..3)
        .map(|axis| maximum[axis] - minimum[axis])
        .fold(0.0, f64::max))
}

fn closed_mesh(mesh: &TaggedSurfaceMesh) -> Result<(), ModelError> {
    let (_, triangles) = export::welded(mesh)?;
    let mut edges: HashMap<(usize, usize), (usize, i32)> = HashMap::new();
    for (nodes, _) in triangles {
        for (a, b) in [
            (nodes[0], nodes[1]),
            (nodes[1], nodes[2]),
            (nodes[2], nodes[0]),
        ] {
            let key = (a.min(b), a.max(b));
            let entry = edges.entry(key).or_default();
            entry.0 += 1;
            entry.1 += if a < b { 1 } else { -1 };
        }
    }
    if edges
        .values()
        .any(|(count, direction)| *count != 2 || *direction != 0)
    {
        return Err(ModelError::new(
            "wall screening requires a closed consistently oriented welded mesh",
        ));
    }
    Ok(())
}

struct Node {
    minimum: [f64; 3],
    maximum: [f64; 3],
    content: Content,
}
enum Content {
    Leaf(Vec<usize>),
    Branch(Box<Node>, Box<Node>),
}
impl Node {
    fn build(mesh: &TaggedSurfaceMesh, mut indices: Vec<usize>) -> Self {
        let mut minimum = [f64::INFINITY; 3];
        let mut maximum = [f64::NEG_INFINITY; 3];
        for point in indices
            .iter()
            .flat_map(|index| mesh.triangles[*index].points)
            .map(xyz)
        {
            for axis in 0..3 {
                minimum[axis] = minimum[axis].min(point[axis]);
                maximum[axis] = maximum[axis].max(point[axis]);
            }
        }
        let content = if indices.len() <= 8 {
            Content::Leaf(indices)
        } else {
            let axis = (0..3)
                .max_by(|left, right| {
                    (maximum[*left] - minimum[*left])
                        .total_cmp(&(maximum[*right] - minimum[*right]))
                })
                .expect("three axes");
            let middle = indices.len() / 2;
            indices.select_nth_unstable_by(middle, |left, right| {
                centroid(&mesh.triangles[*left], axis)
                    .total_cmp(&centroid(&mesh.triangles[*right], axis))
            });
            let right = indices.split_off(middle);
            Content::Branch(
                Box::new(Self::build(mesh, indices)),
                Box::new(Self::build(mesh, right)),
            )
        };
        Self {
            minimum,
            maximum,
            content,
        }
    }
    fn intersects(
        &self,
        origin: [f64; 3],
        direction: [f64; 3],
        nearest: f64,
        tolerance: f64,
    ) -> bool {
        let mut low = 0.0_f64;
        let mut high = nearest;
        for axis in 0..3 {
            if direction[axis] == 0.0 {
                if origin[axis] < self.minimum[axis] - tolerance
                    || origin[axis] > self.maximum[axis] + tolerance
                {
                    return false;
                }
            } else {
                let a = (self.minimum[axis] - origin[axis]) / direction[axis];
                let b = (self.maximum[axis] - origin[axis]) / direction[axis];
                low = low.max(a.min(b) - tolerance);
                high = high.min(a.max(b) + tolerance);
                if low > high {
                    return false;
                }
            }
        }
        true
    }
    #[allow(clippy::too_many_arguments)]
    fn nearest(
        &self,
        mesh: &TaggedSurfaceMesh,
        normals: &[[f64; 3]],
        source: usize,
        origin: [f64; 3],
        direction: [f64; 3],
        span: f64,
        tolerance: f64,
        nearest: &mut f64,
    ) {
        if !self.intersects(origin, direction, *nearest, tolerance) {
            return;
        }
        match &self.content {
            Content::Leaf(indices) => {
                for index in indices {
                    if *index == source || dot(normals[*index], direction) <= 1e-12 {
                        continue;
                    }
                    if let Some(distance) =
                        ray_triangle(&mesh.triangles[*index], origin, direction, span, tolerance)
                    {
                        *nearest = nearest.min(distance);
                    }
                }
            }
            Content::Branch(left, right) => {
                left.nearest(
                    mesh, normals, source, origin, direction, span, tolerance, nearest,
                );
                right.nearest(
                    mesh, normals, source, origin, direction, span, tolerance, nearest,
                );
            }
        }
    }
}
fn centroid(triangle: &MeshTriangle, axis: usize) -> f64 {
    triangle
        .points
        .iter()
        .map(|point| xyz(*point)[axis] / 3.0)
        .sum()
}

fn ray_triangle(
    triangle: &MeshTriangle,
    origin: [f64; 3],
    direction: [f64; 3],
    span: f64,
    tolerance: f64,
) -> Option<f64> {
    let [a, b, c] = triangle.points.map(xyz);
    let edge1 = subtract(b, a).map(|value| value / span);
    let edge2 = subtract(c, a).map(|value| value / span);
    let h = cross(direction, edge2);
    let determinant = dot(edge1, h);
    if determinant.abs() < 1e-15 {
        return None;
    }
    let offset = subtract(origin, a).map(|value| value / span);
    let u = dot(offset, h) / determinant;
    if !(-1e-9..=1.0 + 1e-9).contains(&u) {
        return None;
    }
    let q = cross(offset, edge1);
    let v = dot(direction, q) / determinant;
    if v < -1e-9 || u + v > 1.0 + 1e-9 {
        return None;
    }
    let distance = dot(edge2, q) / determinant * span;
    (distance.is_finite() && distance > tolerance).then_some(distance)
}
