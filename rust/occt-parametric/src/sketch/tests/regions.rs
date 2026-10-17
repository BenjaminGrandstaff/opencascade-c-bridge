//! Several disjoint face regions in one sketch (schema 127).

use super::*;
use serde_json::{Value, json};
use std::f64::consts::PI;

fn mm(value: f64) -> Value {
    json!({"literal": {"value": value, "dimension": "length", "unit": "millimeter"}})
}

fn sketch(rest: Value) -> SketchDefinition {
    let mut value = json!({
        "id": "s",
        "origin": {"literal": {"x": {"value": 0, "dimension": "length", "unit": "millimeter"},
            "y": {"value": 0, "dimension": "length", "unit": "millimeter"},
            "z": {"value": 0, "dimension": "length", "unit": "millimeter"}}},
        "x_axis": {"literal": {"x": {"value": 1, "dimension": "scalar", "unit": null},
            "y": {"value": 0, "dimension": "scalar", "unit": null},
            "z": {"value": 0, "dimension": "scalar", "unit": null}}},
        "y_axis": {"literal": {"x": {"value": 0, "dimension": "scalar", "unit": null},
            "y": {"value": 1, "dimension": "scalar", "unit": null},
            "z": {"value": 0, "dimension": "scalar", "unit": null}}},
        "points": [],
        "lines": [],
    });
    for (key, item) in rest.as_object().unwrap() {
        value[key] = item.clone();
    }
    serde_json::from_value(value).unwrap()
}

fn p(id: &str, x: f64, y: f64) -> Value {
    json!({"id": id, "x": mm(x), "y": mm(y), "fixed": true})
}

/// A 20 × 10 rectangle, a disc of radius 4 with a 1.5 hole beside it, and a
/// slot region; `offset` moves the disc toward the rectangle.
fn three_regions(offset: f64) -> SketchDefinition {
    sketch(json!({
        "points": [p("a", 0., 0.), p("b", 20., 10.), p("c", 30. - offset, 5.), p("r", 34. - offset, 5.),
            p("hr", 31.5 - offset, 5.), p("s0", 45., 5.), p("s1", 55., 5.)],
        "circles": [{"id": "disc", "center": "c", "rim": "r"}, {"id": "bore", "center": "c", "rim": "hr"}],
        "shapes": [{"rectangle": {"id": "plate", "first": "a", "opposite": "b"}},
            {"slot": {"id": "slot", "start": "s0", "end": "s1", "radius": mm(2.)}}],
        "profile": ["plate"],
        "regions": [{"profile": ["disc"], "holes": [["bore"]]}, {"profile": ["slot"]}],
    }))
}

fn faces(sketch: &SketchDefinition) -> Result<(usize, f64), ModelError> {
    let session = Session::new().unwrap();
    let face = sketch.face(&session, &HashMap::new())?;
    assert!(session.is_valid(&face).unwrap());
    Ok((
        session.subshape_count(&face, ShapeType::Face).unwrap(),
        session.surface_area(&face).unwrap(),
    ))
}

#[test]
fn automatic_regions_split_intersections_and_select_named_seed_cells() {
    let s = sketch(json!({
        "points": [p("c", 0., 0.), p("r", 10., 0.), p("l", -12., 0.), p("h", 12., 0.), p("pick", 0., 5.)],
        "circles": [{"id": "disc", "center": "c", "rim": "r"}],
        "lines": [{"id": "divider", "start": "l", "end": "h"}],
        "region_discovery": {"seeds": ["pick", "pick"]}
    }));
    let (count, area) = faces(&s).unwrap();
    assert_eq!(count, 1);
    assert!((area - 50. * PI).abs() < 1e-7);
    let session = Session::new().unwrap();
    let solution = s.solve(&HashMap::new()).unwrap();
    let preview = s
        .preview_edited_profile(&session, &HashMap::new(), &solution, 32, true)
        .unwrap();
    assert!(!preview.is_empty());
    assert!(preview.iter().flatten().all(|p| p.y >= -1e-7));
    let mut all = s;
    all.region_discovery.as_mut().unwrap().seeds.clear();
    let (count, area) = faces(&all).unwrap();
    assert_eq!(count, 2);
    assert!((area - 100. * PI).abs() < 1e-7);
}

#[test]
fn region_composition_removes_internal_boundaries_and_fills_selected_holes() {
    let mut s = sketch(json!({
        "points": [p("c", 0., 0.), p("r", 10., 0.), p("b", 3., 0.), p("l", -12., 0.), p("h", 12., 0.)],
        "circles": [{"id": "disc", "center": "c", "rim": "r"}, {"id": "bore", "center": "c", "rim": "b"}],
        "lines": [{"id": "divider", "start": "l", "end": "h"}],
        "region_discovery": {"merge_adjacent": true}
    }));
    let (count, area) = faces(&s).unwrap();
    assert_eq!(count, 1);
    assert!((area - 100. * PI).abs() < 1e-7);
    let session = Session::new().unwrap();
    let face = s.face(&session, &HashMap::new()).unwrap();
    assert_eq!(session.subshape_count(&face, ShapeType::Wire).unwrap(), 1);
    let solution = s.solve(&HashMap::new()).unwrap();
    let preview = s
        .preview_edited_profile(&session, &HashMap::new(), &solution, 32, true)
        .unwrap();
    // Both the divider and the filled inner circle disappear from the boundary.
    for p in preview.iter().flatten() {
        assert!((p.x.hypot(p.y) - 10.).abs() < 1e-6);
    }
    s.region_discovery.as_mut().unwrap().merge_adjacent = false;
    assert_eq!(faces(&s).unwrap().0, 4);
}

#[test]
fn region_composition_preserves_holes_and_disconnected_solids() {
    let s = sketch(json!({
        "points": [p("c", 0., 0.), p("r", 10., 0.), p("b", 3., 0.), p("l", -12., 0.), p("h", 12., 0.),
            p("top", 5., 2.), p("bottom", 5., -2.), p("ic", 30., 0.), p("ir", 34., 0.)],
        "circles": [{"id": "disc", "center": "c", "rim": "r"}, {"id": "bore", "center": "c", "rim": "b"},
            {"id": "island", "center": "ic", "rim": "ir"}],
        "lines": [{"id": "divider", "start": "l", "end": "h"}],
        "region_discovery": {"seeds": ["top", "bottom", "ic"], "merge_adjacent": true}
    }));
    let session = Session::new().unwrap();
    let regions = s.face(&session, &HashMap::new()).unwrap();
    let faces = session.subshapes(&regions, ShapeType::Face).unwrap();
    assert_eq!(faces.len(), 2);
    assert_eq!(
        session.subshape_count(&regions, ShapeType::Wire).unwrap(),
        3
    );
    assert!((session.surface_area(&regions).unwrap() - 107. * PI).abs() < 1e-7);
    let solids: Vec<_> = faces
        .iter()
        .map(|f| {
            session
                .create_prism_from_face(f, Vec3::new(0., 0., 2.))
                .unwrap()
        })
        .collect();
    let body = session
        .create_compound(&solids.iter().collect::<Vec<_>>())
        .unwrap();
    assert!(session.is_valid(&body).unwrap());
    assert_eq!(session.subshape_count(&body, ShapeType::Solid).unwrap(), 2);
    assert!((session.volume(&body).unwrap() - 214. * PI).abs() < 1e-7);
}

#[test]
fn region_composition_keeps_point_contacts_separate() {
    let s = sketch(json!({
        "points": [p("c0", 0., 0.), p("r0", 0., 5.), p("c1", 10., 0.), p("r1", 10., 5.)],
        "circles": [{"id": "left", "center": "c0", "rim": "r0"}, {"id": "right", "center": "c1", "rim": "r1"}],
        "region_discovery": {"merge_adjacent": true}
    }));
    let (count, area) = faces(&s).unwrap();
    assert_eq!(count, 2);
    assert!((area - 50. * PI).abs() < 1e-7);
}

#[test]
fn region_composition_unions_overlapping_circles_without_double_counting() {
    let s = sketch(json!({
        "points": [p("c0", 0., 0.), p("r0", 0., 10.), p("c1", 10., 0.), p("r1", 10., 10.)],
        "circles": [{"id": "left", "center": "c0", "rim": "r0"}, {"id": "right", "center": "c1", "rim": "r1"}],
        "region_discovery": {"merge_adjacent": true}
    }));
    let (count, area) = faces(&s).unwrap();
    assert_eq!(count, 1);
    let lens_area = 200. * PI / 3. - 50. * 3_f64.sqrt();
    assert!((area - (200. * PI - lens_area)).abs() < 1e-7);
}

#[test]
fn named_splits_build_exact_profiles_and_extensions_stop_at_crossings() {
    let mut s = sketch(json!({
        "points":[p("a",0.,0.),p("b",10.,0.),p("c",10.,10.),p("d",0.,10.),p("cut",4.,0.),p("v0",6.,-2.),p("v1",6.,12.)],
        "lines":[{"id":"bottom","start":"a","end":"b"},{"id":"right","start":"b","end":"c"},
            {"id":"top","start":"c","end":"d"},{"id":"left","start":"d","end":"a"},{"id":"stop","start":"v0","end":"v1"}],
        "curve_operations":[{"split":{"entity":"bottom","at":{"point":{"point":"cut"}},"before":"first","after":"second"}}],
        "profile":["first","second","right","top","left"]
    }));
    assert!((faces(&s).unwrap().1 - 100.).abs() < 1e-7);
    assert_eq!(s.solve(&HashMap::new()).unwrap().free_degrees, 0);
    s.profile = vec!["first".into()];
    s.curve_operations.push(
        serde_json::from_value(
            json!({"extend_to":{"entity":"first","end":"end","target":"stop","limit":mm(3.)}}),
        )
        .unwrap(),
    );
    let session = Session::new().unwrap();
    let wire = s.open_wire(&session, &HashMap::new(), None).unwrap();
    let (a, b, _, _) = session.curve_end_data(&wire).unwrap();
    assert!((a.x - 0.).abs() < 1e-7 && (b.x - 6.).abs() < 1e-7);
    // Authored endpoints and constraints stay unchanged.
    assert_eq!(s.solve(&HashMap::new()).unwrap().points["cut"].x, 4.);
    s.curve_operations[1] = serde_json::from_value(
        json!({"extend_to":{"entity":"first","end":"end","target":"stop","limit":mm(1.)}}),
    )
    .unwrap();
    assert!(
        s.open_wire(&session, &HashMap::new(), None)
            .unwrap_err()
            .message
            .contains("no crossing")
    );
}

#[test]
fn named_circle_and_spline_pieces_preserve_native_geometry() {
    let circle = sketch(json!({
        "points":[p("c",0.,0.),p("r",10.,0.),p("v0",0.,-12.),p("v1",0.,12.),p("hint",0.,10.)],
        "circles":[{"id":"disc","center":"c","rim":"r"}],
        "lines":[{"id":"divider","start":"v0","end":"v1"}],
        "curve_operations":[{"split":{"entity":"disc","at":{"crossing":{"curve":"divider","near":"hint"}},"before":"arc-a","after":"arc-b"}}],
        "profile":["arc-a","arc-b"]
    }));
    assert!((faces(&circle).unwrap().1 - 100. * PI).abs() < 1e-7);
    let mut spline = sketch(json!({
        "points":[p("a",0.,0.),p("b",5.,4.),p("c",10.,0.)],
        "splines":[{"id":"arch","points":["a","b","c"]}],
        "lines":[{"id":"base","start":"c","end":"a"}],
        "curve_operations":[{"split":{"entity":"arch","at":{"point":{"point":"b"}},"before":"arch-a","after":"arch-b"}}],
        "profile":["arch-a","arch-b","base"]
    }));
    let (_, split_area) = faces(&spline).unwrap();
    spline.curve_operations.clear();
    spline.profile = vec!["arch".into(), "base".into()];
    assert!((faces(&spline).unwrap().1 - split_area).abs() < 1e-7);
}

#[test]
fn region_edits_fillet_an_additional_region_and_offset_discovered_material() {
    let mut s = sketch(json!({
        "points":[p("a",0.,0.),p("b",20.,10.),p("c",30.,0.),p("d",40.,10.),p("pick",35.,5.)],
        "shapes":[{"rectangle":{"id":"first","first":"a","opposite":"b"}},
            {"rectangle":{"id":"second","first":"c","opposite":"d"}}],
        "profile":["first"],"regions":[{"profile":["second"]}],
        "region_edits":[{"seed":"pick","operations":[{"fillet":{"point":"c","radius":mm(2.)}}]}]
    }));
    assert!((faces(&s).unwrap().1 - (296. + PI)).abs() < 1e-7);
    s.profile.clear();
    s.regions.clear();
    s.region_discovery = Some(serde_json::from_value(json!({"merge_adjacent":true})).unwrap());
    s.region_edits[0]
        .operations
        .push(serde_json::from_value(json!({"offset":{"distance":mm(1.)}})).unwrap());
    assert!((faces(&s).unwrap().1 - (332. + 3. * PI)).abs() < 1e-6);
}

#[test]
fn region_offsets_expand_material_and_contract_holes() {
    let s = sketch(json!({
        "points":[p("c",0.,0.),p("r",10.,0.),p("b",3.,0.),p("pick",0.,6.)],
        "circles":[{"id":"disc","center":"c","rim":"r"},{"id":"bore","center":"c","rim":"b"}],
        "region_discovery":{"seeds":["pick"],"merge_adjacent":true},
        "region_edits":[{"seed":"pick","operations":[{"offset":{"distance":mm(1.)}}]}]
    }));
    let (count, area) = faces(&s).unwrap();
    assert_eq!(count, 1);
    assert!((area - 117. * PI).abs() < 1e-7);
    let session = Session::new().unwrap();
    let face = s.face(&session, &HashMap::new()).unwrap();
    assert_eq!(session.subshape_count(&face, ShapeType::Wire).unwrap(), 2);
}

#[test]
fn region_offsets_and_corner_edits_handle_polygonal_holes() {
    let mut s = sketch(json!({
        "points":[p("a",0.,0.),p("b",40.,30.),p("h0",10.,10.),p("h1",20.,18.),p("pick",5.,5.)],
        "shapes":[{"rectangle":{"id":"outer","first":"a","opposite":"b"}},
            {"rectangle":{"id":"bore","first":"h0","opposite":"h1"}}],
        "profile":["outer"],"holes":[["bore"]],
        "region_edits":[{"seed":"pick","operations":[{"offset":{"distance":mm(1.)}}]}]
    }));
    assert!((faces(&s).unwrap().1 - (1292. + PI)).abs() < 1e-6);
    s.region_edits[0].operations =
        serde_json::from_value(json!([{"fillet":{"point":"h0","radius":mm(1.)}}])).unwrap();
    assert!((faces(&s).unwrap().1 - (1121. - PI / 4.)).abs() < 1e-6);
}

#[test]
fn extend_to_both_ends_uses_the_nearest_crossing_or_an_explicit_hint() {
    let mut s = sketch(json!({
        "points":[p("a",0.,0.),p("b",4.,0.),p("c",10.,0.),p("r",12.,0.),p("hint",12.,0.)],
        "lines":[{"id":"line","start":"a","end":"b"}],
        "circles":[{"id":"stop","center":"c","rim":"r"}],"profile":["line"],
        "curve_operations":[{"extend_to":{"entity":"line","end":"end","target":"stop","limit":mm(10.)}}]
    }));
    let session = Session::new().unwrap();
    let end = |s: &SketchDefinition| {
        session
            .curve_end_data(&s.open_wire(&session, &HashMap::new(), None).unwrap())
            .unwrap()
    };
    assert!((end(&s).1.x - 8.).abs() < 1e-7);
    if let SketchCurveOperation::ExtendTo { near, .. } = &mut s.curve_operations[0] {
        *near = Some("hint".into());
    }
    assert!((end(&s).1.x - 12.).abs() < 1e-7);
    s.points.iter_mut().find(|p| p.id == "c").unwrap().x = serde_json::from_value(mm(-7.)).unwrap();
    s.points.iter_mut().find(|p| p.id == "r").unwrap().x = serde_json::from_value(mm(-5.)).unwrap();
    if let SketchCurveOperation::ExtendTo { end, near, .. } = &mut s.curve_operations[0] {
        *end = SketchCurveEnd::Start;
        *near = None;
    }
    assert!((end(&s).0.x + 5.).abs() < 1e-7);
}

#[test]
fn geometry_edit_errors_reject_lost_corners_bad_names_and_region_seeds() {
    let mut s = sketch(json!({
        "points":[p("a",0.,0.),p("b",10.,10.),p("pick",5.,5.)],
        "shapes":[{"rectangle":{"id":"box","first":"a","opposite":"b"}}],"profile":["box"],
        "region_edits":[{"seed":"pick","operations":[{"chamfer":{"point":"pick","incoming":mm(1.),"outgoing":mm(1.)}}]}]
    }));
    assert!(
        faces(&s)
            .unwrap_err()
            .message
            .contains("not a remaining boundary vertex")
    );
    s.region_edits[0].seed = "a".into();
    assert!(
        faces(&s)
            .unwrap_err()
            .message
            .contains("lies on a boundary")
    );
    s.region_edits.clear();
    s.curve_operations=serde_json::from_value(json!([{"split":{"entity":"missing","at":{"point":{"point":"a"}},"before":"one","after":"two"}}])).unwrap();
    assert!(
        faces(&s)
            .unwrap_err()
            .message
            .contains("active known curve")
    );
}

#[test]
fn automatic_regions_handle_nested_loops_and_overlapping_circles() {
    let nested = sketch(json!({
        "points": [p("c", 0., 0.), p("r", 10., 0.), p("b", 3., 0.), p("pick", 5., 0.)],
        "circles": [{"id": "disc", "center": "c", "rim": "r"}, {"id": "hole", "center": "c", "rim": "b"}],
        "region_discovery": {"seeds": ["pick"]}
    }));
    let (count, area) = faces(&nested).unwrap();
    assert_eq!(count, 1);
    assert!((area - 91. * PI).abs() < 1e-7);
    let lens = sketch(json!({
        "points": [p("c0", 0., 0.), p("r0", 0., 10.), p("c1", 10., 0.), p("r1", 10., 10.), p("pick", 5., 0.)],
        "circles": [{"id": "left", "center": "c0", "rim": "r0"}, {"id": "right", "center": "c1", "rim": "r1"}],
        "region_discovery": {"seeds": ["pick"]}
    }));
    let (_, area) = faces(&lens).unwrap();
    assert!((area - (200. * PI / 3. - 50. * 3_f64.sqrt())).abs() < 1e-7);
}

#[test]
fn automatic_regions_expand_shapes_and_use_native_ellipse_and_spline_edges() {
    let rectangles = sketch(json!({
        "points": [p("a", 0., 0.), p("b", 20., 10.), p("c", 30., 0.), p("d", 40., 10.)],
        "shapes": [{"rectangle": {"id": "first", "first": "a", "opposite": "b"}},
            {"rectangle": {"id": "second", "first": "c", "opposite": "d"}}],
        "region_discovery": {"entities": ["first", "second"]}
    }));
    let (count, area) = faces(&rectangles).unwrap();
    assert_eq!(count, 2);
    assert!((area - 300.).abs() < 1e-7);
    let ellipse = sketch(json!({
        "points": [p("c", 0., 0.), p("a", 10., 0.), p("b", 0., 4.), p("l", -12., 0.), p("r", 12., 0.), p("pick", 0., 2.)],
        "ellipses": [{"id": "e", "center": "c", "major": "a", "minor": "b"}],
        "lines": [{"id": "line", "start": "l", "end": "r"}],
        "region_discovery": {"seeds": ["pick"]}
    }));
    assert!((faces(&ellipse).unwrap().1 - 20. * PI).abs() < 1e-7);
    let spline = sketch(json!({
        "points": [p("a", 0., 0.), p("b", 5., 4.), p("c", 10., 0.), p("l", -2., 0.), p("r", 12., 0.)],
        "splines": [{"id": "arch", "points": ["a", "b", "c"]}],
        "lines": [{"id": "line", "start": "l", "end": "r"}],
        "region_discovery": {}
    }));
    let (count, area) = faces(&spline).unwrap();
    assert_eq!(count, 1);
    assert!(area > 20. && area < 40.);
}

#[test]
fn automatic_region_errors_reject_ambiguous_requests_and_bad_seeds() {
    let base = |seed: (f64, f64)| {
        sketch(json!({
            "points": [p("c", 0., 0.), p("r", 10., 0.), p("pick", seed.0, seed.1)],
            "circles": [{"id": "disc", "center": "c", "rim": "r"}],
            "region_discovery": {"seeds": ["pick"]}
        }))
    };
    assert!(
        faces(&base((10., 0.)))
            .unwrap_err()
            .message
            .contains("boundary")
    );
    assert!(
        faces(&base((20., 0.)))
            .unwrap_err()
            .message
            .contains("outside")
    );
    let mut s = base((0., 0.));
    s.profile = vec!["disc".into()];
    assert!(
        faces(&s)
            .unwrap_err()
            .message
            .contains("mutually exclusive")
    );
    s.profile.clear();
    s.region_discovery.as_mut().unwrap().entities = vec!["missing".into()];
    assert!(
        faces(&s)
            .unwrap_err()
            .message
            .contains("unknown or repeated")
    );
    s.region_discovery.as_mut().unwrap().entities.clear();
    s.region_discovery.as_mut().unwrap().seeds = vec!["missing".into()];
    assert!(faces(&s).unwrap_err().message.contains("unknown seed"));
    s.region_discovery.as_mut().unwrap().seeds.clear();
    s.region_discovery.as_mut().unwrap().seeds = vec!["pick".into(); 1001];
    assert!(faces(&s).unwrap_err().message.contains("exceeds 1000"));
    s.region_discovery.as_mut().unwrap().seeds.clear();
    let session = Session::new().unwrap();
    assert!(
        s.wire(&session, &HashMap::new(), None)
            .unwrap_err()
            .message
            .contains("only to face outputs")
    );
    let open = sketch(json!({"points": [p("a", 0., 0.), p("b", 10., 0.)],
        "lines": [{"id": "line", "start": "a", "end": "b"}], "region_discovery": {}}));
    assert!(
        faces(&open)
            .unwrap_err()
            .message
            .contains("no bounded cells")
    );
}

#[test]
fn disjoint_regions_build_one_compound_face_with_their_own_holes() {
    let (count, area) = faces(&three_regions(0.)).unwrap();
    assert_eq!(count, 3);
    let expected = 200. + PI * (16. - 2.25) + (2. * 2. * 10. + PI * 4.);
    assert!((area - expected).abs() < 1e-9, "{area} vs {expected}");
}

#[test]
fn overlapping_regions_and_misused_loops_are_rejected() {
    // The disc (radius 4 at x = 30 − 7) overlaps the rectangle ending at 20.
    let error = faces(&three_regions(7.)).unwrap_err();
    assert!(
        error.message.contains("profile and region 0 overlap"),
        "{}",
        error.message
    );
    // Touching at a single point is not an overlap.
    let (count, _) = faces(&three_regions(6.)).unwrap();
    assert_eq!(count, 3);
    // Wire outputs reject regions.
    let session = Session::new().unwrap();
    let error = three_regions(0.)
        .wire(&session, &HashMap::new(), None)
        .unwrap_err();
    assert!(error.message.contains("regions apply only to face outputs"));
    // An entity may belong to one loop only.
    let mut reused = three_regions(0.);
    reused.regions[1].profile = vec!["disc".into()];
    let error = reused.solve(&HashMap::new()).unwrap_err();
    assert!(
        error.message.contains("reuse entities"),
        "{}",
        error.message
    );
    let mut empty = three_regions(0.);
    empty.regions[0].profile.clear();
    let error = empty.solve(&HashMap::new()).unwrap_err();
    assert!(error.message.contains("nonempty"), "{}", error.message);
}

#[test]
fn profile_operations_apply_to_the_profile_beside_regions() {
    let mut s = three_regions(0.);
    s.profile_operations =
        serde_json::from_value(json!([{"fillet": {"point": "a", "radius": mm(2.)}}])).unwrap();
    let (count, area) = faces(&s).unwrap();
    assert_eq!(count, 3);
    // One rounded corner removes (1 − π/4) r² from the rectangle.
    let expected = 200. - (1. - PI / 4.) * 4. + PI * (16. - 2.25) + (40. + PI * 4.);
    assert!((area - expected).abs() < 1e-9, "{area} vs {expected}");
}

#[test]
fn curve_edits_with_crossings_work_on_sketch_planes_other_than_xy() {
    // A crossing split and an extend-to, on XY and on two other frames.
    let base = json!({
        "points":[p("a",0.,0.),p("b",10.,0.),p("c",10.,10.),p("d",0.,10.),p("v0",6.,-2.),p("v1",6.,12.),
            p("hint",6.,0.),p("m0",2.,5.),p("m1",4.,5.)],
        "lines":[{"id":"bottom","start":"a","end":"b"},{"id":"right","start":"b","end":"c"},
            {"id":"top","start":"c","end":"d"},{"id":"left","start":"d","end":"a"},{"id":"stop","start":"v0","end":"v1"},
            {"id":"spur","start":"m0","end":"m1"}],
        "curve_operations":[
            {"split":{"entity":"bottom","at":{"crossing":{"curve":"stop","near":"hint"}},"before":"first","after":"second"}},
            {"extend_to":{"entity":"spur","end":"end","target":"stop","limit":mm(5.)}}],
        "profile":["first","second","right","top","left"]
    });
    let vector = |x: f64, y: f64, z: f64, dimension: &str, unit: Value| {
        json!({"literal": {"x": {"value": x, "dimension": dimension, "unit": unit},
            "y": {"value": y, "dimension": dimension, "unit": unit},
            "z": {"value": z, "dimension": dimension, "unit": unit}}})
    };
    let s = 0.5f64.sqrt();
    for (origin, x, y) in [
        ([0., 0., 0.], [1., 0., 0.], [0., 1., 0.]),
        ([-1., 2., 3.], [0., 1., 0.], [0., 0., 1.]),
        ([4., -2., 7.], [s, 0., -s], [0.5, s, 0.5]),
    ] {
        let mut rest = base.clone();
        rest["origin"] = vector(
            origin[0],
            origin[1],
            origin[2],
            "length",
            json!("millimeter"),
        );
        rest["x_axis"] = vector(x[0], x[1], x[2], "scalar", Value::Null);
        rest["y_axis"] = vector(y[0], y[1], y[2], "scalar", Value::Null);
        let mut sketch = sketch(rest);
        let (count, area) = faces(&sketch).unwrap();
        assert_eq!((count, (area - 100.).abs() < 1e-7), (1, true), "{x:?}");
        // The spur, extended alone, ends where the stop line crosses it.
        sketch.profile = vec!["spur".into()];
        let session = Session::new().unwrap();
        let wire = sketch.open_wire(&session, &HashMap::new(), None).unwrap();
        let (_, end, _, _) = session.curve_end_data(&wire).unwrap();
        let expected = [0, 1, 2].map(|i| origin[i] + 6. * x[i] + 5. * y[i]);
        assert!(
            (end.x - expected[0]).abs() < 1e-7
                && (end.y - expected[1]).abs() < 1e-7
                && (end.z - expected[2]).abs() < 1e-7,
            "{end:?} {expected:?}"
        );
    }
}
