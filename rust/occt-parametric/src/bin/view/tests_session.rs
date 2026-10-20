//! Shared editing sessions: guarded applies, undo, notices and limits.
use super::*;

fn post(
    studio: &mut serve::Studio,
    path: &str,
    body: &serde_json::Value,
) -> (u16, serde_json::Value) {
    let response = studio.handle("POST", path, body.to_string().as_bytes());
    (
        response.status,
        serde_json::from_slice(&response.body).unwrap_or(serde_json::Value::Null),
    )
}

/// `model` with the thickness default set to `mm`.
fn with_thickness(model: &serde_json::Value, mm: f64) -> serde_json::Value {
    let mut model = model.clone();
    model["family"]["parameters"][0]["default"]["scalar"]["value"] = json!(mm);
    model
}

#[test]
fn session_changes_apply_only_against_the_model_they_read() {
    let directory = Directory::new();
    let model = PathBuf::from(directory.model(&document()));
    let mut studio = serve::Studio::load(&model, None).unwrap();
    let (status, session) = get(&mut studio, "/api/session");
    assert_eq!(status, 200);
    assert_eq!(session["schema"], "occb-studio-session-v1");
    assert_eq!(session["family"], "bracket");
    assert_eq!(session["dirty"], false);
    assert_eq!(session["last_change"], serde_json::Value::Null);
    let sha = session["model_sha256"].as_str().unwrap().to_owned();
    assert_eq!(sha.len(), 64);
    let read = session["model"].clone();
    assert_eq!(
        ModelDocument::from_json(&read.to_string()).unwrap(),
        document()
    );

    // A verified change applies, joins undo history and is announced.
    let change = |sha: &str, model: serde_json::Value, message: &str| json!({"expected_model_sha256": sha, "model": model, "author": "assistant", "message": message});
    let (status, applied) = post(
        &mut studio,
        "/api/session/apply",
        &change(
            &sha,
            with_thickness(&read, 8.0),
            "Thicker plate for stiffness",
        ),
    );
    assert_eq!(status, 200, "{applied}");
    assert_eq!(applied["changed"], true);
    assert_eq!(applied["dirty"], true);
    assert_ne!(applied["model_sha256"], json!(sha));
    let state = get(&mut studio, "/api/state").1;
    assert_eq!(thickness(&state), 8.0);
    assert_eq!(state["version"], applied["version"]);
    assert_eq!(
        state["last_change"],
        json!({"author": "assistant", "message": "Thicker plate for stiffness", "version": applied["version"]})
    );
    // The model on disk is untouched until the person saves.
    assert_eq!(
        ModelDocument::from_json(&fs::read_to_string(&model).unwrap()).unwrap(),
        document()
    );

    // The old hash is now stale: nothing changes and the reply says why.
    let (status, conflict) = post(
        &mut studio,
        "/api/session/apply",
        &change(&sha, with_thickness(&read, 12.0), "Overwrite"),
    );
    assert_eq!(status, 409);
    assert!(
        conflict["error"]
            .as_str()
            .unwrap()
            .contains("changed since it was read")
    );
    assert_eq!(thickness(&get(&mut studio, "/api/state").1), 8.0);

    // A person's edit also moves the hash on.
    let current = applied["model_sha256"].as_str().unwrap().to_owned();
    let edit = studio.handle(
        "POST",
        "/api/parameters",
        json!({"set": {"thickness": 6.0}}).to_string().as_bytes(),
    );
    assert_eq!(edit.status, 200);
    let state = get(&mut studio, "/api/state").1;
    assert_eq!(state["last_change"], serde_json::Value::Null);
    let (status, _) = post(
        &mut studio,
        "/api/session/apply",
        &change(&current, with_thickness(&read, 9.0), "Late"),
    );
    assert_eq!(status, 409);

    // Undo takes the person's edit back, then the assistant's.
    for expected in [8.0, 5.0] {
        let version = get(&mut studio, "/api/state").1["version"].clone();
        let undo = studio.handle(
            "POST",
            "/api/undo",
            json!({"version": version}).to_string().as_bytes(),
        );
        assert_eq!(undo.status, 200);
        assert_eq!(thickness(&get(&mut studio, "/api/state").1), expected);
    }
    let session = get(&mut studio, "/api/session").1;
    assert_eq!(session["model_sha256"], json!(sha));
    assert_eq!(session["dirty"], false);
}

#[test]
fn session_changes_are_validated_and_bounded() {
    let directory = Directory::new();
    let model = PathBuf::from(directory.model(&document()));
    let mut studio = serve::Studio::load(&model, None).unwrap();
    let session = get(&mut studio, "/api/session").1;
    let sha = session["model_sha256"].clone();
    let read = session["model"].clone();
    let version = session["version"].clone();
    let apply = |model: serde_json::Value, author: &str, message: &str| json!({"expected_model_sha256": sha, "model": model, "author": author, "message": message});
    // Out-of-range parameters and broken documents leave the session alone.
    for (model, status) in [
        (with_thickness(&read, 50.0), 422),
        (json!({"schema_version": 1}), 422),
    ] {
        let (code, reply) = post(
            &mut studio,
            "/api/session/apply",
            &apply(model, "assistant", "Bad"),
        );
        assert_eq!(code, status, "{reply}");
        assert!(reply["error"].is_string());
    }
    for (author, message) in [
        ("", "Empty author"),
        ("assistant", " "),
        ("assistant", &"x".repeat(201)),
    ] {
        let (code, _) = post(
            &mut studio,
            "/api/session/apply",
            &apply(read.clone(), author, message),
        );
        assert_eq!(code, 400);
    }
    let (code, _) = post(&mut studio, "/api/session/apply", &json!({"model": read}));
    assert_eq!(code, 400);
    // An identical model is accepted as a no-op.
    let (code, reply) = post(
        &mut studio,
        "/api/session/apply",
        &apply(read.clone(), "assistant", "Same"),
    );
    assert_eq!(code, 200);
    assert_eq!(reply["changed"], false);
    let state = get(&mut studio, "/api/state").1;
    assert_eq!(state["version"], version);
    assert_eq!(state["dirty"], false);
    assert_eq!(state["last_change"], serde_json::Value::Null);
    assert_eq!(state["undo_count"], 0);
}
