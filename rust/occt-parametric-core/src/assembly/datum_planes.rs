//! Constructed datum planes (schema 165): planes offset or turned from a base
//! plane, on a face, midway between faces, or through three points. Each is
//! fully determined by its references and parameter expressions, so a plane
//! can never float free: it moves only when a parameter or a referenced face
//! does.

use super::*;

/// The plane a constructed plane is built from: a principal plane of the
/// family's coordinates or another plane datum.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PlaneBase {
    /// Origin, normal +Z, x axis +X.
    Xy,
    /// Origin, normal -Y, x axis +X (y axis +Z).
    Xz,
    /// Origin, normal +X, x axis +Y (y axis +Z).
    Yz,
    Datum(String),
}

impl PlaneBase {
    /// Origin, normal and x axis of a principal plane; `None` for a datum.
    pub fn principal(&self) -> Option<(Vec3, Vec3, Vec3)> {
        let origin = Vec3::new(0.0, 0.0, 0.0);
        match self {
            Self::Xy => Some((origin, Vec3::new(0.0, 0.0, 1.0), Vec3::new(1.0, 0.0, 0.0))),
            Self::Xz => Some((origin, Vec3::new(0.0, -1.0, 0.0), Vec3::new(1.0, 0.0, 0.0))),
            Self::Yz => Some((origin, Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0))),
            Self::Datum(_) => None,
        }
    }
}

/// Planar faces of generated feature outputs, for planes built on faces.
pub trait PlaneFaces {
    /// Centre and unit outward normal of the one planar face `face` selects
    /// on output `input`.
    fn planar_face(&self, input: &str, face: &FaceSelector) -> Result<(Vec3, Vec3), ModelError>;
}

/// No geometry: planes on faces fail with a clear message.
pub struct NoFaces;

impl PlaneFaces for NoFaces {
    fn planar_face(&self, input: &str, _: &FaceSelector) -> Result<(Vec3, Vec3), ModelError> {
        Err(ModelError::new(format!(
            "plane is built on a face of '{input}' and needs the generated model"
        )))
    }
}

/// A unit XY face at the origin for every selection, so validation can
/// check expressions, base chains and cycles without geometry.
pub struct StandInFaces;

impl PlaneFaces for StandInFaces {
    fn planar_face(&self, _: &str, _: &FaceSelector) -> Result<(Vec3, Vec3), ModelError> {
        Ok((Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 1.0)))
    }
}

/// Datums by id. O(datums).
pub fn datum_index(datums: &[DatumDefinition]) -> HashMap<&str, &DatumDefinition> {
    datums
        .iter()
        .map(|datum| (datum.id.as_str(), datum))
        .collect()
}

/// Resolves datum `id`, following base datums. A chain longer than the
/// datum count is a cycle. O(chain length × (expression size + face query)).
pub fn resolve_datum(
    datums: &HashMap<&str, &DatumDefinition>,
    id: &str,
    parameters: &HashMap<String, ParameterValue>,
    faces: &dyn PlaneFaces,
) -> Result<ResolvedDatum, ModelError> {
    fn walk(
        datums: &HashMap<&str, &DatumDefinition>,
        id: &str,
        parameters: &HashMap<String, ParameterValue>,
        faces: &dyn PlaneFaces,
        depth: usize,
    ) -> Result<ResolvedDatum, ModelError> {
        if depth > datums.len() {
            return Err(ModelError::new(format!(
                "datum plane '{id}' is built on itself through its bases"
            )));
        }
        let datum = datums
            .get(id)
            .ok_or_else(|| ModelError::new(format!("unknown datum '{id}'")))?;
        datum.kind.resolve(
            parameters,
            &|base| walk(datums, base, parameters, faces, depth + 1),
            faces,
        )
    }
    walk(datums, id, parameters, faces, 0)
}

impl DatumKind {
    /// True for every kind that resolves to a plane.
    pub fn is_plane(&self) -> bool {
        !matches!(self, Self::Point { .. } | Self::Axis { .. })
    }

    /// The datum this plane is built on, if any.
    pub fn base_datum(&self) -> Option<&str> {
        match self {
            Self::OffsetPlane {
                base: PlaneBase::Datum(id),
                ..
            }
            | Self::AnglePlane {
                base: PlaneBase::Datum(id),
                ..
            } => Some(id),
            _ => None,
        }
    }

    /// The feature output and face selectors this plane reads, if any.
    pub fn faces(&self) -> Option<(&str, Vec<&FaceSelector>)> {
        match self {
            Self::FacePlane { input, face, .. } => Some((input, vec![face.as_ref()])),
            Self::Midplane {
                input,
                first,
                second,
            } => Some((input, vec![first.as_ref(), second.as_ref()])),
            _ => None,
        }
    }

    /// Parameters this datum's own expressions read (not its base's).
    pub fn collect_parameters<'a>(&'a self, names: &mut HashSet<&'a str>) {
        match self {
            Self::Point { origin } => collect_vector_parameters(origin, names),
            Self::Axis { origin, direction } => {
                collect_vector_parameters(origin, names);
                collect_vector_parameters(direction, names);
            }
            Self::Plane { origin, normal } => {
                collect_vector_parameters(origin, names);
                collect_vector_parameters(normal, names);
            }
            Self::OffsetPlane { distance, .. } => collect_scalar_parameters(distance, names),
            Self::AnglePlane {
                hinge,
                angle_radians,
                ..
            } => {
                collect_vector_parameters(hinge, names);
                collect_scalar_parameters(angle_radians, names);
            }
            Self::FacePlane { face, distance, .. } => {
                collect_face_selector_parameters(face, names);
                collect_scalar_parameters(distance, names);
            }
            Self::Midplane { first, second, .. } => {
                collect_face_selector_parameters(first, names);
                collect_face_selector_parameters(second, names);
            }
            Self::ThreePointPlane {
                first,
                second,
                third,
            } => {
                for point in [first, second, third] {
                    collect_vector_parameters(point, names);
                }
            }
        }
    }

    /// Resolves this datum with `base` resolving datums it is built on and
    /// `faces` the faces it reads.
    pub fn resolve(
        &self,
        parameters: &HashMap<String, ParameterValue>,
        base: &dyn Fn(&str) -> Result<ResolvedDatum, ModelError>,
        faces: &dyn PlaneFaces,
    ) -> Result<ResolvedDatum, ModelError> {
        let point = |expression| vector(expression, parameters, Dimension::Length);
        let direction =
            |expression| vector(expression, parameters, Dimension::Scalar).and_then(unit);
        let millimetres = |expression| scalar(expression, parameters, Dimension::Length);
        let base_plane = |plane: &PlaneBase| match plane.principal() {
            Some((origin, normal, _)) => Ok((origin, normal)),
            None => {
                let PlaneBase::Datum(id) = plane else {
                    unreachable!()
                };
                match base(id)? {
                    ResolvedDatum::Plane { origin, normal } => Ok((origin, normal)),
                    _ => Err(ModelError::new(format!(
                        "base datum '{id}' must be a plane"
                    ))),
                }
            }
        };
        let plane = |origin: Vec3, normal: Vec3| {
            if [origin.x, origin.y, origin.z].iter().all(|v| v.is_finite()) {
                Ok(ResolvedDatum::Plane { origin, normal })
            } else {
                Err(ModelError::new("datum plane origin must be finite"))
            }
        };
        Ok(match self {
            Self::Point { origin } => ResolvedDatum::Point {
                origin: point(origin)?,
            },
            Self::Axis {
                origin,
                direction: axis,
            } => ResolvedDatum::Axis {
                origin: point(origin)?,
                direction: direction(axis)?,
            },
            Self::Plane { origin, normal } => ResolvedDatum::Plane {
                origin: point(origin)?,
                normal: direction(normal)?,
            },
            Self::OffsetPlane {
                base: from,
                distance,
            } => {
                let (origin, normal) = base_plane(from)?;
                plane(add(origin, scale(normal, millimetres(distance)?)), normal)?
            }
            Self::AnglePlane {
                base: from,
                hinge,
                angle_radians,
            } => {
                let (origin, normal) = base_plane(from)?;
                let hinge = vector(hinge, parameters, Dimension::Scalar)?;
                let hinge = unit(subtract(hinge, scale(normal, dot(hinge, normal))))
                    .map_err(|_| ModelError::new("plane hinge must not be normal to its base"))?;
                let angle = scalar(angle_radians, parameters, Dimension::Scalar)?;
                if !angle.is_finite() {
                    return Err(ModelError::new("plane angle must be finite"));
                }
                let turned = add(
                    scale(normal, angle.cos()),
                    scale(cross(hinge, normal), angle.sin()),
                );
                plane(origin, unit(turned)?)?
            }
            Self::FacePlane {
                input,
                face,
                distance,
            } => {
                let (centre, normal) = faces.planar_face(input, face)?;
                let normal = unit(normal)?;
                plane(add(centre, scale(normal, millimetres(distance)?)), normal)?
            }
            Self::Midplane {
                input,
                first,
                second,
            } => {
                let (a, first_normal) = faces.planar_face(input, first)?;
                let (b, second_normal) = faces.planar_face(input, second)?;
                let normal = unit(first_normal)?;
                if length(cross(normal, unit(second_normal)?)) > 1e-7 {
                    return Err(ModelError::new("midplane faces must be parallel"));
                }
                plane(scale(add(a, b), 0.5), normal)?
            }
            Self::ThreePointPlane {
                first,
                second,
                third,
            } => {
                let a = point(first)?;
                let normal = cross(subtract(point(second)?, a), subtract(point(third)?, a));
                let normal = unit(normal).map_err(|_| {
                    ModelError::new("plane points must not be coincident or collinear")
                })?;
                plane(a, normal)?
            }
        })
    }
}

/// The feature outputs a datum depends on through faces it reads, its base
/// chain, and the named references its face selectors use. O(chain length
/// × selector size); a cycle stops after the datum count.
pub fn datum_dependencies<'a>(
    datums: &HashMap<&str, &'a DatumDefinition>,
    references: &References<'a>,
    id: &str,
    dependencies: &mut Vec<&'a str>,
) {
    for datum in datum_chain(datums, id) {
        let Some((input, selectors)) = datum.kind.faces() else {
            continue;
        };
        dependencies.push(input);
        for selector in selectors {
            selector.dependencies(dependencies);
            let mut names = Vec::new();
            selector.names(&mut names);
            for (name, _) in names {
                if let Some(reference) = references.get(name) {
                    reference.target.dependencies(dependencies);
                }
            }
        }
    }
}

/// An x axis in a plane with `normal`: world X projected into the plane, or
/// world Y when X is nearly normal to it. Used to give new sketches on a
/// datum plane a frame.
pub fn plane_x_axis(normal: Vec3) -> Vec3 {
    let pick = if dot(normal, Vec3::new(1.0, 0.0, 0.0)).abs() > 0.9 {
        Vec3::new(0.0, 1.0, 0.0)
    } else {
        Vec3::new(1.0, 0.0, 0.0)
    };
    unit(subtract(pick, scale(normal, dot(pick, normal)))).unwrap_or(pick)
}

/// Rejects a plane whose x axis cannot be projected into it.
pub fn project_into_plane(x_axis: Vec3, normal: Vec3) -> Result<Vec3, ModelError> {
    let projected = subtract(x_axis, scale(normal, dot(x_axis, normal)));
    if length(projected) < 1e-6 * length(x_axis) {
        return Err(ModelError::new("sketch x axis must lie in its datum plane"));
    }
    unit(projected)
}

/// Datum `id` followed by the datums it is built on. Stops at a missing
/// datum or after the datum count (a cycle).
pub fn datum_chain<'a>(
    datums: &HashMap<&str, &'a DatumDefinition>,
    id: &str,
) -> Vec<&'a DatumDefinition> {
    let mut chain = Vec::new();
    let mut next = Some(id);
    while let Some(datum) = next.and_then(|id| datums.get(id).copied()) {
        if chain.len() > datums.len() {
            break;
        }
        chain.push(datum);
        next = datum.kind.base_datum();
    }
    chain
}

/// True when datum `id` or a datum it is built on reads model faces.
pub fn datum_reads_faces(datums: &HashMap<&str, &DatumDefinition>, id: &str) -> bool {
    datum_chain(datums, id)
        .iter()
        .any(|datum| datum.kind.faces().is_some())
}
