//! Cylindrical hole construction using the existing cylinder and boolean APIs.

use super::*;
use crate::assembly::{add, dot, scale, subtract, unit};

pub(super) struct HoleSpec<'a> {
    pub position: &'a VectorExpr,
    pub axis: &'a VectorExpr,
    pub diameter: &'a ScalarExpr,
    pub extent: &'a HoleExtent,
    pub bottom: &'a HoleBottom,
    pub finish: &'a HoleFinish,
    pub thread: Option<&'a ThreadSpecification>,
}

pub(super) fn execute_hole<'session>(
    session: &'session Session,
    input: &Shape<'_>,
    parameters: &HashMap<String, ParameterValue>,
    spec: HoleSpec<'_>,
) -> Result<Shape<'session>, ModelError> {
    let HoleSpec {
        position,
        axis,
        diameter,
        extent,
        bottom,
        finish,
        thread,
    } = spec;
    if session.subshape_count(input, ShapeType::Solid)? != 1 || !session.is_valid(input)? {
        return Err(ModelError::new("hole input must contain one valid solid"));
    }
    let position = vector(position, parameters, Dimension::Length)?;
    let axis = unit(vector(axis, parameters, Dimension::Scalar)?)?;
    let diameter = scalar(diameter, parameters, Dimension::Length)?;
    if diameter <= 0.0 {
        return Err(ModelError::new("hole diameter must be positive"));
    }
    if let Some(thread) = thread {
        validate_thread(thread, diameter, parameters)?;
    }
    let radius = diameter / 2.0;
    let (start, depth) = match extent {
        HoleExtent::Blind { depth } => (position, scalar(depth, parameters, Dimension::Length)?),
        HoleExtent::ThroughAll => through_span(session, input, position, axis, radius)?,
    };
    if !depth.is_finite() || depth <= 0.0 {
        return Err(ModelError::new("hole depth must be finite and positive"));
    }
    let tool = session.create_cylinder(start, axis, radius, depth)?;
    let tool = if let HoleBottom::DrillPoint { angle_radians } = bottom {
        if !matches!(extent, HoleExtent::Blind { .. }) {
            return Err(ModelError::new("a drill point requires a blind hole"));
        }
        let angle = scalar(angle_radians, parameters, Dimension::Scalar)?;
        if angle <= 0.0 || angle >= std::f64::consts::PI {
            return Err(ModelError::new(
                "drill point included angle must be between zero and pi radians",
            ));
        }
        let tip_depth = radius / (angle / 2.0).tan();
        if !tip_depth.is_finite() || tip_depth <= 0.0 {
            return Err(ModelError::new(
                "drill point depth must be finite and positive",
            ));
        }
        let tip =
            session.create_cone(add(start, scale(axis, depth)), axis, radius, 0.0, tip_depth)?;
        let tip_volume = session.volume(&tip)?;
        let inside_volume = session.overlap_volume(input, &tip)?;
        if !tip_volume.is_finite() || !inside_volume.is_finite() || tip_volume <= 0.0 {
            return Err(ModelError::new(
                "drill point must have finite positive volume",
            ));
        }
        if (tip_volume - inside_volume).abs() > tip_volume * 1e-9 {
            return Err(ModelError::new(
                "blind drill point breaks out of the input material",
            ));
        }
        session.fuse(&tool, &tip)?
    } else {
        tool
    };
    let recess = match finish {
        HoleFinish::Plain => None,
        HoleFinish::Counterbore {
            diameter,
            depth: recess_depth,
        } => {
            let recess_radius = scalar(diameter, parameters, Dimension::Length)? / 2.0;
            let recess_depth = scalar(recess_depth, parameters, Dimension::Length)?;
            check_recess(radius, recess_radius, recess_depth, extent, depth)?;
            Some(session.create_cylinder(position, axis, recess_radius, recess_depth)?)
        }
        HoleFinish::Countersink {
            diameter,
            angle_radians,
        } => {
            let recess_radius = scalar(diameter, parameters, Dimension::Length)? / 2.0;
            let angle = scalar(angle_radians, parameters, Dimension::Scalar)?;
            if angle <= 0.0 || angle >= std::f64::consts::PI {
                return Err(ModelError::new(
                    "countersink included angle must be between zero and pi radians",
                ));
            }
            let recess_depth = (recess_radius - radius) / (angle / 2.0).tan();
            check_recess(radius, recess_radius, recess_depth, extent, depth)?;
            Some(session.create_cone(position, axis, recess_radius, radius, recess_depth)?)
        }
    };
    // Fuse overlapping cutters so validation never sees an overlapping-solid
    // compound. One cut records history directly from the original input.
    let tool = if let Some(recess) = recess {
        session.fuse(&tool, &recess)?
    } else {
        tool
    };
    let volume_before = session.volume(input)?;
    let result = session.cut(input, &tool)?;
    validate_hole_result(session, &result, volume_before)?;
    // The cut retains history; dropping the cylindrical tool releases only its
    // temporary handle. Preserve the kernel's result wrapper rather than
    // extracting a solid subshape, which would discard the recorded history.
    Ok(result)
}

fn check_recess(
    radius: f64,
    recess_radius: f64,
    recess_depth: f64,
    extent: &HoleExtent,
    bore_depth: f64,
) -> Result<(), ModelError> {
    if recess_radius <= radius {
        return Err(ModelError::new(
            "hole recess diameter must exceed bore diameter",
        ));
    }
    if !recess_depth.is_finite() || recess_depth <= 0.0 {
        return Err(ModelError::new(
            "hole recess depth must be finite and positive",
        ));
    }
    if matches!(extent, HoleExtent::Blind { .. }) && recess_depth >= bore_depth {
        return Err(ModelError::new(
            "hole recess must be shallower than blind bore depth",
        ));
    }
    Ok(())
}

fn through_span(
    session: &Session,
    input: &Shape<'_>,
    position: Vec3,
    axis: Vec3,
    radius: f64,
) -> Result<(Vec3, f64), ModelError> {
    let bounds = session.exact_bounds(input)?;
    let mut minimum = f64::INFINITY;
    let mut maximum = f64::NEG_INFINITY;
    for x in [bounds.min.x, bounds.max.x] {
        for y in [bounds.min.y, bounds.max.y] {
            for z in [bounds.min.z, bounds.max.z] {
                let projection = dot(subtract(Vec3::new(x, y, z), position), axis);
                minimum = minimum.min(projection);
                maximum = maximum.max(projection);
            }
        }
    }
    // Extend beyond both bounds to avoid coincident caps. Padding scales with
    // the part's projected span/radius and has a 1e-6 mm minimum.
    let padding = (maximum - minimum).max(radius).max(1.0) * 1e-6;
    Ok((
        add(position, scale(axis, minimum - padding)),
        maximum - minimum + 2.0 * padding,
    ))
}

fn validate_thread(
    thread: &ThreadSpecification,
    diameter: f64,
    parameters: &HashMap<String, ParameterValue>,
) -> Result<(), ModelError> {
    if thread.designation.trim().is_empty() {
        return Err(ModelError::new("thread designation must not be blank"));
    }
    let nominal = scalar(&thread.nominal_diameter, parameters, Dimension::Length)?;
    let pitch = scalar(&thread.pitch, parameters, Dimension::Length)?;
    if nominal <= diameter {
        return Err(ModelError::new(
            "thread nominal diameter must exceed bore diameter",
        ));
    }
    if pitch <= 0.0 {
        return Err(ModelError::new("thread pitch must be positive"));
    }
    Ok(())
}

fn validate_hole_result(
    session: &Session,
    result: &Shape<'_>,
    volume_before: f64,
) -> Result<(), ModelError> {
    let volume_after = session.volume(result)?;
    if session.subshape_count(result, ShapeType::Solid)? != 1
        || !session.is_valid(result)?
        || volume_after <= 0.0
    {
        return Err(ModelError::new(
            "hole must leave one valid solid with positive volume",
        ));
    }
    if volume_after >= volume_before {
        return Err(ModelError::new(
            "hole does not remove material from its input",
        ));
    }
    Ok(())
}
