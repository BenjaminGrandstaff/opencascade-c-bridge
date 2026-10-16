//! Constructed datum planes (schema 165): offset, angle, face, midplane and
//! three-point planes, chained on each other, driving sketches and
//! following the faces and parameters they are built from.
use super::*;

fn document() -> ModelDocument {
    let r: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tools/model/face-pocket.request.json"
    ))
    .unwrap();
    ModelDocument::from_json(&r["model"].to_string()).unwrap()
}
fn part(f: &FamilyDefinition) -> PartInstance<'_> {
    PartInstance {
        id: "part".into(),
        definition: f,
        overrides: HashMap::new(),
        provenance: "test".into(),
    }
}
fn sketch(f: &mut FamilyDefinition) -> &mut SketchDefinition {
    let FeatureOperation::SketchFace { sketch } = &mut f
        .features
        .iter_mut()
        .find(|x| x.id == "profile")
        .unwrap()
        .operation
    else {
        panic!()
    };
    sketch
}
fn mm(v: f64) -> ScalarExpr {
    ScalarExpr::Literal(Quantity::length(v, LengthUnit::Millimeter))
}
fn scalars(x: f64, y: f64, z: f64) -> VectorExpr {
    VectorExpr::Literal(VectorQuantity::scalars(x, y, z))
}
fn lengths(x: f64, y: f64, z: f64) -> VectorExpr {
    VectorExpr::Literal(VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter))
}
fn facing(x: f64, y: f64, z: f64) -> Box<FaceSelector> {
    Box::new(FaceSelector::NormalAligned {
        direction: scalars(x, y, z),
        minimum_dot: ScalarExpr::Literal(Quantity::scalar(0.999999)),
    })
}
fn datum(id: &str, kind: DatumKind) -> DatumDefinition {
    DatumDefinition {
        id: id.into(),
        kind,
    }
}
fn length_parameter(id: &str, value: f64) -> ParameterDefinition {
    ParameterDefinition {
        id: id.into(),
        parameter_type: ParameterType::Scalar(Dimension::Length),
        default: ParameterValue::Scalar(Quantity::length(value, LengthUnit::Millimeter)),
        minimum: None,
        maximum: None,
    }
}
fn set(p: &mut PartInstance<'_>, id: &str, value: f64) {
    p.overrides.insert(
        id.into(),
        ParameterValue::Scalar(Quantity::length(value, LengthUnit::Millimeter)),
    );
}
fn plane(planes: &[(String, Result<ResolvedDatum, ModelError>)], id: &str) -> (Vec3, Vec3) {
    match planes
        .iter()
        .find(|(d, _)| d == id)
        .unwrap()
        .1
        .as_ref()
        .unwrap()
    {
        ResolvedDatum::Plane { origin, normal } => (*origin, *normal),
        other => panic!("{other:?}"),
    }
}
fn close(a: Vec3, b: [f64; 3]) -> bool {
    (a.x - b[0]).abs() < 1e-7 && (a.y - b[1]).abs() < 1e-7 && (a.z - b[2]).abs() < 1e-7
}

/// The face-pocket part with its pocket sketch moved from face support onto
/// a datum plane `gap` above the block's top face.
fn on_lid() -> ModelDocument {
    let mut d = document();
    d.family.parameters.push(length_parameter("gap", 5.0));
    d.family.datums.push(datum(
        "lid",
        DatumKind::FacePlane {
            input: "block".into(),
            face: facing(0., 0., 1.),
            distance: ScalarExpr::Parameter("gap".into()),
        },
    ));
    let s = sketch(&mut d.family);
    s.face_support = None;
    s.datum_plane = Some("lid".into());
    ModelDocument::from_json(&d.to_json_pretty().unwrap()).unwrap()
}

#[test]
fn a_face_plane_follows_its_face_and_distance_and_rebuilds_only_what_reads_them() {
    let d = on_lid();
    let session = Session::new().unwrap();
    let mut p = part(&d.family);
    let first = p.regenerate(&session).unwrap();
    let z = |result: &GeneratedResult<'_>| {
        session
            .exact_bounds(result.shape("profile").unwrap())
            .unwrap()
            .min
            .z
    };
    assert!((z(&first) - 25.).abs() < 1e-7);
    assert_eq!(
        d.family.feature_inputs().unwrap()["profile"],
        vec!["block"],
        "the sketch depends on the face its plane reads"
    );
    set(&mut p, "height", 30.);
    let taller = p.regenerate_incremental(&session, &first).unwrap();
    assert_eq!(
        taller.regeneration.rebuilt,
        ["block", "profile", "tool", "body"]
    );
    assert!((z(&taller) - 35.).abs() < 1e-7);
    set(&mut p, "gap", 8.);
    let wider = p.regenerate_incremental(&session, &taller).unwrap();
    assert_eq!(wider.regeneration.rebuilt, ["profile", "tool", "body"]);
    assert_eq!(wider.regeneration.reused, ["block"]);
    assert!((z(&wider) - 38.).abs() < 1e-7);
    let (origin, normal) = plane(&p.datum_planes(&session, &wider).unwrap(), "lid");
    assert!(close(origin, [30., 20., 38.]) && close(normal, [0., 0., 1.]));
}

#[test]
fn offset_angle_midplane_and_three_point_planes_resolve_and_chain() {
    let mut d = on_lid();
    let quarter = std::f64::consts::FRAC_PI_2;
    d.family.datums.extend([
        datum(
            "front",
            DatumKind::OffsetPlane {
                base: PlaneBase::Xz,
                distance: mm(7.),
            },
        ),
        datum(
            "raised",
            DatumKind::OffsetPlane {
                base: PlaneBase::Datum("lid".into()),
                distance: mm(3.),
            },
        ),
        datum(
            "tilt",
            DatumKind::AnglePlane {
                base: PlaneBase::Datum("raised".into()),
                hinge: scalars(1., 0., 0.),
                angle_radians: ScalarExpr::Literal(Quantity::scalar(quarter)),
            },
        ),
        datum(
            "middle",
            DatumKind::Midplane {
                input: "block".into(),
                first: facing(0., 0., 1.),
                second: facing(0., 0., -1.),
            },
        ),
        datum(
            "slope",
            DatumKind::ThreePointPlane {
                first: lengths(0., 0., 0.),
                second: lengths(10., 0., 0.),
                third: lengths(0., 10., 10.),
            },
        ),
    ]);
    let d = ModelDocument::from_json(&d.to_json_pretty().unwrap()).unwrap();
    let session = Session::new().unwrap();
    let p = part(&d.family);
    let generated = p.regenerate(&session).unwrap();
    let planes = p.datum_planes(&session, &generated).unwrap();
    let (origin, normal) = plane(&planes, "front");
    assert!(close(origin, [0., -7., 0.]) && close(normal, [0., -1., 0.]));
    let (origin, normal) = plane(&planes, "raised");
    assert!(close(origin, [30., 20., 28.]) && close(normal, [0., 0., 1.]));
    // A quarter turn about +X takes +Z to X × Z = -Y.
    let (origin, normal) = plane(&planes, "tilt");
    assert!(close(origin, [30., 20., 28.]) && close(normal, [0., -1., 0.]));
    let (origin, normal) = plane(&planes, "middle");
    assert!(close(origin, [30., 20., 10.]) && close(normal, [0., 0., 1.]));
    let (origin, normal) = plane(&planes, "slope");
    let r = 0.5f64.sqrt();
    assert!(close(origin, [0., 0., 0.]) && close(normal, [0., -r, r]));
    // Geometry-free planes resolve without the model; face planes say why not.
    let datums = datum_index(&d.family.datums);
    let parameters = p.resolved_parameters().unwrap();
    assert!(resolve_datum(&datums, "front", &parameters, &NoFaces).is_ok());
    let error = resolve_datum(&datums, "tilt", &parameters, &NoFaces).unwrap_err();
    assert!(
        error.message.contains("needs the generated model"),
        "{error}"
    );
}

#[test]
fn a_sketch_on_a_turned_plane_keeps_its_x_axis_in_the_plane() {
    let mut d = document();
    d.family.datums.push(datum(
        "tilt",
        DatumKind::AnglePlane {
            base: PlaneBase::Xy,
            hinge: scalars(0., 1., 0.),
            angle_radians: ScalarExpr::Literal(Quantity::scalar(0.3)),
        },
    ));
    let s = sketch(&mut d.family);
    s.face_support = None;
    s.datum_plane = Some("tilt".into());
    let d = ModelDocument::from_json(&d.to_json_pretty().unwrap()).unwrap();
    let session = Session::new().unwrap();
    let generated = part(&d.family).regenerate(&session).unwrap();
    let bounds = session
        .exact_bounds(generated.shape("profile").unwrap())
        .unwrap();
    // The pocket outline is 20 mm wide in x, now tilted 0.3 rad about Y.
    assert!((bounds.max.z - bounds.min.z - 20. * 0.3f64.sin()).abs() < 1e-6);
}

#[test]
fn plane_cycles_unknown_faces_and_degenerate_constructions_are_rejected() {
    let reject = |edit: &dyn Fn(&mut FamilyDefinition), expected: &str| {
        let mut d = on_lid();
        edit(&mut d.family);
        let error = d
            .to_json_pretty()
            .and_then(|json| ModelDocument::from_json(&json))
            .unwrap_err();
        assert!(error.message.contains(expected), "{error}");
    };
    reject(
        &|f| {
            f.datums.push(datum(
                "a",
                DatumKind::OffsetPlane {
                    base: PlaneBase::Datum("b".into()),
                    distance: mm(1.),
                },
            ));
            f.datums.push(datum(
                "b",
                DatumKind::OffsetPlane {
                    base: PlaneBase::Datum("a".into()),
                    distance: mm(1.),
                },
            ));
        },
        "built on itself",
    );
    reject(
        &|f| {
            f.datums[0].kind = DatumKind::FacePlane {
                input: "missing".into(),
                face: facing(0., 0., 1.),
                distance: mm(0.),
            }
        },
        "unknown output 'missing'",
    );
    reject(
        &|f| {
            f.datums.push(datum(
                "line",
                DatumKind::ThreePointPlane {
                    first: lengths(0., 0., 0.),
                    second: lengths(1., 0., 0.),
                    third: lengths(2., 0., 0.),
                },
            ))
        },
        "collinear",
    );
    reject(
        &|f| {
            f.datums.push(datum(
                "flat",
                DatumKind::AnglePlane {
                    base: PlaneBase::Xy,
                    hinge: scalars(0., 0., 1.),
                    angle_radians: ScalarExpr::Literal(Quantity::scalar(0.5)),
                },
            ))
        },
        "hinge must not be normal",
    );
    reject(
        &|f| {
            f.datums.push(datum(
                "point",
                DatumKind::Point {
                    origin: lengths(0., 0., 0.),
                },
            ));
            f.datums.push(datum(
                "off",
                DatumKind::OffsetPlane {
                    base: PlaneBase::Datum("point".into()),
                    distance: mm(1.),
                },
            ));
        },
        "must be a plane",
    );
    // Faces that are not parallel fail when the model regenerates.
    let mut d = on_lid();
    d.family.datums.push(datum(
        "skew",
        DatumKind::Midplane {
            input: "block".into(),
            first: facing(0., 0., 1.),
            second: facing(1., 0., 0.),
        },
    ));
    let d = ModelDocument::from_json(&d.to_json_pretty().unwrap()).unwrap();
    let session = Session::new().unwrap();
    let p = part(&d.family);
    let generated = p.regenerate(&session).unwrap();
    let planes = p.datum_planes(&session, &generated).unwrap();
    let skew = &planes.iter().find(|(id, _)| id == "skew").unwrap().1;
    assert!(skew.as_ref().unwrap_err().message.contains("parallel"));
}
