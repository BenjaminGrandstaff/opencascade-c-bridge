use occt_bridge::{Session, Shape, Vec3};
use occt_parametric::{InstanceGraph, ModelDocument, PartInstance, VerificationStatus};
use serde::Deserialize;
use std::{collections::HashMap, error::Error, path::Path, path::PathBuf};

mod project;
mod structure;

#[derive(Deserialize)]
struct Sections {
    schema: String,
    units: String,
    halves: Vec<Vec<Vec<[f64; 3]>>>,
}

fn run() -> Result<(), Box<dyn Error>> {
    let mut args: Vec<_> = std::env::args_os().skip(1).collect();
    let build = match args.iter().position(|arg| arg == "--build") {
        Some(index) if index + 1 < args.len() => {
            let path = args.remove(index + 1);
            args.remove(index);
            Some(serde_json::from_slice::<structure::Build>(&std::fs::read(
                path,
            )?)?)
        }
        Some(_) => return Err("--build needs a build file.".into()),
        None => None,
    };
    if args.len() != 2 {
        return Err(
            "Usage: occb-wing-cad (wing-project.json [--build build.json] | wing-sections.json) \
             output.step (also writes output.brep; a project also writes output.model.json, and \
             a build output.parts/*.stl)"
                .into(),
        );
    }
    let bytes = std::fs::read(&args[0])?;
    let output = PathBuf::from(&args[1]);
    if !output
        .extension()
        .is_some_and(|x| x.eq_ignore_ascii_case("step") || x.eq_ignore_ascii_case("stp"))
    {
        return Err("Output filename must end in .step or .stp.".into());
    }
    let schema: serde_json::Value = serde_json::from_slice(&bytes)?;
    if schema["schema"] == "occb-wing-layout-v1" {
        return parametric(serde_json::from_slice(&bytes)?, build.as_ref(), &output);
    }
    if build.is_some() {
        return Err("--build applies to project files, not sections.".into());
    }
    let input: Sections = serde_json::from_slice(&bytes)?;
    if input.schema != "occb-wing-sections-v1" || input.units != "mm" || input.halves.len() != 2 {
        return Err(
            "Expected an occb-wing-layout-v1 project, or occb-wing-sections-v1 \
             in millimeters with exactly two wing halves."
                .into(),
        );
    }
    let session = Session::new()?;
    let mut shapes = Vec::new();
    for (side, half) in input.halves.iter().enumerate() {
        if half.len() < 2 || half.len() > 200 {
            return Err("Each half needs 2–200 stations.".into());
        }
        let count = half[0].len();
        if !(6..=1000).contains(&count) {
            return Err("Each station needs 6–1000 polygon points.".into());
        }
        let mut previous_y = None;
        let mut sections = Vec::new();
        for station in half {
            if station.len() != count || station.iter().flatten().any(|x| !x.is_finite()) {
                return Err(
                    "Section point counts must agree and coordinates must be finite.".into(),
                );
            }
            let y = station[0][1];
            if station.iter().any(|p| (p[1] - y).abs() > 1e-7) {
                return Err("Each station must lie in a constant-Y plane.".into());
            }
            if previous_y.is_none() && y.abs() > 1e-7 {
                return Err("Each half must start at the centerline.".into());
            }
            if previous_y.is_some_and(|prev: f64| if side == 0 { y <= prev } else { y >= prev }) {
                return Err(
                    "Right stations must increase Y; left stations must decrease Y.".into(),
                );
            }
            previous_y = Some(y);
            sections.push(
                station
                    .iter()
                    .map(|p| Vec3::new(p[0], p[1], p[2]))
                    .collect::<Vec<_>>(),
            );
        }
        // A ruled loft preserves linear spanwise panels and prevents smooth interpolation overshoot.
        let slices: Vec<_> = sections.iter().map(Vec::as_slice).collect();
        let shape = session.create_loft(&slices, true, true)?;
        let volume = session.volume(&shape)?;
        if !session.is_valid(&shape)? || !volume.is_finite() || volume <= 0.0 {
            return Err(format!(
                "Wing half {} is not a valid positive-volume solid.",
                side + 1
            )
            .into());
        }
        println!("Half {}: valid solid, volume {:.3} mm³", side + 1, volume);
        shapes.push(shape);
    }
    export(&session, &shapes.iter().collect::<Vec<_>>(), &output)
}

/// A project becomes a parametric family: smooth airfoil lofts whose station
/// values are parameters, checked by stored requirements on every regeneration.
fn parametric(
    project: project::Project,
    build: Option<&structure::Build>,
    output: &Path,
) -> Result<(), Box<dyn Error>> {
    if project.schema != "occb-wing-layout-v1" {
        return Err("Unsupported project schema.".into());
    }
    let mut family = project::family(&project)?;
    let parts = match build {
        Some(build) => structure::add_structure(&mut family, &project, build)?,
        None => Vec::new(),
    };
    let session = Session::new()?;
    let part = PartInstance {
        id: "wing".into(),
        definition: &family,
        overrides: HashMap::new(),
        provenance: format!("occb-wing-cad: {}", project.name),
    };
    let generated = part.regenerate(&session)?;
    for result in &generated.verification {
        let status = match result.status {
            VerificationStatus::Passed => "passed",
            VerificationStatus::Failed => "FAILED",
        };
        println!(
            "Requirement {} {status}: {}",
            result.requirement_id, result.message
        );
        if let Some(witness) = result
            .witness
            .as_ref()
            .filter(|_| result.status == VerificationStatus::Failed)
        {
            let points = witness
                .points_mm
                .iter()
                .map(|p| format!("({:.1}, {:.1}, {:.1})", p.x, p.y, p.z))
                .collect::<Vec<_>>()
                .join(" to ");
            println!("    at {} {points} mm", witness.subjects.join(", "));
        }
    }
    let mut halves = Vec::new();
    for (index, id) in ["right", "left"].into_iter().enumerate() {
        let shape = generated
            .shape(id)
            .ok_or_else(|| format!("wing half '{id}' was not generated"))?;
        println!(
            "Half {}: valid solid, volume {:.3} mm³ (smooth sections)",
            index + 1,
            session.volume(shape)?
        );
        halves.push(shape);
    }
    if parts.is_empty() {
        export(&session, &halves, output)?;
    } else {
        let directory = output.with_extension("parts");
        std::fs::create_dir_all(&directory)?;
        let mut shapes = Vec::new();
        for part in &parts {
            let shape = generated
                .shape(part)
                .ok_or_else(|| format!("part '{part}' was not generated"))?;
            session.save_stl(
                shape,
                directory.join(format!("{part}.stl")),
                Default::default(),
            )?;
            println!("Part {part}: volume {:.3} mm³", session.volume(shape)?);
            shapes.push(shape);
        }
        export(&session, &shapes, output)?;
        println!(
            "Wrote {} printable parts to {}",
            parts.len(),
            directory.display()
        );
    }
    let mut graph = InstanceGraph::new(&family);
    graph.add_base("wing", HashMap::new(), "occb-wing-cad")?;
    let document = output.with_extension("model.json");
    std::fs::write(
        &document,
        ModelDocument::from_graph(&graph).to_json_pretty()?,
    )?;
    println!("Wrote parametric model {}", document.display());
    Ok(())
}

/// Writes STEP and BREP and verifies both round trips by validity and volume.
fn export(session: &Session, shapes: &[&Shape<'_>], output: &Path) -> Result<(), Box<dyn Error>> {
    // Keep the two halves as separate touching solids for downstream CAD work.
    let compound = session.create_compound(shapes)?;
    session.save_step(&compound, output)?;
    session.save_brep(&compound, output.with_extension("brep"))?;
    let expected_volume = session.volume(&compound)?;
    for (kind, path) in [
        ("STEP", output.to_path_buf()),
        ("BREP", output.with_extension("brep")),
    ] {
        let restored = if kind == "STEP" {
            session.load_step(&path)?
        } else {
            session.load_brep(&path)?
        };
        let restored_volume = session.volume(&restored)?;
        if !session.is_valid(&restored)?
            || (restored_volume - expected_volume).abs() > expected_volume * 1e-6
        {
            return Err(format!("{kind} roundtrip failed validity or volume verification.").into());
        }
    }
    println!("STEP and BREP roundtrips verified.");
    println!(
        "Wrote {} and {}",
        output.display(),
        output.with_extension("brep").display()
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
