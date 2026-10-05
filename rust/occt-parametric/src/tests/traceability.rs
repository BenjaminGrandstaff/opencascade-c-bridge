//! Assumptions and requirement trace links, and the requirements change
//! impact reports for re-verification.

use super::*;

fn requirement(id: &str, rule: VerificationRule, traces: Vec<TraceTarget>) -> Requirement {
    Requirement {
        id: id.into(),
        version: 1,
        kind: RequirementKind::Functional,
        priority: RequirementPriority::Advisory,
        statement: format!("{id} holds"),
        rule,
        provenance: "test".into(),
        traces,
    }
}

/// The block family with a stated load assumption and three requirements:
/// one on the body alone, one on the placed output tracing the width, and a
/// stiffness requirement on the body tracing the load assumption.
fn traced_family() -> FamilyDefinition {
    let mut family = family(RequirementPriority::Advisory, 1e12);
    family.assumptions = vec![Assumption {
        id: "load".into(),
        statement: "The bracket carries at most 50 N".into(),
        provenance: "customer brief".into(),
    }];
    let valid = |output: &str| VerificationRule::ShapeValid {
        output: output.into(),
    };
    family.requirements = vec![
        requirement("body.valid", valid("body"), vec![]),
        requirement(
            "placed.width",
            valid("placed"),
            vec![TraceTarget::Parameter("width".into())],
        ),
        requirement(
            "body.stiff",
            valid("body"),
            vec![
                TraceTarget::Assumption("load".into()),
                TraceTarget::Feature("body".into()),
            ],
        ),
    ];
    family
}

fn document(family: &FamilyDefinition) -> ModelDocument {
    let mut graph = InstanceGraph::new(family);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    ModelDocument::from_graph(&graph)
}

fn impacted(before: &ModelDocument, after: &ModelDocument) -> Vec<String> {
    let impact = before.change_impact(after).unwrap();
    assert!(impact.instances.len() <= 1);
    impact
        .instances
        .first()
        .map(|instance| instance.requirements.clone())
        .unwrap_or_default()
}

#[test]
fn change_impact_names_the_requirements_to_reverify() {
    let family = traced_family();
    let before = document(&family);

    // Restating the assumption touches no geometry, only what rests on it.
    let mut restated = before.clone();
    restated.family.assumptions[0].statement = "The bracket carries at most 80 N".into();
    let impact = before.change_impact(&restated).unwrap();
    assert!(impact.instances[0].features.is_empty());
    assert_eq!(impact.instances[0].requirements, vec!["body.stiff"]);

    // A width edit rebuilds both features, so every requirement on them and
    // the one tracing the width needs re-verification.
    let mut widened = before.clone();
    widened.family.parameters[0].default =
        ParameterValue::Scalar(Quantity::length(12.0, LengthUnit::Millimeter));
    assert_eq!(
        impacted(&before, &widened),
        vec!["body.stiff", "body.valid", "placed.width"]
    );

    // Editing one requirement's statement flags only that requirement.
    let mut reworded = before.clone();
    reworded.family.requirements[1].statement = "placed output stays valid".into();
    assert_eq!(impacted(&before, &reworded), vec!["placed.width"]);

    // Adding a requirement flags it; nothing else changed.
    let mut extended = before.clone();
    extended.family.requirements.push(requirement(
        "placed.volume",
        VerificationRule::ShapeValid {
            output: "placed".into(),
        },
        vec![],
    ));
    assert_eq!(impacted(&before, &extended), vec!["placed.volume"]);

    // An unchanged document reports nothing.
    assert!(
        before
            .change_impact(&before.clone())
            .unwrap()
            .instances
            .is_empty()
    );
}

#[test]
fn traces_and_assumptions_validate_and_persist() {
    let session = Session::new().unwrap();
    let regenerate = |family: &FamilyDefinition| {
        PartInstance {
            id: "part".into(),
            definition: family,
            overrides: HashMap::new(),
            provenance: "test".into(),
        }
        .regenerate(&session)
        .map(drop)
    };
    regenerate(&traced_family()).unwrap();
    for (target, message) in [
        (
            TraceTarget::Feature("bolt".into()),
            "unknown feature 'bolt'",
        ),
        (
            TraceTarget::Parameter("length".into()),
            "unknown parameter 'length'",
        ),
        (
            TraceTarget::Assumption("wind".into()),
            "unknown assumption 'wind'",
        ),
    ] {
        let mut family = traced_family();
        family.requirements[0].traces.push(target);
        let error = regenerate(&family).unwrap_err();
        assert!(error.message.contains(message), "{}", error.message);
    }
    let mut repeated = traced_family();
    repeated.assumptions.push(repeated.assumptions[0].clone());
    assert!(
        regenerate(&repeated)
            .unwrap_err()
            .message
            .contains("assumption ids")
    );
    let mut blank = traced_family();
    blank.assumptions[0].statement = " ".into();
    assert!(
        regenerate(&blank)
            .unwrap_err()
            .message
            .contains("needs a statement")
    );

    let document = document(&traced_family());
    let json = document.to_json_pretty().unwrap();
    assert!(json.contains("\"assumptions\"") && json.contains("\"traces\""));
    assert_eq!(ModelDocument::from_json(&json).unwrap(), document);
    // Families without either omit the fields.
    let mut plain = traced_family();
    plain.assumptions.clear();
    for requirement in &mut plain.requirements {
        requirement.traces.clear();
    }
    let json = serde_json::to_string(&plain).unwrap();
    assert!(!json.contains("assumptions") && !json.contains("traces"));
}
