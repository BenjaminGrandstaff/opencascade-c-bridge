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
    ClearanceSeries, CoordinateAxis, DatumDefinition, DatumKind, DatumRef, Dimension, Extremum,
    FaceSelector, FamilyDefinition, FeatureDefinition, FeatureOperation, HoleExtent, HoleFinish,
    InstanceGraph, LengthUnit, ModelDocument, ModelError, ParameterDefinition, ParameterType,
    ParameterValue, PartInstance, PatternRule, Placement, Quantity, RelationKind,
    RelationshipTolerances, RequirementKind, RequirementPriority, ScalarExpr, SketchArc,
    SketchCircle, SketchConstraint, SketchDefinition, SketchLine, SketchPoint, ThreadHandedness,
    ThreadSpecification, VectorExpr, VectorQuantity, iso273_clearance_v1,
};
use std::collections::HashMap;
use std::process::ExitCode;
use std::time::{Duration, Instant};

mod draft;
mod graphs;
mod holes;
mod memory;
mod ribs;
mod sketches;
mod solver;
mod validation;
mod variable_fillet;

use graphs::*;
use holes::*;
use memory::*;
use sketches::*;
use solver::*;
use validation::*;

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
    outcomes.push(sketch_solver_case());
    outcomes.push(curved_sketch_solver_case());
    outcomes.push(datum_sketch_wire_case());
    outcomes.push(profile_sweep_case(false));
    outcomes.push(profile_sweep_case(true));
    outcomes.push(hole_features_case());
    outcomes.push(draft::draft_features_case());
    outcomes.push(ribs::rib_features_case(false));
    outcomes.push(ribs::rib_features_case(true));
    outcomes.push(ribs::open_rib_features_case());
    outcomes.push(ribs::next_rib_features_case());
    outcomes.push(variable_fillet::variable_fillet_features_case());
    outcomes.push(clearance_catalog_case());
    outcomes.push(threaded_hole_features_case());
    outcomes.push(entry_hole_features_case(false));
    outcomes.push(entry_hole_features_case(true));
    outcomes.push(large_sketch_case(false));
    outcomes.push(large_sketch_case(true));
    outcomes.push(solver_grid(definition));
    outcomes.extend(validation_cases());
    report(&outcomes)
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
