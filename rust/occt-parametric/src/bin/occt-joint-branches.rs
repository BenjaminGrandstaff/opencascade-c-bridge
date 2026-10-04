//! Discover alternative linkage poses and export reloadable branch models.
use occt_parametric::{JointBranchSearchOptions, JointSeedAxis, ModelDocument};
use serde::Deserialize;
use serde_json::json;
use std::{error::Error, ffi::OsString, fs, path::Path, process::ExitCode};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Setup {
    schema: String,
    axes: Vec<JointSeedAxis>,
    #[serde(default)]
    options: JointBranchSearchOptions,
}
fn main() -> ExitCode {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("occt-joint-branches: {error}");
            ExitCode::FAILURE
        }
    }
}
fn run(args: &[OsString]) -> Result<(), Box<dyn Error>> {
    let [model, setup, output] = args else {
        return Err("usage: occt-joint-branches MODEL.json SETUP.json NEW_OUTPUT_DIRECTORY".into());
    };
    let destination = Path::new(output);
    if destination.try_exists()? {
        return Err("output directory already exists; choose a new directory".into());
    }
    let document = ModelDocument::from_json(&fs::read_to_string(model)?)?;
    let setup: Setup = serde_json::from_str(&fs::read_to_string(setup)?)?;
    if setup.schema != "occb-joint-branches-v1" {
        return Err("expected occb-joint-branches-v1".into());
    }
    let graph = document.instance_graph()?;
    let result = graph.search_joint_branches(&setup.axes, setup.options)?;
    let mut models = Vec::new();
    for branch in &result.branches {
        let mut candidate = graph.clone();
        for position in &branch.positions {
            candidate.set_joint_coordinate(&position.frame, position.coordinate, position.value)?;
        }
        let mut model = document.clone();
        model.assembly.joints = candidate.assembly().joints.clone();
        models.push(model.to_json_pretty()?);
    }
    let files: Vec<_> = (1..=models.len())
        .map(|index| format!("{index:04}.model.json"))
        .collect();
    let report = serde_json::to_string_pretty(
        &json!({"schema":"occb-joint-branch-report-v1","search":result,"models":files}),
    )?;
    fs::create_dir(destination)?;
    for (file, model) in files.iter().zip(models) {
        fs::write(destination.join(file), model)?;
    }
    fs::write(destination.join("report.json"), report)?;
    println!(
        "Wrote {} discovered branches; status {:?}; {} of {} starts attempted",
        result.branches.len(),
        result.status,
        result.attempted_starts,
        result.planned_starts
    );
    Ok(())
}
#[cfg(test)]
#[path = "joint_branches/tests.rs"]
mod tests;
