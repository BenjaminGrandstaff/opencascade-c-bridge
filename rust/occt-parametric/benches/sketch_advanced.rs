//! Scale gates for schema-72 sketch equations and native profile edits.
//! Independent components keep sparse storage/fill local. A point-on-spline
//! touches its defining points; interpolation/projection cost scales with that
//! curve rather than all sketch points. Native handles are released each loop.
use occt_bridge::{Session, ShapeType};
use occt_parametric::*;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
fn mm(v: f64) -> ScalarExpr {
    ScalarExpr::Literal(Quantity::length(v, LengthUnit::Millimeter))
}
fn scalar(v: f64) -> ScalarExpr {
    ScalarExpr::Literal(Quantity::scalar(v))
}
fn point(id: String, x: f64, y: f64, fixed: bool) -> SketchPoint {
    SketchPoint {
        id,
        x: mm(x),
        y: mm(y),
        fixed,
    }
}
fn sketch() -> SketchDefinition {
    SketchDefinition {
        measurements: Vec::new(),
        shapes: Vec::new(),
        regions: Vec::new(),
        region_discovery: None,
        texts: Vec::new(),
        acceptance: Default::default(),
        id: "scale".into(),
        datum_plane: None,
        face_support: None,
        projections: Vec::new(),
        origin: VectorExpr::Literal(VectorQuantity::lengths(0., 0., 0., LengthUnit::Millimeter)),
        x_axis: VectorExpr::Literal(VectorQuantity::scalars(1., 0., 0.)),
        y_axis: VectorExpr::Literal(VectorQuantity::scalars(0., 1., 0.)),
        points: vec![],
        lines: vec![],
        circles: vec![],
        ellipses: vec![],
        arcs: vec![],
        splines: vec![],
        profile: vec![],
        holes: Vec::new(),
        patterns: Vec::new(),
        constraints: vec![],
        profile_operations: vec![],
    }
}
fn gate(name: &str, budget: u64, run: impl FnOnce()) {
    let start = Instant::now();
    run();
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(budget),
        "{name} exceeded its {budget}s budget: {elapsed:?}"
    );
    println!("PASS {name}: {:.3}s / {budget}s", elapsed.as_secs_f64());
}
fn main() {
    gate(
        "1000 disjoint circle cells discovered, selected by 500 seeds and extruded",
        10,
        || {
            let session = Session::new().unwrap();
            let origin = occt_bridge::Vec3::new(0., 0., 0.);
            let normal = occt_bridge::Vec3::new(0., 0., 1.);
            let x = occt_bridge::Vec3::new(1., 0., 0.);
            let circles: Vec<_> = (0..1000)
                .map(|i| {
                    session
                        .create_circle_wire(
                            occt_bridge::Vec3::new(i as f64 * 3., 0., 0.),
                            normal,
                            1.,
                        )
                        .unwrap()
                })
                .collect();
            let seeds: Vec<_> = (0..1000)
                .step_by(2)
                .map(|i| occt_bridge::Vec3::new(i as f64 * 3., 0., 0.))
                .collect();
            let faces = session
                .discover_planar_regions(
                    &circles.iter().collect::<Vec<_>>(),
                    origin,
                    normal,
                    x,
                    &seeds,
                )
                .unwrap();
            assert_eq!(
                session.subshape_count(&faces, ShapeType::Face).unwrap(),
                500
            );
            let solids: Vec<_> = session
                .subshapes(&faces, ShapeType::Face)
                .unwrap()
                .iter()
                .map(|face| {
                    session
                        .create_prism_from_face(face, occt_bridge::Vec3::new(0., 0., 2.))
                        .unwrap()
                })
                .collect();
            let body = session
                .create_compound(&solids.iter().collect::<Vec<_>>())
                .unwrap();
            assert!(session.is_valid(&body).unwrap());
            assert!((session.volume(&body).unwrap() - 1000. * std::f64::consts::PI).abs() < 1e-6);
        },
    );
    gate(
        "300 automatically selected lens profiles regenerated after circle-spacing edits",
        10,
        || {
            let request: serde_json::Value = serde_json::from_str(include_str!(
                "../../../tools/model/discovered-lens.request.json"
            ))
            .unwrap();
            let family = ModelDocument::from_json(&request["model"].to_string())
                .unwrap()
                .family;
            let session = Session::new().unwrap();
            let mut part = PartInstance {
                id: "lens".into(),
                definition: &family,
                overrides: HashMap::new(),
                provenance: "scale".into(),
            };
            let mut previous = part.regenerate(&session).unwrap();
            for i in 0..300 {
                let spacing: f64 = 8. + (i % 6) as f64;
                part.overrides.insert(
                    "spacing".into(),
                    ParameterValue::Scalar(Quantity::length(spacing, LengthUnit::Millimeter)),
                );
                let built = part.regenerate_incremental(&session, &previous).unwrap();
                let expected = 2.
                    * (288. * (spacing / 24.).acos()
                        - spacing * (576. - spacing * spacing).sqrt() / 2.);
                assert!(
                    (session.volume(built.shape("body").unwrap()).unwrap() - expected).abs() < 1e-6
                );
                previous = built;
            }
        },
    );
    gate(
        "1000 cached line-spline touch aliases with a fully constrained pad",
        10,
        || {
            let request: serde_json::Value = serde_json::from_str(include_str!(
                "../../../tools/model/projected-tangent-intersection-pad.request.json"
            ))
            .unwrap();
            let mut family = ModelDocument::from_json(&request["model"].to_string())
                .unwrap()
                .family;
            let FeatureOperation::SketchFace { sketch } = &mut family.features[2].operation else {
                panic!()
            };
            let template = sketch.projections[0].intersections[0].clone();
            sketch.projections[0].intersections = (0..1000)
                .map(|i| SketchProjectionIntersection {
                    id: if i == 0 {
                        "touch".into()
                    } else {
                        format!("touch-{i}")
                    },
                    ..template.clone()
                })
                .collect();
            let session = Session::new().unwrap();
            let part = PartInstance {
                id: "part".into(),
                definition: &family,
                overrides: HashMap::new(),
                provenance: "scale".into(),
            };
            let built = part.regenerate(&session).unwrap();
            let snapshots = part.resolved_sketches(&session, &built).unwrap();
            let sk = &snapshots["profile"].as_ref().unwrap().sketch;
            let solution = sk.solve(&part.resolved_parameters().unwrap()).unwrap();
            assert!(solution.solved && solution.free_degrees == 0);
            assert_eq!(
                sk.points
                    .iter()
                    .filter(|p| p.id.contains(":intersection:"))
                    .count(),
                1000
            );
            assert!((solution.points["reference:intersection:touch-999"].x - 20.).abs() < 1e-8);
            drop(built);
            assert_eq!(session.shape_count().unwrap(), 0);
        },
    );
    gate(
        "1000 line-spline tangent profile edits with width and height controls",
        10,
        || {
            let request: serde_json::Value = serde_json::from_str(include_str!(
                "../../../tools/model/projected-tangent-intersection-pad.request.json"
            ))
            .unwrap();
            let mut family = ModelDocument::from_json(&request["model"].to_string())
                .unwrap()
                .family;
            family.features.retain(|f| f.id != "body");
            let session = Session::new().unwrap();
            let mut part = PartInstance {
                id: "part".into(),
                definition: &family,
                overrides: HashMap::new(),
                provenance: "scale".into(),
            };
            for i in 0..1000 {
                part.overrides.insert(
                    "width".into(),
                    ParameterValue::Scalar(Quantity::length(
                        [40., 45., 50.][i % 3],
                        LengthUnit::Millimeter,
                    )),
                );
                part.overrides.insert(
                    "rise".into(),
                    ParameterValue::Scalar(Quantity::length(
                        [20., 22., 24.][i % 3],
                        LengthUnit::Millimeter,
                    )),
                );
                let built = part.regenerate(&session).unwrap();
                assert!(session.is_valid(built.shape("profile").unwrap()).unwrap());
                drop(built);
                assert_eq!(session.shape_count().unwrap(), 0);
            }
        },
    );
    gate(
        "1000 cached native spline intersection aliases with solved pad",
        10,
        || {
            let request: serde_json::Value = serde_json::from_str(include_str!(
                "../../../tools/model/projected-spline-intersection-pad.request.json"
            ))
            .unwrap();
            let mut family = ModelDocument::from_json(&request["model"].to_string())
                .unwrap()
                .family;
            let FeatureOperation::SketchFace { sketch } = &mut family.features[2].operation else {
                panic!()
            };
            let template = sketch.projections[0].intersections[0].clone();
            sketch.projections[0].intersections = (0..1000)
                .map(|i| SketchProjectionIntersection {
                    id: if i == 0 {
                        "right".into()
                    } else {
                        format!("crossing-{i}")
                    },
                    ..template.clone()
                })
                .collect();
            let session = Session::new().unwrap();
            let part = PartInstance {
                id: "part".into(),
                definition: &family,
                overrides: HashMap::new(),
                provenance: "scale".into(),
            };
            let built = part.regenerate(&session).unwrap();
            let snapshots = part.resolved_sketches(&session, &built).unwrap();
            let sketch = &snapshots["profile"].as_ref().unwrap().sketch;
            assert_eq!(
                sketch
                    .points
                    .iter()
                    .filter(|p| p.id.contains(":intersection:"))
                    .count(),
                1000
            );
            let solution = sketch.solve(&part.resolved_parameters().unwrap()).unwrap();
            assert!(solution.solved && solution.free_degrees == 0);
            assert!(
                (solution.points["reference:intersection:crossing-999"].x - 20. - 160f64.sqrt())
                    .abs()
                    < 1e-7
            );
            drop(built);
            assert_eq!(session.shape_count().unwrap(), 0);
        },
    );
    for ellipse in [false, true] {
        gate(
            if ellipse {
                "1000 native ellipse-pair intersection profile edits"
            } else {
                "1000 native rational spline intersection profile edits"
            },
            10,
            || {
                let request: serde_json::Value = serde_json::from_str(if ellipse {
                    include_str!(
                        "../../../tools/model/projected-ellipse-intersection-pad.request.json"
                    )
                } else {
                    include_str!(
                        "../../../tools/model/projected-spline-intersection-pad.request.json"
                    )
                })
                .unwrap();
                let mut family = ModelDocument::from_json(&request["model"].to_string())
                    .unwrap()
                    .family;
                family.features.retain(|f| f.id != "body");
                let session = Session::new().unwrap();
                let mut part = PartInstance {
                    id: "part".into(),
                    definition: &family,
                    overrides: HashMap::new(),
                    provenance: "scale".into(),
                };
                for i in 0..1000 {
                    part.overrides.insert(
                        if ellipse { "radius" } else { "width" }.into(),
                        ParameterValue::Scalar(Quantity::length(
                            if ellipse {
                                [10., 12., 14.][i % 3]
                            } else {
                                [40., 45., 50.][i % 3]
                            },
                            LengthUnit::Millimeter,
                        )),
                    );
                    if !ellipse {
                        part.overrides.insert(
                            "weight".into(),
                            ParameterValue::Scalar(Quantity::scalar([0.75, 1., 1.25][i % 3])),
                        );
                    }
                    let built = part.regenerate(&session).unwrap();
                    assert!(session.is_valid(built.shape("profile").unwrap()).unwrap());
                    drop(built);
                    assert_eq!(session.shape_count().unwrap(), 0);
                }
            },
        );
    }
    gate(
        "1000 named intersection references sharing one finite curve pair",
        10,
        || {
            let request: serde_json::Value = serde_json::from_str(include_str!(
                "../../../tools/model/projected-intersection-pad.request.json"
            ))
            .unwrap();
            let mut family = ModelDocument::from_json(&request["model"].to_string())
                .unwrap()
                .family;
            let FeatureOperation::SketchFace { sketch } = &mut family.features[2].operation else {
                panic!()
            };
            let template = sketch.projections[0].intersections[0].clone();
            sketch.projections[0].intersections = (0..1000)
                .map(|i| SketchProjectionIntersection {
                    id: if i == 0 {
                        "right".into()
                    } else {
                        format!("crossing-{i}")
                    },
                    ..template.clone()
                })
                .collect();
            let session = Session::new().unwrap();
            let part = PartInstance {
                id: "part".into(),
                definition: &family,
                overrides: HashMap::new(),
                provenance: "scale".into(),
            };
            let built = part.regenerate(&session).unwrap();
            let snapshots = part.resolved_sketches(&session, &built).unwrap();
            let sketch = &snapshots["profile"].as_ref().unwrap().sketch;
            assert_eq!(
                sketch
                    .points
                    .iter()
                    .filter(|p| p.id.contains(":intersection:"))
                    .count(),
                1000
            );
            let solution = sketch.solve(&part.resolved_parameters().unwrap()).unwrap();
            assert!(solution.solved && solution.free_degrees == 0);
            assert!((solution.points["reference:intersection:crossing-999"].x - 8.).abs() < 1e-8);
            drop(built);
            assert_eq!(session.shape_count().unwrap(), 0);
        },
    );
    gate(
        "1000 projected intersection profile edits with driven pad centres",
        10,
        || {
            let request: serde_json::Value = serde_json::from_str(include_str!(
                "../../../tools/model/projected-intersection-pad.request.json"
            ))
            .unwrap();
            let mut family = ModelDocument::from_json(&request["model"].to_string())
                .unwrap()
                .family;
            family.features.retain(|f| f.id != "body");
            let session = Session::new().unwrap();
            let mut part = PartInstance {
                id: "part".into(),
                definition: &family,
                overrides: HashMap::new(),
                provenance: "scale".into(),
            };
            for i in 0..1000 {
                part.overrides.insert(
                    "radius".into(),
                    ParameterValue::Scalar(Quantity::length(
                        [10., 12., 14.][i % 3],
                        LengthUnit::Millimeter,
                    )),
                );
                let built = part.regenerate(&session).unwrap();
                assert!(session.is_valid(built.shape("profile").unwrap()).unwrap());
                drop(built);
                assert_eq!(session.shape_count().unwrap(), 0);
            }
        },
    );
    gate(
        "1000 named junction references on a 1000-edge projected loop",
        10,
        || {
            let request: serde_json::Value = serde_json::from_str(include_str!(
                "../../../tools/model/projected-junction-pad.request.json"
            ))
            .unwrap();
            let mut family = ModelDocument::from_json(&request["model"].to_string())
                .unwrap()
                .family;
            family.features.retain(|f| f.id != "body");
            family.requirements.clear();
            let mut source = sketch();
            source.acceptance = SketchAcceptance::FullyConstrained;
            let mut references = Vec::new();
            for i in 0..1000 {
                let angle = std::f64::consts::TAU * i as f64 / 1000.;
                let (x, y) = (10. * angle.cos(), 10. * angle.sin());
                source.points.push(point(format!("p{i}"), x, y, true));
                source.lines.push(SketchLine {
                    id: format!("l{i}"),
                    start: format!("p{i}"),
                    end: format!("p{}", (i + 1) % 1000),
                });
                source.profile.push(format!("l{i}"));
                references.push(SketchProjectionJunction {
                    id: format!("v{i}"),
                    near: VectorExpr::Literal(VectorQuantity::lengths(
                        x,
                        y,
                        0.,
                        LengthUnit::Millimeter,
                    )),
                    maximum_distance: mm(1e-6),
                });
            }
            family.features[0].operation = FeatureOperation::SketchWire {
                sketch: Box::new(source),
            };
            let FeatureOperation::SketchFace { sketch } = &mut family.features[1].operation else {
                panic!()
            };
            sketch.points.clear();
            sketch.constraints.clear();
            sketch.projections[0].junctions = references;
            let session = Session::new().unwrap();
            let part = PartInstance {
                id: "part".into(),
                definition: &family,
                overrides: HashMap::new(),
                provenance: "scale".into(),
            };
            let built = part.regenerate(&session).unwrap();
            assert!(session.is_valid(built.shape("profile").unwrap()).unwrap());
            let snapshots = part.resolved_sketches(&session, &built).unwrap();
            let sk = &snapshots["profile"].as_ref().unwrap().sketch;
            assert_eq!(
                sk.points
                    .iter()
                    .filter(|p| p.id.contains(":junction:"))
                    .count(),
                1000
            );
            let solved = sk.solve(&part.resolved_parameters().unwrap()).unwrap();
            assert_eq!(solved.free_degrees, 0);
            let p = solved.points["reference:junction:v0"];
            assert!((p.x - 10.).abs() < 1e-7 && (p.y + 2.).abs() < 1e-7);
            drop(built);
            assert_eq!(session.shape_count().unwrap(), 0);
        },
    );
    gate(
        "1000 named-junction profile edits with driven point and dimension",
        10,
        || {
            let request: serde_json::Value = serde_json::from_str(include_str!(
                "../../../tools/model/projected-junction-pad.request.json"
            ))
            .unwrap();
            let mut family = ModelDocument::from_json(&request["model"].to_string())
                .unwrap()
                .family;
            family.features.retain(|f| f.id != "body");
            family.requirements.clear();
            let session = Session::new().unwrap();
            let mut part = PartInstance {
                id: "part".into(),
                definition: &family,
                overrides: HashMap::new(),
                provenance: "scale".into(),
            };
            for i in 0..1000 {
                part.overrides.insert(
                    "width".into(),
                    ParameterValue::Scalar(Quantity::length(
                        [40., 50., 60.][i % 3],
                        LengthUnit::Millimeter,
                    )),
                );
                let built = part.regenerate(&session).unwrap();
                assert!(session.is_valid(built.shape("profile").unwrap()).unwrap());
                drop(built);
                assert_eq!(session.shape_count().unwrap(), 0);
            }
        },
    );
    gate(
        "1000-edge closed chain projection and 1001-edge limit rejection",
        10,
        || {
            let request: serde_json::Value = serde_json::from_str(include_str!(
                "../../../tools/model/projected-chain-pad.request.json"
            ))
            .unwrap();
            let mut family = ModelDocument::from_json(&request["model"].to_string())
                .unwrap()
                .family;
            family.requirements.clear();
            family.features.retain(|f| f.id != "body");
            let session = Session::new().unwrap();
            for count in [1000, 1001] {
                let mut source = sketch();
                source.acceptance = SketchAcceptance::FullyConstrained;
                for i in 0..count {
                    let angle = std::f64::consts::TAU * i as f64 / count as f64;
                    source.points.push(point(
                        format!("p{i}"),
                        10. * angle.cos(),
                        10. * angle.sin(),
                        true,
                    ));
                    source.lines.push(SketchLine {
                        id: format!("l{i}"),
                        start: format!("p{i}"),
                        end: format!("p{}", (i + 1) % count),
                    });
                    source.profile.push(format!("l{i}"));
                }
                family.features[0].operation = FeatureOperation::SketchWire {
                    sketch: Box::new(source),
                };
                let part = PartInstance {
                    id: "part".into(),
                    definition: &family,
                    overrides: HashMap::new(),
                    provenance: "scale".into(),
                };
                if count == 1000 {
                    let built = part.regenerate(&session).unwrap();
                    assert!(session.is_valid(built.shape("profile").unwrap()).unwrap());
                    let expected = 0.5
                        * count as f64
                        * 100.
                        * (std::f64::consts::TAU / count as f64).sin()
                        * 3f64.sqrt()
                        / 2.;
                    assert!(
                        (session
                            .surface_area(built.shape("profile").unwrap())
                            .unwrap()
                            - expected)
                            .abs()
                            < 1e-6
                    );
                    drop(built);
                } else {
                    assert!(
                        part.regenerate(&session)
                            .err()
                            .unwrap()
                            .to_string()
                            .contains("requires 1–1000 edges")
                    );
                }
                assert_eq!(session.shape_count().unwrap(), 0);
            }
        },
    );
    gate(
        "1000 exact mixed-degree chain projection profiles with source edits",
        10,
        || {
            let request: serde_json::Value = serde_json::from_str(include_str!(
                "../../../tools/model/projected-chain-pad.request.json"
            ))
            .unwrap();
            let mut family = ModelDocument::from_json(&request["model"].to_string())
                .unwrap()
                .family;
            family.features.retain(|f| f.id != "body");
            family.requirements.clear();
            let session = Session::new().unwrap();
            let mut part = PartInstance {
                id: "part".into(),
                definition: &family,
                overrides: HashMap::new(),
                provenance: "scale".into(),
            };
            for i in 0..1000 {
                part.overrides.insert(
                    "weight".into(),
                    ParameterValue::Scalar(Quantity::scalar([0.5, 0.7, 1.4][i % 3])),
                );
                let built = part.regenerate(&session).unwrap();
                assert!(session.is_valid(built.shape("profile").unwrap()).unwrap());
                assert!(
                    session
                        .surface_area(built.shape("profile").unwrap())
                        .unwrap()
                        > 400.
                );
                drop(built);
                assert_eq!(session.shape_count().unwrap(), 0);
            }
        },
    );
    gate(
        "1000 exact rational projected profiles with source weight edits",
        10,
        || {
            let request: serde_json::Value = serde_json::from_str(include_str!(
                "../../../tools/model/projected-spline-pad.request.json"
            ))
            .unwrap();
            let mut family = ModelDocument::from_json(&request["model"].to_string())
                .unwrap()
                .family;
            family.features.retain(|f| f.id != "body");
            family.requirements.clear();
            let session = Session::new().unwrap();
            let mut part = PartInstance {
                id: "part".into(),
                definition: &family,
                overrides: HashMap::new(),
                provenance: "scale".into(),
            };
            for i in 0..1000 {
                part.overrides.insert(
                    "weight".into(),
                    ParameterValue::Scalar(Quantity::scalar([0.5, 0.7, 1.4][i % 3])),
                );
                let built = part.regenerate(&session).unwrap();
                assert!(session.is_valid(built.shape("profile").unwrap()).unwrap());
                assert!(
                    session
                        .surface_area(built.shape("profile").unwrap())
                        .unwrap()
                        > 400.
                );
                drop(built);
                assert_eq!(session.shape_count().unwrap(), 0);
            }
        },
    );
    gate(
        "250 exact rational projected pads with source weight edits",
        10,
        || {
            let request: serde_json::Value = serde_json::from_str(include_str!(
                "../../../tools/model/projected-spline-pad.request.json"
            ))
            .unwrap();
            let doc = ModelDocument::from_json(&request["model"].to_string()).unwrap();
            let session = Session::new().unwrap();
            let mut part = PartInstance {
                id: "part".into(),
                definition: &doc.family,
                overrides: HashMap::new(),
                provenance: "scale".into(),
            };
            for i in 0..250 {
                part.overrides.insert(
                    "weight".into(),
                    ParameterValue::Scalar(Quantity::scalar([0.5, 0.7, 1.4][i % 3])),
                );
                let built = part.regenerate(&session).unwrap();
                assert!(session.is_valid(built.shape("body").unwrap()).unwrap());
                let area = session
                    .surface_area(built.shape("profile").unwrap())
                    .unwrap();
                let volume = session.volume(built.shape("body").unwrap()).unwrap();
                assert!((volume - area * 10.).abs() < 1e-7 * volume);
                drop(built);
                assert_eq!(session.shape_count().unwrap(), 0);
            }
        },
    );
    gate(
        "1000 strict sketch builds and 1000 free-motion rejections",
        10,
        || {
            let request: serde_json::Value = serde_json::from_str(include_str!(
                "../../../tools/model/strict-sketch-block.request.json"
            ))
            .unwrap();
            let doc = ModelDocument::from_json(&request["model"].to_string()).unwrap();
            let session = Session::new().unwrap();
            let mut part = PartInstance {
                id: "part".into(),
                definition: &doc.family,
                overrides: HashMap::new(),
                provenance: "scale".into(),
            };
            for _ in 0..1000 {
                part.overrides.insert(
                    "lift".into(),
                    ParameterValue::Scalar(Quantity::length(4., LengthUnit::Millimeter)),
                );
                let built = part.regenerate(&session).unwrap();
                assert!(session.is_valid(built.shape("body").unwrap()).unwrap());
                assert!(
                    (session.volume(built.shape("body").unwrap()).unwrap() - 10000.).abs() < 1e-6
                );
                drop(built);
                part.overrides.insert(
                    "lift".into(),
                    ParameterValue::Scalar(Quantity::length(0., LengthUnit::Millimeter)),
                );
                assert!(
                    part.regenerate(&session)
                        .err()
                        .unwrap()
                        .to_string()
                        .contains("requires fully constrained")
                );
                assert_eq!(session.shape_count().unwrap(), 0);
            }
        },
    );
    gate("10000 independent duplicate-equation diagnoses", 10, || {
        let mut s = sketch();
        for i in 0..10000 {
            let a = format!("a{i}");
            let b = format!("b{i}");
            s.points.extend([
                point(a.clone(), 0., 0., true),
                point(b.clone(), 0., 0., false),
            ]);
            let c = SketchConstraint::Coincident {
                first: a,
                second: b,
            };
            s.constraints.extend([c.clone(), c]);
        }
        let solved = s.solve(&HashMap::new()).unwrap();
        assert!(
            solved.solved && solved.diagnosis.complete && solved.diagnosis.rank_agrees_with_solver
        );
        assert_eq!(solved.diagnosis.locally_redundant_constraints.len(), 10000);
        assert!(solved.diagnosis.free_coordinates.is_empty());
    });
    gate("5000-point coupled-chain free-motion diagnosis", 10, || {
        let mut s = sketch();
        for i in 0..5000 {
            s.points.push(point(format!("p{i}"), 0., 0., false));
            if i > 0 {
                s.constraints.push(SketchConstraint::Coincident {
                    first: format!("p{}", i - 1),
                    second: format!("p{i}"),
                });
            }
        }
        let solved = s.solve(&HashMap::new()).unwrap();
        assert!(
            solved.solved && solved.diagnosis.complete && solved.diagnosis.rank_agrees_with_solver
        );
        assert_eq!(solved.diagnosis.free_coordinates.len(), 2);
        for mode in &solved.diagnosis.free_motions {
            assert_eq!(mode.points.len(), 64);
            assert_eq!(mode.omitted_points, 4936);
        }
    });
    gate(
        "10000 fixed-conflict diagnoses with bounded group output",
        10,
        || {
            let mut s = sketch();
            s.points.extend([
                point("a".into(), 0., 0., true),
                point("b".into(), 1., 0., true),
            ]);
            s.constraints = (0..10000)
                .map(|_| SketchConstraint::Distance {
                    first: "a".into(),
                    second: "b".into(),
                    value: mm(2.),
                })
                .collect();
            let solved = s.solve(&HashMap::new()).unwrap();
            assert!(!solved.solved && solved.diagnosis.complete);
            assert_eq!(solved.diagnosis.failed_constraints.len(), 10000);
            assert_eq!(solved.diagnosis.conflict_groups.len(), 128);
            assert_eq!(solved.diagnosis.omitted_conflict_groups, 9872);
        },
    );
    gate(
        "1000 native open fillet/chamfer path regenerations",
        10,
        || {
            let request: serde_json::Value = serde_json::from_str(include_str!(
                "../../../tools/model/open-corner-pipe.request.json"
            ))
            .unwrap();
            let doc = ModelDocument::from_json(&request["model"].to_string()).unwrap();
            let mut family = doc.family.clone();
            family.features.retain(|f| f.id == "path");
            family.requirements.clear();
            let session = Session::new().unwrap();
            let mut part = PartInstance {
                id: "part".into(),
                definition: &family,
                overrides: HashMap::new(),
                provenance: "scale".into(),
            };
            for i in 0..1000 {
                let r = if i % 2 == 0 { 4. } else { 6. };
                part.overrides.insert(
                    "fillet_radius".into(),
                    ParameterValue::Scalar(Quantity::length(r, LengthUnit::Millimeter)),
                );
                let built = part.regenerate(&session).unwrap();
                let path = built.shape("path").unwrap();
                assert!(session.is_valid(path).unwrap() && !session.wire_is_closed(path).unwrap());
                let length = session
                    .subshapes(path, ShapeType::Edge)
                    .unwrap()
                    .iter()
                    .map(|e| session.edge_length(e).unwrap())
                    .sum::<f64>();
                assert!(
                    (length - (95. - 2. * r + std::f64::consts::FRAC_PI_2 * r - 8. + 34f64.sqrt()))
                        .abs()
                        < 1e-6
                );
                drop(built);
                assert_eq!(session.shape_count().unwrap(), 0);
            }
        },
    );
    gate(
        "250 open fillet/chamfer paths and swept pipes with radius edits",
        10,
        || {
            let request: serde_json::Value = serde_json::from_str(include_str!(
                "../../../tools/model/open-corner-pipe.request.json"
            ))
            .unwrap();
            let doc = ModelDocument::from_json(&request["model"].to_string()).unwrap();
            let session = Session::new().unwrap();
            let mut part = PartInstance {
                id: "part".into(),
                definition: &doc.family,
                overrides: HashMap::new(),
                provenance: "scale".into(),
            };
            for i in 0..250 {
                let r = if i % 2 == 0 { 4. } else { 6. };
                part.overrides.insert(
                    "fillet_radius".into(),
                    ParameterValue::Scalar(Quantity::length(r, LengthUnit::Millimeter)),
                );
                let built = part.regenerate(&session).unwrap();
                let body = built.shape("body").unwrap();
                assert!(session.is_valid(body).unwrap());
                let expected = 95. - 2. * r + std::f64::consts::FRAC_PI_2 * r - 8. + 34f64.sqrt();
                assert!(
                    (session.volume(body).unwrap() - std::f64::consts::PI * expected).abs() < 1e-4
                );
                assert!(
                    !session
                        .wire_is_closed(built.shape("path").unwrap())
                        .unwrap()
                );
                drop(built);
                assert_eq!(session.shape_count().unwrap(), 0);
            }
        },
    );
    gate(
        "1000 native fillet/chamfer plates with radius edits",
        10,
        || {
            let request: serde_json::Value = serde_json::from_str(include_str!(
                "../../../tools/model/corner-treated-plate.request.json"
            ))
            .unwrap();
            let doc = ModelDocument::from_json(&request["model"].to_string()).unwrap();
            let session = Session::new().unwrap();
            let mut part = PartInstance {
                id: "part".into(),
                definition: &doc.family,
                overrides: HashMap::new(),
                provenance: "scale".into(),
            };
            for i in 0..1000 {
                let r = if i % 2 == 0 { 4. } else { 6. };
                part.overrides.insert(
                    "fillet_radius".into(),
                    ParameterValue::Scalar(Quantity::length(r, LengthUnit::Millimeter)),
                );
                let built = part.regenerate(&session).unwrap();
                let body = built.shape("body").unwrap();
                assert!(session.is_valid(body).unwrap());
                assert!(
                    (session.volume(body).unwrap()
                        - (1000. - r * r * (1. - std::f64::consts::FRAC_PI_4) - 7.5) * 10.)
                        .abs()
                        < 1e-5
                );
                drop(built);
                assert_eq!(session.shape_count().unwrap(), 0);
            }
        },
    );
    gate(
        "1000 native ellipse tangent pairs with axis edits",
        10,
        || {
            let request: serde_json::Value = serde_json::from_str(include_str!(
                "../../../tools/model/ellipse-tangent-pair.request.json"
            ))
            .unwrap();
            let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
            let session = Session::new().unwrap();
            let mut part = PartInstance {
                id: "part".into(),
                definition: &document.family,
                overrides: HashMap::new(),
                provenance: "scale".into(),
            };
            for i in 0..1000 {
                part.overrides.insert(
                    "semi_minor".into(),
                    ParameterValue::Scalar(Quantity::length(
                        if i % 2 == 0 { 3. } else { 4. },
                        LengthUnit::Millimeter,
                    )),
                );
                let built = part.regenerate(&session).unwrap();
                assert!(session.is_valid(built.shape("body").unwrap()).unwrap());
                if i % 2 == 0 {
                    assert!(
                        (session.volume(built.shape("body").unwrap()).unwrap()
                            - std::f64::consts::PI * 4. * 2. * 8.)
                            .abs()
                            < 1e-5
                    );
                }
                let peak = if i % 2 == 0 { 3. } else { 4. };
                assert!(
                    (session
                        .exact_bounds(built.shape("body").unwrap())
                        .unwrap()
                        .min
                        .y
                        - peak)
                        .abs()
                        < 2e-7
                );
                drop(built);
                assert_eq!(session.shape_count().unwrap(), 0);
            }
        },
    );

    gate(
        "1000 native ellipse tangent circle bosses with axis edits",
        10,
        || {
            let request: serde_json::Value = serde_json::from_str(include_str!(
                "../../../tools/model/ellipse-tangent-boss.request.json"
            ))
            .unwrap();
            let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
            let session = Session::new().unwrap();
            let mut part = PartInstance {
                id: "part".into(),
                definition: &document.family,
                overrides: HashMap::new(),
                provenance: "scale".into(),
            };
            for i in 0..1000 {
                part.overrides.insert(
                    "semi_minor".into(),
                    ParameterValue::Scalar(Quantity::length(
                        if i % 2 == 0 { 3. } else { 4. },
                        LengthUnit::Millimeter,
                    )),
                );
                let built = part.regenerate(&session).unwrap();
                assert!(session.is_valid(built.shape("body").unwrap()).unwrap());
                if i % 2 == 0 {
                    assert!(
                        (session.volume(built.shape("body").unwrap()).unwrap()
                            - std::f64::consts::PI * 2. * 2. * 8.)
                            .abs()
                            < 1e-5
                    );
                }
                let peak = if i % 2 == 0 { 3. } else { 4. };
                assert!(
                    (session
                        .exact_bounds(built.shape("body").unwrap())
                        .unwrap()
                        .min
                        .y
                        - peak)
                        .abs()
                        < 2e-7
                );
                drop(built);
                assert_eq!(session.shape_count().unwrap(), 0);
            }
        },
    );

    gate(
        "1000 native spline tangent ellipses with rational weight edits",
        10,
        || {
            let request: serde_json::Value = serde_json::from_str(include_str!(
                "../../../tools/model/spline-tangent-ellipse.request.json"
            ))
            .unwrap();
            let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
            let session = Session::new().unwrap();
            let mut part = PartInstance {
                id: "part".into(),
                definition: &document.family,
                overrides: HashMap::new(),
                provenance: "scale".into(),
            };
            for i in 0..1000 {
                part.overrides.insert(
                    "middle_weight".into(),
                    ParameterValue::Scalar(Quantity::scalar(if i % 2 == 0 { 1. } else { 2. })),
                );
                let built = part.regenerate(&session).unwrap();
                assert!(session.is_valid(built.shape("body").unwrap()).unwrap());
                if i % 2 == 0 {
                    assert!(
                        (session.volume(built.shape("body").unwrap()).unwrap()
                            - std::f64::consts::PI * 5. * 3. * 8.)
                            .abs()
                            < 1e-5
                    );
                }
                let peak = if i % 2 == 0 { 5. } else { 20. / 3. };
                assert!(
                    (session
                        .exact_bounds(built.shape("body").unwrap())
                        .unwrap()
                        .min
                        .y
                        - peak)
                        .abs()
                        < 2e-7
                );
                drop(built);
                assert_eq!(session.shape_count().unwrap(), 0);
            }
        },
    );

    gate(
        "1000 native spline tangent pairs with rational weight edits",
        10,
        || {
            let request: serde_json::Value = serde_json::from_str(include_str!(
                "../../../tools/model/spline-tangent-pair.request.json"
            ))
            .unwrap();
            let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
            let session = Session::new().unwrap();
            let mut part = PartInstance {
                id: "part".into(),
                definition: &document.family,
                overrides: HashMap::new(),
                provenance: "scale".into(),
            };
            for i in 0..1000 {
                part.overrides.insert(
                    "middle_weight".into(),
                    ParameterValue::Scalar(Quantity::scalar(if i % 2 == 0 { 1. } else { 2. })),
                );
                let built = part.regenerate(&session).unwrap();
                assert!(session.is_valid(built.shape("body").unwrap()).unwrap());
                if i % 2 == 0 {
                    assert!(
                        (session.volume(built.shape("body").unwrap()).unwrap()
                            - 20. * 8. / 3. * 8.)
                            .abs()
                            < 1e-5
                    );
                }
                let peak = if i % 2 == 0 { 5. } else { 20. / 3. };
                assert!(
                    (session
                        .exact_bounds(built.shape("body").unwrap())
                        .unwrap()
                        .min
                        .y
                        - peak)
                        .abs()
                        < 2e-7
                );
                drop(built);
                assert_eq!(session.shape_count().unwrap(), 0);
            }
        },
    );

    gate(
        "1000 native spline tangent circle bosses with rational weight edits",
        10,
        || {
            let request: serde_json::Value = serde_json::from_str(include_str!(
                "../../../tools/model/spline-tangent-boss.request.json"
            ))
            .unwrap();
            let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
            let session = Session::new().unwrap();
            let mut part = PartInstance {
                id: "part".into(),
                definition: &document.family,
                overrides: HashMap::new(),
                provenance: "scale".into(),
            };
            for i in 0..1000 {
                part.overrides.insert(
                    "middle_weight".into(),
                    ParameterValue::Scalar(Quantity::scalar(if i % 2 == 0 { 1. } else { 2. })),
                );
                let built = part.regenerate(&session).unwrap();
                assert!(session.is_valid(built.shape("body").unwrap()).unwrap());
                if i % 2 == 0 {
                    assert!(
                        (session.volume(built.shape("body").unwrap()).unwrap()
                            - std::f64::consts::PI * 9. * 8.)
                            .abs()
                            < 1e-5
                    );
                }
                drop(built);
                assert_eq!(session.shape_count().unwrap(), 0);
            }
        },
    );

    gate(
        "1000 native spline tangent caps with rational weight edits",
        10,
        || {
            let request: serde_json::Value = serde_json::from_str(include_str!(
                "../../../tools/model/spline-tangent-cap.request.json"
            ))
            .unwrap();
            let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
            let session = Session::new().unwrap();
            let mut part = PartInstance {
                id: "part".into(),
                definition: &document.family,
                overrides: HashMap::new(),
                provenance: "scale".into(),
            };
            for i in 0..1000 {
                part.overrides.insert(
                    "middle_weight".into(),
                    ParameterValue::Scalar(Quantity::scalar(if i % 2 == 0 { 1. } else { 2. })),
                );
                let built = part.regenerate(&session).unwrap();
                assert!(session.is_valid(built.shape("body").unwrap()).unwrap());
                if i % 2 == 0 {
                    assert!(
                        (session.volume(built.shape("body").unwrap()).unwrap()
                            - 20. * 10. / 3. * 8.)
                            .abs()
                            < 1e-5
                    );
                }
                drop(built);
                assert_eq!(session.shape_count().unwrap(), 0);
            }
        },
    );

    gate(
        "1000 native ellipse tangent pads with axis edits",
        10,
        || {
            let request: serde_json::Value = serde_json::from_str(include_str!(
                "../../../tools/model/ellipse-tangent-pad.request.json"
            ))
            .unwrap();
            let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
            let session = Session::new().unwrap();
            let mut part = PartInstance {
                id: "part".into(),
                definition: &document.family,
                overrides: HashMap::new(),
                provenance: "scale".into(),
            };
            for i in 0..1000 {
                part.overrides.insert(
                    "semi_minor".into(),
                    ParameterValue::Scalar(Quantity::length(
                        if i % 2 == 0 { 3. } else { 4. },
                        LengthUnit::Millimeter,
                    )),
                );
                let built = part.regenerate(&session).unwrap();
                assert!(session.is_valid(built.shape("body").unwrap()).unwrap());
                if i % 2 == 0 {
                    assert!(
                        (session.volume(built.shape("body").unwrap()).unwrap()
                            - std::f64::consts::PI * 5. * 3. * 8.)
                            .abs()
                            < 1e-5
                    );
                }
                drop(built);
                assert_eq!(session.shape_count().unwrap(), 0);
            }
        },
    );

    gate(
        "1000 native non-clamped B-spline caps with rational weight edits",
        10,
        || {
            let request: serde_json::Value = serde_json::from_str(include_str!(
                "../../../tools/model/non-clamped-spline-cap.request.json"
            ))
            .unwrap();
            let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
            let session = Session::new().unwrap();
            let mut part = PartInstance {
                id: "part".into(),
                definition: &document.family,
                overrides: HashMap::new(),
                provenance: "scale".into(),
            };
            for i in 0..1000 {
                part.overrides.insert(
                    "middle_weight".into(),
                    ParameterValue::Scalar(Quantity::scalar(if i % 2 == 0 { 1. } else { 2. })),
                );
                let built = part.regenerate(&session).unwrap();
                assert!(session.is_valid(built.shape("body").unwrap()).unwrap());
                if i % 2 == 0 {
                    assert!(
                        (session.volume(built.shape("body").unwrap()).unwrap()
                            - 20. * 10. / 12. * 8.)
                            .abs()
                            < 1e-5
                    );
                }
                drop(built);
                assert_eq!(session.shape_count().unwrap(), 0);
            }
        },
    );

    gate(
        "1000 native periodic B-spline pads with rational weight edits",
        10,
        || {
            let request: serde_json::Value = serde_json::from_str(include_str!(
                "../../../tools/model/periodic-spline-pad.request.json"
            ))
            .unwrap();
            let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
            let session = Session::new().unwrap();
            let mut part = PartInstance {
                id: "part".into(),
                definition: &document.family,
                overrides: HashMap::new(),
                provenance: "scale".into(),
            };
            for i in 0..1000 {
                part.overrides.insert(
                    "corner_weight".into(),
                    ParameterValue::Scalar(Quantity::scalar(if i % 2 == 0 { 1. } else { 2. })),
                );
                let built = part.regenerate(&session).unwrap();
                assert!(session.is_valid(built.shape("body").unwrap()).unwrap());
                if i % 2 == 0 {
                    assert!(
                        (session.volume(built.shape("body").unwrap()).unwrap()
                            - 122. / 45. * 10. * 8. * 8.)
                            .abs()
                            < 1e-5
                    );
                }
                drop(built);
                assert_eq!(session.shape_count().unwrap(), 0);
            }
        },
    );
    gate(
        "1000 explicit rational B-spline profiles with native weight edits",
        10,
        || {
            let request: serde_json::Value = serde_json::from_str(include_str!(
                "../../../tools/model/rational-spline-cap.request.json"
            ))
            .unwrap();
            let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
            let session = Session::new().unwrap();
            let mut part = PartInstance {
                id: "part".into(),
                definition: &document.family,
                overrides: HashMap::new(),
                provenance: "scale".into(),
            };
            for i in 0..1000 {
                let weight = if i % 2 == 0 { 0.5_f64.sqrt() } else { 1. };
                part.overrides.insert(
                    "middle_weight".into(),
                    ParameterValue::Scalar(Quantity::scalar(weight)),
                );
                let built = part.regenerate(&session).unwrap();
                assert!(session.is_valid(built.shape("body").unwrap()).unwrap());
                let area = if i % 2 == 0 {
                    (std::f64::consts::PI / 4. - 0.5) * 100.
                } else {
                    100. / 3.
                };
                assert!(
                    (session.volume(built.shape("body").unwrap()).unwrap() - area * 8.).abs()
                        < 1e-5
                );
                drop(built);
                assert_eq!(session.shape_count().unwrap(), 0);
            }
        },
    );
    gate(
        "1000 arc-span-aware line and circular tangent components",
        10,
        || {
            let mut s = sketch();
            for i in 0..1000 {
                let id = |name: &str| format!("{name}-{i}");
                let x = i as f64 * 30.;
                s.points.extend([
                    point(id("c"), x, 0., true),
                    point(id("start"), x, 4., true),
                    point(id("end"), x, -4., true),
                    point(id("a"), x - 10., -4., true),
                    point(id("b"), x + 10., -4., true),
                    point(id("c2"), x + 6., -1., false),
                    point(id("start2"), x + 6., 1., false),
                    point(id("end2"), x + 8., -1., false),
                ]);
                s.lines.extend([
                    SketchLine {
                        id: id("line"),
                        start: id("a"),
                        end: id("b"),
                    },
                    SketchLine {
                        id: id("end-axis"),
                        start: id("c2"),
                        end: id("end2"),
                    },
                    SketchLine {
                        id: id("start-axis"),
                        start: id("c2"),
                        end: id("start2"),
                    },
                ]);
                s.arcs.extend([
                    SketchArc {
                        id: id("first"),
                        center: id("c"),
                        start: id("start"),
                        end: id("end"),
                        clockwise: true,
                    },
                    SketchArc {
                        id: id("second"),
                        center: id("c2"),
                        start: id("start2"),
                        end: id("end2"),
                        clockwise: false,
                    },
                ]);
                s.constraints.extend([
                    SketchConstraint::LineCircleTangent {
                        line: id("line"),
                        circle: id("second"),
                        side: SketchLineSide::Left,
                    },
                    SketchConstraint::CircleCircleTangent {
                        first: id("first"),
                        second: id("second"),
                        mode: SketchCircleTangency::External,
                    },
                    SketchConstraint::Radius {
                        curve: id("second"),
                        value: mm(2.),
                    },
                    SketchConstraint::Horizontal {
                        line: id("end-axis"),
                    },
                    SketchConstraint::Vertical {
                        line: id("start-axis"),
                    },
                ]);
            }
            let params = HashMap::new();
            let solved = s.solve(&params).unwrap();
            assert!(solved.solved);
            assert_eq!(solved.free_degrees, 0);
            assert!(
                s.constraint_checks(&params, &solved)
                    .unwrap()
                    .iter()
                    .all(|c| c.satisfied)
            );
        },
    );
    gate(
        "1000 independent line-circle and circle-circle tangent components",
        10,
        || {
            let mut s = sketch();
            for i in 0..1000 {
                let id = |name: &str| format!("{name}-{i}");
                let x = i as f64 * 20.;
                s.points.extend([
                    point(id("c"), x, 0., true),
                    point(id("r"), x + 4., 0., true),
                    point(id("a"), x - 10., -4., true),
                    point(id("b"), x + 10., -4., true),
                    point(id("c2"), x + 6., -1., false),
                    point(id("r2"), x + 8., -1., false),
                ]);
                s.lines.extend([
                    SketchLine {
                        id: id("line"),
                        start: id("a"),
                        end: id("b"),
                    },
                    SketchLine {
                        id: id("radius"),
                        start: id("c2"),
                        end: id("r2"),
                    },
                ]);
                s.circles.extend([
                    SketchCircle {
                        id: id("first"),
                        center: id("c"),
                        rim: id("r"),
                    },
                    SketchCircle {
                        id: id("second"),
                        center: id("c2"),
                        rim: id("r2"),
                    },
                ]);
                s.constraints.extend([
                    SketchConstraint::LineCircleTangent {
                        line: id("line"),
                        circle: id("second"),
                        side: SketchLineSide::Left,
                    },
                    SketchConstraint::CircleCircleTangent {
                        first: id("first"),
                        second: id("second"),
                        mode: SketchCircleTangency::External,
                    },
                    SketchConstraint::Radius {
                        curve: id("second"),
                        value: mm(2.),
                    },
                    SketchConstraint::Horizontal { line: id("radius") },
                ]);
            }
            let params = HashMap::new();
            let solved = s.solve(&params).unwrap();
            assert!(solved.solved);
            assert_eq!(solved.free_degrees, 0);
            assert!(
                s.constraint_checks(&params, &solved)
                    .unwrap()
                    .iter()
                    .all(|c| c.satisfied)
            );
        },
    );
    gate(
        "1000 signed point-line dimensions on tilted references",
        10,
        || {
            let mut s = sketch();
            for i in 0..1000 {
                let id = |name: &str| format!("{name}-{i}");
                let x = i as f64 * 20.;
                s.points.extend([
                    point(id("a"), x, 0., true),
                    point(id("b"), x + 3., 4., true),
                    point(id("p"), x + 2., 0., false),
                    point(id("axis"), x + 2., -20., true),
                ]);
                s.lines.extend([
                    SketchLine {
                        id: id("line"),
                        start: id("a"),
                        end: id("b"),
                    },
                    SketchLine {
                        id: id("vertical"),
                        start: id("axis"),
                        end: id("p"),
                    },
                ]);
                s.constraints.extend([
                    SketchConstraint::Vertical {
                        line: id("vertical"),
                    },
                    SketchConstraint::PointLineDistance {
                        point: id("p"),
                        line: id("line"),
                        value: mm(if i % 2 == 0 { 3. } else { -3. }),
                    },
                ]);
            }
            let params = HashMap::new();
            let solved = s.solve(&params).unwrap();
            assert!(solved.solved);
            assert_eq!(solved.free_degrees, 0);
            assert!(
                s.constraint_checks(&params, &solved)
                    .unwrap()
                    .iter()
                    .all(|c| c.satisfied)
            );
        },
    );
    gate(
        "1000 midpoint and concentric components with sparse equations",
        10,
        || {
            let mut s = sketch();
            for i in 0..1000 {
                let id = |name: &str| format!("{name}-{i}");
                let x = i as f64 * 20.;
                s.points.extend([
                    point(id("a"), x, 2., true),
                    point(id("b"), x + 8., 6., true),
                    point(id("mid"), x + 1., 1., false),
                    point(id("c"), x + 2., 3., false),
                    point(id("r"), x + 10., 3., true),
                    point(id("fixed-rim"), x + 10., 4., true),
                ]);
                s.lines.push(SketchLine {
                    id: id("line"),
                    start: id("a"),
                    end: id("b"),
                });
                s.circles.extend([
                    SketchCircle {
                        id: id("first"),
                        center: id("mid"),
                        rim: id("fixed-rim"),
                    },
                    SketchCircle {
                        id: id("second"),
                        center: id("c"),
                        rim: id("r"),
                    },
                ]);
                s.constraints.extend([
                    SketchConstraint::Midpoint {
                        point: id("mid"),
                        line: id("line"),
                    },
                    SketchConstraint::Concentric {
                        first: id("first"),
                        second: id("second"),
                    },
                ]);
            }
            let params = HashMap::new();
            let solved = s.solve(&params).unwrap();
            assert!(solved.solved);
            assert_eq!(solved.free_degrees, 0);
            assert!(
                s.constraint_checks(&params, &solved)
                    .unwrap()
                    .iter()
                    .all(|c| c.satisfied)
            );
        },
    );
    gate(
        "1000 linked edge-projection sketches, runtime snapshots and source-depth edits",
        10,
        || {
            let request: serde_json::Value = serde_json::from_str(include_str!(
                "../../../tools/model/projected-pocket.request.json"
            ))
            .unwrap();
            let mut document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
            let family = &mut document.family;
            family.requirements.clear();
            let profile = family
                .features
                .iter()
                .find(|f| f.id == "profile")
                .unwrap()
                .clone();
            family.features.retain(|f| f.id == "block");
            for i in 0..1000 {
                let mut f = profile.clone();
                f.id = format!("profile-{i}");
                family.features.push(f);
            }
            let session = Session::new().unwrap();
            let mut part = PartInstance {
                id: "part".into(),
                definition: family,
                overrides: HashMap::new(),
                provenance: "bench".into(),
            };
            let first = part.regenerate(&session).unwrap();
            let count = session.shape_count().unwrap();
            let snapshots = part.resolved_sketches(&session, &first).unwrap();
            assert_eq!(snapshots.len(), 1000);
            for snapshot in snapshots.values() {
                let sketch = &snapshot.as_ref().unwrap().sketch;
                assert!(sketch.projections.is_empty());
                assert_eq!(sketch.points.len(), 10);
            }
            assert_eq!(session.shape_count().unwrap(), count);
            drop(snapshots);
            part.overrides.insert(
                "depth".into(),
                ParameterValue::Scalar(Quantity::length(50.0, LengthUnit::Millimeter)),
            );
            let edited = part.regenerate_incremental(&session, &first).unwrap();
            assert_eq!(edited.regeneration.rebuilt.len(), 1001);
            for i in 0..1000 {
                let bounds = session
                    .exact_bounds(edited.shape(&format!("profile-{i}")).unwrap())
                    .unwrap();
                assert!((bounds.min.y - 34.0).abs() < 1e-7 && (bounds.max.y - 46.0).abs() < 1e-7);
            }
            drop((first, edited));
            assert_eq!(session.shape_count().unwrap(), 0);
        },
    );
    gate(
        "1000 face-attached sketch profiles, plane queries and height edits",
        10,
        || {
            let request: serde_json::Value = serde_json::from_str(include_str!(
                "../../../tools/model/face-pocket.request.json"
            ))
            .unwrap();
            let mut document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
            let family = &mut document.family;
            family.requirements.clear();
            let profile = family
                .features
                .iter()
                .find(|f| f.id == "profile")
                .unwrap()
                .clone();
            family.features.retain(|f| f.id == "block");
            for i in 0..1000 {
                let mut f = profile.clone();
                f.id = format!("profile-{i}");
                family.features.push(f);
            }
            let session = Session::new().unwrap();
            let mut part = PartInstance {
                id: "part".into(),
                definition: family,
                overrides: HashMap::new(),
                provenance: "bench".into(),
            };
            let first = part.regenerate(&session).unwrap();
            let count = session.shape_count().unwrap();
            let planes = part.sketch_support_planes(&session, &first).unwrap();
            assert_eq!(planes.len(), 1000);
            for plane in planes.values() {
                let ResolvedDatum::Plane { origin, .. } = plane.as_ref().unwrap() else {
                    panic!()
                };
                assert!((origin.z - 20.0).abs() < 1e-7);
            }
            assert_eq!(session.shape_count().unwrap(), count);
            part.overrides.insert(
                "height".into(),
                ParameterValue::Scalar(Quantity::length(30.0, LengthUnit::Millimeter)),
            );
            let edited = part.regenerate_incremental(&session, &first).unwrap();
            assert_eq!(edited.regeneration.rebuilt.len(), 1001);
            for i in 0..1000 {
                assert!(
                    (session
                        .exact_bounds(edited.shape(&format!("profile-{i}")).unwrap())
                        .unwrap()
                        .min
                        .z
                        - 30.0)
                        .abs()
                        < 1e-7
                );
            }
            drop((first, edited));
            assert_eq!(session.shape_count().unwrap(), 0);
        },
    );
    gate(
        "1000 equal-radius circle pairs with residual diagnostics",
        10,
        || {
            let mut s = sketch();
            for i in 0..1000 {
                let x = i as f64 * 30.0;
                let id = |p: &str| format!("{p}-{i}");
                for (name, dx, fixed) in [
                    ("a", 0.0, true),
                    ("ar", 3.0, true),
                    ("b", 10.0, true),
                    ("br", 14.0, false),
                ] {
                    s.points.push(point(id(name), x + dx, 0.0, fixed));
                }
                for (name, c, r) in [("first", "a", "ar"), ("second", "b", "br")] {
                    s.circles.push(SketchCircle {
                        id: id(name),
                        center: id(c),
                        rim: id(r),
                    });
                }
                s.lines.push(SketchLine {
                    id: id("axis"),
                    start: id("b"),
                    end: id("br"),
                });
                s.constraints
                    .push(SketchConstraint::Horizontal { line: id("axis") });
                s.constraints.push(SketchConstraint::EqualRadius {
                    first: id("first"),
                    second: id("second"),
                });
            }
            s.profile = vec!["first-0".into()];
            let parameters = HashMap::new();
            let solution = s.solve(&parameters).unwrap();
            assert!(solution.solved);
            assert_eq!(solution.free_degrees, 0);
            for i in 0..1000 {
                assert!(
                    (solution.points[&format!("br-{i}")].x - (i as f64 * 30.0 + 13.0)).abs() < 1e-7
                );
            }
            let checks = s.constraint_checks(&parameters, &solution).unwrap();
            assert_eq!(checks.len(), 2000);
            assert!(checks.iter().all(|c| c.satisfied && !c.by_construction));
        },
    );
    gate("1000 coupled advanced sketch components", 10, || {
        let mut s = sketch();
        for i in 0..1000 {
            let x = i as f64 * 30.;
            let id = |p: &str| format!("{p}{i}");
            for (p, dx, y, fixed) in [
                ("c", 0., 0., true),
                ("a", 8., 0., true),
                ("b", 0., 3., false),
                ("p", 4., 4., false),
                ("r", 3., 0., false),
                ("m", -5., 1., false),
                ("t", 5., 3., false),
            ] {
                s.points.push(point(id(p), x + dx, y, fixed));
            }
            for (l, a, b) in [
                ("x", "c", "a"),
                ("axis", "c", "b"),
                ("radial", "c", "r"),
                ("ray", "c", "t"),
            ] {
                s.lines.push(SketchLine {
                    id: id(l),
                    start: id(a),
                    end: id(b),
                });
            }
            s.circles.push(SketchCircle {
                id: id("circle"),
                center: id("c"),
                rim: id("r"),
            });
            s.ellipses.push(SketchEllipse {
                id: id("ellipse"),
                center: id("c"),
                major: id("a"),
                minor: id("b"),
                start: None,
                end: None,
                clockwise: false,
            });
            s.constraints.extend([
                SketchConstraint::Distance {
                    first: id("c"),
                    second: id("b"),
                    value: mm(4.),
                },
                SketchConstraint::PointOnCurve {
                    point: id("p"),
                    curve: id("ellipse"),
                },
                SketchConstraint::Radius {
                    curve: id("circle"),
                    value: mm(6.),
                },
                SketchConstraint::Diameter {
                    curve: id("circle"),
                    value: mm(12.),
                },
                SketchConstraint::Horizontal { line: id("radial") },
                SketchConstraint::Symmetric {
                    first: id("a"),
                    second: id("m"),
                    axis: id("axis"),
                },
                SketchConstraint::Angle {
                    first: id("x"),
                    second: id("ray"),
                    value: scalar(std::f64::consts::FRAC_PI_3),
                },
                SketchConstraint::Distance {
                    first: id("c"),
                    second: id("t"),
                    value: mm(10.),
                },
            ]);
        }
        let result = s.solve(&HashMap::new()).unwrap();
        assert!(result.solved, "residual {}", result.max_residual);
        assert_eq!(result.free_degrees, 1000);
        for i in [0, 499, 999] {
            let x = i as f64 * 30.;
            assert!((result.points[&format!("m{i}")].x - (x - 8.)).abs() < 1e-7);
            assert!((result.points[&format!("r{i}")].x - (x + 6.)).abs() < 1e-7);
        }
    });
    gate("1000 native point-on-spline components", 10, || {
        let mut s = sketch();
        for i in 0..1000 {
            let x = i as f64 * 20.;
            let id = |p: &str| format!("{p}{i}");
            for (p, dx, y, fixed) in [
                ("a", 0., 0., true),
                ("b", 5., 3., true),
                ("c", 10., 0., true),
                ("p", 3., 4., false),
            ] {
                s.points.push(point(id(p), x + dx, y, fixed));
            }
            s.splines.push(SketchSpline {
                endpoints: None,
                basis: None,
                id: id("s"),
                points: vec![id("a"), id("b"), id("c")],
            });
            s.constraints.push(SketchConstraint::PointOnCurve {
                point: id("p"),
                curve: id("s"),
            });
        }
        let result = s.solve(&HashMap::new()).unwrap();
        assert!(result.solved, "spline residual {}", result.max_residual);
        assert_eq!(result.free_degrees, 1000);
    });
    gate(
        "1000 native trim-extend-offset profile regenerations",
        10,
        || {
            let mut s = sketch();
            s.points = vec![
                point("c".into(), 0., 0., true),
                point("r".into(), 4., 0., true),
            ];
            s.circles.push(SketchCircle {
                id: "circle".into(),
                center: "c".into(),
                rim: "r".into(),
            });
            s.profile_operations = vec![
                SketchProfileOperation::Trim {
                    entity: "circle".into(),
                    first: scalar(0.),
                    last: scalar(0.25),
                },
                SketchProfileOperation::Extend {
                    entity: "circle".into(),
                    start: mm(0.),
                    end: mm(2. * std::f64::consts::PI),
                },
                SketchProfileOperation::Offset {
                    distance: mm(1.),
                    join: SketchOffsetJoin::Arc,
                },
            ];
            let mut family:FamilyDefinition=serde_json::from_value(serde_json::json!({"id":"advanced-profiles","version":1,"parameters":[],"features":[],"requirements":[]})).unwrap();
            family.features.push(FeatureDefinition {
                id: "profile".into(),
                operation: FeatureOperation::SketchOpenWire {
                    sketch: Box::new(s),
                },
            });
            let part = PartInstance {
                id: "part".into(),
                definition: &family,
                overrides: HashMap::new(),
                provenance: "scale".into(),
            };
            let session = Session::new().unwrap();
            for _ in 0..1000 {
                let generated = part.regenerate(&session).unwrap();
                let bounds = session
                    .exact_bounds(generated.shape("profile").unwrap())
                    .unwrap();
                assert!(
                    (bounds.min.x + 5.).abs() < 1e-6
                        && (bounds.max.x - 5.).abs() < 1e-6
                        && (bounds.max.y - 5.).abs() < 1e-6,
                    "offset half-circle bounds {bounds:?}"
                );
                drop(generated);
                assert_eq!(session.shape_count().unwrap(), 0);
            }
        },
    );
    gate(
        "1000 native spline-corner fillet/chamfer profile regenerations",
        10,
        || {
            let request: serde_json::Value = serde_json::from_str(include_str!(
                "../../../tools/model/spline-corner-pad.request.json"
            ))
            .unwrap();
            let doc = ModelDocument::from_json(&request["model"].to_string()).unwrap();
            let mut family = doc.family.clone();
            family.features.retain(|f| f.id == "profile");
            family.requirements.clear();
            let session = Session::new().unwrap();
            let mut part = PartInstance {
                id: "part".into(),
                definition: &family,
                overrides: HashMap::new(),
                provenance: "scale".into(),
            };
            for i in 0..1000 {
                let r = [2., 3., 4., 5.][i % 4];
                part.overrides.insert(
                    "fillet_radius".into(),
                    ParameterValue::Scalar(Quantity::length(r, LengthUnit::Millimeter)),
                );
                let built = part.regenerate(&session).unwrap();
                let face = built.shape("profile").unwrap();
                let radii = session
                    .subshapes(face, ShapeType::Edge)
                    .unwrap()
                    .iter()
                    .filter_map(|e| session.edge_circle_radius(e).unwrap())
                    .collect::<Vec<_>>();
                assert!(radii.len() == 1 && (radii[0] - r).abs() < 1e-9);
                drop(built);
                assert_eq!(session.shape_count().unwrap(), 0);
            }
        },
    );
    gate(
        "1000 mirrored-flange profiles with circular bolt patterns and sketch holes",
        10,
        || {
            let request: serde_json::Value = serde_json::from_str(include_str!(
                "../../../tools/model/mirrored-flange.request.json"
            ))
            .unwrap();
            let doc = ModelDocument::from_json(&request["model"].to_string()).unwrap();
            let mut family = doc.family.clone();
            family.features.retain(|f| f.id == "profile");
            family.requirements.clear();
            let session = Session::new().unwrap();
            let mut part = PartInstance {
                id: "part".into(),
                definition: &family,
                overrides: HashMap::new(),
                provenance: "scale".into(),
            };
            for i in 0..1000 {
                let width = [60., 70., 80., 90.][i % 4];
                part.overrides.insert(
                    "width".into(),
                    ParameterValue::Scalar(Quantity::length(width, LengthUnit::Millimeter)),
                );
                let built = part.regenerate(&session).unwrap();
                let face = built.shape("profile").unwrap();
                let area = width * 30. + std::f64::consts::PI * (width * width / 8. - 64. - 54.);
                assert!((session.surface_area(face).unwrap() - area).abs() < 1e-6 * area);
                drop(built);
                assert_eq!(session.shape_count().unwrap(), 0);
            }
        },
    );
    gate(
        "one 1000-instance circular sketch pattern solved and cut as holes",
        10,
        || {
            let pattern = serde_json::json!({"circular": {"id": "ring", "entities": ["hole"],
                "center": "O", "count": 1000}});
            let point = |id: &str, x: f64, y: f64| {
                serde_json::json!({"id": id,
                "x": {"literal": {"value": x, "dimension": "length", "unit": "millimeter"}},
                "y": {"literal": {"value": y, "dimension": "length", "unit": "millimeter"}},
                "fixed": id == "O" || id == "R"})
            };
            let holes = std::iter::once(serde_json::json!(["hole"]))
                .chain((1..1000).map(|k| serde_json::json!([format!("ring:{k}:hole")])))
                .collect::<Vec<_>>();
            let sketch: SketchDefinition = serde_json::from_value(serde_json::json!({
                "id": "ring",
                "origin": {"literal": {"x": {"value": 0, "dimension": "length", "unit": "millimeter"},
                    "y": {"value": 0, "dimension": "length", "unit": "millimeter"},
                    "z": {"value": 0, "dimension": "length", "unit": "millimeter"}}},
                "x_axis": {"literal": {"x": {"value": 1, "dimension": "scalar", "unit": null},
                    "y": {"value": 0, "dimension": "scalar", "unit": null},
                    "z": {"value": 0, "dimension": "scalar", "unit": null}}},
                "y_axis": {"literal": {"x": {"value": 0, "dimension": "scalar", "unit": null},
                    "y": {"value": 1, "dimension": "scalar", "unit": null},
                    "z": {"value": 0, "dimension": "scalar", "unit": null}}},
                "points": [point("O", 0., 0.), point("R", 1100., 0.), point("h", 1000., 0.),
                    point("r", 1001., 0.)],
                "lines": [{"id": "ray", "start": "h", "end": "r"}],
                "circles": [{"id": "rim", "center": "O", "rim": "R"},
                    {"id": "hole", "center": "h", "rim": "r"}],
                "profile": ["rim"],
                "holes": holes,
                "patterns": [pattern],
                "constraints": [{"horizontal": {"line": "ray"}},
                    {"radius": {"curve": "hole", "value": {"literal": {"value": 1.5,
                        "dimension": "length", "unit": "millimeter"}}}}],
            }))
            .unwrap();
            let diagnosed = sketch.solve(&HashMap::new()).unwrap();
            assert!(diagnosed.solved && diagnosed.diagnosis.complete);
            assert_eq!(diagnosed.diagnosis.free_coordinates.len(), 2);
            assert!(
                diagnosed
                    .diagnosis
                    .free_motions
                    .iter()
                    .all(|m| m.omitted_points > 1000)
            );
            let mut family = ModelDocument::from_json(
                &serde_json::from_str::<serde_json::Value>(include_str!(
                    "../../../tools/model/mirrored-flange.request.json"
                ))
                .unwrap()["model"]
                    .to_string(),
            )
            .unwrap()
            .family;
            family.requirements.clear();
            family.features = vec![FeatureDefinition {
                id: "profile".into(),
                operation: FeatureOperation::SketchFace {
                    sketch: Box::new(sketch),
                },
            }];
            let session = Session::new().unwrap();
            let part = PartInstance {
                id: "part".into(),
                definition: &family,
                overrides: HashMap::new(),
                provenance: "scale".into(),
            };
            let built = part.regenerate(&session).unwrap();
            let face = built.shape("profile").unwrap();
            assert_eq!(session.subshape_count(face, ShapeType::Wire).unwrap(), 1001);
            let area = std::f64::consts::PI * (1100. * 1100. - 1000. * 1.5 * 1.5);
            assert!((session.surface_area(face).unwrap() - area).abs() < 1e-6 * area);
        },
    );
    gate(
        "1000-segment chain driven by axis distances, line angles and curve lengths, with 1000 reference dimensions",
        10,
        || {
            // Even segments: axis distances. Odd segments: line angle plus
            // curve length.
            let mut s = sketch();
            s.points = (0..=1000)
                .map(|i| {
                    point(
                        format!("p{i}"),
                        3.0 * i as f64,
                        0.1 * (i % 7) as f64,
                        i == 0,
                    )
                })
                .collect();
            s.lines = (0..1000)
                .map(|i| SketchLine {
                    id: format!("l{i}"),
                    start: format!("p{i}"),
                    end: format!("p{}", i + 1),
                })
                .collect();
            for i in 0..1000 {
                let (a, b) = (format!("p{i}"), format!("p{}", i + 1));
                if i % 2 == 0 {
                    s.constraints.push(SketchConstraint::HorizontalDistance {
                        first: a.clone(),
                        second: b.clone(),
                        value: mm(3.0),
                    });
                    s.constraints.push(SketchConstraint::VerticalDistance {
                        first: a,
                        second: b,
                        value: mm(1.0),
                    });
                } else {
                    s.constraints.push(SketchConstraint::LineAngle {
                        line: format!("l{i}"),
                        value: scalar(0.25),
                    });
                    s.constraints.push(SketchConstraint::CurveLength {
                        curve: format!("l{i}"),
                        value: mm(2.0),
                    });
                }
            }
            s.measurements = (0..1000)
                .map(|i| SketchMeasurement {
                    id: format!("m{i}"),
                    measure: SketchMeasure::Distance {
                        first: "p0".into(),
                        second: format!("p{}", i + 1),
                    },
                })
                .collect();
            let solution = s.solve(&HashMap::new()).unwrap();
            assert!(solution.solved && solution.free_degrees == 0);
            let end = solution.points["p1000"];
            let (x, y) = (
                500. * (3. + 2. * 0.25f64.cos()),
                500. * (1. + 2. * 0.25f64.sin()),
            );
            assert!((end.x - x).abs() < 1e-7 && (end.y - y).abs() < 1e-7);
            let measured = s.measure(&HashMap::new(), &solution).unwrap();
            assert!((measured[999].1 - x.hypot(y)).abs() < 1e-7);
        },
    );
    gate("200 splines driven by native curve lengths", 10, || {
        let mut s = sketch();
        for k in 0..200 {
            let y = 20.0 * k as f64;
            s.points.extend([
                point(format!("a{k}"), 0., y, true),
                point(format!("m{k}"), 25., y + 3., false),
                point(format!("b{k}"), 50., y, true),
            ]);
            s.splines.push(SketchSpline {
                id: format!("s{k}"),
                points: vec![format!("a{k}"), format!("m{k}"), format!("b{k}")],
                basis: None,
                endpoints: None,
            });
            s.constraints.push(SketchConstraint::HorizontalDistance {
                first: format!("a{k}"),
                second: format!("m{k}"),
                value: mm(25.),
            });
            s.constraints.push(SketchConstraint::CurveLength {
                curve: format!("s{k}"),
                value: mm(55. + 0.01 * k as f64),
            });
            s.measurements.push(SketchMeasurement {
                id: format!("len{k}"),
                measure: SketchMeasure::CurveLength {
                    curve: format!("s{k}"),
                },
            });
        }
        let solution = s.solve(&HashMap::new()).unwrap();
        assert!(solution.solved && solution.free_degrees == 0);
        for (k, (_, length)) in s
            .measure(&HashMap::new(), &solution)
            .unwrap()
            .iter()
            .enumerate()
        {
            assert!((length - (55. + 0.01 * k as f64)).abs() < 1e-7);
        }
    });
    gate(
        "500 slot holes and a 1000-sided polygon expanded, solved and built as one face",
        10,
        || {
            let mut s = sketch();
            s.points = vec![
                point("a".into(), 0., 0., true),
                point("c".into(), 1000., 500., true),
            ];
            s.shapes.push(SketchShape::Rectangle {
                id: "plate".into(),
                first: "a".into(),
                opposite: "c".into(),
                angle: None,
            });
            s.profile = vec!["plate".into()];
            for k in 0..500 {
                let (x, y) = (10. + 20. * (k % 25) as f64, 10. + 20. * (k / 25) as f64);
                s.points.extend([
                    point(format!("s{k}a"), x, y, true),
                    point(format!("s{k}b"), x + 8., y, true),
                ]);
                s.shapes.push(SketchShape::Slot {
                    id: format!("slot{k}"),
                    start: format!("s{k}a"),
                    end: format!("s{k}b"),
                    radius: mm(3.),
                });
                s.holes.push(vec![format!("slot{k}")]);
            }
            s.points.extend([
                point("pc".into(), 750., 250., true),
                point("pv".into(), 950., 250., true),
            ]);
            s.shapes.push(SketchShape::Polygon {
                id: "gear".into(),
                center: "pc".into(),
                vertex: "pv".into(),
                sides: 1000,
            });
            s.holes.push(vec!["gear".into()]);
            let solution = s.solve(&HashMap::new()).unwrap();
            assert!(solution.solved && solution.free_degrees == 0);
            let mut family = ModelDocument::from_json(
                &serde_json::from_str::<serde_json::Value>(include_str!(
                    "../../../tools/model/slotted-plate.request.json"
                ))
                .unwrap()["model"]
                    .to_string(),
            )
            .unwrap()
            .family;
            family.requirements.clear();
            family.features = vec![FeatureDefinition {
                id: "profile".into(),
                operation: FeatureOperation::SketchFace {
                    sketch: Box::new(s),
                },
            }];
            let session = Session::new().unwrap();
            let part = PartInstance {
                id: "part".into(),
                definition: &family,
                overrides: HashMap::new(),
                provenance: "scale".into(),
            };
            let built = part.regenerate(&session).unwrap();
            let face = built.shape("profile").unwrap();
            assert_eq!(session.subshape_count(face, ShapeType::Wire).unwrap(), 502);
            let slot = 2. * 3. * 8. + std::f64::consts::PI * 9.;
            let gear = 0.5 * 1000. * 200. * 200. * (std::f64::consts::TAU / 1000.).sin();
            let area = 1000. * 500. - 500. * slot - gear;
            assert!((session.surface_area(face).unwrap() - area).abs() < 1e-6 * area);
        },
    );
    gate(
        "500 arched-panel rise edits with exact elliptical-arc volumes",
        10,
        || {
            let request: serde_json::Value = serde_json::from_str(include_str!(
                "../../../tools/model/arched-panel.request.json"
            ))
            .unwrap();
            let family = ModelDocument::from_json(&request["model"].to_string())
                .unwrap()
                .family;
            let session = Session::new().unwrap();
            let mut part = PartInstance {
                id: "panel".into(),
                definition: &family,
                overrides: HashMap::new(),
                provenance: "scale".into(),
            };
            let mut previous = part.regenerate(&session).unwrap();
            for k in 0..500 {
                let rise = 6. + (k % 10) as f64;
                part.overrides.insert(
                    "rise".into(),
                    ParameterValue::Scalar(Quantity::length(rise, LengthUnit::Millimeter)),
                );
                let built = part.regenerate_incremental(&session, &previous).unwrap();
                let body = built.shape("body").unwrap();
                let area = 40. * 30. + std::f64::consts::PI * 20. * rise / 2.;
                assert!((session.volume(body).unwrap() - 3. * area).abs() < 1e-7 * area);
                previous = built;
            }
        },
    );
    gate(
        "500 conic nose-cone rho edits regenerated as revolved solids",
        10,
        || {
            let request: serde_json::Value =
                serde_json::from_str(include_str!("../../../tools/model/conic-nose.request.json"))
                    .unwrap();
            let family = ModelDocument::from_json(&request["model"].to_string())
                .unwrap()
                .family;
            let session = Session::new().unwrap();
            let mut part = PartInstance {
                id: "nose".into(),
                definition: &family,
                overrides: HashMap::new(),
                provenance: "scale".into(),
            };
            let mut previous = part.regenerate(&session).unwrap();
            let mut volumes = [0.0; 5];
            for k in 0..500 {
                let rho = 0.3 + 0.1 * (k % 5) as f64;
                part.overrides
                    .insert("rho".into(), ParameterValue::Scalar(Quantity::scalar(rho)));
                let built = part.regenerate_incremental(&session, &previous).unwrap();
                volumes[k % 5] = session.volume(built.shape("body").unwrap()).unwrap();
                previous = built;
            }
            // Fuller shoulders hold more volume.
            assert!(volumes.windows(2).all(|pair| pair[0] < pair[1]));
        },
    );
    gate(
        "500 keyhole neck edits trimmed at native crossings and extruded",
        10,
        || {
            let request: serde_json::Value = serde_json::from_str(include_str!(
                "../../../tools/model/keyhole-tag.request.json"
            ))
            .unwrap();
            let family = ModelDocument::from_json(&request["model"].to_string())
                .unwrap()
                .family;
            let session = Session::new().unwrap();
            let mut part = PartInstance {
                id: "tag".into(),
                definition: &family,
                overrides: HashMap::new(),
                provenance: "scale".into(),
            };
            let mut previous = part.regenerate(&session).unwrap();
            for k in 0..500 {
                let neck = 3. + 0.5 * (k % 10) as f64;
                part.overrides.insert(
                    "neck_width".into(),
                    ParameterValue::Scalar(Quantity::length(neck, LengthUnit::Millimeter)),
                );
                let built = part.regenerate_incremental(&session, &previous).unwrap();
                assert!(session.is_valid(built.shape("body").unwrap()).unwrap());
                previous = built;
            }
        },
    );
    gate(
        "1000 disjoint rectangle regions in one sketch built and extruded into 1001 solids",
        10,
        || {
            let mut s = sketch();
            let mut rectangle = |k: usize, x: f64, y: f64| {
                s.points.extend([
                    point(format!("a{k}"), x, y, true),
                    point(format!("b{k}"), x + 4., y + 4., true),
                ]);
                s.shapes.push(SketchShape::Rectangle {
                    id: format!("r{k}"),
                    first: format!("a{k}"),
                    opposite: format!("b{k}"),
                    angle: None,
                });
            };
            for k in 0..=1000 {
                rectangle(k, 6. * (k % 40) as f64, 6. * (k / 40) as f64);
            }
            s.profile = vec!["r0".into()];
            s.regions = (1..=1000)
                .map(|k| SketchRegion {
                    profile: vec![format!("r{k}")],
                    holes: vec![],
                })
                .collect();
            let mut family = ModelDocument::from_json(
                &serde_json::from_str::<serde_json::Value>(include_str!(
                    "../../../tools/model/spacer-set.request.json"
                ))
                .unwrap()["model"]
                    .to_string(),
            )
            .unwrap()
            .family;
            family.requirements.clear();
            let FeatureOperation::SketchFace { sketch: profile } =
                &mut family.features[0].operation
            else {
                panic!("spacer-set starts with its sketch")
            };
            **profile = s;
            let session = Session::new().unwrap();
            let part = PartInstance {
                id: "part".into(),
                definition: &family,
                overrides: HashMap::new(),
                provenance: "scale".into(),
            };
            let built = part.regenerate(&session).unwrap();
            let body = built.shape("body").unwrap();
            assert_eq!(
                session.subshape_count(body, ShapeType::Solid).unwrap(),
                1001
            );
            assert!((session.volume(body).unwrap() - 1001. * 16. * 2.).abs() < 1e-6);
        },
    );
    gate(
        "500 G2 paddle edits solving four curvature-continuous joints",
        10,
        || {
            let request: serde_json::Value =
                serde_json::from_str(include_str!("../../../tools/model/g2-paddle.request.json"))
                    .unwrap();
            let family = ModelDocument::from_json(&request["model"].to_string())
                .unwrap()
                .family;
            let session = Session::new().unwrap();
            let mut part = PartInstance {
                id: "paddle".into(),
                definition: &family,
                overrides: HashMap::new(),
                provenance: "scale".into(),
            };
            let mut previous = part.regenerate(&session).unwrap();
            for k in 0..500 {
                let rise = 6. + (k % 10) as f64;
                part.overrides.insert(
                    "rise".into(),
                    ParameterValue::Scalar(Quantity::length(rise, LengthUnit::Millimeter)),
                );
                let built = part.regenerate_incremental(&session, &previous).unwrap();
                let bounds = session.exact_bounds(built.shape("body").unwrap()).unwrap();
                assert!((bounds.max.y - (30. + rise + 12.)).abs() < 1e-6);
                previous = built;
            }
        },
    );
    gate(
        "1000-character text sketch built as glyph faces and extruded",
        10,
        || {
            let session = Session::new().unwrap();
            let Some(font) = ["Liberation Sans", "DejaVu Sans", "Noto Sans", "FreeSans"]
                .into_iter()
                .find(|f| {
                    session
                        .font_file(f, occt_bridge::FontStyle::Regular)
                        .is_ok()
                })
            else {
                println!("SKIP text gate: no common sans font installed");
                return;
            };
            let mut s = sketch();
            s.points = vec![point("t".into(), 0., 0., true)];
            s.texts = vec![SketchText {
                id: "long".into(),
                text: "The quick brown fox jumps over the lazy dog 0123456789. "
                    .repeat(18)
                    .chars()
                    .take(1000)
                    .collect(),
                font: font.into(),
                style: SketchFontStyle::Regular,
                height: mm(5.),
                anchor: "t".into(),
                angle: None,
                align: SketchTextAlign::Start,
            }];
            let mut family = ModelDocument::from_json(
                &serde_json::from_str::<serde_json::Value>(include_str!(
                    "../../../tools/model/spacer-set.request.json"
                ))
                .unwrap()["model"]
                    .to_string(),
            )
            .unwrap()
            .family;
            family.requirements.clear();
            let FeatureOperation::SketchFace { sketch: profile } =
                &mut family.features[0].operation
            else {
                panic!("spacer-set starts with its sketch")
            };
            **profile = s;
            let part = PartInstance {
                id: "part".into(),
                definition: &family,
                overrides: HashMap::new(),
                provenance: "scale".into(),
            };
            let built = part.regenerate(&session).unwrap();
            let body = built.shape("body").unwrap();
            assert!(session.subshape_count(body, ShapeType::Solid).unwrap() > 700);
            assert!(session.is_valid(body).unwrap());
        },
    );
}
