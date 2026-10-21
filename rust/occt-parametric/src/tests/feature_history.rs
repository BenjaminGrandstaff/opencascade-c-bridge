//! Feature history as a commit timeline (schema 182): checkout, suppress
//! and reorder, regenerated with exact volumes.

use super::*;
use std::f64::consts::PI;

fn at(x: f64, y: f64, z: f64) -> VectorExpr {
    VectorExpr::Literal(VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter))
}
fn feature(id: &str, operation: FeatureOperation) -> FeatureDefinition {
    FeatureDefinition {
        id: id.into(),
        operation,
    }
}

/// base (20 mm cube) → notched (a 10 mm corner cut away by `tool`) →
/// drilled (a Ø4 bore cut through) → moved (100 mm along x), with a required
/// volume check on `moved`.
fn part_family() -> FamilyDefinition {
    let mut family = FamilyDefinition {
        id: "part".into(),
        version: 1,
        parameters: vec![],
        derived_parameters: vec![],
        derived_vector_parameters: vec![],
        constraints: vec![],
        features: vec![
            feature(
                "base",
                FeatureOperation::Box {
                    origin: at(0.0, 0.0, 0.0),
                    size: at(20.0, 20.0, 20.0),
                },
            ),
            feature(
                "tool",
                FeatureOperation::Box {
                    origin: at(10.0, 10.0, 10.0),
                    size: at(10.0, 10.0, 10.0),
                },
            ),
            feature(
                "notched",
                FeatureOperation::Cut {
                    object: "base".into(),
                    tool: "tool".into(),
                },
            ),
            feature(
                "bore",
                FeatureOperation::Cylinder {
                    origin: at(5.0, 5.0, -5.0),
                    axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
                    radius: ScalarExpr::Literal(Quantity::length(2.0, LengthUnit::Millimeter)),
                    height: ScalarExpr::Literal(Quantity::length(30.0, LengthUnit::Millimeter)),
                },
            ),
            feature(
                "drilled",
                FeatureOperation::Cut {
                    object: "notched".into(),
                    tool: "bore".into(),
                },
            ),
            feature(
                "moved",
                FeatureOperation::Translate {
                    input: "drilled".into(),
                    offset: at(100.0, 0.0, 0.0),
                },
            ),
        ],
        requirements: vec![],
        datums: vec![],
        references: vec![],
        feature_colors: Default::default(),
        assumptions: vec![],
        history: FeatureHistory::default(),
    };
    family.requirements.push(Requirement {
        id: "moved-volume".into(),
        version: 1,
        kind: RequirementKind::Dimensional,
        priority: RequirementPriority::Required,
        statement: "the finished part keeps the notch and bore".into(),
        rule: VerificationRule::VolumeRange {
            output: "moved".into(),
            minimum: Volume {
                value: 6000.0,
                unit: LengthUnit::Millimeter,
            },
            maximum: Volume {
                value: 7000.0,
                unit: LengthUnit::Millimeter,
            },
        },
        provenance: "test".into(),
        traces: vec![],
    });
    family
}

/// Volumes of the features that produced a shape, and the requirement ids
/// evaluated.
fn build(family: &FamilyDefinition) -> Result<(HashMap<String, f64>, Vec<String>), ModelError> {
    let session = Session::new().unwrap();
    let part = PartInstance {
        id: "part".into(),
        definition: family,
        overrides: HashMap::new(),
        provenance: "test".into(),
    };
    let result = part.regenerate(&session).map(|generated| {
        let volumes = family
            .features
            .iter()
            .filter_map(|f| {
                generated
                    .shape(&f.id)
                    .map(|shape| (f.id.clone(), session.volume(shape).unwrap()))
            })
            .collect();
        let checked = generated
            .verification
            .iter()
            .map(|r| r.requirement_id.clone())
            .collect();
        (volumes, checked)
    });
    assert_eq!(session.shape_count().unwrap(), 0, "handles released");
    result
}

fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-6, "{actual} vs {expected}");
}

#[test]
fn suppressed_modifiers_replay_later_features_over_their_input() {
    let bore = PI * 4.0 * 20.0;
    let (volumes, checked) = build(&part_family()).unwrap();
    close(volumes["moved"], 7000.0 - bore);
    assert_eq!(checked, ["moved-volume"]);
    // Reverting the notch: the bore and move replay over the plain cube.
    let mut family = part_family();
    family.requirements[0].priority = RequirementPriority::Advisory;
    family.history.suppressed = vec!["notched".into()];
    let (volumes, _) = build(&family).unwrap();
    close(volumes["notched"], 8000.0);
    close(volumes["moved"], 8000.0 - bore);
    // ...which the required check now rejects, like any other edit.
    family.requirements[0].priority = RequirementPriority::Required;
    let error = PartInstance {
        id: "part".into(),
        definition: &family,
        overrides: HashMap::new(),
        provenance: "test".into(),
    }
    .regenerate(&Session::new().unwrap())
    .err()
    .unwrap();
    assert!(error.message.contains("moved-volume"), "{}", error.message);
    // Reverting both cuts leaves the moved cube.
    family.history.suppressed = vec!["notched".into(), "drilled".into()];
    family.requirements.clear();
    let (volumes, _) = build(&family).unwrap();
    close(volumes["moved"], 8000.0);
}

#[test]
fn suppressed_bodies_and_rollback_leave_dependents_and_their_checks_out() {
    // The tool is a new body: without it the notch, bore cut and move are
    // out of the build, and the check on `moved` is not evaluated.
    let mut family = part_family();
    family.history.suppressed = vec!["tool".into()];
    let (volumes, checked) = build(&family).unwrap();
    let mut built: Vec<_> = volumes.keys().cloned().collect();
    built.sort();
    assert_eq!(built, ["base", "bore"]);
    assert!(checked.is_empty());
    // Checking out the timeline at the notch rolls back everything after.
    let mut family = part_family();
    family.history.head = Some("notched".into());
    let (volumes, checked) = build(&family).unwrap();
    let mut built: Vec<_> = volumes.keys().cloned().collect();
    built.sort();
    assert_eq!(built, ["base", "notched", "tool"]);
    close(volumes["notched"], 7000.0);
    assert!(checked.is_empty());
    // Moving the head forward again restores the full build.
    family.history.head = Some("moved".into());
    assert_eq!(build(&family).unwrap().0.len(), 6);
}

#[test]
fn reordered_features_build_the_same_part_and_history_round_trips() {
    let bore = PI * 4.0 * 20.0;
    let mut family = part_family();
    family.move_feature("bore", None).unwrap();
    family.move_feature("tool", Some("bore")).unwrap();
    assert_eq!(family.features[0].id, "bore");
    close(build(&family).unwrap().0["moved"], 7000.0 - bore);
    assert!(family.move_feature("moved", Some("notched")).is_err());
    // The history saves and loads with the model, and bad names are refused.
    family.history = FeatureHistory {
        head: Some("drilled".into()),
        suppressed: vec!["notched".into()],
    };
    let mut graph = InstanceGraph::new(&family);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let document = ModelDocument::from_graph(&graph);
    let text = document.to_json_pretty().unwrap();
    assert!(text.contains("\"history\""), "{text}");
    assert_eq!(
        ModelDocument::from_json(&text).unwrap().family.history,
        family.history
    );
    let mut bad = document.clone();
    bad.family.history.head = Some("ghost".into());
    assert!(bad.to_json_pretty().is_err());
    let error = ModelDocument::from_json(&serde_json::to_string(&bad).unwrap())
        .unwrap_err()
        .message;
    assert!(error.contains("unknown feature 'ghost'"), "{error}");
    // Without history the document has no history key at all.
    family.history = FeatureHistory::default();
    let mut graph = InstanceGraph::new(&family);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    assert!(
        !ModelDocument::from_graph(&graph)
            .to_json_pretty()
            .unwrap()
            .contains("\"history\"")
    );
}
