//! Requirement rules at assembly scale: once-per-variant part checks and
//! indexed exact collision checks driven from stored requirements.

use super::*;
use occt_parametric::{
    AssemblyRequirement, InstanceOutputRef, OutputSet, Requirement, RequirementKind,
    VerificationRule, VerificationStatus,
};

fn assembly_requirement(id: &str, rule: AssemblyVerificationRule) -> AssemblyRequirement {
    AssemblyRequirement {
        id: id.into(),
        version: 1,
        kind: RequirementKind::Assembly,
        priority: RequirementPriority::Advisory,
        statement: id.into(),
        rule,
        provenance: "bench".into(),
    }
}

/// `count` members 50 mm apart along x, offset by `y`; the source sits far
/// away so it is neither a neighbor nor coincident with member 0.
fn row<'a>(
    definition: &'a FamilyDefinition,
    prefix: &str,
    count: usize,
    y: f64,
) -> Result<InstanceGraph<'a>, ModelError> {
    let mut graph = InstanceGraph::new(definition);
    add_row(&mut graph, prefix, count, |_| y)?;
    Ok(graph)
}

fn add_row(
    graph: &mut InstanceGraph<'_>,
    prefix: &str,
    count: usize,
    y: impl Fn(usize) -> f64,
) -> Result<(), ModelError> {
    let source = format!("{prefix}-source");
    graph.add_base(&source, HashMap::new(), "bench")?;
    graph.set_placement(
        &source,
        Placement::translated(VectorQuantity::lengths(
            0.0,
            0.0,
            -1.0e6,
            LengthUnit::Millimeter,
        )),
    )?;
    let members = graph.add_linear_pattern(
        prefix,
        prefix,
        &source,
        count,
        VectorQuantity::lengths(50.0, 0.0, 0.0, LengthUnit::Millimeter),
        "bench",
    )?;
    for member in &members {
        let index = member
            .trim_start_matches(prefix)
            .trim_matches(['[', ']'])
            .parse::<usize>()
            .map_err(|error| failure(error.to_string()))?;
        graph.set_placement(
            member,
            Placement::translated(VectorQuantity::lengths(
                50.0 * index as f64,
                y(index),
                0.0,
                LengthUnit::Millimeter,
            )),
        )?;
    }
    Ok(())
}

fn released(session: &Session) -> Result<(), ModelError> {
    if session
        .shape_count()
        .map_err(|error| failure(error.to_string()))?
        != 0
    {
        return Err(failure("requirement handles leaked".into()));
    }
    Ok(())
}

pub(crate) fn requirement_cases(definition: &FamilyDefinition) -> Vec<Outcome> {
    let mut checked = definition.clone();
    checked.requirements.push(Requirement {
        id: "body.connected".into(),
        version: 1,
        kind: RequirementKind::Topological,
        priority: RequirementPriority::Required,
        statement: "one solid without loose topology".into(),
        rule: VerificationRule::Connectivity {
            output: "body".into(),
            solids: 1,
            allow_voids: false,
        },
        provenance: "bench".into(),
    });
    let checked: &'static FamilyDefinition = Box::leak(Box::new(checked));
    vec![
        timed(
            "requirements 10000: connectivity, no interference, clearance".into(),
            ms(3_000),
            Expectation::Required,
            || {
                let session = Session::new().map_err(|error| failure(error.to_string()))?;
                let mut graph = row(checked, "part", 10_000, 0.0)?;
                graph.add_assembly_requirements([
                    assembly_requirement(
                        "apart",
                        AssemblyVerificationRule::NoInterference {
                            outputs: OutputSet::AllWithOutput("body".into()),
                        },
                    ),
                    assembly_requirement(
                        "gap",
                        AssemblyVerificationRule::MinimumClearance {
                            first: OutputSet::AllWithOutput("body".into()),
                            second: None,
                            minimum: Quantity::length(20.0, LengthUnit::Millimeter),
                        },
                    ),
                ])?;
                let generation = graph.regenerate_all(&session)?;
                if generation.generated_variants() != 1 {
                    return Err(failure("part requirements lost variant sharing".into()));
                }
                let verification = generation.verification();
                if verification.len() != 2
                    || verification
                        .iter()
                        .any(|result| result.status != VerificationStatus::Passed)
                {
                    return Err(failure(format!("unexpected verification {verification:?}")));
                }
                drop(generation);
                released(&session)?;
                Ok("10002 instances, 1 variant checked once, 2 exact assembly rules passed".into())
            },
        ),
        timed(
            "requirements: clearance between 5000 x 5000 outputs".into(),
            ms(5_000),
            Expectation::Required,
            || {
                let session = Session::new().map_err(|error| failure(error.to_string()))?;
                // Every tenth back member sits 20 mm from its front neighbor
                // (depth 20), inside a 25 mm minimum; the rest are 960 mm
                // away. 10,000 indexed outputs, 500 exact pair queries.
                let mut graph = row(definition, "front", 5_000, 0.0)?;
                add_row(&mut graph, "back", 5_000, |index| {
                    if index % 10 == 0 { 40.0 } else { 1_000.0 }
                })?;
                let members = |prefix: &str| {
                    OutputSet::Explicit(
                        (0..5_000)
                            .map(|index| InstanceOutputRef {
                                instance: format!("{prefix}[{index}]"),
                                output: "body".into(),
                            })
                            .collect(),
                    )
                };
                graph.add_assembly_requirement(assembly_requirement(
                    "rows",
                    AssemblyVerificationRule::MinimumClearance {
                        first: members("front"),
                        second: Some(members("back")),
                        minimum: Quantity::length(25.0, LengthUnit::Millimeter),
                    },
                ))?;
                let generation = graph.regenerate_all(&session)?;
                let result = &generation.verification()[0];
                let closest = result.measured.map(|measured| measured.value);
                if result.status != VerificationStatus::Failed
                    || !result.message.starts_with("500 pair(s)")
                    || closest.is_none_or(|value| (value - 20.0).abs() > 1e-6)
                {
                    return Err(failure(format!("unexpected result {result:?}")));
                }
                drop(generation);
                released(&session)?;
                Ok(
                    "500 of 5000 x 5000 cross-row pairs found at 20 mm; in-row pairs never inspected"
                        .into(),
                )
            },
        ),
    ]
}

/// The kernel queries behind `MinimumRadius`, timed apart from construction:
/// exact analytic faces at scale, and one freeform face at a high sample count.
pub(crate) fn radius_case() -> Outcome {
    const HOLES: usize = 1_000;
    const SAMPLES: u32 = 256;
    timed(
        format!("minimum radius: {HOLES}-hole plate exact, blend at {SAMPLES}^2 samples"),
        ms(15_000),
        Expectation::Required,
        || {
            let session = Session::new().map_err(|error| failure(error.to_string()))?;
            let plate =
                session.create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(810.0, 510.0, 10.0))?;
            let bores = (0..HOLES)
                .map(|index| {
                    let (row, column) = ((index / 40) as f64, (index % 40) as f64);
                    session.create_cylinder(
                        Vec3::new(15.0 + column * 20.0, 15.0 + row * 20.0, -1.0),
                        Vec3::new(0.0, 0.0, 1.0),
                        4.0,
                        12.0,
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            let tools = session.create_compound(&bores.iter().collect::<Vec<_>>())?;
            let drilled = session.cut(&plate, &tools)?;
            let block =
                session.create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(20.0, 10.0, 10.0))?;
            let edge = session
                .subshapes(&block, ShapeType::Edge)?
                .into_iter()
                .find(|edge| {
                    // Bounds include the edge tolerance, about 1e-7 mm.
                    session.bounds(edge).is_ok_and(|bounds| {
                        bounds.min.y.abs() < 1e-6
                            && bounds.max.y.abs() < 1e-6
                            && (bounds.min.z - 10.0).abs() < 1e-6
                            && (bounds.max.z - 10.0).abs() < 1e-6
                    })
                })
                .ok_or_else(|| failure("blend edge missing".into()))?;
            let blended = session.variable_fillet(&block, &[&edge], 1.0, 2.0)?;

            let start = Instant::now();
            let faces = session.face_radius_bounds(&drilled, 17)?;
            let exact = start.elapsed();
            let edges = session.edge_concavities(&drilled, 1e-6)?;
            let analysed = start.elapsed() - exact;
            let blend = session.face_radius_bounds(&blended, SAMPLES)?;
            let sampled_time = start.elapsed() - exact - analysed;

            let concave = faces
                .iter()
                .filter_map(|face| face.concave.map(|(radius, _)| radius))
                .collect::<Vec<_>>();
            if faces.len() != HOLES + 6
                || !faces.iter().all(|face| face.exact)
                || concave.len() != HOLES
                || concave.iter().any(|radius| (radius - 4.0).abs() > 1e-9)
                || edges.contains(&occt_bridge::EdgeConcavity::Concave)
            {
                return Err(failure("drilled plate radii or concavity differ".into()));
            }
            let sampled = blend.iter().find(|face| !face.exact);
            if sampled.is_none_or(|face| {
                face.convex
                    .is_none_or(|(radius, _)| (radius - 1.0).abs() > 1e-2)
            }) {
                return Err(failure("blend radius differs".into()));
            }
            Ok(format!(
                "{} exact faces {:.3}s, {} edges {:.3}s, {} blend samples {:.3}s",
                faces.len(),
                exact.as_secs_f64(),
                edges.len(),
                analysed.as_secs_f64(),
                sampled.map_or(0, |face| face.samples),
                sampled_time.as_secs_f64()
            ))
        },
    )
}
