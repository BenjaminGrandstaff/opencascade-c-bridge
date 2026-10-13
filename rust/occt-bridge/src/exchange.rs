//! BREP, STEP, and STL exchange.

use super::*;

pub(crate) fn path_to_c_string(path: &Path) -> Result<CString, BridgeError> {
    CString::new(path.to_string_lossy().as_bytes()).map_err(|_| BridgeError {
        status: 1,
        category: "invalid argument".into(),
        message: "path contains an interior NUL byte".into(),
        diagnostics: Vec::new(),
    })
}

/// A presentation's name and flattened points, which its raw form points
/// into.
struct PresentationBuffers {
    name: CString,
    points: Vec<f64>,
    counts: Vec<u32>,
    origin: [f64; 3],
    normal: [f64; 3],
    x_direction: [f64; 3],
}

impl PresentationBuffers {
    fn new(presentation: &StepPresentation<'_>) -> Result<Self, BridgeError> {
        let xyz = |v: Vec3| [v.x, v.y, v.z];
        let mut counts = Vec::with_capacity(presentation.polylines.len());
        for polyline in presentation.polylines {
            counts.push(u32::try_from(polyline.len()).map_err(|_| BridgeError {
                status: 1,
                category: "invalid argument".into(),
                message: "a presentation polyline has too many points".into(),
                diagnostics: Vec::new(),
            })?);
        }
        Ok(Self {
            name: text_to_c_string(presentation.name, "presentation name")?,
            points: presentation
                .polylines
                .iter()
                .flatten()
                .flat_map(|point| xyz(*point))
                .collect(),
            counts,
            origin: xyz(presentation.origin),
            normal: xyz(presentation.normal),
            x_direction: xyz(presentation.x_direction),
        })
    }

    fn raw(&self) -> RawStepPresentation {
        RawStepPresentation {
            name: self.name.as_ptr(),
            points: self.points.as_ptr(),
            point_count: self.points.len() / 3,
            polyline_points: self.counts.as_ptr(),
            polyline_count: self.counts.len(),
            origin: self.origin,
            normal: self.normal,
            x_direction: self.x_direction,
        }
    }
}

fn text_to_c_string(text: &str, what: &str) -> Result<CString, BridgeError> {
    CString::new(text).map_err(|_| BridgeError {
        status: 1,
        category: "invalid argument".into(),
        message: format!("{what} contains an interior NUL byte"),
        diagnostics: Vec::new(),
    })
}

/// The assembly inputs shared by the tree and PMI exports.
#[derive(Clone, Copy)]
struct StepTree<'a, 'shape, 'session> {
    nodes: &'a [StepNode<'a>],
    components: &'a [StepComponent<'shape, 'session>],
    component_nodes: &'a [Option<usize>],
    face_colors: &'a [StepFaceColor],
}

impl Session {
    /// Writes one named STEP assembly: a named component per entry, placed
    /// at its shape's location, referring to shared named and colored parts.
    /// Returns the number of distinct parts written.
    pub fn save_step_assembly(
        &self,
        path: impl AsRef<Path>,
        assembly_name: &str,
        components: &[StepComponent<'_, '_>],
    ) -> Result<usize, BridgeError> {
        self.save_step_assembly_tree(path, assembly_name, &[], components, &[], &[])
    }

    /// Like [`Self::save_step_assembly`], with nested named sub-assemblies:
    /// component `i` belongs to `component_nodes[i]` (`None` for the top
    /// level; an empty slice puts every component there). Shapes stay placed
    /// in model coordinates and are located relative to their node, so the
    /// model-space geometry matches the flat export. Every node must contain
    /// a component directly or through its descendants. `face_colors` color
    /// individual part faces over the part color.
    pub fn save_step_assembly_tree(
        &self,
        path: impl AsRef<Path>,
        assembly_name: &str,
        nodes: &[StepNode<'_>],
        components: &[StepComponent<'_, '_>],
        component_nodes: &[Option<usize>],
        face_colors: &[StepFaceColor],
    ) -> Result<usize, BridgeError> {
        let tree = StepTree {
            nodes,
            components,
            component_nodes,
            face_colors,
        };
        Ok(self.save_tree(path.as_ref(), assembly_name, &tree, None)?.0)
    }

    /// [`Self::save_step_assembly_tree`] written as AP242 with the semantic
    /// GD&T of `pmi` on part faces, then read back with OCCT's AP242 reader:
    /// returns the number of parts and what each PMI item lost on the way.
    #[allow(clippy::too_many_arguments)]
    pub fn save_step_assembly_pmi(
        &self,
        path: impl AsRef<Path>,
        assembly_name: &str,
        nodes: &[StepNode<'_>],
        components: &[StepComponent<'_, '_>],
        component_nodes: &[Option<usize>],
        face_colors: &[StepFaceColor],
        pmi: &StepPmi<'_>,
    ) -> Result<(usize, StepPmiVerification), BridgeError> {
        let tree = StepTree {
            nodes,
            components,
            component_nodes,
            face_colors,
        };
        let (parts, verification) =
            self.save_tree(path.as_ref(), assembly_name, &tree, Some(pmi))?;
        Ok((parts, verification.unwrap_or_default()))
    }

    fn save_tree(
        &self,
        path: &Path,
        assembly_name: &str,
        tree: &StepTree<'_, '_, '_>,
        pmi: Option<&StepPmi<'_>>,
    ) -> Result<(usize, Option<StepPmiVerification>), BridgeError> {
        let StepTree {
            nodes,
            components,
            component_nodes,
            face_colors,
        } = *tree;
        let path = path_to_c_string(path)?;
        let name = text_to_c_string(assembly_name, "assembly name")?;
        if !component_nodes.is_empty() && component_nodes.len() != components.len() {
            return Err(BridgeError {
                status: 1,
                category: "invalid argument".into(),
                message: "component_nodes must be empty or match the components".into(),
                diagnostics: Vec::new(),
            });
        }
        let mut names = Vec::with_capacity(components.len());
        for component in components {
            self.validate_shape(component.shape)?;
            names.push((
                text_to_c_string(component.name, "component name")?,
                text_to_c_string(component.part_name, "part name")?,
            ));
        }
        let node_names = nodes
            .iter()
            .map(|node| text_to_c_string(node.name, "sub-assembly name"))
            .collect::<Result<Vec<_>, _>>()?;
        let raw_nodes = nodes
            .iter()
            .zip(&node_names)
            .map(|(node, name)| RawStepNode {
                name: name.as_ptr(),
                parent: node.parent.unwrap_or(usize::MAX),
                transform: node.transform,
            })
            .collect::<Vec<_>>();
        let raw = components
            .iter()
            .zip(&names)
            .map(|(component, (name, part))| RawStepComponent {
                shape: component.shape.id,
                name: name.as_ptr(),
                part_name: part.as_ptr(),
                has_color: i32::from(component.color.is_some()),
                color: component.color.unwrap_or([0.0; 3]),
            })
            .collect::<Vec<_>>();
        let memberships = component_nodes
            .iter()
            .map(|node| node.unwrap_or(usize::MAX))
            .collect::<Vec<_>>();
        let raw_faces = face_colors
            .iter()
            .map(|entry| RawStepFaceColor {
                component: entry.component,
                face: entry.face,
                color: entry.color,
            })
            .collect::<Vec<_>>();
        let memberships = if memberships.is_empty() {
            std::ptr::null()
        } else {
            memberships.as_ptr()
        };
        let mut parts = 0;
        let Some(pmi) = pmi else {
            // SAFETY: The strings and the node, component, membership, and face buffers
            // outlive the call, and the output count is writable.
            self.check(unsafe {
                occt_bridge_step_save_assembly_tree(
                    self.raw.as_ptr(),
                    path.as_ptr(),
                    name.as_ptr(),
                    raw_nodes.as_ptr(),
                    raw_nodes.len(),
                    raw.as_ptr(),
                    memberships,
                    raw.len(),
                    raw_faces.as_ptr(),
                    raw_faces.len(),
                    &mut parts,
                )
            })?;
            return Ok((parts, None));
        };
        let targets = |target: &StepPmiFaces<'_>| RawStepPmiFaces {
            component: target.component,
            faces: target.faces.as_ptr(),
            face_count: target.faces.len(),
        };
        let labels = pmi
            .datums
            .iter()
            .map(|datum| text_to_c_string(datum.label, "datum label"))
            .collect::<Result<Vec<_>, _>>()?;
        // Presentation buffers first, then the raw presentations pointing
        // into them, then the items pointing at those.
        let buffers = |presentation: &Option<StepPresentation<'_>>| {
            presentation
                .as_ref()
                .map(PresentationBuffers::new)
                .transpose()
        };
        let datum_buffers = pmi
            .datums
            .iter()
            .map(|datum| buffers(&datum.presentation))
            .collect::<Result<Vec<_>, _>>()?;
        let tolerance_buffers = pmi
            .tolerances
            .iter()
            .map(|tolerance| buffers(&tolerance.presentation))
            .collect::<Result<Vec<_>, _>>()?;
        let raw_presentations = |buffers: &[Option<PresentationBuffers>]| {
            buffers
                .iter()
                .map(|buffers| buffers.as_ref().map(PresentationBuffers::raw))
                .collect::<Vec<_>>()
        };
        let datum_presentations = raw_presentations(&datum_buffers);
        let tolerance_presentations = raw_presentations(&tolerance_buffers);
        let pointer = |raw: &Option<RawStepPresentation>| {
            raw.as_ref()
                .map_or(std::ptr::null(), |raw| raw as *const RawStepPresentation)
        };
        let datums = pmi
            .datums
            .iter()
            .zip(&labels)
            .zip(&datum_presentations)
            .map(|((datum, label), presentation)| RawStepDatum {
                label: label.as_ptr(),
                target: targets(&datum.target),
                presentation: pointer(presentation),
            })
            .collect::<Vec<_>>();
        let material = |material: StepMaterial| match material {
            StepMaterial::Regardless => 0,
            StepMaterial::Maximum => 1,
            StepMaterial::Least => 2,
        };
        let references = pmi
            .tolerances
            .iter()
            .map(|tolerance| {
                tolerance
                    .datums
                    .iter()
                    .map(|reference| RawStepDatumReference {
                        datum: reference.datum,
                        compartment: reference.compartment,
                        boundary: material(reference.boundary),
                        translation: i32::from(reference.translation),
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let tolerances = pmi
            .tolerances
            .iter()
            .zip(&references)
            .zip(&tolerance_presentations)
            .map(|((tolerance, references), presentation)| RawStepTolerance {
                target: targets(&tolerance.target),
                characteristic: tolerance.characteristic as i32,
                value_mm: tolerance.value_mm,
                diameter: i32::from(tolerance.diameter),
                material: material(tolerance.material),
                modifiers: u32::from(tolerance.modifiers.free_state)
                    | u32::from(tolerance.modifiers.statistical) << 1
                    | u32::from(tolerance.modifiers.tangent_plane) << 2,
                projected_mm: tolerance.projected_mm.unwrap_or(0.0),
                datums: references.as_ptr(),
                datum_count: references.len(),
                presentation: pointer(presentation),
            })
            .collect::<Vec<_>>();
        let sizes = pmi
            .sizes
            .iter()
            .map(|size| RawStepSize {
                target: targets(&size.target),
                kind: match size.kind {
                    StepSizeKind::Diameter => 0,
                    StepSizeKind::Width => 1,
                },
                lower_mm: size.lower_mm,
                upper_mm: size.upper_mm,
            })
            .collect::<Vec<_>>();
        let mut datum_losses = vec![0u32; datums.len()];
        let mut tolerance_losses = vec![0u32; tolerances.len()];
        let mut size_losses = vec![0u32; sizes.len()];
        let view_names = pmi
            .views
            .iter()
            .map(|view| text_to_c_string(view.name, "view name"))
            .collect::<Result<Vec<_>, _>>()?;
        let mut view_datums_lost = pmi
            .views
            .iter()
            .map(|view| vec![0u8; view.datums.len()])
            .collect::<Vec<_>>();
        let mut view_tolerances_lost = pmi
            .views
            .iter()
            .map(|view| vec![0u8; view.tolerances.len()])
            .collect::<Vec<_>>();
        let view_planes = pmi
            .views
            .iter()
            .map(|view| {
                view.clipping_planes
                    .iter()
                    .map(|plane| RawStepClippingPlane {
                        origin: [plane.origin.x, plane.origin.y, plane.origin.z],
                        normal: [plane.normal.x, plane.normal.y, plane.normal.z],
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let views = pmi
            .views
            .iter()
            .zip(view_names.iter().zip(&view_planes))
            .zip(view_datums_lost.iter_mut().zip(&mut view_tolerances_lost))
            .map(
                |((view, (name, planes)), (datums_lost, tolerances_lost))| RawStepView {
                    name: name.as_ptr(),
                    direction: [view.direction.x, view.direction.y, view.direction.z],
                    up: [view.up.x, view.up.y, view.up.z],
                    datums: view.datums.as_ptr(),
                    datum_count: view.datums.len(),
                    tolerances: view.tolerances.as_ptr(),
                    tolerance_count: view.tolerances.len(),
                    out_datums_lost: datums_lost.as_mut_ptr(),
                    out_tolerances_lost: tolerances_lost.as_mut_ptr(),
                    clipping_planes: planes.as_ptr(),
                    clipping_plane_count: planes.len(),
                },
            )
            .collect::<Vec<_>>();
        let mut view_losses = vec![0u32; views.len()];
        let mut raw_pmi = RawStepPmi {
            datums: datums.as_ptr(),
            datum_count: datums.len(),
            tolerances: tolerances.as_ptr(),
            tolerance_count: tolerances.len(),
            sizes: sizes.as_ptr(),
            size_count: sizes.len(),
            out_datum_losses: datum_losses.as_mut_ptr(),
            out_tolerance_losses: tolerance_losses.as_mut_ptr(),
            out_size_losses: size_losses.as_mut_ptr(),
            views: views.as_ptr(),
            view_count: views.len(),
            out_view_losses: view_losses.as_mut_ptr(),
        };
        // SAFETY: The strings, the node, component, membership, face, PMI,
        // presentation and view buffers outlive the call; the loss buffers
        // hold one entry per item, view or view index, and the output count
        // is writable.
        self.check(unsafe {
            occt_bridge_step_save_assembly_pmi(
                self.raw.as_ptr(),
                path.as_ptr(),
                name.as_ptr(),
                raw_nodes.as_ptr(),
                raw_nodes.len(),
                raw.as_ptr(),
                memberships,
                raw.len(),
                raw_faces.as_ptr(),
                raw_faces.len(),
                &mut raw_pmi,
                &mut parts,
            )
        })?;
        let losses = |bits: Vec<u32>| {
            bits.into_iter()
                .map(|bits| StepPmiLoss {
                    item: bits & 1 != 0,
                    value: bits & 2 != 0,
                    zone: bits & 4 != 0,
                    material: bits & 8 != 0,
                    modifiers: bits & 16 != 0,
                    projected: bits & 32 != 0,
                    datums: bits & 64 != 0,
                    presentation: bits & 128 != 0,
                })
                .collect()
        };
        drop(views);
        let views = view_losses
            .into_iter()
            .zip(view_datums_lost.into_iter().zip(view_tolerances_lost))
            .map(|(bits, (datums, tolerances))| StepViewLoss {
                view: bits & 1 != 0,
                camera: bits & 2 != 0,
                clipping: bits & 4 != 0,
                datums: datums.into_iter().map(|lost| lost != 0).collect(),
                tolerances: tolerances.into_iter().map(|lost| lost != 0).collect(),
            })
            .collect();
        Ok((
            parts,
            Some(StepPmiVerification {
                datums: losses(datum_losses),
                tolerances: losses(tolerance_losses),
                sizes: losses(size_losses),
                views,
            }),
        ))
    }

    pub fn save_brep(&self, shape: &Shape<'_>, path: impl AsRef<Path>) -> Result<(), BridgeError> {
        self.validate_shape(shape)?;
        let path = path_to_c_string(path.as_ref())?;
        // SAFETY: The session and NUL-terminated path remain valid for the call.
        self.check(unsafe { occt_bridge_brep_save(self.raw.as_ptr(), shape.id, path.as_ptr()) })
    }

    pub fn load_brep(&self, path: impl AsRef<Path>) -> Result<Shape<'_>, BridgeError> {
        let path = path_to_c_string(path.as_ref())?;
        let mut shape = 0;
        // SAFETY: The session, NUL-terminated path, and output pointer are valid.
        self.check(unsafe { occt_bridge_brep_load(self.raw.as_ptr(), path.as_ptr(), &mut shape) })?;
        Ok(self.shape(shape))
    }

    /// Exports geometry and topology to a STEP file. Session handles,
    /// operation history, and application metadata are not serialized.
    pub fn save_step(&self, shape: &Shape<'_>, path: impl AsRef<Path>) -> Result<(), BridgeError> {
        self.validate_shape(shape)?;
        let path = path_to_c_string(path.as_ref())?;
        // SAFETY: The session and NUL-terminated path remain valid for the call.
        self.check(unsafe { occt_bridge_step_save(self.raw.as_ptr(), shape.id, path.as_ptr()) })
    }

    pub fn load_step(&self, path: impl AsRef<Path>) -> Result<Shape<'_>, BridgeError> {
        let path = path_to_c_string(path.as_ref())?;
        let mut shape = 0;
        // SAFETY: The session, NUL-terminated path, and output pointer are valid.
        self.check(unsafe { occt_bridge_step_load(self.raw.as_ptr(), path.as_ptr(), &mut shape) })?;
        Ok(self.shape(shape))
    }

    /// Tessellates a shape and exports it as STL. STL contains triangles only;
    /// it does not preserve exact CAD geometry, topology, or operation history.
    pub fn save_stl(
        &self,
        shape: &Shape<'_>,
        path: impl AsRef<Path>,
        options: StlOptions,
    ) -> Result<(), BridgeError> {
        self.validate_shape(shape)?;
        let path = path_to_c_string(path.as_ref())?;
        let binary = i32::from(options.format == StlFormat::Binary);
        // SAFETY: The session and NUL-terminated path remain valid for the call.
        self.check(unsafe {
            occt_bridge_stl_save(
                self.raw.as_ptr(),
                shape.id,
                path.as_ptr(),
                options.linear_deflection,
                options.angular_deflection_radians,
                binary,
            )
        })
    }
}
