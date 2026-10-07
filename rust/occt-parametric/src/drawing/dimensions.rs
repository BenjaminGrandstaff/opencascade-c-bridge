//! Manufacturing dimension presentation; no GD&T or thread-fit certification.
use super::*;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum DimensionTolerance {
    #[default]
    None,
    Symmetric {
        deviation: Quantity,
    },
    /// Signed deviations from nominal; lower <= 0 <= upper.
    Deviations {
        lower: Quantity,
        upper: Quantity,
    },
    Limits {
        lower: Quantity,
        upper: Quantity,
    },
    Basic,
    Reference,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct DimensionPresentation {
    pub length_unit: LengthUnit,
    pub tolerance: DimensionTolerance,
    /// Optional live Hole feature supplying diameter, depth, recess and thread.
    pub hole: Option<InstanceOutputRef>,
}
impl Default for DimensionPresentation {
    fn default() -> Self {
        Self {
            length_unit: LengthUnit::Millimeter,
            tolerance: DimensionTolerance::None,
            hole: None,
        }
    }
}
impl DimensionPresentation {
    pub(super) fn is_default(&self) -> bool {
        self == &Self::default()
    }
}

/// Indexes each callout instance once: O(sum(features + parameters) + dimensions)
/// plus parameter-expression evaluation, with proportional index storage.
pub(super) struct DimensionContext<'a> {
    holes: HashMap<String, HoleContext<'a>>,
}
struct HoleContext<'a> {
    features: HashMap<&'a str, &'a FeatureDefinition>,
    parameters: HashMap<String, ParameterValue>,
}
impl<'a> DimensionContext<'a> {
    pub(super) fn new(
        dimensions: &[DrawingDimension],
        graph: &InstanceGraph<'a>,
    ) -> Result<Self, ModelError> {
        let mut holes = HashMap::new();
        for reference in dimensions
            .iter()
            .filter_map(|d| d.presentation.hole.as_ref())
        {
            if holes.contains_key(&reference.instance) {
                continue;
            }
            if graph.is_suppressed(&reference.instance) {
                return Err(ModelError::new("hole callout instance is suppressed"));
            }
            let instance = graph.resolve(&reference.instance)?;
            holes.insert(
                reference.instance.clone(),
                HoleContext {
                    features: instance
                        .definition
                        .features
                        .iter()
                        .map(|f| (f.id.as_str(), f))
                        .collect(),
                    parameters: resolve_parameters(instance.definition, &instance.overrides)?,
                },
            );
        }
        Ok(Self { holes })
    }
}

pub(super) fn require_coplanar(
    view: &DrawingView,
    graph: &InstanceGraph<'_>,
    refs: &[&DatumRef],
) -> Result<(), ModelError> {
    let frame = view.frame()?;
    // A radius/angle must not silently become a foreshortened projected value.
    for reference in refs {
        let delta = subtract(datum_origin(graph, reference)?, frame.origin);
        let distance = dot(delta, frame.direction).abs();
        let scale = delta.x.abs().max(delta.y.abs()).max(delta.z.abs()).max(1.0);
        if distance > 1e-7 + scale * f64::EPSILON * 32.0 {
            return Err(ModelError::new(
                "radial and angular dimension datums must lie in the view plane",
            ));
        }
    }
    Ok(())
}

pub(super) fn angular_geometry(
    d: &DrawingDimension,
    view: &DrawingView,
    graph: &InstanceGraph<'_>,
) -> Result<DimensionGeometry, ModelError> {
    let DimensionDirection::Angular { vertex } = &d.direction else {
        unreachable!()
    };
    require_coplanar(view, graph, &[&d.first, &d.second, vertex])?;
    let center = view.project(datum_origin(graph, vertex)?)?;
    let first = view.project(datum_origin(graph, &d.first)?)?;
    let second = view.project(datum_origin(graph, &d.second)?)?;
    let a = [first[0] - center[0], first[1] - center[1]];
    let b = [second[0] - center[0], second[1] - center[1]];
    let la = a[0].hypot(a[1]);
    let lb = b[0].hypot(b[1]);
    if !la.is_finite() || !lb.is_finite() || la <= 1e-12 || lb <= 1e-12 || d.offset_mm <= 0.0 {
        return Err(ModelError::new(
            "angular dimensions need nonzero rays and a positive paper arc radius",
        ));
    }
    let a = [a[0] / la, a[1] / la];
    let b = [b[0] / lb, b[1] / lb];
    let angle = (a[0] * b[1] - a[1] * b[0]).atan2(a[0] * b[0] + a[1] * b[1]);
    if angle.abs() <= 1e-12 {
        return Err(ModelError::new("angular dimension has zero angle"));
    }
    Ok((view.paper(center)?, a, b, angle))
}

fn display_value(d: &DrawingDimension, q: Quantity) -> Result<f64, ModelError> {
    let angular = matches!(d.direction, DimensionDirection::Angular { .. });
    if q.dimension
        != if angular {
            Dimension::Scalar
        } else {
            Dimension::Length
        }
    {
        return Err(ModelError::new(
            "dimension tolerance has incompatible units (angles use scalar radians)",
        ));
    }
    let value = q.normalized()?;
    let value = if angular {
        value.to_degrees()
    } else {
        value / d.presentation.length_unit.millimeter_factor()
    };
    if !value.is_finite() {
        return Err(ModelError::new("dimension tolerance exceeds finite limits"));
    }
    Ok(value)
}

fn tolerance_text(d: &DrawingDimension, nominal: f64) -> Result<String, ModelError> {
    let p = usize::from(d.precision);
    let text = match &d.presentation.tolerance {
        DimensionTolerance::None | DimensionTolerance::Basic => format!("{nominal:.p$}"),
        DimensionTolerance::Reference => format!("{nominal:.p$}"),
        DimensionTolerance::Symmetric { deviation } => {
            let v = display_value(d, *deviation)?;
            if v < 0.0 || nominal - v < 0.0 || !(nominal + v).is_finite() {
                return Err(ModelError::new("invalid symmetric dimension tolerance"));
            }
            format!("{nominal:.p$} ±{v:.p$}")
        }
        DimensionTolerance::Deviations { lower, upper } => {
            let lo = display_value(d, *lower)?;
            let hi = display_value(d, *upper)?;
            if lo > 0.0 || hi < 0.0 || nominal + lo < 0.0 || !(nominal + hi).is_finite() {
                return Err(ModelError::new("invalid signed dimension deviations"));
            }
            format!("{nominal:.p$} {hi:+.p$}/{lo:+.p$}")
        }
        DimensionTolerance::Limits { lower, upper } => {
            let lo = display_value(d, *lower)?;
            let hi = display_value(d, *upper)?;
            if lo < 0.0 || lo > nominal || hi < nominal || hi < lo {
                return Err(ModelError::new("dimension limits must contain nominal"));
            }
            format!("{hi:.p$}/{lo:.p$}")
        }
    };
    Ok(text)
}

fn unit_text(unit: LengthUnit) -> &'static str {
    match unit {
        LengthUnit::Millimeter => "mm",
        LengthUnit::Centimeter => "cm",
        LengthUnit::Meter => "m",
        LengthUnit::Inch => "in",
    }
}

pub(super) fn label(
    d: &DrawingDimension,
    value: f64,
    context: &DimensionContext<'_>,
) -> Result<String, ModelError> {
    let angular = matches!(d.direction, DimensionDirection::Angular { .. });
    let (value, suffix) = if angular {
        (value.abs().to_degrees(), "°")
    } else {
        (
            value / d.presentation.length_unit.millimeter_factor(),
            unit_text(d.presentation.length_unit),
        )
    };
    let prefix = match d.direction {
        DimensionDirection::Radius => "R",
        DimensionDirection::Diameter => "Ø",
        _ => "",
    };
    let mut text = if let Some(hole) = &d.presentation.hole {
        hole_label(d, hole, context)?
    } else {
        format!("{prefix}{} {suffix}", tolerance_text(d, value)?)
    };
    if matches!(d.presentation.tolerance, DimensionTolerance::Reference) {
        text = format!("({text})");
    }
    validate_text(&text)?;
    Ok(text)
}

pub(super) fn validate(
    d: &DrawingDimension,
    view: &DrawingView,
    graph: &InstanceGraph<'_>,
    context: &DimensionContext<'_>,
) -> Result<(), ModelError> {
    let (_, _, _, value) = dimension_geometry(d, view, graph)?;
    label(d, value, context)?;
    if d.presentation.hole.is_some() && !matches!(d.direction, DimensionDirection::Diameter) {
        return Err(ModelError::new(
            "hole callouts require a diameter dimension",
        ));
    }
    Ok(())
}

fn hole_label(
    d: &DrawingDimension,
    reference: &InstanceOutputRef,
    context: &DimensionContext<'_>,
) -> Result<String, ModelError> {
    let source = context
        .holes
        .get(&reference.instance)
        .ok_or_else(|| ModelError::new("hole callout instance is unknown"))?;
    let feature = source
        .features
        .get(reference.output.as_str())
        .ok_or_else(|| ModelError::new("hole callout feature is unknown"))?;
    let FeatureOperation::Hole {
        diameter,
        extent,
        finish,
        thread,
        ..
    } = &feature.operation
    else {
        return Err(ModelError::new(
            "hole callout must reference a Hole feature",
        ));
    };
    let params = &source.parameters;
    let factor = d.presentation.length_unit.millimeter_factor();
    let eval = |expr: &ScalarExpr, dimension: Dimension| -> Result<f64, ModelError> {
        let result = evaluate_resolved_expression(expr, params)?;
        if result.dimension != dimension || !result.value.is_finite() || result.value <= 0.0 {
            return Err(ModelError::new(
                "hole callout requires positive values with compatible units",
            ));
        }
        Ok(if dimension == Dimension::Length {
            result.value / factor
        } else {
            result.value.to_degrees()
        })
    };
    let p = usize::from(d.precision);
    let unit = unit_text(d.presentation.length_unit);
    let mut text = format!(
        "Ø{} {unit}",
        tolerance_text(d, eval(diameter, Dimension::Length)?)?
    );
    match extent {
        HoleExtent::ThroughAll => text.push_str(" THRU"),
        HoleExtent::Blind { depth } => text.push_str(&format!(
            " DEPTH {:.p$} {unit}",
            eval(depth, Dimension::Length)?
        )),
    }
    match finish {
        HoleFinish::Plain => {}
        HoleFinish::Counterbore { diameter, depth } => text.push_str(&format!(
            "; CBORE Ø{:.p$} DEPTH {:.p$} {unit}",
            eval(diameter, Dimension::Length)?,
            eval(depth, Dimension::Length)?
        )),
        HoleFinish::Countersink {
            diameter,
            angle_radians,
        } => text.push_str(&format!(
            "; CSINK Ø{:.p$} {unit} x {:.p$}°",
            eval(diameter, Dimension::Length)?,
            eval(angle_radians, Dimension::Scalar)?
        )),
    }
    if let Some(thread) = thread {
        validate_text(&thread.designation)?;
        if thread.designation.trim().is_empty() {
            return Err(ModelError::new("thread designation is empty"));
        }
        let hand = if thread.handedness == ThreadHandedness::Left {
            "LH"
        } else {
            "RH"
        };
        text.push_str(&format!(
            "; THREAD {} ({:.p$} x {:.p$} {unit}, {hand})",
            thread.designation,
            eval(&thread.nominal_diameter, Dimension::Length)?,
            eval(&thread.pitch, Dimension::Length)?
        ));
    }
    Ok(text)
}

fn line(drawing: &mut GeneratedDrawing, points: Vec<[f64; 2]>) -> Result<(), ModelError> {
    if points.iter().any(|p| !finite_pair(*p)) {
        return Err(ModelError::new(
            "dimension exceeds finite paper coordinates",
        ));
    }
    drawing.polylines.push(DrawingPolyline {
        points_mm: points,
        hidden: false,
    });
    Ok(())
}

pub(super) fn append_special(
    d: &DrawingDimension,
    view: &DrawingView,
    graph: &InstanceGraph<'_>,
    drawing: &mut GeneratedDrawing,
    context: &DimensionContext<'_>,
) -> Result<(), ModelError> {
    let (center, a, b, value) = dimension_geometry(d, view, graph)?;
    if matches!(d.direction, DimensionDirection::Angular { .. }) {
        append_angular(d, center, a, b, value, drawing)?;
    } else {
        // Linear/radial geometry stores endpoint before direction; angular
        // geometry stores two unit rays instead.
        let (a, b) = (b, a);
        let radius = (b[0] - center[0]).hypot(b[1] - center[1]);
        let start = if matches!(d.direction, DimensionDirection::Diameter) {
            [center[0] - a[0] * radius, center[1] - a[1] * radius]
        } else {
            center
        };
        line(drawing, vec![start, b])?;
        let arrow_start = drawing.polylines.len();
        append_arrows(start, b, drawing)?;
        if matches!(d.direction, DimensionDirection::Radius) {
            drawing.polylines.drain(arrow_start..arrow_start + 2);
        }
        let normal = [-a[1], a[0]];
        let endpoint = [b[0] + a[0] * d.offset_mm, b[1] + a[1] * d.offset_mm];
        line(drawing, vec![b, endpoint])?;
        drawing.labels.push(DrawingLabel {
            position_mm: [endpoint[0] + normal[0] * 2.0, endpoint[1] + normal[1] * 2.0],
            text: String::new(),
            stack: None,
        });
    }
    drawing.labels.last_mut().expect("dimension label").text = label(d, value, context)?;
    stack_label(d, drawing);
    decorate_basic(d, drawing)
}

fn append_angular(
    d: &DrawingDimension,
    center: [f64; 2],
    a: [f64; 2],
    b: [f64; 2],
    angle: f64,
    drawing: &mut GeneratedDrawing,
) -> Result<(), ModelError> {
    let start = a[1].atan2(a[0]);
    let r = d.offset_mm;
    let point = |theta: f64| [center[0] + r * theta.cos(), center[1] + r * theta.sin()];
    line(
        drawing,
        (0..=64)
            .map(|i| point(start + angle * f64::from(i) / 64.0))
            .collect(),
    )?;
    for ray in [a, b] {
        line(
            drawing,
            vec![
                center,
                [
                    center[0] + ray[0] * (r + 2.0),
                    center[1] + ray[1] * (r + 2.0),
                ],
            ],
        )?;
    }
    // Arrow wings follow tangents into the measured minor arc.
    for (theta, sign) in [(start, angle.signum()), (start + angle, -angle.signum())] {
        let tip = point(theta);
        let along = [-theta.sin() * sign, theta.cos() * sign];
        for side in [-1.0, 1.0] {
            line(
                drawing,
                vec![
                    tip,
                    [
                        tip[0] + 2.0 * along[0] - side * 0.7 * along[1],
                        tip[1] + 2.0 * along[1] + side * 0.7 * along[0],
                    ],
                ],
            )?;
        }
    }
    let mid = start + angle / 2.0;
    drawing.labels.push(DrawingLabel {
        position_mm: [
            center[0] + (r + 3.0) * mid.cos(),
            center[1] + (r + 3.0) * mid.sin(),
        ],
        text: String::new(),
        stack: None,
    });
    Ok(())
}

pub(super) fn decorate_basic(
    d: &DrawingDimension,
    drawing: &mut GeneratedDrawing,
) -> Result<(), ModelError> {
    if matches!(d.presentation.tolerance, DimensionTolerance::Basic) {
        let label = drawing.labels.last().expect("dimension label");
        let [x, y] = label.position_mm;
        let width = label.text.chars().count() as f64 * 2.0 + 2.0;
        line(
            drawing,
            vec![
                [x - 1.0, y - 1.0],
                [x + width, y - 1.0],
                [x + width, y + 4.0],
                [x - 1.0, y + 4.0],
                [x - 1.0, y - 1.0],
            ],
        )?;
    }
    Ok(())
}

/// Keep the flat text for consumers; exporters place tolerance/limit values stacked.
pub(super) fn stack_label(d: &DrawingDimension, drawing: &mut GeneratedDrawing) {
    let label = drawing.labels.last_mut().expect("dimension label");
    label.stack = dimension_stack(d, &label.text);
}
fn dimension_stack(d: &DrawingDimension, text: &str) -> Option<DrawingDimensionStack> {
    let (first, rest) = text.split_once(' ')?;
    let (prefix, upper, lower, suffix) = match d.presentation.tolerance {
        DimensionTolerance::Deviations { .. } => {
            let (values, suffix) = rest.split_once(' ')?;
            let (upper, lower) = values.split_once('/')?;
            (
                first.to_owned(),
                upper.to_owned(),
                lower.to_owned(),
                suffix.to_owned(),
            )
        }
        DimensionTolerance::Limits { .. } => {
            let (upper, lower) = first.split_once('/')?;
            let (prefix, upper) = if let Some(value) = upper.strip_prefix('Ø') {
                ("Ø", value)
            } else if let Some(value) = upper.strip_prefix('R') {
                ("R", value)
            } else {
                ("", upper)
            };
            (
                prefix.to_owned(),
                upper.to_owned(),
                lower.to_owned(),
                rest.to_owned(),
            )
        }
        _ => return None,
    };
    Some(DrawingDimensionStack {
        prefix,
        upper,
        lower,
        suffix,
    })
}
