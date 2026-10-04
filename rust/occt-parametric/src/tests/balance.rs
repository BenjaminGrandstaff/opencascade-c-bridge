use super::*;

fn mm(value: f64) -> Quantity {
    Quantity::length(value, LengthUnit::Millimeter)
}
fn station(span: f64, chord: f64, leading_x: f64) -> PlanformStation {
    PlanformStation {
        span: mm(span),
        chord: mm(chord),
        leading_edge: VectorQuantity::lengths(leading_x, span, 0.0, LengthUnit::Millimeter),
    }
}
fn near(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-8, "{actual} != {expected}");
}

#[test]
fn exact_mac_matches_rectangular_trapezoidal_and_triangular_panels() {
    for (tip, mac, span, x) in [
        (100.0, 100.0, 250.0, 100.0),
        (50.0, 700.0 / 9.0, 2000.0 / 9.0, 800.0 / 9.0),
        (0.0, 200.0 / 3.0, 500.0 / 3.0, 200.0 / 3.0),
    ] {
        let wing = SymmetricWingPlanform {
            stations: vec![station(0.0, 100.0, 0.0), station(500.0, tip, 200.0)],
        };
        let result = wing.mean_aerodynamic_chord().unwrap();
        near(result.length_mm, mac);
        near(result.leading_edge_mm.x, x);
        near(result.leading_edge_mm.y, span);
        near(result.planform_area_mm2, 500.0 * (100.0 + tip));
        let balance = result
            .reference(VectorQuantity::scalars(1.0, 0.0, 0.0))
            .locate(Vec3::new(x + 0.25 * mac, 0.0, 30.0))
            .unwrap();
        near(balance.chord_fraction, 0.25);
    }
}

#[test]
fn mac_is_invariant_under_panel_splitting_units_and_far_origin_translation() {
    let wing = SymmetricWingPlanform {
        stations: vec![station(0.0, 100.0, 0.0), station(500.0, 50.0, 200.0)],
    };
    let expected = wing.mean_aerodynamic_chord().unwrap();
    let mut split = wing.clone();
    split.stations.insert(1, station(200.0, 80.0, 80.0));
    for station in &mut split.stations {
        station.span = Quantity::length(station.span.value / 10.0, LengthUnit::Centimeter);
        station.chord = Quantity::length(station.chord.value / 10.0, LengthUnit::Centimeter);
        station.leading_edge.x.value += 1e9;
        station.leading_edge.z.value += 1e9;
    }
    let actual = split.mean_aerodynamic_chord().unwrap();
    near(actual.length_mm, expected.length_mm);
    near(actual.planform_area_mm2, expected.planform_area_mm2);
    assert!((actual.leading_edge_mm.x - 1e9 - expected.leading_edge_mm.x).abs() < 1e-6);
    near(actual.leading_edge_mm.z, 1e9);
}

#[test]
fn arbitrary_chord_axes_and_outside_chord_cg_are_reported_without_clamping() {
    let reference = ChordReference {
        leading_edge: VectorQuantity::lengths(1e9, 2e9, 3e9, LengthUnit::Millimeter),
        direction: VectorQuantity::scalars(0.0, 0.0, -1e-300),
        length: Quantity::length(10.0, LengthUnit::Centimeter),
    };
    near(
        reference
            .locate(Vec3::new(1e9, 2e9, 3e9 - 25.0))
            .unwrap()
            .chord_fraction,
        0.25,
    );
    near(
        reference
            .locate(Vec3::new(1e9, 2e9, 3e9 + 50.0))
            .unwrap()
            .chord_fraction,
        -0.5,
    );
    near(
        reference
            .locate(Vec3::new(1e9, 2e9, 3e9 - 200.0))
            .unwrap()
            .chord_fraction,
        2.0,
    );
    let mut invalid = reference;
    invalid.direction = VectorQuantity::scalars(0.0, 0.0, 0.0);
    assert!(invalid.locate(Vec3::new(0.0, 0.0, 0.0)).is_err());
    invalid = reference;
    invalid.length = Quantity::scalar(1.0);
    assert!(invalid.locate(Vec3::new(0.0, 0.0, 0.0)).is_err());
    invalid = reference;
    invalid.length = mm(0.0);
    assert!(invalid.locate(Vec3::new(0.0, 0.0, 0.0)).is_err());
    assert!(
        reference
            .locate(Vec3::new(f64::INFINITY, 0.0, 0.0))
            .is_err()
    );
}

#[test]
fn malformed_and_unrepresentable_planforms_fail_without_kernel_work() {
    let valid = SymmetricWingPlanform {
        stations: vec![station(0.0, 100.0, 0.0), station(500.0, 50.0, 200.0)],
    };
    let mut cases = vec![SymmetricWingPlanform { stations: vec![] }];
    let mut add = |edit: fn(&mut SymmetricWingPlanform)| {
        let mut wing = valid.clone();
        edit(&mut wing);
        cases.push(wing);
    };
    add(|wing| wing.stations[0].span = mm(1.0));
    add(|wing| wing.stations[1].span = mm(0.0));
    add(|wing| wing.stations[1].span = Quantity::scalar(10.0));
    add(|wing| wing.stations[1].chord = mm(-1.0));
    add(|wing| wing.stations[1].chord = mm(f64::NAN));
    add(|wing| wing.stations[1].leading_edge.x = Quantity::scalar(0.0));
    add(|wing| {
        wing.stations[0].chord = mm(0.0);
        wing.stations[1].chord = mm(0.0);
    });
    add(|wing| wing.stations[1].span = mm(f64::MAX));
    for wing in cases {
        assert!(wing.mean_aerodynamic_chord().is_err(), "{wing:?}");
    }
    // c² would overflow, but the normalized panel formula remains usable.
    let huge = SymmetricWingPlanform {
        stations: vec![station(0.0, 1e200, 0.0), station(1e-100, 1e200, 0.0)],
    };
    let mac = huge.mean_aerodynamic_chord().unwrap();
    assert!((mac.length_mm / 1e200 - 1.0).abs() < 1e-14);
}

#[test]
fn material_totals_reuse_measurements_and_preserve_central_inertia_at_large_origins() {
    let mut family = family(RequirementPriority::Advisory, 1e12);
    family.requirements.clear();
    let mut graph = InstanceGraph::new(&family);
    for (id, density) in [("light", 1000.0), ("heavy", 2000.0)] {
        graph
            .add_material(Material {
                id: id.into(),
                name: id.into(),
                density_kg_per_cubic_meter: density,
            })
            .unwrap();
    }
    graph.add_base("base", HashMap::new(), "test").unwrap();
    graph.assign_material("base", Some("light")).unwrap();
    let mut outputs = vec![];
    for (id, x, material) in [
        ("first", 0.0, None),
        ("second", 20.0, None),
        ("third", 40.0, Some("heavy")),
    ] {
        graph.add_clone(id, "base", HashMap::new(), "test").unwrap();
        graph
            .set_placement(
                id,
                Placement::translated(VectorQuantity::lengths(
                    1e9 + x,
                    0.0,
                    0.0,
                    LengthUnit::Millimeter,
                )),
            )
            .unwrap();
        if let Some(material) = material {
            graph.assign_material(id, Some(material)).unwrap();
        }
        outputs.push(InstanceOutputRef {
            instance: id.into(),
            output: "body".into(),
        });
    }
    let accepted = ModelDocument::from_graph(&graph);
    let session = Session::new().unwrap();
    let report = graph.mass_properties(&session, &outputs).unwrap();
    assert_eq!(report.generated_variants, 1);
    let grouped = report.material_totals().unwrap();
    assert_eq!(
        grouped
            .iter()
            .map(|group| group.material.as_str())
            .collect::<Vec<_>>(),
        vec!["heavy", "light"]
    );
    assert_eq!(grouped[1].components, 2);
    near(grouped[1].properties.mass_kg, 0.012);
    near(grouped[1].properties.center_mm.x, 1e9 + 15.0);
    near(grouped[1].properties.inertia_kg_mm2[1][1], 2.2);
    near(
        grouped.iter().map(|group| group.properties.mass_kg).sum(),
        report.total.mass_kg,
    );
    let reference = ChordReference {
        leading_edge: VectorQuantity::lengths(1e9, 0.0, 0.0, LengthUnit::Millimeter),
        direction: VectorQuantity::scalars(1.0, 0.0, 0.0),
        length: mm(100.0),
    };
    near(report.balance(reference).unwrap().chord_fraction, 0.3);
    assert_eq!(ModelDocument::from_graph(&graph), accepted);
    assert_eq!(session.shape_count().unwrap(), 0);
    let mut invalid = report.clone();
    invalid.components.clear();
    assert!(invalid.material_totals().is_err());
    invalid = report.clone();
    invalid.components[0].properties.mass_kg = -1.0;
    assert!(invalid.material_totals().is_err());
    invalid = report;
    invalid.total.mass_kg = 0.0;
    assert!(invalid.balance(reference).is_err());
}
