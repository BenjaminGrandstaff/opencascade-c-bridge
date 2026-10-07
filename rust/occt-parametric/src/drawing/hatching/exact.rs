//! Kernel-trimmed hatch lines, independent of boundary sampling resolution.
use super::*;
use occt_bridge::AnalyticCurve;

const BATCH_LINES: i64 = 64;

struct FacePlane {
    anchor: Vec3,
    paper_anchor: [f64; 2],
    right: Vec3,
    up: Vec3,
    direction: Vec3,
    normal: Vec3,
    scale: f64,
}
impl FacePlane {
    fn new(session: &Session, view: &DrawingView, face: &Shape<'_>) -> Result<Self, ModelError> {
        if !session.face_is_planar(face)? {
            return Err(ModelError::new(
                "exact section hatching requires planar cut faces",
            ));
        }
        let frame = view.frame()?;
        let anchor = session.center_of_mass(face)?;
        let normal = session.face_normal(face)?;
        if dot(normal, frame.direction).abs() < 1.0 - 1e-10 {
            return Err(ModelError::new(
                "exact section hatching must look normal to the cut face",
            ));
        }
        Ok(Self {
            anchor,
            paper_anchor: view.paper(view.project(anchor)?)?,
            right: frame.x_axis,
            up: cross(frame.direction, frame.x_axis),
            direction: frame.direction,
            normal,
            scale: view.scale,
        })
    }
    fn world(&self, paper: [f64; 2]) -> Result<Vec3, ModelError> {
        let x = (paper[0] - self.paper_anchor[0]) / self.scale;
        let y = (paper[1] - self.paper_anchor[1]) / self.scale;
        let delta = Vec3::new(
            self.right.x * x + self.up.x * y,
            self.right.y * x + self.up.y * y,
            self.right.z * x + self.up.z * y,
        );
        // Lift onto the actual face plane, also for small allowed axis roundoff.
        let z = -dot(self.normal, delta) / dot(self.normal, self.direction);
        let point = Vec3::new(
            self.anchor.x + delta.x + self.direction.x * z,
            self.anchor.y + delta.y + self.direction.y * z,
            self.anchor.z + delta.z + self.direction.z * z,
        );
        if ![point.x, point.y, point.z].iter().all(|v| v.is_finite()) {
            return Err(ModelError::new(
                "exact hatch coordinates exceed finite limits",
            ));
        }
        Ok(point)
    }
}

fn paper_bounds(
    session: &Session,
    view: &DrawingView,
    face: &Shape<'_>,
) -> Result<Option<[[f64; 2]; 2]>, ModelError> {
    let bounds = session.exact_bounds(face)?;
    let mut low = [f64::INFINITY; 2];
    let mut high = [f64::NEG_INFINITY; 2];
    for x in [bounds.min.x, bounds.max.x] {
        for y in [bounds.min.y, bounds.max.y] {
            for z in [bounds.min.z, bounds.max.z] {
                let paper = view.paper(view.project(Vec3::new(x, y, z))?)?;
                for axis in 0..2 {
                    low[axis] = low[axis].min(paper[axis]);
                    high[axis] = high[axis].max(paper[axis]);
                }
            }
        }
    }
    if let Some(detail) = view.detail {
        let a = view.paper(detail.minimum_mm)?;
        let b = view.paper(detail.maximum_mm)?;
        for i in 0..2 {
            low[i] = low[i].max(a[i]);
            high[i] = high[i].min(b[i]);
        }
    }
    Ok((low[0] < high[0] && low[1] < high[1]).then_some([low, high]))
}

fn grid_range(
    grid: &ScanGrid,
    bounds: [[f64; 2]; 2],
    scale: f64,
) -> Result<([f64; 2], [i64; 2]), ModelError> {
    let [low, high] = bounds;
    let mut along = [f64::INFINITY, f64::NEG_INFINITY];
    let mut normal = [f64::INFINITY, f64::NEG_INFINITY];
    for paper in [low, high, [low[0], high[1]], [high[0], low[1]]] {
        let p = grid.coordinates(paper)?;
        along[0] = along[0].min(p[0]);
        along[1] = along[1].max(p[0]);
        normal[0] = normal[0].min(p[1]);
        normal[1] = normal[1].max(p[1]);
    }
    let margin = scale.max(128.0 * f64::EPSILON * along[0].abs().max(along[1].abs()));
    along[0] -= margin;
    along[1] += margin;
    if !finite_pair(along) {
        return Err(ModelError::new(
            "exact hatch line extent exceeds finite limits",
        ));
    }
    Ok((along, [normal[0].ceil() as i64, normal[1].ceil() as i64]))
}

struct HatchBatch {
    along: [f64; 2],
    indices: std::ops::Range<i64>,
}

fn append_batch(
    session: &Session,
    view: &DrawingView,
    face: &Shape<'_>,
    plane: &FacePlane,
    grid: &mut ScanGrid,
    batch: HatchBatch,
    union: &mut Intervals,
) -> Result<(), ModelError> {
    let HatchBatch { along, indices } = batch;
    let lines = indices
        .clone()
        .map(|index| {
            let points = [
                plane.world(grid.point(index, along[0])?)?,
                plane.world(grid.point(index, along[1])?)?,
            ];
            Ok(session.create_polyline_wire(&points, false)?)
        })
        .collect::<Result<Vec<_>, ModelError>>()?;
    let compound = session.create_compound(&lines.iter().collect::<Vec<_>>())?;
    let material = session.common(&compound, face)?;
    let count = session.subshape_count(&material, ShapeType::Edge)?;
    grid.charge(count)?;
    for edge in session.subshapes(&material, ShapeType::Edge)? {
        let Some(AnalyticCurve::Line { start, end }) = session.edge_analytic_curve(&edge)? else {
            return Err(ModelError::new(
                "kernel hatch trim returned a non-linear edge",
            ));
        };
        let first = grid.coordinates(view.paper(view.project(start)?)?)?;
        let second = grid.coordinates(view.paper(view.project(end)?)?)?;
        let index = (0.5 * first[1] + 0.5 * second[1]).round() as i64;
        if !indices.contains(&index)
            || (first[1] - index as f64).abs() > 0.25
            || (second[1] - index as f64).abs() > 0.25
        {
            return Err(ModelError::new(
                "kernel hatch trim exceeds scanline precision limits",
            ));
        }
        let interval = [first[0].min(second[0]), first[0].max(second[0])];
        if interval[0] < interval[1] {
            union.entry(index).or_default().push(interval);
        }
    }
    Ok(())
}

/// Per-face bounded batches avoid an unbounded all-scanline Boolean. Kernel work
/// is topology-dependent; preprocessing is O(F + E + L), interval union O(K log K),
/// and storage O(F + K + 64), beyond source/kernel topology. Edge/line candidates
/// and returned intervals share a two-million work limit; endpoints use the
/// drawing's cumulative vertex budget. ON-boundary line segments are retained;
/// isolated tangent points create no hatch interval.
pub(super) fn append(
    session: &Session,
    view: &DrawingView,
    section: &Shape<'_>,
    options: DrawingRenderOptions,
    vertices: &mut usize,
    drawing: &mut GeneratedDrawing,
    work: &mut usize,
) -> Result<(), ModelError> {
    let mut grid = ScanGrid::new(view.hatching.expect("hatching checked before exact path"));
    grid.work = *work;
    let mut union = Intervals::new();
    for face in session.subshapes(section, ShapeType::Face)? {
        let Some(bounds) = paper_bounds(session, view, &face)? else {
            continue;
        };
        let plane = FacePlane::new(session, view, &face)?;
        let magnitude = plane
            .anchor
            .x
            .abs()
            .max(plane.anchor.y.abs())
            .max(plane.anchor.z.abs())
            .max(1.0);
        if grid.spacing / view.scale < 1e-6_f64.max(256.0 * f64::EPSILON * magnitude) {
            return Err(ModelError::new(
                "exact hatch spacing is below kernel/coordinate resolution",
            ));
        }
        let (along, [low, high]) = grid_range(&grid, bounds, view.scale)?;
        let edges = session.subshape_count(&face, ShapeType::Edge)?;
        let line_count = usize::try_from(high - low)
            .map_err(|_| ModelError::new("exact hatch scan range overflow"))?;
        grid.charge(
            line_count
                .checked_mul(edges.max(1))
                .ok_or_else(|| ModelError::new("exact hatch work overflow"))?,
        )?;
        let mut first = low;
        while first < high {
            let end = (first + BATCH_LINES).min(high);
            append_batch(
                session,
                view,
                &face,
                &plane,
                &mut grid,
                HatchBatch {
                    along,
                    indices: first..end,
                },
                &mut union,
            )?;
            first = end;
        }
    }
    *work = grid.work;
    append_intervals(view, options, vertices, drawing, &grid, union)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn spline_hatch_endpoints_follow_a_parabola_and_tangencies_add_no_segments() {
        let session = Session::new().unwrap();
        let wire = session
            .create_curve_wire(
                &[
                    occt_bridge::CurveSegment::Spline {
                        points: vec![
                            Vec3::new(-1.0, 1.0, 0.0),
                            Vec3::new(0.0, 0.0, 0.0),
                            Vec3::new(1.0, 1.0, 0.0),
                        ],
                        start_tangent: None,
                        end_tangent: None,
                        periodic: false,
                    },
                    occt_bridge::CurveSegment::Line {
                        start: Vec3::new(1.0, 1.0, 0.0),
                        end: Vec3::new(-1.0, 1.0, 0.0),
                    },
                ],
                true,
            )
            .unwrap();
        let face = session.create_face_from_wire(&wire).unwrap();
        let mut view = DrawingView {
            id: "curved-hatch".into(),
            outputs: vec![],
            origin: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
            direction: VectorQuantity::scalars(0.0, 0.0, 1.0),
            x_axis: VectorQuantity::scalars(1.0, 0.0, 0.0),
            paper_origin_mm: [0.0, 0.0],
            scale: 1.0,
            show_hidden: false,
            kind: DrawingViewKind::Slice,
            detail: None,
            material_hatching: Default::default(),
            hatching: Some(SectionHatching {
                angle_radians: 0.0,
                spacing_mm: 0.1,
                phase_mm: 0.05,
            }),
        };
        let mut drawing = GeneratedDrawing {
            curves: vec![],
            gdt_lines: vec![],
            gdt_labels: vec![],
            sheet_lines: vec![],
            sheet_labels: vec![],
            id: "curved".into(),
            title: "Curved".into(),
            paper_size_mm: [100.0, 100.0],
            polylines: vec![],
            guides: vec![],
            hatches: vec![],
            labels: vec![],
            metadata: Default::default(),
            generated_variants: 0,
        };
        let options = DrawingRenderOptions {
            exact_curves: true,
            curve_samples: 2,
            ..DrawingRenderOptions::default()
        };
        append(
            &session,
            &view,
            &face,
            options,
            &mut 0,
            &mut drawing,
            &mut 0,
        )
        .unwrap();
        assert_eq!(drawing.hatches.len(), 10);
        for line in &drawing.hatches {
            let y = line.points_mm[0][1];
            assert!((line.points_mm[0][0] + y.sqrt()).abs() < 1e-7);
            assert!((line.points_mm[1][0] - y.sqrt()).abs() < 1e-7);
        }
        let circle = session
            .create_circle_wire(Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 1.0), 2.0)
            .unwrap();
        let disk = session.create_face_from_wire(&circle).unwrap();
        drawing.hatches.clear();
        view.hatching = Some(SectionHatching {
            angle_radians: 0.0,
            spacing_mm: 1.0,
            phase_mm: 0.0,
        });
        append(
            &session,
            &view,
            &disk,
            options,
            &mut 0,
            &mut drawing,
            &mut 0,
        )
        .unwrap();
        assert_eq!(drawing.hatches.len(), 3);
        assert!(
            drawing
                .hatches
                .iter()
                .all(|line| line.points_mm[0][0] < line.points_mm[1][0])
        );
        // More than one batch: 100 horizontal lines against the same face.
        drawing.hatches.clear();
        view.hatching.as_mut().unwrap().spacing_mm = 0.01;
        view.hatching.as_mut().unwrap().phase_mm = 0.005;
        append(
            &session,
            &view,
            &face,
            options,
            &mut 0,
            &mut drawing,
            &mut 0,
        )
        .unwrap();
        assert_eq!(drawing.hatches.len(), 100);
        assert!(
            append(
                &session,
                &view,
                &face,
                options,
                &mut (options.maximum_vertices - 1),
                &mut drawing,
                &mut 0,
            )
            .is_err()
        );
        view.hatching.as_mut().unwrap().spacing_mm = 1.01e-6;
        view.hatching.as_mut().unwrap().angle_radians = std::f64::consts::FRAC_PI_4;
        assert!(
            append(
                &session,
                &view,
                &face,
                options,
                &mut 0,
                &mut drawing,
                &mut 0
            )
            .unwrap_err()
            .message
            .contains("work budget")
        );
        // Separate line families share the same view work limit.
        view.hatching = Some(SectionHatching {
            angle_radians: 0.0,
            spacing_mm: 0.01,
            phase_mm: 0.005,
        });
        let mut work = MAXIMUM_WORK - 500;
        append(
            &session,
            &view,
            &face,
            options,
            &mut 0,
            &mut drawing,
            &mut work,
        )
        .unwrap();
        assert!(
            append(
                &session,
                &view,
                &face,
                options,
                &mut 0,
                &mut drawing,
                &mut work
            )
            .unwrap_err()
            .message
            .contains("work budget")
        );
        drop(disk);
        drop(circle);
        drop(face);
        drop(wire);
        assert_eq!(session.shape_count().unwrap(), 0);
    }
}
