use super::*;
fn mm(value: f64) -> Quantity {
    Quantity::length(value, LengthUnit::Millimeter)
}
fn inch(value: f64) -> Quantity {
    Quantity::length(value, LengthUnit::Inch)
}
#[test]
fn frozen_cut_taps_keep_published_sizes_and_units() {
    for (nominal, pitch, expected) in [
        (1.6, 0.35, 1.25),
        (2.5, 0.45, 2.05),
        (6.0, 1.0, 5.0),
        (8.0, 1.25, 6.8),
        (10.0, 1.25, 8.8),
        (27.0, 2.0, 25.0),
    ] {
        assert_eq!(
            carr_lane_tap_drill_v1(HoleCatalogSystem::Metric, mm(nominal), mm(pitch)).unwrap(),
            mm(expected)
        );
    }
    for (nominal, tpi, expected) in [
        (0.06, 80.0, 3.0 / 64.0),
        (0.112, 40.0, 0.089),
        (0.19, 32.0, 0.159),
        (0.25, 20.0, 0.201),
        (0.3125, 18.0, 0.257),
        (0.5, 13.0, 27.0 / 64.0),
        (1.125, 12.0, 1.0 + 3.0 / 64.0),
    ] {
        let actual =
            carr_lane_tap_drill_v1(HoleCatalogSystem::Inch, inch(nominal), inch(1.0 / tpi))
                .unwrap();
        assert!((actual.normalized().unwrap() - expected * 25.4).abs() < 1e-12);
    }
    assert_eq!(
        carr_lane_tap_drill_v1(
            HoleCatalogSystem::Metric,
            Quantity::length(0.006, LengthUnit::Meter),
            Quantity::length(0.1, LengthUnit::Centimeter)
        )
        .unwrap(),
        mm(5.0)
    );
}
#[test]
fn socket_recesses_preserve_manufacturer_precision_and_fit() {
    let metric = carr_lane_socket_head_v1(HoleCatalogSystem::Metric, mm(6.0)).unwrap();
    assert_eq!(
        metric,
        SocketHeadRecess {
            counterbore_diameter: mm(11.2),
            counterbore_depth: mm(6.0),
            normal_clearance: mm(6.8),
            close_clearance: mm(6.4),
            countersink_relief: mm(6.8),
        }
    );
    let imperial = carr_lane_socket_head_v1(HoleCatalogSystem::Inch, inch(5.0 / 16.0)).unwrap();
    // Depth is the published 0.312, deliberately not nominal 0.3125.
    assert_eq!(imperial.counterbore_depth, mm(0.312 * 25.4));
    for (dimension, expected) in [
        (SocketHeadDimension::CounterboreDiameter, 17.0 / 32.0),
        (SocketHeadDimension::CounterboreDepth, 0.312),
        (SocketHeadDimension::NormalClearance, 11.0 / 32.0),
        (SocketHeadDimension::CloseClearance, 21.0 / 64.0),
    ] {
        assert_eq!(
            carr_lane_socket_dimension_v1(HoleCatalogSystem::Inch, inch(0.3125), dimension)
                .unwrap(),
            mm(expected * 25.4)
        );
        assert!(
            carr_lane_socket_dimension_v1(HoleCatalogSystem::Metric, mm(6.0), dimension)
                .unwrap()
                .value
                > 0.0
        );
    }
    let number = carr_lane_socket_head_v1(HoleCatalogSystem::Inch, inch(0.06)).unwrap();
    assert_eq!(number.normal_clearance, mm(0.073 * 25.4));
    assert_eq!(number.close_clearance, mm(0.067 * 25.4));
}
#[test]
fn catalogs_reject_unknown_pairs_and_wrong_dimensions() {
    for nominal in [
        Quantity::scalar(6.0),
        mm(0.0),
        mm(-6.0),
        mm(f64::NAN),
        Quantity::length(f64::MAX, LengthUnit::Meter),
        mm(6.00001),
        mm(7.0),
    ] {
        assert!(carr_lane_socket_head_v1(HoleCatalogSystem::Metric, nominal).is_err());
        assert!(carr_lane_tap_drill_v1(HoleCatalogSystem::Metric, nominal, mm(1.0)).is_err());
    }
    for pitch in [
        Quantity::scalar(1.0),
        mm(0.0),
        mm(-1.0),
        mm(f64::INFINITY),
        mm(0.9),
    ] {
        assert!(carr_lane_tap_drill_v1(HoleCatalogSystem::Metric, mm(6.0), pitch).is_err());
    }
    assert!(carr_lane_tap_drill_v1(HoleCatalogSystem::Inch, inch(0.25), mm(20.0)).is_err());
}
fn catalog_family() -> FamilyDefinition {
    let mut family = family(RequirementPriority::Advisory, 1e9);
    family.requirements.clear();
    family.features.clear();
    family.parameters = vec![
        length_parameter("nominal", 6.0),
        length_parameter("pitch", 1.0),
    ];
    family.derived_parameters = vec![DerivedParameterDefinition {
        id: "drill".into(),
        dimension: Dimension::Length,
        expression: ScalarExpr::CarrLaneTapDrillV1 {
            nominal_diameter: Box::new(ScalarExpr::Parameter("nominal".into())),
            pitch: Box::new(ScalarExpr::Parameter("pitch".into())),
            system: HoleCatalogSystem::Metric,
        },
    }];
    let position =
        |x, y, z| VectorExpr::Literal(VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter));
    let socket = |dimension| ScalarExpr::CarrLaneSocketHeadV1 {
        nominal_diameter: Box::new(ScalarExpr::Parameter("nominal".into())),
        system: HoleCatalogSystem::Metric,
        dimension,
    };
    family.features = vec![
        FeatureDefinition {
            id: "body".into(),
            operation: FeatureOperation::Box {
                origin: position(0.0, 0.0, 0.0),
                size: position(40.0, 40.0, 20.0),
            },
        },
        FeatureDefinition {
            id: "tap".into(),
            operation: FeatureOperation::Hole {
                bottom: HoleBottom::Flat,
                input: "body".into(),
                position: position(10.0, 10.0, 20.0),
                axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, -1.0)),
                diameter: ScalarExpr::Parameter("drill".into()),
                extent: HoleExtent::ThroughAll,
                finish: HoleFinish::Plain,
                thread: None,
            },
        },
        FeatureDefinition {
            id: "recess".into(),
            operation: FeatureOperation::Hole {
                bottom: HoleBottom::Flat,
                input: "tap".into(),
                position: position(30.0, 30.0, 20.0),
                axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, -1.0)),
                diameter: socket(SocketHeadDimension::NormalClearance),
                extent: HoleExtent::ThroughAll,
                finish: HoleFinish::Counterbore {
                    diameter: socket(SocketHeadDimension::CounterboreDiameter),
                    depth: socket(SocketHeadDimension::CounterboreDepth),
                    relief: None,
                },
                thread: None,
            },
        },
    ];
    family
}
#[test]
fn catalog_expressions_drive_exact_holes_edits_and_round_trips() {
    let session = Session::new().unwrap();
    let mut family = catalog_family();
    // Evaluate both catalog kinds through derived and resolved expressions.
    family.derived_parameters.push(DerivedParameterDefinition {
        id: "clearance".into(),
        dimension: Dimension::Length,
        expression: ScalarExpr::CarrLaneSocketHeadV1 {
            nominal_diameter: Box::new(ScalarExpr::Parameter("nominal".into())),
            system: HoleCatalogSystem::Metric,
            dimension: SocketHeadDimension::NormalClearance,
        },
    });
    let resolved = resolve_parameters(&family, &HashMap::new()).unwrap();
    assert_eq!(resolved["clearance"], ParameterValue::Scalar(mm(6.8)));
    let direct = ScalarExpr::CarrLaneTapDrillV1 {
        nominal_diameter: Box::new(ScalarExpr::Parameter("nominal".into())),
        pitch: Box::new(ScalarExpr::Parameter("pitch".into())),
        system: HoleCatalogSystem::Metric,
    };
    assert_eq!(scalar(&direct, &resolved, Dimension::Length).unwrap(), 5.0);
    let mut part = PartInstance {
        id: "part".into(),
        definition: &family,
        overrides: HashMap::new(),
        provenance: "test".into(),
    };
    let first = part.regenerate(&session).unwrap();
    let expected = 32000.0
        - std::f64::consts::PI / 4.0
            * (5.0_f64.powi(2) * 20.0 + 6.8_f64.powi(2) * 14.0 + 11.2_f64.powi(2) * 6.0);
    assert!((session.volume(first.shape("recess").unwrap()).unwrap() - expected).abs() < 1e-7);
    part.overrides
        .insert("nominal".into(), ParameterValue::Scalar(mm(8.0)));
    part.overrides
        .insert("pitch".into(), ParameterValue::Scalar(mm(1.25)));
    let second = part.regenerate_incremental(&session, &first).unwrap();
    assert_eq!(second.regeneration.reused, vec!["body"]);
    assert_eq!(second.regeneration.rebuilt, vec!["tap", "recess"]);
    part.overrides
        .insert("pitch".into(), ParameterValue::Scalar(mm(1.2)));
    assert!(part.regenerate_incremental(&session, &second).is_err());
    assert!(session.is_valid(second.shape("recess").unwrap()).unwrap());
    let mut graph = InstanceGraph::new(&family);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let document = ModelDocument::from_graph(&graph);
    assert_eq!(
        ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap(),
        document
    );
    drop((first, second));
    assert_eq!(session.shape_count().unwrap(), 0);
}
#[test]
fn countersink_reliefs_match_column_c_and_clear_every_screw() {
    // Spot checks of printed column C.
    for (nominal, expected) in [(1.6, 2.0), (8.0, 9.2), (12.0, 14.2), (48.0, 52.6)] {
        let relief = carr_lane_socket_dimension_v1(
            HoleCatalogSystem::Metric,
            mm(nominal),
            SocketHeadDimension::CountersinkRelief,
        )
        .unwrap();
        assert_eq!(relief, mm(expected));
    }
    for (nominal, expected) in [(0.06, 0.074), (0.19, 0.218), (0.3125, 0.346), (2.0, 2.18)] {
        let relief = carr_lane_socket_dimension_v1(
            HoleCatalogSystem::Inch,
            inch(nominal),
            SocketHeadDimension::CountersinkRelief,
        )
        .unwrap();
        assert_eq!(relief, mm(expected * 25.4));
    }
    // Every relief clears its screw by 2-31% (M2 is 2.6 mm) and fits inside
    // its counterbore,
    // which catches a misread digit.
    let metric = [
        1.6, 2.0, 2.5, 3.0, 4.0, 5.0, 6.0, 8.0, 10.0, 12.0, 14.0, 16.0, 20.0, 24.0, 30.0, 36.0,
        42.0, 48.0,
    ];
    let inches = [
        0.06, 0.086, 0.112, 0.125, 0.138, 0.164, 0.19, 0.25, 0.3125, 0.375, 0.4375, 0.5, 0.625,
        0.75, 0.875, 1.0, 1.25, 1.5, 1.75, 2.0,
    ];
    for (system, sizes, unit) in [
        (HoleCatalogSystem::Metric, metric.as_slice(), 1.0),
        (HoleCatalogSystem::Inch, inches.as_slice(), 25.4),
    ] {
        let mut last = 0.0;
        for nominal in sizes {
            let recess = carr_lane_socket_head_v1(system, mm(nominal * unit)).unwrap();
            let relief = recess.countersink_relief.normalized().unwrap();
            let ratio = relief / (nominal * unit);
            assert!((1.02..1.31).contains(&ratio), "{nominal}: {ratio}");
            assert!(relief < recess.counterbore_diameter.normalized().unwrap());
            assert!(relief > last);
            last = relief;
        }
    }
}
#[test]
fn counterbore_reliefs_cut_a_sixty_degree_cone_at_the_floor() {
    let session = Session::new().unwrap();
    let socket = |dimension| ScalarExpr::CarrLaneSocketHeadV1 {
        nominal_diameter: Box::new(ScalarExpr::Parameter("nominal".into())),
        system: HoleCatalogSystem::Metric,
        dimension,
    };
    let volume_with = |clearance, relief: Option<ScalarExpr>| {
        let mut family = catalog_family();
        let FeatureOperation::Hole {
            diameter,
            finish: HoleFinish::Counterbore { relief: slot, .. },
            ..
        } = &mut family.features[2].operation
        else {
            unreachable!()
        };
        *diameter = socket(clearance);
        *slot = relief;
        let part = PartInstance {
            id: "part".into(),
            definition: &family,
            overrides: HashMap::new(),
            provenance: "test".into(),
        };
        part.regenerate(&session).map(|generated| {
            let shape = generated.shape("recess").unwrap();
            assert!(session.is_valid(shape).unwrap());
            session.volume(shape).unwrap()
        })
    };
    let relief = Some(socket(SocketHeadDimension::CountersinkRelief));
    // M6 close fit: a 6.4 mm bore under a 6.8 mm relief, so the cone is
    // 0.2 / tan 30° deep below the 6 mm deep counterbore.
    let plain = volume_with(SocketHeadDimension::CloseClearance, None).unwrap();
    let relieved = volume_with(SocketHeadDimension::CloseClearance, relief.clone()).unwrap();
    let (big, small) = (3.4_f64, 3.2_f64);
    let depth = (big - small) / (std::f64::consts::PI / 6.0).tan();
    let cone = std::f64::consts::PI * depth / 3.0 * (big * big + big * small + small * small)
        - std::f64::consts::PI * small * small * depth;
    assert!(
        (plain - relieved - cone).abs() < 1e-7,
        "{}",
        plain - relieved
    );
    // M6 normal fit's 6.8 mm bore already clears the 6.8 mm relief.
    let normal = volume_with(SocketHeadDimension::NormalClearance, None).unwrap();
    assert_eq!(
        volume_with(SocketHeadDimension::NormalClearance, relief).unwrap(),
        normal
    );
    // A relief as wide as the counterbore, or a negative one, is rejected.
    let mm_expr = |v| ScalarExpr::Literal(Quantity::length(v, LengthUnit::Millimeter));
    for bad in [11.2, -1.0] {
        assert!(volume_with(SocketHeadDimension::CloseClearance, Some(mm_expr(bad))).is_err());
    }
    assert_eq!(session.shape_count().unwrap(), 0);
}
