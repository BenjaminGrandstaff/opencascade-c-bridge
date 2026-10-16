//! Hole callouts driven by hole features and geometry.

use super::*;

fn callout_page() -> DrawingDefinition {
    let mut page = drawing();
    page.notes.clear();
    page.views[0].outputs[0].output = "hole".into();
    let mut dimension =
        manufactured_dimension(DimensionDirection::Diameter, DimensionTolerance::None);
    dimension.presentation.hole = Some(InstanceOutputRef {
        instance: "part".into(),
        output: "hole".into(),
    });
    page.dimensions = vec![dimension];
    page
}
#[test]
fn hole_callouts_follow_feature_edits_and_preserve_recess_depth_and_thread_intent() {
    let mm = |v| ScalarExpr::Literal(Quantity::length(v, LengthUnit::Millimeter));
    let definition = callout_family(
        HoleFinish::Counterbore {
            diameter: mm(6.0),
            depth: mm(2.0),
        },
        HoleExtent::Blind { depth: mm(8.0) },
    );
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let page = callout_page();
    let session = Session::new().unwrap();
    let mut limited = page.clone();
    limited.dimensions[0].presentation.tolerance = DimensionTolerance::Limits {
        lower: Quantity::length(3.9, LengthUnit::Millimeter),
        upper: Quantity::length(4.1, LengthUnit::Millimeter),
    };
    let limited = limited
        .generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap();
    assert!(limited.labels[0].text.starts_with("Ø4.100/3.900 mm"));
    assert_eq!(limited.labels[0].stack.as_ref().unwrap().upper, "4.100");
    let before = page
        .generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap();
    assert_eq!(
        before.labels[0].text,
        "Ø4.000 mm DEPTH 8.000 mm; CBORE Ø6.000 DEPTH 2.000 mm; THREAD M5x0.8 (5.000 x 0.800 mm, LH)"
    );
    graph
        .set_override(
            "part",
            "bore",
            ParameterValue::Scalar(Quantity::length(4.2, LengthUnit::Millimeter)),
        )
        .unwrap();
    let mut document = ModelDocument::from_graph(&graph);
    document.drawings.push(page.clone());
    let reloaded = ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap();
    let after = reloaded.drawings[0]
        .generate(
            &reloaded.instance_graph().unwrap(),
            &session,
            DrawingRenderOptions::default(),
        )
        .unwrap();
    assert!(after.labels[0].text.starts_with("Ø4.200 mm"));
    assert!(after.to_dxf().contains("THREAD M5x0.8"));
    assert_eq!(session.shape_count().unwrap(), 0);
}
#[test]
fn hole_callouts_cover_through_plain_countersink_and_invalid_references() {
    let mm = |v| ScalarExpr::Literal(Quantity::length(v, LengthUnit::Millimeter));
    let session = Session::new().unwrap();
    for finish in [
        HoleFinish::Plain,
        HoleFinish::Countersink {
            diameter: mm(6.0),
            angle_radians: ScalarExpr::Literal(Quantity::scalar(std::f64::consts::FRAC_PI_2)),
        },
    ] {
        let definition = callout_family(finish.clone(), HoleExtent::ThroughAll);
        let mut graph = InstanceGraph::new(&definition);
        graph.add_base("part", HashMap::new(), "test").unwrap();
        let mut page = callout_page();
        let generated = page
            .generate(&graph, &session, DrawingRenderOptions::default())
            .unwrap();
        assert!(generated.labels[0].text.contains("THRU"));
        if !matches!(finish, HoleFinish::Plain) {
            assert!(
                generated.labels[0]
                    .text
                    .contains("CSINK Ø6.000 mm x 90.000°")
            );
        }
        for reference in [
            InstanceOutputRef {
                instance: "missing".into(),
                output: "hole".into(),
            },
            InstanceOutputRef {
                instance: "part".into(),
                output: "body".into(),
            },
            InstanceOutputRef {
                instance: "part".into(),
                output: "missing".into(),
            },
        ] {
            page.dimensions[0].presentation.hole = Some(reference);
            assert!(
                page.generate(&graph, &session, DrawingRenderOptions::default())
                    .is_err()
            );
        }
        page = callout_page();
        page.dimensions[0].direction = DimensionDirection::Radius;
        assert!(
            page.generate(&graph, &session, DrawingRenderOptions::default())
                .is_err()
        );
        assert_eq!(session.shape_count().unwrap(), 0);
    }
}

#[test]
fn pointed_hole_callouts_distinguish_full_diameter_depth_and_included_angle() {
    let mut definition = callout_family(
        HoleFinish::Plain,
        HoleExtent::Blind {
            depth: ScalarExpr::Literal(Quantity::length(4.0, LengthUnit::Millimeter)),
        },
    );
    let FeatureOperation::Hole { bottom, .. } =
        &mut definition.features.last_mut().unwrap().operation
    else {
        panic!()
    };
    *bottom = HoleBottom::DrillPoint {
        angle_radians: ScalarExpr::Literal(Quantity::scalar(2.0 * std::f64::consts::PI / 3.0)),
    };
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let session = Session::new().unwrap();
    let sheet = callout_page()
        .generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap();
    assert!(
        sheet.labels[0]
            .text
            .contains("FULL DIA DEPTH 4.000 mm; DRILL POINT 120.000°")
    );
    assert!(sheet.to_dxf().contains("DRILL POINT 120.000"));
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn geometry_driven_hole_callouts_preserve_the_face_limit_mode() {
    let session = Session::new().unwrap();
    for (extent, label) in [
        (HoleExtent::UpToNext, "UP TO NEXT FACE"),
        (
            HoleExtent::UpToFace {
                face: Box::new(FaceSelector::AtExtreme {
                    axis: CoordinateAxis::Z,
                    extremum: Extremum::Maximum,
                    tolerance: ScalarExpr::Literal(Quantity::length(1e-6, LengthUnit::Millimeter)),
                }),
            },
            "UP TO FACE",
        ),
    ] {
        let definition = callout_family(HoleFinish::Plain, extent);
        let mut graph = InstanceGraph::new(&definition);
        graph.add_base("part", HashMap::new(), "test").unwrap();
        let sheet = callout_page()
            .generate(&graph, &session, DrawingRenderOptions::default())
            .unwrap();
        assert!(sheet.labels[0].text.contains(label));
        assert!(sheet.to_dxf().contains(label));
        assert_eq!(session.shape_count().unwrap(), 0);
    }
}
#[test]
fn tolerance_classes_set_callout_limits_and_judge_measured_sizes() {
    let mm = |v| ScalarExpr::Literal(Quantity::length(v, LengthUnit::Millimeter));
    let definition = callout_family(HoleFinish::Plain, HoleExtent::Blind { depth: mm(8.0) });
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let session = Session::new().unwrap();
    let class = |text: &str| ToleranceClass::try_from(text.to_owned()).unwrap();
    let mut page = callout_page();
    page.dimensions[0].presentation.tolerance = DimensionTolerance::FitClass { class: class("H7") };
    let generated = page
        .generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap();
    // H7 over 3-6 mm is 12 µm above the 4 mm bore.
    assert!(
        generated.labels[0]
            .text
            .starts_with("Ø4.000 H7 (4.012/4.000) mm DEPTH 8.000 mm"),
        "{}",
        generated.labels[0].text
    );
    let measure = |value: f64| DrawingDimensionMeasurement {
        dimension: "manufactured".into(),
        value: Quantity::length(value, LengthUnit::Millimeter),
    };
    let results = page
        .evaluate_dimension_measurements(&graph, &[measure(4.010), measure(4.013), measure(3.999)])
        .unwrap();
    let dispositions: Vec<_> = results.iter().map(|r| r.evaluation.disposition).collect();
    assert_eq!(
        dispositions,
        [
            DimensionMeasurementDisposition::WithinLimits,
            DimensionMeasurementDisposition::AboveUpperLimit,
            DimensionMeasurementDisposition::BelowLowerLimit,
        ]
    );
    let limits = results[0].evaluation.limits.unwrap();
    assert!((limits.upper.value - 4.012).abs() < 1e-12);

    // Inch callouts show the same limits converted.
    let mut inch = page.clone();
    inch.dimensions[0].presentation.length_unit = LengthUnit::Inch;
    inch.dimensions[0].precision = 5;
    let generated = inch
        .generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap();
    assert!(
        generated.labels[0]
            .text
            .starts_with("Ø0.15748 H7 (0.15795/0.15748) in"),
        "{}",
        generated.labels[0].text
    );

    // A shaft class on a plain 5 mm linear dimension sits below nominal.
    let annotated = annotated_family();
    let mut plain_graph = InstanceGraph::new(&annotated);
    plain_graph
        .add_base("part", HashMap::new(), "test")
        .unwrap();
    let mut shaft = drawing();
    shaft.dimensions = vec![manufactured_dimension(
        DimensionDirection::Horizontal,
        DimensionTolerance::FitClass { class: class("h6") },
    )];
    let generated = shaft
        .generate(&plain_graph, &session, DrawingRenderOptions::default())
        .unwrap();
    let label = generated
        .labels
        .iter()
        .find(|l| l.text.contains("h6"))
        .unwrap();
    assert_eq!(label.text, "5.000 h6 (5.000/4.992) mm");

    // Radii and angles are not sizes; a hole over 500 mm is off the table.
    for direction in [
        DimensionDirection::Radius,
        DimensionDirection::Angular {
            vertex: DatumRef::new("part", "y"),
        },
    ] {
        let mut bad = shaft.clone();
        bad.dimensions[0].direction = direction;
        assert!(
            bad.generate(&plain_graph, &session, DrawingRenderOptions::default())
                .is_err()
        );
    }
    graph
        .set_override(
            "part",
            "bore",
            ParameterValue::Scalar(Quantity::length(501.0, LengthUnit::Millimeter)),
        )
        .unwrap();
    let error = page
        .evaluate_dimension_measurements(&graph, &[measure(501.0)])
        .unwrap_err();
    assert!(error.to_string().contains("up to 500 mm"), "{error}");

    let mut document = ModelDocument::from_graph(&plain_graph);
    document.drawings.push(shaft.clone());
    let json = document.to_json_pretty().unwrap();
    assert!(json.contains("\"class\": \"h6\""));
    assert_eq!(ModelDocument::from_json(&json).unwrap(), document);
    assert!(
        ModelDocument::from_json(&json.replace("\"class\": \"h6\"", "\"class\": \"g6\"")).is_err()
    );
    assert_eq!(session.shape_count().unwrap(), 0);
}
