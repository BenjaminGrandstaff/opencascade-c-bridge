//! Semantic edge and face selection.

use super::*;

/// Feature definitions by id and the family's named references, for selectors
/// that follow topology through the feature graph or use a reference by name.
#[derive(Default)]
pub(crate) struct Features<'a> {
    pub(crate) by_id: HashMap<&'a str, &'a FeatureDefinition>,
    pub(crate) references: References<'a>,
    /// The family's feature colors.
    pub(crate) colors: HashMap<&'a str, [f64; 3]>,
}

impl<'a> Features<'a> {
    pub(crate) fn new(family: &'a FamilyDefinition) -> Self {
        Self {
            by_id: family
                .features
                .iter()
                .map(|feature| (feature.id.as_str(), feature))
                .collect(),
            references: reference_map(family),
            colors: family
                .feature_colors
                .iter()
                .map(|(feature, color)| (feature.as_str(), *color))
                .collect(),
        }
    }

    fn reference(&self, name: &str, kind: ReferenceUse) -> Result<&'a ReferenceTarget, ModelError> {
        let reference = self
            .references
            .get(name)
            .ok_or_else(|| ModelError::new(format!("unknown named reference '{name}'")))?;
        if reference.target.kind() != kind {
            let expected = if kind == ReferenceUse::Faces {
                "faces"
            } else {
                "edges"
            };
            return Err(ModelError::new(format!(
                "named reference '{name}' does not name {expected}"
            )));
        }
        Ok(&reference.target)
    }
}

mod queries;
pub(crate) use queries::*;

pub(crate) fn resolve_edge_selectors<'session>(
    session: &'session Session,
    input: &Shape<'session>,
    selectors: &[EdgeSelector],
    parameters: &HashMap<String, ParameterValue>,
    shapes: &HashMap<String, Shape<'session>>,
    operation: &str,
    definitions: &Features<'_>,
) -> Result<(Vec<Shape<'session>>, Vec<usize>), ModelError> {
    if selectors.is_empty() {
        return Err(ModelError::new(format!(
            "{operation} requires at least one edge selector"
        )));
    }
    let mut selected = Vec::new();
    let mut sizes = Vec::with_capacity(selectors.len());
    for selector in selectors {
        match resolve_edge_selector(session, input, selector, parameters, shapes, definitions) {
            Ok(edges) => {
                sizes.push(edges.len());
                selected.extend(edges);
            }
            Err(error) => {
                cleanup_shapes(session, selected);
                return Err(error);
            }
        }
    }
    Ok((selected, sizes))
}

pub(crate) fn resolve_edge_selector<'session>(
    session: &'session Session,
    result: &Shape<'session>,
    selector: &EdgeSelector,
    parameters: &HashMap<String, ParameterValue>,
    shapes: &HashMap<String, Shape<'session>>,
    definitions: &Features<'_>,
) -> Result<Vec<Shape<'session>>, ModelError> {
    match selector {
        EdgeSelector::NearestCenter {
            target,
            maximum_distance,
        } => select_nearest_center(
            session,
            result,
            ShapeType::Edge,
            "edge",
            vector(target, parameters, Dimension::Length)?,
            scalar(maximum_distance, parameters, Dimension::Length)?,
        )
        .map(|shape| vec![shape]),
        EdgeSelector::AtExtreme {
            axis,
            extremum,
            tolerance,
        } => select_at_extreme(
            session,
            result,
            ShapeType::Edge,
            "edge",
            *axis,
            *extremum,
            scalar(tolerance, parameters, Dimension::Length)?,
        ),
        EdgeSelector::Longest {
            allow_ties,
            relative_tolerance,
        } => select_longest_edges(
            session,
            result,
            *allow_ties,
            scalar(relative_tolerance, parameters, Dimension::Scalar)?,
        ),
        EdgeSelector::CircularRadius { minimum, maximum } => select_circular_edges_by_radius(
            session,
            result,
            scalar(minimum, parameters, Dimension::Length)?,
            scalar(maximum, parameters, Dimension::Length)?,
        ),
        EdgeSelector::CurvatureRadius { minimum, maximum } => select_edges_by_curvature_radius(
            session,
            result,
            scalar(minimum, parameters, Dimension::Length)?,
            scalar(maximum, parameters, Dimension::Length)?,
        ),
        EdgeSelector::CurvatureRadiusRange {
            minimum,
            maximum,
            sample_count,
            require_entire_edge,
        } => select_edges_by_curvature_radius_range(
            session,
            result,
            scalar(minimum, parameters, Dimension::Length)?,
            scalar(maximum, parameters, Dimension::Length)?,
            *sample_count,
            *require_entire_edge,
        ),
        EdgeSelector::CurvatureRadiusBounds {
            minimum,
            maximum,
            relative_tolerance,
            require_entire_edge,
        } => select_edges_by_bounded_curvature_radius(
            session,
            result,
            scalar(minimum, parameters, Dimension::Length)?,
            scalar(maximum, parameters, Dimension::Length)?,
            scalar(relative_tolerance, parameters, Dimension::Scalar)?,
            *require_entire_edge,
        ),
        EdgeSelector::Union(selectors) => {
            let sets = resolve_edge_selector_sets(
                session,
                result,
                selectors,
                parameters,
                shapes,
                definitions,
            )?;
            compose_shape_sets(session, sets, ShapeSetOperation::Union)
        }
        EdgeSelector::Intersection(selectors) => {
            let sets = resolve_edge_selector_sets(
                session,
                result,
                selectors,
                parameters,
                shapes,
                definitions,
            )?;
            compose_shape_sets(session, sets, ShapeSetOperation::Intersection)
        }
        EdgeSelector::Difference { base, subtract } => {
            let base =
                resolve_edge_selector(session, result, base, parameters, shapes, definitions)?;
            let subtract = match resolve_edge_selector(
                session,
                result,
                subtract,
                parameters,
                shapes,
                definitions,
            ) {
                Ok(subtract) => subtract,
                Err(error) => {
                    cleanup_shapes(session, base);
                    return Err(error);
                }
            };
            compose_shape_sets(session, vec![base, subtract], ShapeSetOperation::Difference)
        }
        EdgeSelector::Named(name) => match definitions.reference(name, ReferenceUse::Edges)? {
            ReferenceTarget::Edges(selector) => {
                resolve_edge_selector(session, result, selector, parameters, shapes, definitions)
            }
            ReferenceTarget::Faces(_) => unreachable!("kind checked by reference()"),
        },
        EdgeSelector::Persistent { feature, select } => {
            let origin = shape(shapes, feature)?;
            let chosen =
                resolve_edge_selector(session, origin, select, parameters, shapes, definitions)?;
            follow_forward(
                session,
                result,
                chosen,
                feature,
                shapes,
                definitions,
                ShapeType::Edge,
            )
        }
        EdgeSelector::History {
            source_feature,
            source,
            relation,
        } => {
            let source_result = shape(shapes, source_feature)?;
            let source_edges = resolve_edge_selector(
                session,
                source_result,
                source,
                parameters,
                shapes,
                definitions,
            )?;
            resolve_history(
                session,
                result,
                source_edges,
                (*relation).into(),
                source_feature,
                "edges",
            )
        }
    }
}

pub(crate) fn resolve_face_selector<'session>(
    session: &'session Session,
    result: &Shape<'session>,
    selector: &FaceSelector,
    parameters: &HashMap<String, ParameterValue>,
    shapes: &HashMap<String, Shape<'session>>,
    definitions: &Features<'_>,
) -> Result<Vec<Shape<'session>>, ModelError> {
    match selector {
        FaceSelector::NearestCenter {
            target,
            maximum_distance,
        } => select_nearest_center(
            session,
            result,
            ShapeType::Face,
            "face",
            vector(target, parameters, Dimension::Length)?,
            scalar(maximum_distance, parameters, Dimension::Length)?,
        )
        .map(|shape| vec![shape]),
        FaceSelector::AtExtreme {
            axis,
            extremum,
            tolerance,
        } => select_at_extreme(
            session,
            result,
            ShapeType::Face,
            "face",
            *axis,
            *extremum,
            scalar(tolerance, parameters, Dimension::Length)?,
        ),
        FaceSelector::NormalAligned {
            direction,
            minimum_dot,
        } => select_faces_by_normal(
            session,
            result,
            vector(direction, parameters, Dimension::Scalar)?,
            scalar(minimum_dot, parameters, Dimension::Scalar)?,
        ),
        FaceSelector::LargestArea {
            planar_only,
            allow_ties,
            relative_tolerance,
        } => select_largest_faces(
            session,
            result,
            *planar_only,
            *allow_ties,
            scalar(relative_tolerance, parameters, Dimension::Scalar)?,
        ),
        FaceSelector::AdjacentToEdges {
            edges,
            minimum_count,
        } => {
            let edges =
                resolve_edge_selector(session, result, edges, parameters, shapes, definitions)?;
            select_faces_adjacent_to_edges(session, result, edges, *minimum_count)
        }
        FaceSelector::TangentTo {
            faces,
            minimum_count,
            angular_tolerance,
        } => {
            let angular_tolerance = angular_tolerance
                .as_ref()
                .map(|tolerance| scalar(tolerance, parameters, Dimension::Scalar))
                .transpose()?;
            if angular_tolerance.is_some_and(|tolerance| {
                !(tolerance > 0.0 && tolerance < std::f64::consts::FRAC_PI_2)
            }) {
                return Err(ModelError::new(
                    "face tangency angular tolerance must be in (0, pi/2) radians",
                ));
            }
            let faces =
                resolve_face_selector(session, result, faces, parameters, shapes, definitions)?;
            select_faces_tangent_to_faces(session, result, faces, *minimum_count, angular_tolerance)
        }
        FaceSelector::Union(selectors) => {
            let sets = resolve_face_selector_sets(
                session,
                result,
                selectors,
                parameters,
                shapes,
                definitions,
            )?;
            compose_shape_sets(session, sets, ShapeSetOperation::Union)
        }
        FaceSelector::Intersection(selectors) => {
            let sets = resolve_face_selector_sets(
                session,
                result,
                selectors,
                parameters,
                shapes,
                definitions,
            )?;
            compose_shape_sets(session, sets, ShapeSetOperation::Intersection)
        }
        FaceSelector::Difference { base, subtract } => {
            let base =
                resolve_face_selector(session, result, base, parameters, shapes, definitions)?;
            let subtract = match resolve_face_selector(
                session,
                result,
                subtract,
                parameters,
                shapes,
                definitions,
            ) {
                Ok(subtract) => subtract,
                Err(error) => {
                    cleanup_shapes(session, base);
                    return Err(error);
                }
            };
            compose_shape_sets(session, vec![base, subtract], ShapeSetOperation::Difference)
        }
        FaceSelector::Named(name) => match definitions.reference(name, ReferenceUse::Faces)? {
            ReferenceTarget::Faces(selector) => {
                resolve_face_selector(session, result, selector, parameters, shapes, definitions)
            }
            ReferenceTarget::Edges(_) => unreachable!("kind checked by reference()"),
        },
        FaceSelector::Persistent { feature, select } => {
            let origin = shape(shapes, feature)?;
            let chosen =
                resolve_face_selector(session, origin, select, parameters, shapes, definitions)?;
            follow_forward(
                session,
                result,
                chosen,
                feature,
                shapes,
                definitions,
                ShapeType::Face,
            )
        }
        FaceSelector::History {
            source_feature,
            source,
            relation,
        } => {
            let source_result = shape(shapes, source_feature)?;
            let source_faces = resolve_face_selector(
                session,
                source_result,
                source,
                parameters,
                shapes,
                definitions,
            )?;
            resolve_history(
                session,
                result,
                source_faces,
                (*relation).into(),
                source_feature,
                "faces",
            )
        }
        FaceSelector::GeneratedFromEdges {
            source_feature,
            source,
        } => {
            let source_result = shape(shapes, source_feature)?;
            let edges = resolve_edge_selector(
                session,
                source_result,
                source,
                parameters,
                shapes,
                definitions,
            )?;
            generated_faces_from_edges(session, result, edges, source_feature)
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum ShapeSetOperation {
    Union,
    Intersection,
    Difference,
}

pub(crate) fn resolve_edge_selector_sets<'session>(
    session: &'session Session,
    result: &Shape<'session>,
    selectors: &[EdgeSelector],
    parameters: &HashMap<String, ParameterValue>,
    shapes: &HashMap<String, Shape<'session>>,
    definitions: &Features<'_>,
) -> Result<Vec<Vec<Shape<'session>>>, ModelError> {
    let mut sets = Vec::new();
    for selector in selectors {
        match resolve_edge_selector(session, result, selector, parameters, shapes, definitions) {
            Ok(set) => sets.push(set),
            Err(error) => {
                cleanup_shapes(session, sets.into_iter().flatten());
                return Err(error);
            }
        }
    }
    Ok(sets)
}

pub(crate) fn resolve_face_selector_sets<'session>(
    session: &'session Session,
    result: &Shape<'session>,
    selectors: &[FaceSelector],
    parameters: &HashMap<String, ParameterValue>,
    shapes: &HashMap<String, Shape<'session>>,
    definitions: &Features<'_>,
) -> Result<Vec<Vec<Shape<'session>>>, ModelError> {
    let mut sets = Vec::new();
    for selector in selectors {
        match resolve_face_selector(session, result, selector, parameters, shapes, definitions) {
            Ok(set) => sets.push(set),
            Err(error) => {
                cleanup_shapes(session, sets.into_iter().flatten());
                return Err(error);
            }
        }
    }
    Ok(sets)
}

pub(crate) fn shape_set_contains(
    session: &Session,
    set: &[Shape<'_>],
    candidate: &Shape<'_>,
) -> Result<bool, ModelError> {
    for item in set {
        if session.is_same(item, candidate)? {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(crate) fn cleanup_shape_sets<'session>(
    session: &'session Session,
    sets: Vec<Vec<Shape<'session>>>,
) {
    cleanup_shapes(session, sets.into_iter().flatten());
}

pub(crate) fn compose_shape_sets<'session>(
    session: &'session Session,
    sets: Vec<Vec<Shape<'session>>>,
    operation: ShapeSetOperation,
) -> Result<Vec<Shape<'session>>, ModelError> {
    if sets.is_empty() || matches!(operation, ShapeSetOperation::Difference) && sets.len() != 2 {
        cleanup_shape_sets(session, sets);
        return Err(ModelError::new(
            "selector composition requires at least one selector",
        ));
    }
    let result = match operation {
        ShapeSetOperation::Union => union_shapes(session, sets)?,
        ShapeSetOperation::Intersection => intersect_shapes(session, sets)?,
        ShapeSetOperation::Difference => {
            let mut sets = sets.into_iter();
            let base = sets.next().unwrap_or_default();
            let subtract = sets.next().unwrap_or_default();
            let difference = filter_by_membership(session, base, &subtract, false);
            cleanup_shapes(session, subtract);
            difference?
        }
    };
    if result.is_empty() {
        return Err(ModelError::new("selector composition found no matches"));
    }
    Ok(result)
}

/// Keeps the first occurrence of each topologically distinct shape, in order.
pub(crate) fn union_shapes<'session>(
    session: &'session Session,
    sets: Vec<Vec<Shape<'session>>>,
) -> Result<Vec<Shape<'session>>, ModelError> {
    let mut union = Vec::new();
    let mut pending = sets.into_iter().flatten();
    while let Some(candidate) = pending.next() {
        match shape_set_contains(session, &union, &candidate) {
            Ok(true) => {
                let _ = session.remove(candidate);
            }
            Ok(false) => union.push(candidate),
            Err(error) => {
                let _ = session.remove(candidate);
                cleanup_shapes(session, pending);
                cleanup_shapes(session, union);
                return Err(error);
            }
        }
    }
    Ok(union)
}

pub(crate) fn intersect_shapes<'session>(
    session: &'session Session,
    sets: Vec<Vec<Shape<'session>>>,
) -> Result<Vec<Shape<'session>>, ModelError> {
    let mut sets = sets.into_iter();
    let mut intersection = sets.next().unwrap_or_default();
    for set in sets.by_ref() {
        let kept = filter_by_membership(session, intersection, &set, true);
        cleanup_shapes(session, set);
        match kept {
            Ok(kept) => intersection = kept,
            Err(error) => {
                cleanup_shapes(session, sets.flatten());
                return Err(error);
            }
        }
    }
    Ok(intersection)
}

/// Keeps candidates whose membership in `reference` equals `keep_members`,
/// in order, releasing the rest. On error every candidate handle is released.
pub(crate) fn filter_by_membership<'session>(
    session: &'session Session,
    candidates: Vec<Shape<'session>>,
    reference: &[Shape<'session>],
    keep_members: bool,
) -> Result<Vec<Shape<'session>>, ModelError> {
    let mut kept = Vec::new();
    let mut pending = candidates.into_iter();
    while let Some(candidate) = pending.next() {
        match shape_set_contains(session, reference, &candidate) {
            Ok(member) if member == keep_members => kept.push(candidate),
            Ok(_) => {
                let _ = session.remove(candidate);
            }
            Err(error) => {
                let _ = session.remove(candidate);
                cleanup_shapes(session, pending);
                cleanup_shapes(session, kept);
                return Err(error);
            }
        }
    }
    Ok(kept)
}

impl PartInstance<'_> {
    /// Resolve a semantic face query against a result generated from this
    /// instance's current definition/parameters. Returned handles are owned;
    /// dropping them releases query temporaries. Topology indices are not
    /// persistent identities across regeneration.
    pub fn select_faces<'session>(
        &self,
        session: &'session Session,
        generated: &GeneratedResult<'session>,
        output: &str,
        selector: &FaceSelector,
    ) -> Result<Vec<Shape<'session>>, ModelError> {
        let shape = generated
            .shape(output)
            .ok_or_else(|| ModelError::new(format!("unknown generated output '{output}'")))?;
        resolve_face_selector(
            session,
            shape,
            selector,
            &self.resolved_parameters()?,
            &generated.shapes,
            &Features::new(self.definition),
        )
    }

    /// Resolve a semantic edge query against a result generated from this
    /// instance's current definition/parameters. Returned handles are owned.
    pub fn select_edges<'session>(
        &self,
        session: &'session Session,
        generated: &GeneratedResult<'session>,
        output: &str,
        selector: &EdgeSelector,
    ) -> Result<Vec<Shape<'session>>, ModelError> {
        let shape = generated
            .shape(output)
            .ok_or_else(|| ModelError::new(format!("unknown generated output '{output}'")))?;
        resolve_edge_selector(
            session,
            shape,
            selector,
            &self.resolved_parameters()?,
            &generated.shapes,
            &Features::new(self.definition),
        )
    }
}
