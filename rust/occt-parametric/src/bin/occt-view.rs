//! Regenerates a saved model and opens its parts in OCCT's DRAW viewer.
use occt_bridge::Session;
use occt_parametric::{ModelDocument, OutputSet};
use std::{
    error::Error,
    ffi::{OsStr, OsString},
    fs,
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
    process::{Child, Command, ExitCode, Stdio},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const USAGE: &str = "usage: occt-view MODEL.json [--output NAME] [--dir NEW_DIRECTORY] [--no-open] [--watch]\n       occt-view MODEL.json --serve [--port PORT] [--output NAME] [--no-open]";
/// Default `--serve` port, next to the wing layout workshop's 8790.
const DEFAULT_PORT: u16 = 8791;

#[path = "view/serve.rs"]
mod serve;
/// How often `--watch` checks the model file and the viewer.
const POLL: Duration = Duration::from_millis(250);

struct Options {
    model: PathBuf,
    output: Option<String>,
    directory: Option<PathBuf>,
    open: bool,
    watch: bool,
    serve: bool,
    port: u16,
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
    let mut watch = false;
    let mut serve = false;
    let mut port = DEFAULT_PORT;
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
            Some("--watch") => watch = true,
            Some("--serve") => serve = true,
            Some("--port") => {
                port = value()?
                    .to_str()
                    .and_then(|text| text.parse().ok())
                    .ok_or("port must be a number from 0 to 65535")?
            }
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
        watch,
        serve,
        port,
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

/// Regenerates the model and writes its view into `directory`.
fn export(options: &Options, directory: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let document = ModelDocument::from_json(&fs::read_to_string(&options.model)?)?;
    // By default show each instance's final feature of the primary family.
    let output = match &options.output {
        Some(output) => output.clone(),
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
    Ok(graph.export_draw_view(
        &session,
        &generation,
        directory,
        &OutputSet::AllWithOutput(output),
    )?)
}

/// Writes the view, then starts `viewer` on it without waiting for it to
/// close; with `--watch`, keeps reloading it as the model file changes.
fn run(args: &[OsString], viewer: &OsStr) -> Result<(), Box<dyn Error>> {
    let options = parse(args)?;
    if options.serve {
        return serve_model(&options);
    }
    let directory = match &options.directory {
        Some(directory) => {
            if directory.try_exists()? {
                return Err("view directory already exists; choose a new directory".into());
            }
            directory.clone()
        }
        None => scratch_directory(&options.model),
    };
    let script = export(&options, &directory)?;
    println!("{}", script.display());
    let child = if options.open {
        Some(
            Command::new(viewer)
                .args(["-i", "-f", "view.tcl"])
                .current_dir(&directory)
                // A watched viewer takes reload commands on its standard input.
                .stdin(if options.watch {
                    Stdio::piped()
                } else {
                    Stdio::null()
                })
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|error| {
                    format!(
                        "view written, but '{}' could not start ({error}); open it with: cd {} && DRAWEXE -i -f view.tcl",
                        viewer.to_string_lossy(),
                        directory.display()
                    )
                })?,
        )
    } else {
        None
    };
    if options.watch {
        watch(&options, &directory, child)?;
    }
    Ok(())
}

/// Serves the browser viewer on 127.0.0.1 until the process is stopped.
fn serve_model(options: &Options) -> Result<(), Box<dyn Error>> {
    if options.watch || options.directory.is_some() {
        return Err("--serve follows file changes itself and writes no view directory".into());
    }
    let mut studio = serve::Studio::load(&options.model, options.output.clone())?;
    let listener = std::net::TcpListener::bind(("127.0.0.1", options.port))?;
    let url = format!("http://127.0.0.1:{}/", listener.local_addr()?.port());
    println!("{url}");
    eprintln!(
        "occt-view: serving {}; press Ctrl-C to stop",
        options.model.display()
    );
    if options.open {
        // Best effort: the URL is printed either way.
        let _ = Command::new("xdg-open")
            .arg(&url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
    }
    serve::serve(&mut studio, &listener)
}

/// Modification time and length, which change when an editor saves.
fn stamp(path: &Path) -> Option<(SystemTime, u64)> {
    let metadata = fs::metadata(path).ok()?;
    Some((metadata.modified().ok()?, metadata.len()))
}

/// Re-exports after each settled change of the model file and tells the
/// viewer to reload, until the viewer exits (or forever without a viewer).
/// A change is used once two polls see the same stamp, so a save in
/// progress is not read; an invalid model keeps the previous view. Each
/// poll is O(1); each reload costs one full regeneration and export.
fn watch(
    options: &Options,
    directory: &Path,
    mut viewer: Option<Child>,
) -> Result<(), Box<dyn Error>> {
    eprintln!(
        "occt-view: watching {}; close the viewer to stop",
        options.model.display()
    );
    let mut seen = stamp(&options.model);
    let mut pending = None;
    loop {
        thread::sleep(POLL);
        if let Some(child) = viewer.as_mut()
            && child.try_wait()?.is_some()
        {
            eprintln!("occt-view: viewer closed");
            return Ok(());
        }
        let now = stamp(&options.model);
        if now.is_none() || now == seen {
            pending = None;
            continue;
        }
        if pending != now {
            pending = now;
            continue;
        }
        seen = now;
        pending = None;
        if let Err(error) = export(options, directory) {
            eprintln!("occt-view: {error}; keeping the previous view");
            continue;
        }
        eprintln!("occt-view: reloaded");
        let Some(stdin) = viewer.as_mut().and_then(|child| child.stdin.as_mut()) else {
            continue;
        };
        match stdin
            .write_all(b"source reload.tcl\n")
            .and_then(|()| stdin.flush())
        {
            Ok(()) => {}
            Err(error) if error.kind() == ErrorKind::BrokenPipe => {
                eprintln!("occt-view: viewer closed");
                return Ok(());
            }
            Err(error) => return Err(error.into()),
        }
    }
}

#[cfg(test)]
#[path = "view/tests.rs"]
mod tests;
