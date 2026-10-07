use occt_parametric::*;
use serde_json::{Value, json};

pub fn diagnostics(items: &[FeatureDiagnostic]) -> Value {
    json!(
        items
            .iter()
            .map(
                |d| json!({"feature":d.feature,"kind":format!("{:?}",d.kind),
        "code":d.code,"name":d.name,"selection":d.selection,"selector":d.selector,"input":d.input})
            )
            .collect::<Vec<_>>()
    )
}
pub(super) fn result(scope: &str, r: &VerificationResult) -> Value {
    let measured = r.measured.map(|m| json!({"value":m.value,"unit":format!("{:?}",m.unit),"minimum":m.minimum,"maximum":m.maximum}));
    let evidence = match r.evidence {
        Evidence::Exact => json!({"kind":"exact"}),
        Evidence::Sampled {
            samples,
            unresolved,
        } => json!({"kind":"sampled","samples":samples,"unresolved":unresolved}),
    };
    let witness = r.witness.as_ref().map(|w| json!({"subjects":w.subjects,"points_mm":w.points_mm.iter().map(|p| [p.x,p.y,p.z]).collect::<Vec<_>>()}));
    json!({"scope":scope,"requirement":r.requirement_id,
        "status":if r.status == VerificationStatus::Passed {"passed"} else {"failed"},
        "message":r.message,"measured":measured,"evidence":evidence,"witness":witness})
}
pub fn verification(generation: &GraphRegeneration<'_>) -> Value {
    let mut instances = generation.instances().collect::<Vec<_>>();
    instances.sort_by_key(|(id, _)| *id);
    let mut results = Vec::new();
    for (id, generated) in instances {
        results.extend(generated.verification.iter().map(|r| result(id, r)));
    }
    results.extend(
        generation
            .verification()
            .iter()
            .map(|r| result("assembly", r)),
    );
    json!(results)
}
