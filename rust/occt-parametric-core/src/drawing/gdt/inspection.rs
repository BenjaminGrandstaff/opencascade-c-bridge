//! Measured inspection: datum simulators fitted to measured points and
//! conformance of planar form/orientation, round form (circularity and
//! cylindricity, minimum zone) and axis position controls.
//!
//! Measured points are in model coordinates at the current poses, as from a
//! coordinate measuring machine aligned to the part; the residual misalignment
//! must be small because fits start from the nominal orientations. Nominal
//! datum plane normals point out of the material, so each datum simulator
//! contacts the measured high points on that side (constrained L∞).
use super::*;
use crate::assembly::{add, length, scale, unit};
pub mod fit;
use fit::{Fit, circle_zone, cylinder_zone, fit_circle, fit_cylinder, fit_plane};

/// Upper bound on measured points in one record, which bounds memory and time.
pub const MAX_INSPECTION_POINTS: usize = 10_000_000;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeasuredFeature {
    /// Drawing datum feature ID or feature control frame ID.
    pub id: String,
    pub points_mm: Vec<[f64; 3]>,
}

/// Measured points for one part against one drawing's GD&T.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InspectionRecord {
    pub drawing: String,
    #[serde(default)]
    pub datum_features: Vec<MeasuredFeature>,
    #[serde(default)]
    pub controls: Vec<MeasuredFeature>,
    /// Measured roughness values by drawing surface texture ID.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub surface_textures: Vec<MeasuredTexture>,
}

/// Roughness readings for one surface texture requirement, each in the
/// requirement's parameter and unit (for example several Ra traces in µm).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeasuredTexture {
    pub id: String,
    pub values: Vec<f64>,
}

/// Measured roughness against a requirement by the maximum rule: every
/// reading must be at or below the maximum and at or above any minimum.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TextureMeasurement {
    pub readings: usize,
    pub highest: f64,
    pub lowest: f64,
    pub maximum: f64,
    pub minimum: Option<f64>,
    pub conforms: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "status", content = "detail")]
pub enum TextureResult {
    Evaluated(TextureMeasurement),
    NotMeasured,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TextureInspection {
    pub texture: String,
    pub result: TextureResult,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ControlMeasurement {
    /// Width of the measured zone: plane separation, radial separation of the
    /// minimum-zone circles or cylinders, or twice the radial axis offset.
    pub deviation_mm: f64,
    pub tolerance_mm: f64,
    /// Material-condition bonus from the measured size; zero regardless of size.
    pub bonus_mm: f64,
    /// Actual mating size of a feature of size, when evaluated.
    pub actual_size_mm: Option<f64>,
    pub size_conforms: Option<bool>,
    pub conforms: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "status", content = "detail")]
pub enum ControlResult {
    Evaluated(ControlMeasurement),
    NotMeasured,
    /// The control or its measurements are outside what this evaluator supports.
    NotEvaluated(String),
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ControlInspection {
    pub control: String,
    pub result: ControlResult,
}

/// A complete three-plane frame established from measured datum simulators.
#[derive(Clone, Debug, PartialEq)]
pub struct MeasuredDatumFrame {
    pub datum_features: Vec<String>,
    pub frame: DrawingDatumCoordinateFrame,
}

#[derive(Clone, Debug, PartialEq)]
pub struct InspectionReport {
    pub drawing: String,
    pub datum_frames: Vec<MeasuredDatumFrame>,
    /// Named datum reference frames whose last datum is a measured axis.
    pub datum_axes: Vec<MeasuredDatumAxis>,
    /// One entry per effective feature control frame, in drawing order.
    pub controls: Vec<ControlInspection>,
    /// One entry per surface texture requirement, in drawing order.
    pub surface_textures: Vec<TextureInspection>,
}

impl InspectionReport {
    /// True only when every control and surface texture was evaluated and
    /// conforms.
    pub fn conforms(&self) -> bool {
        self.controls
            .iter()
            .all(|c| matches!(&c.result, ControlResult::Evaluated(m) if m.conforms))
            && self
                .surface_textures
                .iter()
                .all(|t| matches!(&t.result, TextureResult::Evaluated(m) if m.conforms))
    }
}

/// A datum simulator: a contacting plane (point, outward normal) or the axis
/// of an actual mating envelope (point, unit direction, mating size).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Simulator {
    Plane(Vec3, Vec3),
    Axis(Vec3, Vec3, f64),
}
type Simulators = Vec<Simulator>;
impl Simulator {
    fn plane_normal(self) -> Result<Vec3, String> {
        match self {
            Self::Plane(_, normal) => Ok(normal),
            Self::Axis(..) => Err("orientation to an axis datum is not evaluated".into()),
        }
    }
}

/// A datum axis established from a measured cylindrical datum feature of size.
#[derive(Clone, Debug, PartialEq)]
pub struct MeasuredDatumAxis {
    pub datum_features: Vec<String>,
    pub origin_mm: Vec3,
    pub direction: Vec3,
    /// Actual mating size (diameter) of the axis datum feature.
    pub mating_size_mm: f64,
}

struct Context<'a, 'g> {
    graph: &'a InstanceGraph<'g>,
    features: HashMap<&'a str, &'a DrawingDatumFeature>,
    measured_datums: HashMap<&'a str, Vec<Vec3>>,
    /// Simulator planes keyed by datum precedence prefix; each prefix is fitted once.
    simulators: HashMap<Vec<String>, Result<Simulators, String>>,
}

impl DrawingDefinition {
    /// Evaluates measured points against this drawing's controls.
    ///
    /// Datum simulators are fitted once per distinct datum precedence prefix
    /// and shared by every control that references it, so time is
    /// O(controls · datums + Σ fit cost), each fit O(linearizations · pivots ·
    /// points); memory is O(points). Record errors (unknown IDs, non-finite or
    /// too many points) fail the call; unsupported controls are reported.
    pub fn evaluate_inspection(
        &self,
        graph: &InstanceGraph<'_>,
        record: &InspectionRecord,
    ) -> Result<InspectionReport, ModelError> {
        self.validate(graph)?;
        if record.drawing != self.id {
            return Err(ModelError::new(
                "inspection record names a different drawing",
            ));
        }
        let frames = effective_frames(&self.feature_control_frames, &self.datum_reference_frames)?;
        let control_ids: HashSet<&str> = frames.iter().map(|f| f.id.as_str()).collect();
        let features: HashMap<_, _> = self
            .datum_features
            .iter()
            .map(|f| (f.id.as_str(), f))
            .collect();
        let mut total = 0usize;
        let measured_datums = measured(&record.datum_features, &mut total, |id| {
            features.contains_key(id)
        })?;
        let measured_controls =
            measured(&record.controls, &mut total, |id| control_ids.contains(id))?;
        let mut context = Context {
            graph,
            features,
            measured_datums,
            simulators: HashMap::new(),
        };
        let mut controls = Vec::with_capacity(frames.len());
        for frame in &frames {
            let result = match measured_controls.get(frame.id.as_str()) {
                None => ControlResult::NotMeasured,
                Some(points) => match context.evaluate(frame, points) {
                    Ok(m) => ControlResult::Evaluated(m),
                    Err(reason) => ControlResult::NotEvaluated(reason),
                },
            };
            controls.push(ControlInspection {
                control: frame.id.clone(),
                result,
            });
        }
        let mut datum_frames = Vec::new();
        let mut datum_axes = Vec::new();
        for reference in &self.datum_reference_frames {
            let ids: Vec<String> = reference
                .datums
                .iter()
                .map(|d| d.datum_feature.clone())
                .collect();
            if let Ok(frame) = context.measured_frame(&reference.datums) {
                datum_frames.push(MeasuredDatumFrame {
                    datum_features: ids,
                    frame,
                });
            } else if let Ok(Some(Simulator::Axis(origin, direction, size))) = context
                .simulators(&reference.datums)
                .map(|s| s.last().copied())
            {
                datum_axes.push(MeasuredDatumAxis {
                    datum_features: ids,
                    origin_mm: origin,
                    direction,
                    mating_size_mm: size,
                });
            }
        }
        let surface_textures = self.inspect_textures(&record.surface_textures, &mut total)?;
        Ok(InspectionReport {
            drawing: self.id.clone(),
            datum_frames,
            datum_axes,
            controls,
            surface_textures,
        })
    }

    /// Compares readings with each texture requirement. Unknown or repeated
    /// IDs, empty or invalid readings, and the shared reading budget fail the
    /// record. O(textures + readings).
    fn inspect_textures(
        &self,
        measured: &[MeasuredTexture],
        total: &mut usize,
    ) -> Result<Vec<TextureInspection>, ModelError> {
        let mut readings: HashMap<&str, &[f64]> = HashMap::with_capacity(measured.len());
        for texture in measured {
            *total = total.saturating_add(texture.values.len());
            if *total > MAX_INSPECTION_POINTS {
                return Err(ModelError::new(
                    "inspection record exceeds the measured-point budget",
                ));
            }
            if !self.surface_textures.iter().any(|t| t.id == texture.id) {
                return Err(ModelError::new(format!(
                    "inspection record references unknown surface texture '{}'",
                    texture.id
                )));
            }
            if texture.values.is_empty()
                || !texture.values.iter().all(|v| v.is_finite() && *v >= 0.0)
            {
                return Err(ModelError::new(
                    "roughness readings must be nonempty, finite and nonnegative",
                ));
            }
            if readings
                .insert(texture.id.as_str(), &texture.values)
                .is_some()
            {
                return Err(ModelError::new(
                    "inspection record repeats a surface texture",
                ));
            }
        }
        Ok(self
            .surface_textures
            .iter()
            .map(|texture| TextureInspection {
                texture: texture.id.clone(),
                result: match readings.get(texture.id.as_str()) {
                    None => TextureResult::NotMeasured,
                    Some(values) => {
                        let highest = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                        let lowest = values.iter().copied().fold(f64::INFINITY, f64::min);
                        let limits = &texture.roughness;
                        TextureResult::Evaluated(TextureMeasurement {
                            readings: values.len(),
                            highest,
                            lowest,
                            maximum: limits.maximum,
                            minimum: limits.minimum,
                            conforms: highest <= limits.maximum
                                && limits.minimum.is_none_or(|minimum| lowest >= minimum),
                        })
                    }
                },
            })
            .collect())
    }
}

/// Indexes finite measured points by feature ID within the shared point budget.
fn measured<'a>(
    list: &'a [MeasuredFeature],
    total: &mut usize,
    known: impl Fn(&str) -> bool,
) -> Result<HashMap<&'a str, Vec<Vec3>>, ModelError> {
    let mut map = HashMap::with_capacity(list.len());
    for m in list {
        *total = total.saturating_add(m.points_mm.len());
        if *total > MAX_INSPECTION_POINTS {
            return Err(ModelError::new(
                "inspection record exceeds the measured-point budget",
            ));
        }
        if !known(&m.id) {
            return Err(ModelError::new(format!(
                "inspection record references unknown feature '{}'",
                m.id
            )));
        }
        let points = m
            .points_mm
            .iter()
            .map(|p| {
                if p.iter().all(|v| v.is_finite()) {
                    Ok(Vec3::new(p[0], p[1], p[2]))
                } else {
                    Err(ModelError::new("measured points must be finite"))
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        if map.insert(m.id.as_str(), points).is_some() {
            return Err(ModelError::new(
                "inspection record repeats a measured feature",
            ));
        }
    }
    Ok(map)
}

fn plane_datum(geometry: ResolvedDatum, what: &str) -> Result<(Vec3, Vec3), String> {
    match geometry {
        ResolvedDatum::Plane { origin, normal } => {
            Ok((origin, unit(normal).map_err(|e| e.to_string())?))
        }
        _ => Err(format!("{what} needs plane datum geometry")),
    }
}

impl Context<'_, '_> {
    fn nominal(&self, anchor: &DatumRef) -> Result<ResolvedDatum, String> {
        self.graph
            .datum(&anchor.instance, &anchor.datum)
            .map_err(|e| e.to_string())
    }

    /// Nominal plane of a datum feature (normal out of the material).
    fn nominal_datum(&self, id: &str) -> Result<(Vec3, Vec3), String> {
        plane_datum(
            self.nominal(&self.features[id].attachment.anchor)?,
            "measured datum evaluation",
        )
    }

    /// Simulator planes (point, outward normal) for an ordered datum prefix.
    fn simulators(&mut self, datums: &[DrawingDatumReference]) -> Result<Simulators, String> {
        let key: Vec<String> = datums.iter().map(|d| d.datum_feature.clone()).collect();
        if let Some(cached) = self.simulators.get(&key) {
            return cached.clone();
        }
        let result = self.fit_simulators(datums);
        self.simulators.insert(key, result.clone());
        result
    }

    fn fit_simulators(&mut self, datums: &[DrawingDatumReference]) -> Result<Simulators, String> {
        let Some((last, prefix)) = datums.split_last() else {
            return Ok(Vec::new());
        };
        let mut simulators = self.simulators(prefix)?;
        if last.boundary != DatumMaterialBoundary::Regardless {
            return Err("datum shift at a material boundary is not evaluated".into());
        }
        let id = last.datum_feature.as_str();
        let points = self
            .measured_datums
            .get(id)
            .ok_or_else(|| format!("datum feature '{id}' was not measured"))?;
        let feature = self.features[id];
        if let ResolvedDatum::Axis { direction, .. } = self.nominal(&feature.attachment.anchor)? {
            simulators.push(self.axis_simulator(feature, points, direction, &simulators)?);
            return Ok(simulators);
        }
        if simulators.iter().any(|s| matches!(s, Simulator::Axis(..))) {
            return Err("plane datums after an axis datum are not evaluated".into());
        }
        let planes: Vec<Vec3> = simulators
            .iter()
            .map(|s| match s {
                Simulator::Plane(_, n) => *n,
                Simulator::Axis(_, d, _) => *d,
            })
            .collect();
        let (_, nominal) = self.nominal_datum(id)?;
        let (normal, axes) = match planes.as_slice() {
            [] => {
                let (u, v) = perpendiculars(nominal);
                (nominal, vec![u, v])
            }
            [primary] => (
                unit(subtract(nominal, scale(*primary, dot(nominal, *primary))))
                    .map_err(|_| "secondary datum plane is parallel to the primary".to_string())?,
                vec![*primary],
            ),
            [primary, secondary, ..] => {
                let n = cross(*primary, *secondary);
                (
                    if dot(n, nominal) < 0.0 {
                        scale(n, -1.0)
                    } else {
                        n
                    },
                    Vec::new(),
                )
            }
        };
        let fitted = fit_plane(points, normal, &axes).map_err(|e| e.to_string())?;
        simulators.push(Simulator::Plane(
            scale(fitted.normal, fitted.contact),
            fitted.normal,
        ));
        Ok(simulators)
    }

    /// Axis of the actual mating envelope of a cylindrical datum feature of
    /// size: the largest inscribed cylinder of a hole or the smallest
    /// circumscribed cylinder of a shaft. As primary the axis is free; after
    /// a primary plane it is held perpendicular to that plane.
    fn axis_simulator(
        &self,
        feature: &DrawingDatumFeature,
        points: &[Vec3],
        nominal: Vec3,
        prefix: &[Simulator],
    ) -> Result<Simulator, String> {
        let kind = feature.size_kind.ok_or_else(|| {
            format!(
                "axis datum feature '{}' needs a declared size kind (internal or external)",
                feature.id
            )
        })?;
        let fit = match kind {
            FeatureOfSizeKind::Internal => Fit::Inscribed,
            FeatureOfSizeKind::External => Fit::Circumscribed,
        };
        let nominal = unit(nominal).map_err(|e| e.to_string())?;
        match prefix {
            [] => {
                let (origin, direction, radius) =
                    fit_cylinder(points, nominal, fit).map_err(|e| e.to_string())?;
                let direction = if dot(direction, nominal) < 0.0 {
                    scale(direction, -1.0)
                } else {
                    direction
                };
                Ok(Simulator::Axis(origin, direction, 2.0 * radius))
            }
            [Simulator::Plane(point, normal)] => {
                if length(cross(nominal, *normal)) > 1e-9 {
                    return Err(
                        "a secondary axis datum must be nominally perpendicular to the primary plane"
                            .into(),
                    );
                }
                let (u, v) = perpendiculars(*normal);
                let projected: Vec<[f64; 2]> = points
                    .iter()
                    .map(|&p| {
                        let d = subtract(p, *point);
                        [dot(d, u), dot(d, v)]
                    })
                    .collect();
                let (center, radius) = fit_circle(&projected, fit).map_err(|e| e.to_string())?;
                let origin = add(*point, add(scale(u, center[0]), scale(v, center[1])));
                Ok(Simulator::Axis(origin, *normal, 2.0 * radius))
            }
            _ => Err(
                "axis datums are evaluated as the primary datum or after one primary plane".into(),
            ),
        }
    }

    fn measured_frame(
        &mut self,
        datums: &[DrawingDatumReference],
    ) -> Result<DrawingDatumCoordinateFrame, String> {
        let planes = self.simulators(datums)?;
        let resolved = self.resolved_frame(datums, Some(&planes))?;
        resolved.nominal_planar_321().map_err(|e| e.to_string())
    }

    fn resolved_frame(
        &self,
        datums: &[DrawingDatumReference],
        planes: Option<&Simulators>,
    ) -> Result<ResolvedDrawingDatumReferenceFrame, String> {
        let datums = datums
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let geometry = match planes {
                    Some(p) => match p[i] {
                        Simulator::Plane(origin, normal) => ResolvedDatum::Plane { origin, normal },
                        Simulator::Axis(origin, direction, _) => {
                            ResolvedDatum::Axis { origin, direction }
                        }
                    },
                    None => {
                        self.nominal(&self.features[r.datum_feature.as_str()].attachment.anchor)?
                    }
                };
                Ok(ResolvedDrawingDatumReference {
                    datum_feature: r.datum_feature.clone(),
                    label: self.features[r.datum_feature.as_str()].label.clone(),
                    precedence: [
                        DatumPrecedence::Primary,
                        DatumPrecedence::Secondary,
                        DatumPrecedence::Tertiary,
                    ][i],
                    boundary: r.boundary,
                    geometry,
                })
            })
            .collect::<Result<_, String>>()?;
        Ok(ResolvedDrawingDatumReferenceFrame {
            id: String::new(),
            datums,
        })
    }

    fn evaluate(
        &mut self,
        frame: &DrawingFeatureControlFrame,
        points: &[Vec3],
    ) -> Result<ControlMeasurement, String> {
        if frame.refinement.is_some() {
            return Err("composite controls are not evaluated".into());
        }
        let tolerance = frame.tolerance.normalized().map_err(|e| e.to_string())?;
        let c = frame.characteristic;
        if c == GeometricCharacteristic::Position {
            return self.position(frame, points, tolerance);
        }
        if matches!(
            c,
            GeometricCharacteristic::Circularity | GeometricCharacteristic::Cylindricity
        ) {
            return self.round_form(frame, points, tolerance);
        }
        if matches!(
            c,
            GeometricCharacteristic::CircularRunout | GeometricCharacteristic::TotalRunout
        ) {
            return self.runout(frame, points, tolerance);
        }
        if frame.material != ToleranceMaterialCondition::Regardless
            || frame.zone != GeometricToleranceZone::Characteristic
        {
            return Err("only planar zones regardless of feature size are evaluated for form and orientation".into());
        }
        let (_, nominal) = plane_datum(self.nominal(&frame.attachment.anchor)?, "planar control")?;
        let fitted = match c {
            GeometricCharacteristic::Flatness => {
                let (u, v) = perpendiculars(nominal);
                fit_plane(points, nominal, &[u, v])
            }
            GeometricCharacteristic::Parallelism
            | GeometricCharacteristic::Perpendicularity
            | GeometricCharacteristic::Angularity => {
                let (normal, axes) = self.oriented(frame, nominal)?;
                fit_plane(points, normal, &axes)
            }
            _ => return Err(format!("{c:?} is not evaluated")),
        }
        .map_err(|e| e.to_string())?;
        Ok(ControlMeasurement {
            deviation_mm: fitted.width,
            tolerance_mm: tolerance,
            bonus_mm: 0.0,
            actual_size_mm: None,
            size_conforms: None,
            conforms: fitted.width <= tolerance,
        })
    }

    /// Circularity (worst section) or cylindricity about the nominal axis of
    /// the control's axis anchor. Sections are the points sharing a position
    /// along that axis (within 1e-3 mm or 1e-6 of the axial span).
    fn round_form(
        &mut self,
        frame: &DrawingFeatureControlFrame,
        points: &[Vec3],
        tolerance: f64,
    ) -> Result<ControlMeasurement, String> {
        if frame.material != ToleranceMaterialCondition::Regardless
            || frame.zone != GeometricToleranceZone::Characteristic
        {
            return Err(
                "circularity and cylindricity are evaluated regardless of feature size".into(),
            );
        }
        let ResolvedDatum::Axis { origin, direction } = self.nominal(&frame.attachment.anchor)?
        else {
            return Err("circularity and cylindricity need axis datum geometry".into());
        };
        let axis = unit(direction).map_err(|e| e.to_string())?;
        let deviation = if frame.characteristic == GeometricCharacteristic::Cylindricity {
            cylinder_zone(points, axis).map_err(|e| e.to_string())?
        } else {
            let (u, v) = perpendiculars(axis);
            let mut placed: Vec<(f64, [f64; 2])> = points
                .iter()
                .map(|&p| {
                    let d = subtract(p, origin);
                    (dot(d, axis), [dot(d, u), dot(d, v)])
                })
                .collect();
            placed.sort_by(|a, b| a.0.total_cmp(&b.0));
            let span = placed.last().map_or(0.0, |p| p.0) - placed.first().map_or(0.0, |p| p.0);
            let gap = (1e-6 * span).max(1e-3);
            let mut worst: f64 = 0.0;
            let mut start = 0;
            for end in 1..=placed.len() {
                if end == placed.len() || placed[end].0 - placed[end - 1].0 > gap {
                    let section: Vec<[f64; 2]> = placed[start..end].iter().map(|p| p.1).collect();
                    worst = worst.max(circle_zone(&section).map_err(|e| e.to_string())?);
                    start = end;
                }
            }
            worst
        };
        Ok(ControlMeasurement {
            deviation_mm: deviation,
            tolerance_mm: tolerance,
            bonus_mm: 0.0,
            actual_size_mm: None,
            size_conforms: None,
            conforms: deviation <= tolerance,
        })
    }

    /// Runout as full indicator movement about the datum axis established by
    /// the control's datums (an axis datum feature of size, alone or after a
    /// primary plane). Surfaces coaxial with the nominal axis are indicated
    /// radially; faces perpendicular to it, axially. Circular runout is the
    /// worst circular element (points sharing an axial position, or a radius
    /// for faces, within 1e-3 mm or 1e-6 of the span); total runout spans all
    /// points. O(n log n) after the datum fit.
    fn runout(
        &mut self,
        frame: &DrawingFeatureControlFrame,
        points: &[Vec3],
        tolerance: f64,
    ) -> Result<ControlMeasurement, String> {
        let Some(Simulator::Axis(origin, axis, _)) =
            self.simulators(&frame.datums)?.last().copied()
        else {
            return Err(
                "runout needs a datum axis from a measured cylindrical datum feature".into(),
            );
        };
        // The surface's relation to the datum axis is design intent: compare
        // nominal directions, not the measured (tilted) axis.
        let datum = frame
            .datums
            .last()
            .map(|d| d.datum_feature.as_str())
            .unwrap_or_default();
        let ResolvedDatum::Axis {
            direction: datum_axis,
            ..
        } = self.nominal(&self.features[datum].attachment.anchor)?
        else {
            return Err(
                "runout needs a datum axis from a measured cylindrical datum feature".into(),
            );
        };
        let datum_axis = unit(datum_axis).map_err(|e| e.to_string())?;
        let radial = match self.nominal(&frame.attachment.anchor)? {
            ResolvedDatum::Axis { direction, .. } => {
                let direction = unit(direction).map_err(|e| e.to_string())?;
                if length(cross(direction, datum_axis)) > 1e-6 {
                    return Err("runout surface axis is not coaxial with the datum axis".into());
                }
                true
            }
            ResolvedDatum::Plane { normal, .. } => {
                let normal = unit(normal).map_err(|e| e.to_string())?;
                if length(cross(normal, datum_axis)) > 1e-6 {
                    return Err("runout face is not perpendicular to the datum axis".into());
                }
                false
            }
            _ => {
                return Err("runout needs an axis or plane anchor on the controlled surface".into());
            }
        };
        // (element key, indicated value): radial readings are grouped by axial
        // position, axial (face) readings by radius.
        let mut readings: Vec<(f64, f64)> = points
            .iter()
            .map(|&p| {
                let d = subtract(p, origin);
                let along = dot(d, axis);
                let rho = length(subtract(d, scale(axis, along)));
                if radial { (along, rho) } else { (rho, along) }
            })
            .collect();
        let deviation = if frame.characteristic == GeometricCharacteristic::TotalRunout {
            spread(readings.iter().map(|r| r.1))
        } else {
            let mut worst: f64 = 0.0;
            for element in elements(&mut readings) {
                if element.len() < 3 {
                    return Err("each circular runout element needs at least three points".into());
                }
                worst = worst.max(spread(element.iter().map(|r| r.1)));
            }
            worst
        };
        Ok(ControlMeasurement {
            deviation_mm: deviation,
            tolerance_mm: tolerance,
            bonus_mm: 0.0,
            actual_size_mm: None,
            size_conforms: None,
            conforms: deviation <= tolerance,
        })
    }

    /// Feature normal at its basic orientation to the measured datums, and the
    /// rotations those datums leave free.
    fn oriented(
        &mut self,
        frame: &DrawingFeatureControlFrame,
        nominal: Vec3,
    ) -> Result<(Vec3, Vec<Vec3>), String> {
        let datums = &frame.datums[..frame.datums.len().min(2)];
        let planes = self.simulators(datums)?;
        let (_, a) = self.nominal_datum(&datums[0].datum_feature)?;
        let measured_a = planes[0].plane_normal()?;
        let reference = match datums.get(1) {
            Some(d) => self.nominal_datum(&d.datum_feature)?.1,
            None => nominal,
        };
        let e = unit(subtract(reference, scale(a, dot(reference, a))))
            .unwrap_or_else(|_| perpendiculars(a).0);
        let measured_e = match planes.get(1) {
            Some(simulator) => simulator.plane_normal()?,
            None => unit(subtract(e, scale(measured_a, dot(e, measured_a))))
                .map_err(|e| e.to_string())?,
        };
        let normal = add(
            add(
                scale(measured_a, dot(nominal, a)),
                scale(measured_e, dot(nominal, e)),
            ),
            scale(cross(measured_a, measured_e), dot(nominal, cross(a, e))),
        );
        let free = planes.len() == 1 && length(cross(normal, measured_a)) > 1e-9;
        Ok((normal, if free { vec![measured_a] } else { Vec::new() }))
    }

    fn position(
        &mut self,
        frame: &DrawingFeatureControlFrame,
        points: &[Vec3],
        tolerance: f64,
    ) -> Result<ControlMeasurement, String> {
        let limits = frame
            .size_limits
            .as_ref()
            .ok_or("position evaluation needs size limits to identify the mating envelope")?;
        if frame.datums.len() != 3 {
            return Err("position evaluation needs three plane datums".into());
        }
        let ResolvedDatum::Axis { origin, direction } = self.nominal(&frame.attachment.anchor)?
        else {
            return Err("position evaluation needs axis datum geometry".into());
        };
        let nominal_frame = self
            .resolved_frame(&frame.datums, None)?
            .nominal_planar_321()
            .map_err(|e| e.to_string())?;
        if length(cross(
            unit(direction).map_err(|e| e.to_string())?,
            nominal_frame.z_axis,
        )) > 1e-9
        {
            return Err(
                "position evaluation needs a feature axis normal to the primary datum".into(),
            );
        }
        let measured_frame = self.measured_frame(&frame.datums)?;
        let true_position = nominal_frame
            .coordinates_mm(origin)
            .map_err(|e| e.to_string())?;
        let projected = points
            .iter()
            .map(|&p| measured_frame.coordinates_mm(p).map(|c| [c.x, c.y]))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        // Internal features mate with the largest inscribed cylinder normal to
        // the primary datum; external ones with the smallest circumscribed one.
        let internal = limits.kind == FeatureOfSizeKind::Internal;
        let (mating, minimum_material) = if internal {
            (Fit::Inscribed, Fit::Circumscribed)
        } else {
            (Fit::Circumscribed, Fit::Inscribed)
        };
        let (lower, upper) = limits.bounds().map_err(|e| e.to_string())?;
        let (center, radius) = fit_circle(&projected, mating).map_err(|e| e.to_string())?;
        let size = 2.0 * radius;
        let (minimum_center, minimum_radius) =
            fit_circle(&projected, minimum_material).map_err(|e| e.to_string())?;
        let minimum_size = 2.0 * minimum_radius;
        let (center, bonus) = match frame.material {
            ToleranceMaterialCondition::Regardless => (center, 0.0),
            ToleranceMaterialCondition::Maximum => (
                center,
                if internal { size - lower } else { upper - size }.max(0.0),
            ),
            // Least material is located on the minimum material envelope axis.
            ToleranceMaterialCondition::Least => (
                minimum_center,
                if internal {
                    upper - minimum_size
                } else {
                    minimum_size - lower
                }
                .max(0.0),
            ),
        };
        let deviation = 2.0 * (center[0] - true_position.x).hypot(center[1] - true_position.y);
        let size_conforms = if internal {
            size >= lower && minimum_size <= upper
        } else {
            size <= upper && minimum_size >= lower
        };
        Ok(ControlMeasurement {
            deviation_mm: deviation,
            tolerance_mm: tolerance,
            bonus_mm: bonus,
            actual_size_mm: Some(size),
            size_conforms: Some(size_conforms),
            conforms: size_conforms && deviation <= tolerance + bonus,
        })
    }
}

/// Largest minus smallest value; zero for an empty sequence.
fn spread(values: impl Iterator<Item = f64>) -> f64 {
    let (lo, hi) = values.fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| {
        (lo.min(v), hi.max(v))
    });
    if lo.is_finite() { hi - lo } else { 0.0 }
}

/// Sorts readings by key and splits them where consecutive keys differ by
/// more than 1e-3 mm or 1e-6 of the key span.
fn elements(readings: &mut [(f64, f64)]) -> Vec<&[(f64, f64)]> {
    readings.sort_by(|a, b| a.0.total_cmp(&b.0));
    let span = readings.last().map_or(0.0, |r| r.0) - readings.first().map_or(0.0, |r| r.0);
    let gap = (1e-6 * span).max(1e-3);
    let mut out = Vec::new();
    let mut start = 0;
    for end in 1..=readings.len() {
        if end == readings.len() || readings[end].0 - readings[end - 1].0 > gap {
            out.push(&readings[start..end]);
            start = end;
        }
    }
    out
}

/// Two unit vectors completing a right-handed basis with unit `n`.
fn perpendiculars(n: Vec3) -> (Vec3, Vec3) {
    let seed = if n.x.abs() < 0.9 {
        Vec3::new(1.0, 0.0, 0.0)
    } else {
        Vec3::new(0.0, 1.0, 0.0)
    };
    let u = scale(cross(n, seed), 1.0 / length(cross(n, seed)));
    (u, cross(n, u))
}
