use occt_parametric::*;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Setup {
    pub schema: String,
    pub samples: usize,
    pub components: Vec<Component>,
    #[serde(default)]
    pub collision_options: CollisionOptions,
    #[serde(default)]
    pub continuous_options: ContinuousCollisionOptions,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Component {
    pub id: String,
    pub source: String,
    pub output: String,
    pub hinge: Option<Hinge>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hinge {
    /// In the source instance's enclosing frame coordinates, in millimeters.
    pub origin_mm: [f64; 3],
    pub axis: [f64; 3],
    pub minimum_deg: f64,
    pub maximum_deg: f64,
    pub start_deg: f64,
    pub end_deg: f64,
}

pub fn frame_id(component: &str) -> String {
    format!("motion-study.{component}.hinge")
}

pub fn prepare<'a>(
    document: &'a ModelDocument,
    setup: &Setup,
) -> Result<(InstanceGraph<'a>, MotionStudy), Box<dyn std::error::Error>> {
    if setup.schema != "occb-motion-study-v1"
        || !(2..=MAX_MOTION_SAMPLES).contains(&setup.samples)
        || !(1..=10_000).contains(&setup.components.len())
    {
        return Err("expected occb-motion-study-v1, 2–10000 samples and 1–10000 components".into());
    }
    let sources = document.instance_graph()?;
    let mut graph = sources.clone();
    let mut outputs = Vec::with_capacity(setup.components.len());
    let mut joints = Vec::new();
    for component in &setup.components {
        if let Some(joint) = add_component(&mut graph, &sources, component)? {
            joints.push(joint);
        }
        outputs.push(InstanceOutputRef {
            instance: component.id.clone(),
            output: component.output.clone(),
        });
    }
    if joints.is_empty() {
        return Err("a motion setup needs at least one hinge".into());
    }
    if setup.samples * joints.len() > 1_000_000 {
        return Err("motion setup exceeds one million sampled joint positions".into());
    }
    graph.add_joints(joints)?;
    let mut samples = vec![MotionSample { positions: vec![] }; setup.samples];
    for component in &setup.components {
        add_positions(&mut graph, component, &mut samples)?;
    }
    Ok((
        graph,
        MotionStudy {
            samples,
            outputs,
            collision_options: setup.collision_options,
        },
    ))
}

fn add_component(
    graph: &mut InstanceGraph<'_>,
    sources: &InstanceGraph<'_>,
    component: &Component,
) -> Result<Option<AssemblyJoint>, Box<dyn std::error::Error>> {
    if sources.is_suppressed(&component.source) {
        return Err(format!("source instance '{}' is suppressed", component.source).into());
    }
    let resolved = sources.resolve(&component.source)?;
    if !resolved
        .definition
        .features
        .iter()
        .any(|feature| feature.id == component.output)
    {
        return Err(format!("unknown output '{}:{}'", component.source, component.output).into());
    }
    let source = sources
        .node(&component.source)
        .ok_or("missing source instance")?;
    graph.add_clone(
        &component.id,
        &component.source,
        HashMap::new(),
        "occt-motion-study",
    )?;
    graph.set_placement(&component.id, source.placement())?;
    let Some(hinge) = &component.hinge else {
        graph.set_instance_frame(&component.id, source.frame())?;
        return Ok(None);
    };
    let frame = frame_id(&component.id);
    graph.add_frame(
        &frame,
        source.frame(),
        Placement::identity(),
        "occt-motion-study",
    )?;
    let [x, y, z] = hinge.origin_mm;
    let [ax, ay, az] = hinge.axis;
    graph.set_instance_frame(&component.id, Some(&frame))?;
    Ok(Some(AssemblyJoint {
        id: frame.clone(),
        frame: frame.clone(),
        origin: VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter),
        axis: VectorQuantity::scalars(ax, ay, az),
        kind: JointKind::Revolute {
            angle: JointScalar {
                value: radians(hinge.start_deg)?,
                minimum: Some(radians(hinge.minimum_deg)?),
                maximum: Some(radians(hinge.maximum_deg)?),
            },
        },
    }))
}

fn radians(degrees: f64) -> Result<Quantity, Box<dyn std::error::Error>> {
    if !degrees.is_finite() {
        return Err("hinge angles must be finite degrees".into());
    }
    Ok(Quantity::scalar(degrees.to_radians()))
}

fn add_positions(
    graph: &mut InstanceGraph<'_>,
    component: &Component,
    samples: &mut [MotionSample],
) -> Result<(), Box<dyn std::error::Error>> {
    let Some(hinge) = &component.hinge else {
        return Ok(());
    };
    let start = radians(hinge.start_deg)?;
    let end = radians(hinge.end_deg)?;
    let frame = frame_id(&component.id);
    graph.set_joint_coordinate(&frame, JointDof::Angle, end)?;
    graph.set_joint_coordinate(&frame, JointDof::Angle, start)?;
    let last = samples.len() - 1;
    for (index, sample) in samples.iter_mut().enumerate() {
        let fraction = index as f64 / last as f64;
        sample.positions.push(JointPosition {
            frame: frame_id(&component.id),
            coordinate: JointDof::Angle,
            value: Quantity::scalar((1.0 - fraction) * start.value + fraction * end.value),
        });
    }
    Ok(())
}

/// Preserve non-graph document data such as drawings, revisions and exports.
pub fn assembly_document(original: &ModelDocument, graph: &InstanceGraph<'_>) -> ModelDocument {
    let parts = ModelDocument::from_graph(graph);
    let mut document = original.clone();
    document.instances = parts.instances;
    document.frames = parts.frames;
    document.assembly = parts.assembly;
    document
}
