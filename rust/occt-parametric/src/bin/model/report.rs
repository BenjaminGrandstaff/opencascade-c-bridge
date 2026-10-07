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
pub(super) fn result(scope: &str, result: &VerificationResult) -> Value {
    super::view_data::verification_result(scope, result)
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
