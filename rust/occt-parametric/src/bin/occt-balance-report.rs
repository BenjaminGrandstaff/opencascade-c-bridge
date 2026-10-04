//! Selected component and per-material mass, inertia, and geometric CG report.
#[path = "balance_report/config.rs"]
mod config;
#[path = "balance_report/report.rs"]
mod report;
use occt_bridge::Session;
use occt_parametric::ModelDocument;
use std::{
    error::Error,
    ffi::OsString,
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    process::ExitCode,
};

fn main() -> ExitCode {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("occt-balance-report: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[OsString]) -> Result<(), Box<dyn Error>> {
    let [model, setup_path, output] = args else {
        return Err("usage: occt-balance-report MODEL.json SETUP.json NEW_REPORT.json".into());
    };
    let output = Path::new(output);
    if output.try_exists()? {
        return Err("report already exists; choose a new output file".into());
    }
    let document = ModelDocument::from_json(&fs::read_to_string(model)?)?;
    let setup: config::Setup = serde_json::from_str(&fs::read_to_string(setup_path)?)?;
    let directory = Path::new(setup_path)
        .parent()
        .unwrap_or_else(|| Path::new("."));
    let (reference, mac) = config::reference(&setup, directory)?;
    let (graph, outputs) = config::prepare(&document, &setup)?;
    let session = Session::new()?;
    let mass = graph.mass_properties(&session, &outputs)?;
    let report = serde_json::to_string_pretty(&report::document(&mass, reference, mac)?)?;
    // Reserve the output exclusively after all validation and measurements.
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    file.write_all(report.as_bytes())?;
    file.write_all(b"\n")?;
    println!(
        "Wrote mass {:.6} kg and CG {:.3}% chord to {}",
        mass.total.mass_kg,
        mass.balance(reference)?.chord_fraction * 100.0,
        output.display()
    );
    Ok(())
}

#[cfg(test)]
#[path = "balance_report/tests.rs"]
mod tests;
