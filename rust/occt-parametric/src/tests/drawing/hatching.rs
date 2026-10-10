//! Section hatching from sampled cut faces.

use super::*;

#[test]
fn section_hatching_follows_real_cut_faces_and_circular_holes_with_safe_exports() {
    let definition = callout_family(HoleFinish::Plain, HoleExtent::ThroughAll);
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let page = hatched_slice("hole");
    let session = Session::new().unwrap();
    let generated = page
        .generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap();
    assert!(generated.hatches.len() > 20);
    for line in &generated.hatches {
        let [a, b] = [line.points_mm[0], line.points_mm[1]];
        assert!((a[1] - b[1]).abs() < 1e-10);
        assert!(a[0] >= -1e-7 && b[0] <= 10.0 + 1e-7);
        for i in 1..10 {
            let x = a[0] + (b[0] - a[0]) * f64::from(i) / 10.0;
            // Polygonal chord approximation can enter the circle slightly;
            // 64 samples on a radius-2 circle bound this observed fixture error.
            assert!((x - 5.0).hypot(a[1] - 5.0) >= 1.99);
        }
    }
    let svg = generated.to_svg();
    let dxf = generated.to_dxf();
    assert!(svg.contains("stroke-width=\"0.13\""));
    assert!(dxf.contains("8\nSECTION_HATCH\n370\n13"));
    if std::env::var_os("OCCT_DRAWING_QA").is_some() {
        let mut qa = page.clone();
        qa.paper_size_mm = [100.0, 120.0];
        qa.views[0].paper_origin_mm = [20.0, 50.0];
        qa.views[0].scale = 2.0;
        qa.views[0].hatching = Some(SectionHatching::default());
        let qa = qa
            .generate(&graph, &session, DrawingRenderOptions::default())
            .unwrap();
        std::fs::write("/tmp/occb-hatched-hole.svg", qa.to_svg()).unwrap();
        std::fs::write("/tmp/occb-hatched-hole.dxf", qa.to_dxf()).unwrap();
    }
    let mut section = page.clone();
    section.views[0].kind = DrawingViewKind::Section {
        origin: VectorQuantity::lengths(0.0, 0.0, 5.0, LengthUnit::Millimeter),
        normal: VectorQuantity::scalars(0.0, 0.0, 1.0),
        keep_positive: false,
    };
    let section = section
        .generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap();
    assert!(!section.hatches.is_empty());
    let mut document = ModelDocument::from_graph(&graph);
    document.drawings.push(page.clone());
    let loaded = ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap();
    assert_eq!(loaded, document);
    graph
        .set_override(
            "part",
            "bore",
            ParameterValue::Scalar(Quantity::length(4.5, LengthUnit::Millimeter)),
        )
        .unwrap();
    let edited = page
        .generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap();
    assert_ne!(edited.hatches, generated.hatches);
    assert_eq!(session.shape_count().unwrap(), 0);
}
#[test]
fn hatching_unions_overlapping_components_and_keeps_disconnected_islands() {
    let definition = family_with_datums();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    graph
        .add_clone("overlap", "part", HashMap::new(), "test")
        .unwrap();
    graph
        .set_placement(
            "overlap",
            Placement::translated(VectorQuantity::lengths(
                5.0,
                0.0,
                0.0,
                LengthUnit::Millimeter,
            )),
        )
        .unwrap();
    graph
        .add_clone("island", "part", HashMap::new(), "test")
        .unwrap();
    graph
        .set_placement(
            "island",
            Placement::translated(VectorQuantity::lengths(
                30.0,
                0.0,
                0.0,
                LengthUnit::Millimeter,
            )),
        )
        .unwrap();
    let mut page = hatched_slice("body");
    page.views[0].outputs.extend([
        InstanceOutputRef {
            instance: "overlap".into(),
            output: "body".into(),
        },
        InstanceOutputRef {
            instance: "island".into(),
            output: "body".into(),
        },
    ]);
    let session = Session::new().unwrap();
    let generated = page
        .generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap();
    assert_eq!(generated.hatches.len(), 40);
    let mut section = page.clone();
    section.views[0].kind = DrawingViewKind::Section {
        origin: VectorQuantity::lengths(0.0, 0.0, 5.0, LengthUnit::Millimeter),
        normal: VectorQuantity::scalars(0.0, 0.0, 1.0),
        keep_positive: false,
    };
    let section = section
        .generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap();
    assert_eq!(section.hatches, generated.hatches);
    assert!(
        section
            .polylines
            .iter()
            .flat_map(|l| &l.points_mm)
            .any(|p| (p[0] - 40.0).abs() < 1e-7)
    );
    for y in [0.5, 5.5, 19.5] {
        let lines: Vec<_> = generated
            .hatches
            .iter()
            .filter(|l| (l.points_mm[0][1] - y).abs() < 1e-7)
            .collect();
        assert_eq!(lines.len(), 2);
        assert!((lines[0].points_mm[0][0]).abs() < 1e-7);
        assert!((lines[0].points_mm[1][0] - 15.0).abs() < 1e-7);
        assert!((lines[1].points_mm[0][0] - 30.0).abs() < 1e-7);
        assert!((lines[1].points_mm[1][0] - 40.0).abs() < 1e-7);
    }
    assert_eq!(session.shape_count().unwrap(), 0);
}
#[test]
fn hatching_clips_details_uses_paper_spacing_and_handles_empty_or_boundary_planes() {
    let definition = family_with_datums();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let session = Session::new().unwrap();
    let mut page = hatched_slice("body");
    page.views[0].scale = 2.0;
    page.views[0].paper_origin_mm = [10.0, 20.0];
    page.views[0].detail = Some(DrawingDetail {
        minimum_mm: [2.0, 3.0],
        maximum_mm: [8.0, 7.0],
    });
    let generated = page
        .generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap();
    assert_eq!(generated.hatches.len(), 8);
    for line in &generated.hatches {
        assert!((line.points_mm[0][0] - 10.0).abs() < 1e-7);
        assert!((line.points_mm[1][0] - 22.0).abs() < 1e-7);
        assert!(line.points_mm.iter().all(|p| p[1] >= 20.0 && p[1] <= 28.0));
    }
    for z in [0.0, 30.0] {
        let mut boundary = hatched_slice("body");
        boundary.views[0].origin.z.value = z;
        assert_eq!(
            boundary
                .generate(&graph, &session, DrawingRenderOptions::default())
                .unwrap()
                .hatches
                .len(),
            20
        );
    }
    let mut outside = hatched_slice("body");
    outside.views[0].origin.z.value = 40.0;
    assert!(
        outside
            .generate(&graph, &session, DrawingRenderOptions::default())
            .unwrap()
            .hatches
            .is_empty()
    );
    assert_eq!(session.shape_count().unwrap(), 0);
}
#[test]
fn hatching_rejects_invalid_patterns_projection_and_work_or_export_exhaustion() {
    let definition = family_with_datums();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let session = Session::new().unwrap();
    for pattern in [
        SectionHatching {
            spacing_mm: 0.0,
            ..Default::default()
        },
        SectionHatching {
            angle_radians: f64::NAN,
            ..Default::default()
        },
        SectionHatching {
            phase_mm: f64::INFINITY,
            ..Default::default()
        },
        SectionHatching {
            spacing_mm: 1e-12,
            ..Default::default()
        },
    ] {
        let mut page = hatched_slice("body");
        page.views[0].hatching = Some(pattern);
        assert!(
            page.generate(&graph, &session, DrawingRenderOptions::default())
                .is_err()
        );
        assert_eq!(session.shape_count().unwrap(), 0);
    }
    let mut page = hatched_slice("body");
    page.views[0].kind = DrawingViewKind::Orthographic;
    assert!(
        page.generate(&graph, &session, DrawingRenderOptions::default())
            .is_err()
    );
    page.views[0].kind = DrawingViewKind::Section {
        origin: VectorQuantity::lengths(5.0, 0.0, 0.0, LengthUnit::Millimeter),
        normal: VectorQuantity::scalars(1.0, 0.0, 0.0),
        keep_positive: true,
    };
    assert!(
        page.generate(&graph, &session, DrawingRenderOptions::default())
            .is_err()
    );
    page = hatched_slice("body");
    assert!(
        page.generate(
            &graph,
            &session,
            DrawingRenderOptions {
                curve_tolerance_mm: 0.01,
                exact_curves: false,
                curve_samples: 2,
                maximum_vertices: 25
            }
        )
        .is_err()
    );
    assert_eq!(session.shape_count().unwrap(), 0);
    let mut document = ModelDocument::from_graph(&graph);
    document.drawings.push(page);
    let mut old: serde_json::Value =
        serde_json::from_str(&document.to_json_pretty().unwrap()).unwrap();
    old["schema_version"] = 59.into();
    old["drawings"][0]["views"][0]
        .as_object_mut()
        .unwrap()
        .remove("hatching");
    assert!(
        ModelDocument::from_json(&old.to_string()).unwrap().drawings[0].views[0]
            .hatching
            .is_none()
    );
}
