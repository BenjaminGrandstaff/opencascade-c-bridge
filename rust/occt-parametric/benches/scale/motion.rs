use super::*;
use occt_parametric::{
    AssemblyJoint, CollisionOptions, InstanceOutputRef, JointDof, JointKind, JointScalar,
    MotionStudy,
};

fn joint(frame: String) -> AssemblyJoint {
    AssemblyJoint {
        id: format!("{frame}.joint"),
        frame,
        origin: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
        axis: VectorQuantity::scalars(1.0, 0.0, 0.0),
        kind: JointKind::Prismatic {
            distance: JointScalar {
                value: Quantity::length(0.0, LengthUnit::Millimeter),
                minimum: None,
                maximum: None,
            },
        },
    }
}

pub(crate) fn assembly_cases(definition: &'static FamilyDefinition) -> Vec<Outcome> {
    let mut outcomes = Vec::new();
    outcomes.push(timed(
        "assembly joints 10000: insert and drive".into(),
        ms(2000),
        Expectation::Required,
        || {
            let mut graph = InstanceGraph::new(definition);
            for index in 0..10_000 {
                graph.add_frame(
                    format!("frame{index}"),
                    None,
                    Placement::identity(),
                    "bench",
                )?;
            }
            graph.add_joints((0..10_000).map(|index| joint(format!("frame{index}"))))?;
            for index in 0..10_000 {
                graph.set_joint_coordinate(
                    &format!("frame{index}"),
                    JointDof::Axial,
                    Quantity::length(index as f64, LengthUnit::Millimeter),
                )?;
            }
            if graph.joints().count() != 10_000 {
                return Err(failure("joint count changed".into()));
            }
            Ok("10000 checked joints and pose edits".into())
        },
    ));
    outcomes.push(timed(
        "assembly collision BVH 10000: separated solids".into(),
        ms(10_000),
        Expectation::Required,
        || {
            let session = Session::new().map_err(|error| failure(error.to_string()))?;
            let mut graph = InstanceGraph::new(definition);
            graph.add_base("source", HashMap::new(), "bench")?;
            let members = graph.add_linear_pattern(
                "row",
                "part",
                "source",
                10_000,
                VectorQuantity::lengths(50.0, 0.0, 0.0, LengthUnit::Millimeter),
                "bench",
            )?;
            let outputs = members
                .iter()
                .map(|instance| InstanceOutputRef {
                    instance: instance.clone(),
                    output: "body".into(),
                })
                .collect::<Vec<_>>();
            let generation = graph.regenerate_instances(
                &session,
                &members.iter().map(String::as_str).collect::<Vec<_>>(),
            )?;
            if generation.generated_variants() != 1 {
                return Err(failure("geometry reuse lost".into()));
            }
            let collisions =
                generation.check_collisions(&session, &outputs, CollisionOptions::default())?;
            if !collisions.is_empty() {
                return Err(failure("separated solids reported collisions".into()));
            }
            drop(generation);
            if session
                .shape_count()
                .map_err(|error| failure(error.to_string()))?
                != 0
            {
                return Err(failure("collision handles leaked".into()));
            }
            Ok("1 generated variant, 10000 bodies, no contacts or leaked handles".into())
        },
    ));
    outcomes.push(timed(
        "assembly motion 1000: shared slider and obstacle".into(),
        ms(20_000),
        Expectation::Required,
        || {
            let session = Session::new().map_err(|error| failure(error.to_string()))?;
            let mut graph = InstanceGraph::new(definition);
            graph.add_base("moving", HashMap::new(), "bench")?;
            graph.add_clone("obstacle", "moving", HashMap::new(), "bench")?;
            graph.add_frame("slide", None, Placement::identity(), "bench")?;
            graph.set_instance_frame("moving", Some("slide"))?;
            graph.add_joint(joint("slide".into()))?;
            graph.set_placement(
                "obstacle",
                Placement::translated(VectorQuantity::lengths(
                    20.0,
                    0.0,
                    0.0,
                    LengthUnit::Millimeter,
                )),
            )?;
            let outputs = ["moving", "obstacle"]
                .map(|instance| InstanceOutputRef {
                    instance: instance.into(),
                    output: "body".into(),
                })
                .to_vec();
            let study = MotionStudy::linear(
                "slide",
                JointDof::Axial,
                Quantity::length(0.0, LengthUnit::Millimeter),
                Quantity::length(40.0, LengthUnit::Millimeter),
                1000,
                outputs,
                CollisionOptions::default(),
            )?;
            let result = graph.run_motion_study(&session, &study)?;
            if result.generated_variants != 1
                || result.samples.len() != 1000
                || !result.samples[0].collisions.is_empty()
                || result.samples[500].collisions.is_empty()
            {
                return Err(failure("motion geometry or contacts differ".into()));
            }
            if session
                .shape_count()
                .map_err(|error| failure(error.to_string()))?
                != 0
            {
                return Err(failure("motion handles leaked".into()));
            }
            Ok("1000 samples, one variant, crossing detected, zero retained handles".into())
        },
    ));
    outcomes.push(timed(
        "assembly mass properties 1000: inherited material".into(),
        ms(5000),
        Expectation::Required,
        || {
            let session = Session::new().map_err(|error| failure(error.to_string()))?;
            let mut graph = InstanceGraph::new(definition);
            graph.add_base("source", HashMap::new(), "bench")?;
            graph.add_material(occt_parametric::Material {
                id: "material".into(),
                name: "Test density".into(),
                density_kg_per_cubic_meter: 1000.0,
            })?;
            graph.assign_material("source", Some("material"))?;
            let members = graph.add_linear_pattern(
                "row",
                "part",
                "source",
                1000,
                VectorQuantity::lengths(50.0, 0.0, 0.0, LengthUnit::Millimeter),
                "bench",
            )?;
            let outputs = members
                .iter()
                .map(|instance| InstanceOutputRef {
                    instance: instance.clone(),
                    output: "body".into(),
                })
                .collect::<Vec<_>>();
            let report = graph.mass_properties(&session, &outputs)?;
            if report.generated_variants != 1
                || (report.total.mass_kg - 6.0).abs() > 1e-8
                || (report.total.center_mm.x - 24980.0).abs() > 1e-6
                || session
                    .shape_count()
                    .map_err(|error| failure(error.to_string()))?
                    != 0
            {
                return Err(failure(
                    "mass, center, geometry reuse, or cleanup differs".into(),
                ));
            }
            Ok("1000 components, 6 kg, shared geometry, no retained handles".into())
        },
    ));
    outcomes
}
