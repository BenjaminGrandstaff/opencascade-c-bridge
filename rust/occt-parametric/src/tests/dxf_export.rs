//! DXF export round trips: a face is exported, read back by the DXF
//! importer, and the rebuilt face must have the same area and curve types.

use super::*;
use crate::dxf::{DxfImportOptions, import_dxf};
use crate::dxf_export::face_to_dxf;
use occt_bridge::{ShapeType, Vec3};
use serde_json::{Value, json};
use std::f64::consts::PI;

fn family(features: Value) -> FamilyDefinition {
    serde_json::from_value(json!({"id": "dxf", "version": 1, "parameters": [], "derived_parameters": [],
        "derived_vector_parameters": [], "constraints": [], "requirements": [], "datums": [], "features": features}))
        .unwrap()
}
fn outline(sketch: SketchDefinition) -> FamilyDefinition {
    family(json!([{"id": "outline", "operation": {"sketch_face": {"sketch": sketch}}}]))
}

/// Faces of the last feature whose normal is `normal`, largest first,
/// exported; returns the DXF text, the report and the face's area.
fn export(
    definition: &FamilyDefinition,
    normal: [f64; 3],
) -> (String, crate::dxf_export::DxfExportReport) {
    let session = Session::new().unwrap();
    let part = PartInstance {
        id: "p".into(),
        definition,
        overrides: HashMap::new(),
        provenance: "test".into(),
    };
    let generated = part.regenerate(&session).unwrap();
    let shape = generated
        .shape(&definition.features.last().unwrap().id)
        .unwrap();
    let face = session
        .subshapes(shape, ShapeType::Face)
        .unwrap()
        .into_iter()
        .filter(|f| session.face_is_planar(f).unwrap())
        .filter(|f| {
            let n = session.face_normal(f).unwrap();
            (n.x * normal[0] + n.y * normal[1] + n.z * normal[2]) > 0.999
        })
        .max_by(|a, b| {
            session
                .surface_area(a)
                .unwrap()
                .total_cmp(&session.surface_area(b).unwrap())
        })
        .unwrap();
    let bends = [[Vec3::new(0.0, 5.0, 0.0), Vec3::new(10.0, 5.0, 0.0)]];
    let result = face_to_dxf(&session, &face, if normal[2] > 0.5 { &bends } else { &[] }).unwrap();
    drop((face, generated));
    assert_eq!(session.shape_count().unwrap(), 0);
    result
}

/// Area of the face the DXF text rebuilds (without its bend lines).
fn reimported_area(text: &str) -> f64 {
    let options = DxfImportOptions {
        layers: vec!["OUTLINE".into(), "HOLES".into()],
        ..Default::default()
    };
    let sketch = import_dxf(text, &options).unwrap().sketch;
    let definition = outline(sketch);
    let session = Session::new().unwrap();
    let part = PartInstance {
        id: "p".into(),
        definition: &definition,
        overrides: HashMap::new(),
        provenance: "test".into(),
    };
    let generated = part.regenerate(&session).unwrap();
    session
        .surface_area(generated.shape("outline").unwrap())
        .unwrap()
}

fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-6 * b.max(1.0), "{a} vs {b}");
}

#[test]
fn the_bracket_round_trips_with_its_fillet_holes_and_bend_layer() {
    let text = include_str!("../../../../tools/model/dxf/bracket.dxf");
    let options = DxfImportOptions {
        layers: vec!["OUTLINE".into(), "HOLES".into()],
        ..Default::default()
    };
    let sketch = import_dxf(text, &options).unwrap().sketch;
    let (dxf, report) = export(&outline(sketch), [0.0, 0.0, 1.0]);
    let area = 2525.0 - 33.25 * PI;
    close(report.area_mm2, area);
    assert_eq!(report.entities["LINE"], 7, "{report:?}");
    assert_eq!((report.entities["ARC"], report.entities["CIRCLE"]), (1, 3));
    assert_eq!(
        (
            report.layers["OUTLINE"],
            report.layers["HOLES"],
            report.layers["BEND"]
        ),
        (7, 3, 1)
    );
    assert_eq!(report.bounds_mm, [[0.0, 0.0], [80.0, 50.0]]);
    close(reimported_area(&dxf), area);
}

#[test]
fn ellipses_splines_and_islands_round_trip_exactly() {
    // A 100 square with a 60 square hole, a 20 square island in it, an
    // ellipse beside it and a spline D shape.
    let line = |a: [f64; 2], b: [f64; 2]| {
        format!(
            "0\nLINE\n8\n0\n10\n{}\n20\n{}\n11\n{}\n21\n{}\n",
            a[0], a[1], b[0], b[1]
        )
    };
    let square = |x: f64, y: f64, s: f64| {
        [
            line([x, y], [x + s, y]),
            line([x + s, y], [x + s, y + s]),
            line([x + s, y + s], [x, y + s]),
            line([x, y + s], [x, y]),
        ]
        .concat()
    };
    let source = format!(
        "0\nSECTION\n2\nENTITIES\n{}{}{}0\nELLIPSE\n8\n0\n10\n150\n20\n50\n11\n12\n21\n16\n40\n0.5\n41\n0\n42\n6.283185307179586\n\
0\nSPLINE\n8\n0\n70\n8\n71\n3\n72\n8\n73\n4\n40\n0\n40\n0\n40\n0\n40\n0\n40\n1\n40\n1\n40\n1\n40\n1\n\
10\n200\n20\n0\n10\n210\n20\n30\n10\n220\n20\n30\n10\n230\n20\n0\n{}0\nENDSEC\n0\nEOF\n",
        square(0.0, 0.0, 100.0),
        square(20.0, 20.0, 60.0),
        square(40.0, 40.0, 20.0),
        line([230.0, 0.0], [200.0, 0.0])
    );
    let sketch = import_dxf(&source, &DxfImportOptions::default())
        .unwrap()
        .sketch;
    // Each region is its own face: export the largest (the frame with the
    // hole) and compare all of them through a second import.
    let definition = outline(sketch);
    let (dxf, report) = export(&definition, [0.0, 0.0, 1.0]);
    close(report.area_mm2, 10000.0 - 3600.0);
    close(reimported_area(&dxf), 10000.0 - 3600.0);
    // Every face on its own: the ellipse (its major axis turned 53°), the
    // spline D shape and the island.
    let session = Session::new().unwrap();
    let part = PartInstance {
        id: "p".into(),
        definition: &definition,
        overrides: HashMap::new(),
        provenance: "test".into(),
    };
    let generated = part.regenerate(&session).unwrap();
    let faces = session
        .subshapes(generated.shape("outline").unwrap(), ShapeType::Face)
        .unwrap();
    let mut seen = std::collections::BTreeSet::new();
    for face in &faces {
        let (text, report) = face_to_dxf(&session, face, &[]).unwrap();
        close(reimported_area(&text), report.area_mm2);
        seen.extend(report.entities.keys().cloned());
    }
    assert!(
        seen.contains("ELLIPSE") && seen.contains("SPLINE") && seen.contains("LINE"),
        "{seen:?}"
    );
}

#[test]
fn solid_faces_export_in_their_own_plane() {
    // A 40 × 30 × 20 block with a Ø10 hole along Y. Its +Y face is seen
    // from +Y: plane X runs along model -X and plane Y along +Z.
    let lengths = |x: f64, y: f64, z: f64| {
        json!({"literal": {"x": {"value": x, "dimension": "length", "unit": "millimeter"},
        "y": {"value": y, "dimension": "length", "unit": "millimeter"}, "z": {"value": z, "dimension": "length", "unit": "millimeter"}}})
    };
    let scalars = |x: f64, y: f64, z: f64| {
        json!({"literal": {"x": {"value": x, "dimension": "scalar", "unit": null},
        "y": {"value": y, "dimension": "scalar", "unit": null}, "z": {"value": z, "dimension": "scalar", "unit": null}}})
    };
    let mm = |v: f64| json!({"literal": {"value": v, "dimension": "length", "unit": "millimeter"}});
    let definition = family(json!([
        {"id": "block", "operation": {"box": {"origin": lengths(0.0, 0.0, 0.0), "size": lengths(40.0, 30.0, 20.0)}}},
        {"id": "rod", "operation": {"cylinder": {"origin": lengths(20.0, -1.0, 10.0), "axis": scalars(0.0, 1.0, 0.0), "radius": mm(5.0), "height": mm(32.0)}}},
        {"id": "drilled", "operation": {"cut": {"object": "block", "tool": "rod"}}}]));
    let (dxf, report) = export(&definition, [0.0, 1.0, 0.0]);
    close(report.area_mm2, 800.0 - 25.0 * PI);
    assert_eq!(report.x_axis, [-1.0, 0.0, 0.0]);
    assert_eq!(report.y_axis, [0.0, 0.0, 1.0]);
    assert_eq!(report.layers["HOLES"], 1);
    assert!(!report.layers.contains_key("BEND"));
    // The +Y face is at y = 30; its plane origin is there.
    close(report.origin_mm[1], 30.0);
    close(reimported_area(&dxf), 800.0 - 25.0 * PI);
}
