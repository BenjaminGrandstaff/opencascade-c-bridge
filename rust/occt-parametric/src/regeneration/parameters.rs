//! Collection of the parameters each feature operation, selector, and expression reads.

use super::*;

pub(crate) fn collect_operation_parameters<'a>(
    operation: &'a FeatureOperation,
    names: &mut HashSet<&'a str>,
) {
    match operation {
        FeatureOperation::SheetMetal { definition } => definition.collect_parameters(names),
        FeatureOperation::SheetMetalFlat { neutral_factor, .. } => {
            collect_scalar_parameters(neutral_factor, names)
        }
        FeatureOperation::Box { origin, size } => {
            collect_vector_parameters(origin, names);
            collect_vector_parameters(size, names);
        }
        FeatureOperation::Loft { sections, .. } => collect_loft_parameters(sections, names),
        FeatureOperation::ProfileLoft { .. } | FeatureOperation::PlanarRegion { .. } => {}
        FeatureOperation::Sweep { orientation, .. } => {
            if let SweepOrientation::Binormal { direction } = orientation {
                collect_vector_parameters(direction, names);
            }
        }
        FeatureOperation::Cylinder {
            origin,
            axis,
            radius,
            height,
        } => {
            collect_vector_parameters(origin, names);
            collect_vector_parameters(axis, names);
            collect_scalar_parameters(radius, names);
            collect_scalar_parameters(height, names);
        }
        FeatureOperation::Cone {
            origin,
            axis,
            base_radius,
            top_radius,
            height,
        } => {
            collect_vector_parameters(origin, names);
            collect_vector_parameters(axis, names);
            collect_scalar_parameters(base_radius, names);
            collect_scalar_parameters(top_radius, names);
            collect_scalar_parameters(height, names);
        }
        FeatureOperation::Sphere { center, radius } => {
            collect_vector_parameters(center, names);
            collect_scalar_parameters(radius, names);
        }
        FeatureOperation::SketchFace { sketch }
        | FeatureOperation::SketchWire { sketch }
        | FeatureOperation::SketchOpenWire { sketch } => sketch.collect_parameters(names),
        FeatureOperation::Translate { offset, .. } => collect_vector_parameters(offset, names),
        FeatureOperation::Extrude {
            direction, extent, ..
        } => {
            collect_vector_parameters(direction, names);
            if let ExtrudeExtent::UpToFace { face, .. } = extent {
                collect_face_selector_parameters(face, names);
            }
        }
        FeatureOperation::Rib {
            thickness,
            direction,
            profile_mode,
            ..
        } => {
            collect_scalar_parameters(thickness, names);
            collect_vector_parameters(direction, names);
            match profile_mode {
                RibProfileMode::Closed => {}
                RibProfileMode::OpenStrip { offset } => collect_vector_parameters(offset, names),
                RibProfileMode::OpenToNext {
                    direction,
                    maximum_length,
                } => {
                    collect_vector_parameters(direction, names);
                    collect_scalar_parameters(maximum_length, names);
                }
            }
        }
        FeatureOperation::Hole {
            position,
            axis,
            diameter,
            extent,
            bottom,
            finish,
            thread,
            ..
        } => {
            collect_vector_parameters(position, names);
            collect_vector_parameters(axis, names);
            collect_scalar_parameters(diameter, names);
            collect_hole_parameters(extent, bottom, finish, thread.as_deref(), names);
        }

        FeatureOperation::Thread {
            origin,
            axis,
            major_diameter,
            pitch,
            length,
            ..
        } => {
            collect_vector_parameters(origin, names);
            collect_vector_parameters(axis, names);
            collect_scalar_parameters(major_diameter, names);
            collect_scalar_parameters(pitch, names);
            collect_scalar_parameters(length, names);
        }
        FeatureOperation::Helix {
            origin,
            axis,
            start,
            radius,
            pitch,
            turns,
            ..
        } => {
            collect_vector_parameters(origin, names);
            collect_vector_parameters(axis, names);
            collect_vector_parameters(start, names);
            collect_scalar_parameters(radius, names);
            collect_scalar_parameters(pitch, names);
            collect_scalar_parameters(turns, names);
        }
        FeatureOperation::Mirror { origin, normal, .. } => {
            collect_vector_parameters(origin, names);
            collect_vector_parameters(normal, names);
        }
        FeatureOperation::CircularPattern {
            origin,
            axis,
            count,
            angle_step_radians,
            ..
        } => {
            collect_vector_parameters(origin, names);
            collect_vector_parameters(axis, names);
            collect_scalar_parameters(count, names);
            collect_scalar_parameters(angle_step_radians, names);
        }
        FeatureOperation::LinearPattern { step, count, .. } => {
            collect_vector_parameters(step, names);
            collect_scalar_parameters(count, names);
        }
        FeatureOperation::Scale { center, factor, .. } => {
            collect_vector_parameters(center, names);
            collect_scalar_parameters(factor, names);
        }
        FeatureOperation::Rotate {
            origin,
            axis,
            angle_radians,
            ..
        }
        | FeatureOperation::Revolve {
            origin,
            axis,
            angle_radians,
            ..
        } => {
            collect_vector_parameters(origin, names);
            collect_vector_parameters(axis, names);
            collect_scalar_parameters(angle_radians, names);
        }
        FeatureOperation::Fillet { edges, radius, .. } => {
            for selector in edges {
                collect_edge_selector_parameters(selector, names);
            }
            collect_scalar_parameters(radius, names);
        }
        FeatureOperation::VariableFillet {
            edges,
            start_radius,
            end_radius,
            stations,
            spine_direction,
            ..
        } => {
            for selector in edges {
                collect_edge_selector_parameters(selector, names);
            }
            collect_scalar_parameters(start_radius, names);
            collect_scalar_parameters(end_radius, names);
            collect_fillet_law_parameters(stations, spine_direction, names);
        }
        FeatureOperation::Chamfer {
            edges, distance, ..
        } => {
            for selector in edges {
                collect_edge_selector_parameters(selector, names);
            }
            collect_scalar_parameters(distance, names);
        }
        FeatureOperation::Hollow {
            faces,
            thickness,
            tolerance,
            ..
        } => {
            for selector in faces {
                collect_face_selector_parameters(selector, names);
            }
            collect_scalar_parameters(thickness, names);
            collect_scalar_parameters(tolerance, names);
        }
        FeatureOperation::Offset {
            distance,
            tolerance,
            ..
        } => {
            collect_scalar_parameters(distance, names);
            collect_scalar_parameters(tolerance, names);
        }
        FeatureOperation::Unify {
            linear_tolerance,
            angular_tolerance,
            ..
        } => {
            collect_scalar_parameters(linear_tolerance, names);
            collect_scalar_parameters(angular_tolerance, names);
        }
        FeatureOperation::Sew { tolerance, .. } => {
            collect_scalar_parameters(tolerance, names);
        }
        FeatureOperation::Draft {
            faces,
            neutral_origin,
            neutral_normal,
            pull_direction,
            angle_radians,
            ..
        } => {
            for selector in faces {
                collect_face_selector_parameters(selector, names);
            }
            collect_vector_parameters(neutral_origin, names);
            collect_vector_parameters(neutral_normal, names);
            collect_vector_parameters(pull_direction, names);
            collect_scalar_parameters(angle_radians, names);
        }
        FeatureOperation::Compound { .. } | FeatureOperation::MakeSolid { .. } => {}
        FeatureOperation::Fuse { .. }
        | FeatureOperation::Cut { .. }
        | FeatureOperation::Common { .. } => {}
    }
}

fn collect_fillet_law_parameters<'a>(
    stations: &'a [FilletRadiusStation],
    direction: &'a FilletSpineDirection,
    names: &mut HashSet<&'a str>,
) {
    for station in stations {
        collect_scalar_parameters(&station.position, names);
        collect_scalar_parameters(&station.radius, names);
    }
    if let FilletSpineDirection::FromPoint { point } = direction {
        collect_vector_parameters(point, names);
    }
}

pub(crate) fn collect_scalar_parameters<'a>(
    expression: &'a ScalarExpr,
    names: &mut HashSet<&'a str>,
) {
    match expression {
        ScalarExpr::Parameter(name) => {
            names.insert(name);
        }
        ScalarExpr::CarrLaneTapDrillV1 {
            nominal_diameter,
            pitch,
            ..
        } => {
            collect_scalar_parameters(nominal_diameter, names);
            collect_scalar_parameters(pitch, names);
        }
        ScalarExpr::Negate(value)
        | ScalarExpr::Absolute(value)
        | ScalarExpr::SquareRoot(value)
        | ScalarExpr::Sine(value)
        | ScalarExpr::Cosine(value)
        | ScalarExpr::Tangent(value)
        | ScalarExpr::ArcSine(value)
        | ScalarExpr::ArcCosine(value)
        | ScalarExpr::CarrLaneSocketHeadV1 {
            nominal_diameter: value,
            ..
        }
        | ScalarExpr::Iso273ClearanceV1 {
            nominal_diameter: value,
            ..
        } => {
            collect_scalar_parameters(value, names);
        }
        ScalarExpr::Add(left, right)
        | ScalarExpr::Subtract(left, right)
        | ScalarExpr::Multiply(left, right)
        | ScalarExpr::Divide(left, right)
        | ScalarExpr::Minimum(left, right)
        | ScalarExpr::Maximum(left, right)
        | ScalarExpr::Hypotenuse(left, right)
        | ScalarExpr::Power {
            base: left,
            exponent: right,
        }
        | ScalarExpr::ArcTangent2 { y: left, x: right }
        | ScalarExpr::RoundToStep {
            value: left,
            step: right,
            ..
        } => {
            collect_scalar_parameters(left, names);
            collect_scalar_parameters(right, names);
        }
        ScalarExpr::Interpolate { from, to, fraction } => {
            collect_scalar_parameters(from, names);
            collect_scalar_parameters(to, names);
            collect_scalar_parameters(fraction, names);
        }
        ScalarExpr::VectorLength(value) => collect_vector_parameters(value, names),
        ScalarExpr::DotProduct(left, right) => {
            collect_vector_parameters(left, names);
            collect_vector_parameters(right, names);
        }
        ScalarExpr::Clamp {
            value,
            minimum,
            maximum,
        } => {
            collect_scalar_parameters(value, names);
            collect_scalar_parameters(minimum, names);
            collect_scalar_parameters(maximum, names);
        }
        ScalarExpr::Conditional {
            left,
            right,
            when_true,
            when_false,
            ..
        } => {
            collect_scalar_parameters(left, names);
            collect_scalar_parameters(right, names);
            collect_scalar_parameters(when_true, names);
            collect_scalar_parameters(when_false, names);
        }
        ScalarExpr::Literal(_) => {}
    }
}

pub(crate) fn collect_vector_parameters<'a>(
    expression: &'a VectorExpr,
    names: &mut HashSet<&'a str>,
) {
    match expression {
        VectorExpr::Parameter(name) => {
            names.insert(name);
        }
        VectorExpr::Components { x, y, z } => {
            collect_scalar_parameters(x, names);
            collect_scalar_parameters(y, names);
            collect_scalar_parameters(z, names);
        }
        VectorExpr::Add(left, right) | VectorExpr::Subtract(left, right) => {
            collect_vector_parameters(left, names);
            collect_vector_parameters(right, names);
        }
        VectorExpr::Scale { vector, factor } => {
            collect_vector_parameters(vector, names);
            collect_scalar_parameters(factor, names);
        }
        VectorExpr::Normalize(vector) => collect_vector_parameters(vector, names),
        VectorExpr::Literal(_) => {}
    }
}

pub(crate) fn collect_edge_selector_parameters<'a>(
    selector: &'a EdgeSelector,
    names: &mut HashSet<&'a str>,
) {
    match selector {
        EdgeSelector::NearestCenter {
            target,
            maximum_distance,
        } => {
            collect_vector_parameters(target, names);
            collect_scalar_parameters(maximum_distance, names);
        }
        EdgeSelector::AtExtreme { tolerance, .. } => {
            collect_scalar_parameters(tolerance, names);
        }
        EdgeSelector::Longest {
            relative_tolerance, ..
        } => collect_scalar_parameters(relative_tolerance, names),
        EdgeSelector::CircularRadius { minimum, maximum }
        | EdgeSelector::CurvatureRadius { minimum, maximum }
        | EdgeSelector::CurvatureRadiusRange {
            minimum, maximum, ..
        } => {
            collect_scalar_parameters(minimum, names);
            collect_scalar_parameters(maximum, names);
        }
        EdgeSelector::CurvatureRadiusBounds {
            minimum,
            maximum,
            relative_tolerance,
            ..
        } => {
            collect_scalar_parameters(minimum, names);
            collect_scalar_parameters(maximum, names);
            collect_scalar_parameters(relative_tolerance, names);
        }
        EdgeSelector::Union(selectors) | EdgeSelector::Intersection(selectors) => {
            for selector in selectors {
                collect_edge_selector_parameters(selector, names);
            }
        }
        EdgeSelector::Difference { base, subtract } => {
            collect_edge_selector_parameters(base, names);
            collect_edge_selector_parameters(subtract, names);
        }
        EdgeSelector::History { source, .. } => collect_edge_selector_parameters(source, names),
        EdgeSelector::Persistent { select, .. } => collect_edge_selector_parameters(select, names),
        // A named reference's parameters join its users' signatures directly.
        EdgeSelector::Named(_) => {}
    }
}

pub(crate) fn collect_face_selector_parameters<'a>(
    selector: &'a FaceSelector,
    names: &mut HashSet<&'a str>,
) {
    match selector {
        FaceSelector::NearestCenter {
            target,
            maximum_distance,
        } => {
            collect_vector_parameters(target, names);
            collect_scalar_parameters(maximum_distance, names);
        }
        FaceSelector::AtExtreme { tolerance, .. } => {
            collect_scalar_parameters(tolerance, names);
        }
        FaceSelector::NormalAligned {
            direction,
            minimum_dot,
        } => {
            collect_vector_parameters(direction, names);
            collect_scalar_parameters(minimum_dot, names);
        }
        FaceSelector::LargestArea {
            relative_tolerance, ..
        } => collect_scalar_parameters(relative_tolerance, names),
        FaceSelector::AdjacentToEdges { edges, .. } => {
            collect_edge_selector_parameters(edges, names);
        }
        FaceSelector::TangentTo {
            faces,
            angular_tolerance,
            ..
        } => {
            collect_face_selector_parameters(faces, names);
            if let Some(tolerance) = angular_tolerance {
                collect_scalar_parameters(tolerance, names);
            }
        }
        FaceSelector::Union(selectors) | FaceSelector::Intersection(selectors) => {
            for selector in selectors {
                collect_face_selector_parameters(selector, names);
            }
        }
        FaceSelector::Difference { base, subtract } => {
            collect_face_selector_parameters(base, names);
            collect_face_selector_parameters(subtract, names);
        }
        FaceSelector::History { source, .. } => collect_face_selector_parameters(source, names),
        FaceSelector::Persistent { select, .. } => collect_face_selector_parameters(select, names),
        FaceSelector::Named(_) => {}
        FaceSelector::GeneratedFromEdges { source, .. } => {
            collect_edge_selector_parameters(source, names)
        }
    }
}

fn collect_hole_parameters<'a>(
    extent: &'a HoleExtent,
    bottom: &'a HoleBottom,
    finish: &'a HoleFinish,
    thread: Option<&'a ThreadSpecification>,
    names: &mut HashSet<&'a str>,
) {
    if let HoleBottom::DrillPoint { angle_radians } = bottom {
        collect_scalar_parameters(angle_radians, names);
    }
    if let Some(thread) = thread {
        collect_scalar_parameters(&thread.nominal_diameter, names);
        collect_scalar_parameters(&thread.pitch, names);
    }
    match extent {
        HoleExtent::Blind { depth } => collect_scalar_parameters(depth, names),
        HoleExtent::UpToFace { face } => collect_face_selector_parameters(face, names),
        _ => {}
    }
    match finish {
        HoleFinish::Plain => {}
        HoleFinish::Counterbore { diameter, depth } => {
            collect_scalar_parameters(diameter, names);
            collect_scalar_parameters(depth, names);
        }
        HoleFinish::Countersink {
            diameter,
            angle_radians,
        } => {
            collect_scalar_parameters(diameter, names);
            collect_scalar_parameters(angle_radians, names);
        }
    }
}
