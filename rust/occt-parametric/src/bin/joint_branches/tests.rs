use super::*;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "occb-joint-branches-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn inputs(directory: &Directory) -> Vec<OsString> {
    let model = directory.0.join("model.json");
    let setup = directory.0.join("setup.json");
    fs::write(
        &model,
        include_str!("../../../../../tools/joint-branches/example.model.json"),
    )
    .unwrap();
    fs::write(
        &setup,
        include_str!("../../../../../tools/joint-branches/example.setup.json"),
    )
    .unwrap();
    vec![
        model.into_os_string(),
        setup.into_os_string(),
        directory.0.join("output").into_os_string(),
    ]
}
#[test]
fn command_exports_reloadable_alternative_models_and_preserves_source() {
    let directory = Directory::new();
    let args = inputs(&directory);
    let before = fs::read(&args[0]).unwrap();
    run(&args).unwrap();
    let output = Path::new(&args[2]);
    let report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(output.join("report.json")).unwrap()).unwrap();
    assert_eq!(report["search"]["status"], "seeds_exhausted");
    assert_eq!(report["models"].as_array().unwrap().len(), 2);
    let original = ModelDocument::from_json(&String::from_utf8(before.clone()).unwrap()).unwrap();
    for file in report["models"].as_array().unwrap() {
        let loaded = ModelDocument::from_json(
            &fs::read_to_string(output.join(file.as_str().unwrap())).unwrap(),
        )
        .unwrap();
        assert_eq!(loaded.family, original.family);
        assert_eq!(loaded.frames, original.frames);
        assert_eq!(loaded.instances, original.instances);
        assert!(
            loaded
                .instance_graph()
                .unwrap()
                .check_relationships()
                .unwrap()
                .iter()
                .all(|check| check.satisfied)
        );
        assert_eq!(
            loaded.assembly.joints["crank"],
            original.assembly.joints["crank"]
        );
    }
    assert_eq!(fs::read(&args[0]).unwrap(), before);
    assert!(run(&args).is_err());
    assert!(run(&[]).is_err());
}
#[test]
fn command_reports_partial_search_and_rejects_invalid_setup_without_outputs() {
    let directory = Directory::new();
    let args = inputs(&directory);
    let mut setup: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&args[1]).unwrap()).unwrap();
    setup["options"]["maximum_branches"] = json!(1);
    fs::write(&args[1], setup.to_string()).unwrap();
    run(&args).unwrap();
    let report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(Path::new(&args[2]).join("report.json")).unwrap())
            .unwrap();
    assert_eq!(report["search"]["status"], "branch_limit_reached");
    assert_eq!(report["models"].as_array().unwrap().len(), 1);
    let directory = Directory::new();
    let args = inputs(&directory);
    setup["schema"] = json!("future");
    fs::write(&args[1], setup.to_string()).unwrap();
    assert!(run(&args).is_err());
    assert!(!Path::new(&args[2]).exists());
    setup["schema"] = json!("occb-joint-branches-v1");
    setup["options"]["unknown_option"] = json!(true);
    fs::write(&args[1], setup.to_string()).unwrap();
    assert!(run(&args).is_err());
    assert!(!Path::new(&args[2]).exists());
    setup["options"]
        .as_object_mut()
        .unwrap()
        .remove("unknown_option");
    setup["axes"] = json!([]);
    fs::write(&args[1], setup.to_string()).unwrap();
    assert!(run(&args).is_err());
    assert!(!Path::new(&args[2]).exists());
}
