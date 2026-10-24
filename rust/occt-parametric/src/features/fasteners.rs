//! Fastened joints (schema 186): the kernel measures each stack part along
//! every screw's axis line, `occt_parametric_core::fasteners::plan_joint`
//! plans the screw, and this module places the hardware or cuts one part's
//! holes. Identical hardware and hole tools are built once and translated,
//! so a joint costs O(positions · stack) ray casts, a few solids, and one
//! boolean cut per drilled part.

use super::*;
use crate::assembly::{add, cross, dot, rotate_by, scale, subtract, transform_point, unit};
use occt_bridge::WireSegment;
use occt_parametric_core::fasteners::{
    JointHole, JointHoleKind, JointPlan, JointSettings, MAX_JOINT_POSITIONS, MAX_JOINT_STACK,
    StackSpan, check_spacing, plan_joint,
};
use occt_parametric_core::hole_sizes::ClearanceSeries;
use occt_parametric_core::quantity::NormalizedPlacement;
use occt_parametric_core::standard_parts::StandardPartKind;

/// Most surface crossings followed along one screw line through one part.
const MAXIMUM_CROSSINGS: usize = 64;

/// A planned joint with its evaluated placement.
pub struct PlacedJoint {
    pub plan: JointPlan,
    pub positions: Vec<Vec3>,
    pub axis: Vec3,
    pub stack: Vec<String>,
}

/// Where the line `origin + t·axis` crosses the solid's surface, as values
/// of t in order, from repeated nearest hits. O(crossings · ray).
fn crossings(
    session: &Session,
    shape: &Shape<'_>,
    origin: Vec3,
    axis: Vec3,
    reach: f64,
) -> Result<Vec<f64>, ModelError> {
    let mut hits = Vec::new();
    let mut travelled = 0.0;
    let step = 1e-7 * reach.max(1.0);
    while hits.len() < MAXIMUM_CROSSINGS && travelled < reach {
        let from = add(origin, scale(axis, travelled));
        let Some((_, distance)) = session.ray_first_hit(shape, from, axis, reach - travelled)?
        else {
            break;
        };
        travelled += distance;
        hits.push(travelled);
        travelled += step;
    }
    Ok(hits)
}

/// Evaluates and plans a `fastened_joint` operation against its stack's
/// generated shapes, which `placements` (applied in order) have moved.
pub(crate) fn place(
    session: &Session,
    operation: &FeatureOperation,
    parameters: &HashMap<String, ParameterValue>,
    shapes: &HashMap<String, Shape<'_>>,
    placements: &[NormalizedPlacement],
) -> Result<PlacedJoint, ModelError> {
    let FeatureOperation::FastenedJoint {
        stack,
        positions,
        axis,
        nominal_diameter,
        fastening,
        head,
        washers,
        clearance,
        length,
        engagement,
    } = operation
    else {
        return Err(ModelError::new("expected a fastened joint"));
    };
    if stack.is_empty() || stack.len() > MAX_JOINT_STACK {
        return Err(ModelError::new(format!(
            "a fastened joint stacks 1 to {MAX_JOINT_STACK} parts"
        )));
    }
    if (1..stack.len()).any(|i| stack[..i].contains(&stack[i])) {
        return Err(ModelError::new(
            "fastened joint stack parts must be distinct",
        ));
    }
    if positions.is_empty() || positions.len() > MAX_JOINT_POSITIONS {
        return Err(ModelError::new(format!(
            "a fastened joint has 1 to {MAX_JOINT_POSITIONS} positions"
        )));
    }
    let mut axis = unit(vector(axis, parameters, Dimension::Scalar)?)?;
    let mut positions = positions
        .iter()
        .map(|position| vector(position, parameters, Dimension::Length))
        .collect::<Result<Vec<_>, _>>()?;
    for placement in placements {
        axis = rotate_by(axis, placement);
        for position in &mut positions {
            *position = transform_point(*position, placement);
        }
    }
    let optional = |value: &Option<ScalarExpr>| {
        value
            .as_ref()
            .map(|value| scalar(value, parameters, Dimension::Length))
            .transpose()
    };
    let settings = JointSettings {
        nominal_diameter: scalar(nominal_diameter, parameters, Dimension::Length)?,
        fastening: *fastening,
        head: *head,
        washers: *washers,
        clearance: clearance.unwrap_or(ClearanceSeries::Medium),
        length: optional(length)?,
        engagement: optional(engagement)?,
    };
    let parts = stack
        .iter()
        .map(|id| shape(shapes, id))
        .collect::<Result<Vec<_>, _>>()?;
    // Start every measuring line outside all the parts' bounds.
    let (mut low, mut high) = (
        Vec3::new(f64::MAX, f64::MAX, f64::MAX),
        Vec3::new(f64::MIN, f64::MIN, f64::MIN),
    );
    for part in &parts {
        let b = session.exact_bounds(part)?;
        low = Vec3::new(low.x.min(b.min.x), low.y.min(b.min.y), low.z.min(b.min.z));
        high = Vec3::new(
            high.x.max(b.max.x),
            high.y.max(b.max.y),
            high.z.max(b.max.z),
        );
    }
    let size = {
        let d = subtract(high, low);
        dot(d, d).sqrt()
    };
    let center = scale(add(low, high), 0.5);
    let mut spans = Vec::with_capacity(positions.len());
    for (index, &position) in positions.iter().enumerate() {
        let offset = subtract(position, center);
        let back = dot(offset, offset).sqrt() + size + 1.0;
        let start = add(position, scale(axis, -back));
        let mut stack_spans = Vec::with_capacity(parts.len());
        for (part, id) in parts.iter().zip(stack) {
            let hits = crossings(session, part, start, axis, 2.0 * back)?;
            let (Some(first), Some(last)) = (hits.first(), hits.last()) else {
                return Err(ModelError::new(format!(
                    "the screw axis at joint position {} misses '{id}'",
                    index + 1
                )));
            };
            if hits.len() < 2 {
                return Err(ModelError::new(format!(
                    "the screw axis at joint position {} only grazes '{id}'",
                    index + 1
                )));
            }
            stack_spans.push(StackSpan {
                entry: first - back,
                exit: last - back,
            });
        }
        spans.push(stack_spans);
    }
    let names: Vec<&str> = stack.iter().map(String::as_str).collect();
    let plan = plan_joint(&settings, &names, &spans)?;
    let (u, v) = basis(axis)?;
    let offsets: Vec<[f64; 2]> = positions
        .iter()
        .map(|&p| {
            let r = subtract(p, positions[0]);
            [dot(r, u), dot(r, v)]
        })
        .collect();
    check_spacing(&offsets, plan.footprint)?;
    Ok(PlacedJoint {
        plan,
        positions,
        axis,
        stack: stack.clone(),
    })
}

/// Two unit vectors completing `axis` to a right-handed frame.
fn basis(axis: Vec3) -> Result<(Vec3, Vec3), ModelError> {
    let seed = if axis.x.abs() < 0.9 {
        Vec3::new(1.0, 0.0, 0.0)
    } else {
        Vec3::new(0.0, 1.0, 0.0)
    };
    let u = unit(cross(axis, seed))?;
    Ok((u, cross(axis, u)))
}

/// Translated copies of `prototype`, built at `origin`, to each of `targets`.
fn copies<'session>(
    session: &'session Session,
    prototype: &Shape<'_>,
    origin: Vec3,
    targets: impl IntoIterator<Item = Vec3>,
    out: &mut Vec<Shape<'session>>,
) -> Result<(), ModelError> {
    for target in targets {
        out.push(session.translate(prototype, subtract(target, origin))?);
    }
    Ok(())
}

/// A `fastened_joint`'s hardware at every position, as one compound.
pub(super) fn hardware<'session>(
    session: &'session Session,
    operation: &FeatureOperation,
    parameters: &HashMap<String, ParameterValue>,
    shapes: &HashMap<String, Shape<'session>>,
) -> Result<Shape<'session>, ModelError> {
    let joint = place(session, operation, parameters, shapes, &[])?;
    let PlacedJoint {
        plan,
        positions,
        axis,
        ..
    } = &joint;
    let d = plan.nominal_diameter;
    let origin = positions[0];
    let at = |position: Vec3, t: f64| add(position, scale(*axis, t));
    let mut solids = Vec::new();
    let screw = super::primitives::standard_part(
        session,
        StandardPartKind::SocketHeadCapScrewIso4762,
        d,
        Some(plan.length),
        origin,
        *axis,
    )?;
    copies(
        session,
        &screw,
        origin,
        positions
            .iter()
            .zip(&plan.screws)
            .map(|(&p, s)| at(p, s.seat)),
        &mut solids,
    )?;
    let washer_seats: Vec<Vec3> = positions
        .iter()
        .zip(&plan.screws)
        .flat_map(|(&p, s)| {
            [s.head_washer, s.nut_washer]
                .into_iter()
                .flatten()
                .map(move |t| at(p, t))
        })
        .collect();
    if !washer_seats.is_empty() {
        let washer = super::primitives::standard_part(
            session,
            StandardPartKind::PlainWasherIso7089,
            d,
            None,
            origin,
            *axis,
        )?;
        copies(session, &washer, origin, washer_seats, &mut solids)?;
    }
    let nut_seats: Vec<Vec3> = positions
        .iter()
        .zip(&plan.screws)
        .filter_map(|(&p, s)| s.nut.map(|t| at(p, t)))
        .collect();
    if !nut_seats.is_empty() {
        let nut = super::primitives::standard_part(
            session,
            StandardPartKind::HexNutIso4032,
            d,
            None,
            origin,
            *axis,
        )?;
        copies(session, &nut, origin, nut_seats, &mut solids)?;
    }
    Ok(session.create_compound(&solids.iter().collect::<Vec<_>>())?)
}

/// The revolved cutter for one hole, from `position` along `axis`, padded
/// past the part's faces so no cap is coincident.
fn hole_tool<'session>(
    session: &'session Session,
    hole: &JointHole,
    position: Vec3,
    axis: Vec3,
) -> Result<Shape<'session>, ModelError> {
    let (radial, _) = basis(axis)?;
    let pad = (hole.exit - hole.entry).max(1.0) * 1e-3;
    let a = hole.entry - pad;
    let profile: Vec<(f64, f64)> = match hole.kind {
        JointHoleKind::Clearance {
            diameter,
            counterbore,
        } => {
            let r = diameter / 2.0;
            let b = hole.exit + pad;
            match counterbore {
                Some((recess, depth)) => vec![
                    (0.0, a),
                    (recess / 2.0, a),
                    (recess / 2.0, hole.entry + depth),
                    (r, hole.entry + depth),
                    (r, b),
                    (0.0, b),
                ],
                None => vec![(0.0, a), (r, a), (r, b), (0.0, b)],
            }
        }
        JointHoleKind::Tapped { drill, depth, .. } => {
            let r = drill / 2.0;
            let bottom = hole.entry + depth;
            // 118° drill point.
            let tip = bottom + r / 59f64.to_radians().tan();
            vec![(0.0, a), (r, a), (r, bottom), (0.0, tip)]
        }
    };
    let points: Vec<Vec3> = profile
        .iter()
        .map(|&(rho, z)| add(position, add(scale(radial, rho), scale(axis, z))))
        .collect();
    let segments: Vec<WireSegment> = (0..points.len())
        .map(|i| WireSegment::Line {
            start: points[i],
            end: points[(i + 1) % points.len()],
        })
        .collect();
    let wire = session.create_segment_wire(&segments, true)?;
    let face = session.create_face_from_wire(&wire)?;
    Ok(session.create_revolve_from_face(&face, position, axis, std::f64::consts::TAU)?)
}

/// One stack part with the holes its joint needs, cut at once.
pub(super) fn holes<'session>(
    session: &'session Session,
    joint: &str,
    input: &str,
    parameters: &HashMap<String, ParameterValue>,
    shapes: &HashMap<String, Shape<'session>>,
    definitions: &Features<'_>,
) -> Result<Shape<'session>, ModelError> {
    let operation = &definitions
        .by_id
        .get(joint)
        .ok_or_else(|| ModelError::new(format!("unknown joint '{joint}'")))?
        .operation;
    if !matches!(operation, FeatureOperation::FastenedJoint { .. }) {
        return Err(ModelError::new(format!(
            "'{joint}' is not a fastened joint"
        )));
    }
    let placed = place(session, operation, parameters, shapes, &[])?;
    let index = placed
        .stack
        .iter()
        .position(|id| id == input)
        .ok_or_else(|| {
            ModelError::new(format!("'{input}' is not in the stack of joint '{joint}'"))
        })?;
    let target = shape(shapes, input)?;
    if !session.is_valid(target)? {
        return Err(ModelError::new("joint holes input must be valid"));
    }
    // Holes of equal shape differ only by a translation.
    let mut tools: Vec<Shape<'session>> = Vec::with_capacity(placed.positions.len());
    let mut built: Vec<(JointHoleKind, f64, Vec3, usize)> = Vec::new();
    for (&position, screw) in placed.positions.iter().zip(&placed.plan.screws) {
        let hole = &screw.holes[index];
        let start = add(position, scale(placed.axis, hole.entry));
        let thickness = hole.exit - hole.entry;
        let same = built.iter().find(|(kind, t, _, _)| {
            *kind == hole.kind && (t - thickness).abs() <= 1e-9 * thickness.max(1.0)
        });
        let tool = match same {
            Some(&(_, _, origin, k)) => session.translate(&tools[k], subtract(start, origin))?,
            None => {
                built.push((hole.kind, thickness, start, tools.len()));
                hole_tool(session, hole, position, placed.axis)?
            }
        };
        tools.push(tool);
    }
    let cutter = session.create_compound(&tools.iter().collect::<Vec<_>>())?;
    let before = session.volume(target)?;
    let result = session.cut(target, &cutter)?;
    if !session.is_valid(&result)? || session.volume(&result)? >= before {
        return Err(ModelError::new(format!(
            "joint '{joint}' holes did not cut '{input}' cleanly"
        )));
    }
    Ok(result)
}

/// Fastened-joint plans of a generated instance, for parts lists and
/// reports.
pub trait PartInstanceFasteners {
    /// Each `fastened_joint` feature's id and plan, in feature order, for a
    /// result generated from this instance's current definition and
    /// parameters and then moved by `placements` in order (none for a local
    /// result; the instance's placement then its frames for an instance
    /// graph's). Creates no retained handles.
    fn fastened_joints(
        &self,
        session: &Session,
        generated: &GeneratedResult<'_>,
        placements: &[Placement],
    ) -> Result<Vec<(String, PlacedJoint)>, ModelError>;
}

impl PartInstanceFasteners for PartInstance<'_> {
    fn fastened_joints(
        &self,
        session: &Session,
        generated: &GeneratedResult<'_>,
        placements: &[Placement],
    ) -> Result<Vec<(String, PlacedJoint)>, ModelError> {
        let joints: Vec<_> = self
            .definition
            .features
            .iter()
            .filter(|f| matches!(f.operation, FeatureOperation::FastenedJoint { .. }))
            .collect();
        if joints.is_empty() {
            return Ok(Vec::new());
        }
        let parameters = self.resolved_parameters()?;
        let placements = placements
            .iter()
            .map(|placement| placement.normalized())
            .collect::<Result<Vec<_>, _>>()?;
        joints
            .into_iter()
            .map(|feature| {
                place(
                    session,
                    &feature.operation,
                    &parameters,
                    &generated.shapes,
                    &placements,
                )
                .map(|placed| (feature.id.clone(), placed))
                .map_err(|error| error.in_feature(&feature.id))
            })
            .collect()
    }
}

/// The placed plans of every joint on `instance`, by joint feature id, as
/// the instance graph generated and placed them.
fn instance_joints(
    session: &Session,
    graph: &InstanceGraph<'_>,
    generation: &GraphRegeneration<'_>,
    instance: &str,
) -> Result<HashMap<String, PlacedJoint>, ModelError> {
    let resolved = graph.resolve_with_placement(instance)?;
    let result = generation
        .result(instance)
        .ok_or_else(|| ModelError::new(format!("instance '{instance}' was not generated")))?;
    let placements: Vec<Placement> = std::iter::once(resolved.placement)
        .chain(resolved.frames.iter().copied())
        .collect();
    Ok(resolved
        .instance
        .fastened_joints(session, result, &placements)?
        .into_iter()
        .collect())
}

/// Measured callouts for the hole callouts among `dimensions` that
/// reference `joint_holes` features (schema 186). Each referenced instance's
/// joints are measured once. O(referenced joints · positions · stack rays).
pub fn joint_hole_callouts(
    session: &Session,
    graph: &InstanceGraph<'_>,
    generation: &GraphRegeneration<'_>,
    dimensions: &[DrawingDimension],
) -> Result<occt_parametric_core::drawing::dimensions::JointCallouts, ModelError> {
    let references =
        occt_parametric_core::drawing::dimensions::joint_callout_references(dimensions, graph)?;
    let mut measured: HashMap<String, HashMap<String, PlacedJoint>> = HashMap::new();
    let mut callouts = HashMap::new();
    for reference in references {
        if !measured.contains_key(&reference.instance) {
            let joints = instance_joints(session, graph, generation, &reference.instance)?;
            measured.insert(reference.instance.clone(), joints);
        }
        let placed = measured[&reference.instance]
            .get(&reference.joint)
            .ok_or_else(|| {
                ModelError::new(format!(
                    "'{}' is not a fastened joint of instance '{}'",
                    reference.joint, reference.instance
                ))
            })?;
        let part = placed
            .stack
            .iter()
            .position(|id| *id == reference.input)
            .ok_or_else(|| {
                ModelError::new(format!(
                    "'{}' is not in the stack of joint '{}'",
                    reference.input, reference.joint
                ))
            })?;
        let callout = placed
            .plan
            .hole_callout(part, &reference.input)
            .map_err(|error| error.in_feature(&reference.feature))?;
        callouts.insert((reference.instance, reference.feature), callout);
    }
    Ok(callouts)
}

/// The tapped threads of the joint holes `output` is built from, unmoved
/// since, in assembly coordinates: one per screw, from the tapped part's
/// face through the tapped depth. O(features + joints · positions).
pub(crate) fn joint_threads(
    session: &Session,
    graph: &InstanceGraph<'_>,
    generation: &GraphRegeneration<'_>,
    output: &InstanceOutputRef,
) -> Result<Vec<occt_parametric_core::drawing::thread_lines::PlacedThread>, ModelError> {
    use occt_parametric_core::drawing::thread_lines::{PlacedThread, unmoved_ancestors};
    let resolved = graph.resolve_with_placement(&output.instance)?;
    let tapped: Vec<(&str, &str)> = unmoved_ancestors(resolved.instance.definition, &output.output)
        .into_iter()
        .filter_map(|feature| match &feature.operation {
            FeatureOperation::JointHoles { joint, input } => Some((joint.as_str(), input.as_str())),
            _ => None,
        })
        .collect();
    if tapped.is_empty() {
        return Ok(Vec::new());
    }
    let joints = instance_joints(session, graph, generation, &output.instance)?;
    let mut threads = Vec::new();
    for (joint, input) in tapped {
        let Some(placed) = joints.get(joint) else {
            continue;
        };
        let Some(part) = placed.stack.iter().position(|id| id == input) else {
            continue;
        };
        let d = placed.plan.nominal_diameter;
        for (position, screw) in placed.positions.iter().zip(&placed.plan.screws) {
            if let Some(JointHole {
                kind: JointHoleKind::Tapped { depth, pitch, .. },
                entry,
                ..
            }) = screw.holes.get(part)
            {
                threads.push(PlacedThread {
                    start: add(*position, scale(placed.axis, *entry)),
                    axis: placed.axis,
                    length: Some(*depth),
                    major: d,
                    // ISO 68-1 basic minor diameter, D − 2 · 5H/8.
                    minor: d - 1.082_532 * pitch,
                    internal: true,
                });
            }
        }
    }
    Ok(threads)
}
