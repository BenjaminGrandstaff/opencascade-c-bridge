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

/// 10,000 pattern members in 100 patterns, each in its own frame under one
/// of 10 turned top-level frames: 110 nested sub-assemblies, one part.
pub(crate) fn step_frames_case(definition: &'static FamilyDefinition) -> Outcome {
    const GROUPS: usize = 10;
    const PATTERNS: usize = 100;
    const MEMBERS: usize = 100;
    timed(
        format!(
            "STEP assembly of {} members in {} nested frames",
            PATTERNS * MEMBERS,
            GROUPS + PATTERNS
        ),
        ms(3_000),
        Expectation::Required,
        || {
            let session = Session::new().map_err(|error| failure(error.to_string()))?;
            let mut graph = InstanceGraph::new(definition);
            let lengths =
                |x: f64, y: f64| VectorQuantity::lengths(x, y, 0.0, LengthUnit::Millimeter);
            for group in 0..GROUPS {
                graph.add_frame(
                    format!("group-{group}"),
                    None,
                    Placement {
                        translation: lengths(0.0, 2_000.0 * group as f64),
                        rotation: Some(AxisAngle {
                            origin: lengths(0.0, 0.0),
                            axis: VectorQuantity::scalars(0.0, 0.0, 1.0),
                            angle_radians: 0.01 * group as f64,
                        }),
                    },
                    "bench",
                )?;
            }
            graph.add_base("source", HashMap::new(), "bench")?;
            for pattern in 0..PATTERNS {
                let frame = format!("row-{pattern}");
                graph.add_frame(
                    frame.as_str(),
                    Some(format!("group-{}", pattern % GROUPS).as_str()),
                    Placement::translated(lengths(0.0, 100.0 * (pattern / GROUPS) as f64)),
                    "bench",
                )?;
                let id = format!("pattern-{pattern}");
                graph.add_linear_pattern(
                    id.as_str(),
                    format!("member-{pattern}").as_str(),
                    "source",
                    MEMBERS,
                    lengths(50.0, 0.0),
                    "bench",
                )?;
                graph.set_pattern_frame(&id, Some(&frame))?;
            }
            let generation = graph.regenerate_all(&session)?;
            let path =
                std::env::temp_dir().join(format!("occb-bench-frames-{}.step", std::process::id()));
            let parts = graph.export_step(
                &session,
                &generation,
                &path,
                "fleet",
                &OutputSet::AllWithOutput("body".into()),
            )?;
            let text =
                std::fs::read_to_string(&path).map_err(|error| failure(error.to_string()))?;
            std::fs::remove_file(&path).map_err(|error| failure(error.to_string()))?;
            let occurrences = text.matches("NEXT_ASSEMBLY_USAGE_OCCURRENCE(").count();
            // Every pattern also places its source member, plus the base.
            let components = PATTERNS * MEMBERS + 1;
            let expected = components + GROUPS + PATTERNS;
            if parts != 1 || occurrences != expected {
                return Err(failure(format!(
                    "{parts} parts, {occurrences} occurrences; expected 1 and {expected}"
                )));
            }
            Ok(format!(
                "{components} components in {} sub-assemblies, 1 shared part",
                GROUPS + PATTERNS
            ))
        },
    )
}

/// 10,000 pattern members written as a DRAW view: one BREP and a script.
pub(crate) fn draw_view_case(definition: &'static FamilyDefinition) -> Outcome {
    const MEMBERS: usize = 10_000;
    timed(
        format!("DRAW view of {MEMBERS} pattern members: BREP and script"),
        ms(2_000),
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
            let directory =
                std::env::temp_dir().join(format!("occb-bench-view-{}", std::process::id()));
            let script = graph.export_draw_view(
                &session,
                &generation,
                &directory,
                &OutputSet::AllWithOutput("body".into()),
            )?;
            let lines = std::fs::read_to_string(&script)
                .map_err(|error| failure(error.to_string()))?
                .lines()
                .filter(|line| line.starts_with("vdisplay"))
                .count();
            std::fs::remove_dir_all(&directory).map_err(|error| failure(error.to_string()))?;
            if lines != MEMBERS + 1 {
                return Err(failure(format!("{lines} displayed parts")));
            }
            drop(generation);
            if session
                .shape_count()
                .map_err(|error| failure(error.to_string()))?
                != 0
            {
                return Err(failure("view export retained handles".into()));
            }
            Ok(format!(
                "{} named parts in one BREP and script",
                MEMBERS + 1
            ))
        },
    )
}
