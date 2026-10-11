//! Shape handles and the native forms of the shared value types.

use super::*;
pub use occt_types::*;

impl From<Vec3> for RawVec3 {
    fn from(value: Vec3) -> Self {
        Self {
            x: value.x,
            y: value.y,
            z: value.z,
        }
    }
}

impl From<RawVec3> for Vec3 {
    fn from(value: RawVec3) -> Self {
        Self::new(value.x, value.y, value.z)
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct RawDiagnostic {
    pub(crate) kind: i32,
    pub(crate) code: i32,
    pub(crate) input_index: i64,
    pub(crate) has_shape: c_int,
}

pub(crate) fn diagnostic_kind(kind: i32) -> DiagnosticKind {
    {
        match kind {
            1 => DiagnosticKind::FilletEdge,
            2 => DiagnosticKind::FilletVertex,
            3 => DiagnosticKind::IsolatedEdge,
            4 => DiagnosticKind::Offset,
            5 => DiagnosticKind::BooleanAlert,
            6 => DiagnosticKind::InvalidSubshape,
            7 => DiagnosticKind::Draft,
            other => DiagnosticKind::Other(other),
        }
    }
}

/// A session-owned shape handle. Dropping it releases the kernel shape and
/// its operation history; [`Session::remove`] releases it immediately and
/// reports errors. Handles are not `Clone`; use [`Session::duplicate`] for a
/// second handle to the same geometry.
#[derive(Debug, PartialEq, Eq)]
pub struct Shape<'session> {
    pub(crate) id: RawShapeId,
    pub(crate) owner: NonNull<c_void>,
    pub(crate) generation: u64,
    pub(crate) _session: PhantomData<&'session Session>,
}

impl Drop for Shape<'_> {
    fn drop(&mut self) {
        // SAFETY: The `'session` lifetime keeps the owning session alive.
        // Release ignores handles already removed or cleared (ids are never
        // reused) and leaves the session's diagnostics untouched.
        unsafe { occt_bridge_shape_release(self.owner.as_ptr(), self.id) };
    }
}

#[derive(Debug, PartialEq)]
pub struct WallTorch<'session> {
    pub fixture: Shape<'session>,
    pub flame: Shape<'session>,
    pub light: LightDesc,
}

impl Shape<'_> {
    pub const fn id(&self) -> u64 {
        self.id
    }
}

/// One placed component of [`Session::save_step_assembly`].
#[derive(Clone, Copy, Debug)]
pub struct StepComponent<'a, 'session> {
    /// The placed shape. Components whose shapes share geometry at different
    /// locations, such as rigidly placed copies, become one STEP part.
    pub shape: &'a Shape<'session>,
    pub name: &'a str,
    /// Names the part; for a shared part, the first component's name wins.
    pub part_name: &'a str,
    /// sRGB channels in [0, 1]; for a shared part, the first component's wins.
    pub color: Option<[f64; 3]>,
}

#[derive(Debug)]
pub struct ProjectedEdges<'session> {
    pub visible: Shape<'session>,
    pub hidden: Shape<'session>,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct RawSessionOptions {
    pub(crate) validate_results: c_int,
    pub(crate) heal_invalid_results: c_int,
    pub(crate) boolean_fuzzy_tolerance: f64,
}
