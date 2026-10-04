//! Builds a parametric wing family from an editor project (`occb-wing-layout-v1`):
//! one smooth ruled loft per half, with span, chord, leading-edge, height, and
//! twist parameters per station, so edits regenerate instead of re-exporting.

use occt_parametric::{
    Dimension, FamilyDefinition, FeatureDefinition, FeatureOperation, LengthUnit, LoftSection,
    ParameterDefinition, ParameterType, ParameterValue, Quantity, Requirement, RequirementKind,
    RequirementPriority, ScalarExpr, VectorExpr, VectorQuantity, VerificationRule,
};
use serde::Deserialize;
use std::error::Error;

/// Cosine intervals per airfoil surface, matching the editor's CAD export.
const INTERVALS: usize = 40;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub schema: String,
    pub name: String,
    pub span_mm: f64,
    pub stations: Vec<Station>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Station {
    pub fraction: f64,
    pub chord_mm: f64,
    pub leading_edge_mm: f64,
    pub height_mm: f64,
    pub twist_deg: f64,
    pub pivot_fraction: f64,
    pub airfoil: Airfoil,
}

#[derive(Deserialize)]
pub struct Airfoil {
    pub points: Vec<[f64; 2]>,
}

/// Linear interpolation along one surface, clamped to its x range.
fn interpolate(surface: &[[f64; 2]], x: f64) -> f64 {
    let (first, last) = (surface[0], surface[surface.len() - 1]);
    let direction = if last[0] >= first[0] { 1.0 } else { -1.0 };
    if (x - first[0]) * direction <= 0.0 {
        return first[1];
    }
    if (x - last[0]) * direction >= 0.0 {
        return last[1];
    }
    let (mut low, mut high) = (0, surface.len() - 1);
    while high - low > 1 {
        let middle = (low + high) / 2;
        if (x - surface[middle][0]) * direction >= 0.0 {
            low = middle;
        } else {
            high = middle;
        }
    }
    let (a, b) = (surface[low], surface[high]);
    if (b[0] - a[0]).abs() < 1e-12 {
        a[1]
    } else {
        a[1] + (b[1] - a[1]) * (x - a[0]) / (b[0] - a[0])
    }
}

/// The editor's resampling: trailing edge (closed at the midpoint), upper
/// surface to the leading edge, lower surface back, at cosine spacing.
fn resample(points: &[[f64; 2]]) -> Vec<[f64; 2]> {
    let leading = (0..points.len())
        .min_by(|a, b| points[*a][0].total_cmp(&points[*b][0]))
        .unwrap_or(0);
    let upper = &points[..=leading];
    let mut lower = points[leading..].to_vec();
    if lower.last().is_some_and(|point| point[0] < 0.999_999) {
        lower.push(points[0]);
    }
    let x = |i: usize| (1.0 + (std::f64::consts::PI * i as f64 / INTERVALS as f64).cos()) / 2.0;
    let mut result = (0..=INTERVALS)
        .map(|i| [x(i), interpolate(upper, x(i))])
        .chain(
            (1..INTERVALS)
                .rev()
                .map(|i| [x(i), interpolate(&lower, x(i))]),
        )
        .collect::<Vec<_>>();
    result[0][1] = (interpolate(upper, 1.0) + interpolate(&lower, 1.0)) / 2.0;
    result
}

fn parameter(id: String, dimension: Dimension, value: f64) -> ParameterDefinition {
    let quantity = match dimension {
        Dimension::Length => Quantity::length(value, LengthUnit::Millimeter),
        _ => Quantity::scalar(value),
    };
    ParameterDefinition {
        id,
        parameter_type: ParameterType::Scalar(dimension),
        default: ParameterValue::Scalar(quantity),
        minimum: None,
        maximum: None,
    }
}

fn named(id: &str) -> ScalarExpr {
    ScalarExpr::Parameter(id.into())
}

fn requirement(id: String, kind: RequirementKind, rule: VerificationRule) -> Requirement {
    Requirement {
        statement: format!("{id} holds for the generated wing half"),
        id,
        version: 1,
        kind,
        priority: RequirementPriority::Required,
        rule,
        provenance: "occb-wing-cad".into(),
    }
}

/// Validates the project's structure and returns the wing family. Station
/// values become parameter defaults; airfoils become literal profiles.
pub fn family(project: &Project) -> Result<FamilyDefinition, Box<dyn Error>> {
    let stations = &project.stations;
    if !(2..=200).contains(&stations.len())
        || !(project.span_mm.is_finite() && project.span_mm > 0.0)
    {
        return Err("A project needs a positive span and 2–200 stations.".into());
    }
    if stations[0].fraction != 0.0
        || stations[stations.len() - 1].fraction != 1.0
        || stations
            .windows(2)
            .any(|pair| pair[1].fraction <= pair[0].fraction)
    {
        return Err("Station fractions must increase from 0 to 1.".into());
    }
    if stations.iter().any(|station| {
        station.airfoil.points.len() < 6
            || station
                .airfoil
                .points
                .iter()
                .flatten()
                .any(|v| !v.is_finite())
            || !(0.0..=1.0).contains(&station.pivot_fraction)
    }) {
        return Err(
            "Each station needs 6 or more finite airfoil points and a pivot in [0, 1].".into(),
        );
    }
    let mut parameters = vec![parameter("span".into(), Dimension::Length, project.span_mm)];
    for (index, station) in stations.iter().enumerate() {
        parameters.extend([
            parameter(
                format!("chord_{index}"),
                Dimension::Length,
                station.chord_mm,
            ),
            parameter(
                format!("leading_edge_{index}"),
                Dimension::Length,
                station.leading_edge_mm,
            ),
            parameter(
                format!("height_{index}"),
                Dimension::Length,
                station.height_mm,
            ),
            parameter(
                format!("twist_{index}"),
                Dimension::Scalar,
                station.twist_deg.to_radians(),
            ),
        ]);
    }
    let half = |side: f64| FeatureOperation::Loft {
        smooth: true,
        // Straight spanwise panels, as in the editor; no overshoot between stations.
        ruled: true,
        sections: stations
            .iter()
            .enumerate()
            .map(|(index, station)| LoftSection {
                profile: resample(&station.airfoil.points),
                origin: VectorExpr::Components {
                    x: named(&format!("leading_edge_{index}")),
                    y: ScalarExpr::Multiply(
                        Box::new(ScalarExpr::Literal(Quantity::scalar(
                            side * station.fraction / 2.0,
                        ))),
                        Box::new(named("span")),
                    ),
                    z: named(&format!("height_{index}")),
                },
                x_axis: VectorExpr::Literal(VectorQuantity::scalars(1.0, 0.0, 0.0)),
                y_axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
                scale: named(&format!("chord_{index}")),
                // X aft and Z up: positive incidence raises the leading edge,
                // a clockwise turn in the section plane.
                rotation_radians: Some(ScalarExpr::Negate(Box::new(named(&format!(
                    "twist_{index}"
                ))))),
                pivot: [station.pivot_fraction, 0.0],
            })
            .collect(),
    };
    let mut requirements = Vec::new();
    for output in ["right", "left"] {
        requirements.push(requirement(
            format!("{output}.valid"),
            RequirementKind::Validation,
            VerificationRule::ShapeValid {
                output: output.into(),
            },
        ));
        requirements.push(requirement(
            format!("{output}.single-solid"),
            RequirementKind::Topological,
            VerificationRule::Connectivity {
                output: output.into(),
                solids: 1,
                allow_voids: false,
            },
        ));
    }
    Ok(FamilyDefinition {
        references: Vec::new(),
        id: "wing".into(),
        version: 1,
        parameters,
        derived_parameters: Vec::new(),
        derived_vector_parameters: Vec::new(),
        constraints: Vec::new(),
        features: vec![
            FeatureDefinition {
                id: "right".into(),
                operation: half(1.0),
            },
            FeatureDefinition {
                id: "left".into(),
                operation: half(-1.0),
            },
        ],
        requirements,
        datums: Vec::new(),
    })
}
