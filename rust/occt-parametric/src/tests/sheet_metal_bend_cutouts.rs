//! Cutouts across bends (schema 178): pieces on flanges cut square to them,
//! pieces in bends cut radially, and the blank stretched exactly through
//! each bend's zone.
use super::*;

const PI: f64 = std::f64::consts::PI;
const HALF: f64 = std::f64::consts::FRAC_PI_2;

fn length(value: f64) -> ScalarExpr {
    ScalarExpr::Literal(Quantity::length(value, LengthUnit::Millimeter))
}

/// Two 20 mm flanges and a quarter bend at inside radius 3 (midplane arc
/// 2π), 10 mm wide and 2 mm thick.
fn angle() -> SheetMetalDefinition {
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
        flanges: vec![length(20.0); 2],
        bends: vec![SheetMetalBend {
            angle_radians: ScalarExpr::Literal(Quantity::scalar(HALF)),
            inside_radius: length(3.0),
        }],
        cutouts: Vec::new(),
        start_hem: None,
        end_hem: None,
        edge_flanges: Vec::new(),
    }
}

fn hole(id: &str, along: f64, diameter: f64) -> SheetMetalCutout {
    SheetMetalCutout {
        id: id.into(),
        flange: 0,
        along: length(along),
        across: length(5.0),
        shape: SheetMetalCutoutShape::Circle {
            diameter: length(diameter),
        },
    }
}

/// Folded and flat volumes at neutral factor `k`; both one valid solid.
fn build(sheet: SheetMetalDefinition, k: f64) -> (f64, f64) {
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
                neutral_factor: ScalarExpr::Literal(Quantity::scalar(k)),
                bend_table: None,
            },
        },
    ];
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
    (measure("folded"), measure("flat"))
}

fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= 1e-6 * expected,
        "{actual} vs {expected}"
    );
}

#[test]
fn cutouts_crossing_bends_remove_their_area_times_the_thickness() {
    let plain = (40.0 + 2.0 * PI) * 10.0 * 2.0;
    // Inside the bend, across a tangent line, and a slot spanning the bend
    // into both flanges: each removes its developed area × 2 mm, folded
    // and flat alike at K = 0.5.
    let middle = 20.0 + PI;
    let slot = SheetMetalCutout {
        id: "slot".into(),
        flange: 0,
        along: length(middle),
        across: length(5.0),
        shape: SheetMetalCutoutShape::Slot {
            length: length(16.0),
            width: length(3.0),
        },
    };
    let slot_area = 13.0 * 3.0 + PI * 1.5 * 1.5;
    for (cutout, area) in [
        (hole("inside", middle, 4.0), PI * 4.0),
        (hole("tangent", 19.0, 4.0), PI * 4.0),
        (slot, slot_area),
    ] {
        let mut sheet = angle();
        let id = cutout.id.clone();
        sheet.cutouts.push(cutout);
        let (folded, flat) = build(sheet, 0.5);
        close(folded, plain - area * 2.0);
        close(flat, plain - area * 2.0);
        let _ = id;
    }
}

#[test]
fn blanks_stretch_a_crossing_cutout_through_the_bend_zone() {
    // At K = 0.3 the bend's allowance is (π/2)(3 + 0.6) against its 2π
    // midplane arc: the hole inside the bend becomes an ellipse that much
    // longer along the blank, while the folded hole is unchanged.
    let k = 0.3;
    let allowance = HALF * (3.0 + k * 2.0);
    let stretch = allowance / (2.0 * PI);
    let mut sheet = angle();
    sheet.cutouts.push(hole("inside", 20.0 + PI, 4.0));
    let (folded, flat) = build(sheet.clone(), k);
    close(folded, (40.0 + 2.0 * PI) * 20.0 - PI * 4.0 * 2.0);
    close(flat, (40.0 + allowance) * 20.0 - PI * 4.0 * stretch * 2.0);
    let metrics = sheet.flat_pattern(&HashMap::new(), k).unwrap();
    let cutout = &metrics.cutouts[0];
    assert!(
        cutout
            .mapped
            .iter()
            .any(|s| matches!(s, FlatSegment::EllipseArc { .. }))
    );
    assert!((cutout.center_mm[0] - (20.0 + allowance * 0.5)).abs() < 1e-12);
    // The drawing outlines it from its mapped pieces.
    let drawing = metrics.drawing("angle").unwrap();
    assert_eq!(drawing.polylines.len(), 1 + 1 + 1);
}

#[test]
fn cutouts_across_bends_are_validated() {
    let check = |change: &dyn Fn(&mut SheetMetalDefinition), expected: &str| {
        let mut sheet = angle();
        change(&mut sheet);
        let error = sheet.evaluate(&HashMap::new()).err().unwrap().message;
        assert!(error.contains(expected), "{expected}: {error}");
    };
    check(&|s| s.cutouts.push(hole("off", 1.0, 4.0)), "on the sheet");
    check(
        &|s| s.cutouts.push(hole("off", 40.0 + 2.0 * PI - 1.0, 4.0)),
        "on the sheet",
    );
    check(
        &|s| {
            s.cutouts.push(hole("under", 20.0 + 2.0 * PI + 17.0, 4.0));
            s.end_hem = Some(SheetMetalHem::Closed {
                side: HemSide::Positive,
                length: length(5.0),
            });
        },
        "under a closed or teardrop hem",
    );
    check(
        &|s| {
            let mut near = hole("near", 20.0 + PI, 4.0);
            near.across = length(7.0);
            s.cutouts.push(near);
            s.edge_flanges.push(SheetMetalEdgeFlange {
                id: "wall".into(),
                flange: 0,
                edge: SheetEdge::Far,
                start: length(10.0),
                length: length(10.0),
                angle_radians: ScalarExpr::Literal(Quantity::scalar(HALF)),
                inside_radius: length(1.0),
                height: length(5.0),
                relief_depth: Some(length(4.0)),
            });
        },
        "runs into a bend relief",
    );
}
