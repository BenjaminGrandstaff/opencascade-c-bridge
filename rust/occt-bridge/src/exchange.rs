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

impl Session {
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
