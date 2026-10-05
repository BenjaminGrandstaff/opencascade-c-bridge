//! Supplied dimensional and fixed position measurements against saved drawings.
use occt_parametric::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    error::Error,
    ffi::OsString,
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    process::ExitCode,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Setup {
    schema: String,
    drawings: Vec<Request>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    drawing: String,
    #[serde(default)]
    dimensions: Vec<DrawingDimensionMeasurement>,
    #[serde(default)]
    positions: Vec<DrawingPositionMeasurement>,
}
#[derive(Default, Serialize)]
struct Summary {
    dimensions_within_limits: usize,
    dimensions_outside_limits: usize,
    dimensions_without_limits: usize,
    positions_within_zone: usize,
    positions_outside_zone: usize,
}
impl Summary {
    fn failed(&self) -> bool {
        self.dimensions_outside_limits != 0 || self.positions_outside_zone != 0
    }
    fn add(&mut self, report: &DrawingReport) {
        for result in &report.dimensions {
            match result.evaluation.disposition {
                DimensionMeasurementDisposition::WithinLimits => self.dimensions_within_limits += 1,
                DimensionMeasurementDisposition::BelowLowerLimit
                | DimensionMeasurementDisposition::AboveUpperLimit => {
                    self.dimensions_outside_limits += 1
                }
                _ => self.dimensions_without_limits += 1,
            }
        }
        for result in &report.positions {
            if result.evaluation.samples_within_zone {
                self.positions_within_zone += 1;
            } else {
                self.positions_outside_zone += 1;
            }
        }
    }
}
#[derive(Serialize)]
struct DrawingReport {
    drawing: String,
    dimensions: Vec<DrawingDimensionMeasurementResult>,
    positions: Vec<DrawingPositionMeasurementResult>,
}
#[derive(Serialize)]
struct Report {
    schema: &'static str,
    model_schema_version: u32,
    summary: Summary,
    drawings: Vec<DrawingReport>,
}

fn main() -> ExitCode {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    match run(&args) {
        Ok(false) => ExitCode::SUCCESS,
        Ok(true) => ExitCode::from(2),
        Err(error) => {
            eprintln!("occt-inspection-report: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[OsString]) -> Result<bool, Box<dyn Error>> {
    let [model, setup_path, output] = args else {
        return Err(
            "usage: occt-inspection-report MODEL.json MEASUREMENTS.json NEW_REPORT.json".into(),
        );
    };
    let output = Path::new(output);
    if output.try_exists()? {
        return Err("report already exists; choose a new output file".into());
    }
    let document = ModelDocument::from_json(&fs::read_to_string(model)?)?;
    let setup: Setup = serde_json::from_str(&fs::read_to_string(setup_path)?)?;
    let report = evaluate(&document, &setup)?;
    let failed = report.summary.failed();
    let data = serde_json::to_string_pretty(&report)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    file.write_all(data.as_bytes())?;
    file.write_all(b"\n")?;
    println!(
        "Wrote {} drawing measurement groups to {} ({} dimensional violations, {} position violations, {} dimensions without limits)",
        report.drawings.len(),
        output.display(),
        report.summary.dimensions_outside_limits,
        report.summary.positions_outside_zone,
        report.summary.dimensions_without_limits
    );
    Ok(failed)
}

fn evaluate(document: &ModelDocument, setup: &Setup) -> Result<Report, Box<dyn Error>> {
    if setup.schema != "occb-inspection-setup-v1" || setup.drawings.is_empty() {
        return Err("expected occb-inspection-setup-v1 with at least one drawing group".into());
    }
    let graph = document.instance_graph()?;
    let index: HashMap<_, _> = document
        .drawings
        .iter()
        .map(|d| (d.id.as_str(), d))
        .collect();
    let mut seen = HashSet::new();
    let mut summary = Summary::default();
    let mut drawings = Vec::with_capacity(setup.drawings.len());
    for request in &setup.drawings {
        if !seen.insert(&request.drawing)
            || request.dimensions.is_empty() && request.positions.is_empty()
        {
            return Err("drawing groups must be unique and contain measurements".into());
        }
        let drawing = index
            .get(request.drawing.as_str())
            .ok_or_else(|| format!("unknown measurement drawing '{}'", request.drawing))?;
        let report = DrawingReport {
            drawing: request.drawing.clone(),
            dimensions: drawing.evaluate_dimension_measurements(&graph, &request.dimensions)?,
            positions: drawing.evaluate_position_measurements(&graph, &request.positions)?,
        };
        summary.add(&report);
        drawings.push(report);
    }
    Ok(Report {
        schema: "occb-inspection-report-v1",
        model_schema_version: document.schema_version,
        summary,
        drawings,
    })
}

#[cfg(test)]
#[path = "inspection_report/tests.rs"]
mod tests;
