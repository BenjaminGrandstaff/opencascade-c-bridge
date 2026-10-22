use super::*;

/// A DXF file from group-code pairs: an optional `$INSUNITS`, BLOCKS and
/// ENTITIES.
fn file(insunits: Option<i32>, blocks: &[&str], entities: &[&str]) -> String {
    let mut text = String::new();
    if let Some(units) = insunits {
        text += &format!("0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n{units}\n0\nENDSEC\n");
    }
    if !blocks.is_empty() {
        text += "0\nSECTION\n2\nBLOCKS\n";
        for block in blocks {
            text += block;
        }
        text += "0\nENDSEC\n";
    }
    text += "0\nSECTION\n2\nENTITIES\n";
    for entity in entities {
        text += entity;
    }
    text + "0\nENDSEC\n0\nEOF\n"
}

fn line(layer: &str, a: [f64; 2], b: [f64; 2]) -> String {
    format!(
        "0\nLINE\n8\n{layer}\n10\n{}\n20\n{}\n30\n0\n11\n{}\n21\n{}\n31\n0\n",
        a[0], a[1], b[0], b[1]
    )
}
fn circle(layer: &str, c: [f64; 2], r: f64) -> String {
    format!(
        "0\nCIRCLE\n8\n{layer}\n10\n{}\n20\n{}\n30\n0\n40\n{r}\n",
        c[0], c[1]
    )
}
fn square(layer: &str, at: [f64; 2], size: f64) -> Vec<String> {
    let [x, y] = at;
    vec![
        line(layer, [x, y], [x + size, y]),
        line(layer, [x + size, y], [x + size, y + size]),
        line(layer, [x + size, y + size], [x, y + size]),
        line(layer, [x, y + size], [x, y]),
    ]
}
fn lwpolyline(layer: &str, closed: bool, vertices: &[(f64, f64, f64)]) -> String {
    let mut text = format!(
        "0\nLWPOLYLINE\n8\n{layer}\n90\n{}\n70\n{}\n",
        vertices.len(),
        u8::from(closed)
    );
    for (x, y, bulge) in vertices {
        text += &format!("10\n{x}\n20\n{y}\n");
        if *bulge != 0.0 {
            text += &format!("42\n{bulge}\n");
        }
    }
    text
}
fn refs(entities: &[String]) -> Vec<&str> {
    entities.iter().map(String::as_str).collect()
}

fn import(text: &str) -> DxfImport {
    import_dxf(text, &DxfImportOptions::default()).unwrap()
}

fn point(sketch: &SketchDefinition, id: &str) -> [f64; 2] {
    let p = sketch.points.iter().find(|p| p.id == id).unwrap();
    let value = |e: &crate::ScalarExpr| match e {
        crate::ScalarExpr::Literal(q) => q.value,
        _ => panic!("literal expected"),
    };
    [value(&p.x), value(&p.y)]
}

/// Each loop's curves run end to start, as the sketch profile requires.
fn assert_directed(sketch: &SketchDefinition) {
    let ends = |id: &str| -> Option<(String, String)> {
        if let Some(l) = sketch.lines.iter().find(|l| l.id == id) {
            return Some((l.start.clone(), l.end.clone()));
        }
        if let Some(a) = sketch.arcs.iter().find(|a| a.id == id) {
            return Some((a.start.clone(), a.end.clone()));
        }
        if let Some(e) = sketch.ellipses.iter().find(|e| e.id == id) {
            return e.arc_ends().map(|(a, b)| (a.to_string(), b.to_string()));
        }
        let s = sketch.splines.iter().find(|s| s.id == id)?;
        Some((s.points[0].clone(), s.points.last().unwrap().clone()))
    };
    let mut loops = vec![sketch.profile.clone()];
    loops.extend(sketch.holes.iter().cloned());
    for region in &sketch.regions {
        loops.push(region.profile.clone());
        loops.extend(region.holes.iter().cloned());
    }
    for curves in loops.iter().filter(|l| l.len() > 1) {
        for (i, id) in curves.iter().enumerate() {
            let next = &curves[(i + 1) % curves.len()];
            assert_eq!(ends(id).unwrap().1, ends(next).unwrap().0, "{curves:?}");
        }
    }
}

#[test]
fn lines_and_circles_become_a_profile_with_a_hole() {
    let mut entities = square("0", [0.0, 0.0], 40.0);
    entities.push(circle("0", [20.0, 20.0], 5.0));
    let result = import(&file(None, &[], &refs(&entities)));
    let (sketch, report) = (&result.sketch, &result.report);
    assert_eq!(sketch.lines.len(), 4);
    assert_eq!(sketch.circles.len(), 1);
    assert_eq!(sketch.profile.len(), 4);
    assert_eq!(sketch.holes, [vec!["c1".to_string()]]);
    // Four corners shared by two lines each, plus the circle's center and rim.
    assert_eq!(report.points, 6);
    assert_eq!(report.merged_endpoints, 4);
    assert_eq!(
        (
            report.loops,
            report.holes,
            report.regions,
            report.open_curves
        ),
        (2, 1, 0, 0)
    );
    assert_eq!(report.unit, "millimeter");
    assert_eq!(report.unit_source, "no $INSUNITS; millimeters assumed");
    assert_eq!(report.bounds_mm, Some([[0.0, 0.0], [40.0, 40.0]]));
    assert_eq!(report.imported["LINE"], 4);
    let rim = point(sketch, &sketch.circles[0].rim);
    assert_eq!(rim, [25.0, 20.0]);
}

#[test]
fn polyline_bulges_become_arcs_and_units_scale_to_millimeters() {
    // A 20 × 10 slot in centimeters: straight sides, semicircular ends
    // (bulge 1).
    let slot = lwpolyline(
        "0",
        true,
        &[
            (0.0, 0.0, 0.0),
            (2.0, 0.0, 1.0),
            (2.0, 1.0, 0.0),
            (0.0, 1.0, 1.0),
        ],
    );
    let result = import(&file(Some(5), &[], &[&slot]));
    let (sketch, report) = (&result.sketch, &result.report);
    assert_eq!(report.unit, "centimeter");
    assert_eq!(report.unit_source, "$INSUNITS");
    assert_eq!((sketch.lines.len(), sketch.arcs.len()), (2, 2));
    assert_eq!(sketch.profile.len(), 4);
    assert_directed(sketch);
    for arc in &sketch.arcs {
        let (c, a, b) = (
            point(sketch, &arc.center),
            point(sketch, &arc.start),
            point(sketch, &arc.end),
        );
        let r = |p: [f64; 2]| (p[0] - c[0]).hypot(p[1] - c[1]);
        assert!((r(a) - 5.0).abs() < 1e-9 && (r(b) - 5.0).abs() < 1e-9);
        assert!(!arc.clockwise);
    }
    let [lo, hi] = report.bounds_mm.unwrap();
    assert!(
        (lo[0] + 5.0).abs() < 1e-9 && (hi[0] - 25.0).abs() < 1e-9,
        "{lo:?} {hi:?}"
    );
    // An explicit unit overrides the header.
    let options = DxfImportOptions {
        unit: Some(DxfUnit::Inch),
        ..Default::default()
    };
    let inch = import_dxf(&file(Some(5), &[], &[&slot]), &options).unwrap();
    assert_eq!(inch.report.unit_source, "option");
    assert!((inch.report.bounds_mm.unwrap()[1][0] - 2.5 * 25.4).abs() < 1e-9);
}

#[test]
fn layers_filter_and_blocks_insert_with_rotation_and_scale() {
    // A 10 mm square block on layer 0, inserted on layer CUT at (100, 0),
    // turned 90° and doubled; a note line on layer NOTES.
    let mut block = String::from("0\nBLOCK\n2\nSQ\n8\n0\n10\n0\n20\n0\n");
    for entity in square("0", [0.0, 0.0], 10.0) {
        block += &entity;
    }
    block += "0\nENDBLK\n";
    let insert = "0\nINSERT\n8\nCUT\n2\nSQ\n10\n100\n20\n0\n41\n2\n42\n2\n50\n90\n";
    let note = line("NOTES", [0.0, 50.0], [80.0, 50.0]);
    let text = file(
        None,
        &[&block],
        &[insert, &note, "0\nTEXT\n8\nNOTES\n1\nhello\n"],
    );
    let options = DxfImportOptions {
        layers: vec!["CUT".into()],
        ..Default::default()
    };
    let result = import_dxf(&text, &options).unwrap();
    let report = &result.report;
    assert_eq!(report.layers, ["CUT"]);
    assert_eq!(report.curves, 4);
    assert_eq!(report.profile_curves, 4);
    let [lo, hi] = report.bounds_mm.unwrap();
    for (got, want) in [lo, hi].iter().flatten().zip([80.0, 0.0, 100.0, 20.0]) {
        assert!((got - want).abs() < 1e-9, "{lo:?} {hi:?}");
    }
    // Without the filter the note is an open curve and the text is skipped.
    let all = import(&text);
    assert_eq!(all.report.open_curves, 1);
    assert_eq!(all.report.skipped["TEXT: not geometry for a sketch"], 1);
    assert_eq!(all.report.layers, ["CUT", "NOTES"]);
}

#[test]
fn nested_loops_alternate_between_material_and_holes() {
    // A 100 square with a 60 square hole, a 20 square island inside it, and
    // a separate 10 square far away.
    let mut entities = square("0", [0.0, 0.0], 100.0);
    entities.extend(square("0", [20.0, 20.0], 60.0));
    entities.extend(square("0", [40.0, 40.0], 20.0));
    entities.extend(square("0", [200.0, 0.0], 10.0));
    let result = import(&file(None, &[], &refs(&entities)));
    let (sketch, report) = (&result.sketch, &result.report);
    assert_eq!((report.loops, report.holes, report.regions), (4, 1, 2));
    assert_directed(sketch);
    assert_eq!(sketch.profile, ["l1", "l2", "l3", "l4"]);
    assert_eq!(sketch.holes, [vec!["l5", "l6", "l7", "l8"]]);
    let regions: Vec<_> = sketch
        .regions
        .iter()
        .map(|r| (r.profile[0].clone(), r.holes.len()))
        .collect();
    assert!(
        regions.contains(&("l9".into(), 0)) && regions.contains(&("l13".into(), 0)),
        "{regions:?}"
    );
}

#[test]
fn splines_ellipses_and_arcs_keep_their_exact_definitions() {
    // A clamped cubic spline from (0,0) to (30,0) over an arc closing it
    // underneath; a full ellipse and a half ellipse beside it.
    let spline = "0\nSPLINE\n8\n0\n70\n8\n71\n3\n72\n8\n73\n4\n\
40\n0\n40\n0\n40\n0\n40\n0\n40\n1\n40\n1\n40\n1\n40\n1\n\
10\n0\n20\n0\n30\n0\n10\n10\n20\n10\n30\n0\n10\n20\n20\n10\n30\n0\n10\n30\n20\n0\n30\n0\n";
    let arc = "0\nARC\n8\n0\n10\n15\n20\n0\n30\n0\n40\n15\n50\n180\n51\n360\n";
    let ellipse = "0\nELLIPSE\n8\n0\n10\n100\n20\n0\n30\n0\n11\n20\n21\n0\n31\n0\n40\n0.5\n41\n0\n42\n6.283185307179586\n";
    let half = "0\nELLIPSE\n8\n0\n10\n200\n20\n0\n30\n0\n11\n0\n21\n10\n31\n0\n40\n0.5\n41\n0\n42\n3.141592653589793\n";
    let result = import(&file(None, &[], &[spline, arc, ellipse, half]));
    let (sketch, report) = (&result.sketch, &result.report);
    assert_eq!(sketch.splines.len(), 1);
    let basis = sketch.splines[0].basis.as_ref().unwrap();
    assert_eq!(
        (
            basis.degree,
            basis.knots.clone(),
            basis.multiplicities.clone()
        ),
        (3, vec![0.0, 1.0], vec![4, 4])
    );
    assert_eq!(sketch.splines[0].points.len(), 4);
    // The arc lies under the spline and closes the loop: it runs clockwise
    // from the spline's end back to its start.
    let arc = &sketch.arcs[0];
    assert!(arc.clockwise);
    assert!((point(sketch, &arc.start)[0] - 30.0).abs() < 1e-9);
    assert_eq!(point(sketch, &arc.end), [0.0, 0.0]);
    assert_eq!(arc.end, sketch.splines[0].points[0]);
    assert_directed(sketch);
    // Spline + arc, and the full ellipse, are loops; the half ellipse is open.
    assert_eq!((report.loops, report.open_curves), (2, 1));
    let full = &sketch.ellipses[0];
    assert!(full.arc_ends().is_none());
    assert_eq!(point(sketch, &full.major), [120.0, 0.0]);
    assert_eq!(point(sketch, &full.minor), [100.0, 10.0]);
    let open = &sketch.ellipses[1];
    let (start, end) = open.arc_ends().unwrap();
    assert!((point(sketch, start)[1] - 10.0).abs() < 1e-9);
    assert!((point(sketch, end)[1] + 10.0).abs() < 1e-9);
}

#[test]
fn mirrored_arcs_and_gaps_follow_the_tolerance() {
    // An arc drawn with a -Z normal: object (10, 0) is world (-10, 0), and
    // its 0°..90° runs from world 90° to 180°.
    let arc = "0\nARC\n8\n0\n10\n10\n20\n0\n30\n0\n40\n5\n50\n0\n51\n90\n210\n0\n220\n0\n230\n-1\n";
    let result = import(&file(None, &[], &[arc]));
    let s = &result.sketch;
    let a = &s.arcs[0];
    assert_eq!(point(s, &a.center), [-10.0, 0.0]);
    let start = point(s, &a.start);
    assert!(
        (start[0] + 10.0).abs() < 1e-9 && (start[1] - 5.0).abs() < 1e-9,
        "{start:?}"
    );
    let end = point(s, &a.end);
    assert!(
        (end[0] + 15.0).abs() < 1e-9 && end[1].abs() < 1e-9,
        "{end:?}"
    );
    // A 0.0005 mm gap closes at the default tolerance; 0.01 does not.
    let mut lines = square("0", [0.0, 0.0], 10.0);
    lines[3] = line("0", [0.0, 10.0], [0.0, 0.0005]);
    assert_eq!(import(&file(None, &[], &refs(&lines))).report.loops, 1);
    lines[3] = line("0", [0.0, 10.0], [0.0, 0.01]);
    let open = import(&file(None, &[], &refs(&lines))).report;
    assert_eq!((open.loops, open.open_curves), (0, 4));
}

#[test]
fn bad_files_and_options_are_refused_with_reasons() {
    let error =
        |text: &str, options: &DxfImportOptions| import_dxf(text, options).unwrap_err().message;
    let options = DxfImportOptions::default();
    assert!(error("AutoCAD Binary DXF\r\n", &options).contains("binary DXF"));
    assert!(error("0\nSECTION\n2\nHEADER\n0\nENDSEC\n", &options).contains("no ENTITIES"));
    assert!(error("abc\nLINE\n", &options).contains("not a number"));
    assert!(error("0\nSECTION\n2\nENTITIES\n0\nLINE\n", &options).contains("ENDSEC"));
    let notes = file(None, &[], &["0\nTEXT\n8\n0\n1\nx\n"]);
    assert!(error(&notes, &options).contains("no importable curves"));
    let circle_file = |units| file(units, &[], &[&circle("0", [0.0, 0.0], 1.0)]);
    assert!(error(&circle_file(Some(14)), &options).contains("$INSUNITS 14"));
    let tight = DxfImportOptions {
        tolerance_mm: 0.0,
        ..Default::default()
    };
    assert!(error(&notes, &tight).contains("tolerance_mm"));
    let only = DxfImportOptions {
        layers: vec!["NONE".into()],
        ..Default::default()
    };
    assert!(error(&circle_file(None), &only).contains("selected layers"));
    // Unsupported pieces are counted, not fatal.
    let mixed = file(
        None,
        &[],
        &[
            &circle("0", [0.0, 0.0], 1.0),
            "0\nPOLYLINE\n8\n0\n70\n16\n0\nSEQEND\n",
            "0\n3DFACE\n8\n0\n",
        ],
    );
    let report = import(&mixed).report;
    assert_eq!(report.skipped["POLYLINE: 3D polyline or mesh"], 1);
    assert_eq!(report.skipped["3DFACE: unsupported entity"], 1);
}

#[test]
fn old_style_polylines_read_their_vertices() {
    let polyline = "0\nPOLYLINE\n8\n0\n66\n1\n70\n1\n10\n0\n20\n0\n30\n0\n\
0\nVERTEX\n8\n0\n10\n0\n20\n0\n0\nVERTEX\n8\n0\n10\n10\n20\n0\n42\n1\n\
0\nVERTEX\n8\n0\n10\n10\n20\n10\n0\nVERTEX\n8\n0\n10\n0\n20\n10\n0\nSEQEND\n";
    let result = import(&file(None, &[], &[polyline]));
    assert_eq!(
        (result.sketch.lines.len(), result.sketch.arcs.len()),
        (3, 1)
    );
    assert_eq!(result.report.loops, 1);
    assert_directed(&result.sketch);
}

#[test]
fn ten_thousand_curves_import_quickly_and_limits_are_refused() {
    // Ten 170 mm plates, each with 249 square 5 mm holes: 10,000 curves.
    let mut entities = Vec::new();
    for plate in 0..10 {
        let x = plate as f64 * 200.0;
        entities.extend(square("0", [x, 0.0], 170.0));
        for hole in 0..249 {
            let (i, j) = ((hole % 16) as f64, (hole / 16) as f64);
            entities.extend(square("0", [x + 5.0 + i * 10.0, 5.0 + j * 10.0], 5.0));
        }
    }
    let started = std::time::Instant::now();
    let report = import(&file(None, &[], &refs(&entities))).report;
    let elapsed = started.elapsed().as_secs_f64();
    assert_eq!(
        (report.curves, report.loops, report.regions, report.holes),
        (10_000, 2_500, 9, 2_490)
    );
    assert!(elapsed < 20.0, "{elapsed} s");
    entities.push(circle("0", [0.0, -50.0], 1.0));
    let message = import_dxf(
        &file(None, &[], &refs(&entities)),
        &DxfImportOptions::default(),
    )
    .unwrap_err()
    .message;
    assert!(message.contains("10000 curves"), "{message}");
    // More than 1,000 separate regions exceed the sketch's limit.
    let scattered: Vec<String> = (0..1002)
        .flat_map(|k| square("0", [k as f64 * 20.0, 0.0], 10.0))
        .collect();
    let message = import_dxf(
        &file(None, &[], &refs(&scattered)),
        &DxfImportOptions::default(),
    )
    .unwrap_err()
    .message;
    assert!(message.contains("1000 separate regions"), "{message}");
}
