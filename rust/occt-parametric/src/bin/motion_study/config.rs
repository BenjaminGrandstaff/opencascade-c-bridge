use occt_parametric::*;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};

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
    #[serde(default)]
    pub excluded_pairs: Vec<[String; 2]>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Component {
    pub id: String,
    pub source: String,
    pub output: String,
    pub hinge: Option<Hinge>,
    pub slider: Option<Slider>,
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

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Slider {
    /// Direction in the source instance's enclosing frame; normalized by the engine.
    pub axis: [f64; 3],
    pub minimum_mm: f64,
    pub maximum_mm: f64,
    pub start_mm: f64,
    pub end_mm: f64,
}

pub fn slider_frame_id(component: &str) -> String {
    format!("motion-study.{component}.slider")
}

struct Drive {
    joint: AssemblyJoint,
    coordinate: JointDof,
    start: Quantity,
    end: Quantity,
}

fn drive(component: &Component) -> Result<Option<Drive>, Box<dyn std::error::Error>> {
    match (&component.hinge, &component.slider) {
        (None, None) => Ok(None),
        (Some(hinge), None) => hinge_drive(&component.id, hinge).map(Some),
        (None, Some(slider)) => slider_drive(&component.id, slider).map(Some),
        (Some(_), Some(_)) => Err("a component cannot have both a hinge and a slider".into()),
    }
}

fn hinge_drive(id: &str, hinge: &Hinge) -> Result<Drive, Box<dyn std::error::Error>> {
    let start = radians(hinge.start_deg)?;
    let [x, y, z] = hinge.origin_mm;
    let [ax, ay, az] = hinge.axis;
    Ok(Drive {
        joint: AssemblyJoint {
            id: frame_id(id),
            frame: frame_id(id),
            origin: VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter),
            axis: VectorQuantity::scalars(ax, ay, az),
            kind: JointKind::Revolute {
                angle: JointScalar {
                    value: start,
                    minimum: Some(radians(hinge.minimum_deg)?),
                    maximum: Some(radians(hinge.maximum_deg)?),
                },
            },
        },
        coordinate: JointDof::Angle,
        start,
        end: radians(hinge.end_deg)?,
    })
}

fn slider_drive(id: &str, slider: &Slider) -> Result<Drive, Box<dyn std::error::Error>> {
    let start = millimeters(slider.start_mm)?;
    let [ax, ay, az] = slider.axis;
    Ok(Drive {
        joint: AssemblyJoint {
            id: slider_frame_id(id),
            frame: slider_frame_id(id),
            origin: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
            axis: VectorQuantity::scalars(ax, ay, az),
            kind: JointKind::Prismatic {
                distance: JointScalar {
                    value: start,
                    minimum: Some(millimeters(slider.minimum_mm)?),
                    maximum: Some(millimeters(slider.maximum_mm)?),
                },
            },
        },
        coordinate: JointDof::Axial,
        start,
        end: millimeters(slider.end_mm)?,
    })
}

fn millimeters(value: f64) -> Result<Quantity, Box<dyn std::error::Error>> {
    if !value.is_finite() {
        return Err("slider travel must be finite millimeters".into());
    }
    Ok(Quantity::length(value, LengthUnit::Millimeter))
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
        return Err("a motion setup needs at least one hinge or slider".into());
    }
    if setup.samples * joints.len() > 1_000_000 {
        return Err("motion setup exceeds one million sampled joint positions".into());
    }
    let excluded_pairs = excluded_pairs(setup, &outputs)?;
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
            excluded_pairs,
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
    let Some(drive) = drive(component)? else {
        graph.set_instance_frame(&component.id, source.frame())?;
        return Ok(None);
    };
    graph.add_frame(
        &drive.joint.frame,
        source.frame(),
        Placement::identity(),
        "occt-motion-study",
    )?;
    graph.set_instance_frame(&component.id, Some(&drive.joint.frame))?;
    Ok(Some(drive.joint))
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
    let Some(drive) = drive(component)? else {
        return Ok(());
    };
    let frame = drive.joint.frame;
    graph.set_joint_coordinate(&frame, drive.coordinate, drive.end)?;
    graph.set_joint_coordinate(&frame, drive.coordinate, drive.start)?;
    let last = samples.len() - 1;
    for (index, sample) in samples.iter_mut().enumerate() {
        let fraction = index as f64 / last as f64;
        let mut value = drive.start;
        value.value = (1.0 - fraction) * drive.start.value + fraction * drive.end.value;
        sample.positions.push(JointPosition {
            frame: frame.clone(),
            coordinate: drive.coordinate,
            value,
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

fn excluded_pairs(
    setup: &Setup,
    outputs: &[InstanceOutputRef],
) -> Result<Vec<CollisionPairRef>, Box<dyn std::error::Error>> {
    if setup.excluded_pairs.len() > 1_000_000 {
        return Err("at most one million collision exclusions are supported".into());
    }
    let selected = outputs
        .iter()
        .map(|output| (output.instance.as_str(), output))
        .collect::<HashMap<_, _>>();
    let mut seen = HashSet::new();
    setup
        .excluded_pairs
        .iter()
        .map(|[first, second]| {
            if first == second || !seen.insert((first.min(second), first.max(second))) {
                return Err(
                    "collision exclusions must be distinct unordered pairs without self-pairs"
                        .into(),
                );
            }
            let resolve = |id: &str| {
                selected
                    .get(id)
                    .map(|output| (*output).clone())
                    .ok_or_else(|| format!("excluded component '{id}' is not selected"))
            };
            Ok(CollisionPairRef {
                first: resolve(first)?,
                second: resolve(second)?,
            })
        })
        .collect()
}
