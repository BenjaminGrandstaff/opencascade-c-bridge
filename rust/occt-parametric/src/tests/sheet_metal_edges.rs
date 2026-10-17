//! Edge flanges and bend reliefs (schema 176): flanges folded up from the
//! strip's width edges, notches through bend zones where they meet a bend,
//! and the polygonal blank that unfolds them, checked by exact volumes.
use super::*;

const PI: f64 = std::f64::consts::PI;
const HALF: f64 = std::f64::consts::FRAC_PI_2;

fn length(value: f64) -> ScalarExpr {
    ScalarExpr::Literal(Quantity::length(value, LengthUnit::Millimeter))
}

/// A U channel: three 20 mm flanges, two quarter bends at inside radius 3,
/// 10 mm wide and 2 mm thick.
fn channel() -> SheetMetalDefinition {
    SheetMetalDefinition {
        origin: VectorExpr::Literal(VectorQuantity::lengths(
            0.0,
            0.0,
            0.0,
            LengthUnit::Millimeter,
        )),
        width_axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 1.0, 0.0)),
        start_direction: VectorExpr::Literal(VectorQuantity::scalars(1.0, 0.0, 0.0)),
        width: length(10.0),
        thickness: length(2.0),
        flanges: vec![length(20.0); 3],
        bends: vec![
            SheetMetalBend {
                angle_radians: ScalarExpr::Literal(Quantity::scalar(HALF)),
                inside_radius: length(3.0),
            };
            2
        ],
        cutouts: Vec::new(),
        start_hem: None,
        end_hem: None,
        edge_flanges: Vec::new(),
    }
}

fn edge(id: &str, flange: usize, side: SheetEdge, start: f64, span: f64) -> SheetMetalEdgeFlange {
    SheetMetalEdgeFlange {
        id: id.into(),
        flange,
        edge: side,
        start: length(start),
        length: length(span),
        angle_radians: ScalarExpr::Literal(Quantity::scalar(HALF)),
        inside_radius: length(1.0),
        height: length(8.0),
        relief_depth: None,
    }
}

fn family(sheet: SheetMetalDefinition) -> FamilyDefinition {
    let mut family = super::inspection::plate();
    family.parameters.clear();
    family.datums.clear();
    family.references.clear();
    family.requirements.clear();
    family.features = vec![
        FeatureDefinition {
            id: "folded".into(),
            operation: FeatureOperation::SheetMetal {
                definition: Box::new(sheet),
            },
        },
        FeatureDefinition {
            id: "flat".into(),
            operation: FeatureOperation::SheetMetalFlat {
                input: "folded".into(),
                neutral_factor: ScalarExpr::Literal(Quantity::scalar(0.5)),
                bend_table: None,
            },
        },
    ];
    family
}

fn volumes(sheet: SheetMetalDefinition) -> (f64, f64) {
    let (folded, flat, _) = measured(sheet);
    (folded, flat)
}

/// Folded and flat volumes and the folded sheet's exact bounds.
fn measured(sheet: SheetMetalDefinition) -> (f64, f64, occt_bridge::Bounds) {
    let family = family(sheet);
    let session = Session::new().unwrap();
    let generated = PartInstance {
        id: "part".into(),
        definition: &family,
        overrides: HashMap::new(),
        provenance: "test".into(),
    }
    .regenerate(&session)
    .unwrap();
    let measure = |id: &str| {
        let shape = generated.shape(id).unwrap();
        assert_eq!(session.shape_type(shape).unwrap(), ShapeType::Solid, "{id}");
        assert!(session.is_valid(shape).unwrap(), "{id}");
        session.volume(shape).unwrap()
    };
    let bounds = session
        .exact_bounds(generated.shape("folded").unwrap())
        .unwrap();
    (measure("folded"), measure("flat"), bounds)
}

fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= 1e-7 * expected,
        "{actual} vs {expected}"
    );
}

/// Midplane length of a quarter bend at inside radius `r` in 2 mm sheet.
fn quarter(r: f64) -> f64 {
    HALF * (r + 1.0)
}

#[test]
fn edge_flanges_and_reliefs_fold_and_unfold_to_the_same_volume() {
    // A tray: the channel's floor gets walls on both edges over its whole
    // length, with 4 mm reliefs where they meet the two bends.
    let mut sheet = channel();
    for side in [SheetEdge::Origin, SheetEdge::Far] {
        let mut wall = edge(&format!("{side:?}"), 1, side, 0.0, 20.0);
        wall.relief_depth = Some(length(4.0));
        sheet.edge_flanges.push(wall);
    }
    let strip = (60.0 + 2.0 * quarter(3.0)) * 10.0 * 2.0;
    let walls = 2.0 * (quarter(1.0) + 8.0) * 20.0 * 2.0;
    let reliefs = 4.0 * quarter(3.0) * 2.0 * 4.0;
    let (folded, flat, bounds) = measured(sheet.clone());
    // The sheet normal is width × start = −z: walls rise with the bends,
    // and stand 1 + 2 mm outside the 10 mm width on both sides.
    assert!((bounds.max.z - 1.0).abs() < 1e-9, "{bounds:?}");
    assert!(
        (bounds.max.y - 13.0).abs() < 1e-9 && (bounds.min.y + 3.0).abs() < 1e-9,
        "{bounds:?}"
    );
    close(folded, strip + walls - reliefs);
    close(flat, strip + walls - reliefs);

    // The blank: tabs off both long edges of the floor and notches through
    // both bend zones; its outline encloses the flat volume over 2 mm.
    let metrics = sheet.flat_pattern(&HashMap::new(), 0.5).unwrap();
    let floor = 20.0 + quarter(3.0);
    assert_eq!(metrics.edge_tabs.len(), 2);
    assert_eq!(metrics.reliefs.len(), 4);
    let far = metrics.edge_tabs.iter().find(|t| t.id == "Far").unwrap();
    assert!(
        (far.along_mm[0] - floor).abs() < 1e-12 && (far.along_mm[1] - floor - 20.0).abs() < 1e-12
    );
    assert!((far.across_mm[1] - (10.0 + quarter(1.0) + 8.0)).abs() < 1e-12);
    let outline = metrics.outline();
    let area = outline
        .iter()
        .zip(outline.iter().cycle().skip(1))
        .map(|(a, b)| a[0] * b[1] - b[0] * a[1])
        .sum::<f64>()
        * 0.5;
    close(area.abs() * 2.0, flat);
    let drawing = metrics.drawing("tray").unwrap();
    assert_eq!(drawing.polylines.len(), 1 + 2 + 2);
}

#[test]
fn edge_flanges_turn_either_way_part_way_along_and_after_a_start_hem() {
    // A short flange turning away from the bends, not reaching any bend,
    // on a sheet with a rolled start hem that renumbers flanges inside.
    let mut sheet = channel();
    sheet.start_hem = Some(SheetMetalHem::Rolled {
        side: HemSide::Negative,
        inside_radius: length(1.0),
        angle_radians: ScalarExpr::Literal(Quantity::scalar(4.0)),
    });
    let mut lip = edge("lip", 0, SheetEdge::Origin, 5.0, 10.0);
    lip.angle_radians = ScalarExpr::Literal(Quantity::scalar(-2.0));
    sheet.edge_flanges.push(lip);
    let (folded, flat) = volumes(sheet.clone());
    close(folded, flat);
    let plain = channel();
    let (plain_folded, _) = volumes(plain);
    let hem = 4.0 * (1.0 + 1.0) * 10.0 * 2.0;
    close(folded, plain_folded + hem + (2.0 * 2.0 + 8.0) * 10.0 * 2.0);
    // On the blank the tab follows the hem's allowance.
    let metrics = sheet.flat_pattern(&HashMap::new(), 0.5).unwrap();
    let tab = &metrics.edge_tabs[0];
    assert!((tab.along_mm[0] - (8.0 + 5.0)).abs() < 1e-12, "{tab:?}");
    assert!(tab.across_mm[1] == 0.0 && tab.across_mm[0] < 0.0);
}

#[test]
fn edge_flanges_are_validated() {
    let check = |change: &dyn Fn(&mut SheetMetalDefinition), expected: &str| {
        let mut sheet = channel();
        change(&mut sheet);
        let error = sheet.evaluate(&HashMap::new()).err().unwrap().message;
        assert!(error.contains(expected), "{expected}: {error}");
    };
    check(
        &|s| s.edge_flanges.push(edge("a", 1, SheetEdge::Far, 5.0, 20.0)),
        "straight region",
    );
    check(
        &|s| s.edge_flanges.push(edge("a", 7, SheetEdge::Far, 0.0, 5.0)),
        "names flange 7",
    );
    check(
        &|s| {
            s.edge_flanges.push(edge("a", 1, SheetEdge::Far, 0.0, 10.0));
            s.edge_flanges.push(edge("b", 1, SheetEdge::Far, 5.0, 10.0));
        },
        "overlap",
    );
    check(
        &|s| {
            s.edge_flanges.push(edge("a", 1, SheetEdge::Far, 0.0, 10.0));
            s.edge_flanges
                .push(edge("a", 1, SheetEdge::Origin, 0.0, 10.0));
        },
        "unique",
    );
    check(
        &|s| {
            let mut a = edge("a", 1, SheetEdge::Far, 5.0, 10.0);
            a.relief_depth = Some(length(3.0));
            s.edge_flanges.push(a);
        },
        "reaches no bend",
    );
    check(
        &|s| {
            for (id, side) in [("a", SheetEdge::Far), ("b", SheetEdge::Origin)] {
                let mut a = edge(id, 1, side, 0.0, 10.0);
                a.relief_depth = Some(length(5.0));
                s.edge_flanges.push(a);
            }
        },
        "leave material",
    );
    check(
        &|s| {
            let mut a = edge("a", 1, SheetEdge::Far, 0.0, 10.0);
            a.angle_radians = ScalarExpr::Literal(Quantity::scalar(PI));
            s.edge_flanges.push(a);
        },
        "bend angle",
    );
    // JSON: the new fields round-trip and older sheets keep their form.
    let mut sheet = channel();
    let mut wall = edge("wall", 1, SheetEdge::Far, 0.0, 20.0);
    wall.relief_depth = Some(length(4.0));
    sheet.edge_flanges.push(wall);
    let json = serde_json::to_string(&sheet).unwrap();
    assert!(json.contains("\"edge\":\"far\"") && json.contains("relief_depth"));
    assert_eq!(
        serde_json::from_str::<SheetMetalDefinition>(&json).unwrap(),
        sheet
    );
    assert!(
        !serde_json::to_string(&channel())
            .unwrap()
            .contains("edge_flanges")
    );
}
