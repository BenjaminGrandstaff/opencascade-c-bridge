//! STEP AP242 export of drawing GD&T as semantic PMI on the faces its
//! attachments name (schema 144, ABI 70), with the drawing's annotation
//! views as saved views of 3D presentations (schema 145, ABI 71), verified
//! by reading the file back.
use super::*;
use crate::assembly::InstanceGraphStepWrite;
use crate::selection::{Features, resolve_face_selector};
use occt_bridge::{
    StepApproval, StepCharacteristic, StepClippingPlane, StepDataset, StepDatum,
    StepDatumReference, StepMaterial, StepPmi, StepPmiFaces, StepPmiLoss, StepPresentation,
    StepSize, StepSizeKind, StepTolerance, StepToleranceModifiers, StepView,
};
use std::path::Path;

pub trait DrawingStepPmi {
    /// Writes `outputs` as [`InstanceGraphStepExport::export_step`] does, in
    /// AP242 with this drawing's GD&T as semantic PMI: each datum feature,
    /// tolerance (one per composite segment; one per pattern on an output)
    /// and size limit whose attachment names `faces` on an exported output
    /// is attached to those faces of its part. The file is then read back
    /// and the report lists, per item, what the format cannot represent and
    /// what did not read back, and which annotations were skipped and why.
    ///
    /// Each annotation view becomes a saved view: a camera along its
    /// direction framing the export, showing the 3D presentations of its
    /// annotations, which lie in the plane of the first view that shows
    /// them. The report says per view what did not read back.
    ///
    /// Faces are selected on one local regeneration per annotated instance,
    /// where its selectors are defined, and carried to the placed output by
    /// face index. Time: the STEP export plus one regeneration per annotated
    /// instance, O(items + faces) labels and one STEP read.
    #[allow(clippy::too_many_arguments)]
    fn export_step_pmi(
        &self,
        graph: &InstanceGraph<'_>,
        session: &Session,
        generation: &GraphRegeneration<'_>,
        path: impl AsRef<Path>,
        assembly_name: &str,
        outputs: &OutputSet,
    ) -> Result<PmiExchangeReport, ModelError>;
}

/// A target's face indices on its output, and whether it is exactly two
/// planar faces (a width rather than a diameter).
struct Resolved {
    component: usize,
    faces: Vec<usize>,
    width: bool,
}

fn faces(resolved: &Resolved) -> StepPmiFaces<'_> {
    StepPmiFaces {
        component: resolved.component,
        faces: &resolved.faces,
    }
}

fn characteristic(characteristic: GeometricCharacteristic) -> StepCharacteristic {
    match characteristic {
        GeometricCharacteristic::Straightness => StepCharacteristic::Straightness,
        GeometricCharacteristic::Flatness => StepCharacteristic::Flatness,
        GeometricCharacteristic::Circularity => StepCharacteristic::Circularity,
        GeometricCharacteristic::Cylindricity => StepCharacteristic::Cylindricity,
        GeometricCharacteristic::ProfileLine => StepCharacteristic::ProfileLine,
        GeometricCharacteristic::ProfileSurface => StepCharacteristic::ProfileSurface,
        GeometricCharacteristic::Parallelism => StepCharacteristic::Parallelism,
        GeometricCharacteristic::Perpendicularity => StepCharacteristic::Perpendicularity,
        GeometricCharacteristic::Angularity => StepCharacteristic::Angularity,
        GeometricCharacteristic::Position => StepCharacteristic::Position,
        GeometricCharacteristic::CircularRunout => StepCharacteristic::CircularRunout,
        GeometricCharacteristic::TotalRunout => StepCharacteristic::TotalRunout,
    }
}

fn boundary(boundary: DatumMaterialBoundary) -> StepMaterial {
    match boundary {
        DatumMaterialBoundary::Regardless => StepMaterial::Regardless,
        DatumMaterialBoundary::Maximum => StepMaterial::Maximum,
        DatumMaterialBoundary::Least => StepMaterial::Least,
    }
}

fn material(material: ToleranceMaterialCondition) -> StepMaterial {
    match material {
        ToleranceMaterialCondition::Regardless => StepMaterial::Regardless,
        ToleranceMaterialCondition::Maximum => StepMaterial::Maximum,
        ToleranceMaterialCondition::Least => StepMaterial::Least,
    }
}

fn presentation(presentation: &Option<PmiPresentation>) -> Option<StepPresentation<'_>> {
    presentation.as_ref().map(|p| StepPresentation {
        name: &p.name,
        polylines: &p.polylines,
        origin: p.origin,
        normal: p.normal,
        x_direction: p.x_direction,
    })
}

fn lost(loss: &StepPmiLoss) -> Vec<PmiProperty> {
    [
        (loss.item, PmiProperty::Item),
        (loss.value, PmiProperty::Value),
        (loss.zone, PmiProperty::Zone),
        (loss.material, PmiProperty::Material),
        (loss.modifiers, PmiProperty::Modifiers),
        (loss.projected, PmiProperty::ProjectedZone),
        (loss.datums, PmiProperty::Datums),
        (loss.presentation, PmiProperty::Presentation),
    ]
    .into_iter()
    .filter_map(|(lost, property)| lost.then_some(property))
    .collect()
}

/// One annotated instance regenerated locally, where its selectors are
/// defined, with one selection context so every selector on it shares face
/// measurements.
struct Local<'definition, 'session> {
    generated: GeneratedResult<'session>,
    parameters: HashMap<String, ParameterValue>,
    features: Features<'definition>,
}

/// Local regenerations of annotated instances, made on first use, and the
/// targets already resolved on them.
struct Parts<'graph, 'definition, 'session> {
    graph: &'graph InstanceGraph<'definition>,
    session: &'session Session,
    generation: &'graph GraphRegeneration<'session>,
    parts: HashMap<String, Local<'definition, 'session>>,
    checked: HashSet<InstanceOutputRef>,
    /// By output and selector identities: a tolerance and its size limits,
    /// or a datum feature of size, share one resolution.
    resolved: HashMap<(InstanceOutputRef, Vec<usize>), std::rc::Rc<Resolved>>,
}

impl<'session> Parts<'_, '_, 'session> {
    fn resolve(
        &mut self,
        target: &PmiTarget<'_>,
        component: usize,
        id: &str,
    ) -> Result<std::rc::Rc<Resolved>, ModelError> {
        let output = target.output;
        let key = (
            output.clone(),
            target
                .faces
                .iter()
                .map(|selector| std::ptr::from_ref(*selector) as usize)
                .collect(),
        );
        if let Some(resolved) = self.resolved.get(&key) {
            return Ok(resolved.clone());
        }
        if !self.parts.contains_key(&output.instance) {
            let instance = self.graph.resolve(&output.instance)?;
            let local = Local {
                generated: instance.regenerate(self.session)?,
                parameters: instance.resolved_parameters()?,
                features: Features::new(instance.definition),
            };
            self.parts.insert(output.instance.clone(), local);
        }
        let part = &self.parts[&output.instance];
        let local = part.generated.shape(&output.output).ok_or_else(|| {
            ModelError::new(format!("unknown generated output '{}'", output.output))
        })?;
        // Placement keeps topology, and so the face order, of the local result.
        if self.checked.insert(output.clone()) {
            let placed = self
                .generation
                .result(&output.instance)
                .and_then(|result| result.shape(&output.output))
                .ok_or_else(|| {
                    ModelError::new(format!(
                        "missing generated output '{}:{}'",
                        output.instance, output.output
                    ))
                })?;
            if self.session.subshape_count(local, ShapeType::Face)?
                != self.session.subshape_count(placed, ShapeType::Face)?
            {
                return Err(ModelError::new(format!(
                    "placed output '{}:{}' does not keep its local faces",
                    output.instance, output.output
                )));
            }
        }
        let mut faces = Vec::new();
        let mut planar = true;
        for selector in &target.faces {
            let selected = resolve_face_selector(
                self.session,
                local,
                selector,
                &part.parameters,
                &part.generated.shapes,
                &part.features,
            )
            .map_err(|e| ModelError::new(format!("faces of '{id}': {}", e.message)))?;
            for face in &selected {
                planar &= self.session.face_is_planar(face)?;
            }
            let references = selected.iter().collect::<Vec<_>>();
            for index in self
                .session
                .subshape_lookup(local, ShapeType::Face, &references)?
            {
                faces.push(index.ok_or_else(|| {
                    ModelError::new(format!("faces of '{id}' are not faces of its output"))
                })?);
            }
        }
        faces.sort_unstable();
        faces.dedup();
        if faces.is_empty() {
            return Err(ModelError::new(format!("faces of '{id}' select no faces")));
        }
        let resolved = std::rc::Rc::new(Resolved {
            component,
            width: planar && faces.len() == 2,
            faces,
        });
        self.resolved.insert(key, resolved.clone());
        Ok(resolved)
    }
}

impl DrawingStepPmi for DrawingDefinition {
    fn export_step_pmi(
        &self,
        graph: &InstanceGraph<'_>,
        session: &Session,
        generation: &GraphRegeneration<'_>,
        path: impl AsRef<Path>,
        assembly_name: &str,
        outputs: &OutputSet,
    ) -> Result<PmiExchangeReport, ModelError> {
        let outputs = graph.generated_outputs(generation, outputs)?;
        let components: HashMap<&InstanceOutputRef, usize> = outputs
            .iter()
            .enumerate()
            .map(|(index, output)| (output, index))
            .collect();
        let plan = self.pmi_plan(graph, |output| components.contains_key(output))?;
        let mut parts = Parts {
            graph,
            session,
            generation,
            parts: HashMap::new(),
            checked: HashSet::new(),
            resolved: HashMap::new(),
        };
        let mut resolve =
            |target: &PmiTarget<'_>, id: &str| parts.resolve(target, components[target.output], id);
        let datums = plan
            .datums
            .iter()
            .map(|datum| resolve(&datum.target, datum.id))
            .collect::<Result<Vec<_>, _>>()?;
        let tolerances = plan
            .tolerances
            .iter()
            .map(|tolerance| resolve(&tolerance.target, tolerance.id))
            .collect::<Result<Vec<_>, _>>()?;
        // A datum feature of size and its own position control may give the
        // same limits on the same faces; one size dimension states them.
        let mut sizes = Vec::new();
        let mut seen = HashSet::new();
        for size in &plan.sizes {
            let resolved = resolve(&size.target, size.id)?;
            let key = (
                resolved.component,
                resolved.faces.clone(),
                size.lower_mm.to_bits(),
                size.upper_mm.to_bits(),
            );
            if seen.insert(key) {
                sizes.push((size, resolved));
            }
        }
        let references = plan
            .tolerances
            .iter()
            .map(|tolerance| {
                tolerance
                    .datums
                    .iter()
                    .map(|reference| StepDatumReference {
                        datum: reference.datum,
                        compartment: reference.compartment,
                        boundary: boundary(reference.boundary),
                        translation: reference.translation,
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();

        let step_datums = plan
            .datums
            .iter()
            .zip(&datums)
            .map(|(datum, resolved)| StepDatum {
                label: datum.label,
                target: faces(resolved),
                presentation: presentation(&datum.presentation),
            })
            .collect::<Vec<_>>();
        let step_tolerances = plan
            .tolerances
            .iter()
            .zip(&tolerances)
            .zip(&references)
            .map(|((tolerance, resolved), references)| StepTolerance {
                target: faces(resolved),
                characteristic: characteristic(tolerance.characteristic),
                value_mm: tolerance.value_mm,
                diameter: tolerance.diameter,
                material: material(tolerance.material),
                modifiers: StepToleranceModifiers {
                    free_state: tolerance.free_state,
                    statistical: tolerance.statistical,
                    tangent_plane: tolerance.tangent_plane,
                },
                projected_mm: tolerance.projected_mm,
                datums: references,
                presentation: presentation(&tolerance.presentation),
            })
            .collect::<Vec<_>>();
        let step_sizes = sizes
            .iter()
            .map(|(size, resolved)| StepSize {
                target: faces(resolved),
                kind: if resolved.width {
                    StepSizeKind::Width
                } else {
                    StepSizeKind::Diameter
                },
                lower_mm: size.lower_mm,
                upper_mm: size.upper_mm,
            })
            .collect::<Vec<_>>();
        let planes = plan
            .views
            .iter()
            .map(|view| {
                view.clipping_planes
                    .iter()
                    .map(|&(origin, normal)| StepClippingPlane { origin, normal })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        // STEP cameras look along the view, away from the viewer.
        let step_views = plan
            .views
            .iter()
            .zip(&planes)
            .map(|(view, planes)| StepView {
                name: view.name,
                direction: Vec3::new(
                    -view.axes.direction.x,
                    -view.axes.direction.y,
                    -view.axes.direction.z,
                ),
                up: view.axes.up,
                datums: &view.datums,
                tolerances: &view.tolerances,
                clipping_planes: planes,
            })
            .collect::<Vec<_>>();
        let approvals = plan
            .dataset
            .as_ref()
            .and_then(|planned| planned.release)
            .map_or_else(Vec::new, |release| {
                release
                    .approvals
                    .iter()
                    .map(|approval| StepApproval {
                        role: &approval.role,
                        name: &approval.name,
                        date: &approval.date,
                    })
                    .collect()
            });
        let pmi = StepPmi {
            datums: &step_datums,
            tolerances: &step_tolerances,
            sizes: &step_sizes,
            views: &step_views,
            dataset: plan.dataset.as_ref().map(|planned| StepDataset {
                identifier: &planned.dataset.identifier,
                organization: &planned.dataset.organization,
                authority: planned.dataset.authority.statement(),
                revision: planned.release.map(|release| release.revision.as_str()),
                description: planned.release.map(|release| release.description.as_str()),
                approvals: &approvals,
            }),
        };
        let (parts, verification) = graph.write_step(
            session,
            generation,
            path.as_ref(),
            assembly_name,
            &outputs,
            Some(&pmi),
        )?;
        let item = |id: &str,
                    kind: PmiItemKind,
                    segment: usize,
                    resolved: &Resolved,
                    unrepresented: Vec<PmiProperty>,
                    loss: &StepPmiLoss| PmiExchangeItem {
            id: id.to_owned(),
            kind,
            segment,
            instance: outputs[resolved.component].instance.clone(),
            faces: resolved.faces.len(),
            unrepresented,
            lost: lost(loss),
        };
        let mut items = Vec::with_capacity(datums.len() + tolerances.len() + sizes.len());
        for ((datum, resolved), loss) in plan.datums.iter().zip(&datums).zip(&verification.datums) {
            items.push(item(
                datum.id,
                PmiItemKind::Datum,
                0,
                resolved,
                Vec::new(),
                loss,
            ));
        }
        for ((tolerance, resolved), loss) in plan
            .tolerances
            .iter()
            .zip(&tolerances)
            .zip(&verification.tolerances)
        {
            items.push(item(
                tolerance.id,
                PmiItemKind::Tolerance,
                tolerance.segment,
                resolved,
                tolerance.unrepresented.clone(),
                loss,
            ));
        }
        for ((size, resolved), loss) in sizes.iter().zip(&verification.sizes) {
            items.push(item(
                size.id,
                PmiItemKind::Size,
                0,
                resolved,
                Vec::new(),
                loss,
            ));
        }
        let views = plan
            .views
            .iter()
            .zip(&verification.views)
            .map(|(view, loss)| {
                let mut lost_annotations: Vec<String> = Vec::new();
                let lost_ids = view
                    .datums
                    .iter()
                    .zip(&loss.datums)
                    .filter(|(_, lost)| **lost)
                    .map(|(&datum, _)| plan.datums[datum].id)
                    .chain(
                        view.tolerances
                            .iter()
                            .zip(&loss.tolerances)
                            .filter(|(_, lost)| **lost)
                            .map(|(&tolerance, _)| plan.tolerances[tolerance].id),
                    );
                for id in lost_ids {
                    if !lost_annotations.iter().any(|known| known == id) {
                        lost_annotations.push(id.to_owned());
                    }
                }
                PmiViewReport {
                    id: view.id.to_owned(),
                    name: view.name.to_owned(),
                    annotations: view.annotations.iter().map(|id| (*id).to_owned()).collect(),
                    omitted: view.omitted.iter().map(|id| (*id).to_owned()).collect(),
                    lost: if loss.view {
                        Some(PmiViewLoss::View)
                    } else if loss.camera {
                        Some(PmiViewLoss::Camera)
                    } else if loss.clipping {
                        Some(PmiViewLoss::Clipping)
                    } else {
                        None
                    },
                    lost_annotations,
                }
            })
            .collect();
        let dataset = plan
            .dataset
            .as_ref()
            .zip(verification.dataset.as_ref())
            .map(|(planned, loss)| PmiDatasetReport {
                identifier: planned.dataset.identifier.clone(),
                authority: planned.dataset.authority,
                revision: planned.release.map(|release| release.revision.clone()),
                lost: [
                    (loss.identifier, PmiDatasetProperty::Identifier),
                    (loss.revision, PmiDatasetProperty::Revision),
                    (loss.authority, PmiDatasetProperty::Authority),
                    (loss.approvals, PmiDatasetProperty::Approvals),
                ]
                .into_iter()
                .filter_map(|(lost, property)| lost.then_some(property))
                .collect(),
            });
        Ok(PmiExchangeReport {
            parts,
            items,
            skipped: plan.skipped,
            views,
            dataset,
        })
    }
}
