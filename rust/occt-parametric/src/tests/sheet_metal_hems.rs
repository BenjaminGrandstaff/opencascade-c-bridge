//! Closed and teardrop hems (schema 177): hems lying on their flange, built
//! as caps on the strip's outline, a teardrop's void as a hole through the
//! profile, checked by exact volumes against their blanks.
use super::*;

const PI: f64 = std::f64::consts::PI;

fn length(value: f64) -> ScalarExpr {
    ScalarExpr::Literal(Quantity::length(value, LengthUnit::Millimeter))
}

/// One flat 30 mm flange, 10 mm wide, 2 mm thick, starting at the origin
/// along +x; the sheet normal (positive side) is width × start = −z.
fn strip() -> SheetMetalDefinition {
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
        flanges: vec![length(30.0)],
        bends: Vec::new(),
        cutouts: Vec::new(),
        start_hem: None,
        end_hem: None,
        edge_flanges: Vec::new(),
    }
}

fn closed(side: HemSide, leaf: f64) -> SheetMetalHem {
    SheetMetalHem::Closed {
        side,
        length: length(leaf),
    }
}

fn teardrop(side: HemSide, radius: f64, angle: f64) -> SheetMetalHem {
    SheetMetalHem::Teardrop {
        side,
        inside_radius: length(radius),
        angle_radians: ScalarExpr::Literal(Quantity::scalar(angle)),
    }
}

/// Folded and flat (K = 0.5) volumes and the folded sheet's exact bounds;
/// both must be one valid solid.
fn build(sheet: SheetMetalDefinition) -> (f64, f64, occt_bridge::Bounds, usize) {
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
    let folded = generated.shape("folded").unwrap();
    (
        measure("folded"),
        measure("flat"),
        session.exact_bounds(folded).unwrap(),
        session.subshape_count(folded, ShapeType::Shell).unwrap(),
    )
}

fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= 1e-7 * expected,
        "{actual} vs {expected}"
    );
}

#[test]
fn closed_hems_lie_flat_on_their_flange_at_either_end_and_side() {
    // Flange, a 10 mm leaf and the fold's half circle of radius t.
    let expected = (30.0 + 10.0) * 10.0 * 2.0 + PI * 2.0 * 2.0 / 2.0 * 10.0;
    for (side, z) in [
        (HemSide::Positive, (-3.0, 1.0)),
        (HemSide::Negative, (-1.0, 3.0)),
    ] {
        let mut sheet = strip();
        sheet.end_hem = Some(closed(side, 10.0));
        let (folded, flat, bounds, _) = build(sheet);
        close(folded, expected);
        close(flat, expected);
        assert!(
            (bounds.max.x - 32.0).abs() < 1e-9 && bounds.min.x.abs() < 1e-9,
            "{bounds:?}"
        );
        assert!(
            (bounds.min.z - z.0).abs() < 1e-9 && (bounds.max.z - z.1).abs() < 1e-9,
            "{side:?} {bounds:?}"
        );
        let mut sheet = strip();
        sheet.start_hem = Some(closed(side, 10.0));
        let (folded, flat, bounds, _) = build(sheet);
        close(folded, expected);
        close(flat, expected);
        assert!(
            (bounds.min.x + 2.0).abs() < 1e-9 && (bounds.max.x - 30.0).abs() < 1e-9,
            "{bounds:?}"
        );
        assert!(
            (bounds.min.z - z.0).abs() < 1e-9 && (bounds.max.z - z.1).abs() < 1e-9,
            "{side:?} {bounds:?}"
        );
    }
}

#[test]
fn teardrop_hems_land_on_their_flange_around_a_void() {
    let (radius, angle) = (2.0, 1.6 * PI);
    let geometry = TeardropGeometry::new(radius, angle, 2.0).unwrap();
    // The flange, the roll at its midplane radius and the leg to its landing.
    let expected = (30.0 + angle * (radius + 1.0) + geometry.leg) * 10.0 * 2.0;
    for side in [HemSide::Positive, HemSide::Negative] {
        for start in [false, true] {
            let mut sheet = strip();
            let hem = Some(teardrop(side, radius, angle));
            if start {
                sheet.start_hem = hem;
            } else {
                sheet.end_hem = hem;
            }
            let (folded, flat, bounds, shells) = build(sheet);
            close(folded, expected);
            close(flat, expected);
            // The void runs through the width: one shell, enclosing nothing.
            assert_eq!(shells, 1);
            // The roll reaches its outside radius beyond the flange's end.
            let reach = radius + 2.0 + geometry.center[0];
            if start {
                assert!((bounds.min.x + reach).abs() < 1e-6, "{bounds:?}");
            } else {
                assert!((bounds.max.x - 30.0 - reach).abs() < 1e-6, "{bounds:?}");
            }
        }
    }
    // The leg lands behind the fold, within the flange.
    assert!(geometry.inner_landing[0] < 0.0 && geometry.covered() < 30.0);
}

#[test]
fn hems_lying_on_flanges_work_with_edge_flanges_and_reliefs() {
    // A tray floor with walls and reliefs, ending in a closed hem on its
    // last flange: the slab path keeps the hem in the chain holding it.
    let mut sheet = strip();
    sheet.flanges = vec![length(20.0); 3];
    sheet.bends = vec![
        SheetMetalBend {
            angle_radians: ScalarExpr::Literal(Quantity::scalar(PI / 2.0)),
            inside_radius: length(3.0),
        };
        2
    ];
    sheet.end_hem = Some(closed(HemSide::Negative, 8.0));
    for side in [SheetEdge::Origin, SheetEdge::Far] {
        sheet.edge_flanges.push(SheetMetalEdgeFlange {
            id: format!("{side:?}"),
            flange: 1,
            edge: side,
            start: length(0.0),
            length: length(20.0),
            angle_radians: ScalarExpr::Literal(Quantity::scalar(PI / 2.0)),
            inside_radius: length(1.0),
            height: length(8.0),
            relief_depth: Some(length(4.0)),
        });
    }
    let (folded, flat, _, _) = build(sheet);
    close(folded, flat);
}

#[test]
fn hems_lying_on_flanges_are_validated() {
    let check = |change: &dyn Fn(&mut SheetMetalDefinition), expected: &str| {
        let mut sheet = strip();
        change(&mut sheet);
        let error = sheet.evaluate(&HashMap::new()).err().unwrap().message;
        assert!(error.contains(expected), "{expected}: {error}");
    };
    check(
        &|s| s.end_hem = Some(closed(HemSide::Positive, 30.0)),
        "within its flange",
    );
    // Barely past half a turn, the leg runs back beyond the flange.
    check(
        &|s| s.end_hem = Some(teardrop(HemSide::Positive, 2.0, 1.01 * PI)),
        "within its flange",
    );
    check(
        &|s| s.end_hem = Some(teardrop(HemSide::Positive, 2.0, 1.99 * PI)),
        "angle must lie",
    );
    check(
        &|s| {
            s.end_hem = Some(closed(HemSide::Positive, 10.0));
            s.cutouts.push(SheetMetalCutout {
                id: "hole".into(),
                flange: 0,
                along: length(25.0),
                across: length(5.0),
                shape: SheetMetalCutoutShape::Circle {
                    diameter: length(2.0),
                },
            });
        },
        "under a closed or teardrop hem",
    );
    check(
        &|s| {
            s.end_hem = Some(closed(HemSide::Positive, 10.0));
            s.edge_flanges.push(SheetMetalEdgeFlange {
                id: "wall".into(),
                flange: 0,
                edge: SheetEdge::Far,
                start: length(10.0),
                length: length(20.0),
                angle_radians: ScalarExpr::Literal(Quantity::scalar(PI / 2.0)),
                inside_radius: length(1.0),
                height: length(5.0),
                relief_depth: Some(length(3.0)),
            });
        },
        "cannot be relieved",
    );
    let mut sheet = strip();
    sheet.end_hem = Some(teardrop(HemSide::Negative, 2.0, 1.6 * PI));
    sheet.start_hem = Some(closed(HemSide::Positive, 5.0));
    let json = serde_json::to_string(&sheet).unwrap();
    assert!(json.contains("\"kind\":\"teardrop\"") && json.contains("\"kind\":\"closed\""));
    assert_eq!(
        serde_json::from_str::<SheetMetalDefinition>(&json).unwrap(),
        sheet
    );
}
