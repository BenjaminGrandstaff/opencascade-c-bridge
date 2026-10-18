//! Measured Unified threads against their class's limits of size.
use super::*;
use crate::screw_threads::{UnifiedLimits, UnifiedThread, UnifiedThreadSpec};

/// Diameters measured on a modeled thread or tapped hole that names a
/// Unified class: the pitch diameter (for example by the three-wire method)
/// and optionally the major and minor diameters, in millimetres.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MeasuredThread {
    pub instance: String,
    /// The `thread` feature, or the `hole` feature whose callout is threaded.
    pub feature: String,
    pub pitch_diameter_mm: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub major_diameter_mm: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minor_diameter_mm: Option<f64>,
}

/// Which diameter of a thread a check is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadDiameter {
    Major,
    Pitch,
    Minor,
}

/// One measured diameter against its limits; a limit B1.1 does not set is
/// absent.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ThreadDiameterCheck {
    pub diameter: ThreadDiameter,
    pub measured_mm: f64,
    pub minimum_mm: Option<f64>,
    pub maximum_mm: Option<f64>,
    pub conforms: bool,
}

/// A measured thread: its standard designation and the check of each
/// measured diameter under the record's decision rule.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ThreadInspection {
    pub instance: String,
    pub feature: String,
    pub designation: String,
    pub diameters: Vec<ThreadDiameterCheck>,
    /// Expanded uncertainty the decision rule applied.
    pub uncertainty_mm: f64,
    pub conforms: bool,
}

/// The Unified thread a feature names, with its length of engagement in
/// inches when given.
fn feature_thread(
    graph: &InstanceGraph<'_>,
    measured: &MeasuredThread,
) -> Result<(UnifiedThread, Option<f64>), ModelError> {
    let instance = graph.resolve(&measured.instance)?;
    let parameters = instance.resolved_parameters()?;
    let feature = instance
        .definition
        .features
        .iter()
        .find(|f| f.id == measured.feature)
        .ok_or_else(|| {
            ModelError::new(format!(
                "instance '{}' has no feature '{}'",
                measured.instance, measured.feature
            ))
        })?;
    let length = |e: &ScalarExpr| crate::expressions::scalar(e, &parameters, Dimension::Length);
    let named =
        |spec: Option<&UnifiedThreadSpec>, diameter: &ScalarExpr, pitch: &ScalarExpr, internal| {
            let spec = spec.ok_or_else(|| {
                ModelError::new(format!(
                    "feature '{}' names no Unified thread class",
                    measured.feature
                ))
            })?;
            let thread = spec.thread(length(diameter)?, length(pitch)?, internal)?;
            let engagement = spec
                .length_of_engagement
                .as_ref()
                .map(|e| length(e).map(|mm| mm / 25.4))
                .transpose()?;
            Ok((thread, engagement))
        };
    match &feature.operation {
        FeatureOperation::Thread {
            major_diameter,
            pitch,
            internal,
            unified,
            ..
        } => named(unified.as_ref(), major_diameter, pitch, *internal),
        FeatureOperation::Hole {
            thread: Some(thread),
            ..
        } => named(
            thread.unified.as_ref(),
            &thread.nominal_diameter,
            &thread.pitch,
            true,
        ),
        _ => Err(ModelError::new(format!(
            "feature '{}' is not a thread or threaded hole",
            measured.feature
        ))),
    }
}

/// Checks each measured thread against its class limits. A measured value
/// lies `deviation` outside its limits (negative inside), and conforms when
/// `rule` accepts that deviation against a zero allowance with uncertainty
/// `u`. Repeated or unknown threads and non-finite values fail the record.
/// O(threads), each resolving its instance's parameters.
pub(super) fn inspect_threads(
    graph: &InstanceGraph<'_>,
    measured: &[MeasuredThread],
    rule: DecisionRule,
    u: f64,
) -> Result<Vec<ThreadInspection>, ModelError> {
    let mut seen = HashSet::new();
    measured
        .iter()
        .map(|m| {
            if !seen.insert((m.instance.as_str(), m.feature.as_str())) {
                return Err(ModelError::new(
                    "inspection record repeats a measured thread",
                ));
            }
            let values = [
                Some(m.pitch_diameter_mm),
                m.major_diameter_mm,
                m.minor_diameter_mm,
            ];
            if !values.iter().flatten().all(|v| v.is_finite() && *v > 0.0) {
                return Err(ModelError::new(
                    "measured thread diameters must be positive and finite",
                ));
            }
            let (thread, engagement) = feature_thread(graph, m)?;
            let limits: UnifiedLimits = thread.limits(engagement)?;
            let mm = |inches: Option<f64>| inches.map(|v| v * 25.4);
            let mut diameters = vec![];
            let mut check = |diameter, measured: Option<f64>, [low, high]: [Option<f64>; 2]| {
                if let Some(measured) = measured {
                    let (minimum, maximum) = (mm(low), mm(high));
                    let deviation = f64::max(
                        maximum.map_or(f64::NEG_INFINITY, |max| measured - max),
                        minimum.map_or(f64::NEG_INFINITY, |min| min - measured),
                    );
                    diameters.push(ThreadDiameterCheck {
                        diameter,
                        measured_mm: measured,
                        minimum_mm: minimum,
                        maximum_mm: maximum,
                        conforms: rule.conforms(deviation, 0.0, u),
                    });
                }
            };
            check(ThreadDiameter::Major, m.major_diameter_mm, limits.major_in);
            check(
                ThreadDiameter::Pitch,
                Some(m.pitch_diameter_mm),
                limits.pitch_in.map(Some),
            );
            check(
                ThreadDiameter::Minor,
                m.minor_diameter_mm,
                limits.minor_in.map_or([None, None], |l| l.map(Some)),
            );
            Ok(ThreadInspection {
                instance: m.instance.clone(),
                feature: m.feature.clone(),
                designation: thread.designation(),
                conforms: diameters.iter().all(|d| d.conforms),
                diameters,
                uncertainty_mm: u,
            })
        })
        .collect()
}
