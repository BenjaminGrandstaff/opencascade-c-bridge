use super::*;
use occt_parametric::{
    InstanceGraphParallelExport, InstanceOutputRef, ManufacturingSettings, MeshExportDefinition,
    MeshSettings, TaggedSurfaceMesh,
};

pub(crate) fn gltf_case(definition: &'static FamilyDefinition) -> Outcome {
    timed(
        "glTF 1000 parts: shared generation and mesh buffers".into(),
        ms(10_000),
        Expectation::Required,
        || {
            let session = Session::new().map_err(|error| failure(error.to_string()))?;
            let mut graph = InstanceGraph::new(definition);
            graph.add_base("part", HashMap::new(), "bench")?;
            let members = graph.add_linear_pattern(
                "row",
                "member",
                "part",
                1000,
                VectorQuantity::lengths(50.0, 0.0, 0.0, LengthUnit::Millimeter),
                "bench",
            )?;
            let definitions: Vec<_> = members
                .into_iter()
                .map(|instance| MeshExportDefinition {
                    id: instance.clone(),
                    output: InstanceOutputRef {
                        instance,
                        output: "body".into(),
                    },
                    settings: MeshSettings::default(),
                    face_tags: vec![],
                    manufacturing: None,
                })
                .collect();
            let exported = graph.export_gltf(&session, &definitions)?;
            let value: serde_json::Value =
                serde_json::from_str(&exported).map_err(|error| failure(error.to_string()))?;
            if value["nodes"].as_array().map(Vec::len) != Some(1000)
                || value["meshes"].as_array().map(Vec::len) != Some(1)
                || value["extras"]["generatedVariants"] != 1
                || session
                    .shape_count()
                    .map_err(|error| failure(error.to_string()))?
                    != 0
            {
                return Err(failure(
                    "glTF lost components, failed sharing, or leaked handles".into(),
                ));
            }
            Ok("1000 posed nodes, one mesh, one generated variant, no retained handles".into())
        },
    )
}
pub(crate) fn manufacturing_case() -> Outcome {
    timed(
        "manufacturing mesh: 10000 indexed wall rays".into(),
        ms(10_000),
        Expectation::Required,
        || {
            let session = Session::new().map_err(|error| failure(error.to_string()))?;
            let sphere = session
                .create_sphere(Vec3::new(0.0, 0.0, 0.0), 100.0)
                .map_err(|error| failure(error.to_string()))?;
            let triangles = session
                .surface_mesh(
                    &sphere,
                    occt_bridge::MeshOptions {
                        linear_deflection: 0.01,
                        angular_deflection_radians: 0.1,
                        ..occt_bridge::MeshOptions::default()
                    },
                )
                .map_err(|error| failure(error.to_string()))?;
            let count = triangles.len();
            let mesh = TaggedSurfaceMesh {
                id: "sphere".into(),
                triangles,
                face_tags: vec![0],
                names: vec![],
                faces: vec![],
            };
            let report = mesh.check_manufacturability(ManufacturingSettings {
                maximum_wall_samples: 10000,
                ..ManufacturingSettings::default()
            })?;
            if report.wall_samples.len() != 10000
                || report.unresolved_wall_samples != 0
                || report
                    .minimum_sampled_wall_mm
                    .is_none_or(|value| value < 150.0 || value > 200.0)
            {
                return Err(failure(format!(
                    "invalid indexed wall screening: {} samples, {} unresolved, {:?} minimum",
                    report.wall_samples.len(),
                    report.unresolved_wall_samples,
                    report.minimum_sampled_wall_mm
                )));
            }
            drop(sphere);
            if session
                .shape_count()
                .map_err(|error| failure(error.to_string()))?
                != 0
            {
                return Err(failure("mesh screening leaked handles".into()));
            }
            Ok(format!(
                "{count} triangles, 10000 indexed rays, sampled draft/overhang, no retained handles"
            ))
        },
    )
}

/// The browser viewer's model export: every instance with an output, selected
/// by name rather than listed, at the 10,000-instance target.
pub(crate) fn gltf_output_case(definition: &'static FamilyDefinition) -> Outcome {
    timed(
        "glTF of a named output on 10000 parts (viewer export)".into(),
        ms(2_000),
        Expectation::Required,
        || {
            let session = Session::new().map_err(|error| failure(error.to_string()))?;
            let mut graph = InstanceGraph::new(definition);
            graph.add_base("part", HashMap::new(), "bench")?;
            graph.add_linear_pattern(
                "row",
                "member",
                "part",
                10_000,
                VectorQuantity::lengths(50.0, 0.0, 0.0, LengthUnit::Millimeter),
                "bench",
            )?;
            let exported = graph.export_gltf_output(&session, "body", MeshSettings::default())?;
            let value: serde_json::Value =
                serde_json::from_str(&exported).map_err(|error| failure(error.to_string()))?;
            if value["nodes"].as_array().map(Vec::len) != Some(10_001)
                || value["meshes"].as_array().map(Vec::len) != Some(1)
                || session
                    .shape_count()
                    .map_err(|error| failure(error.to_string()))?
                    != 0
            {
                return Err(failure(
                    "viewer glTF lost parts, failed sharing, or leaked handles".into(),
                ));
            }
            Ok("10001 named nodes, one shared mesh, no retained handles".into())
        },
    )
}

/// 400 distinct parameter variants (each block its own width), regenerated
/// and exported on eight worker threads, as the viewer loads a model. The
/// case's time covers both builds; the detail gives each.
pub(crate) fn gltf_parallel_case(definition: &'static FamilyDefinition) -> Outcome {
    timed(
        "glTF of 400 distinct variants on 8 worker threads".into(),
        ms(3_000),
        Expectation::Required,
        || {
            let session = Session::new().map_err(|error| failure(error.to_string()))?;
            let mut graph = InstanceGraph::new(definition);
            for i in 0..400 {
                let overrides = HashMap::from([(
                    "width".to_owned(),
                    ParameterValue::Scalar(Quantity::length(
                        20.0 + i as f64 * 0.01,
                        LengthUnit::Millimeter,
                    )),
                )]);
                graph.add_base(format!("part{i:03}"), overrides, "bench")?;
            }
            let started = Instant::now();
            let parallel = graph
                .regenerate_gltf_parallel(&session, "body", MeshSettings::default(), 8)?
                .ok_or_else(|| failure("the parallel build was refused".into()))?;
            let threaded = started.elapsed();
            if parallel.generated_variants != 400 || parallel.instances.len() != 400 {
                return Err(failure("the parallel build lost variants".into()));
            }
            let started = Instant::now();
            let generation = graph.regenerate_all(&session)?;
            graph.export_gltf_generated(&session, &generation, "body", MeshSettings::default())?;
            let serial = started.elapsed();
            Ok(format!(
                "400 variants and nodes: {:.2} s on 8 threads, {:.2} s serial",
                threaded.as_secs_f64(),
                serial.as_secs_f64()
            ))
        },
    )
}
