//! Measured inspection: datum simulators fitted to measured points and
//! conformance of planar form/orientation and axis position controls.
//!
//! Measured points are in model coordinates at the current poses, as from a
//! coordinate measuring machine aligned to the part; the residual misalignment
//! must be small because fits start from the nominal orientations. Nominal
//! datum plane normals point out of the material, so each datum simulator
//! contacts the measured high points on that side (constrained L∞).
use super::*;
use crate::assembly::{add, length, scale, unit};
mod fit;
use fit::{Fit, fit_circle, fit_plane};

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
}

#[derive(Clone, Debug, PartialEq)]
pub struct ControlMeasurement {
    /// Width of the measured zone: plane separation, or twice the radial axis offset.
    pub deviation_mm: f64,
    pub tolerance_mm: f64,
    /// Material-condition bonus from the measured size; zero regardless of size.
    pub bonus_mm: f64,
    /// Actual mating size of a feature of size, when evaluated.
    pub actual_size_mm: Option<f64>,
    pub size_conforms: Option<bool>,
    pub conforms: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ControlResult {
    Evaluated(ControlMeasurement),
    NotMeasured,
    /// The control or its measurements are outside what this evaluator supports.
    NotEvaluated(String),
}

#[derive(Clone, Debug, PartialEq)]
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
    /// One entry per effective feature control frame, in drawing order.
    pub controls: Vec<ControlInspection>,
}

impl InspectionReport {
    /// True only when every control was evaluated and conforms.
    pub fn conforms(&self) -> bool {
        self.controls
            .iter()
            .all(|c| matches!(&c.result, ControlResult::Evaluated(m) if m.conforms))
    }
}

type Simulators = Vec<(Vec3, Vec3)>;

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
            }
        }
        Ok(InspectionReport {
            drawing: self.id.clone(),
            datum_frames,
            controls,
        })
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
        let mut planes = self.simulators(prefix)?;
        if last.boundary != DatumMaterialBoundary::Regardless {
            return Err("datum shift at a material boundary is not evaluated".into());
        }
        let id = last.datum_feature.as_str();
        let points = self
            .measured_datums
            .get(id)
            .ok_or_else(|| format!("datum feature '{id}' was not measured"))?;
        let (_, nominal) = self.nominal_datum(id)?;
        let (normal, axes) = match planes.as_slice() {
            [] => {
                let (u, v) = perpendiculars(nominal);
                (nominal, vec![u, v])
            }
            [(_, primary)] => (
                unit(subtract(nominal, scale(*primary, dot(nominal, *primary))))
                    .map_err(|_| "secondary datum plane is parallel to the primary".to_string())?,
                vec![*primary],
            ),
            [(_, primary), (_, secondary), ..] => {
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
        planes.push((scale(fitted.normal, fitted.contact), fitted.normal));
        Ok(planes)
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
                    Some(p) => ResolvedDatum::Plane {
                        origin: p[i].0,
                        normal: p[i].1,
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
        let measured_a = planes[0].1;
        let reference = match datums.get(1) {
            Some(d) => self.nominal_datum(&d.datum_feature)?.1,
            None => nominal,
        };
        let e = unit(subtract(reference, scale(a, dot(reference, a))))
            .unwrap_or_else(|_| perpendiculars(a).0);
        let measured_e = match planes.get(1) {
            Some(&(_, n)) => n,
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
        let (mating, minimum_material) = if limits.internal {
            (Fit::Inscribed, Fit::Circumscribed)
        } else {
            (Fit::Circumscribed, Fit::Inscribed)
        };
        let (lower, upper) = limits.millimeters().map_err(|e| e.to_string())?;
        let (center, radius) = fit_circle(&projected, mating).map_err(|e| e.to_string())?;
        let size = 2.0 * radius;
        let (minimum_center, minimum_radius) =
            fit_circle(&projected, minimum_material).map_err(|e| e.to_string())?;
        let minimum_size = 2.0 * minimum_radius;
        let (center, bonus) = match frame.material {
            ToleranceMaterialCondition::Regardless => (center, 0.0),
            ToleranceMaterialCondition::Maximum => (
                center,
                if limits.internal {
                    size - lower
                } else {
                    upper - size
                }
                .max(0.0),
            ),
            // Least material is located on the minimum material envelope axis.
            ToleranceMaterialCondition::Least => (
                minimum_center,
                if limits.internal {
                    upper - minimum_size
                } else {
                    minimum_size - lower
                }
                .max(0.0),
            ),
        };
        let deviation = 2.0 * (center[0] - true_position.x).hypot(center[1] - true_position.y);
        let size_conforms = if limits.internal {
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
