//! Buildable structure on the parametric wing (`occb-wing-build-v1`): a spar
//! channel following the sweep, optional elevons behind a hinge line, and
//! spanwise print segments, each checked by stored requirements. Every tool is
//! an expression of the station parameters, so edits regenerate the structure.

use occt_parametric::{
    Dimension, FamilyDefinition, FeatureDefinition, FeatureOperation, LengthUnit, LoftSection,
    MeshSettings, ParameterDefinition, ParameterType, ParameterValue, Quantity, Requirement,
    RequirementKind, RequirementPriority, ScalarExpr, VectorExpr, VectorQuantity, VerificationRule,
};
use serde::Deserialize;
use std::error::Error;

use crate::project::Project;

/// Half-size of the slabs and rods that trim and cut, in millimeters: far
/// larger than any wing the editor allows.
const REACH: f64 = 100_000.0;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Build {
    pub schema: String,
    pub spar: Spar,
    #[serde(default)]
    pub elevon: Option<Elevon>,
    /// Equal spanwise print segments per half.
    pub segments: usize,
    pub printer: Printer,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Spar {
    pub chord_fraction: f64,
    pub diameter_mm: f64,
    /// The spar runs straight from the root to this station.
    pub to_station: usize,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Elevon {
    pub from_fraction: f64,
    pub to_fraction: f64,
    pub hinge_fraction: f64,
    pub gap_mm: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Printer {
    pub bed_mm: [f64; 3],
    pub max_overhang_deg: f64,
}

fn literal(value: f64) -> ScalarExpr {
    ScalarExpr::Literal(Quantity::scalar(value))
}

fn millimeters(value: f64) -> ScalarExpr {
    ScalarExpr::Literal(Quantity::length(value, LengthUnit::Millimeter))
}

fn named(id: &str) -> ScalarExpr {
    ScalarExpr::Parameter(id.into())
}

fn add(a: ScalarExpr, b: ScalarExpr) -> ScalarExpr {
    ScalarExpr::Add(Box::new(a), Box::new(b))
}

fn times(a: ScalarExpr, b: ScalarExpr) -> ScalarExpr {
    ScalarExpr::Multiply(Box::new(a), Box::new(b))
}

/// Span position of fraction `fraction` on `side` (+1 right, -1 left).
fn span_y(side: f64, fraction: f64) -> ScalarExpr {
    times(literal(side * fraction / 2.0), named("span"))
}

/// A station quantity (`leading_edge`, `chord`, `height`) at any span
/// fraction, interpolated linearly like the ruled loft between stations.
fn at(project: &Project, quantity: &str, fraction: f64) -> ScalarExpr {
    let stations = &project.stations;
    let index = stations
        .windows(2)
        .position(|pair| fraction <= pair[1].fraction)
        .unwrap_or(stations.len() - 2);
    let (a, b) = (&stations[index], &stations[index + 1]);
    let t = (fraction - a.fraction) / (b.fraction - a.fraction);
    let start = named(&format!("{quantity}_{index}"));
    let end = named(&format!("{quantity}_{}", index + 1));
    add(times(literal(1.0 - t), start), times(literal(t), end))
}

fn feature(id: String, operation: FeatureOperation) -> FeatureDefinition {
    FeatureDefinition { id, operation }
}

/// A slab spanning `from`..`to` span fractions on one side, unbounded otherwise.
fn slab(side: f64, from: f64, to: f64) -> FeatureOperation {
    // On the left the slab starts at the outboard (more negative) end.
    let start = if side > 0.0 { from } else { to };
    FeatureOperation::Box {
        origin: VectorExpr::Components {
            x: millimeters(-REACH),
            y: span_y(side, start),
            z: millimeters(-REACH),
        },
        size: VectorExpr::Components {
            x: millimeters(2.0 * REACH),
            y: span_y(1.0, to - from),
            z: millimeters(2.0 * REACH),
        },
    }
}

/// A wedge from just behind (or ahead of) the hinge line to beyond the
/// trailing edge, between two span fractions. `offset` is the signed fraction
/// of the hinge gap: -0.5 cuts the wing, +0.5 bounds the elevon.
fn elevon_tool(project: &Project, side: f64, elevon: &Elevon, offset: f64) -> FeatureOperation {
    let section = |fraction: f64| {
        let chord = at(project, "chord", fraction);
        LoftSection {
            profile: vec![[0.0, -1.0], [1.0, -1.0], [1.0, 1.0], [0.0, 1.0]],
            origin: VectorExpr::Components {
                x: add(
                    add(
                        at(project, "leading_edge", fraction),
                        times(named("elevon_hinge"), chord.clone()),
                    ),
                    times(literal(offset), named("elevon_gap")),
                ),
                y: span_y(side, fraction),
                z: at(project, "height", fraction),
            },
            x_axis: VectorExpr::Literal(VectorQuantity::scalars(1.0, 0.0, 0.0)),
            y_axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
            scale: chord,
            rotation_radians: None,
            pivot: [0.0, 0.0],
        }
    };
    FeatureOperation::Loft {
        sections: vec![section(elevon.from_fraction), section(elevon.to_fraction)],
        smooth: false,
        ruled: true,
    }
}

fn parameter(id: &str, dimension: Dimension, value: f64) -> ParameterDefinition {
    let quantity = match dimension {
        Dimension::Length => Quantity::length(value, LengthUnit::Millimeter),
        _ => Quantity::scalar(value),
    };
    ParameterDefinition {
        id: id.into(),
        parameter_type: ParameterType::Scalar(dimension),
        default: ParameterValue::Scalar(quantity),
        minimum: None,
        maximum: None,
    }
}

fn requirement(
    id: String,
    kind: RequirementKind,
    priority: RequirementPriority,
    rule: VerificationRule,
) -> Requirement {
    Requirement {
        statement: format!("{id} holds for the printed part"),
        id,
        version: 1,
        kind,
        priority,
        rule,
        provenance: "occb-wing-cad build".into(),
    }
}

/// Adds the structure to a wing family and returns the printable part
/// outputs. Structure checks: every part is one solid (required, since a
/// broken part is unusable), fits the bed, and prints without steep overhangs
/// when standing on its inboard face (preferred, reported but not blocking).
pub fn add_structure(
    family: &mut FamilyDefinition,
    project: &Project,
    build: &Build,
) -> Result<Vec<String>, Box<dyn Error>> {
    let stations = project.stations.len();
    let spar = &build.spar;
    if build.schema != "occb-wing-build-v1"
        || !(1..=50).contains(&build.segments)
        || !(0.0 < spar.chord_fraction && spar.chord_fraction < 1.0)
        || !(spar.diameter_mm.is_finite() && spar.diameter_mm > 0.0)
        || !(1..stations).contains(&spar.to_station)
        || build
            .printer
            .bed_mm
            .iter()
            .any(|value| !(value.is_finite() && *value > 0.0))
        || !(0.0..=90.0).contains(&build.printer.max_overhang_deg)
    {
        return Err(
            "Expected occb-wing-build-v1 with 1–50 segments, a spar chord fraction in \
                    (0, 1), a positive spar diameter, a spar station after the root, a positive \
                    bed size, and an overhang limit in [0, 90] degrees."
                .into(),
        );
    }
    if let Some(elevon) = &build.elevon
        && !(0.0 <= elevon.from_fraction
            && elevon.from_fraction < elevon.to_fraction
            && elevon.to_fraction <= 1.0
            && 0.0 < elevon.hinge_fraction
            && elevon.hinge_fraction < 1.0
            && elevon.gap_mm.is_finite()
            && elevon.gap_mm >= 0.0)
    {
        return Err(
            "Elevon fractions must increase within [0, 1], the hinge must lie inside the \
                    chord, and the gap must be nonnegative."
                .into(),
        );
    }
    family.parameters.extend([
        parameter("spar_fraction", Dimension::Scalar, spar.chord_fraction),
        parameter("spar_diameter", Dimension::Length, spar.diameter_mm),
    ]);
    if let Some(elevon) = &build.elevon {
        family.parameters.extend([
            parameter("elevon_hinge", Dimension::Scalar, elevon.hinge_fraction),
            parameter("elevon_gap", Dimension::Length, elevon.gap_mm),
        ]);
    }
    let tip = project.stations[spar.to_station].fraction;
    let bed = build.printer.bed_mm;
    let mut parts = Vec::new();
    for (name, side) in [("right", 1.0), ("left", -1.0)] {
        // The spar centerline joins the chord-line points at the root and at
        // `to_station`; it follows sweep and dihedral. A rod longer than the
        // span is trimmed to the stations' span range.
        let spar_point = |index: usize| VectorExpr::Components {
            x: add(
                named(&format!("leading_edge_{index}")),
                times(named("spar_fraction"), named(&format!("chord_{index}"))),
            ),
            y: span_y(side, project.stations[index].fraction),
            z: named(&format!("height_{index}")),
        };
        family.features.extend([
            feature(
                format!("{name}_spar_rod"),
                FeatureOperation::Cylinder {
                    origin: spar_point(0),
                    axis: VectorExpr::Normalize(Box::new(VectorExpr::Subtract(
                        Box::new(spar_point(spar.to_station)),
                        Box::new(spar_point(0)),
                    ))),
                    radius: times(literal(0.5), named("spar_diameter")),
                    height: times(literal(2.0), named("span")),
                },
            ),
            feature(format!("{name}_spar_span"), slab(side, 0.0, tip)),
            feature(
                format!("{name}_spar"),
                FeatureOperation::Common {
                    left: format!("{name}_spar_rod"),
                    right: format!("{name}_spar_span"),
                },
            ),
            feature(
                format!("{name}_drilled"),
                FeatureOperation::Cut {
                    object: name.into(),
                    tool: format!("{name}_spar"),
                },
            ),
        ]);
        let mut body = format!("{name}_drilled");
        if let Some(elevon) = &build.elevon {
            family.features.extend([
                feature(
                    format!("{name}_elevon_cutter"),
                    elevon_tool(project, side, elevon, -0.5),
                ),
                feature(
                    format!("{name}_elevon_bound"),
                    elevon_tool(project, side, elevon, 0.5),
                ),
                feature(
                    format!("{name}_body"),
                    FeatureOperation::Cut {
                        object: body.clone(),
                        tool: format!("{name}_elevon_cutter"),
                    },
                ),
                feature(
                    format!("{name}_elevon"),
                    FeatureOperation::Common {
                        left: body.clone(),
                        right: format!("{name}_elevon_bound"),
                    },
                ),
            ]);
            parts.push(format!("{name}_elevon"));
            body = format!("{name}_body");
        }
        for segment in 0..build.segments {
            let (from, to) = (
                segment as f64 / build.segments as f64,
                (segment + 1) as f64 / build.segments as f64,
            );
            let id = format!("{name}_segment_{segment}");
            family.features.extend([
                feature(format!("{id}_span"), slab(side, from, to)),
                feature(
                    id.clone(),
                    FeatureOperation::Common {
                        left: body.clone(),
                        right: format!("{id}_span"),
                    },
                ),
            ]);
            // Printed standing on the inboard face, outboard end up.
            family.requirements.push(requirement(
                format!("{id}.overhang"),
                RequirementKind::Manufacturing,
                RequirementPriority::Preferred,
                VerificationRule::Overhang {
                    output: id.clone(),
                    build_direction: VectorQuantity::scalars(0.0, side, 0.0),
                    maximum_radians: build.printer.max_overhang_deg.to_radians(),
                    mesh: MeshSettings::default(),
                },
            ));
            parts.push(id);
        }
    }
    for part in &parts {
        family.requirements.extend([
            requirement(
                format!("{part}.single-solid"),
                RequirementKind::Topological,
                RequirementPriority::Required,
                VerificationRule::Connectivity {
                    output: part.clone(),
                    solids: 1,
                    allow_voids: false,
                },
            ),
            requirement(
                format!("{part}.fits-bed"),
                RequirementKind::Manufacturing,
                RequirementPriority::Preferred,
                VerificationRule::FitsWithin {
                    output: part.clone(),
                    envelope: VectorQuantity::lengths(
                        bed[0],
                        bed[1],
                        bed[2],
                        LengthUnit::Millimeter,
                    ),
                },
            ),
        ]);
    }
    Ok(parts)
}
