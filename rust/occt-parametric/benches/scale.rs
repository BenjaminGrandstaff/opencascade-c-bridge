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

use occt_bridge::{Session, SessionOptions, ShapeType, Vec3};
use occt_parametric::{
    AssemblyRelationship, AssemblyRequirement, AssemblyVerificationRule, AxisAngle,
    DatumDefinition, DatumKind, DatumRef, Dimension, FamilyDefinition, FeatureDefinition,
    FeatureOperation, InstanceGraph, LengthUnit, ModelDocument, ModelError, ParameterDefinition,
    ParameterType, ParameterValue, PatternRule, Placement, Quantity, RelationKind,
    RelationshipTolerances, RequirementKind, RequirementPriority, ScalarExpr, VectorExpr,
    VectorQuantity,
};
use std::collections::HashMap;
use std::process::ExitCode;
use std::time::{Duration, Instant};

const PATTERN_MEMBERS: usize = 10_000;
const CLONE_CHAIN_DEPTH: usize = 20_000;
const REGENERATION_ROUNDS: usize = 10;
const REGENERATION_MEMBERS: usize = 1_000;
const VALIDATED_CUTS: usize = 100;
const PLACED_COPIES: usize = 5_000;
/// Rigid placement shares geometry; measured about 0.7 KiB per copy of a
/// 20-hole plate, against about 168 KiB when transforms copied geometry.
const PLACED_COPY_BUDGET_KIB: f64 = 4.0;
/// Largest acceptable slowdown of a boolean chain from result validation.
/// Measured: 1.45x at 25 cuts, 1.53x at 50, 1.57x at 100 (BRepCheck_Analyzer
/// alone took 1.65x, 1.75x, and 1.97x, growing with holes per face).
const VALIDATION_OVERHEAD_LIMIT: f64 = 2.0;
/// Holes drilled into one plate face by a single boolean.
const MANY_HOLES: usize = 400;
/// Largest acceptable slowdown of that boolean from validation. Measured
/// about 1.1x; BRepCheck_Analyzer alone took 1.5x, and its share grows
/// quadratically with holes per face (3.5x of a cut at 400, 5.6x at 800).
const MANY_HOLES_OVERHEAD_LIMIT: f64 = 1.5;

#[derive(Clone, Copy, PartialEq)]
enum Expectation {
    /// Must meet its budget and correctness check.
    Required,
    /// Tied to an open roadmap item; reported but does not fail the run.
    /// Kept for the next measured gap even while no case uses it.
    #[allow(dead_code)]
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
    // First, so freed memory from other cases cannot hide growth.
    outcomes.push(placed_copy_memory());
    outcomes.extend(pattern_cases(definition));
    outcomes.extend(deep_clone_chain(definition));
    outcomes.extend(regeneration_handle_cases(definition));
    outcomes.extend(solver_cases(definition));
    outcomes.push(solver_grid(definition));
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
    outcomes.push(timed(
        format!("assembly requirements {size}: add datum clearances"),
        ms(2_000),
        Expectation::Required,
        || {
            let requirements = (0..size)
                .map(|index| AssemblyRequirement {
                    id: format!("clearance[{index}]"),
                    version: 1,
                    kind: RequirementKind::Assembly,
                    priority: RequirementPriority::Advisory,
                    statement: "Pattern member datum stays measurable from its source".into(),
                    rule: AssemblyVerificationRule::DatumClearance {
                        first: DatumRef::new("source", "axis"),
                        second: DatumRef::new(format!("m[{index}]"), "axis"),
                        minimum: Quantity::length(0.0, LengthUnit::Millimeter),
                        maximum: None,
                    },
                    provenance: "bench".into(),
                })
                .collect::<Vec<_>>();
            graph.add_assembly_requirements(requirements)?;
            Ok(format!("{size} validated requirements"))
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
            if generation.verification().len() != size {
                return Err(failure(format!(
                    "{} assembly verification results, expected {size}",
                    generation.verification().len()
                )));
            }
            Ok(format!("{} shapes", session.shape_count()?))
        },
    ));
    outcomes
}

fn deep_clone_chain(definition: &'static FamilyDefinition) -> Vec<Outcome> {
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
    std::thread::spawn(move || {
        let deepest = timed(
            format!("clone chain {CLONE_CHAIN_DEPTH}: resolve deepest"),
            ms(2_000),
            Expectation::Required,
            || {
                graph
                    .resolve(&deepest)
                    .map(|_| "resolved iteratively on default stack".into())
            },
        );
        graph.add_configuration("deep").unwrap();
        let all = timed(
            format!("clone chain {CLONE_CHAIN_DEPTH}: validate all with memoized parents"),
            ms(2_000),
            Expectation::Required,
            || {
                graph
                    .set_configuration_override("deep", "c0", "width", length(12.0))
                    .map(|()| format!("validated {CLONE_CHAIN_DEPTH} instances"))
            },
        );
        vec![deepest, all]
    })
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
        (50, 1_000.0, false, None, ms(1_000)),
        (20, 1_000.0, true, None, ms(500)),
        (50, 1_000.0, true, None, ms(1_000)),
        (1_000, 1_000.0, false, None, ms(2_000)),
        (1_000, 1_000.0, true, None, ms(5_000)),
        (50, 1_000_000.0, false, None, ms(1_000)),
        (50, 1_000_000.0, false, Some(1e-8), ms(1_000)),
    ];
    cases
        .into_iter()
        .map(|(count, offset, constrained, linear_tolerance, budget)| {
            let mut graph = stacked_blocks(definition, count, offset, constrained);
            if let Some(linear_millimeters) = linear_tolerance {
                graph
                    .set_relationship_tolerances(RelationshipTolerances {
                        linear_millimeters,
                        ..RelationshipTolerances::default()
                    })
                    .unwrap();
            }
            let ids = (1..=count)
                .map(|index| format!("i{index}"))
                .collect::<Vec<_>>();
            let ids = ids.iter().map(String::as_str).collect::<Vec<_>>();
            let kind = if constrained { "constrained" } else { "seated" };
            let tolerance = linear_tolerance
                .map(|value| format!(" at {value:.0e} mm tolerance"))
                .unwrap_or_default();
            timed(
                format!("solve {count} {kind} parts at {offset} mm{tolerance}"),
                budget,
                Expectation::Required,
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
    let mut outcomes = vec![unchecked, checked];
    outcomes.extend(many_hole_validation_cases());
    outcomes
}

/// Drills `MANY_HOLES` holes into one plate with a single boolean, with
/// validation off and on, and bounds the overhead of checking a face with
/// hundreds of wires.
fn many_hole_validation_cases() -> Vec<Outcome> {
    let drill = |validate: bool| -> Result<Duration, ModelError> {
        let session = Session::new()?;
        session.set_options(SessionOptions {
            validate_results: validate,
            ..SessionOptions::default()
        })?;
        let plate = session.create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(420.0, 420.0, 10.0))?;
        let holes = (0..MANY_HOLES)
            .map(|index| {
                let (row, column) = ((index / 20) as f64, (index % 20) as f64);
                session.create_cylinder(
                    Vec3::new(10.0 + column * 20.0, 10.0 + row * 20.0, -1.0),
                    Vec3::new(0.0, 0.0, 1.0),
                    4.0,
                    12.0,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        let tools = session.create_compound(&holes.iter().collect::<Vec<_>>())?;
        let start = Instant::now();
        let drilled = session.cut(&plate, &tools)?;
        let elapsed = start.elapsed();
        let faces = session.subshape_count(&drilled, ShapeType::Face)?;
        if faces != MANY_HOLES + 6 {
            return Err(failure(format!(
                "expected {} faces, found {faces}",
                MANY_HOLES + 6
            )));
        }
        Ok(elapsed)
    };
    let mut baseline = Duration::ZERO;
    let unchecked = timed(
        format!("{MANY_HOLES}-hole plate in one cut: validation off"),
        ms(5_000),
        Expectation::Required,
        || {
            baseline = drill(false)?;
            Ok("baseline".into())
        },
    );
    let checked = timed(
        format!("{MANY_HOLES}-hole plate in one cut: validation on"),
        ms(7_500),
        Expectation::Required,
        || {
            let elapsed = drill(true)?;
            let ratio = elapsed.as_secs_f64() / baseline.as_secs_f64().max(1e-9);
            if ratio <= MANY_HOLES_OVERHEAD_LIMIT {
                Ok(format!("{ratio:.2}x of unvalidated"))
            } else {
                Err(failure(format!(
                    "validation costs {ratio:.2}x, limit {MANY_HOLES_OVERHEAD_LIMIT}x"
                )))
            }
        },
    );
    vec![unchecked, checked]
}

/// Places `PLACED_COPIES` rigid copies of a 20-hole plate and bounds the
/// resident memory each copy adds; shared geometry keeps it near constant.
fn placed_copy_memory() -> Outcome {
    timed(
        format!("place {PLACED_COPIES} copies: memory per copy"),
        ms(5_000),
        Expectation::Required,
        || {
            let session = Session::new()?;
            let mut plate =
                session.create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(200.0, 200.0, 10.0))?;
            for index in 0..20 {
                let (row, column) = ((index / 5) as f64, (index % 5) as f64);
                let hole = session.create_cylinder(
                    Vec3::new(20.0 + column * 40.0, 20.0 + row * 40.0, -1.0),
                    Vec3::new(0.0, 0.0, 1.0),
                    6.0,
                    12.0,
                )?;
                plate = session.cut(&plate, &hole)?;
            }
            let before = resident_kib();
            let copies = (0..PLACED_COPIES)
                .map(|index| {
                    let turned = session.rotate(
                        &plate,
                        Vec3::new(0.0, 0.0, 0.0),
                        Vec3::new(0.0, 0.0, 1.0),
                        0.1,
                    )?;
                    session.translate(&turned, Vec3::new(index as f64 * 250.0, 0.0, 0.0))
                })
                .collect::<Result<Vec<_>, _>>()?;
            let Some(grown) = resident_kib()
                .zip(before)
                .map(|(after, start)| after.saturating_sub(start))
            else {
                return Ok(format!(
                    "{} copies; memory not measurable here",
                    copies.len()
                ));
            };
            let per_copy = grown as f64 / PLACED_COPIES as f64;
            if per_copy <= PLACED_COPY_BUDGET_KIB {
                Ok(format!(
                    "{per_copy:.2} KiB per copy (budget {PLACED_COPY_BUDGET_KIB} KiB)"
                ))
            } else {
                Err(failure(format!(
                    "{per_copy:.1} KiB per copy exceeds {PLACED_COPY_BUDGET_KIB} KiB"
                )))
            }
        },
    )
}

/// Resident set size in KiB on Linux; `None` elsewhere.
fn resident_kib() -> Option<u64> {
    std::fs::read_to_string("/proc/self/status")
        .ok()?
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
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

const GRID_ROWS: usize = 30;
const GRID_COLUMNS: usize = 34;
const GRID_PITCH: f64 = 50.0;

/// A grid of blocks, each tied to its left and upper neighbors (coplanar
/// tops, parallel sides, axis distances), so elimination meets
/// two-dimensional fill-in rather than a chain.
fn solver_grid(definition: &'static FamilyDefinition) -> Outcome {
    let at = |x: f64, y: f64, z: f64| VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter);
    let id = |row: usize, column: usize| format!("g{row}_{column}");
    let mut graph = InstanceGraph::new(definition);
    for row in 0..GRID_ROWS {
        for column in 0..GRID_COLUMNS {
            let name = id(row, column);
            let (x, y) = (column as f64 * GRID_PITCH, row as f64 * GRID_PITCH);
            if row == 0 && column == 0 {
                graph
                    .add_base(name.clone(), HashMap::new(), "bench")
                    .unwrap();
                continue;
            }
            graph
                .add_clone(name.clone(), id(0, 0), HashMap::new(), "bench")
                .unwrap();
            // Start a few millimeters and a small turn away from the solution.
            let wobble = ((row * 31 + column * 17) % 7) as f64 - 3.0;
            graph
                .set_placement(
                    &name,
                    Placement {
                        translation: at(x + wobble, y - wobble, wobble),
                        rotation: Some(AxisAngle {
                            origin: at(0.0, 0.0, 0.0),
                            axis: VectorQuantity::scalars(0.0, 0.0, 1.0),
                            angle_radians: 0.01 * wobble,
                        }),
                    },
                )
                .unwrap();
            let mut relate = |suffix: &str, kind, other: String, first: &str, second: &str| {
                graph
                    .add_relationship(AssemblyRelationship {
                        id: format!("{name}-{suffix}"),
                        kind,
                        first: DatumRef::new(other, first),
                        second: DatumRef::new(name.clone(), second),
                    })
                    .unwrap();
            };
            let pitch =
                RelationKind::Distance(Quantity::length(GRID_PITCH, LengthUnit::Millimeter));
            if column > 0 {
                relate(
                    "level",
                    RelationKind::Coincident,
                    id(row, column - 1),
                    "top",
                    "top",
                );
                relate("left", pitch, id(row, column - 1), "axis", "axis");
                relate(
                    "square",
                    RelationKind::Parallel,
                    id(row, column - 1),
                    "right",
                    "right",
                );
            }
            if row > 0 {
                relate("up", pitch, id(row - 1, column), "axis", "axis");
                if column == 0 {
                    relate(
                        "level",
                        RelationKind::Coincident,
                        id(row - 1, column),
                        "top",
                        "top",
                    );
                    relate(
                        "square",
                        RelationKind::Parallel,
                        id(row - 1, column),
                        "right",
                        "right",
                    );
                }
            }
        }
    }
    let free = (0..GRID_ROWS)
        .flat_map(|row| (0..GRID_COLUMNS).map(move |column| (row, column)))
        .filter(|&cell| cell != (0, 0))
        .map(|(row, column)| id(row, column))
        .collect::<Vec<_>>();
    let ids = free.iter().map(String::as_str).collect::<Vec<_>>();
    timed(
        format!(
            "solve {}x{} grid ({} parts)",
            GRID_ROWS,
            GRID_COLUMNS,
            ids.len()
        ),
        ms(20_000),
        Expectation::Required,
        || {
            let solution = graph.solve_placements(&ids)?;
            if solution.solved {
                Ok(format!(
                    "{} iterations, {} free degrees",
                    solution.iterations, solution.free_degrees
                ))
            } else {
                Err(failure(format!(
                    "unsolved, max residual {:.1e}",
                    solution.max_residual
                )))
            }
        },
    )
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
    ModelError {
        message,
        diagnostics: Vec::new(),
    }
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
