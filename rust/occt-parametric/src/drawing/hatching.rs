//! Paper-space scanline hatching of sampled planar cut faces, with hole parity
//! per face and material union across faces. No chordal-error certification.
use super::*;
use std::collections::BTreeMap;

const MAXIMUM_WORK: usize = 2_000_000;
const MAXIMUM_LINE_INDEX: f64 = (1u64 << 52) as f64;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SectionHatching {
    pub angle_radians: f64,
    /// Perpendicular line spacing and phase in paper millimeters.
    pub spacing_mm: f64,
    pub phase_mm: f64,
}
impl Default for SectionHatching {
    fn default() -> Self {
        Self {
            angle_radians: std::f64::consts::FRAC_PI_4,
            spacing_mm: 3.0,
            phase_mm: 0.0,
        }
    }
}
pub(super) fn validate(view: &DrawingView) -> Result<(), ModelError> {
    let Some(pattern) = view.hatching else {
        return Ok(());
    };
    if !pattern.angle_radians.is_finite()
        || !pattern.spacing_mm.is_finite()
        || pattern.spacing_mm <= 0.0
        || !pattern.phase_mm.is_finite()
    {
        return Err(ModelError::new(
            "section hatching needs a finite angle/phase and positive spacing",
        ));
    }
    match view.kind {
        DrawingViewKind::Orthographic => {
            Err(ModelError::new("hatching requires a Slice or Section view"))
        }
        DrawingViewKind::Section { normal, .. } => {
            if dot(axis(normal)?, axis(view.direction)?).abs() < 1.0 - 1e-10 {
                return Err(ModelError::new(
                    "hatched section must look normal to its cut plane",
                ));
            }
            Ok(())
        }
        DrawingViewKind::Slice => Ok(()),
    }
}
pub(super) fn section_plane_view(view: &DrawingView) -> Result<DrawingView, ModelError> {
    let DrawingViewKind::Section { origin, .. } = view.kind else {
        return Err(ModelError::new("hatching cut-plane view requires Section"));
    };
    let mut plane = view.clone();
    plane.kind = DrawingViewKind::Slice;
    plane.origin = origin;
    plane.hatching = None;
    Ok(plane)
}

struct ScanGrid {
    along: [f64; 2],
    normal: [f64; 2],
    spacing: f64,
    phase: f64,
    work: usize,
}
impl ScanGrid {
    fn new(pattern: SectionHatching) -> Self {
        let (sin, cos) = pattern.angle_radians.sin_cos();
        Self {
            along: [cos, sin],
            normal: [-sin, cos],
            spacing: pattern.spacing_mm,
            phase: pattern.phase_mm.rem_euclid(pattern.spacing_mm),
            work: 0,
        }
    }
    fn charge(&mut self, count: usize) -> Result<(), ModelError> {
        self.work = self
            .work
            .checked_add(count)
            .ok_or_else(|| ModelError::new("section hatch work overflow"))?;
        if self.work > MAXIMUM_WORK {
            return Err(ModelError::new(
                "section hatching exceeds the 2000000 sampling/intersection work budget",
            ));
        }
        Ok(())
    }
    fn coordinates(&self, p: [f64; 2]) -> Result<[f64; 2], ModelError> {
        let along = p[0] * self.along[0] + p[1] * self.along[1];
        let mut line = (p[0] * self.normal[0] + p[1] * self.normal[1] - self.phase) / self.spacing;
        if !along.is_finite() || !line.is_finite() || line.abs() >= MAXIMUM_LINE_INDEX {
            return Err(ModelError::new(
                "hatch grid exceeds finite or precise line-index limits",
            ));
        }
        // Snap roundoff at scanline vertices, keeping adjacent sampled edges on
        // the same half-open side when OCCT endpoints differ by a few ulps.
        let nearest = line.round();
        if (line - nearest).abs() <= 64.0 * f64::EPSILON * line.abs().max(1.0) {
            line = nearest;
        }
        Ok([along, line])
    }
    fn point(&self, line: i64, along: f64) -> Result<[f64; 2], ModelError> {
        let normal = line as f64 * self.spacing + self.phase;
        let p = [
            along * self.along[0] + normal * self.normal[0],
            along * self.along[1] + normal * self.normal[1],
        ];
        if !finite_pair(p) {
            return Err(ModelError::new("hatch endpoint exceeds finite coordinates"));
        }
        Ok(p)
    }
    fn crossings(
        &mut self,
        segment: [[f64; 2]; 2],
        lines: &mut BTreeMap<i64, Vec<f64>>,
    ) -> Result<(), ModelError> {
        let first = self.coordinates(segment[0])?;
        let second = self.coordinates(segment[1])?;
        let low = first[1].min(second[1]).ceil() as i64;
        let high = first[1].max(second[1]).ceil() as i64;
        // Half-open intervals avoid double-counting shared polygon vertices.
        self.charge(
            usize::try_from(high - low)
                .map_err(|_| ModelError::new("hatch scan range overflow"))?,
        )?;
        for index in low..high {
            let fraction = (index as f64 - first[1]) / (second[1] - first[1]);
            let position = (1.0 - fraction) * first[0] + fraction * second[0];
            if !position.is_finite() {
                return Err(ModelError::new(
                    "hatch intersection exceeds finite coordinates",
                ));
            }
            lines.entry(index).or_default().push(position);
        }
        Ok(())
    }
}
type Intervals = BTreeMap<i64, Vec<[f64; 2]>>;
fn face_intervals(
    mut crossings: BTreeMap<i64, Vec<f64>>,
    union: &mut Intervals,
) -> Result<(), ModelError> {
    for (line, values) in &mut crossings {
        values.sort_by(f64::total_cmp);
        if values.len() % 2 != 0 {
            return Err(ModelError::new(
                "sampled hatch face has an unpaired boundary intersection",
            ));
        }
        let intervals = union.entry(*line).or_default();
        for pair in values.as_chunks::<2>().0 {
            if pair[1] > pair[0] {
                intervals.push([pair[0], pair[1]]);
            }
        }
    }
    Ok(())
}
fn merge_intervals(intervals: &mut Vec<[f64; 2]>) {
    intervals.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    let mut written = 0;
    for read in 0..intervals.len() {
        let current = intervals[read];
        if written > 0 && current[0] <= intervals[written - 1][1] {
            intervals[written - 1][1] = intervals[written - 1][1].max(current[1]);
        } else {
            intervals[written] = current;
            written += 1;
        }
    }
    intervals.truncate(written);
}

/// E sampled boundary segments and K segment/scanline intersections cost
/// O(E + K log K) time, O(E + K) temporary storage. Work is capped at 2 million
/// samples/intersections per view; emitted endpoints share the export budget.
pub(super) fn append(
    session: &Session,
    view: &DrawingView,
    section: &Shape<'_>,
    options: DrawingRenderOptions,
    vertices: &mut usize,
    drawing: &mut GeneratedDrawing,
) -> Result<(), ModelError> {
    let Some(pattern) = view.hatching else {
        return Ok(());
    };
    let mut grid = ScanGrid::new(pattern);
    let mut union = Intervals::new();
    for face in session.subshapes(section, ShapeType::Face)? {
        let mut crossings = BTreeMap::new();
        let count = session.subshape_count(&face, ShapeType::Edge)?;
        grid.charge(
            count
                .checked_mul(options.curve_samples)
                .ok_or_else(|| ModelError::new("hatch sample count overflow"))?,
        )?;
        for edge in session.subshapes(&face, ShapeType::Edge)? {
            let points = session.edge_sample_points(&edge, options.curve_samples)?;
            let paper = points
                .into_iter()
                .map(|p| view.paper(view.project(p)?))
                .collect::<Result<Vec<_>, _>>()?;
            for segment in paper.windows(2) {
                grid.crossings([segment[0], segment[1]], &mut crossings)?;
            }
        }
        face_intervals(crossings, &mut union)?;
    }
    append_intervals(view, options, vertices, drawing, &grid, union)
}

fn append_intervals(
    view: &DrawingView,
    options: DrawingRenderOptions,
    vertices: &mut usize,
    drawing: &mut GeneratedDrawing,
    grid: &ScanGrid,
    union: Intervals,
) -> Result<(), ModelError> {
    for (index, mut intervals) in union {
        merge_intervals(&mut intervals);
        for [first, second] in intervals {
            let line = vec![grid.point(index, first)?, grid.point(index, second)?];
            for points_mm in clip(view, &line)? {
                *vertices = vertices
                    .checked_add(points_mm.len())
                    .ok_or_else(|| ModelError::new("hatch export vertex count overflow"))?;
                if *vertices > options.maximum_vertices {
                    return Err(ModelError::new("drawing exceeds export vertex budget"));
                }
                drawing.hatches.push(DrawingPolyline {
                    points_mm,
                    hidden: false,
                });
            }
        }
    }
    Ok(())
}
fn clip(view: &DrawingView, points: &[[f64; 2]]) -> Result<Vec<Vec<[f64; 2]>>, ModelError> {
    let Some(detail) = view.detail else {
        return Ok(vec![points.to_vec()]);
    };
    let window = DrawingDetail {
        minimum_mm: view.paper(detail.minimum_mm)?,
        maximum_mm: view.paper(detail.maximum_mm)?,
    };
    detail::clip_polyline(points, window)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scanline_parity_preserves_holes_and_merges_overlapping_material() {
        let mut crossings = BTreeMap::new();
        crossings.insert(1, vec![10.0, 3.0, 7.0, 0.0]);
        let mut union = Intervals::new();
        face_intervals(crossings, &mut union).unwrap();
        assert_eq!(union[&1], vec![[0.0, 3.0], [7.0, 10.0]]);
        union
            .get_mut(&1)
            .unwrap()
            .extend([[2.0, 5.0], [4.0, 8.0], [12.0, 13.0]]);
        merge_intervals(union.get_mut(&1).unwrap());
        assert_eq!(union[&1], vec![[0.0, 10.0], [12.0, 13.0]]);
        let mut bad = BTreeMap::new();
        bad.insert(0, vec![1.0]);
        assert!(face_intervals(bad, &mut Intervals::new()).is_err());
    }
    #[test]
    fn grid_snaps_roundoff_and_bounds_dense_or_imprecise_ranges() {
        let mut grid = ScanGrid::new(SectionHatching {
            angle_radians: 0.0,
            spacing_mm: 1.0,
            phase_mm: 0.0,
        });
        assert_eq!(grid.coordinates([0.0, -f64::EPSILON]).unwrap()[1], 0.0);
        let mut crossings = BTreeMap::new();
        grid.crossings([[0.0, 0.0], [10.0, 2.0]], &mut crossings)
            .unwrap();
        assert_eq!(crossings[&0], vec![0.0]);
        assert_eq!(crossings[&1], vec![5.0]);
        assert!(!crossings.contains_key(&2));
        assert!(
            grid.crossings([[0.0, 0.0], [0.0, 3e6]], &mut crossings)
                .is_err()
        );
        assert!(grid.coordinates([0.0, MAXIMUM_LINE_INDEX]).is_err());
    }
}
