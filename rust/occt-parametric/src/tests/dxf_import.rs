//! DXF outlines imported into sketches build exact solids: each import is
//! extruded 5 mm and its volume compared with the closed-form area.

use super::*;
use crate::dxf::{DxfImportOptions, import_dxf};
use std::f64::consts::PI;

fn file(entities: &[String]) -> String {
    format!(
        "0\nSECTION\n2\nENTITIES\n{}0\nENDSEC\n0\nEOF\n",
        entities.concat()
    )
}
fn line(a: [f64; 2], b: [f64; 2]) -> String {
    format!(
        "0\nLINE\n8\n0\n10\n{}\n20\n{}\n11\n{}\n21\n{}\n",
        a[0], a[1], b[0], b[1]
    )
}
fn rectangle(x: f64, y: f64, w: f64, h: f64) -> Vec<String> {
    vec![
        line([x, y], [x + w, y]),
        line([x + w, y], [x + w, y + h]),
        line([x + w, y + h], [x, y + h]),
        line([x, y + h], [x, y]),
    ]
}

/// The extruded volume of the imported sketch, and the report.
fn extruded(text: &str) -> (f64, crate::dxf::DxfImportReport) {
    extruded_with(text, &DxfImportOptions::default())
}
fn extruded_with(text: &str, options: &DxfImportOptions) -> (f64, crate::dxf::DxfImportReport) {
    let import = import_dxf(text, options).unwrap();
    let at = |x: f64, y: f64, z: f64| {
        VectorExpr::Literal(VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter))
    };
    let definition = FamilyDefinition {
        id: "dxf".into(),
        version: 1,
        parameters: vec![],
        derived_parameters: vec![],
        derived_vector_parameters: vec![],
        constraints: vec![],
        features: vec![
            FeatureDefinition {
                id: "outline".into(),
                operation: FeatureOperation::SketchFace {
                    sketch: Box::new(import.sketch),
                },
            },
            FeatureDefinition {
                id: "plate".into(),
                operation: FeatureOperation::Extrude {
                    input: "outline".into(),
                    direction: at(0.0, 0.0, 5.0),
                    extent: Default::default(),
                },
            },
        ],
        requirements: vec![],
        datums: vec![],
        references: vec![],
        feature_colors: Default::default(),
        assumptions: vec![],
        history: Default::default(),
    };
    let session = Session::new().unwrap();
    let part = PartInstance {
        id: "plate".into(),
        definition: &definition,
        overrides: HashMap::new(),
        provenance: "test".into(),
    };
    let generated = part.regenerate(&session).unwrap();
    let shape = generated.shape("plate").unwrap();
    assert!(session.is_valid(shape).unwrap());
    (session.volume(shape).unwrap(), import.report)
}

fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1e-6 * expected.max(1.0),
        "{actual} vs {expected}"
    );
}

#[test]
fn a_plate_with_a_hole_and_a_slot_extrudes_exactly() {
    // 60 × 40 plate, Ø10 hole at (15, 20), and a 20 × 8 slot with round ends
    // (a closed polyline with bulges) centred at (40, 20).
    let mut entities = rectangle(0.0, 0.0, 60.0, 40.0);
    entities.push("0\nCIRCLE\n8\n0\n10\n15\n20\n20\n40\n5\n".into());
    entities.push(
        "0\nLWPOLYLINE\n8\n0\n90\n4\n70\n1\n10\n34\n20\n16\n10\n46\n20\n16\n42\n1\n10\n46\n20\n24\n10\n34\n20\n24\n42\n1\n".into(),
    );
    let (volume, report) = extruded(&file(&entities));
    assert_eq!((report.loops, report.holes), (3, 2));
    let slot = 12.0 * 8.0 + PI * 16.0;
    close(volume, 5.0 * (2400.0 - 25.0 * PI - slot));
}

#[test]
fn islands_ellipses_and_splines_become_separate_regions() {
    // A 100 square with a 60 square hole holding a 20 square island; a full
    // ellipse (a = 20, b = 10) beside it; and a D shape of a cubic spline
    // over a straight line.
    let mut entities = rectangle(0.0, 0.0, 100.0, 100.0);
    entities.extend(rectangle(20.0, 20.0, 60.0, 60.0));
    entities.extend(rectangle(40.0, 40.0, 20.0, 20.0));
    entities.push(
        "0\nELLIPSE\n8\n0\n10\n150\n20\n50\n11\n20\n21\n0\n40\n0.5\n41\n0\n42\n6.283185307179586\n"
            .into(),
    );
    entities.push(
        "0\nSPLINE\n8\n0\n70\n8\n71\n3\n72\n8\n73\n4\n40\n0\n40\n0\n40\n0\n40\n0\n40\n1\n40\n1\n40\n1\n40\n1\n\
10\n200\n20\n0\n10\n210\n20\n30\n10\n220\n20\n30\n10\n230\n20\n0\n"
            .into(),
    );
    entities.push(line([230.0, 0.0], [200.0, 0.0]));
    let (volume, report) = extruded(&file(&entities));
    assert_eq!((report.loops, report.holes, report.regions), (5, 1, 3));
    // The D shape's area by Green's theorem: the cubic Bezier's term from
    // its control points, closed by the chord.
    let poles = [[200.0, 0.0], [210.0, 30.0], [220.0, 30.0], [230.0, 0.0]];
    let cross = |a: [f64; 2], b: [f64; 2]| a[0] * b[1] - a[1] * b[0];
    let [p0, p1, p2, p3] = poles;
    let bezier = (6.0 * cross(p0, p1)
        + 3.0 * cross(p0, p2)
        + cross(p0, p3)
        + 3.0 * cross(p1, p2)
        + 3.0 * cross(p1, p3)
        + 6.0 * cross(p2, p3))
        / 20.0;
    let chord = cross(p3, p0) / 2.0;
    let d_shape = (bezier + chord).abs();
    let area = 10000.0 - 3600.0 + 400.0 + PI * 20.0 * 10.0 + d_shape;
    close(volume, 5.0 * area);
}

#[test]
fn curves_drawn_in_any_direction_are_turned_to_follow_the_loop() {
    // A square whose second and fourth sides run backwards, with a hole
    // made of a clockwise polyline (negative bulges: a circle of radius 5
    // drawn as two half arcs) and a spline drawn from right to left over a
    // left-to-right line.
    let mut entities = vec![
        line([0.0, 0.0], [50.0, 0.0]),
        line([50.0, 50.0], [50.0, 0.0]),
        line([50.0, 50.0], [0.0, 50.0]),
        line([0.0, 0.0], [0.0, 50.0]),
    ];
    entities.push(
        "0\nLWPOLYLINE\n8\n0\n90\n2\n70\n1\n10\n10\n20\n25\n42\n-1\n10\n20\n20\n25\n42\n-1\n"
            .into(),
    );
    entities.push(
        "0\nSPLINE\n8\n0\n70\n8\n71\n3\n72\n8\n73\n4\n40\n0\n40\n0\n40\n0\n40\n0\n40\n2\n40\n2\n40\n2\n40\n2\n\
10\n130\n20\n0\n10\n120\n20\n30\n10\n110\n20\n30\n10\n100\n20\n0\n"
            .into(),
    );
    entities.push(line([100.0, 0.0], [130.0, 0.0]));
    let (volume, report) = extruded(&file(&entities));
    assert_eq!((report.loops, report.holes, report.regions), (3, 1, 1));
    // With evenly spaced poles x = 30t and y = 90t(1 - t), so the D shape's
    // area is the integral of 2700t(1 - t): 450.
    let d_shape = 450.0;
    close(volume, 5.0 * (2500.0 - 25.0 * PI + d_shape));
}

#[test]
fn the_bracket_example_imports_its_outline_and_holes_layers() {
    let text = include_str!("../../../../tools/model/dxf/bracket.dxf");
    let options = DxfImportOptions {
        layers: vec!["OUTLINE".into(), "HOLES".into()],
        ..Default::default()
    };
    let (volume, report) = extruded_with(text, &options);
    assert_eq!(report.unit_source, "$INSUNITS");
    assert_eq!((report.loops, report.holes, report.open_curves), (4, 3, 0));
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    // The L, its 5 mm inside fillet, less three Ø6 holes.
    close(volume, 5.0 * (2525.0 - 6.25 * PI - 27.0 * PI));
}
