//! Feature-frame dimensions for primitives, holes, revolves, and helices.

use super::*;

/// O(F) lookup for a directly displayed helix or a sweep with an immediate helix route.
pub(super) fn helix_route<'a>(
    part: &PartInstance<'a>,
    output: &str,
) -> Option<(&'a str, &'a FeatureOperation)> {
    let feature = part.definition.features.iter().find(|f| f.id == output)?;
    match &feature.operation {
        FeatureOperation::Helix { .. } => Some((&feature.id, &feature.operation)),
        FeatureOperation::Sweep { path, .. } => part
            .definition
            .features
            .iter()
            .find(|f| f.id == *path && matches!(f.operation, FeatureOperation::Helix { .. }))
            .map(|f| (f.id.as_str(), &f.operation)),
        _ => None,
    }
}
/// O(turns) display points, with 32 intervals per turn and native's 100k bound.
pub(crate) fn helix_samples(
    operation: &FeatureOperation,
    parameters: &HashMap<String, ParameterValue>,
) -> Result<usize, Failure> {
    let FeatureOperation::Helix { turns, .. } = operation else {
        return Ok(32);
    };
    let turns = turns.evaluate(parameters).stage("visualization")?.value;
    let count = (turns * 32.0).ceil().max(32.0) as usize + 1;
    if count > 100_000 {
        return Err(failure(
            "visualization",
            "helix display needs more than the native 100000-point edge \
                sampling limit",
        ));
    }
    Ok(count)
}

pub(super) fn revolve_dimension(
    session: &Session,
    feature: &FeatureDefinition,
    parameters: &HashMap<String, ParameterValue>,
    profile: &Shape<'_>,
    annotations: &mut Vec<Value>,
    budget: &mut Budget,
) -> Result<(), Failure> {
    let FeatureOperation::Revolve {
        input,
        origin,
        axis,
        angle_radians,
        extent,
    } = &feature.operation
    else {
        return Ok(());
    };
    let evaluate = |expression: &VectorExpr| {
        expression
            .evaluate(parameters)
            .map(|v| [v.x.value, v.y.value, v.z.value])
            .stage("visualization")
    };
    let origin_value = evaluate(origin)?;
    let direction = evaluate(axis)?;
    // Scale first to avoid overflow for large dimensionless axis components.
    let scale = direction.into_iter().map(f64::abs).fold(0.0, f64::max);
    let n = direction.map(|v| v / scale);
    let magnitude = n[0].hypot(n[1].hypot(n[2]));
    let n = n.map(|v| v / magnitude);
    let angle = angle_radians
        .evaluate(parameters)
        .stage("visualization")?
        .value;
    let face = if session.shape_type(profile).stage("visualization")? == ShapeType::Wire {
        Some(
            session
                .create_face_from_wire(profile)
                .stage("visualization")?,
        )
    } else {
        None
    };
    let start = point(
        session
            .center_of_mass(face.as_ref().unwrap_or(profile))
            .stage("visualization")?,
    );
    let along = (0..3)
        .map(|i| (start[i] - origin_value[i]) * n[i])
        .sum::<f64>();
    let center = std::array::from_fn::<_, 3, _>(|i| origin_value[i] + along * n[i]);
    let u = std::array::from_fn::<_, 3, _>(|i| start[i] - center[i]);
    let radius = u[0].hypot(u[1].hypot(u[2]));
    let v = [
        n[1] * u[2] - n[2] * u[1],
        n[2] * u[0] - n[0] * u[2],
        n[0] * u[1] - n[1] * u[0],
    ];
    let mut arc = Vec::new();
    let start_angle = if matches!(extent, RevolveExtent::Symmetric) {
        -0.5 * angle
    } else {
        0.0
    };
    if radius > 1e-7 {
        let segments = ((angle.abs() / std::f64::consts::TAU * 64.0).ceil() as usize).clamp(2, 64);
        check_budget(&mut budget.vertices, segments + 1, "vertex")?;
        for i in 0..=segments {
            let t = start_angle + angle * i as f64 / segments as f64;
            arc.push(std::array::from_fn::<_, 3, _>(|j| {
                center[j] + u[j] * t.cos() + v[j] * t.sin()
            }));
        }
    }
    let anchors = if let (Some(begin), Some(end)) = (arc.first(), arc.last()) {
        json!([begin, center, end])
    } else {
        json!([center])
    };
    annotations.push(
        Annotation {
            id: "driving-revolve-angle".into(),
            label: format!(
                "{}revolve ∠ {angle:.3} rad",
                if matches!(extent, RevolveExtent::Symmetric) {
                    "symmetric "
                } else {
                    ""
                }
            ),
            kind: AnnotationKind::Dimension,
            status: AnnotationStatus::Driving,
            targets: vec![feature.id.clone()],
            parameters: names(&serde_json::to_value(angle_radians).stage("visualization")?),
            anchors,
            detail: json!({
                "input": input,
                "expression": angle_radians,
                "extent": extent,
                "start_angle_radians": start_angle,
                "end_angle_radians": start_angle+angle,
                "value_radians": angle,
                "axis_origin": origin_value,
                "axis_direction": n,
                "arc_center": center,
                "arc_radius_mm": radius,
                "angular_arc": arc,
                "driving": true,
                "description": "Signed right-hand sweep around the native \
                    axis. The display arc uses the source profile's \
                    centroid radius; its radius is not a part size \
                    dimension.",
            }),
        }
        .into(),
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn helix_dimensions(
    session: &Session,
    route: &Shape<'_>,
    path: &str,
    output: &str,
    operation: &FeatureOperation,
    parameters: &HashMap<String, ParameterValue>,
    annotations: &mut Vec<Value>,
) -> Result<(), Failure> {
    let FeatureOperation::Helix {
        origin,
        axis,
        radius,
        pitch,
        turns,
        left_handed,
        ..
    } = operation
    else {
        return Ok(());
    };
    let vector = |expr: &VectorExpr| {
        expr.evaluate(parameters)
            .map(|v| [v.x.value, v.y.value, v.z.value])
            .stage("visualization")
    };
    let scalar = |expr: &ScalarExpr| {
        expr.evaluate(parameters)
            .map(|v| v.value)
            .stage("visualization")
    };
    let o = vector(origin)?;
    let direction = vector(axis)?;
    let scale = direction.into_iter().map(f64::abs).fold(0.0, f64::max);
    let n = direction.map(|v| v / scale);
    let magnitude = n[0].hypot(n[1].hypot(n[2]));
    let n = n.map(|v| v / magnitude);
    let (r, p, t) = (scalar(radius)?, scalar(pitch)?, scalar(turns)?);
    let edge = session
        .subshape(route, ShapeType::Edge, 0)
        .stage("visualization")?;
    let endpoints = session
        .edge_sample_points(&edge, 2)
        .stage("visualization")?;
    let start = point(endpoints[0]);
    let end_axis = std::array::from_fn::<_, 3, _>(|i| o[i] + p * t * n[i]);
    let next_turn = std::array::from_fn::<_, 3, _>(|i| start[i] + p * n[i]);
    let common = json!({
        "path": path,
        "axis": n,
        "left_handed": left_handed,
        "driving": true,
    });
    for (id, label, value, expressions, anchors) in [
        (
            "radius",
            format!("coil radius {} mm", length_label(r)),
            r,
            json!([origin, axis, radius]),
            json!([o, start]),
        ),
        (
            "pitch",
            format!("pitch {} mm / turn", length_label(p)),
            p,
            json!([origin, axis, pitch]),
            json!([start, next_turn]),
        ),
        (
            "turns",
            format!("turns {t:.3}"),
            t,
            json!([turns]),
            json!([end_axis]),
        ),
        (
            "rise",
            format!("axial rise {} mm", length_label(p * t)),
            p * t,
            json!([origin, axis, pitch, turns]),
            json!([o, end_axis]),
        ),
    ] {
        let mut detail = common.clone();
        detail["value"] = json!(value);
        detail["dimension"] = json!(if id == "turns" { "scalar" } else { "length" });
        detail["expressions"] = expressions.clone();
        detail["description"] = json!(if id == "rise" {
            "Helix axis rise; excludes wire thickness and end treatments."
        } else if id == "pitch" {
            "Reference axial advance per complete turn, including \
                fractional-turn helices."
        } else {
            "Driving helix geometry."
        });
        annotations.push(
            Annotation {
                id: format!("helix-{id}"),
                label,
                kind: AnnotationKind::Dimension,
                status: AnnotationStatus::Driving,
                targets: vec![output.into()],
                parameters: names(&expressions),
                anchors,
                detail,
            }
            .into(),
        );
    }
    Ok(())
}

pub(super) fn primitive_dimensions(
    feature: &FeatureDefinition,
    parameters: &HashMap<String, ParameterValue>,
    annotations: &mut Vec<Value>,
) -> Result<(), Failure> {
    let vec = |expression: &VectorExpr| {
        expression
            .evaluate(parameters)
            .map(|v| [v.x.value, v.y.value, v.z.value])
            .stage("visualization")
    };
    let mut add =
        |label: &str, expression: &ScalarExpr, a: [f64; 3], b: [f64; 3]| -> Result<(), Failure> {
            let q = expression.evaluate(parameters).stage("visualization")?;
            annotations.push(
                Annotation {
                    id: format!("driving-{label}"),
                    label: format!("{label} {} mm", length_label(q.value)),
                    kind: AnnotationKind::Dimension,
                    status: AnnotationStatus::Driving,
                    targets: vec![feature.id.clone()],
                    parameters: expression
                        .parameter_names()
                        .into_iter()
                        .map(str::to_owned)
                        .collect(),
                    anchors: json!([a, b]),
                    detail: json!({
                        "feature": feature.id,
                        "expression": expression,
                        "value": q,
                        "driving": true,
                    }),
                }
                .into(),
            );
            Ok(())
        };
    match &feature.operation {
        FeatureOperation::Hole { .. } => hole_dimensions(feature, parameters, annotations)?,
        FeatureOperation::Box { origin, size } => {
            let a = vec(origin)?;
            let lengths = vec(size)?;
            let parameter_names = names(&serde_json::to_value(size).stage("visualization")?);
            for axis in 0..3 {
                let mut b = a;
                b[axis] += lengths[axis];
                annotations.push(
                    Annotation {
                        id: format!("driving-box-{axis}"),
                        label: format!(
                            "{} {} mm",
                            ["width", "depth", "height"][axis],
                            lengths[axis]
                        ),
                        kind: AnnotationKind::Dimension,
                        status: AnnotationStatus::Driving,
                        targets: vec![feature.id.clone()],
                        parameters: parameter_names.clone(),
                        anchors: json!([a, b]),
                        detail: json!({
                            "feature": feature.id,
                            "expression": size,
                            "component": axis,
                            "value_mm": lengths[axis],
                            "driving": true,
                        }),
                    }
                    .into(),
                );
            }
        }
        FeatureOperation::Sphere { center, radius } => {
            let a = vec(center)?;
            let r = radius.evaluate(parameters).stage("visualization")?.value;
            add("R", radius, a, [a[0] + r, a[1], a[2]])?;
        }
        FeatureOperation::Cylinder {
            origin,
            axis,
            radius,
            height,
        }
        | FeatureOperation::Cone {
            origin,
            axis,
            base_radius: radius,
            height,
            ..
        } => {
            let a = vec(origin)?;
            let axis = vec(axis)?;
            let scale = axis.into_iter().map(f64::abs).fold(0.0, f64::max);
            let n = axis.map(|x| x / scale);
            let length = n[0].hypot(n[1].hypot(n[2]));
            let n = n.map(|x| x / length);
            let h = height.evaluate(parameters).stage("visualization")?.value;
            let r = radius.evaluate(parameters).stage("visualization")?.value;
            let u = if n[0].abs() < 0.9 {
                [0.0, -n[2], n[1]]
            } else {
                [-n[1], n[0], 0.0]
            };
            let len = u[0].hypot(u[1].hypot(u[2]));
            let u = u.map(|x| x / len);
            add(
                "height",
                height,
                a,
                std::array::from_fn(|i| a[i] + n[i] * h),
            )?;
            if r > 0.0 {
                add("R", radius, a, std::array::from_fn(|i| a[i] + u[i] * r))?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn hole_dimensions(
    feature: &FeatureDefinition,
    parameters: &HashMap<String, ParameterValue>,
    annotations: &mut Vec<Value>,
) -> Result<(), Failure> {
    let FeatureOperation::Hole {
        position,
        axis,
        diameter,
        extent,
        bottom,
        ..
    } = &feature.operation
    else {
        unreachable!()
    };
    let vector = |expr: &VectorExpr| {
        expr.evaluate(parameters)
            .map(|q| [q.x.value, q.y.value, q.z.value])
            .stage("visualization")
    };
    let scalar = |expr: &ScalarExpr| {
        expr.evaluate(parameters)
            .map(|q| q.value)
            .stage("visualization")
    };
    let a = vector(position)?;
    let axis = vector(axis)?;
    let maximum = axis.into_iter().map(f64::abs).fold(0.0, f64::max);
    let n = axis.map(|x| x / maximum);
    let magnitude = n[0].hypot(n[1].hypot(n[2]));
    let n = n.map(|x| x / magnitude);
    let u = if n[0].abs() < 0.9 {
        [0.0, n[2], -n[1]]
    } else {
        [-n[2], 0.0, n[0]]
    };
    let magnitude = u[0].hypot(u[1].hypot(u[2]));
    let u = u.map(|x| x / magnitude);
    let d = scalar(diameter)?;
    let offset = |p: [f64; 3], v: [f64; 3], scale: f64| {
        std::array::from_fn::<_, 3, _>(|i| p[i] + v[i] * scale)
    };
    annotations.push(
        Annotation {
            id: "driving-hole-diameter".into(),
            label: format!("bore Ø {} mm", length_label(d)),
            kind: AnnotationKind::Dimension,
            status: AnnotationStatus::Driving,
            targets: vec![feature.id.clone()],
            parameters: names(&serde_json::to_value(diameter).stage("visualization")?),
            anchors: json!([offset(a, u, -d / 2.0), offset(a, u, d / 2.0)]),
            detail: json!({
                "expression": diameter,
                "value_mm": d,
                "driving": true,
            }),
        }
        .into(),
    );
    if let HoleExtent::Blind { depth } = extent {
        let full_depth = scalar(depth)?;
        let end = offset(a, n, full_depth);
        annotations.push(
            Annotation {
                id: "driving-hole-depth".into(),
                label: format!("full diameter depth {} mm", length_label(full_depth)),
                kind: AnnotationKind::Dimension,
                status: AnnotationStatus::Driving,
                targets: vec![feature.id.clone()],
                parameters: names(&serde_json::to_value(depth).stage("visualization")?),
                anchors: json!([a, end]),
                detail: json!({
                    "expression": depth,
                    "value_mm": full_depth,
                    "depth_reference": "full_diameter",
                    "driving": true,
                }),
            }
            .into(),
        );
        if let HoleBottom::DrillPoint { angle_radians } = bottom {
            let angle = scalar(angle_radians)?;
            let tip_depth = d / (2.0 * (angle / 2.0).tan());
            let apex = offset(end, n, tip_depth);
            let arc_radius = tip_depth.min(d / 2.0) * 0.5;
            let arc = (0..=16)
                .map(|i| {
                    let theta = -angle / 2.0 + angle * f64::from(i) / 16.0;
                    std::array::from_fn::<_, 3, _>(|j| {
                        apex[j] + arc_radius * (theta.sin() * u[j] - theta.cos() * n[j])
                    })
                })
                .collect::<Vec<_>>();
            let controls = names(&json!({
                "diameter": diameter,
                "bottom": bottom,
            }));
            annotations.push(
                Annotation {
                    id: "driving-drill-angle".into(),
                    label: format!("drill point ∠ {angle:.3} rad"),
                    kind: AnnotationKind::Dimension,
                    status: AnnotationStatus::Driving,
                    targets: vec![feature.id.clone()],
                    parameters: names(&serde_json::to_value(angle_radians).stage("visualization")?),
                    anchors: json!([offset(end, u, -d / 2.0), apex, offset(end, u, d / 2.0)]),
                    detail: json!({
                        "expression": angle_radians,
                        "value_radians": angle,
                        "angular_arc": arc,
                        "driving": true,
                    }),
                }
                .into(),
            );
            annotations.push(
                Annotation {
                    id: "measured-drill-tip".into(),
                    label: format!("tip depth {} mm", length_label(tip_depth)),
                    kind: AnnotationKind::Dimension,
                    status: AnnotationStatus::Measured,
                    targets: vec![feature.id.clone()],
                    parameters: controls,
                    anchors: json!([end, apex]),
                    detail: json!({
                        "value_mm": tip_depth,
                        "total_depth_mm": full_depth+tip_depth,
                        "bottom": bottom,
                        "driving": false,
                    }),
                }
                .into(),
            );
        }
    }
    Ok(())
}
