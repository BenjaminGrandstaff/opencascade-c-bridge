//! Structured STEP export at assembly scale.

use super::*;
use occt_parametric::OutputSet;

/// 10,000 pattern members written as one assembly sharing a single part.
pub(crate) fn step_assembly_case(definition: &'static FamilyDefinition) -> Outcome {
    const MEMBERS: usize = 10_000;
    timed(
        format!("STEP assembly of {MEMBERS} pattern members: shared part export"),
        ms(4_000),
        Expectation::Required,
        || {
            let session = Session::new().map_err(|error| failure(error.to_string()))?;
            let mut graph = InstanceGraph::new(definition);
            graph.add_base("source", HashMap::new(), "bench")?;
            graph.add_linear_pattern(
                "row",
                "member",
                "source",
                MEMBERS,
                VectorQuantity::lengths(50.0, 0.0, 0.0, LengthUnit::Millimeter),
                "bench",
            )?;
            let generation = graph.regenerate_all(&session)?;
            let path = std::env::temp_dir().join(format!("occb-bench-{}.step", std::process::id()));
            let parts = graph.export_step(
                &session,
                &generation,
                &path,
                "row",
                &OutputSet::AllWithOutput("body".into()),
            )?;
            let bytes = std::fs::metadata(&path)
                .map_err(|error| failure(error.to_string()))?
                .len();
            std::fs::remove_file(&path).map_err(|error| failure(error.to_string()))?;
            let per_component = bytes as f64 / (MEMBERS + 1) as f64;
            // A placement record is a few hundred bytes; a copied box is several KiB.
            if parts != 1 || per_component > 1_024.0 {
                return Err(failure(format!(
                    "{parts} parts, {per_component:.0} bytes per component"
                )));
            }
            Ok(format!(
                "{} components, 1 shared part, {per_component:.0} bytes per component",
                MEMBERS + 1
            ))
        },
    )
}
