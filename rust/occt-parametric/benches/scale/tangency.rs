//! Face tangency measured where booleans record no continuity, and merging
//! the faces they leave split.

use super::*;
use occt_bridge::Shape;

/// Every face of a 400-hole stadium plate checked for tangency to its top,
/// as `FaceSelector::TangentTo` does. Each hole wall shares an unrecorded
/// edge with the top, so each is measured: the worst case for inference.
pub(crate) fn measured_tangency_case() -> Outcome {
    timed(
        format!("measured tangency: {MANY_HOLES}-hole stadium plate faces vs top"),
        ms(2_000),
        Expectation::Required,
        || {
            let session = Session::new()?;
            let plate = stadium_plate(&session)?;
            let faces = (0..session.subshape_count(&plate, ShapeType::Face)?)
                .map(|index| session.subshape(&plate, ShapeType::Face, index))
                .collect::<Result<Vec<_>, _>>()?;
            // The block's part of the top: the largest face looking up.
            let mut top = None;
            let mut largest = 0.0;
            for (index, face) in faces.iter().enumerate() {
                if session.face_is_planar(face)? && session.face_normal(face)?.z > 0.999 {
                    let area = session.surface_area(face)?;
                    if area > largest {
                        (top, largest) = (Some(index), area);
                    }
                }
            }
            let top = &faces[top.ok_or_else(|| failure("no top face".into()))?];
            let scan = Instant::now();
            let (mut recorded, mut measured) = (0, 0);
            for face in &faces {
                recorded += usize::from(session.faces_are_tangent(&plate, face, top)?);
                measured += usize::from(session.faces_are_tangent_within(&plate, face, top, 1e-3)?);
            }
            let scan = scan.elapsed();
            // Only the half-disc split off inside the block's footprint.
            if recorded != 0 || measured != 1 {
                return Err(failure(format!(
                    "{recorded} recorded and {measured} measured tangent faces; expected 0 and 1"
                )));
            }
            Ok(format!(
                "{} faces checked in {:.3} s; 1 measured tangent, {MANY_HOLES} sharp rims measured",
                faces.len(),
                scan.as_secs_f64()
            ))
        },
    )
}

/// A radius-210 cylinder fused flush with a 420 x 420 x 10 block, drilled
/// with `MANY_HOLES` holes in one cut.
fn stadium_plate(session: &Session) -> Result<Shape<'_>, ModelError> {
    let up = Vec3::new(0.0, 0.0, 1.0);
    let round = session.create_cylinder(Vec3::new(0.0, 0.0, 0.0), up, 210.0, 10.0)?;
    let block = session.create_box(Vec3::new(0.0, -210.0, 0.0), Vec3::new(420.0, 420.0, 10.0))?;
    let stadium = session.fuse(&round, &block)?;
    let holes = (0..MANY_HOLES)
        .map(|index| {
            let (row, column) = ((index / 20) as f64, (index % 20) as f64);
            session.create_cylinder(
                Vec3::new(10.0 + column * 20.0, -200.0 + row * 20.0, -1.0),
                up,
                4.0,
                12.0,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let tools = session.create_compound(&holes.iter().collect::<Vec<_>>())?;
    Ok(session.cut(&stadium, &tools)?)
}

/// Merges the split top and bottom of the 400-hole stadium plate.
pub(crate) fn unify_case() -> Outcome {
    timed(
        format!("unify a {MANY_HOLES}-hole stadium plate"),
        ms(2_000),
        Expectation::Required,
        || {
            let session = Session::new()?;
            let plate = stadium_plate(&session)?;
            let before = session.subshape_count(&plate, ShapeType::Face)?;
            let start = Instant::now();
            let unified = session.unify_same_domain(&plate, 1e-7, 1e-9)?;
            let elapsed = start.elapsed();
            let after = session.subshape_count(&unified, ShapeType::Face)?;
            if after != MANY_HOLES + 6 {
                return Err(failure(format!(
                    "{before} faces unified to {after}; expected {}",
                    MANY_HOLES + 6
                )));
            }
            let (solid, merged) = (session.volume(&plate)?, session.volume(&unified)?);
            if (solid - merged).abs() > 1e-9 * solid {
                return Err(failure(format!("volume {solid} became {merged}")));
            }
            Ok(format!(
                "{before} faces merged to {after} in {:.3} s",
                elapsed.as_secs_f64()
            ))
        },
    )
}

/// Exact pull ranges for every face of the 400-hole stadium plate: planes,
/// the round end, and 400 bores, all analytic.
pub(crate) fn pull_ranges_case() -> Outcome {
    timed(
        format!("exact pull ranges: {MANY_HOLES}-hole stadium plate"),
        ms(2_000),
        Expectation::Required,
        || {
            let session = Session::new()?;
            let plate = stadium_plate(&session)?;
            let start = Instant::now();
            let ranges = session.face_pull_ranges(&plate, Vec3::new(0.0, 0.0, 1.0))?;
            let elapsed = start.elapsed();
            let exact = ranges.iter().flatten().count();
            // Bores and flat sides run along the pull; the round end too.
            let vertical = ranges
                .iter()
                .flatten()
                .filter(|range| range.minimum.0.abs() < 1e-12 && range.maximum.0.abs() < 1e-12)
                .count();
            if exact != ranges.len() || vertical != MANY_HOLES + 4 {
                return Err(failure(format!(
                    "{exact} of {} faces exact, {vertical} vertical; expected all and {}",
                    ranges.len(),
                    MANY_HOLES + 4
                )));
            }
            Ok(format!(
                "{} faces bounded exactly in {:.3} s",
                ranges.len(),
                elapsed.as_secs_f64()
            ))
        },
    )
}

/// The work of an undercut rule on the 400-hole stadium plate: clip at the
/// mid-plane, then exact pull ranges for every face of both halves.
pub(crate) fn undercut_case() -> Outcome {
    timed(
        format!("undercut screening: {MANY_HOLES}-hole stadium plate halves"),
        ms(3_000),
        Expectation::Required,
        || {
            let session = Session::new()?;
            let plate = stadium_plate(&session)?;
            let up = Vec3::new(0.0, 0.0, 1.0);
            let start = Instant::now();
            let mut faces = 0;
            let mut unmeasured = 0;
            for keep_positive in [true, false] {
                let half =
                    session.clip_by_plane(&plate, Vec3::new(0.0, 0.0, 5.0), up, keep_positive)?;
                let ranges = session.face_pull_ranges(&half, up)?;
                faces += ranges.len();
                unmeasured += ranges.iter().filter(|range| range.is_none()).count();
                session.remove(half)?;
            }
            let elapsed = start.elapsed();
            if unmeasured != 0 {
                return Err(failure(format!("{unmeasured} faces unmeasured")));
            }
            Ok(format!(
                "{faces} half faces bounded exactly in {:.3} s",
                elapsed.as_secs_f64()
            ))
        },
    )
}
