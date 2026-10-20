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

/// Most verification rows and outputs in a rejected build's report; failures
/// come first. The report is one stderr line, so it stays well under the
/// MCP server's 1 MiB read.
pub const MAX_REJECTED_ROWS: usize = 500;

/// `scope/requirement` labels of failed required requirements, in table
/// order. Assembly rows use the assembly's requirements, others the family's.
pub fn required_failures(document: &ModelDocument, verification: &Value) -> Vec<String> {
    let required = |scope: &str, id: &str| {
        if scope == "assembly" {
            document
                .assembly
                .requirements
                .iter()
                .any(|r| r.id == id && r.priority == RequirementPriority::Required)
        } else {
            document
                .family
                .requirements
                .iter()
                .any(|r| r.id == id && r.priority == RequirementPriority::Required)
        }
    };
    verification
        .as_array()
        .into_iter()
        .flatten()
        .filter(|row| row["status"] == "failed")
        .filter_map(|row| {
            let (scope, id) = (row["scope"].as_str()?, row["requirement"].as_str()?);
            required(scope, id).then(|| format!("{scope}/{id}"))
        })
        .collect()
}

/// Report fields for a build rejected by required requirements: the whole
/// verification table with measurements and witnesses, every selected
/// output's measurements, and the failing labels, each capped at
/// `MAX_REJECTED_ROWS` with counts of what was left out.
pub fn rejected(
    generation: &GraphRegeneration<'_>,
    verification: Value,
    outputs: Vec<Value>,
    failed: Vec<String>,
) -> Value {
    let rows = verification.as_array().cloned().unwrap_or_default();
    let total = rows.len();
    let passed = rows.iter().filter(|r| r["status"] == "passed").count();
    let (mut kept, rest): (Vec<_>, Vec<_>) =
        rows.into_iter().partition(|r| r["status"] != "passed");
    kept.extend(rest);
    kept.truncate(MAX_REJECTED_ROWS);
    let output_total = outputs.len();
    let failed_total = failed.len();
    json!({
        "accepted": false,
        "generated_variants": generation.generated_variants(),
        "required_failures": failed.into_iter().take(MAX_REJECTED_ROWS).collect::<Vec<_>>(),
        "verification": kept,
        "verification_counts": {
            "total": total,
            "passed": passed,
            "failed": total - passed,
            "required_failed": failed_total,
            "omitted": total - total.min(MAX_REJECTED_ROWS),
        },
        "outputs": outputs.into_iter().take(MAX_REJECTED_ROWS).collect::<Vec<_>>(),
        "outputs_omitted": output_total - output_total.min(MAX_REJECTED_ROWS),
    })
}

/// `required verification failed: a, b, ...` naming at most ten failures.
pub fn failure_message(failed: &[String]) -> String {
    let named = failed
        .iter()
        .take(10)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    match failed.len() {
        0..=10 => format!("required verification failed: {named}"),
        n => format!("required verification failed: {named} and {} more", n - 10),
    }
}
