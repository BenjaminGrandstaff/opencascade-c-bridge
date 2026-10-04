use occt_parametric::*;
use serde::Deserialize;
use std::{
    collections::HashMap,
    error::Error,
    fs,
    path::{Path, PathBuf},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Setup {
    pub schema: String,
    pub components: Vec<Component>,
    #[serde(default)]
    pub materials: Vec<Material>,
    pub reference: Reference,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Component {
    pub id: String,
    pub source: String,
    pub output: String,
    /// Omit to inherit the source material; no density is assumed implicitly.
    pub material: Option<String>,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Reference {
    Chord {
        chord: ChordReference,
    },
    /// Relative to the setup file; editor stations are interpreted in model XYZ.
    WingProject {
        project: PathBuf,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Project {
    schema: String,
    span_mm: f64,
    stations: Vec<Station>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Station {
    fraction: f64,
    chord_mm: f64,
    leading_edge_mm: f64,
    height_mm: f64,
}

pub fn reference(
    setup: &Setup,
    directory: &Path,
) -> Result<(ChordReference, Option<MeanAerodynamicChord>), Box<dyn Error>> {
    match &setup.reference {
        Reference::Chord { chord } => {
            chord.validate()?;
            Ok((*chord, None))
        }
        Reference::WingProject { project } => {
            let project: Project =
                serde_json::from_str(&fs::read_to_string(directory.join(project))?)?;
            let mac = wing_reference(&project)?;
            Ok((
                mac.reference(VectorQuantity::scalars(1.0, 0.0, 0.0)),
                Some(mac),
            ))
        }
    }
}

fn wing_reference(project: &Project) -> Result<MeanAerodynamicChord, Box<dyn Error>> {
    if project.schema != "occb-wing-layout-v1"
        || !(project.span_mm.is_finite() && project.span_mm > 0.0)
        || project
            .stations
            .first()
            .is_none_or(|station| station.fraction != 0.0)
        || project
            .stations
            .last()
            .is_none_or(|station| station.fraction != 1.0)
    {
        return Err("wing reference needs occb-wing-layout-v1, positive finite span, and root/tip fractions 0/1".into());
    }
    let stations = project
        .stations
        .iter()
        .map(|station| {
            let span = station.fraction * (project.span_mm * 0.5);
            PlanformStation {
                span: Quantity::length(span, LengthUnit::Millimeter),
                chord: Quantity::length(station.chord_mm, LengthUnit::Millimeter),
                leading_edge: VectorQuantity::lengths(
                    station.leading_edge_mm,
                    span,
                    station.height_mm,
                    LengthUnit::Millimeter,
                ),
            }
        })
        .collect();
    Ok(SymmetricWingPlanform { stations }.mean_aerodynamic_chord()?)
}

pub fn prepare<'a>(
    document: &'a ModelDocument,
    setup: &Setup,
) -> Result<(InstanceGraph<'a>, Vec<InstanceOutputRef>), Box<dyn Error>> {
    if setup.schema != "occb-balance-report-v1" || !(1..=10_000).contains(&setup.components.len()) {
        return Err("expected occb-balance-report-v1 and 1–10000 selected components".into());
    }
    let sources = document.instance_graph()?;
    let mut graph = sources.clone();
    for material in &setup.materials {
        match document
            .assembly
            .materials
            .iter()
            .find(|existing| existing.id == material.id)
        {
            Some(existing) if existing == material => {}
            Some(_) => {
                return Err(format!(
                    "material '{}' conflicts with the model definition",
                    material.id
                )
                .into());
            }
            None => graph.add_material(material.clone())?,
        }
    }
    let mut outputs = Vec::with_capacity(setup.components.len());
    for component in &setup.components {
        let source = sources
            .node(&component.source)
            .ok_or_else(|| format!("unknown source '{}'", component.source))?;
        if sources.is_suppressed(&component.source) {
            return Err(format!("source '{}' is suppressed", component.source).into());
        }
        graph.add_clone(
            &component.id,
            &component.source,
            HashMap::new(),
            "occt-balance-report",
        )?;
        graph.set_placement(&component.id, source.placement())?;
        graph.set_instance_frame(&component.id, source.frame())?;
        if let Some(material) = &component.material {
            graph.assign_material(&component.id, Some(material))?;
        }
        outputs.push(InstanceOutputRef {
            instance: component.id.clone(),
            output: component.output.clone(),
        });
    }
    Ok((graph, outputs))
}
