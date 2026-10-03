//! Patterns, deep clone chains, and repeated regeneration of large graphs.

use super::*;

pub(crate) fn pattern_cases(definition: &'static FamilyDefinition) -> Vec<Outcome> {
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

pub(crate) fn deep_clone_chain(definition: &'static FamilyDefinition) -> Vec<Outcome> {
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
pub(crate) fn regeneration_handle_cases(definition: &'static FamilyDefinition) -> Vec<Outcome> {
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
