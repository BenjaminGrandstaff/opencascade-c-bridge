use super::*;
use std::{fs, path::PathBuf};

mod construction;
mod draft;
mod exchange;
mod history;
mod inspection;
mod open_profile;
mod operations;
mod results;
mod session;
mod variable_fillet;

fn unit_box(session: &Session, x: f64) -> Shape<'_> {
    session
        .create_box(Vec3::new(x, 0.0, 0.0), Vec3::new(1.0, 1.0, 1.0))
        .unwrap()
}

fn assert_wrong_session(error: BridgeError) {
    assert_eq!(error.status, 1);
    assert_eq!(error.category, "invalid argument");
    assert_eq!(error.message, "shape belongs to a different session");
}

fn fixture(name: &str) -> String {
    format!("{}/../../tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))
}
