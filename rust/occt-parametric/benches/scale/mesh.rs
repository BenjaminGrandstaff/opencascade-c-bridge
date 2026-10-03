use super::*;
use occt_parametric::{
    InstanceOutputRef, ManufacturingSettings, MeshExportDefinition, MeshSettings, TaggedSurfaceMesh,
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
