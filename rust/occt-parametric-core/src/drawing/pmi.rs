//! Drawing GD&T as semantic product manufacturing information (schema 144):
//! which datums, tolerances and sizes a STEP AP242 export carries on the
//! faces their attachments name, and the report of what read back.
use super::*;
use gdt::composite;

/// What a PMI item is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PmiItemKind {
    Datum,
    Tolerance,
    Size,
}

/// A part of a PMI item that an exchange did not carry.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum PmiProperty {
    /// The whole item: nothing of its kind was read on its faces.
    Item,
    Value,
    /// Diameter or plain zone.
    Zone,
    Material,
    Modifiers,
    ProjectedZone,
    Datums,
    ContinuousFeature,
    DynamicProfile,
    UnequalZone,
    /// How a composite's lower segment relates to the segments above it; the
    /// segment itself is written as a tolerance of its own.
    CompositeRelation,
}

/// One exported PMI item: `unrepresented` lists what the exchange format
/// cannot hold, `lost` what it should hold but did not read back.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PmiExchangeItem {
    /// The datum feature or feature control frame, or the first member of a
    /// pattern.
    pub id: String,
    pub kind: PmiItemKind,
    /// Composite segment, 0 for the top; always 0 for datums and sizes.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub segment: usize,
    pub instance: String,
    pub faces: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unrepresented: Vec<PmiProperty>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lost: Vec<PmiProperty>,
}

fn is_zero(value: &usize) -> bool {
    *value == 0
}

/// An annotation left out of the exchange, and why.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PmiSkipped {
    pub id: String,
    pub reason: String,
}

/// What a semantic PMI export wrote and what reading it back recovered.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PmiExchangeReport {
    pub parts: usize,
    pub items: Vec<PmiExchangeItem>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skipped: Vec<PmiSkipped>,
}

impl PmiExchangeReport {
    /// Every exported item read back with nothing lost.
    pub fn verified(&self) -> bool {
        self.items.iter().all(|item| item.lost.is_empty())
    }

    /// Verified, and nothing was skipped or left unrepresented.
    pub fn complete(&self) -> bool {
        self.verified()
            && self.skipped.is_empty()
            && self.items.iter().all(|item| item.unrepresented.is_empty())
    }
}

/// Faces of one output: the union of the selectors' faces.
#[derive(Clone, Debug, PartialEq)]
pub struct PmiTarget<'a> {
    pub output: &'a InstanceOutputRef,
    pub faces: Vec<&'a FaceSelector>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlannedDatum<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub target: PmiTarget<'a>,
}

/// A datum reference: an index into [`PmiPlan::datums`] in compartment 0
/// (primary), 1 or 2.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlannedReference {
    pub datum: usize,
    pub compartment: u32,
    pub boundary: DatumMaterialBoundary,
    pub translation: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlannedTolerance<'a> {
    pub id: &'a str,
    pub segment: usize,
    pub target: PmiTarget<'a>,
    pub characteristic: GeometricCharacteristic,
    pub value_mm: f64,
    pub diameter: bool,
    pub material: ToleranceMaterialCondition,
    pub free_state: bool,
    pub statistical: bool,
    pub tangent_plane: bool,
    pub projected_mm: Option<f64>,
    pub datums: Vec<PlannedReference>,
    pub unrepresented: Vec<PmiProperty>,
}

/// Size limits of a feature of size; its kind (diameter or width) follows
/// from its faces.
#[derive(Clone, Debug, PartialEq)]
pub struct PlannedSize<'a> {
    pub id: &'a str,
    pub target: PmiTarget<'a>,
    pub lower_mm: f64,
    pub upper_mm: f64,
}

/// The drawing GD&T a PMI export writes, in drawing order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PmiPlan<'a> {
    pub datums: Vec<PlannedDatum<'a>>,
    pub tolerances: Vec<PlannedTolerance<'a>>,
    pub sizes: Vec<PlannedSize<'a>>,
    pub skipped: Vec<PmiSkipped>,
}

fn skip(skipped: &mut Vec<PmiSkipped>, id: &str, reason: impl Into<String>) {
    skipped.push(PmiSkipped {
        id: id.to_owned(),
        reason: reason.into(),
    });
}

fn limits_mm(limits: &DrawingSizeLimits) -> Result<(f64, f64), ModelError> {
    Ok((limits.lower.normalized()?, limits.upper.normalized()?))
}

impl DrawingDefinition {
    /// The datum features and feature control frames whose attachments name
    /// `faces` on an output for which `exported` holds, as PMI on those
    /// faces. Pattern members on one output become one tolerance over all
    /// their faces; each composite segment becomes a tolerance of its own.
    /// The rest are skipped with a reason: no faces, an output not exported,
    /// or a datum reference that is skipped or lies on another output.
    /// O(features + frames · segments).
    pub fn pmi_plan<'a>(
        &'a self,
        graph: &InstanceGraph<'_>,
        exported: impl Fn(&InstanceOutputRef) -> bool,
    ) -> Result<PmiPlan<'a>, ModelError> {
        self.validate(graph)?;
        let mut plan = PmiPlan::default();
        let mut datum_index = HashMap::new();
        for feature in &self.datum_features {
            let attachment = &feature.attachment;
            let Some(faces) = &attachment.faces else {
                skip(&mut plan.skipped, &feature.id, "no persistent faces");
                continue;
            };
            if !exported(&attachment.output) {
                skip(&mut plan.skipped, &feature.id, "output not exported");
                continue;
            }
            let target = PmiTarget {
                output: &attachment.output,
                faces: vec![faces],
            };
            datum_index.insert(feature.id.as_str(), plan.datums.len());
            plan.datums.push(PlannedDatum {
                id: &feature.id,
                label: &feature.label,
                target: target.clone(),
            });
            if let Some(limits) = &feature.size_limits {
                let (lower_mm, upper_mm) = limits_mm(limits)?;
                plan.sizes.push(PlannedSize {
                    id: &feature.id,
                    target,
                    lower_mm,
                    upper_mm,
                });
            }
        }
        let resolved =
            gdt::effective_frames(&self.feature_control_frames, &self.datum_reference_frames)?;
        // Pattern members on one output, in drawing order of their first.
        let mut groups: Vec<Vec<usize>> = Vec::new();
        let mut grouped: HashMap<(&str, &InstanceOutputRef), usize> = HashMap::new();
        for (index, frame) in self.feature_control_frames.iter().enumerate() {
            match &frame.pattern {
                Some(pattern) => {
                    let key = (pattern.as_str(), &frame.attachment.output);
                    match grouped.get(&key) {
                        Some(&group) => groups[group].push(index),
                        None => {
                            grouped.insert(key, groups.len());
                            groups.push(vec![index]);
                        }
                    }
                }
                None => groups.push(vec![index]),
            }
        }
        for members in groups {
            let frames: Vec<&DrawingFeatureControlFrame> = members
                .iter()
                .map(|&i| &self.feature_control_frames[i])
                .collect();
            let first = frames[0];
            let attachment = &first.attachment;
            let missing = frames.iter().find(|f| f.attachment.faces.is_none());
            let reason = if let Some(member) = missing {
                Some(if frames.len() > 1 {
                    format!("pattern member '{}' has no persistent faces", member.id)
                } else {
                    "no persistent faces".to_owned()
                })
            } else if !exported(&attachment.output) {
                Some("output not exported".to_owned())
            } else {
                None
            };
            if let Some(reason) = reason {
                for frame in &frames {
                    skip(&mut plan.skipped, &frame.id, reason.clone());
                }
                continue;
            }
            let target = PmiTarget {
                output: &attachment.output,
                faces: frames
                    .iter()
                    .filter_map(|f| f.attachment.faces.as_ref())
                    .collect(),
            };
            let segments = composite::segments(&resolved[members[0]]);
            let mut tolerances = Vec::with_capacity(segments.len());
            let mut problem = None;
            for (segment, control) in segments.iter().enumerate() {
                let mut datums = Vec::new();
                for (compartment, reference) in control.datums.iter().enumerate() {
                    for member in reference.members() {
                        let Some(&datum) = datum_index.get(member) else {
                            problem = Some(format!("datum feature '{member}' is not exported"));
                            break;
                        };
                        if plan.datums[datum].target.output != &attachment.output {
                            problem =
                                Some(format!("datum feature '{member}' lies on another output"));
                            break;
                        }
                        datums.push(PlannedReference {
                            datum,
                            compartment: compartment as u32,
                            boundary: reference.boundary,
                            translation: reference.translation,
                        });
                    }
                }
                let mut unrepresented = Vec::new();
                for modifier in &control.modifiers {
                    match modifier {
                        ToleranceModifier::ContinuousFeature => {
                            unrepresented.push(PmiProperty::ContinuousFeature)
                        }
                        ToleranceModifier::DynamicProfile => {
                            unrepresented.push(PmiProperty::DynamicProfile)
                        }
                        ToleranceModifier::FreeState | ToleranceModifier::Statistical => {}
                    }
                }
                if control.unequal.is_some() {
                    unrepresented.push(PmiProperty::UnequalZone);
                }
                if segment > 0 {
                    unrepresented.push(PmiProperty::CompositeRelation);
                }
                unrepresented.sort();
                tolerances.push(PlannedTolerance {
                    id: &first.id,
                    segment,
                    target: target.clone(),
                    characteristic: control.characteristic,
                    value_mm: control.tolerance.normalized()?,
                    diameter: control.zone == GeometricToleranceZone::Diameter,
                    material: control.material,
                    free_state: control.modifiers.contains(&ToleranceModifier::FreeState),
                    statistical: control.modifiers.contains(&ToleranceModifier::Statistical),
                    tangent_plane: control.tangent_plane,
                    projected_mm: control
                        .projected
                        .as_ref()
                        .map(|p| p.height.normalized())
                        .transpose()?,
                    datums,
                    unrepresented,
                });
            }
            if let Some(reason) = problem {
                for frame in &frames {
                    skip(&mut plan.skipped, &frame.id, reason.clone());
                }
                continue;
            }
            plan.tolerances.extend(tolerances);
            if let Some(limits) = &first.size_limits {
                let (lower_mm, upper_mm) = limits_mm(limits)?;
                plan.sizes.push(PlannedSize {
                    id: &first.id,
                    target,
                    lower_mm,
                    upper_mm,
                });
            }
        }
        Ok(plan)
    }
}
