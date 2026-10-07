//! General drawing and planar cutting-template batch export.
use occt_bridge::Session;
use occt_parametric::{DrawingDefinition, DrawingRenderOptions, ModelDocument};
use serde::Deserialize;
use serde_json::json;
use std::{error::Error, ffi::OsString, fs, path::Path, process::ExitCode};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Setup {
    schema: String,
    #[serde(default)]
    drawings: Vec<DrawingDefinition>,
    #[serde(default)]
    options: DrawingRenderOptions,
}

fn main() -> ExitCode {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("occt-drawing-export: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[OsString]) -> Result<(), Box<dyn Error>> {
    let [model, setup, destination] = args else {
        return Err("usage: occt-drawing-export MODEL.json SETUP.json NEW_OUTPUT_DIRECTORY".into());
    };
    let destination = Path::new(destination);
    if destination.try_exists()? {
        return Err("output directory already exists; choose a new directory".into());
    }
    let mut document = ModelDocument::from_json(&fs::read_to_string(model)?)?;
    let setup: Setup = serde_json::from_str(&fs::read_to_string(setup)?)?;
    if setup.schema != "occb-drawing-export-v1" {
        return Err("expected occb-drawing-export-v1".into());
    }
    let definitions = if setup.drawings.is_empty() {
        document.drawings.clone()
    } else {
        setup.drawings
    };
    retain_definitions(&mut document, &definitions)?;
    let persisted = document.to_json_pretty()?;
    let graph = document.instance_graph()?;
    let session = Session::new()?;
    let generated =
        DrawingDefinition::generate_many(&definitions, &graph, &session, setup.options)?;
    let manifest: Vec<_> = generated
        .iter()
        .enumerate()
        .map(|(index, drawing)| {
            json!({
                "id":drawing.id,"title":drawing.title,
                "dxf":format!("{:04}.dxf",index+1),"svg":format!("{:04}.svg",index+1),
                "polylines":drawing.polylines.len(),"generated_variants":drawing.generated_variants,
                "curves":drawing.curves.len(),
                "empty":drawing.polylines.is_empty() && drawing.curves.is_empty(),
            })
        })
        .collect();
    let manifest = serde_json::to_string_pretty(
        &json!({"schema":"occb-drawing-manifest-v1","drawings":manifest}),
    )?;
    fs::create_dir(destination)?;
    for (index, drawing) in generated.iter().enumerate() {
        fs::write(
            destination.join(format!("{:04}.dxf", index + 1)),
            drawing.to_dxf(),
        )?;
        fs::write(
            destination.join(format!("{:04}.svg", index + 1)),
            drawing.to_svg(),
        )?;
    }
    fs::write(destination.join("drawings.model.json"), persisted)?;
    fs::write(destination.join("manifest.json"), manifest)?;
    println!(
        "Wrote {} drawings in SVG and millimeter DXF to {}",
        generated.len(),
        destination.display()
    );
    Ok(())
}

fn retain_definitions(
    document: &mut ModelDocument,
    definitions: &[DrawingDefinition],
) -> Result<(), Box<dyn Error>> {
    for definition in definitions {
        match document
            .drawings
            .iter()
            .find(|existing| existing.id == definition.id)
        {
            Some(existing) if existing == definition => {}
            Some(_) => {
                return Err(format!(
                    "drawing '{}' conflicts with the model definition",
                    definition.id
                )
                .into());
            }
            None => document.drawings.push(definition.clone()),
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "drawing_export/tests.rs"]
mod tests;
