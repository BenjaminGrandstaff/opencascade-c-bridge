//! Surface texture symbols at drawing scale: 10,000 requirements generated
//! and exported in one drawing.

use super::*;
use occt_parametric::{
    DrawingDefinition, DrawingGdtAttachment, DrawingRenderOptions, DrawingSurfaceTexture,
    DrawingSurfaceTextureFaces, DrawingView, DrawingViewKind, InspectionRecord, InstanceOutputRef,
    MaterialRemoval, MeasuredProfile, MeasuredTexture, NamedReference, ReferenceTarget,
    RoughnessLimits, RoughnessParameter, RoughnessUnit, SurfaceLay, TextureAcceptance,
    TextureResult, Waviness,
};

const TEXTURES: usize = 10_000;

/// A one-view drawing of `count` texture requirements on the bench part.
fn textured(
    definition: &FamilyDefinition,
    count: usize,
    acceptance: TextureAcceptance,
) -> (FamilyDefinition, DrawingDefinition) {
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
    let output = InstanceOutputRef {
        instance: "part".into(),
        output: "body".into(),
    };
    let surface_textures = (0..count)
        .map(|i| DrawingSurfaceTexture {
            id: format!("texture-{i}"),
            attachment: DrawingGdtAttachment {
                view: "top".into(),
                output: output.clone(),
                anchor: DatumRef::new("part", "top"),
                offset_mm: [10.0 + (i % 100) as f64 * 9.0, 10.0 + (i / 100) as f64 * 9.0],
                faces: None,
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
            acceptance,
        })
        .collect();
    let drawing = DrawingDefinition {
        annotation_views: Vec::new(),
        dataset: None,
        approval_workflow: None,
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
            thread_representation: false,
        }],
    };
    (definition, drawing)
}

pub(crate) fn texture_case(definition: &FamilyDefinition) -> Outcome {
    timed(
        format!("surface textures {TEXTURES}: symbols, SVG and DXF"),
        Duration::from_secs(1),
        Expectation::Required,
        || {
            let session = Session::new().map_err(|error| failure(error.to_string()))?;
            let (definition, drawing) = textured(definition, TEXTURES, TextureAcceptance::MaxRule);
            let mut graph = InstanceGraph::new(&definition);
            graph.add_base("part", HashMap::new(), "bench")?;
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

/// Stylus profiles at the filter's limits: 0.4 µm spacing gives 2,000
/// samples per 0.8 mm cutoff, and 83 six-cutoff traces fill the record's
/// 1,000,000-sample budget. Each is Gaussian filtered and accepted by the
/// 16% rule.
pub(crate) fn profile_case(definition: &FamilyDefinition) -> Outcome {
    const PROFILES: usize = 83;
    timed(
        format!("surface textures: {PROFILES} filtered stylus profiles, 996000 samples"),
        Duration::from_secs(3),
        Expectation::Required,
        || {
            let (definition, mut drawing) =
                textured(definition, PROFILES, TextureAcceptance::SixteenPercent);
            // Without a minimum, which takes three readings, one filtered
            // reading under 70% of the maximum decides each texture.
            for texture in &mut drawing.surface_textures {
                texture.roughness.minimum = None;
            }
            let mut graph = InstanceGraph::new(&definition);
            graph.add_base("part", HashMap::new(), "bench")?;
            let surface_textures = (0..PROFILES)
                .map(|i| MeasuredTexture {
                    id: format!("texture-{i}"),
                    values: Vec::new(),
                    profiles: vec![MeasuredProfile {
                        spacing_um: 0.4,
                        heights_um: (0..12_000)
                            .map(|j| {
                                let x = j as f64 * 0.4;
                                0.001 * x
                                    + (1.0 + i as f64 * 0.005)
                                        * (std::f64::consts::TAU * x / 50.0).sin()
                            })
                            .collect(),
                    }],
                })
                .collect();
            let record = InspectionRecord {
                datum_targets: Vec::new(),
                drawing: drawing.id.clone(),
                datum_features: Vec::new(),
                controls: Vec::new(),
                surface_textures,
                threads: Vec::new(),
                uncertainty: None,
            };
            let report = drawing.evaluate_inspection(&graph, &record)?;
            // Ra = 2A/π lies between 0.64 and 0.9 µm, under 70% of 1.6 µm.
            let accepted = report
                .surface_textures
                .iter()
                .filter(|t| matches!(&t.result, TextureResult::Evaluated(m) if m.conforms))
                .count();
            if accepted != PROFILES {
                return Err(failure(format!(
                    "{accepted} of {PROFILES} textures accepted"
                )));
            }
            Ok(format!(
                "{accepted} textures accepted from filtered profiles"
            ))
        },
    )
}

/// 10,000 requirements on the bench part's six named faces: each face's
/// requirements agree, their leaders land on the faces, and the drawing
/// generates with one local regeneration of the part.
pub(crate) fn face_case(definition: &FamilyDefinition) -> Outcome {
    timed(
        format!("surface textures {TEXTURES} on persistent faces: faces, landings, drawing"),
        Duration::from_secs(1),
        Expectation::Required,
        || {
            let session = Session::new().map_err(|error| failure(error.to_string()))?;
            let (mut definition, mut drawing) =
                textured(definition, TEXTURES, TextureAcceptance::MaxRule);
            let names = ["x-min", "x-max", "y-min", "y-max", "z-min", "z-max"];
            definition.references = names
                .iter()
                .enumerate()
                .map(|(i, name)| NamedReference {
                    name: (*name).into(),
                    target: ReferenceTarget::Faces(FaceSelector::AtExtreme {
                        axis: [CoordinateAxis::X, CoordinateAxis::Y, CoordinateAxis::Z][i / 2],
                        extremum: if i % 2 == 0 {
                            Extremum::Minimum
                        } else {
                            Extremum::Maximum
                        },
                        tolerance: ScalarExpr::Literal(Quantity::length(
                            1e-6,
                            LengthUnit::Millimeter,
                        )),
                    }),
                })
                .collect();
            for (i, texture) in drawing.surface_textures.iter_mut().enumerate() {
                let face = i % names.len();
                texture.attachment.faces = Some(FaceSelector::Named(names[face].into()));
                // One requirement per face, repeated in every texture on it.
                texture.roughness = RoughnessLimits {
                    parameter: RoughnessParameter::Ra,
                    maximum: 0.8 * (face + 1) as f64,
                    minimum: None,
                };
                texture.waviness = None;
                texture.lay = None;
                texture.material_removal = MaterialRemoval::Required;
                texture.method = None;
            }
            let mut graph = InstanceGraph::new(&definition);
            graph.add_base("part", HashMap::new(), "bench")?;
            let faces = drawing.surface_texture_faces(&graph, &session)?;
            let distinct: std::collections::BTreeSet<_> =
                faces.iter().map(|f| f.faces.clone()).collect();
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
            if faces.len() != TEXTURES
                || distinct.len() != names.len()
                || generated.gdt_labels.len() < TEXTURES
                || session
                    .shape_count()
                    .map_err(|error| failure(error.to_string()))?
                    != 0
            {
                return Err(failure(format!(
                    "{} textures on {} distinct face sets",
                    faces.len(),
                    distinct.len()
                )));
            }
            Ok(format!(
                "{} textures on {} faces, {} labels, no retained handles",
                faces.len(),
                distinct.len(),
                generated.gdt_labels.len()
            ))
        },
    )
}
