//! Sessions: lifecycle, status checking, diagnostics, warnings, and options.

use super::*;

pub struct Session {
    pub(crate) raw: NonNull<c_void>,
    pub(crate) generation: Cell<u64>,
}

impl Drop for Session {
    fn drop(&mut self) {
        // SAFETY: This is the unique owning Session and Drop runs once.
        unsafe { occt_bridge_session_destroy(self.raw.as_ptr()) };
    }
}

impl Session {
    pub fn new() -> Result<Self, BridgeError> {
        // SAFETY: This function has no preconditions and returns a constant.
        let actual_version = unsafe { occt_bridge_abi_version() };
        if actual_version != ABI_VERSION {
            return Err(BridgeError {
                status: 2,
                category: "unsupported ABI version".into(),
                message: format!("Rust expects {ABI_VERSION}, library provides {actual_version}"),
                diagnostics: Vec::new(),
            });
        }
        let mut raw = ptr::null_mut();
        // SAFETY: `raw` is a valid output pointer and the ABI version was checked.
        let status = unsafe { occt_bridge_session_create(ABI_VERSION, &mut raw) };
        if status != OK {
            return Err(Self::error_without_session(status));
        }
        let raw = NonNull::new(raw).ok_or_else(|| BridgeError {
            status: 8,
            category: "internal error".into(),
            message: "library returned a null session".into(),
            diagnostics: Vec::new(),
        })?;
        Ok(Self {
            raw,
            generation: Cell::new(0),
        })
    }

    pub fn clear(&self) -> Result<(), BridgeError> {
        // SAFETY: `self.raw` remains valid until Drop.
        self.check(unsafe { occt_bridge_session_clear(self.raw.as_ptr()) })?;
        self.generation.set(self.generation.get().wrapping_add(1));
        Ok(())
    }

    pub fn shape_count(&self) -> Result<usize, BridgeError> {
        let mut count = 0;
        // SAFETY: Both pointers are valid for the duration of the call.
        self.check(unsafe { occt_bridge_session_shape_count(self.raw.as_ptr(), &mut count) })?;
        Ok(count)
    }

    /// Removes a shape from this session and consumes its Rust handle.
    ///
    /// A removed shape cannot subsequently be used in safe Rust:
    ///
    /// ```compile_fail
    /// use occt_bridge::{Session, Vec3};
    ///
    /// let session = Session::new().unwrap();
    /// let shape = session
    ///     .create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 1.0, 1.0))
    ///     .unwrap();
    /// session.remove(shape).unwrap();
    /// session.is_valid(&shape).unwrap();
    /// ```
    /// Releases a shape now and reports failures; dropping the handle does
    /// the same silently. A handle rejected because it belongs to another
    /// session is still consumed and released in its own session.
    pub fn remove(&self, shape: Shape<'_>) -> Result<(), BridgeError> {
        self.validate_shape(&shape)?;
        // SAFETY: The session pointer is valid; the C layer validates the handle.
        let status = unsafe { occt_bridge_shape_remove(self.raw.as_ptr(), shape.id) };
        // The handle is gone; skip the release its Drop would perform.
        std::mem::forget(shape);
        self.check(status)
    }

    pub(crate) fn shape(&self, id: RawShapeId) -> Shape<'_> {
        Shape {
            id,
            owner: self.raw,
            generation: self.generation.get(),
            _session: PhantomData,
        }
    }

    pub(crate) fn validate_shape(&self, shape: &Shape<'_>) -> Result<(), BridgeError> {
        if shape.owner != self.raw {
            return Err(BridgeError {
                status: 1,
                category: "invalid argument".into(),
                message: "shape belongs to a different session".into(),
                diagnostics: Vec::new(),
            });
        }
        if shape.generation != self.generation.get() {
            return Err(BridgeError {
                status: 3,
                category: "shape not found".into(),
                message: "shape was invalidated by clearing its session".into(),
                diagnostics: Vec::new(),
            });
        }
        Ok(())
    }

    pub(crate) fn check(&self, status: RawStatus) -> Result<(), BridgeError> {
        if status == OK {
            Ok(())
        } else {
            Err(self.error(status))
        }
    }

    pub(crate) fn error(&self, status: RawStatus) -> BridgeError {
        let mut error = Self::error_without_session(status);
        error.message = self.read_text(occt_bridge_session_last_error);
        error.diagnostics = self.last_diagnostics();
        error
    }

    /// Diagnostics of the most recent call; failed calls also carry them in
    /// [`BridgeError::diagnostics`].
    pub fn last_diagnostics(&self) -> Vec<Diagnostic> {
        let mut count = 0;
        // SAFETY: The session and output pointers are valid.
        if unsafe { occt_bridge_session_diagnostic_count(self.raw.as_ptr(), &mut count) } != OK {
            return Vec::new();
        }
        (0..count)
            .filter_map(|index| {
                let mut raw = RawDiagnostic::default();
                // SAFETY: A null shape output asks for no handle.
                let status = unsafe {
                    occt_bridge_session_diagnostic_at(
                        self.raw.as_ptr(),
                        index,
                        &mut raw,
                        ptr::null_mut(),
                    )
                };
                (status == OK).then(|| Diagnostic {
                    kind: DiagnosticKind::from_raw(raw.kind),
                    code: raw.code,
                    name: self.read_text_at(index, occt_bridge_session_diagnostic_name),
                    input_index: usize::try_from(raw.input_index).ok(),
                    has_shape: raw.has_shape != 0,
                })
            })
            .collect()
    }

    /// The subshape the most recent call's diagnostic `index` names, as a
    /// new handle; `None` when it names none. It may belong to an input or
    /// to a rejected result. Reading it keeps the diagnostics, but any other
    /// call clears them.
    pub fn last_diagnostic_shape(&self, index: usize) -> Result<Option<Shape<'_>>, BridgeError> {
        let mut raw = RawDiagnostic::default();
        let mut shape = 0;
        // SAFETY: The session and output pointers are valid.
        self.check(unsafe {
            occt_bridge_session_diagnostic_at(self.raw.as_ptr(), index, &mut raw, &mut shape)
        })?;
        Ok((shape != 0).then(|| self.shape(shape)))
    }

    /// Reads an indexed session text buffer through the size-query protocol.
    pub(crate) fn read_text_at(
        &self,
        index: usize,
        read: unsafe extern "C" fn(*const c_void, usize, *mut c_char, usize) -> usize,
    ) -> String {
        // SAFETY: A null buffer with zero capacity is the documented size query.
        let required = unsafe { read(self.raw.as_ptr(), index, ptr::null_mut(), 0) };
        if required <= 1 {
            return String::new();
        }
        let mut buffer = vec![0u8; required];
        // SAFETY: `buffer` has exactly the capacity reported by the library.
        unsafe {
            read(
                self.raw.as_ptr(),
                index,
                buffer.as_mut_ptr().cast(),
                buffer.len(),
            );
            CStr::from_ptr(buffer.as_ptr().cast())
                .to_string_lossy()
                .into_owned()
        }
    }

    /// Reads a session text buffer through the library's size-query protocol.
    pub(crate) fn read_text(
        &self,
        read: unsafe extern "C" fn(*const c_void, *mut c_char, usize) -> usize,
    ) -> String {
        // SAFETY: A null buffer with zero capacity is the documented size query.
        let required = unsafe { read(self.raw.as_ptr(), ptr::null_mut(), 0) };
        if required <= 1 {
            return String::new();
        }
        let mut buffer = vec![0u8; required];
        // SAFETY: `buffer` has exactly the capacity reported by the library.
        unsafe {
            read(self.raw.as_ptr(), buffer.as_mut_ptr().cast(), buffer.len());
            CStr::from_ptr(buffer.as_ptr().cast())
                .to_string_lossy()
                .into_owned()
        }
    }

    /// Warnings raised by the most recent call, such as boolean warnings or
    /// a healed result; empty when the call raised none.
    pub fn last_warnings(&self) -> Vec<String> {
        self.read_text(occt_bridge_session_last_warnings)
            .lines()
            .map(str::to_owned)
            .collect()
    }

    pub fn options(&self) -> Result<SessionOptions, BridgeError> {
        let mut raw = RawSessionOptions {
            validate_results: 0,
            heal_invalid_results: 0,
            boolean_fuzzy_tolerance: 0.0,
        };
        // SAFETY: The session and output pointers are valid.
        self.check(unsafe { occt_bridge_session_get_options(self.raw.as_ptr(), &mut raw) })?;
        Ok(SessionOptions {
            validate_results: raw.validate_results != 0,
            heal_invalid_results: raw.heal_invalid_results != 0,
            boolean_fuzzy_tolerance: raw.boolean_fuzzy_tolerance,
        })
    }

    /// Changes how later calls treat kernel results. Rejects negative or
    /// non-finite fuzziness and healing without validation.
    pub fn set_options(&self, options: SessionOptions) -> Result<(), BridgeError> {
        let raw = RawSessionOptions {
            validate_results: c_int::from(options.validate_results),
            heal_invalid_results: c_int::from(options.heal_invalid_results),
            boolean_fuzzy_tolerance: options.boolean_fuzzy_tolerance,
        };
        // SAFETY: The session and input pointers are valid.
        self.check(unsafe { occt_bridge_session_set_options(self.raw.as_ptr(), &raw) })
    }

    pub(crate) fn error_without_session(status: RawStatus) -> BridgeError {
        // SAFETY: The C function always returns a pointer to a static NUL-terminated string.
        let category = unsafe {
            let pointer = occt_bridge_status_string(status);
            if pointer.is_null() {
                "unknown status".into()
            } else {
                CStr::from_ptr(pointer).to_string_lossy().into_owned()
            }
        };
        BridgeError {
            status,
            category,
            message: String::new(),
            diagnostics: Vec::new(),
        }
    }
}
