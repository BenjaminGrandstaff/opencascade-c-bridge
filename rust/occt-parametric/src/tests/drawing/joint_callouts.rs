//! Hole callouts and thread lines for fastened-joint holes (schema 186),
//! measured on the generated parts.

use super::*;

/// The fastened-lid example with two datums on the lid's top face and a
/// drawing calling out both parts' joint holes.
fn lid_with_drawing(thread_representation: bool) -> ModelDocument {
    let request: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tools/model/fastened-lid.request.json"
        ))
        .unwrap(),
    )
    .unwrap();
    let mut document: ModelDocument = serde_json::from_value(request["model"].clone()).unwrap();
    for (id, x) in [("a", 10.0), ("b", 12.75)] {
        document.family.datums.push(DatumDefinition {
            id: id.into(),
            kind: DatumKind::Point {
                origin: VectorExpr::Literal(VectorQuantity::lengths(
                    x,
                    10.0,
                    28.0,
                    LengthUnit::Millimeter,
                )),
            },
        });
    }
    let mut page = drawing();
    page.notes.clear();
    let view = &mut page.views[0];
    view.outputs = vec![InstanceOutputRef {
        instance: "lid".into(),
        output: "assembly".into(),
    }];
    view.origin = VectorQuantity::lengths(0.0, 0.0, 28.0, LengthUnit::Millimeter);
    view.scale = 1.0;
    view.thread_representation = thread_representation;
    page.dimensions = ["lid_drilled", "base_drilled"]
        .into_iter()
        .map(|holes| DrawingDimension {
            id: holes.into(),
            view: "top".into(),
            first: DatumRef::new("lid", "a"),
            second: DatumRef::new("lid", "b"),
            direction: DimensionDirection::Diameter,
            presentation: DimensionPresentation {
                hole: Some(InstanceOutputRef {
                    instance: "lid".into(),
                    output: holes.into(),
                }),
                ..Default::default()
            },
            offset_mm: 10.0,
            precision: 3,
        })
        .collect();
    document.drawings.push(page);
    document
}

#[test]
fn joint_holes_are_called_out_from_the_measured_joint() {
    let document = lid_with_drawing(false);
    let graph = document.instance_graph().unwrap();
    let session = Session::new().unwrap();
    let page = &document.drawings[0];
    // Validation alone needs no kernel and accepts the references.
    page.validate(&graph).unwrap();
    let generated = page
        .generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap();
    let labels: Vec<&str> = generated.labels.iter().map(|l| l.text.as_str()).collect();
    // Four M5 screws counterbored into the 8 mm lid, tapped 9 mm into the
    // base: 9 + 2 × 0.8 mm of 4.2 mm tap drill with a 118° point.
    assert_eq!(
        labels,
        [
            "4× Ø5.500 mm THRU; CBORE Ø9.750 DEPTH 5.000 mm",
            "4× Ø4.200 mm FULL DIA DEPTH 10.600 mm; DRILL POINT 118.000°; THREAD M5x0.8 (5.000 x 0.800 mm, RH)",
        ]
    );
    assert!(generated.to_dxf().contains("THREAD M5x0.8"));
    // Measured inspection nominals are the joint's hole diameters; without
    // the measured joints the core refuses rather than guess.
    let measurements = [DrawingDimensionMeasurement {
        dimension: "base_drilled".into(),
        value: Quantity::length(4.21, LengthUnit::Millimeter),
    }];
    let error = page
        .evaluate_dimension_measurements(&graph, &measurements)
        .unwrap_err();
    assert!(
        error.message.contains("measured on the generated parts"),
        "{}",
        error.message
    );
    let mut regenerated = document.instance_graph().unwrap();
    let generation = regenerated.regenerate_all(&session).unwrap();
    let joints =
        joint_hole_callouts(&session, &regenerated, &generation, &page.dimensions).unwrap();
    assert_eq!(joints.len(), 2);
    let result = page
        .evaluate_dimension_measurements_with(&graph, &measurements, Some(joints))
        .unwrap();
    let evaluation = serde_json::to_value(result[0].evaluation).unwrap();
    assert_eq!(
        evaluation["nominal"],
        serde_json::to_value(Quantity::length(4.2, LengthUnit::Millimeter)).unwrap(),
        "{evaluation}"
    );
    drop(generation);
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn joint_tapped_holes_get_thread_lines() {
    let session = Session::new().unwrap();
    let count = |thread_representation: bool| {
        let document = lid_with_drawing(thread_representation);
        let mut page = document.drawings[0].clone();
        page.dimensions.clear();
        let graph = document.instance_graph().unwrap();
        let generated = page
            .generate(&graph, &session, DrawingRenderOptions::default())
            .unwrap();
        generated.polylines.iter().filter(|p| p.thin).count()
    };
    // Seen along the axis, each of the four tapped holes gets three
    // quarters of its root circle.
    assert_eq!(count(true) - count(false), 4);
}
