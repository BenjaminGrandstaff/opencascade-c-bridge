//! Ordered datum intent and a strictly nominal orthogonal-plane coordinate frame.
use super::*;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DrawingDatumReferenceFrame {
    pub id: String,
    pub datums: Vec<DrawingDatumReference>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DatumPrecedence {
    Primary,
    Secondary,
    Tertiary,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedDrawingDatumReference {
    pub datum_feature: String,
    pub label: String,
    pub precedence: DatumPrecedence,
    pub boundary: DatumMaterialBoundary,
    pub geometry: ResolvedDatum,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedDrawingDatumReferenceFrame {
    pub id: String,
    pub datums: Vec<ResolvedDrawingDatumReference>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DrawingDatumCoordinateFrame {
    pub origin_mm: Vec3,
    pub x_axis: Vec3,
    pub y_axis: Vec3,
    pub z_axis: Vec3,
}
impl DrawingDatumCoordinateFrame {
    pub fn coordinates_mm(&self, point: Vec3) -> Result<Vec3, ModelError> {
        let delta = subtract(point, self.origin_mm);
        let p = Vec3::new(
            dot(delta, self.x_axis),
            dot(delta, self.y_axis),
            dot(delta, self.z_axis),
        );
        if ![p.x, p.y, p.z].into_iter().all(f64::is_finite) {
            return Err(ModelError::new(
                "datum-frame coordinates exceed finite limits",
            ));
        }
        Ok(p)
    }
}
impl ResolvedDrawingDatumReferenceFrame {
    /// Nominal model planes only: no fitted simulators, bonus tolerance or shift.
    pub fn nominal_planar_321(&self) -> Result<DrawingDatumCoordinateFrame, ModelError> {
        let [primary, secondary, tertiary] = self.datums.as_slice() else {
            return Err(ModelError::new(
                "nominal 3-2-1 requires exactly three datum planes",
            ));
        };
        let (p, z) = plane(primary)?;
        let (s, sn) = plane(secondary)?;
        let (t, tn) = plane(tertiary)?;
        if [dot(z, sn), dot(z, tn), dot(sn, tn)]
            .into_iter()
            .any(|v| v.abs() > 1e-9)
        {
            return Err(ModelError::new(
                "nominal 3-2-1 requires mutually orthogonal planes",
            ));
        }
        let determinant = dot(z, cross(sn, tn));
        let sx = cross(tn, z);
        let ty = cross(z, sn);
        let a = dot(sn, subtract(s, p)) / determinant;
        let b = dot(tn, subtract(t, p)) / determinant;
        let origin = Vec3::new(
            p.x + sx.x * a + ty.x * b,
            p.y + sx.y * a + ty.y * b,
            p.z + sx.z * a + ty.z * b,
        );
        if ![origin.x, origin.y, origin.z]
            .into_iter()
            .all(f64::is_finite)
        {
            return Err(ModelError::new(
                "nominal datum intersection exceeds finite limits",
            ));
        }
        let x = axis(VectorQuantity::scalars(
            sn.x - z.x * dot(z, sn),
            sn.y - z.y * dot(z, sn),
            sn.z - z.z * dot(z, sn),
        ))?;
        Ok(DrawingDatumCoordinateFrame {
            origin_mm: origin,
            x_axis: x,
            y_axis: cross(z, x),
            z_axis: z,
        })
    }
}
impl ResolvedDrawingDatumReferenceFrame {
    /// Frame of a primary plane A, a secondary axis B normal to it and an
    /// optional tertiary plane C (schema 137): origin where B meets A, z along
    /// A's normal, x along C's normal projected into A. Without C the x axis is
    /// arbitrary, so only rotation-invariant (coaxial) uses are meaningful.
    /// Boundaries are ignored here; datum shift is applied by the evaluator.
    pub fn plane_axis_frame(&self) -> Result<DrawingDatumCoordinateFrame, ModelError> {
        let (a, b, c) = match self.datums.as_slice() {
            [a, b] => (a, b, None),
            [a, b, c] => (a, b, Some(c)),
            _ => {
                return Err(ModelError::new(
                    "a plane-axis frame needs a primary plane and a secondary axis",
                ));
            }
        };
        let (
            ResolvedDatum::Plane { origin: p, normal },
            ResolvedDatum::Axis {
                origin: q,
                direction,
            },
        ) = (a.geometry, b.geometry)
        else {
            return Err(ModelError::new(
                "a plane-axis frame needs a primary plane and a secondary axis",
            ));
        };
        let z = axis(VectorQuantity::scalars(normal.x, normal.y, normal.z))?;
        let d = axis(VectorQuantity::scalars(
            direction.x,
            direction.y,
            direction.z,
        ))?;
        if length(cross(z, d)) > 1e-6 {
            return Err(ModelError::new(
                "the secondary axis datum must be normal to the primary plane",
            ));
        }
        // Where the axis line meets the primary plane.
        let travel = dot(subtract(p, q), z) / dot(d, z);
        let origin = Vec3::new(q.x + d.x * travel, q.y + d.y * travel, q.z + d.z * travel);
        let reference = match c.map(|c| c.geometry) {
            Some(ResolvedDatum::Plane { normal, .. }) => normal,
            // A tertiary hole or pin clocks the frame toward its axis.
            Some(ResolvedDatum::Axis { origin: r, .. }) => subtract(r, origin),
            Some(_) => {
                return Err(ModelError::new(
                    "the tertiary datum of a plane-axis frame must be a plane or an axis",
                ));
            }
            None if z.x.abs() < 0.9 => Vec3::new(1.0, 0.0, 0.0),
            None => Vec3::new(0.0, 1.0, 0.0),
        };
        let x = axis(VectorQuantity::scalars(
            reference.x - z.x * dot(z, reference),
            reference.y - z.y * dot(z, reference),
            reference.z - z.z * dot(z, reference),
        ))?;
        Ok(DrawingDatumCoordinateFrame {
            origin_mm: origin,
            x_axis: x,
            y_axis: cross(z, x),
            z_axis: z,
        })
    }
}
fn plane(d: &ResolvedDrawingDatumReference) -> Result<(Vec3, Vec3), ModelError> {
    if d.boundary != DatumMaterialBoundary::Regardless {
        return Err(ModelError::new(
            "nominal 3-2-1 does not solve material-boundary datum shift",
        ));
    }
    let ResolvedDatum::Plane { origin, normal } = d.geometry else {
        return Err(ModelError::new(
            "nominal 3-2-1 requires plane datum geometry",
        ));
    };
    Ok((
        origin,
        axis(VectorQuantity::scalars(normal.x, normal.y, normal.z))?,
    ))
}
impl DrawingDefinition {
    pub fn resolve_datum_reference_frame(
        &self,
        id: &str,
        graph: &InstanceGraph<'_>,
    ) -> Result<ResolvedDrawingDatumReferenceFrame, ModelError> {
        self.validate(graph)?;
        let frame = self
            .datum_reference_frames
            .iter()
            .find(|f| f.id == id)
            .ok_or_else(|| ModelError::new("unknown drawing datum reference frame"))?;
        let index: HashMap<_, _> = self
            .datum_features
            .iter()
            .map(|f| (f.id.as_str(), f))
            .collect();
        resolve(frame, &index, graph)
    }
    /// Validates once and resolves all named frames with an indexed feature lookup.
    pub fn resolve_datum_reference_frames(
        &self,
        graph: &InstanceGraph<'_>,
    ) -> Result<Vec<ResolvedDrawingDatumReferenceFrame>, ModelError> {
        self.validate(graph)?;
        let index: HashMap<_, _> = self
            .datum_features
            .iter()
            .map(|f| (f.id.as_str(), f))
            .collect();
        self.datum_reference_frames
            .iter()
            .map(|frame| resolve(frame, &index, graph))
            .collect()
    }
}
fn resolve(
    frame: &DrawingDatumReferenceFrame,
    index: &HashMap<&str, &DrawingDatumFeature>,
    graph: &InstanceGraph<'_>,
) -> Result<ResolvedDrawingDatumReferenceFrame, ModelError> {
    let datums = frame
        .datums
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let feature = index[r.datum_feature.as_str()];
            Ok(ResolvedDrawingDatumReference {
                datum_feature: r.datum_feature.clone(),
                label: r.label(index),
                precedence: match i {
                    0 => DatumPrecedence::Primary,
                    1 => DatumPrecedence::Secondary,
                    _ => DatumPrecedence::Tertiary,
                },
                boundary: r.boundary,
                geometry: graph.datum(
                    &feature.attachment.anchor.instance,
                    &feature.attachment.anchor.datum,
                )?,
            })
        })
        .collect::<Result<_, ModelError>>()?;
    Ok(ResolvedDrawingDatumReferenceFrame {
        id: frame.id.clone(),
        datums,
    })
}
