//! Typed sketch edits. Stable entity names and expected values guard every
//! mutation; operations run sequentially on a private candidate sketch.
use super::*;

#[derive(Clone, PartialEq, Deserialize, serde::Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SketchElement {
    Point(SketchPoint),
    Line(SketchLine),
    Circle(SketchCircle),
    Arc(SketchArc),
    Ellipse(SketchEllipse),
    Spline(SketchSpline),
    Shape(SketchShape),
    Text(SketchText),
    Pattern(SketchPattern),
    Projection(SketchProjection),
    Measurement(SketchMeasurement),
}
impl SketchElement {
    fn mutate(
        &self,
        sketch: &mut SketchDefinition,
        expected: Option<&Self>,
        remove: bool,
    ) -> Result<(), Failure> {
        match self {
            Self::Point(value) => {
                let old = match expected {
                    Some(Self::Point(old)) => Some(old),
                    None => None,
                    _ => return Err(failure("patch", "sketch element kind must stay the same")),
                };
                mutate(&mut sketch.points, value, old, remove, |v: &SketchPoint| {
                    v.id.as_str()
                })
            }
            Self::Line(value) => {
                let old = match expected {
                    Some(Self::Line(old)) => Some(old),
                    None => None,
                    _ => return Err(failure("patch", "sketch element kind must stay the same")),
                };
                mutate(&mut sketch.lines, value, old, remove, |v: &SketchLine| {
                    v.id.as_str()
                })
            }
            Self::Circle(value) => {
                let old = match expected {
                    Some(Self::Circle(old)) => Some(old),
                    None => None,
                    _ => return Err(failure("patch", "sketch element kind must stay the same")),
                };
                mutate(
                    &mut sketch.circles,
                    value,
                    old,
                    remove,
                    |v: &SketchCircle| v.id.as_str(),
                )
            }
            Self::Arc(value) => {
                let old = match expected {
                    Some(Self::Arc(old)) => Some(old),
                    None => None,
                    _ => return Err(failure("patch", "sketch element kind must stay the same")),
                };
                mutate(&mut sketch.arcs, value, old, remove, |v: &SketchArc| {
                    v.id.as_str()
                })
            }
            Self::Ellipse(value) => {
                let old = match expected {
                    Some(Self::Ellipse(old)) => Some(old),
                    None => None,
                    _ => return Err(failure("patch", "sketch element kind must stay the same")),
                };
                mutate(
                    &mut sketch.ellipses,
                    value,
                    old,
                    remove,
                    |v: &SketchEllipse| v.id.as_str(),
                )
            }
            Self::Spline(value) => {
                let old = match expected {
                    Some(Self::Spline(old)) => Some(old),
                    None => None,
                    _ => return Err(failure("patch", "sketch element kind must stay the same")),
                };
                mutate(
                    &mut sketch.splines,
                    value,
                    old,
                    remove,
                    |v: &SketchSpline| v.id.as_str(),
                )
            }
            Self::Shape(value) => {
                let old = match expected {
                    Some(Self::Shape(old)) => Some(old),
                    None => None,
                    _ => return Err(failure("patch", "sketch element kind must stay the same")),
                };
                mutate(&mut sketch.shapes, value, old, remove, |v: &SketchShape| {
                    v.id()
                })
            }
            Self::Text(value) => {
                let old = match expected {
                    Some(Self::Text(old)) => Some(old),
                    None => None,
                    _ => return Err(failure("patch", "sketch element kind must stay the same")),
                };
                mutate(&mut sketch.texts, value, old, remove, |v: &SketchText| {
                    v.id.as_str()
                })
            }
            Self::Pattern(value) => {
                let old = match expected {
                    Some(Self::Pattern(old)) => Some(old),
                    None => None,
                    _ => return Err(failure("patch", "sketch element kind must stay the same")),
                };
                mutate(
                    &mut sketch.patterns,
                    value,
                    old,
                    remove,
                    |v: &SketchPattern| v.id(),
                )
            }
            Self::Projection(value) => {
                let old = match expected {
                    Some(Self::Projection(old)) => Some(old),
                    None => None,
                    _ => return Err(failure("patch", "sketch element kind must stay the same")),
                };
                mutate(
                    &mut sketch.projections,
                    value,
                    old,
                    remove,
                    |v: &SketchProjection| v.id.as_str(),
                )
            }
            Self::Measurement(value) => {
                let old = match expected {
                    Some(Self::Measurement(old)) => Some(old),
                    None => None,
                    _ => return Err(failure("patch", "sketch element kind must stay the same")),
                };
                mutate(
                    &mut sketch.measurements,
                    value,
                    old,
                    remove,
                    |v: &SketchMeasurement| v.id.as_str(),
                )
            }
        }
    }
}
fn mutate<T: Clone + PartialEq>(
    values: &mut Vec<T>,
    value: &T,
    expected: Option<&T>,
    remove: bool,
    id: impl Fn(&T) -> &str,
) -> Result<(), Failure> {
    let name = id(value);
    if name.is_empty() || expected.is_some_and(|old| id(old) != name) {
        return Err(failure(
            "patch",
            "sketch element edits need nonempty unchanged IDs",
        ));
    }
    let index = values.iter().position(|v| id(v) == name);
    match (index, expected) {
        (None, None) if !remove => values.push(value.clone()),
        (Some(i), Some(old)) if &values[i] == old => {
            if remove {
                values.remove(i);
            } else {
                values[i] = value.clone();
            }
        }
        _ => {
            return Err(failure(
                "conflict",
                format!("sketch element '{name}' differs from expected snapshot"),
            ));
        }
    }
    Ok(())
}

/// Non-entity sketch settings. Replacing them cannot silently replace points,
/// constraints, linked projections or patterns.
#[derive(Clone, PartialEq, Deserialize, serde::Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SketchConfiguration {
    pub origin: VectorExpr,
    pub x_axis: VectorExpr,
    pub y_axis: VectorExpr,
    pub datum_plane: Option<String>,
    pub face_support: Option<SketchFaceSupport>,
    pub acceptance: SketchAcceptance,
    pub profile: Vec<String>,
    pub holes: Vec<Vec<String>>,
    pub regions: Vec<SketchRegion>,
    pub region_discovery: Option<SketchRegionDiscovery>,
    pub profile_operations: Vec<SketchProfileOperation>,
    pub curve_operations: Vec<SketchCurveOperation>,
    pub region_edits: Vec<SketchRegionEdit>,
}
impl From<&SketchDefinition> for SketchConfiguration {
    fn from(s: &SketchDefinition) -> Self {
        Self {
            origin: s.origin.clone(),
            x_axis: s.x_axis.clone(),
            y_axis: s.y_axis.clone(),
            datum_plane: s.datum_plane.clone(),
            face_support: s.face_support.clone(),
            acceptance: s.acceptance,
            profile: s.profile.clone(),
            holes: s.holes.clone(),
            regions: s.regions.clone(),
            region_discovery: s.region_discovery.clone(),
            profile_operations: s.profile_operations.clone(),
            curve_operations: s.curve_operations.clone(),
            region_edits: s.region_edits.clone(),
        }
    }
}
impl SketchConfiguration {
    fn assign(&self, s: &mut SketchDefinition) {
        s.origin = self.origin.clone();
        s.x_axis = self.x_axis.clone();
        s.y_axis = self.y_axis.clone();
        s.datum_plane = self.datum_plane.clone();
        s.face_support = self.face_support.clone();
        s.acceptance = self.acceptance;
        s.profile = self.profile.clone();
        s.holes = self.holes.clone();
        s.regions = self.regions.clone();
        s.region_discovery = self.region_discovery.clone();
        s.profile_operations = self.profile_operations.clone();
        s.curve_operations = self.curve_operations.clone();
        s.region_edits = self.region_edits.clone();
    }
}

#[derive(Deserialize, serde::Serialize, schemars::JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum SketchChange {
    UpsertElement {
        expected: Option<Box<SketchElement>>,
        element: Box<SketchElement>,
    },
    RemoveElement {
        expected: Box<SketchElement>,
    },
    AddConstraint {
        constraint: Box<SketchConstraint>,
    },
    ReplaceConstraint {
        index: usize,
        expected: Box<SketchConstraint>,
        constraint: Box<SketchConstraint>,
    },
    RemoveConstraint {
        index: usize,
        expected: Box<SketchConstraint>,
    },
    Configure {
        expected: Box<SketchConfiguration>,
        configuration: Box<SketchConfiguration>,
    },
}
pub(super) fn apply(
    sketch: &mut SketchDefinition,
    changes: &[SketchChange],
) -> Result<(), Failure> {
    if changes.is_empty() || changes.len() > 10000 {
        return Err(failure("patch", "sketch editing needs 1..10000 changes"));
    }
    for (position, change) in changes.iter().enumerate() {
        let result = match change {
            SketchChange::UpsertElement { expected, element } => {
                element.mutate(sketch, expected.as_deref(), false)
            }
            SketchChange::RemoveElement { expected } => {
                expected.mutate(sketch, Some(expected.as_ref()), true)
            }
            SketchChange::AddConstraint { constraint } => {
                sketch.constraints.push(constraint.as_ref().clone());
                Ok(())
            }
            SketchChange::ReplaceConstraint {
                index,
                expected,
                constraint,
            } => {
                if sketch.constraints.get(*index) != Some(expected.as_ref()) {
                    Err(failure(
                        "conflict",
                        format!("constraint {index} differs from expected snapshot"),
                    ))
                } else {
                    sketch.constraints[*index] = constraint.as_ref().clone();
                    Ok(())
                }
            }
            SketchChange::RemoveConstraint { index, expected } => {
                if sketch.constraints.get(*index) != Some(expected.as_ref()) {
                    Err(failure(
                        "conflict",
                        format!("constraint {index} differs from expected snapshot"),
                    ))
                } else {
                    sketch.constraints.remove(*index);
                    Ok(())
                }
            }
            SketchChange::Configure {
                expected,
                configuration,
            } => {
                if &SketchConfiguration::from(&*sketch) != expected.as_ref() {
                    Err(failure(
                        "conflict",
                        "sketch configuration differs from expected snapshot",
                    ))
                } else {
                    configuration.assign(sketch);
                    Ok(())
                }
            }
        };
        result.map_err(|e| failure(e.stage, format!("sketch change {position}: {}", e.message)))?;
    }
    Ok(())
}
