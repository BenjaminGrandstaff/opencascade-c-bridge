//! Datum targets (schema 166): drawn target symbols, validation, and datums
//! established in measured inspection by the points at their targets alone.

use super::inspection::{nominal_record, page, plate};
use super::*;

fn mm(x: f64, y: f64, z: f64) -> VectorQuantity {
    VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter)
}

fn point_target(index: u32, p: [f64; 3]) -> DrawingDatumTarget {
    DrawingDatumTarget {
        index,
        shape: DatumTargetShape::Point {
            at: mm(p[0], p[1], p[2]),
        },
        offset_mm: [-12.0, -12.0],
    }
}

fn graph(definition: &FamilyDefinition) -> InstanceGraph<'_> {
    let mut graph = InstanceGraph::new(definition);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    graph
}

/// The plate page with 3-2-1 point targets: A1–A3 on the bottom, B1–B2 on
/// the left face, C1 on the front.
fn targeted_page() -> DrawingDefinition {
    let mut page = page();
    page.datum_features[0].targets = vec![
        point_target(1, [10.0, 10.0, 0.0]),
        point_target(2, [50.0, 10.0, 0.0]),
        point_target(3, [30.0, 30.0, 0.0]),
    ];
    page.datum_features[1].targets = vec![
        point_target(1, [0.0, 10.0, 5.0]),
        point_target(2, [0.0, 30.0, 5.0]),
    ];
    page.datum_features[2].targets = vec![point_target(1, [30.0, 0.0, 5.0])];
    page
}

fn measured(id: &str, points_mm: Vec<[f64; 3]>) -> MeasuredFeature {
    MeasuredFeature {
        nominals: Vec::new(),
        id: id.into(),
        points_mm,
    }
}

/// The nominal record with its A, B and C surfaces replaced by the points
/// measured at each target (`lift` raises A3).
fn targeted_record(lift: f64) -> InspectionRecord {
    let mut record = nominal_record();
    record.datum_features.clear();
    record.datum_targets = vec![
        measured("A1", vec![[10.0, 10.0, 0.0]]),
        measured("A2", vec![[50.0, 10.0, 0.0]]),
        measured("A3", vec![[30.0, 30.0, lift]]),
        measured("B1", vec![[0.0, 10.0, 5.0]]),
        measured("B2", vec![[0.0, 30.0, 5.0]]),
        measured("C1", vec![[30.0, 0.0, 5.0]]),
    ];
    record
}

fn result<'a>(report: &'a InspectionReport, id: &str) -> &'a ControlResult {
    &report
        .controls
        .iter()
        .find(|c| c.control == id)
        .unwrap()
        .result
}

#[test]
fn datum_targets_are_drawn_with_their_symbols_and_counted() {
    let definition = plate();
    let graph = graph(&definition);
    let session = Session::new().unwrap();
    let mut page = page();
    page.datum_features[0].targets = vec![
        point_target(1, [10.0, 10.0, 0.0]),
        DrawingDatumTarget {
            index: 2,
            shape: DatumTargetShape::Line {
                start: mm(40.0, 5.0, 0.0),
                end: mm(55.0, 5.0, 0.0),
            },
            offset_mm: [10.0, -12.0],
        },
        DrawingDatumTarget {
            index: 3,
            shape: DatumTargetShape::Area {
                center: mm(30.0, 30.0, 0.0),
                diameter: Quantity::length(6.0, LengthUnit::Millimeter),
            },
            offset_mm: [12.0, 12.0],
        },
    ];
    let generated = page
        .generate(&graph, &session, DrawingRenderOptions::default())
        .unwrap();
    for text in ["A1", "A2", "A3", "Ø6"] {
        assert!(
            generated.gdt_labels.iter().any(|l| l.text == text),
            "{text}"
        );
    }
    assert_eq!(
        generated
            .gdt_lines
            .iter()
            .map(|l| l.points_mm.len())
            .sum::<usize>(),
        crate::drawing::gdt::vertex_count(
            &page.datum_features,
            &page.feature_control_frames,
            &page.datum_reference_frames
        )
        .unwrap()
    );
    // The point target's × sits on its projection in the top view.
    let view = &page.views[0];
    let p = view
        .paper(view.project(Vec3::new(10.0, 10.0, 0.0)).unwrap())
        .unwrap();
    assert!(generated.gdt_lines.iter().any(|l| {
        l.points_mm.len() == 2
            && (l.points_mm[0][0] - (p[0] - 1.5)).abs() < 1e-9
            && (l.points_mm[0][1] - (p[1] - 1.5)).abs() < 1e-9
    }));
    let json = serde_json::to_string(&page.datum_features[0]).unwrap();
    assert!(
        json.contains("\"targets\"") && json.contains("\"area\""),
        "{json}"
    );
    assert_eq!(
        serde_json::from_str::<DrawingDatumFeature>(&json).unwrap(),
        page.datum_features[0]
    );
    // Features without targets keep their old JSON.
    assert!(
        !serde_json::to_string(&page.datum_features[1])
            .unwrap()
            .contains("targets")
    );
}

#[test]
fn datum_targets_are_validated() {
    let definition = plate();
    let graph = graph(&definition);
    for (case, expected) in [
        (0, "off its feature's datum plane"),
        (1, "unique indexes"),
        (2, "unique indexes"),
        (3, "at least 8 mm"),
        (4, "positive finite diameter"),
        (5, "distinct ends"),
        (6, "not features of size"),
        (7, "planar datum features"),
    ] {
        let mut page = targeted_page();
        let a = &mut page.datum_features[0];
        match case {
            0 => a.targets[0] = point_target(1, [10.0, 10.0, 0.5]),
            1 => a.targets[1].index = 1,
            2 => a.targets[0].index = 0,
            3 => a.targets[0].offset_mm = [3.0, 4.0],
            4 => {
                a.targets[0].shape = DatumTargetShape::Area {
                    center: mm(10.0, 10.0, 0.0),
                    diameter: Quantity::length(0.0, LengthUnit::Millimeter),
                }
            }
            5 => {
                a.targets[0].shape = DatumTargetShape::Line {
                    start: mm(10.0, 10.0, 0.0),
                    end: mm(10.0, 10.0, 0.0),
                }
            }
            6 => {
                a.feature_of_size = true;
                a.size_kind = Some(FeatureOfSizeKind::External);
            }
            _ => a.attachment.anchor = DatumRef::new("part", "hole"),
        }
        let error = page.validate(&graph).unwrap_err().message;
        assert!(error.contains(expected), "case {case}: {error}");
    }
    targeted_page().validate(&graph).unwrap();
}

#[test]
fn datum_targets_alone_establish_their_datums() {
    let definition = plate();
    let graph = graph(&definition);
    let page = targeted_page();
    // Exact target points give the nominal frame: everything conforms.
    let report = page
        .evaluate_inspection(&graph, &targeted_record(0.0))
        .unwrap();
    assert!(report.conforms(), "{report:?}");
    // A3 measured 0.02 high tilts A by 0.001 about x: the flat top is then
    // 0.04 out of parallel over its 40 mm depth, whatever the rest of the
    // bottom surface does.
    let report = page
        .evaluate_inspection(&graph, &targeted_record(0.02))
        .unwrap();
    let ControlResult::Evaluated(parallel) = result(&report, "parallel") else {
        panic!("{report:?}");
    };
    assert!((parallel.deviation_mm - 0.04).abs() < 1e-6, "{parallel:?}");
    // The secondary plane from two points and the tertiary from one keep
    // the nominal hole at true position.
    let ControlResult::Evaluated(position) = result(&report, "position") else {
        panic!("{report:?}");
    };
    assert!(position.conforms, "{position:?}");
    // A target that was not measured leaves its datum unestablished.
    let mut record = targeted_record(0.0);
    record.datum_targets.retain(|t| t.id != "C1");
    let report = page.evaluate_inspection(&graph, &record).unwrap();
    assert!(
        matches!(
            result(&report, "position"),
            ControlResult::NotEvaluated(reason) if reason.contains("datum targets of 'C' were not all measured")
        ),
        "{report:?}"
    );
    assert!(matches!(
        result(&report, "parallel"),
        ControlResult::Evaluated(_)
    ));
    // Surface points for a targeted feature, or an unknown target, are refused.
    let mut record = targeted_record(0.0);
    record.datum_features = nominal_record().datum_features;
    let error = page
        .evaluate_inspection(&graph, &record)
        .unwrap_err()
        .message;
    assert!(
        error.contains("established by its datum targets"),
        "{error}"
    );
    let mut record = targeted_record(0.0);
    record
        .datum_targets
        .push(measured("A9", vec![[0.0, 0.0, 0.0]]));
    let error = page
        .evaluate_inspection(&graph, &record)
        .unwrap_err()
        .message;
    assert!(error.contains("unknown feature 'A9'"), "{error}");
    let json = serde_json::to_string(&targeted_record(0.0)).unwrap();
    assert_eq!(
        serde_json::from_str::<InspectionRecord>(&json).unwrap(),
        targeted_record(0.0)
    );
}

#[test]
fn area_targets_take_their_high_points_and_scale() {
    // Three Ø10 area targets on the bottom, each measured by 1,000 points
    // with low dips inside: the simulator rests on the targets' high points.
    let definition = plate();
    let graph = graph(&definition);
    let mut page = targeted_page();
    let centers = [[10.0, 10.0], [50.0, 10.0], [30.0, 30.0]];
    page.datum_features[0].targets = centers
        .iter()
        .enumerate()
        .map(|(i, c)| DrawingDatumTarget {
            index: i as u32 + 1,
            shape: DatumTargetShape::Area {
                center: mm(c[0], c[1], 0.0),
                diameter: Quantity::length(10.0, LengthUnit::Millimeter),
            },
            offset_mm: [-12.0, -12.0],
        })
        .collect();
    let mut record = targeted_record(0.0);
    record.datum_targets.retain(|t| !t.id.starts_with('A'));
    for (i, c) in centers.iter().enumerate() {
        let points = (0..1000)
            .map(|k| {
                let (r, a) = (4.5 * ((k % 10) as f64 / 9.0), k as f64 * 0.37);
                let dip = if k % 7 == 0 { 0.03 } else { 0.0 };
                [c[0] + r * a.cos(), c[1] + r * a.sin(), dip]
            })
            .collect();
        record
            .datum_targets
            .push(measured(&format!("A{}", i + 1), points));
    }
    let started = std::time::Instant::now();
    let report = page.evaluate_inspection(&graph, &record).unwrap();
    let elapsed = started.elapsed().as_secs_f64();
    let ControlResult::Evaluated(parallel) = result(&report, "parallel") else {
        panic!("{report:?}");
    };
    assert!(parallel.deviation_mm < 1e-9, "{parallel:?}");
    assert!(report.conforms(), "{report:?}");
    assert!(elapsed < 1.0, "{elapsed} s");
}
