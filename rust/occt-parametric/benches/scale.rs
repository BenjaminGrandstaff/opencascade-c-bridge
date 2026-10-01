//! Scale benchmark suite enforcing the roadmap's scaling requirement.
//!
//! Each case runs at the target sizes (10,000 instances or pattern members,
//! deep clone chains, 50-part solver stacks) and is compared with a time
//! budget. Budgets carry roughly 3-5x headroom over measured release-build
//! times on a developer machine so that ordinary variance does not fail the
//! run while real regressions do. Cases tied to open roadmap items are
//! reported as known gaps; a gap that starts passing is flagged so the
//! roadmap can be updated.
//!
//! Run with `tools/bench/run.sh`. The process exits non-zero when any
//! non-gap case fails.

use occt_bridge::{Session, SessionOptions, Vec3};
use occt_parametric::{
    AssemblyRelationship, AxisAngle, DatumDefinition, DatumKind, DatumRef, Dimension,
    FamilyDefinition, FeatureDefinition, FeatureOperation, InstanceGraph, LengthUnit,
    ModelDocument, ModelError, ParameterDefinition, ParameterType, ParameterValue, PatternRule,
    Placement, Quantity, RelationKind, ScalarExpr, VectorExpr, VectorQuantity,
};
use std::collections::HashMap;
use std::process::ExitCode;
use std::time::{Duration, Instant};

const PATTERN_MEMBERS: usize = 10_000;
const CLONE_CHAIN_DEPTH: usize = 20_000;
const REGENERATION_ROUNDS: usize = 10;
const REGENERATION_MEMBERS: usize = 1_000;
const VALIDATED_CUTS: usize = 100;
/// Largest acceptable slowdown of a boolean chain from result validation.
/// Measured: 1.65x at 25 cuts, 1.75x at 50, 1.97x at 100, 2.18x at 200.
/// Each cut and each check are O(part size), but checking a face with many
/// holes includes pairwise wire-intersection tests, so the ratio grows
/// slowly; cheaper checks for such faces are a roadmap item.
const VALIDATION_OVERHEAD_LIMIT: f64 = 2.5;
/// Clone resolution recurses once per chain link, so a 20,000-deep chain
/// overflows a default 2 MiB thread stack; the case runs on a 16 MiB stack
/// until resolution is iterative (roadmap item).
const DEEP_CHAIN_STACK_BYTES: usize = 16 << 20;

#[derive(Clone, Copy, PartialEq)]
enum Expectation {
    /// Must meet its budget and correctness check.
    Required,
    /// Tied to an open roadmap item; reported but does not fail the run.
    KnownGap(&'static str),
}

struct Outcome {
    name: String,
    elapsed: Duration,
    budget: Duration,
    /// Correctness check alongside timing; `Err` explains the failure.
    check: Result<String, String>,
    expectation: Expectation,
}

impl Outcome {
    fn passed(&self) -> bool {
        self.check.is_ok() && self.elapsed <= self.budget
    }
}

fn main() -> ExitCode {
    let definition: &'static FamilyDefinition = Box::leak(Box::new(block()));
    let mut outcomes = Vec::new();
    outcomes.extend(pattern_cases(definition));
    outcomes.push(deep_clone_chain(definition));
    outcomes.extend(regeneration_handle_cases(definition));
    outcomes.extend(solver_cases(definition));
    outcomes.extend(validation_cases());
    report(&outcomes)
}

// ---- cases

fn pattern_cases(definition: &'static FamilyDefinition) -> Vec<Outcome> {
    let size = PATTERN_MEMBERS;
    let mut graph = InstanceGraph::new(definition);
    graph.add_base("source", HashMap::new(), "bench").unwrap();
    let mut outcomes = Vec::new();
    let step = |millimeters| VectorQuantity::lengths(millimeters, 0.0, 0.0, LengthUnit::Millimeter);

    outcomes.push(timed(
        format!("pattern {size}: create"),
        ms(50),
        Expectation::Required,
        || {
            graph
                .add_linear_pattern("row", "m", "source", size, step(50.0), "bench")
                .map(|members| format!("{} members", members.len()))
        },
    ));
    outcomes.push(timed(
        format!("pattern {size}: edit rule (re-place)"),
        ms(50),
        Expectation::Required,
        || {
            graph
                .set_pattern_rule("row", PatternRule::Linear { step: step(60.0) })
                .map(|()| "re-placed".into())
        },
    ));
    let mut json = String::new();
    outcomes.push(timed(
        format!("pattern {size}: save document"),
        ms(200),
        Expectation::Required,
        || {
            json = ModelDocument::from_graph(&graph).to_json_pretty()?;
            Ok(format!("{} KiB", json.len() / 1024))
        },
    ));
    outcomes.push(timed(
        format!("pattern {size}: load document"),
        ms(300),
        Expectation::Required,
        || {
            ModelDocument::from_json(&json)
                .map(|document| format!("{} instances", document.instances.len()))
        },
    ));
    graph.add_configuration("wide").unwrap();
    outcomes.push(timed(
        format!("pattern {size}: configuration override"),
        ms(200),
        Expectation::Required,
        || {
            graph
                .set_configuration_override("wide", "source", "width", length(12.0))
                .map(|()| "validated".into())
        },
    ));
    let session = Session::new().unwrap();
    outcomes.push(timed(
        format!("pattern {size}: regenerate all (shared)"),
        ms(3_000),
        Expectation::Required,
        || {
            let generation = graph.regenerate_all(&session)?;
            if generation.generated_variants() != 1 {
                return Err(failure(format!(
                    "{} variants, expected 1 shared generation",
                    generation.generated_variants()
                )));
            }
            Ok(format!("{} shapes", session.shape_count()?))
        },
    ));
    outcomes
}

fn deep_clone_chain(definition: &'static FamilyDefinition) -> Outcome {
    let mut graph = InstanceGraph::new(definition);
    graph.add_base("c0", HashMap::new(), "bench").unwrap();
    for index in 1..CLONE_CHAIN_DEPTH {
        graph
            .add_clone(
                format!("c{index}"),
                format!("c{}", index - 1),
                HashMap::new(),
                "bench",
            )
            .unwrap();
    }
    let deepest = format!("c{}", CLONE_CHAIN_DEPTH - 1);
    std::thread::Builder::new()
        .stack_size(DEEP_CHAIN_STACK_BYTES)
        .spawn(move || {
            timed(
                format!("clone chain {CLONE_CHAIN_DEPTH}: resolve deepest"),
                ms(2_000),
                Expectation::Required,
                || {
                    graph
                        .resolve(&deepest)
                        .map(|_| "resolved (needs a 16 MiB stack)".into())
                },
            )
        })
        .unwrap()
        .join()
        .unwrap()
}

/// Repeated regeneration must return the session to its starting shape count
/// once results are dropped. Process RSS is not used: freed memory from
/// earlier cases is reused, so it hides growth.
fn regeneration_handle_cases(definition: &'static FamilyDefinition) -> Vec<Outcome> {
    let mut graph = InstanceGraph::new(definition);
    graph.add_base("source", HashMap::new(), "bench").unwrap();
    graph
        .add_linear_pattern(
            "row",
            "m",
            "source",
            REGENERATION_MEMBERS - 1,
            VectorQuantity::lengths(50.0, 0.0, 0.0, LengthUnit::Millimeter),
            "bench",
        )
        .unwrap();
    let session = Session::new().unwrap();
    let mut counts = Vec::new();
    let timing = timed(
        format!("regenerate {REGENERATION_MEMBERS} x{REGENERATION_ROUNDS}: time"),
        ms(4_000),
        Expectation::Required,
        || {
            for _ in 0..REGENERATION_ROUNDS {
                drop(graph.regenerate_all(&session)?);
                counts.push(session.shape_count()?);
            }
            Ok(format!("{REGENERATION_ROUNDS} rounds"))
        },
    );
    let handles = Outcome {
        name: format!("regenerate {REGENERATION_MEMBERS} x{REGENERATION_ROUNDS}: handles released"),
        elapsed: Duration::ZERO,
        budget: Duration::MAX,
        check: match counts.last() {
            Some(0) => Ok("session back to 0 shapes".into()),
            Some(last) => Err(format!("{last} shapes left after dropping results")),
            None => Err("no rounds ran".into()),
        },
        expectation: Expectation::Required,
    };
    vec![timing, handles]
}

fn solver_cases(definition: &'static FamilyDefinition) -> Vec<Outcome> {
    let cases = [
        (50, 1_000.0, false, ms(1_000), Expectation::Required),
        (20, 1_000.0, true, ms(500), Expectation::Required),
        (50, 1_000.0, true, ms(8_000), Expectation::Required),
        (
            50,
            1_000_000.0,
            false,
            ms(1_000),
            Expectation::KnownGap("roadmap: configurable tolerances"),
        ),
    ];
    cases
        .into_iter()
        .map(|(count, offset, constrained, budget, expectation)| {
            let mut graph = stacked_blocks(definition, count, offset, constrained);
            let ids = (1..=count)
                .map(|index| format!("i{index}"))
                .collect::<Vec<_>>();
            let ids = ids.iter().map(String::as_str).collect::<Vec<_>>();
            let kind = if constrained { "constrained" } else { "seated" };
            timed(
                format!("solve {count} {kind} parts at {offset} mm"),
                budget,
                expectation,
                || {
                    let solution = graph.solve_placements(&ids)?;
                    if solution.solved {
                        Ok(format!("{} iterations", solution.iterations))
                    } else {
                        Err(failure(format!(
                            "unsolved, max residual {:.1e}",
                            solution.max_residual
                        )))
                    }
                },
            )
        })
        .collect()
}

/// Drills `VALIDATED_CUTS` holes into a plate one boolean at a time, with
/// result validation off and on, and bounds the validation overhead.
fn validation_cases() -> Vec<Outcome> {
    let drill = |validate: bool| -> Result<Duration, ModelError> {
        let session = Session::new()?;
        session.set_options(SessionOptions {
            validate_results: validate,
            ..SessionOptions::default()
        })?;
        let mut plate =
            session.create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(400.0, 400.0, 10.0))?;
        let start = Instant::now();
        for index in 0..VALIDATED_CUTS {
            let (row, column) = ((index / 10) as f64, (index % 10) as f64);
            let hole = session.create_cylinder(
                Vec3::new(20.0 + column * 38.0, 20.0 + row * 38.0, -1.0),
                Vec3::new(0.0, 0.0, 1.0),
                6.0,
                12.0,
            )?;
            let drilled = session.cut(&plate, &hole)?;
            session.remove(hole)?;
            session.remove(std::mem::replace(&mut plate, drilled))?;
        }
        Ok(start.elapsed())
    };
    let mut baseline = Duration::ZERO;
    let unchecked = timed(
        format!("{VALIDATED_CUTS} sequential cuts: validation off"),
        ms(10_000),
        Expectation::Required,
        || {
            baseline = drill(false)?;
            Ok("baseline".into())
        },
    );
    let checked = timed(
        format!("{VALIDATED_CUTS} sequential cuts: validation on"),
        ms(15_000),
        Expectation::Required,
        || {
            let elapsed = drill(true)?;
            let ratio = elapsed.as_secs_f64() / baseline.as_secs_f64().max(1e-9);
            if ratio <= VALIDATION_OVERHEAD_LIMIT {
                Ok(format!("{ratio:.2}x of unvalidated"))
            } else {
                Err(failure(format!(
                    "validation costs {ratio:.2}x, limit {VALIDATION_OVERHEAD_LIMIT}x"
                )))
            }
        },
    );
    vec![unchecked, checked]
}

// ---- fixtures

fn length(millimeters: f64) -> ParameterValue {
    ParameterValue::Scalar(Quantity::length(millimeters, LengthUnit::Millimeter))
}

fn ms(milliseconds: u64) -> Duration {
    Duration::from_millis(milliseconds)
}

fn block() -> FamilyDefinition {
    let parameter = |id: &str| ScalarExpr::Parameter(id.into());
    let zero = || ScalarExpr::Literal(Quantity::length(0.0, LengthUnit::Millimeter));
    let half = |id: &str| {
        ScalarExpr::Multiply(
            Box::new(parameter(id)),
            Box::new(ScalarExpr::Literal(Quantity::scalar(0.5))),
        )
    };
    let declare = |id: &str, default| ParameterDefinition {
        id: id.into(),
        parameter_type: ParameterType::Scalar(Dimension::Length),
        default: length(default),
        minimum: Some(Quantity::length(1.0, LengthUnit::Millimeter)),
        maximum: None,
    };
    let datum = |id: &str, kind| DatumDefinition {
        id: id.into(),
        kind,
    };
    let direction = |x, y, z| VectorExpr::Literal(VectorQuantity::scalars(x, y, z));
    let point = |x, y, z| VectorExpr::Components { x, y, z };
    FamilyDefinition {
        id: "Block".into(),
        version: 1,
        parameters: vec![
            declare("width", 10.0),
            declare("depth", 20.0),
            declare("height", 30.0),
        ],
        derived_parameters: Vec::new(),
        derived_vector_parameters: Vec::new(),
        constraints: Vec::new(),
        features: vec![FeatureDefinition {
            id: "body".into(),
            operation: FeatureOperation::Box {
                origin: point(zero(), zero(), zero()),
                size: point(parameter("width"), parameter("depth"), parameter("height")),
            },
        }],
        requirements: Vec::new(),
        datums: vec![
            datum(
                "top",
                DatumKind::Plane {
                    origin: point(zero(), zero(), parameter("height")),
                    normal: direction(0.0, 0.0, 1.0),
                },
            ),
            datum(
                "bottom",
                DatumKind::Plane {
                    origin: point(zero(), zero(), zero()),
                    normal: direction(0.0, 0.0, -1.0),
                },
            ),
            datum(
                "axis",
                DatumKind::Axis {
                    origin: point(half("width"), half("depth"), zero()),
                    direction: direction(0.0, 0.0, 1.0),
                },
            ),
            datum(
                "right",
                DatumKind::Plane {
                    origin: point(parameter("width"), zero(), zero()),
                    normal: direction(1.0, 0.0, 0.0),
                },
            ),
        ],
    }
}

/// A fixed base block and `count` free clones stacked on it, offset from the
/// origin and displaced from their solved positions; constrained stacks also
/// align axes and side faces, after a small turn about Z.
fn stacked_blocks(
    definition: &FamilyDefinition,
    count: usize,
    offset: f64,
    constrained: bool,
) -> InstanceGraph<'_> {
    let at = |x: f64, y: f64, z: f64| VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter);
    let mut graph = InstanceGraph::new(definition);
    graph.add_base("i0", HashMap::new(), "bench").unwrap();
    graph
        .set_placement("i0", Placement::translated(at(offset, -offset / 2.0, 0.0)))
        .unwrap();
    for index in 1..=count {
        let id = format!("i{index}");
        let previous = format!("i{}", index - 1);
        graph
            .add_clone(id.clone(), "i0", HashMap::new(), "bench")
            .unwrap();
        let rotation = constrained.then(|| AxisAngle {
            origin: at(0.0, 0.0, 0.0),
            axis: VectorQuantity::scalars(0.0, 0.0, 1.0),
            angle_radians: 0.2,
        });
        graph
            .set_placement(
                &id,
                Placement {
                    translation: at(
                        offset + index as f64 * 3.0,
                        -offset / 2.0 - 2.0,
                        index as f64 * 25.0,
                    ),
                    rotation,
                },
            )
            .unwrap();
        let mut relate = |suffix: &str, kind, first: &str, second: &str| {
            graph
                .add_relationship(AssemblyRelationship {
                    id: format!("{id}-{suffix}"),
                    kind,
                    first: DatumRef::new(previous.clone(), first),
                    second: DatumRef::new(id.clone(), second),
                })
                .unwrap();
        };
        relate("seat", RelationKind::Coincident, "top", "bottom");
        if constrained {
            relate("axis", RelationKind::Coincident, "axis", "axis");
            relate("square", RelationKind::Parallel, "right", "right");
        }
    }
    graph
}

// ---- measurement and reporting

fn failure(message: String) -> ModelError {
    ModelError { message }
}

fn timed(
    name: String,
    budget: Duration,
    expectation: Expectation,
    work: impl FnOnce() -> Result<String, ModelError>,
) -> Outcome {
    let start = Instant::now();
    let check = work().map_err(|error| error.to_string());
    Outcome {
        name,
        elapsed: start.elapsed(),
        budget,
        check,
        expectation,
    }
}

fn report(outcomes: &[Outcome]) -> ExitCode {
    let mut failures = 0;
    println!(
        "{:<52} {:>10} {:>10}  status  detail",
        "case", "time", "budget"
    );
    for outcome in outcomes {
        let status = match (outcome.passed(), outcome.expectation) {
            (true, Expectation::Required) => "PASS",
            (false, Expectation::Required) => {
                failures += 1;
                "FAIL"
            }
            (true, Expectation::KnownGap(_)) => "FIXED",
            (false, Expectation::KnownGap(_)) => "GAP",
        };
        let detail = match (&outcome.check, outcome.expectation) {
            (Ok(detail), Expectation::KnownGap(item)) => {
                format!("{detail}; now passes, update {item}")
            }
            (Err(problem), Expectation::KnownGap(item)) => format!("{problem} ({item})"),
            (Ok(detail), Expectation::Required) => detail.clone(),
            (Err(problem), Expectation::Required) => problem.clone(),
        };
        let budget = if outcome.budget == Duration::MAX {
            "-".to_owned()
        } else {
            format!("{:.3}s", outcome.budget.as_secs_f64())
        };
        println!(
            "{:<52} {:>9.3}s {:>10}  {:<6}  {detail}",
            outcome.name,
            outcome.elapsed.as_secs_f64(),
            budget,
            status
        );
    }
    if failures == 0 {
        ExitCode::SUCCESS
    } else {
        println!("{failures} case(s) failed their budget or check");
        ExitCode::FAILURE
    }
}
