//! Surface texture symbols at drawing scale: 10,000 requirements generated
//! and exported in one drawing.

use super::*;
use occt_parametric::{
    DrawingDefinition, DrawingGdtAttachment, DrawingRenderOptions, DrawingSurfaceTexture,
    DrawingView, DrawingViewKind, InstanceOutputRef, MaterialRemoval, RoughnessLimits,
    RoughnessParameter, RoughnessUnit, SurfaceLay, Waviness,
};

const TEXTURES: usize = 10_000;

pub(crate) fn texture_case(definition: &FamilyDefinition) -> Outcome {
    timed(
        format!("surface textures {TEXTURES}: symbols, SVG and DXF"),
        Duration::from_secs(1),
        Expectation::Required,
        || {
            let session = Session::new().map_err(|error| failure(error.to_string()))?;
            let mut definition = definition.clone();
            definition.datums = vec![DatumDefinition {
                id: "top".into(),
                kind: DatumKind::Plane {
                    origin: VectorExpr::Literal(VectorQuantity::lengths(
                        0.0,
                        0.0,
                        10.0,
                        LengthUnit::Millimeter,
                    )),
                    normal: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
                },
            }];
            let mut graph = InstanceGraph::new(&definition);
            graph.add_base("part", HashMap::new(), "bench")?;
            let output = InstanceOutputRef {
                instance: "part".into(),
                output: "body".into(),
            };
            let surface_textures = (0..TEXTURES)
                .map(|i| DrawingSurfaceTexture {
                    id: format!("texture-{i}"),
                    attachment: DrawingGdtAttachment {
                        view: "top".into(),
                        output: output.clone(),
                        anchor: DatumRef::new("part", "top"),
                        offset_mm: [10.0 + (i % 100) as f64 * 9.0, 10.0 + (i / 100) as f64 * 9.0],
                    },
                    unit: RoughnessUnit::Micrometer,
                    roughness: RoughnessLimits {
                        parameter: RoughnessParameter::Ra,
                        maximum: 1.6,
                        minimum: (i % 2 == 0).then_some(0.4),
                    },
                    cutoff_mm: Some(0.8),
                    waviness: (i % 3 == 0).then_some(Waviness {
                        height_mm: 0.05,
                        spacing_mm: 25.0,
                    }),
                    lay: Some(if i % 2 == 0 {
                        SurfaceLay::Perpendicular
                    } else {
                        SurfaceLay::Circular
                    }),
                    material_removal: [
                        MaterialRemoval::Any,
                        MaterialRemoval::Required,
                        MaterialRemoval::Prohibited,
                    ][i % 3],
                    method: (i % 5 == 0).then(|| "GRIND".into()),
                    all_around: i % 7 == 0,
                })
                .collect();
            let drawing = DrawingDefinition {
                datum_reference_frames: Vec::new(),
                datum_features: Vec::new(),
                feature_control_frames: Vec::new(),
                surface_textures,
                parts_list: None,
                balloons: Vec::new(),
                releases: Vec::new(),
                revision_table: None,
                sheet: None,
                id: "textures".into(),
                title: "Textures".into(),
                paper_size_mm: [1000.0, 1000.0],
                guides: Vec::new(),
                dimensions: Vec::new(),
                notes: Vec::new(),
                metadata: Default::default(),
                views: vec![DrawingView {
                    id: "top".into(),
                    outputs: vec![output],
                    origin: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
                    direction: VectorQuantity::scalars(0.0, 0.0, 1.0),
                    x_axis: VectorQuantity::scalars(1.0, 0.0, 0.0),
                    paper_origin_mm: [10.0, 10.0],
                    scale: 1.0,
                    show_hidden: false,
                    kind: DrawingViewKind::Orthographic,
                    detail: None,
                    hatching: None,
                    material_hatching: Default::default(),
                }],
            };
            let generated = drawing.generate(
                &graph,
                &session,
                DrawingRenderOptions {
                    exact_curves: false,
                    curve_tolerance_mm: 0.01,
                    curve_samples: 8,
                    maximum_vertices: 2_000_000,
                },
            )?;
            let svg = generated.to_svg();
            let dxf = generated.to_dxf();
            let labels = generated.gdt_labels.len();
            if labels < 2 * TEXTURES
                || !svg.contains("Ra 0.4-1.6 µm")
                || !dxf.contains("Lc 0.8")
                || session
                    .shape_count()
                    .map_err(|error| failure(error.to_string()))?
                    != 0
            {
                return Err(failure(
                    "texture symbols missing or handles retained".into(),
                ));
            }
            Ok(format!(
                "{labels} labels, {} line vertices, {} kB SVG, no retained handles",
                generated
                    .gdt_lines
                    .iter()
                    .map(|l| l.points_mm.len())
                    .sum::<usize>(),
                svg.len() / 1024
            ))
        },
    )
}
