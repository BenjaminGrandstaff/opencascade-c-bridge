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

fn text_to_c_string(text: &str, what: &str) -> Result<CString, BridgeError> {
    CString::new(text).map_err(|_| BridgeError {
        status: 1,
        category: "invalid argument".into(),
        message: format!("{what} contains an interior NUL byte"),
        diagnostics: Vec::new(),
    })
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
        let path = path_to_c_string(path.as_ref())?;
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
        let mut parts = 0;
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
                if memberships.is_empty() {
                    std::ptr::null()
                } else {
                    memberships.as_ptr()
                },
                raw.len(),
                raw_faces.as_ptr(),
                raw_faces.len(),
                &mut parts,
            )
        })?;
        Ok(parts)
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
