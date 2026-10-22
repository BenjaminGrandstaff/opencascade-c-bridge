//! One-shot build requests: edits, failures, exports, and migration.

use super::*;

#[test]
fn bracket_edit_failure_repair_exports_and_preservation() {
    let dir = Directory::new();
    let model = fixture();
    let original = model.to_json_pretty().unwrap();
    let mut initial = request(&model, json!([]));
    initial["step"] = json!(true);
    initial["stl"] = json!(true);
    initial["preview"] = json!(true);
    let built = invoke(&dir, initial, "accepted").unwrap();
    assert_eq!(built["status"], "built");
    assert_eq!(built["verification"].as_array().unwrap().len(), 2);
    assert!(
        built["verification"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["status"] == "passed")
    );
    let session = Session::new().unwrap();
    let imported = session
        .load_step(dir.0.join("accepted/parts.step"))
        .unwrap();
    assert!(session.is_valid(&imported).unwrap());
    assert!(fs::metadata(dir.0.join("accepted/0001.stl")).unwrap().len() > 84);
    assert!(
        fs::read_to_string(dir.0.join("accepted/0001.svg"))
            .unwrap()
            .contains("<polyline")
    );
    let changed = invoke(&dir, request(&model, edit("hole_spacing", 35.0)), "changed").unwrap();
    assert!(
        (changed["outputs"][0]["volume_mm3"].as_f64().unwrap()
            - built["outputs"][0]["volume_mm3"].as_f64().unwrap())
        .abs()
            < 1e-7
    );
    let accepted = fs::read(dir.0.join("accepted/report.json")).unwrap();
    let err = invoke(&dir, request(&model, edit("thickness", 1.0)), "rejected").unwrap_err();
    assert_eq!(err.stage, "regeneration");
    assert!(err.message.contains("wall"));
    assert!(!dir.0.join("rejected").exists());
    invoke(&dir, request(&model, edit("thickness", 5.0)), "repaired").unwrap();
    assert_eq!(
        fs::read(dir.0.join("accepted/report.json")).unwrap(),
        accepted
    );
    assert_eq!(model.to_json_pretty().unwrap(), original);
    let saved =
        ModelDocument::from_json(&fs::read_to_string(dir.0.join("changed/model.json")).unwrap())
            .unwrap();
    assert_eq!(saved.family.requirements, model.family.requirements);
    assert_eq!(
        saved.instances[0].overrides()["hole_spacing"],
        ParameterValue::Scalar(Quantity::length(35.0, LengthUnit::Millimeter))
    );
    assert_eq!(
        invoke(&dir, request(&model, json!([])), "accepted")
            .unwrap_err()
            .stage,
        "publication"
    );
}
#[test]
fn malformed_requests_edits_selections_and_future_schema_rejected() {
    let dir = Directory::new();
    let model = fixture();
    for (name, stage, change) in [
        ("future", "validation", ("future", json!(999))),
        ("unknown", "request", ("unknown", json!(true))),
        ("empty", "request", ("outputs", json!([]))),
        (
            "missing",
            "selection",
            (
                "outputs",
                json!([{"instance":"bracket","output":"missing"}]),
            ),
        ),
        ("edit", "regeneration", ("edits", edit("missing", 1.0))),
        (
            "duplicate",
            "edits",
            (
                "edits",
                json!([edit("thickness", 4.0)[0], edit("thickness", 5.0)[0]]),
            ),
        ),
    ] {
        let mut value = request(&model, json!([]));
        if change.0 == "future" {
            value["model"]["schema_version"] = change.1;
        } else {
            value[change.0] = change.1;
        }
        assert_eq!(invoke(&dir, value, name).unwrap_err().stage, stage);
        assert!(!dir.0.join(name).exists());
    }
}
#[test]
fn migration_and_source_metadata_survive() {
    let dir = Directory::new();
    let model = fixture();
    let mut value = request(&model, json!([]));
    value["model"]["schema_version"] = json!(68);
    invoke(&dir, value, "old").unwrap();
    let saved =
        ModelDocument::from_json(&fs::read_to_string(dir.0.join("old/model.json")).unwrap())
            .unwrap();
    assert_eq!(saved, model);
}

#[test]
fn step_pmi_writes_drawing_gdt_on_faces_and_reports_the_read_back() {
    let dir = Directory::new();
    let mut model = fixture();
    let plane = |id: &str, z: f64, normal: f64| DatumDefinition {
        id: id.into(),
        kind: DatumKind::Plane {
            origin: point(0.0, 0.0, z),
            normal: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, normal)),
        },
    };
    model.family.datums = vec![
        plane("bottom", 0.0, -1.0),
        DatumDefinition {
            id: "hole".into(),
            kind: DatumKind::Axis {
                origin: point(10.0, 18.0, 0.0),
                direction: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
            },
        },
    ];
    model.family.references = vec![
        NamedReference {
            name: "bottom_face".into(),
            target: ReferenceTarget::Faces(FaceSelector::AtExtreme {
                axis: CoordinateAxis::Z,
                extremum: Extremum::Minimum,
                tolerance: mm(1e-6),
            }),
        },
        NamedReference {
            name: "first_bore".into(),
            target: ReferenceTarget::Faces(FaceSelector::NearestCenter {
                target: point(10.0, 18.0, 2.0),
                maximum_distance: mm(1.0),
            }),
        },
    ];
    let attachment = |anchor: &str, faces: &str| {
        json!({"view":"top","output":{"instance":"bracket","output":"body"},
            "anchor":{"instance":"bracket","datum":anchor},"offset_mm":[10.0,10.0],
            "faces":{"named":faces}})
    };
    let length = |v: f64| json!({"value":v,"dimension":"length","unit":"millimeter"});
    model.drawings.push(
        serde_json::from_value(json!({
            "id":"sheet","title":"Bracket","paper_size_mm":[297.0,210.0],
            "views":[{"id":"top","outputs":[{"instance":"bracket","output":"body"}],
                "origin":{"x":length(0.0),"y":length(0.0),"z":length(0.0)},
                "direction":{"x":{"value":0.0,"dimension":"scalar"},"y":{"value":0.0,"dimension":"scalar"},"z":{"value":1.0,"dimension":"scalar"}},
                "x_axis":{"x":{"value":1.0,"dimension":"scalar"},"y":{"value":0.0,"dimension":"scalar"},"z":{"value":0.0,"dimension":"scalar"}},
                "paper_origin_mm":[40.0,60.0],"scale":1.0,"show_hidden":false}],
            "datum_features":[{"id":"A","label":"A","feature_of_size":false,
                "attachment":attachment("bottom","bottom_face")}],
            "feature_control_frames":[{"id":"bore","attachment":attachment("hole","first_bore"),
                "characteristic":"position","tolerance":length(0.1),"display_unit":"millimeter",
                "precision":2,"zone":"diameter","material":"maximum","feature_of_size":true,
                "size_limits":{"kind":"internal","lower":length(4.9),"upper":length(5.1)},
                "datums":[{"datum_feature":"A"}]}],
            "dataset":{"identifier":"BRK-200","organization":"Example Works","authority":"drawing"},
            "releases":[{"revision":"A","description":"Initial release","date":"2026-10-01",
                "approvals":[{"role":"APPROVED","name":"M. Ortiz","date":"2026-10-01"}]}],
            "annotation_views":[{"id":"iso","name":"ISO GD&T",
                "direction":{"x":{"value":1.0,"dimension":"scalar"},"y":{"value":-1.0,"dimension":"scalar"},"z":{"value":1.0,"dimension":"scalar"}},
                "x_axis":{"x":{"value":1.0,"dimension":"scalar"},"y":{"value":1.0,"dimension":"scalar"},"z":{"value":0.0,"dimension":"scalar"}},
                "datum_features":["A"],"feature_control_frames":["bore"]}]
        }))
        .unwrap(),
    );
    let mut value = request(&model, json!([]));
    value["step"] = json!(true);
    value["step_pmi"] = json!("sheet");
    let built = invoke(&dir, value.clone(), "pmi").unwrap();
    let pmi = &built["pmi"];
    assert_eq!(pmi["drawing"], "sheet");
    assert_eq!(pmi["verified"], true, "{pmi:#}");
    assert_eq!(pmi["complete"], true, "{pmi:#}");
    let kinds: Vec<_> = pmi["report"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| (i["id"].as_str().unwrap(), i["kind"].as_str().unwrap()))
        .collect();
    assert_eq!(
        kinds,
        vec![("A", "datum"), ("bore", "tolerance"), ("bore", "size")]
    );
    assert_eq!(
        pmi["report"]["views"],
        json!([{"id":"iso","name":"ISO GD&T","annotations":["A","bore"]}])
    );
    assert_eq!(
        pmi["report"]["dataset"],
        json!({"identifier":"BRK-200","authority":"drawing","revision":"A"})
    );
    // The release names no model revision and the model has no ledger.
    assert_eq!(pmi["release"], json!({"status":"untracked","revision":"A"}));
    assert!(dir.0.join("pmi/parts.step").exists());

    value["step_pmi"] = json!("missing");
    let error = invoke(&dir, value.clone(), "unknown").unwrap_err();
    assert_eq!(error.stage, "request");
    assert!(error.message.contains("unknown drawing 'missing'"));
    value["step_pmi"] = json!("sheet");
    value["step"] = json!(false);
    let error = invoke(&dir, value, "no-step").unwrap_err();
    assert!(error.message.contains("needs a STEP export"));
}

#[test]
fn direct_edit_example_lifts_the_top_and_moves_the_hole_by_parameter() {
    let dir = Directory::new();
    let mut example = view_example("direct-edit");
    example["step"] = json!(false);
    example["preview"] = json!(false);
    let built = invoke(&dir, example.clone(), "default").unwrap();
    assert_eq!(built["status"], "built", "{built}");
    let hole = std::f64::consts::PI * 16.0;
    let volume = |report: &Value| report["outputs"][0]["volume_mm3"].as_f64().unwrap();
    assert!((volume(&built) - (60.0 * 40.0 - hole) * 24.0).abs() < 1e-6);
    assert_eq!(built["outputs"][0]["bounds_mm"]["max"][2], 24.0);
    // Moving the hole further keeps the volume; a taller lift is built
    // exactly and then rejected by the volume requirement.
    let edit = |parameter: &str, value: f64| json!([{"instance":"plate","parameter":parameter,"value":ParameterValue::Scalar(Quantity::length(value,LengthUnit::Millimeter))}]);
    example["edits"] = edit("shift", 30.0);
    let shifted = invoke(&dir, example.clone(), "shifted").unwrap();
    assert!((volume(&shifted) - volume(&built)).abs() < 1e-6);
    example["edits"] = edit("lift", 8.0);
    let lifted = invoke(&dir, example, "lifted").unwrap_err();
    let measured = lifted.details["verification"][0]["measured"]["value"]
        .as_f64()
        .unwrap();
    assert!((measured - (60.0 * 40.0 - hole) * 28.0).abs() < 1e-6);
    assert_eq!(
        lifted.details["required_failures"],
        json!(["plate/moved-volume"])
    );
}

#[test]
fn vent_panel_example_fills_curves_and_tables_its_holes() {
    // Vent sites: a staggered grid through (14, 14) at `pitch`, inside
    // 10..90 square at least 3 mm from its edges and 3 mm from the Ø24
    // keep-out circle at (50, 50).
    let vents = |pitch: f64| {
        let mut count = 0;
        for j in -20..40 {
            for i in -20..40 {
                let x = 14.0 + pitch * i as f64 + if j % 2 != 0 { pitch / 2.0 } else { 0.0 };
                let y = 14.0 + pitch * j as f64;
                let edge = (x - 10.0).min(90.0 - x).min(y - 10.0).min(90.0 - y);
                if edge >= 3.0 - 1e-9 && (x - 50.0).hypot(y - 50.0) >= 15.0 - 1e-9 {
                    count += 1;
                }
            }
        }
        count as f64
    };
    let pi = std::f64::consts::PI;
    let volume = |pitch: f64| {
        let holes = vents(pitch) * 4.0 * pi + 64.0 * pi + 6.0 * 1.6 * 1.6 * pi + 3.0 * 6.25 * pi;
        3.0 * (160.0 * 100.0 - holes)
    };
    let dir = Directory::new();
    let mut example = view_example("vent-panel");
    example["step"] = json!(false);
    example["preview"] = json!(false);
    let built = invoke(&dir, example.clone(), "default").unwrap();
    assert_eq!(built["status"], "built", "{built}");
    let measured = built["outputs"][0]["volume_mm3"].as_f64().unwrap();
    assert!(
        (measured - volume(8.0)).abs() < 1e-6,
        "{measured} vs {}",
        volume(8.0)
    );
    example["edits"] = json!([{"instance":"panel","parameter":"pitch","value":ParameterValue::Scalar(Quantity::length(11.0,LengthUnit::Millimeter))}]);
    let wider = invoke(&dir, example, "wider").unwrap();
    let measured = wider["outputs"][0]["volume_mm3"].as_f64().unwrap();
    assert!(
        (measured - volume(11.0)).abs() < 1e-6,
        "{measured} vs {}",
        volume(11.0)
    );
    assert!(vents(11.0) < vents(8.0));
}
