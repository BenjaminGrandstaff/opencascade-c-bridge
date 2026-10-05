use super::*;
use occt_parametric::*;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let directory = std::env::temp_dir().join(format!(
            "occt-merge-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        Self(directory)
    }
    fn write(&self, name: &str, document: &ModelDocument) -> PathBuf {
        let path = self.0.join(name);
        fs::write(&path, document.to_json_pretty().unwrap()).unwrap();
        path
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn document() -> ModelDocument {
    let family = FamilyDefinition {
        references: Vec::new(),
        feature_colors: Default::default(),
        assumptions: Vec::new(),
        id: "part".into(),
        version: 1,
        parameters: ["enabled", "visible"]
            .into_iter()
            .map(|id| ParameterDefinition {
                id: id.into(),
                parameter_type: ParameterType::Boolean,
                default: ParameterValue::Boolean(false),
                minimum: None,
                maximum: None,
            })
            .collect(),
        derived_parameters: vec![],
        derived_vector_parameters: vec![],
        constraints: vec![],
        features: vec![],
        requirements: vec![],
        datums: vec![],
    };
    ModelDocument::from_graph(&InstanceGraph::new(&family))
}
fn arguments(paths: [&Path; 3]) -> Vec<OsString> {
    paths
        .into_iter()
        .map(|path| path.as_os_str().to_owned())
        .collect()
}

#[test]
fn independent_edits_replace_current_only_and_preserve_permissions() {
    let fixture = Fixture::new();
    let base = document();
    let mut left = base.clone();
    let mut right = base.clone();
    left.family.parameters[0].default = ParameterValue::Boolean(true);
    right.family.parameters[1].default = ParameterValue::Boolean(true);
    let base_path = fixture.write("base", &base);
    let current = fixture.write("current with spaces", &left);
    let other = fixture.write("other", &right);
    let before_base = fs::read(&base_path).unwrap();
    let before_other = fs::read(&other).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&current, fs::Permissions::from_mode(0o640)).unwrap();
    }
    run(&arguments([&base_path, &current, &other])).unwrap();
    let merged = ModelDocument::from_json(&fs::read_to_string(&current).unwrap()).unwrap();
    assert!(
        merged
            .family
            .parameters
            .iter()
            .all(|parameter| parameter.default == ParameterValue::Boolean(true))
    );
    assert_eq!(fs::read(&base_path).unwrap(), before_base);
    assert_eq!(fs::read(&other).unwrap(), before_other);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&current).unwrap().permissions().mode() & 0o777,
            0o640
        );
    }
    assert_eq!(fs::read_dir(&fixture.0).unwrap().count(), 3);
}

#[test]
fn conflicts_and_invalid_inputs_leave_current_bytes_unchanged() {
    let fixture = Fixture::new();
    let base = document();
    let mut left = base.clone();
    let mut right = base.clone();
    left.family.version = 2;
    right.family.version = 3;
    let base_path = fixture.write("base", &base);
    let current = fixture.write("current", &left);
    let other = fixture.write("other", &right);
    let before = fs::read(&current).unwrap();
    let args = arguments([&base_path, &current, &other]);
    let error = run(&args).unwrap_err();
    assert!(error.contains("semantic conflicts") && error.contains("version"));
    assert_eq!(fs::read(&current).unwrap(), before);
    fs::write(&other, "{invalid}").unwrap();
    assert!(run(&args).unwrap_err().contains("parse model document"));
    assert_eq!(fs::read(&current).unwrap(), before);
    assert!(run(&[]).unwrap_err().contains("usage"));
    assert!(
        run(&arguments([&base_path, &base_path, &other]))
            .unwrap_err()
            .contains("separate")
    );
    assert!(
        run(&arguments([&base_path, &current, &current]))
            .unwrap_err()
            .contains("separate")
    );
    assert!(
        run(&arguments([
            &base_path,
            &current,
            &fixture.0.join("absent")
        ]))
        .is_err()
    );
    assert_eq!(fs::read_dir(&fixture.0).unwrap().count(), 3);
}

#[test]
fn temporary_collisions_and_failed_rename_do_not_leak_files() {
    let fixture = Fixture::new();
    let collision = fixture
        .0
        .join(format!(".occt-merge-{}-0.tmp", std::process::id()));
    fs::write(&collision, "preserve").unwrap();
    let destination = fixture.0.join("destination");
    fs::create_dir(&destination).unwrap();
    assert!(replace(&destination, b"cannot replace directory").is_err());
    assert_eq!(fs::read(&collision).unwrap(), b"preserve");
    assert_eq!(fs::read_dir(&fixture.0).unwrap().count(), 2);
    for attempt in 1..100 {
        fs::write(
            fixture
                .0
                .join(format!(".occt-merge-{}-{attempt}.tmp", std::process::id())),
            "preserve",
        )
        .unwrap();
    }
    assert_eq!(
        create_temporary(&fixture.0).unwrap_err().kind(),
        io::ErrorKind::AlreadyExists
    );
    assert!(create_temporary(&fixture.0.join("missing")).is_err());
}
