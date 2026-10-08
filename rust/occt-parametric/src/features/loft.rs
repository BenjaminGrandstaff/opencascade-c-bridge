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

pub(crate) fn collect_parameters<'a>(sections: &'a [LoftSection], names: &mut HashSet<&'a str>) {
    for section in sections {
        for vector in [&section.origin, &section.x_axis, &section.y_axis] {
            collect_vector_parameters(vector, names);
        }
        collect_scalar_parameters(&section.scale, names);
        if let Some(angle) = &section.rotation_radians {
            collect_scalar_parameters(angle, names);
        }
    }
}

/// Linear profile/topology indexing; native curve compatibility and surface
/// interpolation cost depends on edge correspondence, sections and degree.
pub(crate) fn execute_profiles<'a>(
    session: &'a Session,
    profiles: &[String],
    ruled: bool,
    shapes: &HashMap<String, Shape<'a>>,
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
                        "profile loft faces must have exactly one boundary; loft holes separately and cut",
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
    let result = session.create_loft_from_wires(&wires, true, ruled)?;
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
            let error = execute_profiles(&session, &["ring".into(), "upper".into()], true, &shapes)
                .unwrap_err();
            assert!(error.message.contains("one boundary"));
            let sphere = session.create_sphere(Vec3::new(0., 0., 0.), 3.).unwrap();
            let curved = session.subshape(&sphere, ShapeType::Face, 0).unwrap();
            let mut shapes = shapes;
            shapes.insert("curved".into(), curved);
            let error =
                execute_profiles(&session, &["curved".into(), "upper".into()], true, &shapes)
                    .unwrap_err();
            assert!(error.message.contains("planar regions"));
        }
        assert_eq!(session.shape_count().unwrap(), 0);
    }
}
