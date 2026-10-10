//! Driving and measured annotations for the feature that produces a displayed
//! solid output.

use super::dimensions::{primitive_dimensions, revolve_dimension};
use super::*;

/// Shared inputs for annotating one displayed solid output.
#[derive(Clone, Copy)]
pub(super) struct SolidScene<'a, 'session> {
    pub session: &'session Session,
    pub part: &'a PartInstance<'a>,
    pub generated: &'a GeneratedResult<'session>,
    pub output: &'a str,
    pub parameters: &'a HashMap<String, ParameterValue>,
    pub input_map: &'a BTreeMap<&'a str, Vec<&'a str>>,
    pub shape: &'a Shape<'session>,
    pub min: [f64; 3],
    pub max: [f64; 3],
    pub center: [f64; 3],
    pub edge_samples: usize,
}

/// Annotations for the feature that directly produces the output. Direct
/// primitive dimensions have their true feature-frame anchors; for downstream
/// booleans and transforms, controls stay in the linked side panel.
pub(super) fn feature_dimensions(
    scene: &SolidScene<'_, '_>,
    feature: &FeatureDefinition,
    budget: &mut Budget,
    annotations: &mut Vec<Value>,
) -> Result<(), Failure> {
    primitive_dimensions(feature, scene.parameters, annotations)?;
    // Each step ignores other operations; a variable fillet uses the first two.
    edge_treatment(scene, feature, budget, annotations)?;
    variable_fillet_law(scene, feature, annotations)?;
    thread_dimensions(scene, feature, annotations)?;
    circular_pattern_dimensions(scene, feature, budget, annotations)?;
    linear_pattern_dimensions(scene, feature, annotations)?;
    compound_inputs(scene, feature, annotations)?;
    offset_dimension(scene, feature, annotations)?;
    scale_dimension(scene, feature, annotations)?;
    mirror_plane(scene, feature, annotations)?;
    revolve_annotation(scene, feature, budget, annotations)?;
    sweep_route_length(scene, feature, budget, annotations)?;
    loft_section_spacing(scene, feature, annotations)?;
    hole_limit(scene, feature, annotations)?;
    extrusion_dimension(scene, feature, annotations)
}

fn scalar(
    expression: &ScalarExpr,
    parameters: &HashMap<String, ParameterValue>,
) -> Result<f64, Failure> {
    expression
        .evaluate(parameters)
        .map(|q| q.value)
        .map_err(|e| model_failure("visualization", e))
}

fn vector(
    expression: &VectorExpr,
    parameters: &HashMap<String, ParameterValue>,
) -> Result<[f64; 3], Failure> {
    expression
        .evaluate(parameters)
        .map(|v| [v.x.value, v.y.value, v.z.value])
        .map_err(|e| model_failure("visualization", e))
}

/// Unit direction, divided by the largest component first so very small or
/// large inputs normalize without underflow or overflow.
fn unit_direction(raw: [f64; 3]) -> [f64; 3] {
    let scale = raw.into_iter().map(f64::abs).fold(0.0, f64::max);
    let n = raw.map(|v| v / scale);
    let magnitude = n[0].hypot(n[1].hypot(n[2]));
    n.map(|v| v / magnitude)
}

/// Area centroid of a profile. Wire centroids weight boundary length while
/// generated solids weight profile area, so wires are filled first.
fn area_centroid(session: &Session, profile: &Shape<'_>) -> Result<[f64; 3], Failure> {
    let face = if session
        .shape_type(profile)
        .map_err(|e| failure("visualization", e))?
        == ShapeType::Wire
    {
        Some(
            session
                .create_face_from_wire(profile)
                .map_err(|e| failure("visualization", e))?,
        )
    } else {
        None
    };
    Ok(point(
        session
            .center_of_mass(face.as_ref().unwrap_or(profile))
            .map_err(|e| failure("visualization", e))?,
    ))
}

/// Parameter names read by `start` and every feature upstream of it.
fn upstream_controls(scene: &SolidScene<'_, '_>, start: &str) -> Result<BTreeSet<String>, Failure> {
    let definitions = scene
        .part
        .definition
        .features
        .iter()
        .map(|f| (f.id.as_str(), f))
        .collect::<HashMap<_, _>>();
    let mut pending = vec![start];
    let mut visited = BTreeSet::new();
    let mut controls = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if !visited.insert(id) {
            continue;
        }
        if let Some(feature) = definitions.get(id) {
            controls.extend(names(
                &serde_json::to_value(&feature.operation)
                    .map_err(|e| failure("visualization", e))?,
            ));
        }
        if let Some(inputs) = scene.input_map.get(id) {
            pending.extend(inputs.iter().copied());
        }
    }
    Ok(controls)
}

/// Nominal fillet or chamfer value with the selected source edges it applies to.
fn edge_treatment(
    scene: &SolidScene<'_, '_>,
    feature: &FeatureDefinition,
    budget: &mut Budget,
    annotations: &mut Vec<Value>,
) -> Result<(), Failure> {
    let SolidScene {
        session,
        part,
        generated,
        output,
        parameters,
        center,
        ..
    } = *scene;
    let (kind, input, selectors, value) = match &feature.operation {
        FeatureOperation::Fillet {
            input,
            edges,
            radius,
        } => ("fillet", input, edges, radius),
        FeatureOperation::Chamfer {
            input,
            edges,
            distance,
        } => ("chamfer", input, edges, distance),
        FeatureOperation::VariableFillet {
            input,
            edges,
            start_radius,
            ..
        } => ("variable-fillet", input, edges, start_radius),
        _ => return Ok(()),
    };
    let nominal = scalar(value, parameters)?;
    let mut references = Vec::new();
    let mut anchors = Vec::new();
    // Resolve parameters once; query cost follows semantic selection.
    // Additional measurement/display work is bounded to 64 references.
    let edges = part
        .select_edges(
            session,
            generated,
            input,
            &EdgeSelector::Union(selectors.clone()),
        )
        .map_err(|e| model_failure("visualization", e))?;
    let selected_count = edges.len();
    for edge in edges {
        if references.len() < 64 {
            check_budget(&mut budget.vertices, 8, "vertex")?;
            let points = session
                .edge_sample_points(&edge, 8)
                .map_err(|e| failure("visualization", e))?;
            anchors.push(point(
                session
                    .center_of_mass(&edge)
                    .map_err(|e| failure("visualization", e))?,
            ));
            references.push(points.into_iter().map(point).collect::<Vec<_>>());
        }
    }
    let anchor = anchors.first().copied().unwrap_or(center);
    let expressions = if kind == "variable-fillet" {
        json!(feature.operation)
    } else {
        json!([value, selectors])
    };
    let label = if kind == "variable-fillet" {
        "variable fillet law".into()
    } else {
        format!(
            "{kind} {} {} mm",
            if kind == "fillet" {
                "radius"
            } else {
                "distance"
            },
            length_label(nominal)
        )
    };
    annotations.push(annotation(
        format!("driving-{kind}"),label,"dimension","driving",vec![output.into()],names(&expressions),json!([anchor]),
        json!({"input":input,"value_mm":nominal,"selected_edge_count":selected_count,"displayed_reference_count":references.len(),"dimension_paths":references,"source_reference":true,"expressions":expressions,"driving":true,"measurement":false,"description":"Nominal treatment value; overlays mark selected source edges before treatment, not edges of the finished part."})
    ));
    Ok(())
}

/// Station radii of a variable fillet's law, attached to its treatment annotation.
fn variable_fillet_law(
    scene: &SolidScene<'_, '_>,
    feature: &FeatureDefinition,
    annotations: &mut Vec<Value>,
) -> Result<(), Failure> {
    let SolidScene {
        output,
        parameters,
        center,
        ..
    } = *scene;
    let FeatureOperation::VariableFillet {
        start_radius,
        end_radius,
        stations,
        spine_direction,
        ..
    } = &feature.operation
    else {
        return Ok(());
    };
    // Law coordinates describe the native tangent contour, not an edge's
    // curve parameter. Keep labels at the scene centre; do not fabricate
    // spatial station positions from sampled source-edge points.
    let evaluate = |e: &ScalarExpr| scalar(e, parameters);
    let mut law = vec![json!({"position":0.0,"radius_mm":evaluate(start_radius)?})];
    for station in stations {
        law.push(
            json!({"position":evaluate(&station.position)?,"radius_mm":evaluate(&station.radius)?}),
        );
    }
    law.push(json!({"position":1.0,"radius_mm":evaluate(end_radius)?}));
    let reference = annotations
        .iter_mut()
        .find(|a| a["id"] == "driving-variable-fillet")
        .unwrap();
    reference["detail"]
        .as_object_mut()
        .unwrap()
        .remove("value_mm");
    reference["detail"]["radius_law"] = json!(law);
    reference["detail"]["spine_direction"] = json!(spine_direction);
    reference["detail"]["spatial_stations"] = json!(false);
    reference["detail"]["displayed_interior_stations"] = json!(stations.len().min(64));
    reference["detail"]["description"] = json!(
        "Nominal radius law on the native tangent contour. Paths reference original selected edges; labels do not locate stations on the finished solid."
    );
    for (id, radius, position) in [("start", start_radius, 0.0), ("end", end_radius, 1.0)] {
        annotations.push(annotation(format!("fillet-{id}-radius"),format!("{id} R {} mm",length_label(evaluate(radius)?)),"dimension","driving",vec![output.into()],names(&json!(radius)),json!([center]),
            json!({"value_mm":evaluate(radius)?,"position":position,"spine_direction":spine_direction,"expression":radius,"measurement":false,"spatial_station":false})));
    }
    for (index, station) in stations.iter().take(64).enumerate() {
        let radius = evaluate(&station.radius)?;
        let position = evaluate(&station.position)?;
        annotations.push(annotation(format!("fillet-station-{}",index+1),format!("R {} mm @ {:.3}",length_label(radius),position),"dimension","driving",vec![output.into()],names(&json!(station)),json!([center]),
            json!({"value_mm":radius,"position":position,"station_index":index+1,"expression":station,"measurement":false,"spatial_station":false})));
    }
    Ok(())
}

/// Nominal major diameter, pitch, run, and derived turns of a modeled thread.
fn thread_dimensions(
    scene: &SolidScene<'_, '_>,
    feature: &FeatureDefinition,
    annotations: &mut Vec<Value>,
) -> Result<(), Failure> {
    let SolidScene {
        output,
        parameters,
        center,
        ..
    } = *scene;
    let FeatureOperation::Thread {
        input,
        origin,
        axis,
        major_diameter,
        pitch,
        length,
        internal,
        left_handed,
    } = &feature.operation
    else {
        return Ok(());
    };
    let o = vector(origin, parameters)?;
    let n = unit_direction(vector(axis, parameters)?);
    let seed = if n[0].abs() < 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let dot = (0..3).map(|i| seed[i] * n[i]).sum::<f64>();
    let radial = std::array::from_fn::<_, 3, _>(|i| seed[i] - dot * n[i]);
    let norm = radial[0].hypot(radial[1].hypot(radial[2]));
    let radial = radial.map(|v| v / norm);
    let (diameter, p, run) = (
        scalar(major_diameter, parameters)?,
        scalar(pitch, parameters)?,
        scalar(length, parameters)?,
    );
    let ends = [
        std::array::from_fn::<_, 3, _>(|i| o[i] - diameter * 0.5 * radial[i]),
        std::array::from_fn::<_, 3, _>(|i| o[i] + diameter * 0.5 * radial[i]),
    ];
    let end = std::array::from_fn::<_, 3, _>(|i| o[i] + run * n[i]);
    let common = json!({"input":input,"axis_origin":o,"axis":n,"internal":internal,"left_handed":left_handed,"driving":true,"measurement":false});
    for (id, label, value, expressions, anchors) in [
        (
            "major-diameter",
            format!("thread major ⌀ {} mm", length_label(diameter)),
            diameter,
            json!([major_diameter]),
            json!(ends),
        ),
        (
            "pitch",
            format!("thread pitch {} mm", length_label(p)),
            p,
            json!([pitch]),
            json!([center]),
        ),
        (
            "run",
            format!("thread run {} mm", length_label(run)),
            run,
            json!([origin, axis, length]),
            json!([o, end]),
        ),
    ] {
        let mut detail = common.clone();
        detail["value_mm"] = json!(value);
        detail["expressions"] = expressions.clone();
        annotations.push(annotation(
            format!("thread-{id}"),
            label,
            "dimension",
            "driving",
            vec![output.into()],
            names(&expressions),
            anchors,
            detail,
        ));
    }
    annotations.push(annotation("thread-turns".into(),format!("{} {} thread · {:.3} turns",if *internal{"internal"}else{"external"},if *left_handed{"LH"}else{"RH"},run/p),"dimension","derived",vec![output.into()],names(&json!([pitch,length])),json!([center]),json!({"input":input,"turns":run/p,"internal":internal,"left_handed":left_handed,"driving":false,"description":"Derived turns; nominal thread dimensions do not measure tolerance class or fit."})));
    Ok(())
}

/// Copy count and signed angular step of a circular pattern.
fn circular_pattern_dimensions(
    scene: &SolidScene<'_, '_>,
    feature: &FeatureDefinition,
    budget: &mut Budget,
    annotations: &mut Vec<Value>,
) -> Result<(), Failure> {
    let SolidScene {
        session,
        generated,
        output,
        parameters,
        center,
        ..
    } = *scene;
    let FeatureOperation::CircularPattern {
        input,
        origin,
        axis,
        count,
        angle_step_radians,
    } = &feature.operation
    else {
        return Ok(());
    };
    let o = vector(origin, parameters)?;
    let n = unit_direction(vector(axis, parameters)?);
    let c = scalar(count, parameters)?;
    let angle = scalar(angle_step_radians, parameters)?;
    let source = generated
        .shape(input)
        .ok_or_else(|| failure("visualization", "circular pattern source unavailable"))?;
    let start = point(
        session
            .center_of_mass(source)
            .map_err(|e| failure("visualization", e))?,
    );
    let v = std::array::from_fn::<_, 3, _>(|i| start[i] - o[i]);
    let height = (0..3).map(|i| v[i] * n[i]).sum::<f64>();
    let pivot = std::array::from_fn::<_, 3, _>(|i| o[i] + height * n[i]);
    let radial = std::array::from_fn::<_, 3, _>(|i| start[i] - pivot[i]);
    let tangent = [
        n[1] * radial[2] - n[2] * radial[1],
        n[2] * radial[0] - n[0] * radial[2],
        n[0] * radial[1] - n[1] * radial[0],
    ];
    let arc = (0..=32)
        .map(|j| {
            let a = angle * j as f64 / 32.0;
            std::array::from_fn::<_, 3, _>(|i| {
                pivot[i] + radial[i] * a.cos() + tangent[i] * a.sin()
            })
        })
        .collect::<Vec<_>>();
    check_budget(&mut budget.vertices, arc.len(), "vertex")?;
    let expressions = json!([origin, axis, count, angle_step_radians]);
    annotations.push(annotation(
        "circular-pattern-count".into(),
        format!("circular pattern · {c:.0} copies"),
        "dimension",
        "driving",
        vec![output.into()],
        names(&json!([count])),
        json!([center]),
        json!({"input":input,"count":c,"axis_origin":o,"axis":n,"driving":true}),
    ));
    annotations.push(annotation("circular-pattern-angle".into(),format!("angular step {:.3}°",angle.to_degrees()),"dimension","driving",vec![output.into()],names(&expressions),json!([start]),json!({"input":input,"count":c,"value_radians":angle,"angular_arc":arc,"axis_origin":o,"axis":n,"expressions":expressions,"driving":true,"description":"Signed angular spacing between source placements. An on-axis source has no radial arc extent."})));
    Ok(())
}

/// Copy count, step, and span of a linear pattern.
fn linear_pattern_dimensions(
    scene: &SolidScene<'_, '_>,
    feature: &FeatureDefinition,
    annotations: &mut Vec<Value>,
) -> Result<(), Failure> {
    let SolidScene {
        session,
        generated,
        output,
        parameters,
        center,
        ..
    } = *scene;
    let FeatureOperation::LinearPattern { input, step, count } = &feature.operation else {
        return Ok(());
    };
    let step_mm = vector(step, parameters)?;
    let count_value = scalar(count, parameters)?;
    let source = generated
        .shape(input)
        .ok_or_else(|| failure("visualization", "pattern source unavailable"))?;
    let anchor = point(
        session
            .center_of_mass(source)
            .map_err(|e| failure("visualization", e))?,
    );
    let end = std::array::from_fn::<_, 3, _>(|i| anchor[i] + (count_value - 1.0) * step_mm[i]);
    let expressions = json!([step, count]);
    annotations.push(annotation(
        "linear-pattern-count".into(),format!("pattern · {count_value:.0} copies"),"dimension","driving",vec![output.into()],names(&json!([count])),json!([center]),
        json!({"input":input,"count":count_value,"step_mm":step_mm,"expressions":expressions,"driving":true,"description":"Unfused copies including the original placement."})
    ));
    annotations.push(annotation(
        "linear-pattern-spacing".into(),
        format!(
            "step {} mm",
            length_label(step_mm[0].hypot(step_mm[1].hypot(step_mm[2])))
        ),
        "dimension",
        "driving",
        vec![output.into()],
        names(&json!([step])),
        json!([anchor]),
        json!({"input":input,"step_mm":step_mm,"expressions":step,"driving":true}),
    ));
    annotations.push(annotation(
        "linear-pattern-span".into(),format!("pattern span {} mm",length_label((count_value-1.0)*step_mm[0].hypot(step_mm[1].hypot(step_mm[2])))),"dimension","derived",vec![output.into()],names(&expressions),json!([anchor,end]),
        json!({"input":input,"count":count_value,"step_mm":step_mm,"driving":false,"description":"Displacement from first to last source placement; excludes source size."})
    ));
    Ok(())
}

/// Grouping summary for an unfused compound.
fn compound_inputs(
    scene: &SolidScene<'_, '_>,
    feature: &FeatureDefinition,
    annotations: &mut Vec<Value>,
) -> Result<(), Failure> {
    let SolidScene { output, center, .. } = *scene;
    let FeatureOperation::Compound { inputs } = &feature.operation else {
        return Ok(());
    };
    annotations.push(annotation(
        "compound-inputs".into(), format!("group · {} inputs", inputs.len()),
        "group", "derived", vec![output.into()], vec![], json!([center]),
        json!({"inputs":inputs,"input_count":inputs.len(),"description":"Child shapes grouped without fusion or sewing. Overlaps remain."})
    ));
    Ok(())
}

/// Signed skin offset distance.
fn offset_dimension(
    scene: &SolidScene<'_, '_>,
    feature: &FeatureDefinition,
    annotations: &mut Vec<Value>,
) -> Result<(), Failure> {
    let SolidScene {
        output,
        parameters,
        center,
        ..
    } = *scene;
    let FeatureOperation::Offset {
        input,
        distance,
        tolerance,
    } = &feature.operation
    else {
        return Ok(());
    };
    let value = scalar(distance, parameters)?;
    let tolerance_mm = scalar(tolerance, parameters)?;
    let expressions = json!([distance, tolerance]);
    annotations.push(annotation(
        "driving-skin-offset".into(), format!("skin offset {value:+.3} mm"),
        "dimension", "driving", vec![output.into()], names(&expressions), json!([center]),
        json!({"input":input,"value_mm":value,"tolerance_mm":tolerance_mm,"expressions":expressions,"driving":true,"description":"Signed native skin offset. Label anchored at result bounds; this is not a wall-thickness measurement."})
    ));
    Ok(())
}

/// Uniform scale factor about its centre.
fn scale_dimension(
    scene: &SolidScene<'_, '_>,
    feature: &FeatureDefinition,
    annotations: &mut Vec<Value>,
) -> Result<(), Failure> {
    let SolidScene {
        output,
        parameters,
        center,
        ..
    } = *scene;
    let FeatureOperation::Scale {
        input,
        center: scale_center,
        factor,
    } = &feature.operation
    else {
        return Ok(());
    };
    let c = vector(scale_center, parameters)?;
    let value = scalar(factor, parameters)?;
    let expressions = json!([scale_center, factor]);
    annotations.push(annotation("driving-scale-factor".into(),format!("scale × {value:.4}"),"dimension","driving",vec![output.into()],names(&expressions),json!([center]),json!({"input":input,"scale_center":c,"factor":value,"expressions":expressions,"driving":true,"description":"Dimensionless uniform geometry scale about the specified centre. Label anchored at result bounds."})));
    Ok(())
}

/// Mirror plane origin and unit normal.
fn mirror_plane(
    scene: &SolidScene<'_, '_>,
    feature: &FeatureDefinition,
    annotations: &mut Vec<Value>,
) -> Result<(), Failure> {
    let SolidScene {
        output,
        parameters,
        center,
        ..
    } = *scene;
    let FeatureOperation::Mirror {
        input,
        origin,
        normal,
    } = &feature.operation
    else {
        return Ok(());
    };
    let o = vector(origin, parameters)?;
    let unit = unit_direction(vector(normal, parameters)?);
    let expressions = json!([origin, normal]);
    annotations.push(annotation("driving-mirror-plane".into(),"mirror plane".into(),"constraint","driving",vec![output.into()],names(&expressions),json!([center]),json!({"input":input,"plane_origin":o,"plane_normal":unit,"expressions":expressions,"driving":true,"description":"Plane controls define native reflection. The label is anchored at the result's bounding centre."})));
    Ok(())
}

fn revolve_annotation(
    scene: &SolidScene<'_, '_>,
    feature: &FeatureDefinition,
    budget: &mut Budget,
    annotations: &mut Vec<Value>,
) -> Result<(), Failure> {
    let SolidScene {
        session,
        generated,
        parameters,
        ..
    } = *scene;
    let FeatureOperation::Revolve { input, .. } = &feature.operation else {
        return Ok(());
    };
    let profile = generated
        .shape(input)
        .ok_or_else(|| failure("visualization", "revolve profile unavailable"))?;
    revolve_dimension(session, feature, parameters, profile, annotations, budget)?;
    Ok(())
}

/// Native length of a sweep route with the controls that drive it.
fn sweep_route_length(
    scene: &SolidScene<'_, '_>,
    feature: &FeatureDefinition,
    budget: &mut Budget,
    annotations: &mut Vec<Value>,
) -> Result<(), Failure> {
    let SolidScene {
        session,
        generated,
        output,
        edge_samples,
        ..
    } = *scene;
    let FeatureOperation::Sweep {
        path, orientation, ..
    } = &feature.operation
    else {
        return Ok(());
    };
    let route = generated
        .shape(path)
        .ok_or_else(|| failure("visualization", "sweep route unavailable"))?;
    let route_samples = edge_samples;
    let mut distance = 0.0;
    let mut paths = Vec::new();
    for edge in session
        .subshapes(route, ShapeType::Edge)
        .map_err(|e| failure("visualization", e))?
    {
        distance += session
            .edge_length(&edge)
            .map_err(|e| failure("visualization", e))?;
        check_budget(&mut budget.vertices, route_samples, "vertex")?;
        let samples = session
            .edge_sample_points(&edge, route_samples)
            .map_err(|e| failure("visualization", e))?;
        paths.push(samples.into_iter().map(point).collect::<Vec<_>>());
    }
    let controls = upstream_controls(scene, path)?;
    let anchor = point(
        session
            .center_of_mass(route)
            .map_err(|e| failure("visualization", e))?,
    );
    annotations.push(annotation("sweep-route-length".into(),format!("route length {} mm",length_label(distance)),"dimension","measured",vec![output.into()],controls.into_iter().collect(),json!([anchor]),json!({"path":path,"orientation":orientation,"value_mm":distance,"measurement":"native_edge_length_sum","dimension_paths":paths,"driving":false,"description":"Native route length; the displayed curve is sampled for visualization. This is not the endpoint distance or material cut length."})));
    Ok(())
}

/// Centroid spacing between consecutive loft sections.
fn loft_section_spacing(
    scene: &SolidScene<'_, '_>,
    feature: &FeatureDefinition,
    annotations: &mut Vec<Value>,
) -> Result<(), Failure> {
    let SolidScene {
        session,
        part,
        generated,
        output,
        ..
    } = *scene;
    let FeatureOperation::ProfileLoft { profiles, .. } = &feature.operation else {
        return Ok(());
    };
    let section_definitions = part
        .definition
        .features
        .iter()
        .map(|f| (f.id.as_str(), f))
        .collect::<HashMap<_, _>>();
    let mut centers = Vec::new();
    for id in profiles {
        let section = generated
            .shape(id)
            .ok_or_else(|| failure("visualization", "missing loft profile"))?;
        centers.push(area_centroid(session, section)?);
    }
    for (index, pair) in centers.windows(2).enumerate() {
        let delta = std::array::from_fn::<_, 3, _>(|i| pair[1][i] - pair[0][i]);
        let distance = delta[0].hypot(delta[1].hypot(delta[2]));
        let mut control_names = BTreeSet::new();
        for id in &profiles[index..index + 2] {
            if let Some(section) = section_definitions.get(id.as_str()) {
                control_names.extend(names(
                    &serde_json::to_value(&section.operation)
                        .map_err(|e| failure("visualization", e))?,
                ));
            }
        }
        annotations.push(annotation(format!("loft-spacing-{index}"),format!("section spacing {} mm",length_label(distance)),"dimension","measured",vec![output.into()],control_names.into_iter().collect(),json!(pair),json!({"profiles":&profiles[index..index+2],"value_mm":distance,"measurement":"section_area_centroid_spacing","driving":false})));
    }
    Ok(())
}

/// Measured depth of a hole that ends at a face rather than a distance.
fn hole_limit(
    scene: &SolidScene<'_, '_>,
    feature: &FeatureDefinition,
    annotations: &mut Vec<Value>,
) -> Result<(), Failure> {
    let SolidScene {
        session,
        part,
        generated,
        output,
        ..
    } = *scene;
    let FeatureOperation::Hole {
        input,
        extent: extent @ (HoleExtent::UpToFace { .. } | HoleExtent::UpToNext),
        ..
    } = &feature.operation
    else {
        return Ok(());
    };
    let witness = part
        .hole_limit_measurement(session, generated, output)
        .map_err(|e| model_failure("visualization", e))?
        .ok_or_else(|| failure("visualization", "hole limit has no measurement"))?;
    let controls = upstream_controls(scene, output)?
        .into_iter()
        .collect::<Vec<_>>();
    let mode = if matches!(extent, HoleExtent::UpToNext) {
        "next face"
    } else {
        "selected face"
    };
    annotations.push(annotation("measured-hole-limit".into(),format!("hole to {mode}: {} mm",length_label(witness.distance)),"dimension","measured",vec![output.into()],controls,json!([point(witness.first),point(witness.second)]),json!({"extent":extent,"value_mm":witness.distance,"measurement":"bore_centre_ray","input":input,"driving":false})));
    Ok(())
}

/// Extrusion travel: the nominal direction, or the measured centroid ray for
/// face-limited extents.
fn extrusion_dimension(
    scene: &SolidScene<'_, '_>,
    feature: &FeatureDefinition,
    annotations: &mut Vec<Value>,
) -> Result<(), Failure> {
    let SolidScene {
        session,
        output,
        parameters,
        shape,
        min,
        max,
        ..
    } = *scene;
    let FeatureOperation::Extrude {
        input,
        direction,
        extent,
    } = &feature.operation
    else {
        return Ok(());
    };
    let Some(profile) = scene.generated.shape(input) else {
        return Ok(());
    };
    // Use the profile's area centroid, matching the generated prism.
    let mut a = area_centroid(session, profile)?;
    let mut d = vector(direction, parameters)?;
    let geometry_driven = matches!(
        extent,
        ExtrudeExtent::UpToFace { .. } | ExtrudeExtent::UpToNext { .. }
    );
    let mut has_measurement = true;
    if geometry_driven {
        let search_length = (max[0] - min[0]).hypot((max[1] - min[1]).hypot(max[2] - min[2])) + 1.0;
        let hit = session
            .ray_first_hit(
                shape,
                Vec3::new(a[0], a[1], a[2]),
                Vec3::new(d[0], d[1], d[2]),
                search_length,
            )
            .map_err(|e| failure("visualization", e))?;
        if let Some((end, _)) = hit {
            let end = point(end);
            d = std::array::from_fn(|i| end[i] - a[i]);
        } else {
            // A holed/concave profile can have a centroid outside material.
            // Keep the extent control but do not invent a distance glyph.
            has_measurement = false;
            d = [0.0; 3];
        }
    } else if matches!(extent, ExtrudeExtent::Symmetric) {
        a = std::array::from_fn(|i| a[i] - 0.5 * d[i]);
    }
    let length = d[0].hypot(d[1].hypot(d[2]));
    let mode = match extent {
        ExtrudeExtent::Distance => "extrusion",
        ExtrudeExtent::Symmetric => "symmetric extrusion",
        ExtrudeExtent::UpToFace { .. } => "up-to-face",
        ExtrudeExtent::UpToNext { .. } => "up-to-next",
    };
    annotations.push(annotation(
        "driving-extrusion".into(),
        if has_measurement {format!("{mode}{} {} mm",if geometry_driven {": centroid"} else {""},length_label(length))} else {format!("{mode}: no centroid-ray intersection")},
        "dimension",
        if geometry_driven { "measured" } else { "driving" },
        vec![output.into()],
        names(&serde_json::to_value(direction).map_err(|e| failure("visualization", e))?),
        if has_measurement {json!([a, std::array::from_fn::<_, 3, _>(|i| a[i] + d[i])])} else {json!([])},
        json!({"feature":output,"expression":direction,"extent":extent,"value_mm":if has_measurement {Some(length)} else {None},"measurement":if geometry_driven {"profile_centroid_ray"} else {"direction_length"},"driving":!geometry_driven}),
    ));
    Ok(())
}
