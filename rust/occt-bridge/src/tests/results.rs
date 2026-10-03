//! Session options, result validation and healing, diagnostics, and handle release.

use super::*;

#[test]
fn session_options_round_trip_and_reject_invalid_values() {
    let session = Session::new().unwrap();
    assert_eq!(session.options().unwrap(), SessionOptions::default());
    let healing = SessionOptions {
        heal_invalid_results: true,
        boolean_fuzzy_tolerance: 1e-5,
        ..SessionOptions::default()
    };
    session.set_options(healing).unwrap();
    assert_eq!(session.options().unwrap(), healing);
    for invalid in [
        SessionOptions {
            boolean_fuzzy_tolerance: -1.0,
            ..healing
        },
        SessionOptions {
            boolean_fuzzy_tolerance: f64::NAN,
            ..healing
        },
        SessionOptions {
            validate_results: false,
            ..healing
        },
    ] {
        assert_eq!(session.set_options(invalid).unwrap_err().status, 1);
    }
    assert_eq!(session.options().unwrap(), healing);
}

#[test]
fn invalid_results_are_rejected_healed_or_allowed_by_option() {
    let session = Session::new().unwrap();
    let error = session.load_brep(fixture("bowtie_face.brep")).unwrap_err();
    assert_eq!(error.status, 4);
    assert!(
        error
            .message
            .contains("BREP load produced an invalid shape"),
        "{error}"
    );
    // OCCT's STEP reader heals during transfer, so the same defect
    // arrives valid; validation still guards what it produces.
    let imported = session.load_step(fixture("bowtie_face.step")).unwrap();
    assert!(session.is_valid(&imported).unwrap());

    session
        .set_options(SessionOptions {
            validate_results: false,
            ..SessionOptions::default()
        })
        .unwrap();
    let unchecked = session.load_brep(fixture("bowtie_face.brep")).unwrap();
    assert!(!session.is_valid(&unchecked).unwrap());

    session
        .set_options(SessionOptions {
            heal_invalid_results: true,
            ..SessionOptions::default()
        })
        .unwrap();
    let healed = session.load_brep(fixture("bowtie_face.brep")).unwrap();
    assert_eq!(
        session.last_warnings(),
        ["BREP load result was invalid and was healed"]
    );
    assert!(session.is_valid(&healed).unwrap());
    session.shape_count().unwrap();
    assert!(
        session.last_warnings().is_empty(),
        "warnings clear on the next call"
    );

    let error = session.load_brep(fixture("gapped_face.brep")).unwrap_err();
    assert!(
        error.message.contains("healing could not repair"),
        "{error}"
    );
}

#[test]
fn kernel_failures_name_their_cause_and_culprit() {
    let session = Session::new().unwrap();
    let block = session
        .create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 10.0, 10.0))
        .unwrap();
    let first = session.subshape(&block, ShapeType::Edge, 0).unwrap();
    let second = session.subshape(&block, ShapeType::Edge, 3).unwrap();
    let error = session
        .fillet(&block, &[&first, &second], 20.0)
        .unwrap_err();
    assert_eq!(error.status, 6);
    assert_eq!(error.diagnostics.len(), 2);
    let selected = error
        .diagnostics
        .iter()
        .map(|diagnostic| {
            assert_eq!(diagnostic.kind, DiagnosticKind::FilletEdge);
            assert_eq!(diagnostic.name, "ChFiDS_StartsolFailure");
            assert!(diagnostic.has_shape);
            diagnostic.input_index.unwrap()
        })
        .collect::<Vec<_>>();
    assert_eq!(selected, vec![0, 1]);
    assert!(
        error
            .message
            .contains("ChFiDS_StartsolFailure on selection 0 and 1 more")
    );
    let culprit = session.last_diagnostic_shape(1).unwrap().unwrap();
    assert_eq!(session.last_diagnostics(), error.diagnostics);
    assert!(session.is_same(&culprit, &second).unwrap());
    assert!(session.last_diagnostics().is_empty());
    assert!(session.last_diagnostic_shape(0).is_err());

    let error = session.fillet(&block, &[&first], 1.0).map(|_| ());
    assert!(error.is_ok());
    assert!(session.last_diagnostics().is_empty());
}

#[test]
fn healing_carries_operation_history_and_booleans_report_warnings() {
    let session = Session::new().unwrap();
    session
        .set_options(SessionOptions {
            validate_results: false,
            ..SessionOptions::default()
        })
        .unwrap();
    let bowtie = session.load_brep(fixture("bowtie_face.brep")).unwrap();
    let shifted = session
        .translate(&bowtie, Vec3::new(1.0, 0.0, 0.0))
        .unwrap();
    session
        .set_options(SessionOptions {
            heal_invalid_results: true,
            ..SessionOptions::default()
        })
        .unwrap();

    // Sewing two overlapping bow-ties yields an invalid shell that
    // healing repairs; history must lead from each input face to the
    // repaired result.
    let sewn = session.sew(&[&bowtie, &shifted], 1e-3).unwrap();
    // Warnings describe the most recent call, so read them first.
    assert_eq!(
        session.last_warnings(),
        ["sewing result was invalid and was healed"]
    );
    assert!(session.is_valid(&sewn).unwrap());
    let input_face = session.subshape(&bowtie, ShapeType::Face, 0).unwrap();
    let healed_faces = session
        .history_count(&sewn, &input_face, HistoryRelation::Modified)
        .unwrap();
    assert!(healed_faces >= 1);
    for index in 0..healed_faces {
        let face = session
            .history(&sewn, &input_face, HistoryRelation::Modified, index)
            .unwrap();
        assert_eq!(session.shape_type(&face).unwrap(), ShapeType::Face);
        assert!(session.is_adjacent(&sewn, &face, &face).is_ok());
    }

    // OCCT boolean alerts surface as warnings, one per line with the
    // operation name, instead of being dropped.
    let block = session
        .create_box(Vec3::new(2.0, 2.0, -1.0), Vec3::new(3.0, 3.0, 2.0))
        .unwrap();
    session.common(&bowtie, &block).unwrap();
    let warnings = session.last_warnings();
    assert!(!warnings.is_empty());
    assert!(
        warnings
            .iter()
            .all(|warning| warning.starts_with("common: ")),
        "{warnings:?}"
    );
    let error = session.fuse(&bowtie, &block).unwrap_err();
    assert!(error.message.contains("BOPAlgo_Alert"), "{error}");
}

#[test]
fn fuzzy_booleans_merge_nearly_touching_inputs() {
    let session = Session::new().unwrap();
    let left = session
        .create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 10.0, 10.0))
        .unwrap();
    let right = session
        .create_box(
            Vec3::new(10.0 + 1e-6, 0.0, 0.0),
            Vec3::new(10.0, 10.0, 10.0),
        )
        .unwrap();
    let exact = session.fuse(&left, &right).unwrap();
    assert_eq!(session.subshape_count(&exact, ShapeType::Solid).unwrap(), 2);
    session
        .set_options(SessionOptions {
            boolean_fuzzy_tolerance: 1e-5,
            ..SessionOptions::default()
        })
        .unwrap();
    let fuzzy = session.fuse(&left, &right).unwrap();
    assert_eq!(session.subshape_count(&fuzzy, ShapeType::Solid).unwrap(), 1);
    assert_eq!(session.subshape_count(&fuzzy, ShapeType::Face).unwrap(), 10);
    assert!(session.is_valid(&fuzzy).unwrap());
}

#[test]
fn dropped_handles_release_their_shapes_without_losing_diagnostics() {
    let session = Session::new().unwrap();
    let block = session
        .create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 1.0, 1.0))
        .unwrap();
    {
        let _temporary = session.translate(&block, Vec3::new(5.0, 0.0, 0.0)).unwrap();
        let _faces = (0..6)
            .map(|index| session.subshape(&block, ShapeType::Face, index).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(session.shape_count().unwrap(), 8);
    }
    assert_eq!(session.shape_count().unwrap(), 1);

    // Two handles to one geometry are independent.
    let copy = session.duplicate(&block).unwrap();
    drop(copy);
    assert!(session.is_valid(&block).unwrap());

    // Releasing temporaries between a call and reading its diagnostics
    // keeps both the warnings and the last error.
    session
        .set_options(SessionOptions {
            validate_results: false,
            ..SessionOptions::default()
        })
        .unwrap();
    let bowtie = session.load_brep(fixture("bowtie_face.brep")).unwrap();
    let cutter = session
        .create_box(Vec3::new(2.0, 2.0, -1.0), Vec3::new(3.0, 3.0, 2.0))
        .unwrap();
    let common = session.common(&bowtie, &cutter).unwrap();
    drop(common);
    drop(cutter);
    assert!(!session.last_warnings().is_empty());

    // Dropping handles invalidated by clearing is harmless.
    let stale = session.duplicate(&block).unwrap();
    session.clear().unwrap();
    drop(stale);
    drop(block);
    drop(bowtie);
    assert_eq!(session.shape_count().unwrap(), 0);
}
