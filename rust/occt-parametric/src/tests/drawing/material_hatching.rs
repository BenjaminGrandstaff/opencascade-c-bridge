//! Per-material hatch families.

use super::*;

fn hatch_materials(graph: &mut InstanceGraph<'_>) {
    for id in ["steel", "plastic"] {
        graph
            .add_material(Material {
                id: id.into(),
                name: id.into(),
                density_kg_per_cubic_meter: 1000.0,
            })
            .unwrap();
    }
}
fn hatch_family(angle: f64, spacing: f64, phase: f64) -> SectionHatching {
    SectionHatching {
        angle_radians: angle,
        spacing_mm: spacing,
        phase_mm: phase,
    }
}

#[test]
fn material_hatch_families_follow_inheritance_fallback_and_suppression_in_both_modes() {
    let definition = family_with_datums();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    hatch_materials(&mut graph);
    graph.assign_material("part", Some("steel")).unwrap();
    let mut page = hatched_slice("body");
    for (id, x, clone) in [
        ("steel-clone", 20.0, true),
        ("plastic-part", 40.0, true),
        ("unassigned", 60.0, false),
    ] {
        if clone {
            graph.add_clone(id, "part", HashMap::new(), "test").unwrap();
        } else {
            graph.add_base(id, HashMap::new(), "test").unwrap();
        }
        graph
            .set_placement(
                id,
                Placement::translated(VectorQuantity::lengths(x, 0.0, 0.0, LengthUnit::Millimeter)),
            )
            .unwrap();
        page.views[0].outputs.push(InstanceOutputRef {
            instance: id.into(),
            output: "body".into(),
        });
    }
    graph
        .assign_material("plastic-part", Some("plastic"))
        .unwrap();
    page.views[0].material_hatching.insert(
        "steel".into(),
        vec![hatch_family(0.0, 2.0, 0.5), hatch_family(0.0, 2.0, 1.5)],
    );
    page.views[0].material_hatching.insert(
        "plastic".into(),
        vec![
            hatch_family(std::f64::consts::FRAC_PI_2, 2.0, 0.5),
            hatch_family(0.0, 2.0, 0.5),
        ],
    );
    let session = Session::new().unwrap();
    for exact_curves in [false, true] {
        let options = DrawingRenderOptions {
            exact_curves,
            curve_samples: 2,
            ..DrawingRenderOptions::default()
        };
        let generated = page.generate(&graph, &session, options).unwrap();
        assert_eq!(generated.hatches.len(), 75);
        assert_eq!(generated.generated_variants, 1);
        let plastic: Vec<_> = generated
            .hatches
            .iter()
            .filter(|l| l.points_mm[0][0] > 40.0 && l.points_mm[0][0] < 50.0)
            .collect();
        assert_eq!(plastic.len(), 5);
        assert!(
            plastic
                .iter()
                .all(|l| (l.points_mm[0][0] - l.points_mm[1][0]).abs() < 1e-8)
        );
        assert!(generated.to_svg().contains("stroke-width=\"0.13\""));
        assert!(generated.to_dxf().contains("SECTION_HATCH"));
        assert_eq!(session.shape_count().unwrap(), 0);
    }
    let exact = DrawingRenderOptions {
        exact_curves: true,
        ..DrawingRenderOptions::default()
    };
    let slice = page.generate(&graph, &session, exact).unwrap();
    let mut section = page.clone();
    section.views[0].kind = DrawingViewKind::Section {
        origin: VectorQuantity::lengths(0.0, 0.0, 5.0, LengthUnit::Millimeter),
        normal: VectorQuantity::scalars(0.0, 0.0, 1.0),
        keep_positive: false,
    };
    section.views[0].origin.z.value = 100.0;
    assert_eq!(
        section.generate(&graph, &session, exact).unwrap().hatches,
        slice.hatches
    );
    let mut detail = page.clone();
    detail.views[0].detail = Some(DrawingDetail {
        minimum_mm: [1.0, 2.0],
        maximum_mm: [9.0, 18.0],
    });
    detail.views[0].paper_origin_mm = [10.0, 20.0];
    detail.views[0].scale = 2.0;
    let cropped = detail.generate(&graph, &session, exact).unwrap();
    assert_eq!(cropped.hatches.len(), 32);
    assert!(cropped.hatches.iter().all(
        |l| (l.points_mm[0][0] - 10.0).abs() < 1e-7 && (l.points_mm[1][0] - 26.0).abs() < 1e-7
    ));
    graph
        .set_placement(
            "steel-clone",
            Placement::translated(VectorQuantity::lengths(
                5.0,
                0.0,
                0.0,
                LengthUnit::Millimeter,
            )),
        )
        .unwrap();
    let union = page
        .generate(
            &graph,
            &session,
            DrawingRenderOptions {
                exact_curves: true,
                ..DrawingRenderOptions::default()
            },
        )
        .unwrap();
    assert_eq!(union.hatches.len(), 55);
    let steel: Vec<_> = union
        .hatches
        .iter()
        .filter(|l| l.points_mm[0][0].abs() < 1e-8)
        .collect();
    assert_eq!(steel.len(), 20);
    assert!(
        steel
            .iter()
            .all(|l| (l.points_mm[1][0] - 15.0).abs() < 1e-7)
    );
    graph
        .set_placement(
            "steel-clone",
            Placement::translated(VectorQuantity::lengths(
                20.0,
                0.0,
                0.0,
                LengthUnit::Millimeter,
            )),
        )
        .unwrap();
    graph.assign_material("part", Some("plastic")).unwrap();
    let options = DrawingRenderOptions {
        exact_curves: true,
        ..DrawingRenderOptions::default()
    };
    let changed = page.generate(&graph, &session, options).unwrap();
    assert_eq!(changed.hatches.len(), 65);
    assert_eq!(changed.curves.len(), 16);
    page.views[0]
        .material_hatching
        .insert("plastic".into(), vec![]);
    assert_eq!(
        page.generate(&graph, &session, options)
            .unwrap()
            .hatches
            .len(),
        20
    );
    page.views[0].hatching = None;
    let suppressed = page.generate(&graph, &session, options).unwrap();
    assert!(suppressed.hatches.is_empty());
    assert_eq!(suppressed.curves.len(), 16);
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn material_hatch_maps_migrate_persist_and_merge_independent_material_edits() {
    let definition = family_with_datums();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    hatch_materials(&mut graph);
    let mut base = ModelDocument::from_graph(&graph);
    base.drawings.push(hatched_slice("body"));
    let mut old: serde_json::Value = serde_json::from_str(&base.to_json_pretty().unwrap()).unwrap();
    old["schema_version"] = 67.into();
    assert!(
        old["drawings"][0]["views"][0]
            .get("material_hatching")
            .is_none()
    );
    let loaded = ModelDocument::from_json(&old.to_string()).unwrap();
    assert_eq!(loaded.schema_version, CURRENT_SCHEMA_VERSION);
    assert!(loaded.drawings[0].views[0].material_hatching.is_empty());
    let mut left = base.clone();
    let mut right = base.clone();
    left.drawings[0].views[0]
        .material_hatching
        .insert("steel".into(), vec![hatch_family(0.0, 2.0, 0.5)]);
    right.drawings[0].views[0]
        .material_hatching
        .insert("plastic".into(), vec![hatch_family(1.0, 3.0, 1.0)]);
    let DocumentMerge::Merged(merged) = base.three_way_merge(&left, &right).unwrap() else {
        panic!("independent maps should merge")
    };
    assert_eq!(merged.drawings[0].views[0].material_hatching.len(), 2);
    assert_eq!(
        ModelDocument::from_json(&merged.to_json_pretty().unwrap()).unwrap(),
        *merged
    );
    let base = *merged;
    let mut left = base.clone();
    let mut right = base.clone();
    left.drawings[0].views[0]
        .material_hatching
        .get_mut("steel")
        .unwrap()[0]
        .spacing_mm = 4.0;
    right.drawings[0].views[0]
        .material_hatching
        .get_mut("steel")
        .unwrap()[0]
        .spacing_mm = 5.0;
    assert!(matches!(
        base.three_way_merge(&left, &right).unwrap(),
        DocumentMerge::Conflicts(_)
    ));
    let mut left = base.clone();
    let mut right = base.clone();
    left.drawings[0].views[0].material_hatching.remove("steel");
    right.drawings[0].views[0]
        .material_hatching
        .get_mut("plastic")
        .unwrap()[0]
        .spacing_mm = 5.0;
    let DocumentMerge::Merged(merged) = base.three_way_merge(&left, &right).unwrap() else {
        panic!("independent deletion/edit should merge")
    };
    assert!(
        !merged.drawings[0].views[0]
            .material_hatching
            .contains_key("steel")
    );
    assert_eq!(
        merged.drawings[0].views[0].material_hatching["plastic"][0].spacing_mm,
        5.0
    );
}

#[test]
fn material_hatching_rejects_bad_references_patterns_and_cumulative_budgets() {
    let definition = family_with_datums();
    let mut graph = InstanceGraph::new(&definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    hatch_materials(&mut graph);
    graph.assign_material("part", Some("steel")).unwrap();
    let session = Session::new().unwrap();
    let options = DrawingRenderOptions {
        exact_curves: true,
        ..DrawingRenderOptions::default()
    };
    let mut page = hatched_slice("body");
    for (id, patterns) in [
        ("unknown", vec![hatch_family(0.0, 1.0, 0.0)]),
        ("steel", vec![hatch_family(0.0, 0.0, 0.0)]),
        ("steel", vec![SectionHatching::default(); 9]),
    ] {
        page.views[0].material_hatching = BTreeMap::from([(id.into(), patterns)]);
        assert!(page.generate(&graph, &session, options).is_err());
        assert_eq!(session.shape_count().unwrap(), 0);
    }
    page.views[0].material_hatching =
        BTreeMap::from([("steel".into(), vec![hatch_family(0.0, 4.0, 2.0)])]);
    assert!(
        page.generate(
            &graph,
            &session,
            DrawingRenderOptions {
                maximum_vertices: 27,
                ..options
            }
        )
        .is_ok()
    );
    page.views[0]
        .material_hatching
        .get_mut("steel")
        .unwrap()
        .push(hatch_family(0.0, 4.0, 3.0));
    assert!(
        page.generate(
            &graph,
            &session,
            DrawingRenderOptions {
                maximum_vertices: 27,
                ..options
            }
        )
        .is_err()
    );
    page.views[0].hatching = None;
    page.views[0].kind = DrawingViewKind::Orthographic;
    assert!(page.generate(&graph, &session, options).is_err());
    page.views[0].kind = DrawingViewKind::Slice;
    graph
        .assembly
        .material_assignments
        .insert("part".into(), "missing-assignment".into());
    assert!(
        page.generate(&graph, &session, options)
            .unwrap_err()
            .message
            .contains("unknown material")
    );
    assert_eq!(session.shape_count().unwrap(), 0);
}
