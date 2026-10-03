//! Git merge driver: BASE CURRENT OTHER. CURRENT changes only on success.
use occt_parametric::{DocumentMerge, ModelDocument};
use std::{
    ffi::OsString,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    process::ExitCode,
};

fn main() -> ExitCode {
    let arguments: Vec<OsString> = std::env::args_os().skip(1).collect();
    match run(&arguments) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("occt-document-merge: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(arguments: &[OsString]) -> Result<(), String> {
    let [base, current, other] = arguments else {
        return Err("usage: occt-document-merge BASE CURRENT OTHER".into());
    };
    let [base_path, current_path, other_path] = [base, current, other].map(Path::new);
    let paths = [base_path, current_path, other_path]
        .map(|path| fs::canonicalize(path).map_err(|error| format!("{}: {error}", path.display())));
    let [base_identity, current_identity, other_identity] = paths;
    let (base_identity, current_identity, other_identity) =
        (base_identity?, current_identity?, other_identity?);
    if current_identity == base_identity || current_identity == other_identity {
        return Err("CURRENT must be a separate file from BASE and OTHER".into());
    }
    let load = |path: &Path| -> Result<ModelDocument, String> {
        let contents = fs::read_to_string(path)
            .map_err(|error| format!("read {}: {error}", path.display()))?;
        ModelDocument::from_json(&contents)
            .map_err(|error| format!("decode {}: {error}", path.display()))
    };
    let (base, current, other) = (load(base_path)?, load(current_path)?, load(other_path)?);
    match base
        .three_way_merge(&current, &other)
        .map_err(|error| error.to_string())?
    {
        DocumentMerge::Merged(merged) => {
            let mut contents = merged.to_json_pretty().map_err(|error| error.to_string())?;
            contents.push('\n');
            replace(current_path, contents.as_bytes()).map_err(|error| {
                format!("write merged document {}: {error}", current_path.display())
            })
        }
        DocumentMerge::Conflicts(conflicts) => {
            let details =
                serde_json::to_string_pretty(&conflicts).map_err(|error| error.to_string())?;
            Err(format!(
                "semantic conflicts; CURRENT is unchanged:\n{details}"
            ))
        }
    }
}

/// Write beside the destination so the final rename is atomic. Exclusive
/// creation avoids clobbering existing files and following temporary symlinks.
fn replace(destination: &Path, contents: &[u8]) -> io::Result<()> {
    let directory = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let permissions = fs::metadata(destination)?.permissions();
    let (temporary, mut file) = create_temporary(directory)?;
    let outcome = (|| {
        file.set_permissions(permissions)?;
        file.write_all(contents)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, destination)
    })();
    if outcome.is_err() {
        let _ = fs::remove_file(temporary);
    }
    outcome
}

fn create_temporary(directory: &Path) -> io::Result<(PathBuf, fs::File)> {
    for attempt in 0..100 {
        let path = directory.join(format!(".occt-merge-{}-{attempt}.tmp", std::process::id()));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "no unused temporary filename",
    ))
}

#[cfg(test)]
#[path = "occt-document-merge/tests.rs"]
mod tests;
