use super::*;
use std::f64::consts::PI;

fn mm(value: f64) -> ScalarExpr {
    ScalarExpr::Literal(Quantity::length(value, LengthUnit::Millimeter))
}
fn point(x: f64, y: f64, z: f64) -> VectorExpr {
    VectorExpr::Literal(VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter))
}
fn round_family() -> FamilyDefinition {
    let mut family = family(RequirementPriority::Advisory, 1e12);
    family.requirements.clear();
    family.features.clear();
    family.parameters.clear();
    for (id, value) in [
        ("base", 3.0),
        ("top", 1.0),
        ("height", 8.0),
        ("radius", 4.0),
    ] {
        family.parameters.push(ParameterDefinition {
            id: id.into(),
            parameter_type: ParameterType::Scalar(Dimension::Length),
            default: ParameterValue::Scalar(Quantity::length(value, LengthUnit::Millimeter)),
            minimum: None,
            maximum: None,
        });
    }
    for (id, dimension, value) in [
        (
            "origin",
            Dimension::Length,
            VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
        ),
        (
            "axis",
            Dimension::Scalar,
            VectorQuantity::scalars(1.0, 0.0, 0.0),
        ),
        (
            "center",
            Dimension::Length,
            VectorQuantity::lengths(2.0, 3.0, 4.0, LengthUnit::Centimeter),
        ),
    ] {
        family.parameters.push(ParameterDefinition {
            id: id.into(),
            parameter_type: ParameterType::Vector(dimension),
            default: ParameterValue::Vector(value),
            minimum: None,
            maximum: None,
        });
    }
    family.features = vec![
        FeatureDefinition {
            id: "sphere".into(),
            operation: FeatureOperation::Sphere {
                center: VectorExpr::Parameter("center".into()),
                radius: ScalarExpr::Parameter("radius".into()),
            },
        },
        FeatureDefinition {
            id: "sphere-moved".into(),
            operation: FeatureOperation::Translate {
                input: "sphere".into(),
                offset: point(10.0, 20.0, 30.0),
            },
        },
        // Dependency order is intentionally reversed here.
        FeatureDefinition {
            id: "cone-moved".into(),
            operation: FeatureOperation::Translate {
                input: "cone".into(),
                offset: point(0.0, 20.0, 0.0),
            },
        },
        FeatureDefinition {
            id: "cone".into(),
            operation: FeatureOperation::Cone {
                origin: VectorExpr::Parameter("origin".into()),
                axis: VectorExpr::Parameter("axis".into()),
                base_radius: ScalarExpr::Parameter("base".into()),
                top_radius: ScalarExpr::Parameter("top".into()),
                height: ScalarExpr::Parameter("height".into()),
            },
        },
    ];
    family
}
fn part(definition: &FamilyDefinition) -> PartInstance<'_> {
    PartInstance {
        id: "round".into(),
        definition,
        overrides: HashMap::new(),
        provenance: "test".into(),
    }
}
fn close(value: f64, expected: f64) {
    assert!(
        (value - expected).abs() <= 1e-7 * expected.abs().max(1e-12),
        "{value} vs {expected}"
    );
}

#[test]
fn round_primitives_have_analytic_volume_centroids_and_transform_history() {
    let family = round_family();
    let session = Session::new().unwrap();
    let generated = part(&family).regenerate(&session).unwrap();
    let cone = generated.shape("cone").unwrap();
    let sphere = generated.shape("sphere").unwrap();
    close(session.volume(cone).unwrap(), PI * 8.0 * 13.0 / 3.0);
    close(
        session.volume(sphere).unwrap(),
        4.0 * PI * 4.0_f64.powi(3) / 3.0,
    );
    close(session.center_of_mass(cone).unwrap().x, 8.0 * 18.0 / 52.0);
    let center = session.center_of_mass(sphere).unwrap();
    close(center.x, 20.0);
    close(center.y, 30.0);
    close(center.z, 40.0);
    let moved = generated.shape("sphere-moved").unwrap();
    let moved_center = session.center_of_mass(moved).unwrap();
    close(moved_center.x, 30.0);
    close(moved_center.y, 50.0);
    close(moved_center.z, 70.0);
    for (source, target) in [
        (cone, generated.shape("cone-moved").unwrap()),
        (sphere, moved),
    ] {
        assert!(session.is_valid(source).unwrap());
        assert_eq!(session.shape_type(source).unwrap(), ShapeType::Solid);
        let face = session.subshape(source, ShapeType::Face, 0).unwrap();
        assert_eq!(
            session
                .history_count(target, &face, HistoryRelation::Modified)
                .unwrap(),
            1
        );
    }
    drop(generated);
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn cone_limits_units_extreme_directions_and_distant_scales_are_supported() {
    let family = round_family();
    let session = Session::new().unwrap();
    for scale in [0.001, 1.0, 1000.0] {
        for (base, top) in [(3.0, 1.0), (0.0, 3.0), (3.0, 0.0), (3.0, 3.0)] {
            for magnitude in [1e-300, 1e300] {
                let mut part = part(&family);
                for (id, value) in [
                    ("base", base * scale),
                    ("top", top * scale),
                    ("height", 8.0 * scale),
                    ("radius", 4.0 * scale),
                ] {
                    part.overrides.insert(
                        id.into(),
                        ParameterValue::Scalar(Quantity::length(
                            value / 10.0,
                            LengthUnit::Centimeter,
                        )),
                    );
                }
                part.overrides.insert(
                    "origin".into(),
                    ParameterValue::Vector(VectorQuantity::lengths(
                        1e6,
                        -1e6,
                        1e6,
                        LengthUnit::Millimeter,
                    )),
                );
                part.overrides.insert(
                    "center".into(),
                    ParameterValue::Vector(VectorQuantity::lengths(
                        1e6,
                        -1e6,
                        1e6,
                        LengthUnit::Millimeter,
                    )),
                );
                part.overrides.insert(
                    "axis".into(),
                    ParameterValue::Vector(VectorQuantity::scalars(magnitude, 0.0, 0.0)),
                );
                let generated = part.regenerate(&session).unwrap();
                let cone = generated.shape("cone").unwrap();
                close(
                    session.volume(cone).unwrap(),
                    PI * 8.0 * (base * base + base * top + top * top) * scale.powi(3) / 3.0,
                );
                close(
                    session.volume(generated.shape("sphere").unwrap()).unwrap(),
                    4.0 * PI * (4.0 * scale).powi(3) / 3.0,
                );
                let bounds = session.exact_bounds(cone).unwrap();
                assert!((bounds.min.x - 1e6).abs() < 1e-6);
                assert!((bounds.max.x - 1e6 - 8.0 * scale).abs() < 1e-6);
                assert!(session.is_valid(cone).unwrap());
                drop(generated);
                assert_eq!(session.shape_count().unwrap(), 0);
            }
        }
    }
}

#[test]
fn every_round_primitive_parameter_drives_rebuilds_and_failed_edits_retain_results() {
    let family = round_family();
    let session = Session::new().unwrap();
    let mut managed = ManagedPartInstance::new(&session, part(&family));
    managed.regenerate().unwrap();
    for (id, value, expected) in [
        (
            "radius",
            ParameterValue::Scalar(Quantity::length(5.0, LengthUnit::Millimeter)),
            vec!["sphere", "sphere-moved"],
        ),
        (
            "center",
            ParameterValue::Vector(VectorQuantity::lengths(
                20.0,
                40.0,
                50.0,
                LengthUnit::Millimeter,
            )),
            vec!["sphere", "sphere-moved"],
        ),
        (
            "origin",
            ParameterValue::Vector(VectorQuantity::lengths(
                1.0,
                2.0,
                3.0,
                LengthUnit::Millimeter,
            )),
            vec!["cone", "cone-moved"],
        ),
        (
            "axis",
            ParameterValue::Vector(VectorQuantity::scalars(0.0, 1.0, 0.0)),
            vec!["cone", "cone-moved"],
        ),
        (
            "base",
            ParameterValue::Scalar(Quantity::length(4.0, LengthUnit::Millimeter)),
            vec!["cone", "cone-moved"],
        ),
        (
            "top",
            ParameterValue::Scalar(Quantity::length(2.0, LengthUnit::Millimeter)),
            vec!["cone", "cone-moved"],
        ),
        (
            "height",
            ParameterValue::Scalar(Quantity::length(10.0, LengthUnit::Millimeter)),
            vec!["cone", "cone-moved"],
        ),
    ] {
        managed.instance_mut().overrides.insert(id.into(), value);
        managed.regenerate().unwrap();
        let mut rebuilt = managed.accepted().unwrap().regeneration.rebuilt.clone();
        rebuilt.sort();
        assert_eq!(rebuilt, expected);
        assert_eq!(managed.accepted().unwrap().regeneration.reused.len(), 2);
        assert_eq!(session.shape_count().unwrap(), 4);
    }
    let revision = managed.accepted_revision();
    let volume = session
        .volume(managed.accepted().unwrap().shape("sphere").unwrap())
        .unwrap();
    managed.instance_mut().overrides.insert(
        "radius".into(),
        ParameterValue::Scalar(Quantity::length(6.0, LengthUnit::Millimeter)),
    );
    managed.instance_mut().overrides.insert(
        "height".into(),
        ParameterValue::Scalar(Quantity::length(0.0, LengthUnit::Millimeter)),
    );
    assert!(managed.regenerate().is_err());
    assert_eq!(managed.state(), RegenerationState::Stale);
    assert_eq!(managed.accepted_revision(), revision);
    assert_eq!(
        session
            .volume(managed.accepted().unwrap().shape("sphere").unwrap())
            .unwrap(),
        volume
    );
    assert_eq!(session.shape_count().unwrap(), 4);
    managed.instance_mut().overrides.insert(
        "height".into(),
        ParameterValue::Scalar(Quantity::length(10.0, LengthUnit::Millimeter)),
    );
    for i in 0..10 {
        managed.instance_mut().overrides.insert(
            "radius".into(),
            ParameterValue::Scalar(Quantity::length(6.0 + i as f64, LengthUnit::Millimeter)),
        );
        managed.regenerate().unwrap();
        assert_eq!(session.shape_count().unwrap(), 4);
    }
    drop(managed);
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn round_features_persist_migrate_merge_and_reject_bad_geometry_or_dimensions() {
    let family = round_family();
    let session = Session::new().unwrap();
    for (id, value) in [
        ("base", -1.0),
        ("top", -1.0),
        ("height", 0.0),
        ("radius", 0.0),
        ("radius", -1.0),
    ] {
        let mut part = part(&family);
        part.overrides.insert(
            id.into(),
            ParameterValue::Scalar(Quantity::length(value, LengthUnit::Millimeter)),
        );
        assert!(part.regenerate(&session).is_err());
        assert_eq!(session.shape_count().unwrap(), 0);
    }
    let mut invalid = part(&family);
    for id in ["base", "top"] {
        invalid.overrides.insert(
            id.into(),
            ParameterValue::Scalar(Quantity::length(0.0, LengthUnit::Millimeter)),
        );
    }
    assert!(invalid.regenerate(&session).is_err());
    invalid.overrides.clear();
    invalid.overrides.insert(
        "axis".into(),
        ParameterValue::Vector(VectorQuantity::scalars(0.0, 0.0, 0.0)),
    );
    assert!(invalid.regenerate(&session).is_err());
    for operation in [
        FeatureOperation::Sphere {
            center: point(0.0, 0.0, 0.0),
            radius: ScalarExpr::Literal(Quantity::scalar(1.0)),
        },
        FeatureOperation::Sphere {
            center: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 0.0)),
            radius: mm(1.0),
        },
        FeatureOperation::Cone {
            origin: point(0.0, 0.0, 0.0),
            axis: point(0.0, 0.0, 1.0),
            base_radius: mm(1.0),
            top_radius: mm(0.0),
            height: mm(1.0),
        },
    ] {
        let mut bad = family.clone();
        bad.features = vec![FeatureDefinition {
            id: "bad".into(),
            operation,
        }];
        assert!(part(&bad).regenerate(&session).is_err());
        assert_eq!(session.shape_count().unwrap(), 0);
    }
    let mut graph = InstanceGraph::new(&family);
    graph.add_base("part", HashMap::new(), "test").unwrap();
    let base = ModelDocument::from_graph(&graph);
    let loaded = ModelDocument::from_json(&base.to_json_pretty().unwrap()).unwrap();
    assert_eq!(loaded, base);
    assert_eq!(loaded.schema_version, CURRENT_SCHEMA_VERSION);
    let mut left = base.clone();
    let mut right = base.clone();
    let FeatureOperation::Sphere { radius, .. } = &mut left.family.features[0].operation else {
        panic!()
    };
    *radius = mm(7.0);
    let FeatureOperation::Cone { height, .. } = &mut right.family.features[3].operation else {
        panic!()
    };
    *height = mm(20.0);
    let DocumentMerge::Merged(merged) = base.three_way_merge(&left, &right).unwrap() else {
        panic!("independent primitive edits should merge");
    };
    let mut graph = merged.instance_graph().unwrap();
    let generated = graph.regenerate_all(&session).unwrap();
    let result = generated.result("part").unwrap();
    close(
        session.volume(result.shape("sphere").unwrap()).unwrap(),
        4.0 * PI * 7.0_f64.powi(3) / 3.0,
    );
    close(
        session.volume(result.shape("cone").unwrap()).unwrap(),
        PI * 20.0 * 13.0 / 3.0,
    );
    drop(generated);
    let legacy = super::family(RequirementPriority::Advisory, 1e12);
    let graph = InstanceGraph::new(&legacy);
    let mut value: serde_json::Value =
        serde_json::from_str(&ModelDocument::from_graph(&graph).to_json_pretty().unwrap()).unwrap();
    value["schema_version"] = 68.into();
    assert_eq!(
        ModelDocument::from_json(&value.to_string()).unwrap().family,
        legacy
    );
    assert_eq!(session.shape_count().unwrap(), 0);
}
