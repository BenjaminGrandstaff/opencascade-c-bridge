use super::*;
use occt_parametric::{
    AssemblyJoint, CollisionOptions, ContinuousCollisionOptions, ContinuousStatus,
    InstanceOutputRef, JointDof, JointKind, JointScalar, MotionStudy,
};
fn slider(graph: &mut InstanceGraph<'_>) -> Result<(), ModelError> {
    graph.add_frame("slide", None, Placement::identity(), "bench")?;
    graph.add_joint(AssemblyJoint {
        id: "joint".into(),
        frame: "slide".into(),
        origin: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
        axis: VectorQuantity::scalars(1.0, 0.0, 0.0),
        kind: JointKind::Prismatic {
            distance: JointScalar {
                value: Quantity::length(0.0, LengthUnit::Millimeter),
                minimum: None,
                maximum: None,
            },
        },
    })
}
pub(crate) fn continuous_cases(definition: &'static FamilyDefinition) -> Vec<Outcome> {
    vec![
        timed(
            "continuous translation 10000: shared moving bodies".into(),
            ms(10_000),
            Expectation::Required,
            || {
                let session = Session::new().map_err(|error| failure(error.to_string()))?;
                let mut graph = InstanceGraph::new(definition);
                graph.add_base("source", HashMap::new(), "bench")?;
                slider(&mut graph)?;
                let members = graph.add_linear_pattern(
                    "row",
                    "part",
                    "source",
                    10000,
                    VectorQuantity::lengths(50.0, 0.0, 0.0, LengthUnit::Millimeter),
                    "bench",
                )?;
                graph.set_pattern_frame("row", Some("slide"))?;
                let outputs = members
                    .into_iter()
                    .map(|instance| InstanceOutputRef {
                        instance,
                        output: "body".into(),
                    })
                    .collect();
                let study = MotionStudy::linear(
                    "slide",
                    JointDof::Axial,
                    Quantity::length(0.0, LengthUnit::Millimeter),
                    Quantity::length(10.0, LengthUnit::Millimeter),
                    2,
                    outputs,
                    CollisionOptions::default(),
                )?;
                let result = graph.check_translation_motion(
                    &session,
                    &study,
                    ContinuousCollisionOptions::default(),
                )?;
                if result.status != ContinuousStatus::Clear
                    || result.exact_queries != 0
                    || result.candidate_pairs != 0
                    || result.generated_variants != 1
                    || session
                        .shape_count()
                        .map_err(|error| failure(error.to_string()))?
                        != 0
                {
                    return Err(failure(
                        "continuous sparse sharing/culling/cleanup failed".into(),
                    ));
                }
                Ok("10000 translating solids; one local variant; swept BVH rejects all pairs; no retained handles".into())
            },
        ),
        timed(
            "continuous translation 1000: nonuniform obstacle crossings".into(),
            ms(30_000),
            Expectation::Required,
            || {
                let session = Session::new().map_err(|error| failure(error.to_string()))?;
                let mut graph = InstanceGraph::new(definition);
                graph.add_base("moving", HashMap::new(), "bench")?;
                graph.add_clone("obstacle", "moving", HashMap::new(), "bench")?;
                slider(&mut graph)?;
                graph.set_instance_frame("moving", Some("slide"))?;
                let outputs = vec![
                    InstanceOutputRef {
                        instance: "moving".into(),
                        output: "body".into(),
                    },
                    InstanceOutputRef {
                        instance: "obstacle".into(),
                        output: "body".into(),
                    },
                ];
                let study = MotionStudy::linear(
                    "slide",
                    JointDof::Axial,
                    Quantity::length(0.0, LengthUnit::Millimeter),
                    Quantity::length(100.0, LengthUnit::Millimeter),
                    2,
                    outputs,
                    CollisionOptions::default(),
                )?;
                let mut queries = 0;
                for index in 0..1000 {
                    graph.set_placement(
                        "obstacle",
                        Placement::translated(VectorQuantity::lengths(
                            17.37 + index as f64 * 0.061,
                            0.0,
                            0.0,
                            LengthUnit::Millimeter,
                        )),
                    )?;
                    let report = graph.check_translation_motion(
                        &session,
                        &study,
                        ContinuousCollisionOptions::default(),
                    )?;
                    if report.status != ContinuousStatus::Collision
                        || report.unresolved_pairs != 0
                        || report.generated_variants != 1
                    {
                        return Err(failure("continuous crossing was not witnessed".into()));
                    }
                    queries += report.exact_queries;
                }
                if session
                    .shape_count()
                    .map_err(|error| failure(error.to_string()))?
                    != 0
                {
                    return Err(failure("continuous handles retained".into()));
                }
                Ok(format!(
                    "1000 swept crossings, {queries} exact BREP pair queries; no retained handles"
                ))
            },
        ),
    ]
}
