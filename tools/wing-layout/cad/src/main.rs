use occt_bridge::{Session, Vec3};
use serde::Deserialize;
use std::{error::Error, path::PathBuf};

#[derive(Deserialize)]
struct Sections {
    schema: String,
    units: String,
    halves: Vec<Vec<Vec<[f64; 3]>>>,
}

fn run() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 2 {
        return Err(
            "Usage: occb-wing-cad wing-sections.json output.step (also writes output.brep)".into(),
        );
    }
    let input: Sections = serde_json::from_slice(&std::fs::read(&args[0])?)?;
    if input.schema != "occb-wing-sections-v1" || input.units != "mm" || input.halves.len() != 2 {
        return Err(
            "Expected occb-wing-sections-v1, millimeters and exactly two wing halves.".into(),
        );
    }
    let output = PathBuf::from(&args[1]);
    if !output
        .extension()
        .is_some_and(|x| x.eq_ignore_ascii_case("step") || x.eq_ignore_ascii_case("stp"))
    {
        return Err("Output filename must end in .step or .stp.".into());
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
    // Keep the two halves as separate touching solids for downstream CAD work.
    let compound = session.create_compound(&shapes.iter().collect::<Vec<_>>())?;
    session.save_step(&compound, &output)?;
    session.save_brep(&compound, output.with_extension("brep"))?;
    let expected_volume = session.volume(&compound)?;
    for (kind, path) in [
        ("STEP", output.clone()),
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
