//! Lofts through planar outlines placed by parameter expressions.

use super::*;
use crate::assembly::{add, cross, dot, length, scale};

const MAXIMUM_SECTIONS: usize = 1_000;
const MAXIMUM_POINTS: usize = 10_000;

fn unit_vector(value: Vec3, name: &str) -> Result<Vec3, ModelError> {
    let size = length(value);
    if !(size.is_finite() && size > 0.0) {
        return Err(ModelError::new(format!(
            "loft section {name} must be nonzero"
        )));
    }
    Ok(scale(value, 1.0 / size))
}

fn profile_frame(
    session: &Session,
    shapes: &HashMap<String, Shape<'_>>,
    id: &str,
) -> Result<(Vec3, Vec3), ModelError> {
    let source = shape(shapes, id)?;
    let face = match session.shape_type(source)? {
        ShapeType::Wire => session.create_face_from_wire(source)?,
        ShapeType::Face => session.subshape(source, ShapeType::Face, 0)?,
        _ => {
            return Err(ModelError::new(
                "loft end profiles must be planar faces or closed wires",
            ));
        }
    };
    if !session.face_is_planar(&face)? || session.subshape_count(&face, ShapeType::Wire)? != 1 {
        return Err(ModelError::new(
            "loft end profiles must be single-boundary planar sections",
        ));
    }
    Ok((session.face_normal(&face)?, session.center_of_mass(&face)?))
}

/// A resolved end condition in family millimetres, using the regeneration's
/// parameters and geometry. The derivative follows increasing station order.
#[derive(Clone, Debug)]
pub struct ResolvedLoftEndTangent {
    pub end: bool,
    pub profile: String,
    pub origin: Vec3,
    pub derivative: Vec3,
    pub normal_to_profile: bool,
}

/// Exact start and a short native curve segment in boundary traversal order.
#[derive(Clone, Debug)]
pub struct ResolvedLoftProfileSeam {
    pub profile: String,
    pub station: usize,
    /// Zero-based hole track, or None for the outer track.
    pub hole: Option<usize>,
    pub reverse: bool,
    pub points: Vec<Vec3>,
}

impl GeneratedResult<'_> {
    /// Inspect the correspondence actually used by a constrained smooth loft.
    /// Supply the operation and resolved parameters used for this generation.
    /// Free/ruled lofts have automatic compatibility and publish no manual seams.
    pub fn loft_profile_seams(
        &self,
        session: &Session,
        operation: &FeatureOperation,
        parameters: &HashMap<String, ParameterValue>,
    ) -> Result<Vec<ResolvedLoftProfileSeam>, ModelError> {
        let FeatureOperation::ProfileLoft {
            profiles,
            holes,
            ruled,
            start_constraint,
            end_constraint,
            correspondence,
        } = operation
        else {
            return Ok(Vec::new());
        };
        if *ruled || start_constraint.is_none() && end_constraint.is_none() {
            return Ok(Vec::new());
        }
        let resolved = resolve_correspondence(correspondence, parameters)?;
        let mut seams = Vec::new();
        for (track, ids) in std::iter::once(profiles).chain(holes.iter()).enumerate() {
            for (station, id) in ids.iter().enumerate() {
                let source = shape(&self.shapes, id)?;
                let extracted;
                let wire = match session.shape_type(source)? {
                    ShapeType::Wire => source,
                    ShapeType::Face if session.subshape_count(source, ShapeType::Wire)? == 1 => {
                        extracted = session.subshape(source, ShapeType::Wire, 0)?;
                        &extracted
                    }
                    _ => {
                        return Err(ModelError::new(
                            "loft seam inspection requires single-boundary profiles",
                        ));
                    }
                };
                let (anchor, reverse) = resolved.get(id).copied().unwrap_or((None, false));
                // This shares the construction operation, including traversal and exact periodic seams.
                let aligned = session.align_profile_wire(wire, anchor, reverse)?;
                let edge = session.subshape(&aligned, ShapeType::Edge, 0)?;
                let points = (0..=4)
                    .map(|i| session.curve_point_at_fraction(&edge, f64::from(i) * 0.02))
                    .collect::<Result<Vec<_>, _>>()?;
                seams.push(ResolvedLoftProfileSeam {
                    profile: id.clone(),
                    station,
                    hole: track.checked_sub(1),
                    reverse,
                    points,
                });
            }
        }
        Ok(seams)
    }

    /// Inspect terminal conditions with the same resolver used to construct them.
    /// `operation` and `parameters` must be those used for this generation.
    pub fn loft_end_tangents(
        &self,
        session: &Session,
        operation: &FeatureOperation,
        parameters: &HashMap<String, ParameterValue>,
    ) -> Result<Vec<ResolvedLoftEndTangent>, ModelError> {
        let FeatureOperation::ProfileLoft {
            profiles,
            start_constraint,
            end_constraint,
            ..
        } = operation
        else {
            return Ok(Vec::new());
        };
        if profiles.len() < 2 {
            return Err(ModelError::new(
                "loft tangent inspection needs at least two profiles",
            ));
        }
        let mut result = Vec::with_capacity(2);
        for (end, c) in [
            (false, start_constraint.as_deref()),
            (true, end_constraint.as_deref()),
        ] {
            if let Some(derivative) =
                end_derivative(session, profiles, &self.shapes, parameters, c, end)?
            {
                let profile = profiles[if end { profiles.len() - 1 } else { 0 }].clone();
                let (_, origin) = profile_frame(session, &self.shapes, &profile)?;
                result.push(ResolvedLoftEndTangent {
                    end,
                    profile,
                    origin,
                    derivative,
                    normal_to_profile: c.is_some_and(|c| c.normal_to_profile),
                });
            }
        }
        Ok(result)
    }
}

/// Resolve either an explicit derivative direction or the actual terminal plane normal.
/// Both directions follow increasing station order, independent of wire orientation.
pub(crate) fn end_derivative(
    session: &Session,
    profiles: &[String],
    shapes: &HashMap<String, Shape<'_>>,
    parameters: &HashMap<String, ParameterValue>,
    constraint: Option<&LoftEndConstraint>,
    end: bool,
) -> Result<Option<Vec3>, ModelError> {
    let Some(c) = constraint else { return Ok(None) };
    let direction = match (c.normal_to_profile, c.direction.as_ref()) {
        (false, Some(direction)) => vector(direction, parameters, Dimension::Scalar)?,
        (true, None) => {
            if profiles.len() < 2 {
                return Err(ModelError::new(
                    "normal-to-profile loft constraints need at least two sections",
                ));
            }
            let i = if end { profiles.len() - 1 } else { 0 };
            let j = if end { i - 1 } else { 1 };
            let (normal, center) = profile_frame(session, shapes, &profiles[i])?;
            let (_, neighbor) = profile_frame(session, shapes, &profiles[j])?;
            let advance = if end {
                crate::assembly::subtract(center, neighbor)
            } else {
                crate::assembly::subtract(neighbor, center)
            };
            let projection = dot(normal, advance);
            if !projection.is_finite() || projection.abs() <= 1e-9 * length(advance).max(1.0) {
                return Err(ModelError::new(
                    "profile normal has no unambiguous forward direction toward its adjacent station",
                ));
            }
            scale(normal, if projection > 0.0 { 1.0 } else { -1.0 })
        }
        _ => {
            return Err(ModelError::new(
                "loft end constraint requires exactly one of direction or normal_to_profile",
            ));
        }
    };
    let magnitude = scalar(&c.magnitude, parameters, Dimension::Length)?;
    if !magnitude.is_finite() || magnitude <= 0.0 {
        return Err(ModelError::new(
            "loft end constraint strength must be a positive finite length",
        ));
    }
    Ok(Some(scale(
        unit_vector(direction, "end direction")?,
        magnitude,
    )))
}

/// Model-space points of one section: each profile point is rotated about
/// `pivot`, scaled, and laid out on the section's axes from its origin.
fn place(
    section: &LoftSection,
    parameters: &HashMap<String, ParameterValue>,
) -> Result<Vec<Vec3>, ModelError> {
    let origin = vector(&section.origin, parameters, Dimension::Length)?;
    let x_axis = unit_vector(
        vector(&section.x_axis, parameters, Dimension::Scalar)?,
        "x axis",
    )?;
    let y_axis = unit_vector(
        vector(&section.y_axis, parameters, Dimension::Scalar)?,
        "y axis",
    )?;
    if dot(x_axis, y_axis).abs() > 1e-9 || length(cross(x_axis, y_axis)) < 0.5 {
        return Err(ModelError::new("loft section axes must be perpendicular"));
    }
    let size = scalar(&section.scale, parameters, Dimension::Length)?;
    if !(size.is_finite() && size > 0.0) {
        return Err(ModelError::new(
            "loft section scale must be a positive length",
        ));
    }
    let angle = section
        .rotation_radians
        .as_ref()
        .map(|angle| scalar(angle, parameters, Dimension::Scalar))
        .transpose()?
        .unwrap_or(0.0);
    if !angle.is_finite() {
        return Err(ModelError::new("loft section rotation must be finite"));
    }
    let (sine, cosine) = angle.sin_cos();
    let [pivot_u, pivot_v] = section.pivot;
    section
        .profile
        .iter()
        .map(|&[u, v]| {
            if !(u.is_finite() && v.is_finite()) {
                return Err(ModelError::new("loft profile points must be finite"));
            }
            let (du, dv) = (u - pivot_u, v - pivot_v);
            let u = pivot_u + du * cosine - dv * sine;
            let v = pivot_v + du * sine + dv * cosine;
            Ok(add(
                origin,
                add(scale(x_axis, u * size), scale(y_axis, v * size)),
            ))
        })
        .collect()
}

/// O(sections x points) placement, then one kernel loft into a solid.
pub(crate) fn execute<'session>(
    session: &'session Session,
    sections: &[LoftSection],
    smooth: bool,
    ruled: bool,
    parameters: &HashMap<String, ParameterValue>,
) -> Result<Shape<'session>, ModelError> {
    let points = sections.first().map_or(0, |section| section.profile.len());
    if !(2..=MAXIMUM_SECTIONS).contains(&sections.len())
        || !(3..=MAXIMUM_POINTS).contains(&points)
        || sections
            .iter()
            .any(|section| section.profile.len() != points)
    {
        return Err(ModelError::new(format!(
            "a loft needs 2-{MAXIMUM_SECTIONS} sections with the same number of \
             profile points, 3-{MAXIMUM_POINTS}"
        )));
    }
    if sections
        .iter()
        .any(|section| section.pivot.iter().any(|value| !value.is_finite()))
    {
        return Err(ModelError::new("loft section pivots must be finite"));
    }
    let placed = sections
        .iter()
        .map(|section| place(section, parameters))
        .collect::<Result<Vec<_>, _>>()?;
    let slices = placed.iter().map(Vec::as_slice).collect::<Vec<_>>();
    Ok(if smooth {
        session.create_spline_loft(&slices, true, ruled)?
    } else {
        session.create_loft(&slices, true, ruled)?
    })
}

type Correspondence = HashMap<String, (Option<Vec3>, bool)>;

pub(crate) fn resolve_correspondence(
    values: &std::collections::BTreeMap<String, LoftProfileAlignment>,
    parameters: &HashMap<String, ParameterValue>,
) -> Result<Correspondence, ModelError> {
    values
        .iter()
        .map(|(id, value)| {
            Ok((
                id.clone(),
                (
                    value
                        .start_point
                        .as_ref()
                        .map(|point| vector(point, parameters, Dimension::Length))
                        .transpose()?,
                    value.reverse,
                ),
            ))
        })
        .collect()
}

/// Linear profile/topology indexing; native curve compatibility and surface
/// interpolation cost depends on edge correspondence, sections and degree.
#[allow(clippy::too_many_arguments)]
pub(crate) fn execute_profiles_constrained<'a>(
    session: &'a Session,
    profiles: &[String],
    holes: &[Vec<String>],
    ruled: bool,
    shapes: &HashMap<String, Shape<'a>>,
    correspondence: &Correspondence,
    start: Option<Vec3>,
    end: Option<Vec3>,
) -> Result<Shape<'a>, ModelError> {
    let constrained = start.is_some() || end.is_some();
    if !correspondence.is_empty() && (!constrained || ruled) {
        return Err(ModelError::new(
            "manual loft correspondence requires a smooth loft with an end constraint",
        ));
    }
    if correspondence.keys().any(|id| {
        !profiles
            .iter()
            .chain(holes.iter().flatten())
            .any(|p| p == id)
    }) {
        return Err(ModelError::new(
            "loft correspondence references a profile outside its station tracks",
        ));
    }
    if constrained && ruled {
        return Err(ModelError::new(
            "loft end constraints require smooth surfaces; ruled lofts cannot enforce end derivatives",
        ));
    }
    if constrained && profiles.len() > 64 {
        return Err(ModelError::new(
            "constrained loft supports at most 64 stations",
        ));
    }
    if holes.is_empty() {
        return execute_simple_profiles_constrained(
            session,
            profiles,
            ruled,
            shapes,
            correspondence,
            start,
            end,
        );
    }
    if !(2..=MAXIMUM_SECTIONS).contains(&profiles.len())
        || holes.len() > 100
        || holes.iter().any(|track| track.len() != profiles.len())
    {
        return Err(ModelError::new(
            "loft needs 2-1000 outer sections and at most 100 hole tracks with matching section counts",
        ));
    }
    let mut used = HashSet::new();
    if profiles
        .iter()
        .chain(holes.iter().flatten())
        .any(|id| !used.insert(id))
    {
        return Err(ModelError::new(
            "loft sections must be distinct across outer and hole tracks",
        ));
    }
    for (index, outer) in profiles.iter().enumerate() {
        let inner = holes
            .iter()
            .map(|track| track[index].clone())
            .collect::<Vec<_>>();
        let _ = super::regions::execute(session, outer, &inner, shapes)
            .map_err(|e| e.context(&format!("loft section {index}")))?;
    }
    let outer = execute_simple_profiles_constrained(
        session,
        profiles,
        ruled,
        shapes,
        correspondence,
        start,
        end,
    )?;
    let mut inners = Vec::new();
    for (index, track) in holes.iter().enumerate() {
        inners.push(
            execute_simple_profiles_constrained(
                session,
                track,
                ruled,
                shapes,
                correspondence,
                start,
                end,
            )
            .map_err(|e| e.context(&format!("loft hole track {index}")))?,
        );
    }
    super::hollow::subtract(session, &outer, &inners)
}

#[cfg(test)]
fn execute_simple_profiles<'a>(
    session: &'a Session,
    profiles: &[String],
    ruled: bool,
    shapes: &HashMap<String, Shape<'a>>,
) -> Result<Shape<'a>, ModelError> {
    execute_simple_profiles_constrained(
        session,
        profiles,
        ruled,
        shapes,
        &HashMap::new(),
        None,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn execute_simple_profiles_constrained<'a>(
    session: &'a Session,
    profiles: &[String],
    ruled: bool,
    shapes: &HashMap<String, Shape<'a>>,
    correspondence: &Correspondence,
    start: Option<Vec3>,
    end: Option<Vec3>,
) -> Result<Shape<'a>, ModelError> {
    if !(2..=MAXIMUM_SECTIONS).contains(&profiles.len())
        || profiles.iter().collect::<HashSet<_>>().len() != profiles.len()
    {
        return Err(ModelError::new(
            "profile loft needs 2-1000 distinct profile outputs",
        ));
    }
    let mut temporary = Vec::new();
    let mut wires = Vec::new();
    for id in profiles {
        let profile = shape(shapes, id)?;
        match session.shape_type(profile)? {
            ShapeType::Wire => {}
            ShapeType::Face => {
                if !session.face_is_planar(profile)? || !session.is_valid(profile)? {
                    return Err(ModelError::new(
                        "profile loft faces must be valid planar regions",
                    ));
                }
                if session.subshape_count(profile, ShapeType::Wire)? != 1 {
                    return Err(ModelError::new(
                        "profile loft sections must each have one boundary; provide explicit hole tracks",
                    ));
                }
                temporary.push(session.subshape(profile, ShapeType::Wire, 0)?);
            }
            _ => {
                return Err(ModelError::new(
                    "profile loft sections must be planar faces or closed wires",
                ));
            }
        }
    }
    let mut extracted = temporary.iter();
    for id in profiles {
        let profile = shape(shapes, id)?;
        wires.push(if session.shape_type(profile)? == ShapeType::Face {
            extracted.next().expect("face wire extracted above")
        } else {
            profile
        });
    }
    let mut aligned = Vec::new();
    for (id, wire) in profiles.iter().zip(&wires) {
        if let Some(&(anchor, reverse)) = correspondence.get(id) {
            aligned.push(Some(
                session
                    .align_profile_wire(wire, anchor, reverse)
                    .map_err(|e| {
                        ModelError::from(e).context(&format!("loft profile {id} alignment"))
                    })?,
            ));
        } else {
            aligned.push(None);
        }
    }
    let wires = wires
        .iter()
        .zip(&aligned)
        .map(|(wire, aligned)| aligned.as_ref().unwrap_or(wire))
        .collect::<Vec<_>>();
    let mut result = if start.is_some() || end.is_some() {
        session.create_constrained_loft(&wires, start, end)?
    } else {
        session.create_loft_from_wires(&wires, true, ruled)?
    };
    for aligned in aligned.iter().flatten() {
        result = session.compose_history(&result, aligned)?;
    }
    let volume = session.volume(&result)?;
    if session.shape_type(&result)? != ShapeType::Solid
        || !session.is_valid(&result)?
        || !volume.is_finite()
        || volume <= 0.0
    {
        return Err(ModelError::new(
            "profile loft must produce one valid solid with finite positive volume",
        ));
    }
    Ok(result)
}

#[cfg(test)]
mod profile_tests {
    use super::*;
    #[test]
    fn rejects_holed_native_faces_without_discarding_inner_boundaries() {
        let session = Session::new().unwrap();
        {
            let outer = session
                .create_cylinder(Vec3::new(0., 0., 0.), Vec3::new(0., 0., 1.), 4., 1.)
                .unwrap();
            let inner = session
                .create_cylinder(Vec3::new(0., 0., -1.), Vec3::new(0., 0., 1.), 1., 3.)
                .unwrap();
            let ring = session.cut(&outer, &inner).unwrap();
            let face = session
                .subshapes(&ring, ShapeType::Face)
                .unwrap()
                .into_iter()
                .find(|f| {
                    session.face_is_planar(f).unwrap() && session.face_normal(f).unwrap().z > 0.9
                })
                .unwrap();
            assert_eq!(session.subshape_count(&face, ShapeType::Wire).unwrap(), 2);
            let upper = session
                .create_circle_wire(Vec3::new(0., 0., 10.), Vec3::new(0., 0., 1.), 3.)
                .unwrap();
            let shapes = HashMap::from([("ring".into(), face), ("upper".into(), upper)]);
            let error =
                execute_simple_profiles(&session, &["ring".into(), "upper".into()], true, &shapes)
                    .unwrap_err();
            assert!(error.message.contains("one boundary"));
            let sphere = session.create_sphere(Vec3::new(0., 0., 0.), 3.).unwrap();
            let curved = session.subshape(&sphere, ShapeType::Face, 0).unwrap();
            let mut shapes = shapes;
            shapes.insert("curved".into(), curved);
            let error = execute_simple_profiles(
                &session,
                &["curved".into(), "upper".into()],
                true,
                &shapes,
            )
            .unwrap_err();
            assert!(error.message.contains("planar regions"));
        }
        assert_eq!(session.shape_count().unwrap(), 0);
    }
}

#[cfg(test)]
mod normal_tests {
    use super::*;
    #[test]
    fn profile_normals_follow_tilt_and_station_order_and_reject_ambiguous_modes() {
        let session = Session::new().unwrap();
        let c = LoftEndConstraint {
            direction: None,
            normal_to_profile: true,
            magnitude: ScalarExpr::Literal(Quantity::length(10., LengthUnit::Millimeter)),
        };
        for tilt in [0.0_f64, 0.25] {
            let normal = Vec3::new(0., -tilt, 1.);
            let a = session
                .create_ellipse_wire_axes(
                    Vec3::new(0., 0., 0.),
                    normal,
                    Vec3::new(1., 0., 0.),
                    4.,
                    4.,
                )
                .unwrap();
            let b = session
                .create_ellipse_wire_axes(
                    Vec3::new(0., 0., 10.),
                    normal,
                    Vec3::new(1., 0., 0.),
                    2.,
                    2.,
                )
                .unwrap();
            let shapes = HashMap::from([("a".into(), a), ("b".into(), b)]);
            for reverse in [false, true] {
                let profiles = if reverse {
                    vec!["b".into(), "a".into()]
                } else {
                    vec!["a".into(), "b".into()]
                };
                let expected = scale(
                    unit_vector(normal, "normal").unwrap(),
                    if reverse { -10. } else { 10. },
                );
                let start = end_derivative(
                    &session,
                    &profiles,
                    &shapes,
                    &HashMap::new(),
                    Some(&c),
                    false,
                )
                .unwrap()
                .unwrap();
                let end = end_derivative(
                    &session,
                    &profiles,
                    &shapes,
                    &HashMap::new(),
                    Some(&c),
                    true,
                )
                .unwrap()
                .unwrap();
                assert!(length(crate::assembly::subtract(start, expected)) < 1e-7);
                assert!(length(crate::assembly::subtract(end, expected)) < 1e-7);
                let solid = session
                    .create_constrained_loft(
                        &[&shapes[&profiles[0]], &shapes[&profiles[1]]],
                        Some(start),
                        Some(end),
                    )
                    .unwrap();
                assert!(session.is_valid(&solid).unwrap());
            }
            let profiles = vec!["a".into(), "b".into()];
            let mut invalid = c.clone();
            invalid.direction = Some(VectorExpr::Literal(VectorQuantity::scalars(0., 0., 1.)));
            assert!(
                end_derivative(
                    &session,
                    &profiles,
                    &shapes,
                    &HashMap::new(),
                    Some(&invalid),
                    false
                )
                .is_err()
            );
            invalid.direction = None;
            invalid.normal_to_profile = false;
            assert!(
                end_derivative(
                    &session,
                    &profiles,
                    &shapes,
                    &HashMap::new(),
                    Some(&invalid),
                    false
                )
                .is_err()
            );
        }
        let a = session
            .create_circle_wire(Vec3::new(0., 0., 0.), Vec3::new(0., 0., 1.), 2.)
            .unwrap();
        let b = session
            .create_circle_wire(Vec3::new(10., 0., 0.), Vec3::new(0., 0., 1.), 2.)
            .unwrap();
        let shapes = HashMap::from([("a".into(), a), ("b".into(), b)]);
        assert!(
            end_derivative(
                &session,
                &["a".into(), "b".into()],
                &shapes,
                &HashMap::new(),
                Some(&c),
                false
            )
            .is_err()
        );
    }
}
