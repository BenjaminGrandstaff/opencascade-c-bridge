//! Regenerates a saved model and opens its parts in OCCT's DRAW viewer.
use occt_bridge::Session;
use occt_parametric::{ModelDocument, OutputSet};
use std::{
    error::Error,
    ffi::{OsStr, OsString},
    fs,
    path::{Path, PathBuf},
    process::{Command, ExitCode, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

const USAGE: &str = "usage: occt-view MODEL.json [--output NAME] [--dir NEW_DIRECTORY] [--no-open]";

struct Options {
    model: PathBuf,
    output: Option<String>,
    directory: Option<PathBuf>,
    open: bool,
}

fn main() -> ExitCode {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let viewer = std::env::var_os("OCCT_VIEW_DRAWEXE").unwrap_or_else(|| "DRAWEXE".into());
    match run(&args, &viewer) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("occt-view: {error}");
            ExitCode::FAILURE
        }
    }
}

fn parse(args: &[OsString]) -> Result<Options, Box<dyn Error>> {
    let mut model = None;
    let mut output = None;
    let mut directory = None;
    let mut open = true;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let mut value = || args.next().ok_or(USAGE);
        match arg.to_str() {
            Some("--output") => {
                output = Some(
                    value()?
                        .to_str()
                        .ok_or("output name must be UTF-8")?
                        .to_owned(),
                )
            }
            Some("--dir") => directory = Some(PathBuf::from(value()?)),
            Some("--no-open") => open = false,
            Some(flag) if flag.starts_with("--") => return Err(USAGE.into()),
            _ if model.is_none() => model = Some(PathBuf::from(arg)),
            _ => return Err(USAGE.into()),
        }
    }
    Ok(Options {
        model: model.ok_or(USAGE)?,
        output,
        directory,
        open,
    })
}

/// A fresh directory under the system temporary directory.
fn scratch_directory(model: &Path) -> PathBuf {
    let stem = model
        .file_stem()
        .and_then(OsStr::to_str)
        .unwrap_or("model")
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    std::env::temp_dir().join(format!("occt-view-{stem}-{}-{nanos}", std::process::id()))
}

/// Writes the view, then starts `viewer` on it without waiting for it to close.
fn run(args: &[OsString], viewer: &OsStr) -> Result<(), Box<dyn Error>> {
    let options = parse(args)?;
    let directory = match options.directory {
        Some(directory) => {
            if directory.try_exists()? {
                return Err("view directory already exists; choose a new directory".into());
            }
            directory
        }
        None => scratch_directory(&options.model),
    };
    let document = ModelDocument::from_json(&fs::read_to_string(&options.model)?)?;
    // By default show each instance's final feature of the primary family.
    let output = match options.output {
        Some(output) => output,
        None => document
            .family
            .features
            .last()
            .map(|feature| feature.id.clone())
            .ok_or("model family has no features; pass --output")?,
    };
    let mut graph = document.instance_graph()?;
    let session = Session::new()?;
    let generation = graph.regenerate_all(&session)?;
    let script = graph.export_draw_view(
        &session,
        &generation,
        &directory,
        &OutputSet::AllWithOutput(output),
    )?;
    drop(generation);
    println!("{}", script.display());
    if options.open {
        Command::new(viewer)
            .args(["-i", "-f", "view.tcl"])
            .current_dir(&directory)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| {
                format!(
                    "view written, but '{}' could not start ({error}); open it with: cd {} && DRAWEXE -i -f view.tcl",
                    viewer.to_string_lossy(),
                    directory.display()
                )
            })?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "view/tests.rs"]
mod tests;
