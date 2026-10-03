//! Individual selector queries: size, radius and curvature, normals,
//! adjacency, tangency, history, proximity, and extremes.

use super::*;

pub(crate) fn validate_relative_tolerance(value: f64, kind: &str) -> Result<(), ModelError> {
    if !(0.0..=1.0).contains(&value) {
        return Err(ModelError::new(format!(
            "{kind} relative tolerance must be between 0 and 1"
        )));
    }
    Ok(())
}

pub(crate) fn select_longest_edges<'session>(
    session: &'session Session,
    shape: &Shape<'session>,
    allow_ties: bool,
    relative_tolerance: f64,
) -> Result<Vec<Shape<'session>>, ModelError> {
    validate_relative_tolerance(relative_tolerance, "longest-edge selector")?;
    let count = session.subshape_count(shape, ShapeType::Edge)?;
    let mut candidates = Vec::with_capacity(count);
    for index in 0..count {
        let edge = match session.subshape(shape, ShapeType::Edge, index) {
            Ok(edge) => edge,
            Err(error) => {
                cleanup_shapes(
                    session,
                    candidates
                        .into_iter()
                        .map(|item: (Shape<'session>, f64)| item.0),
                );
                return Err(error.into());
            }
        };
        match session.edge_length(&edge) {
            Ok(length) => candidates.push((edge, length)),
            Err(error) => {
                let _ = session.remove(edge);
                cleanup_shapes(session, candidates.into_iter().map(|item| item.0));
                return Err(error.into());
            }
        }
    }
    if candidates.is_empty() {
        return Err(ModelError::new("longest-edge selector found no candidates"));
    }
    candidates.sort_by(|left, right| right.1.total_cmp(&left.1));
    let threshold = candidates[0].1 * (1.0 - relative_tolerance);
    let split = candidates.partition_point(|candidate| candidate.1 >= threshold);
    let rejected = candidates.split_off(split);
    cleanup_shapes(session, rejected.into_iter().map(|item| item.0));
    if !allow_ties && candidates.len() > 1 {
        let count = candidates.len();
        cleanup_shapes(session, candidates.into_iter().map(|item| item.0));
        return Err(ModelError::new(format!(
            "longest-edge selector is ambiguous across {count} edges"
        )));
    }
    Ok(candidates.into_iter().map(|item| item.0).collect())
}

pub(crate) fn select_circular_edges_by_radius<'session>(
    session: &'session Session,
    shape: &Shape<'session>,
    minimum: f64,
    maximum: f64,
) -> Result<Vec<Shape<'session>>, ModelError> {
    if minimum < 0.0 || maximum < minimum {
        return Err(ModelError::new(
            "circular-edge radius range must be nonnegative and ordered",
        ));
    }
    let count = session.subshape_count(shape, ShapeType::Edge)?;
    let mut selected = Vec::new();
    for index in 0..count {
        let edge = match session.subshape(shape, ShapeType::Edge, index) {
            Ok(edge) => edge,
            Err(error) => {
                cleanup_shapes(session, selected);
                return Err(error.into());
            }
        };
        match session.edge_circle_radius(&edge) {
            Ok(Some(radius)) if (minimum..=maximum).contains(&radius) => selected.push(edge),
            Ok(_) => {
                let _ = session.remove(edge);
            }
            Err(error) => {
                let _ = session.remove(edge);
                cleanup_shapes(session, selected);
                return Err(error.into());
            }
        }
    }
    if selected.is_empty() {
        return Err(ModelError::new(
            "circular-edge radius selector found no matches",
        ));
    }
    Ok(selected)
}

pub(crate) fn select_edges_by_curvature_radius<'session>(
    session: &'session Session,
    shape: &Shape<'session>,
    minimum: f64,
    maximum: f64,
) -> Result<Vec<Shape<'session>>, ModelError> {
    if minimum <= 0.0 || maximum < minimum {
        return Err(ModelError::new(
            "curvature-radius range must be positive and ordered",
        ));
    }
    filter_edges(
        session,
        shape,
        "curvature-radius selector found no matches",
        |_, edge| {
            Ok(match session.edge_curvature(edge)? {
                Some(curvature) if curvature > f64::EPSILON => {
                    (minimum..=maximum).contains(&(1.0 / curvature))
                }
                _ => false,
            })
        },
    )
}

/// Visits every edge of `shape` and keeps those `matches` accepts. Rejected
/// edges are released immediately; on error, or when nothing matches, every
/// selected handle is released as well.
pub(crate) fn filter_edges<'session>(
    session: &'session Session,
    shape: &Shape<'session>,
    no_match_message: &str,
    mut matches: impl FnMut(usize, &Shape<'session>) -> Result<bool, ModelError>,
) -> Result<Vec<Shape<'session>>, ModelError> {
    let count = session.subshape_count(shape, ShapeType::Edge)?;
    let mut selected = Vec::new();
    for index in 0..count {
        let edge = match session.subshape(shape, ShapeType::Edge, index) {
            Ok(edge) => edge,
            Err(error) => {
                cleanup_shapes(session, selected);
                return Err(error.into());
            }
        };
        match matches(index, &edge) {
            Ok(true) => selected.push(edge),
            Ok(false) => {
                let _ = session.remove(edge);
            }
            Err(error) => {
                let _ = session.remove(edge);
                cleanup_shapes(session, selected);
                return Err(error);
            }
        }
    }
    if selected.is_empty() {
        return Err(ModelError::new(no_match_message));
    }
    Ok(selected)
}

/// Decides a curvature interval test from proven bounds. `lowest` and
/// `highest` are curvatures (reciprocal radii). `None` means the bounds
/// straddle a range boundary.
pub(crate) fn classify_curvature_bounds(
    extrema: &CurvatureExtrema,
    lowest: f64,
    highest: f64,
    require_entire_edge: bool,
) -> Option<bool> {
    if require_entire_edge {
        if extrema.minimum_lower_bound >= lowest && extrema.maximum_upper_bound <= highest {
            Some(true)
        } else if extrema.minimum < lowest || extrema.maximum > highest {
            Some(false)
        } else {
            None
        }
    } else if extrema.maximum >= lowest && extrema.minimum <= highest {
        Some(true)
    } else if extrema.maximum_upper_bound < lowest || extrema.minimum_lower_bound > highest {
        Some(false)
    } else {
        None
    }
}

pub(crate) fn select_edges_by_bounded_curvature_radius<'session>(
    session: &'session Session,
    shape: &Shape<'session>,
    minimum: f64,
    maximum: f64,
    relative_tolerance: f64,
    require_entire_edge: bool,
) -> Result<Vec<Shape<'session>>, ModelError> {
    if minimum <= 0.0 || maximum < minimum || !maximum.is_finite() {
        return Err(ModelError::new(
            "curvature-radius bounds must be positive, finite, and ordered",
        ));
    }
    if !(relative_tolerance > 0.0 && relative_tolerance <= 1.0) {
        return Err(ModelError::new(
            "curvature-radius relative tolerance must be in (0, 1]",
        ));
    }
    filter_edges(
        session,
        shape,
        "bounded curvature-radius selector found no matches",
        |index, edge| {
            let extrema = session
                .edge_curvature_extrema(edge, relative_tolerance)
                .map_err(|error| {
                    ModelError::new(format!(
                        "curvature-radius bounds for edge {index}: {}",
                        error.message
                    ))
                })?;
            classify_curvature_bounds(&extrema, 1.0 / maximum, 1.0 / minimum, require_entire_edge)
                .ok_or_else(|| {
                    let radius = |curvature: f64| 1.0 / curvature;
                    ModelError::new(format!(
                        "edge {index} curvature radius bounds straddle the selector range \
                         (minimum radius in [{:.9}, {:.9}] mm, maximum radius in [{:.9}, {:.9}] mm); \
                         tighten relative_tolerance",
                        radius(extrema.maximum_upper_bound),
                        radius(extrema.maximum),
                        radius(extrema.minimum),
                        radius(extrema.minimum_lower_bound),
                    ))
                })
        },
    )
}

pub(crate) fn select_edges_by_curvature_radius_range<'session>(
    session: &'session Session,
    shape: &Shape<'session>,
    minimum: f64,
    maximum: f64,
    sample_count: usize,
    require_entire_edge: bool,
) -> Result<Vec<Shape<'session>>, ModelError> {
    if minimum <= 0.0 || maximum < minimum {
        return Err(ModelError::new(
            "curvature-radius range must be positive and ordered",
        ));
    }
    if !(2..=100_000).contains(&sample_count) {
        return Err(ModelError::new(
            "curvature-radius sample count must be 2..100000",
        ));
    }
    filter_edges(
        session,
        shape,
        "curvature-radius range selector found no matches",
        |_, edge| {
            let (minimum_curvature, maximum_curvature) =
                session.edge_curvature_range(edge, sample_count)?;
            if maximum_curvature <= f64::EPSILON {
                return Ok(false);
            }
            let minimum_radius = 1.0 / maximum_curvature;
            let maximum_radius = if minimum_curvature <= f64::EPSILON {
                f64::INFINITY
            } else {
                1.0 / minimum_curvature
            };
            Ok(if require_entire_edge {
                minimum_radius >= minimum && maximum_radius <= maximum
            } else {
                maximum_radius >= minimum && minimum_radius <= maximum
            })
        },
    )
}

pub(crate) fn select_largest_faces<'session>(
    session: &'session Session,
    shape: &Shape<'session>,
    planar_only: bool,
    allow_ties: bool,
    relative_tolerance: f64,
) -> Result<Vec<Shape<'session>>, ModelError> {
    validate_relative_tolerance(relative_tolerance, "largest-face selector")?;
    let count = session.subshape_count(shape, ShapeType::Face)?;
    let mut candidates = Vec::with_capacity(count);
    for index in 0..count {
        let face = match session.subshape(shape, ShapeType::Face, index) {
            Ok(face) => face,
            Err(error) => {
                cleanup_shapes(
                    session,
                    candidates
                        .into_iter()
                        .map(|item: (Shape<'session>, f64)| item.0),
                );
                return Err(error.into());
            }
        };
        if planar_only {
            match session.face_is_planar(&face) {
                Ok(true) => {}
                Ok(false) => {
                    let _ = session.remove(face);
                    continue;
                }
                Err(error) => {
                    let _ = session.remove(face);
                    cleanup_shapes(session, candidates.into_iter().map(|item| item.0));
                    return Err(error.into());
                }
            }
        }
        match session.surface_area(&face) {
            Ok(area) => candidates.push((face, area)),
            Err(error) => {
                let _ = session.remove(face);
                cleanup_shapes(session, candidates.into_iter().map(|item| item.0));
                return Err(error.into());
            }
        }
    }
    if candidates.is_empty() {
        return Err(ModelError::new("largest-face selector found no candidates"));
    }
    candidates.sort_by(|left, right| right.1.total_cmp(&left.1));
    let threshold = candidates[0].1 * (1.0 - relative_tolerance);
    let split = candidates.partition_point(|candidate| candidate.1 >= threshold);
    let rejected = candidates.split_off(split);
    cleanup_shapes(session, rejected.into_iter().map(|item| item.0));
    if !allow_ties && candidates.len() > 1 {
        let count = candidates.len();
        cleanup_shapes(session, candidates.into_iter().map(|item| item.0));
        return Err(ModelError::new(format!(
            "largest-face selector is ambiguous across {count} faces"
        )));
    }
    Ok(candidates.into_iter().map(|item| item.0).collect())
}

pub(crate) fn select_faces_by_normal<'session>(
    session: &'session Session,
    shape: &Shape<'session>,
    direction: Vec3,
    minimum_dot: f64,
) -> Result<Vec<Shape<'session>>, ModelError> {
    let magnitude = direction.x.hypot(direction.y.hypot(direction.z));
    if magnitude <= f64::EPSILON {
        return Err(ModelError::new("face orientation direction is zero"));
    }
    if !(-1.0..=1.0).contains(&minimum_dot) {
        return Err(ModelError::new(
            "face orientation minimum dot must be between -1 and 1",
        ));
    }
    let direction = Vec3::new(
        direction.x / magnitude,
        direction.y / magnitude,
        direction.z / magnitude,
    );
    let count = session.subshape_count(shape, ShapeType::Face)?;
    let mut selected = Vec::new();
    for index in 0..count {
        let face = match session.subshape(shape, ShapeType::Face, index) {
            Ok(face) => face,
            Err(error) => {
                cleanup_shapes(session, selected);
                return Err(error.into());
            }
        };
        let normal = match session.face_normal(&face) {
            Ok(normal) => normal,
            Err(error) => {
                let _ = session.remove(face);
                cleanup_shapes(session, selected);
                return Err(error.into());
            }
        };
        let dot = normal.x * direction.x + normal.y * direction.y + normal.z * direction.z;
        if dot >= minimum_dot {
            selected.push(face);
        } else {
            let _ = session.remove(face);
        }
    }
    if selected.is_empty() {
        return Err(ModelError::new(
            "face orientation selector found no matches",
        ));
    }
    Ok(selected)
}

pub(crate) fn select_faces_adjacent_to_edges<'session>(
    session: &'session Session,
    shape: &Shape<'session>,
    edges: Vec<Shape<'session>>,
    minimum_count: usize,
) -> Result<Vec<Shape<'session>>, ModelError> {
    if minimum_count == 0 {
        cleanup_shapes(session, edges);
        return Err(ModelError::new(
            "face adjacency minimum count must be positive",
        ));
    }
    if edges.len() < minimum_count {
        let edge_count = edges.len();
        cleanup_shapes(session, edges);
        return Err(ModelError::new(format!(
            "face adjacency requires {minimum_count} edges but selector resolved {edge_count}"
        )));
    }
    let count = match session.subshape_count(shape, ShapeType::Face) {
        Ok(count) => count,
        Err(error) => {
            cleanup_shapes(session, edges);
            return Err(error.into());
        }
    };
    let mut selected = Vec::new();
    for index in 0..count {
        let face = match session.subshape(shape, ShapeType::Face, index) {
            Ok(face) => face,
            Err(error) => {
                cleanup_shapes(session, selected);
                cleanup_shapes(session, edges);
                return Err(error.into());
            }
        };
        let mut adjacent_count = 0;
        for edge in &edges {
            match session.is_adjacent(shape, &face, edge) {
                Ok(true) => adjacent_count += 1,
                Ok(false) => {}
                Err(error) => {
                    let _ = session.remove(face);
                    cleanup_shapes(session, selected);
                    cleanup_shapes(session, edges);
                    return Err(error.into());
                }
            }
        }
        if adjacent_count >= minimum_count {
            selected.push(face);
        } else {
            let _ = session.remove(face);
        }
    }
    cleanup_shapes(session, edges);
    if selected.is_empty() {
        return Err(ModelError::new("face adjacency selector found no matches"));
    }
    Ok(selected)
}

pub(crate) fn select_faces_tangent_to_faces<'session>(
    session: &'session Session,
    shape: &Shape<'session>,
    sources: Vec<Shape<'session>>,
    minimum_count: usize,
) -> Result<Vec<Shape<'session>>, ModelError> {
    if minimum_count == 0 {
        cleanup_shapes(session, sources);
        return Err(ModelError::new(
            "face tangency minimum count must be positive",
        ));
    }
    if sources.len() < minimum_count {
        let source_count = sources.len();
        cleanup_shapes(session, sources);
        return Err(ModelError::new(format!(
            "face tangency requires {minimum_count} source faces but selector resolved {source_count}"
        )));
    }
    let count = match session.subshape_count(shape, ShapeType::Face) {
        Ok(count) => count,
        Err(error) => {
            cleanup_shapes(session, sources);
            return Err(error.into());
        }
    };
    let mut selected = Vec::new();
    for index in 0..count {
        let face = match session.subshape(shape, ShapeType::Face, index) {
            Ok(face) => face,
            Err(error) => {
                cleanup_shapes(session, selected);
                cleanup_shapes(session, sources);
                return Err(error.into());
            }
        };
        let mut tangent_count = 0;
        for source in &sources {
            match session.faces_are_tangent(shape, &face, source) {
                Ok(true) => tangent_count += 1,
                Ok(false) => {}
                Err(error) => {
                    let _ = session.remove(face);
                    cleanup_shapes(session, selected);
                    cleanup_shapes(session, sources);
                    return Err(error.into());
                }
            }
        }
        if tangent_count >= minimum_count {
            selected.push(face);
        } else {
            let _ = session.remove(face);
        }
    }
    cleanup_shapes(session, sources);
    if selected.is_empty() {
        return Err(ModelError::new("face tangency selector found no matches"));
    }
    Ok(selected)
}

/// O(edges * history records + targets + faces²) time: set deduplication uses
/// pairwise identity queries on selected topology, without scanning model graphs.
/// Space/handles are O(edges + faces); intermediates drop on success and failure.
pub(crate) fn generated_faces_from_edges<'session>(
    session: &'session Session,
    result: &Shape<'session>,
    sources: Vec<Shape<'session>>,
    source_feature: &str,
) -> Result<Vec<Shape<'session>>, ModelError> {
    let mut faces = Vec::new();
    for source in sources {
        let before = faces.len();
        let count = session.history_count(result, &source, HistoryRelation::Generated)?;
        for index in 0..count {
            let target = session.history(result, &source, HistoryRelation::Generated, index)?;
            if session.shape_type(&target)? == ShapeType::Face {
                faces.push(target);
            }
        }
        if faces.len() == before {
            return Err(ModelError::new(format!(
                "edge history selector from '{source_feature}' resolved to no faces"
            )));
        }
    }
    compose_shape_sets(session, vec![faces], ShapeSetOperation::Union)
}

pub(crate) fn resolve_history<'session>(
    session: &'session Session,
    result: &Shape<'session>,
    sources: Vec<Shape<'session>>,
    relation: HistoryRelation,
    source_feature: &str,
    kind: &str,
) -> Result<Vec<Shape<'session>>, ModelError> {
    let mut resolved = Vec::new();
    for source in &sources {
        let count = match session.history_count(result, source, relation) {
            Ok(count) => count,
            Err(error) => {
                cleanup_shapes(session, resolved);
                cleanup_shapes(session, sources);
                return Err(error.into());
            }
        };
        if count == 0 {
            cleanup_shapes(session, resolved);
            cleanup_shapes(session, sources);
            return Err(ModelError::new(format!(
                "history selector from '{source_feature}' resolved to no {kind}"
            )));
        }
        for index in 0..count {
            match session.history(result, source, relation, index) {
                Ok(shape) => resolved.push(shape),
                Err(error) => {
                    cleanup_shapes(session, resolved);
                    cleanup_shapes(session, sources);
                    return Err(error.into());
                }
            }
        }
    }
    cleanup_shapes(session, sources);
    Ok(resolved)
}

pub(crate) fn select_nearest_center<'session>(
    session: &'session Session,
    shape: &Shape<'session>,
    shape_type: ShapeType,
    kind: &str,
    target: Vec3,
    maximum_distance: f64,
) -> Result<Shape<'session>, ModelError> {
    if maximum_distance < 0.0 {
        return Err(ModelError::new(
            "{kind} selector maximum distance must be nonnegative",
        ));
    }
    let count = session.subshape_count(shape, shape_type)?;
    let mut candidates: Vec<(Shape<'session>, f64)> = Vec::with_capacity(count);
    for index in 0..count {
        let candidate = match session.subshape(shape, shape_type, index) {
            Ok(candidate) => candidate,
            Err(error) => {
                cleanup_shapes(session, candidates.into_iter().map(|item| item.0));
                return Err(error.into());
            }
        };
        let center = match session.center_of_mass(&candidate) {
            Ok(center) => center,
            Err(error) => {
                let _ = session.remove(candidate);
                cleanup_shapes(session, candidates.into_iter().map(|item| item.0));
                return Err(error.into());
            }
        };
        let distance = ((center.x - target.x).powi(2)
            + (center.y - target.y).powi(2)
            + (center.z - target.z).powi(2))
        .sqrt();
        candidates.push((candidate, distance));
    }
    if candidates.is_empty() {
        return Err(ModelError::new(format!(
            "{kind} selector found no candidates"
        )));
    }
    candidates.sort_by(|left, right| left.1.total_cmp(&right.1));
    let best_distance = candidates[0].1;
    if best_distance > maximum_distance {
        cleanup_shapes(session, candidates.into_iter().map(|item| item.0));
        return Err(ModelError::new(format!(
            "nearest {kind} is {best_distance} mm away, beyond the {maximum_distance} mm limit"
        )));
    }
    let ambiguity_tolerance = 1e-9_f64.max(best_distance.abs() * 1e-12);
    if candidates
        .get(1)
        .is_some_and(|candidate| (candidate.1 - best_distance).abs() <= ambiguity_tolerance)
    {
        cleanup_shapes(session, candidates.into_iter().map(|item| item.0));
        return Err(ModelError::new(format!(
            "nearest-{kind} selector is ambiguous at distance {best_distance} mm"
        )));
    }
    let selected = candidates.remove(0).0;
    cleanup_shapes(session, candidates.into_iter().map(|item| item.0));
    Ok(selected)
}

pub(crate) fn select_at_extreme<'session>(
    session: &'session Session,
    shape: &Shape<'session>,
    shape_type: ShapeType,
    kind: &str,
    axis: CoordinateAxis,
    extremum: Extremum,
    tolerance: f64,
) -> Result<Vec<Shape<'session>>, ModelError> {
    if tolerance < 0.0 {
        return Err(ModelError::new(format!(
            "{kind} extremum tolerance must be nonnegative"
        )));
    }
    let bounds = session.bounds(shape)?;
    let target = axis.component(match extremum {
        Extremum::Minimum => bounds.min,
        Extremum::Maximum => bounds.max,
    });
    let count = session.subshape_count(shape, shape_type)?;
    let mut selected = Vec::new();
    for index in 0..count {
        let candidate = match session.subshape(shape, shape_type, index) {
            Ok(candidate) => candidate,
            Err(error) => {
                cleanup_shapes(session, selected);
                return Err(error.into());
            }
        };
        let center = match session.center_of_mass(&candidate) {
            Ok(center) => center,
            Err(error) => {
                let _ = session.remove(candidate);
                cleanup_shapes(session, selected);
                return Err(error.into());
            }
        };
        if (axis.component(center) - target).abs() <= tolerance {
            selected.push(candidate);
        } else {
            let _ = session.remove(candidate);
        }
    }
    if selected.is_empty() {
        return Err(ModelError::new(format!(
            "{kind} extremum selector found no matches"
        )));
    }
    Ok(selected)
}
