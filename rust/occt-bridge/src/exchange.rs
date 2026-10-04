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
        let path = path_to_c_string(path.as_ref())?;
        let name = text_to_c_string(assembly_name, "assembly name")?;
        let mut names = Vec::with_capacity(components.len());
        for component in components {
            self.validate_shape(component.shape)?;
            names.push((
                text_to_c_string(component.name, "component name")?,
                text_to_c_string(component.part_name, "part name")?,
            ));
        }
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
        let mut parts = 0;
        // SAFETY: The strings and component buffer outlive the call, and the
        // output count is writable.
        self.check(unsafe {
            occt_bridge_step_save_assembly(
                self.raw.as_ptr(),
                path.as_ptr(),
                name.as_ptr(),
                raw.as_ptr(),
                raw.len(),
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
