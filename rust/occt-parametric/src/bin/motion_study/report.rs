use occt_bridge::Vec3;
use occt_parametric::*;
use serde_json::{Value, json};

fn point(value: Vec3) -> [f64; 3] {
    [value.x, value.y, value.z]
}

pub fn pair(check: &PairCheck) -> Value {
    let status = match check.status {
        PairStatus::Clear => "clear",
        PairStatus::Touching => "touching",
        PairStatus::Interference => "interference",
        PairStatus::InsufficientClearance => "insufficient_clearance",
    };
    json!({
        "first": check.first, "second": check.second, "status": status,
        "separation_mm": check.separation_mm,
        "overlap_volume_mm3": check.overlap_volume_mm3,
        "first_witness_mm": point(check.first_witness_mm),
        "second_witness_mm": point(check.second_witness_mm),
    })
}

fn status(value: ContinuousStatus) -> &'static str {
    match value {
        ContinuousStatus::Clear => "clear",
        ContinuousStatus::Collision => "collision",
        ContinuousStatus::Unresolved => "unresolved",
    }
}

pub fn document(sampled: &MotionResult, continuous: &ContinuousMotionResult) -> Value {
    let samples: Vec<_> = sampled
        .samples
        .iter()
        .map(|sample| {
            let relationships: Vec<_> = sample
                .relationships
                .iter()
                .map(|check| {
                    json!({
                        "id": check.id, "satisfied": check.satisfied,
                        "linear_residual_mm": check.linear_residual,
                        "angular_residual_radians": check.angular_residual,
                    })
                })
                .collect();
            json!({
                "index": sample.index, "positions": sample.positions,
                "collisions": sample.collisions.iter().map(pair).collect::<Vec<_>>(),
                "relationships": relationships,
            })
        })
        .collect();
    let pairs: Vec<_> = continuous
        .pairs
        .iter()
        .map(|result| {
            json!({
                "segment": result.segment, "first": result.first, "second": result.second,
                "status": status(result.status), "fraction": result.fraction,
                "check": result.check.as_ref().map(pair),
                "unresolved_fraction_range": result.unresolved_fraction_range,
            })
        })
        .collect();
    json!({
        "schema": "occb-motion-report-v1",
        "passed": clear(sampled, continuous),
        "sampled": { "generated_variants": sampled.generated_variants, "samples": samples },
        "continuous": {
            "status": status(continuous.status), "pairs": pairs,
            "segments": continuous.segments, "candidate_pairs": continuous.candidate_pairs,
            "exact_queries": continuous.exact_queries,
            "bounds_rejected_intervals": continuous.bounds_rejected_intervals,
            "generated_variants": continuous.generated_variants,
            "unresolved_pairs": continuous.unresolved_pairs,
        },
    })
}

pub fn clear(sampled: &MotionResult, continuous: &ContinuousMotionResult) -> bool {
    continuous.status == ContinuousStatus::Clear
        && continuous.unresolved_pairs == 0
        && sampled.samples.iter().all(|sample| {
            sample
                .collisions
                .iter()
                .all(|check| check.status == PairStatus::Clear)
                && sample.relationships.iter().all(|check| check.satisfied)
        })
}
