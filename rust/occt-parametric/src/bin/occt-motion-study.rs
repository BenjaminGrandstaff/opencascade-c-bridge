//! Reusable hinge assembly and motion-report command, including wing elevons.
#[path = "motion_study/config.rs"]
mod config;
#[path = "motion_study/report.rs"]
mod report;

use occt_bridge::Session;
use occt_parametric::ModelDocument;
use std::{error::Error, ffi::OsString, fs, path::Path, process::ExitCode};

fn main() -> ExitCode {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    match run(&args) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(2),
        Err(error) => {
            eprintln!("occt-motion-study: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[OsString]) -> Result<bool, Box<dyn Error>> {
    let [model, setup, destination] = args else {
        return Err("usage: occt-motion-study MODEL.json SETUP.json NEW_OUTPUT_DIRECTORY".into());
    };
    let destination = Path::new(destination);
    if destination.try_exists()? {
        return Err("output directory already exists; choose a new directory".into());
    }
    let document = ModelDocument::from_json(&fs::read_to_string(model)?)?;
    let setup: config::Setup = serde_json::from_str(&fs::read_to_string(setup)?)?;
    let (graph, study) = config::prepare(&document, &setup)?;
    let assembly = config::assembly_document(&document, &graph).to_json_pretty()?;
    let session = Session::new()?;
    let sampled = graph.run_motion_study(&session, &study)?;
    let continuous = graph.check_continuous_motion(&session, &study, setup.continuous_options)?;
    let clear = report::clear(&sampled, &continuous);
    let report = serde_json::to_string_pretty(&report::document(&study, &sampled, &continuous))?;
    let study_exclusions = study.excluded_pairs.len();
    let study = serde_json::to_string_pretty(&study)?;
    // Exclusive directory creation protects inputs and existing output artifacts.
    fs::create_dir(destination)?;
    fs::write(destination.join("assembly.model.json"), assembly)?;
    fs::write(destination.join("study.json"), study)?;
    fs::write(destination.join("motion.report.json"), report)?;
    println!(
        "Wrote {} samples and continuous result {:?} ({} excluded pairs, {} unresolved pairs) to {}",
        sampled.samples.len(),
        continuous.status,
        study_exclusions,
        continuous.unresolved_pairs,
        destination.display()
    );
    Ok(clear)
}

#[cfg(test)]
#[path = "motion_study/tests.rs"]
mod tests;
