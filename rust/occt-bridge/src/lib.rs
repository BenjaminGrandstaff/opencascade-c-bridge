//! Safe, dependency-free Rust wrapper around `opencascade-c-bridge`.

use std::{
    cell::Cell,
    error::Error,
    ffi::{CStr, CString, c_char, c_int, c_void},
    fmt,
    marker::PhantomData,
    path::Path,
    ptr::{self, NonNull},
};

const ABI_VERSION: u32 = 26;

#[repr(C)]
struct RawVec3 {
    x: f64,
    y: f64,
    z: f64,
}

#[repr(C)]
struct RawWireSegment {
    kind: c_int,
    start: RawVec3,
    middle: RawVec3,
    end: RawVec3,
}

#[repr(C)]
struct RawBounds {
    min: RawVec3,
    max: RawVec3,
}

#[repr(C)]
struct RawLightDesc {
    light_type: c_int,
    position: RawVec3,
    direction: RawVec3,
    color: RawVec3,
    intensity: f64,
    range: f64,
    spot_angle_degrees: f64,
    cast_shadows: c_int,
}

#[repr(C)]
struct RawWallTorchResult {
    fixture_shape: RawShapeId,
    flame_shape: RawShapeId,
    light: RawLightDesc,
}

type RawShapeId = u64;
type RawStatus = c_int;

const OK: RawStatus = 0;

#[link(name = "occt_bridge")]
unsafe extern "C" {
    fn occt_bridge_abi_version() -> u32;
    fn occt_bridge_status_string(status: RawStatus) -> *const c_char;
    fn occt_bridge_session_create(version: u32, out: *mut *mut c_void) -> RawStatus;
    fn occt_bridge_session_destroy(session: *mut c_void);
    fn occt_bridge_session_clear(session: *mut c_void) -> RawStatus;
    fn occt_bridge_session_shape_count(session: *mut c_void, out: *mut usize) -> RawStatus;
    fn occt_bridge_session_last_error(
        session: *const c_void,
        buffer: *mut c_char,
        capacity: usize,
    ) -> usize;
    fn occt_bridge_session_last_warnings(
        session: *const c_void,
        buffer: *mut c_char,
        capacity: usize,
    ) -> usize;
    fn occt_bridge_session_diagnostic_count(session: *mut c_void, out: *mut usize) -> RawStatus;
    fn occt_bridge_session_diagnostic_at(
        session: *mut c_void,
        index: usize,
        out_diagnostic: *mut RawDiagnostic,
        out_shape: *mut RawShapeId,
    ) -> RawStatus;
    fn occt_bridge_session_diagnostic_name(
        session: *const c_void,
        index: usize,
        buffer: *mut c_char,
        capacity: usize,
    ) -> usize;
    fn occt_bridge_session_get_options(
        session: *mut c_void,
        out: *mut RawSessionOptions,
    ) -> RawStatus;
    fn occt_bridge_session_set_options(
        session: *mut c_void,
        options: *const RawSessionOptions,
    ) -> RawStatus;
    fn occt_bridge_create_box(
        session: *mut c_void,
        origin: RawVec3,
        size: RawVec3,
        out: *mut RawShapeId,
    ) -> RawStatus;
    fn occt_bridge_create_cylinder(
        session: *mut c_void,
        origin: RawVec3,
        axis: RawVec3,
        radius: f64,
        height: f64,
        out: *mut RawShapeId,
    ) -> RawStatus;
    fn occt_bridge_create_cone(
        session: *mut c_void,
        origin: RawVec3,
        axis: RawVec3,
        base_radius: f64,
        top_radius: f64,
        height: f64,
        out: *mut RawShapeId,
    ) -> RawStatus;
    fn occt_bridge_create_sphere(
        session: *mut c_void,
        center: RawVec3,
        radius: f64,
        out: *mut RawShapeId,
    ) -> RawStatus;
    fn occt_bridge_create_polyline_wire(
        session: *mut c_void,
        points: *const RawVec3,
        point_count: usize,
        closed: c_int,
        out: *mut RawShapeId,
    ) -> RawStatus;
    fn occt_bridge_create_segment_wire(
        session: *mut c_void,
        segments: *const RawWireSegment,
        count: usize,
        closed: c_int,
        out: *mut RawShapeId,
    ) -> RawStatus;
    fn occt_bridge_create_circle_wire(
        session: *mut c_void,
        center: RawVec3,
        normal: RawVec3,
        radius: f64,
        out: *mut RawShapeId,
    ) -> RawStatus;
    fn occt_bridge_create_ellipse_wire(
        session: *mut c_void,
        center: RawVec3,
        normal: RawVec3,
        major_radius: f64,
        minor_radius: f64,
        out: *mut RawShapeId,
    ) -> RawStatus;
    fn occt_bridge_create_face_from_wire(
        session: *mut c_void,
        wire: RawShapeId,
        out: *mut RawShapeId,
    ) -> RawStatus;
    fn occt_bridge_create_prism_from_face(
        session: *mut c_void,
        face: RawShapeId,
        direction: RawVec3,
        out: *mut RawShapeId,
    ) -> RawStatus;
    fn occt_bridge_create_polygon_prism(
        session: *mut c_void,
        points: *const RawVec3,
        point_count: usize,
        direction: RawVec3,
        out: *mut RawShapeId,
    ) -> RawStatus;
    fn occt_bridge_create_faceted_stone(
        session: *mut c_void,
        bottom_points: *const RawVec3,
        top_points: *const RawVec3,
        point_count: usize,
        top_center: RawVec3,
        bottom_chamfer: f64,
        top_fillet: f64,
        out: *mut RawShapeId,
    ) -> RawStatus;
    fn occt_bridge_create_wall_torch(
        session: *mut c_void,
        wall_anchor: RawVec3,
        wall_normal: RawVec3,
        scale: f64,
        out: *mut RawWallTorchResult,
    ) -> RawStatus;
    fn occt_bridge_create_polyline_tube(
        session: *mut c_void,
        path_points: *const RawVec3,
        point_count: usize,
        radius: f64,
        out: *mut RawShapeId,
    ) -> RawStatus;
    fn occt_bridge_create_loft(
        session: *mut c_void,
        points: *const RawVec3,
        section_point_counts: *const usize,
        section_count: usize,
        make_solid: c_int,
        ruled: c_int,
        out: *mut RawShapeId,
    ) -> RawStatus;
    fn occt_bridge_sew(
        session: *mut c_void,
        shapes: *const RawShapeId,
        shape_count: usize,
        tolerance: f64,
        out: *mut RawShapeId,
    ) -> RawStatus;
    fn occt_bridge_make_solid(
        session: *mut c_void,
        shell: RawShapeId,
        out: *mut RawShapeId,
    ) -> RawStatus;
    fn occt_bridge_make_solid_from_shells(
        session: *mut c_void,
        shells: *const RawShapeId,
        shell_count: usize,
        out: *mut RawShapeId,
    ) -> RawStatus;
    fn occt_bridge_create_compound(
        session: *mut c_void,
        shapes: *const RawShapeId,
        shape_count: usize,
        out: *mut RawShapeId,
    ) -> RawStatus;
    fn occt_bridge_fuse(
        session: *mut c_void,
        left: RawShapeId,
        right: RawShapeId,
        out: *mut RawShapeId,
    ) -> RawStatus;
    fn occt_bridge_cut(
        session: *mut c_void,
        object: RawShapeId,
        tool: RawShapeId,
        out: *mut RawShapeId,
    ) -> RawStatus;
    fn occt_bridge_common(
        session: *mut c_void,
        left: RawShapeId,
        right: RawShapeId,
        out: *mut RawShapeId,
    ) -> RawStatus;
    fn occt_bridge_fillet(
        session: *mut c_void,
        shape: RawShapeId,
        edges: *const RawShapeId,
        edge_count: usize,
        radius: f64,
        out: *mut RawShapeId,
    ) -> RawStatus;
    fn occt_bridge_chamfer(
        session: *mut c_void,
        shape: RawShapeId,
        edges: *const RawShapeId,
        edge_count: usize,
        distance: f64,
        out: *mut RawShapeId,
    ) -> RawStatus;
    fn occt_bridge_offset(
        session: *mut c_void,
        shape: RawShapeId,
        offset: f64,
        tolerance: f64,
        out: *mut RawShapeId,
    ) -> RawStatus;
    fn occt_bridge_hollow(
        session: *mut c_void,
        shape: RawShapeId,
        faces_to_remove: *const RawShapeId,
        face_count: usize,
        thickness: f64,
        tolerance: f64,
        out: *mut RawShapeId,
    ) -> RawStatus;
    fn occt_bridge_translate(
        session: *mut c_void,
        shape: RawShapeId,
        offset: RawVec3,
        out: *mut RawShapeId,
    ) -> RawStatus;
    fn occt_bridge_rotate(
        session: *mut c_void,
        shape: RawShapeId,
        axis_origin: RawVec3,
        axis_direction: RawVec3,
        angle_radians: f64,
        out: *mut RawShapeId,
    ) -> RawStatus;
    fn occt_bridge_scale(
        session: *mut c_void,
        shape: RawShapeId,
        center: RawVec3,
        factor: f64,
        out: *mut RawShapeId,
    ) -> RawStatus;
    fn occt_bridge_shape_bounds(
        session: *mut c_void,
        shape: RawShapeId,
        out: *mut RawBounds,
    ) -> RawStatus;
    fn occt_bridge_shape_exact_bounds(
        session: *mut c_void,
        shape: RawShapeId,
        out: *mut RawBounds,
    ) -> RawStatus;
    fn occt_bridge_shape_duplicate(
        session: *mut c_void,
        shape: RawShapeId,
        out: *mut RawShapeId,
    ) -> RawStatus;
    fn occt_bridge_shape_type(
        session: *mut c_void,
        shape: RawShapeId,
        out: *mut c_int,
    ) -> RawStatus;
    fn occt_bridge_shape_subshape_count(
        session: *mut c_void,
        shape: RawShapeId,
        subshape_type: c_int,
        out: *mut usize,
    ) -> RawStatus;
    fn occt_bridge_shape_subshape_at(
        session: *mut c_void,
        shape: RawShapeId,
        subshape_type: c_int,
        index: usize,
        out: *mut RawShapeId,
    ) -> RawStatus;
    fn occt_bridge_shape_surface_area(
        session: *mut c_void,
        shape: RawShapeId,
        out: *mut f64,
    ) -> RawStatus;
    fn occt_bridge_shape_volume(
        session: *mut c_void,
        shape: RawShapeId,
        out: *mut f64,
    ) -> RawStatus;
    fn occt_bridge_shape_center_of_mass(
        session: *mut c_void,
        shape: RawShapeId,
        out: *mut RawVec3,
    ) -> RawStatus;
    fn occt_bridge_shape_face_normal(
        session: *mut c_void,
        face: RawShapeId,
        out: *mut RawVec3,
    ) -> RawStatus;
    fn occt_bridge_shape_face_is_planar(
        session: *mut c_void,
        face: RawShapeId,
        out: *mut c_int,
    ) -> RawStatus;
    fn occt_bridge_shape_edge_length(
        session: *mut c_void,
        edge: RawShapeId,
        out: *mut f64,
    ) -> RawStatus;
    fn occt_bridge_shape_edge_circle_radius(
        session: *mut c_void,
        edge: RawShapeId,
        out_is_circle: *mut c_int,
        out_radius: *mut f64,
    ) -> RawStatus;
    fn occt_bridge_shape_edge_curvature(
        session: *mut c_void,
        edge: RawShapeId,
        out_is_defined: *mut c_int,
        out_curvature: *mut f64,
    ) -> RawStatus;
    fn occt_bridge_shape_edge_curvature_range(
        session: *mut c_void,
        edge: RawShapeId,
        sample_count: usize,
        out_minimum_curvature: *mut f64,
        out_maximum_curvature: *mut f64,
    ) -> RawStatus;
    fn occt_bridge_shape_edge_curvature_extrema(
        session: *mut c_void,
        edge: RawShapeId,
        relative_tolerance: f64,
        out_minimum: *mut f64,
        out_minimum_lower_bound: *mut f64,
        out_maximum: *mut f64,
        out_maximum_upper_bound: *mut f64,
        out_is_exact: *mut c_int,
    ) -> RawStatus;
    fn occt_bridge_shape_is_adjacent(
        session: *mut c_void,
        parent: RawShapeId,
        first: RawShapeId,
        second: RawShapeId,
        out: *mut c_int,
    ) -> RawStatus;
    fn occt_bridge_shape_is_same(
        session: *mut c_void,
        first: RawShapeId,
        second: RawShapeId,
        out: *mut c_int,
    ) -> RawStatus;
    fn occt_bridge_shape_faces_are_tangent(
        session: *mut c_void,
        parent: RawShapeId,
        first_face: RawShapeId,
        second_face: RawShapeId,
        out: *mut c_int,
    ) -> RawStatus;
    fn occt_bridge_shape_history_count(
        session: *mut c_void,
        result: RawShapeId,
        source: RawShapeId,
        relation: c_int,
        out: *mut usize,
    ) -> RawStatus;
    fn occt_bridge_shape_history_at(
        session: *mut c_void,
        result: RawShapeId,
        source: RawShapeId,
        relation: c_int,
        index: usize,
        out: *mut RawShapeId,
    ) -> RawStatus;
    fn occt_bridge_shape_history_is_deleted(
        session: *mut c_void,
        result: RawShapeId,
        source: RawShapeId,
        out: *mut c_int,
    ) -> RawStatus;
    fn occt_bridge_shape_is_valid(
        session: *mut c_void,
        shape: RawShapeId,
        out: *mut c_int,
    ) -> RawStatus;
    fn occt_bridge_shape_remove(session: *mut c_void, shape: RawShapeId) -> RawStatus;
    fn occt_bridge_shape_release(session: *mut c_void, shape: RawShapeId);
    fn occt_bridge_brep_save(
        session: *mut c_void,
        shape: RawShapeId,
        path: *const c_char,
    ) -> RawStatus;
    fn occt_bridge_brep_load(
        session: *mut c_void,
        path: *const c_char,
        out: *mut RawShapeId,
    ) -> RawStatus;
    fn occt_bridge_step_save(
        session: *mut c_void,
        shape: RawShapeId,
        path: *const c_char,
    ) -> RawStatus;
    fn occt_bridge_step_load(
        session: *mut c_void,
        path: *const c_char,
        out: *mut RawShapeId,
    ) -> RawStatus;
    fn occt_bridge_stl_save(
        session: *mut c_void,
        shape: RawShapeId,
        path: *const c_char,
        linear_deflection: f64,
        angular_deflection_radians: f64,
        binary: c_int,
    ) -> RawStatus;
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

/// An exact line or circular arc in an ordered wire.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WireSegment {
    Line {
        start: Vec3,
        end: Vec3,
    },
    /// The arc runs from `start` through `middle` to `end`.
    Arc {
        start: Vec3,
        middle: Vec3,
        end: Vec3,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StlFormat {
    Ascii,
    Binary,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StlOptions {
    pub linear_deflection: f64,
    pub angular_deflection_radians: f64,
    pub format: StlFormat,
}

impl Default for StlOptions {
    fn default() -> Self {
        Self {
            linear_deflection: 0.1,
            angular_deflection_radians: 0.5,
            format: StlFormat::Binary,
        }
    }
}

impl Vec3 {
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }
}

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
struct RawDiagnostic {
    kind: i32,
    code: i32,
    input_index: i64,
    has_shape: c_int,
}

/// What a failure diagnostic describes, and which OCCT enumeration its code
/// belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticKind {
    /// A selected edge whose fillet contour failed; code is `ChFiDS_ErrorStatus`.
    FilletEdge,
    /// A vertex where fillet contours could not be joined.
    FilletVertex,
    /// A selected fillet or chamfer edge whose contour fails even when built
    /// alone; the name is the OCCT exception the rebuild raised, if any.
    IsolatedEdge,
    /// An offset or hollow failure; code is `BRepOffset_Error`.
    Offset,
    /// A boolean alert; the name is its OCCT alert key.
    BooleanAlert,
    /// A subshape of a rejected result; code is `BRepCheck_Status`.
    InvalidSubshape,
    /// A kind added by a newer library.
    Other(i32),
}

impl DiagnosticKind {
    fn from_raw(kind: i32) -> Self {
        match kind {
            1 => Self::FilletEdge,
            2 => Self::FilletVertex,
            3 => Self::IsolatedEdge,
            4 => Self::Offset,
            5 => Self::BooleanAlert,
            6 => Self::InvalidSubshape,
            other => Self::Other(other),
        }
    }
}

/// What OCCT reported about the cause of a failed call. The subshape it
/// names, if any, is available from [`Session::last_diagnostic_shape`] until
/// the next call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub kind: DiagnosticKind,
    /// OCCT's enumeration value for the kind.
    pub code: i32,
    /// OCCT's name for the code, such as `ChFiDS_WalkingFailure`.
    pub name: String,
    /// The selected edge or face, or boolean operand (0 left, 1 right), the
    /// diagnostic concerns.
    pub input_index: Option<usize>,
    pub has_shape: bool,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct RawSessionOptions {
    validate_results: c_int,
    heal_invalid_results: c_int,
    boolean_fuzzy_tolerance: f64,
}

/// How a session treats kernel results.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SessionOptions {
    /// Check results of booleans, fillets, chamfers, offsets, hollowing,
    /// sewing, and STEP and BREP import; invalid results become errors.
    pub validate_results: bool,
    /// Repair invalid results with shape fixing when possible, carrying
    /// operation history through the repair and recording a warning.
    /// Requires `validate_results`.
    pub heal_invalid_results: bool,
    /// Distance below which boolean inputs are treated as coincident; zero
    /// performs exact booleans.
    pub boolean_fuzzy_tolerance: f64,
}

impl Default for SessionOptions {
    fn default() -> Self {
        Self {
            validate_results: true,
            heal_invalid_results: false,
            boolean_fuzzy_tolerance: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CurvatureExtrema {
    /// Smallest curvature found at an actual edge point.
    pub minimum: f64,
    /// The true minimum lies in `[minimum_lower_bound, minimum]`.
    pub minimum_lower_bound: f64,
    /// Largest curvature found at an actual edge point.
    pub maximum: f64,
    /// The true maximum lies in `[maximum, maximum_upper_bound]`.
    pub maximum_upper_bound: f64,
    /// Analytic evaluation with coincident bounds.
    pub is_exact: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub min: Vec3,
    pub max: Vec3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum ShapeType {
    Compound = 1,
    CompSolid = 2,
    Solid = 3,
    Shell = 4,
    Face = 5,
    Wire = 6,
    Edge = 7,
    Vertex = 8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum HistoryRelation {
    Generated = 1,
    Modified = 2,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LightType {
    Ambient,
    Directional,
    Positional,
    Spotlight,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightDesc {
    pub light_type: LightType,
    pub position: Vec3,
    pub direction: Vec3,
    pub color: Vec3,
    pub intensity: f64,
    pub range: f64,
    pub spot_angle_degrees: f64,
    pub cast_shadows: bool,
}

/// A session-owned shape handle. Dropping it releases the kernel shape and
/// its operation history; [`Session::remove`] releases it immediately and
/// reports errors. Handles are not `Clone`; use [`Session::duplicate`] for a
/// second handle to the same geometry.
#[derive(Debug, PartialEq, Eq)]
pub struct Shape<'session> {
    id: RawShapeId,
    owner: NonNull<c_void>,
    generation: u64,
    _session: PhantomData<&'session Session>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeError {
    pub status: i32,
    pub category: String,
    pub message: String,
    /// Structured causes of a kernel failure, in the order OCCT reported
    /// them; empty when the failure has none.
    pub diagnostics: Vec<Diagnostic>,
}

impl fmt::Display for BridgeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.message.is_empty() {
            write!(formatter, "{}", self.category)
        } else {
            write!(formatter, "{}: {}", self.category, self.message)
        }
    }
}

impl Error for BridgeError {}

pub struct Session {
    raw: NonNull<c_void>,
    generation: Cell<u64>,
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

    pub fn create_box(&self, origin: Vec3, size: Vec3) -> Result<Shape<'_>, BridgeError> {
        let mut shape = 0;
        // SAFETY: `self.raw` and the output pointer are valid; vectors are passed by value.
        self.check(unsafe {
            occt_bridge_create_box(self.raw.as_ptr(), origin.into(), size.into(), &mut shape)
        })?;
        Ok(self.shape(shape))
    }

    pub fn create_cylinder(
        &self,
        origin: Vec3,
        axis: Vec3,
        radius: f64,
        height: f64,
    ) -> Result<Shape<'_>, BridgeError> {
        let mut shape = 0;
        // SAFETY: The session and output pointers are valid; vectors are passed by value.
        self.check(unsafe {
            occt_bridge_create_cylinder(
                self.raw.as_ptr(),
                origin.into(),
                axis.into(),
                radius,
                height,
                &mut shape,
            )
        })?;
        Ok(self.shape(shape))
    }

    pub fn create_cone(
        &self,
        origin: Vec3,
        axis: Vec3,
        base_radius: f64,
        top_radius: f64,
        height: f64,
    ) -> Result<Shape<'_>, BridgeError> {
        let mut shape = 0;
        // SAFETY: The session and output pointers are valid; vectors are passed by value.
        self.check(unsafe {
            occt_bridge_create_cone(
                self.raw.as_ptr(),
                origin.into(),
                axis.into(),
                base_radius,
                top_radius,
                height,
                &mut shape,
            )
        })?;
        Ok(self.shape(shape))
    }

    pub fn create_sphere(&self, center: Vec3, radius: f64) -> Result<Shape<'_>, BridgeError> {
        let mut shape = 0;
        // SAFETY: The session and output pointers are valid; the center is passed by value.
        self.check(unsafe {
            occt_bridge_create_sphere(self.raw.as_ptr(), center.into(), radius, &mut shape)
        })?;
        Ok(self.shape(shape))
    }

    pub fn create_polyline_wire(
        &self,
        points: &[Vec3],
        closed: bool,
    ) -> Result<Shape<'_>, BridgeError> {
        let points: Vec<RawVec3> = points.iter().copied().map(Into::into).collect();
        let mut shape = 0;
        // SAFETY: The point slice, session, and output remain valid for the call.
        self.check(unsafe {
            occt_bridge_create_polyline_wire(
                self.raw.as_ptr(),
                points.as_ptr(),
                points.len(),
                i32::from(closed),
                &mut shape,
            )
        })?;
        Ok(self.shape(shape))
    }

    /// Creates an ordered, connected wire of exact lines and circular arcs.
    /// `closed` requires closure when true; false permits either open or closed
    /// wires. Planarity and absence of self-intersections are not required.
    pub fn create_segment_wire(
        &self,
        segments: &[WireSegment],
        closed: bool,
    ) -> Result<Shape<'_>, BridgeError> {
        let raw = segments
            .iter()
            .map(|segment| match *segment {
                WireSegment::Line { start, end } => RawWireSegment {
                    kind: 0,
                    start: start.into(),
                    middle: start.into(),
                    end: end.into(),
                },
                WireSegment::Arc { start, middle, end } => RawWireSegment {
                    kind: 1,
                    start: start.into(),
                    middle: middle.into(),
                    end: end.into(),
                },
            })
            .collect::<Vec<_>>();
        let mut shape = 0;
        // SAFETY: The segment slice, session and output remain valid during the call.
        self.check(unsafe {
            occt_bridge_create_segment_wire(
                self.raw.as_ptr(),
                raw.as_ptr(),
                raw.len(),
                i32::from(closed),
                &mut shape,
            )
        })?;
        Ok(self.shape(shape))
    }

    pub fn create_circle_wire(
        &self,
        center: Vec3,
        normal: Vec3,
        radius: f64,
    ) -> Result<Shape<'_>, BridgeError> {
        let mut shape = 0;
        // SAFETY: The session and output pointers are valid; vectors are passed by value.
        self.check(unsafe {
            occt_bridge_create_circle_wire(
                self.raw.as_ptr(),
                center.into(),
                normal.into(),
                radius,
                &mut shape,
            )
        })?;
        Ok(self.shape(shape))
    }

    pub fn create_ellipse_wire(
        &self,
        center: Vec3,
        normal: Vec3,
        major_radius: f64,
        minor_radius: f64,
    ) -> Result<Shape<'_>, BridgeError> {
        let mut shape = 0;
        // SAFETY: The session and output pointers are valid; vectors are passed by value.
        self.check(unsafe {
            occt_bridge_create_ellipse_wire(
                self.raw.as_ptr(),
                center.into(),
                normal.into(),
                major_radius,
                minor_radius,
                &mut shape,
            )
        })?;
        Ok(self.shape(shape))
    }

    pub fn create_face_from_wire<'a>(&'a self, wire: &Shape<'_>) -> Result<Shape<'a>, BridgeError> {
        self.derived_shape(wire, |out| unsafe {
            occt_bridge_create_face_from_wire(self.raw.as_ptr(), wire.id, out)
        })
    }

    pub fn create_prism_from_face<'a>(
        &'a self,
        face: &Shape<'_>,
        direction: Vec3,
    ) -> Result<Shape<'a>, BridgeError> {
        self.derived_shape(face, |out| unsafe {
            occt_bridge_create_prism_from_face(self.raw.as_ptr(), face.id, direction.into(), out)
        })
    }

    pub fn create_polygon_prism(
        &self,
        points: &[Vec3],
        direction: Vec3,
    ) -> Result<Shape<'_>, BridgeError> {
        let raw_points: Vec<RawVec3> = points.iter().copied().map(Into::into).collect();
        let mut shape = 0;
        // SAFETY: The point slice and output remain valid for the duration of the call.
        self.check(unsafe {
            occt_bridge_create_polygon_prism(
                self.raw.as_ptr(),
                raw_points.as_ptr(),
                raw_points.len(),
                direction.into(),
                &mut shape,
            )
        })?;
        Ok(self.shape(shape))
    }

    #[deprecated(note = "use occt_recipes::create_faceted_stone")]
    pub fn create_faceted_stone(
        &self,
        bottom_points: &[Vec3],
        top_points: &[Vec3],
        top_center: Vec3,
        bottom_chamfer: f64,
        top_fillet: f64,
    ) -> Result<Shape<'_>, BridgeError> {
        if bottom_points.len() != top_points.len() {
            return Err(BridgeError {
                status: 1,
                category: "invalid argument".into(),
                message: "bottom and top rings must have the same point count".into(),
                diagnostics: Vec::new(),
            });
        }
        let bottom: Vec<RawVec3> = bottom_points.iter().copied().map(Into::into).collect();
        let top: Vec<RawVec3> = top_points.iter().copied().map(Into::into).collect();
        let mut shape = 0;
        // SAFETY: Both point arrays and the output remain valid for the call.
        self.check(unsafe {
            occt_bridge_create_faceted_stone(
                self.raw.as_ptr(),
                bottom.as_ptr(),
                top.as_ptr(),
                bottom.len(),
                top_center.into(),
                bottom_chamfer,
                top_fillet,
                &mut shape,
            )
        })?;
        Ok(self.shape(shape))
    }

    #[deprecated(note = "use occt_recipes::create_wall_torch")]
    pub fn create_wall_torch(
        &self,
        wall_anchor: Vec3,
        wall_normal: Vec3,
        scale: f64,
    ) -> Result<WallTorch<'_>, BridgeError> {
        let mut result = RawWallTorchResult {
            fixture_shape: 0,
            flame_shape: 0,
            light: RawLightDesc {
                light_type: 0,
                position: RawVec3::from(Vec3::new(0.0, 0.0, 0.0)),
                direction: RawVec3::from(Vec3::new(0.0, 0.0, 0.0)),
                color: RawVec3::from(Vec3::new(0.0, 0.0, 0.0)),
                intensity: 0.0,
                range: 0.0,
                spot_angle_degrees: 0.0,
                cast_shadows: 0,
            },
        };
        // SAFETY: The session and output pointer are valid; vectors are passed by value.
        self.check(unsafe {
            occt_bridge_create_wall_torch(
                self.raw.as_ptr(),
                wall_anchor.into(),
                wall_normal.into(),
                scale,
                &mut result,
            )
        })?;
        Ok(WallTorch {
            fixture: self.shape(result.fixture_shape),
            flame: self.shape(result.flame_shape),
            light: LightDesc {
                light_type: match result.light.light_type {
                    0 => LightType::Ambient,
                    1 => LightType::Directional,
                    2 => LightType::Positional,
                    3 => LightType::Spotlight,
                    value => {
                        return Err(BridgeError {
                            status: 8,
                            category: "internal error".into(),
                            message: format!("library returned unknown light type {value}"),
                            diagnostics: Vec::new(),
                        });
                    }
                },
                position: result.light.position.into(),
                direction: result.light.direction.into(),
                color: result.light.color.into(),
                intensity: result.light.intensity,
                range: result.light.range,
                spot_angle_degrees: result.light.spot_angle_degrees,
                cast_shadows: result.light.cast_shadows != 0,
            },
        })
    }

    pub fn create_polyline_tube(
        &self,
        path_points: &[Vec3],
        radius: f64,
    ) -> Result<Shape<'_>, BridgeError> {
        let points: Vec<RawVec3> = path_points.iter().copied().map(Into::into).collect();
        let mut shape = 0;
        // SAFETY: The point slice and output remain valid for the duration of the call.
        self.check(unsafe {
            occt_bridge_create_polyline_tube(
                self.raw.as_ptr(),
                points.as_ptr(),
                points.len(),
                radius,
                &mut shape,
            )
        })?;
        Ok(self.shape(shape))
    }

    pub fn create_loft(
        &self,
        sections: &[&[Vec3]],
        make_solid: bool,
        ruled: bool,
    ) -> Result<Shape<'_>, BridgeError> {
        let counts: Vec<usize> = sections.iter().map(|section| section.len()).collect();
        let points: Vec<RawVec3> = sections
            .iter()
            .flat_map(|section| section.iter().copied())
            .map(Into::into)
            .collect();
        let mut shape = 0;
        // SAFETY: Flattened points, counts, and output remain valid for the call.
        self.check(unsafe {
            occt_bridge_create_loft(
                self.raw.as_ptr(),
                points.as_ptr(),
                counts.as_ptr(),
                counts.len(),
                i32::from(make_solid),
                i32::from(ruled),
                &mut shape,
            )
        })?;
        Ok(self.shape(shape))
    }

    /// Sews faces and shells whose edges lie within `tolerance` into a shell,
    /// or a compound of shells and free faces when not everything joins.
    /// Records modified and deleted history for the inputs.
    pub fn sew(&self, shapes: &[&Shape<'_>], tolerance: f64) -> Result<Shape<'_>, BridgeError> {
        for shape in shapes {
            self.validate_shape(shape)?;
        }
        let ids: Vec<RawShapeId> = shapes.iter().map(|shape| shape.id).collect();
        let mut sewn = 0;
        // SAFETY: The ID slice and output remain valid for the call.
        self.check(unsafe {
            occt_bridge_sew(
                self.raw.as_ptr(),
                ids.as_ptr(),
                ids.len(),
                tolerance,
                &mut sewn,
            )
        })?;
        Ok(self.shape(sewn))
    }

    /// Builds an outward-oriented solid from a shape with exactly one closed shell.
    pub fn make_solid<'a>(&'a self, shell: &Shape<'_>) -> Result<Shape<'a>, BridgeError> {
        self.derived_shape(shell, |out| unsafe {
            occt_bridge_make_solid(self.raw.as_ptr(), shell.id, out)
        })
    }

    /// Builds one solid from the largest outer boundary and any enclosed void
    /// boundaries found in `shells`.
    pub fn make_solid_from_shells(&self, shells: &[&Shape<'_>]) -> Result<Shape<'_>, BridgeError> {
        for shell in shells {
            self.validate_shape(shell)?;
        }
        let ids: Vec<RawShapeId> = shells.iter().map(|shell| shell.id).collect();
        let mut solid = 0;
        // SAFETY: The ID slice and output remain valid for the duration of the call.
        self.check(unsafe {
            occt_bridge_make_solid_from_shells(
                self.raw.as_ptr(),
                ids.as_ptr(),
                ids.len(),
                &mut solid,
            )
        })?;
        Ok(self.shape(solid))
    }

    pub fn create_compound(&self, shapes: &[&Shape<'_>]) -> Result<Shape<'_>, BridgeError> {
        for shape in shapes {
            self.validate_shape(shape)?;
        }
        let ids: Vec<RawShapeId> = shapes.iter().map(|shape| shape.id).collect();
        let mut compound = 0;
        // SAFETY: The ID slice and output remain valid for the call.
        self.check(unsafe {
            occt_bridge_create_compound(self.raw.as_ptr(), ids.as_ptr(), ids.len(), &mut compound)
        })?;
        Ok(self.shape(compound))
    }

    pub fn fuse<'a>(
        &'a self,
        left: &Shape<'_>,
        right: &Shape<'_>,
    ) -> Result<Shape<'a>, BridgeError> {
        self.boolean(left, right, occt_bridge_fuse)
    }

    pub fn cut<'a>(
        &'a self,
        object: &Shape<'_>,
        tool: &Shape<'_>,
    ) -> Result<Shape<'a>, BridgeError> {
        self.boolean(object, tool, occt_bridge_cut)
    }

    pub fn common<'a>(
        &'a self,
        left: &Shape<'_>,
        right: &Shape<'_>,
    ) -> Result<Shape<'a>, BridgeError> {
        self.boolean(left, right, occt_bridge_common)
    }

    pub fn fillet<'a>(
        &'a self,
        shape: &Shape<'_>,
        edges: &[&Shape<'_>],
        radius: f64,
    ) -> Result<Shape<'a>, BridgeError> {
        self.selected_operation(shape, edges, |ids, out| unsafe {
            occt_bridge_fillet(
                self.raw.as_ptr(),
                shape.id,
                ids.as_ptr(),
                ids.len(),
                radius,
                out,
            )
        })
    }

    pub fn chamfer<'a>(
        &'a self,
        shape: &Shape<'_>,
        edges: &[&Shape<'_>],
        distance: f64,
    ) -> Result<Shape<'a>, BridgeError> {
        self.selected_operation(shape, edges, |ids, out| unsafe {
            occt_bridge_chamfer(
                self.raw.as_ptr(),
                shape.id,
                ids.as_ptr(),
                ids.len(),
                distance,
                out,
            )
        })
    }

    pub fn offset<'a>(
        &'a self,
        shape: &Shape<'_>,
        offset: f64,
        tolerance: f64,
    ) -> Result<Shape<'a>, BridgeError> {
        self.derived_shape(shape, |out| unsafe {
            occt_bridge_offset(self.raw.as_ptr(), shape.id, offset, tolerance, out)
        })
    }

    pub fn hollow<'a>(
        &'a self,
        shape: &Shape<'_>,
        faces_to_remove: &[&Shape<'_>],
        thickness: f64,
        tolerance: f64,
    ) -> Result<Shape<'a>, BridgeError> {
        self.selected_operation(shape, faces_to_remove, |ids, out| unsafe {
            occt_bridge_hollow(
                self.raw.as_ptr(),
                shape.id,
                ids.as_ptr(),
                ids.len(),
                thickness,
                tolerance,
                out,
            )
        })
    }

    pub fn translate<'a>(
        &'a self,
        shape: &Shape<'_>,
        offset: Vec3,
    ) -> Result<Shape<'a>, BridgeError> {
        self.transform(shape, |out| unsafe {
            occt_bridge_translate(self.raw.as_ptr(), shape.id, offset.into(), out)
        })
    }

    pub fn rotate<'a>(
        &'a self,
        shape: &Shape<'_>,
        axis_origin: Vec3,
        axis_direction: Vec3,
        angle_radians: f64,
    ) -> Result<Shape<'a>, BridgeError> {
        self.transform(shape, |out| unsafe {
            occt_bridge_rotate(
                self.raw.as_ptr(),
                shape.id,
                axis_origin.into(),
                axis_direction.into(),
                angle_radians,
                out,
            )
        })
    }

    pub fn scale<'a>(
        &'a self,
        shape: &Shape<'_>,
        center: Vec3,
        factor: f64,
    ) -> Result<Shape<'a>, BridgeError> {
        self.transform(shape, |out| unsafe {
            occt_bridge_scale(self.raw.as_ptr(), shape.id, center.into(), factor, out)
        })
    }

    /// Axis-aligned bounds, enlarged by shape tolerances as OCCT reports them.
    pub fn bounds(&self, shape: &Shape<'_>) -> Result<Bounds, BridgeError> {
        self.query_bounds(shape, occt_bridge_shape_bounds)
    }

    /// Axis-aligned bounds that follow the geometry without tolerance
    /// enlargement; use these to measure lengths.
    pub fn exact_bounds(&self, shape: &Shape<'_>) -> Result<Bounds, BridgeError> {
        self.query_bounds(shape, occt_bridge_shape_exact_bounds)
    }

    fn query_bounds(
        &self,
        shape: &Shape<'_>,
        query: unsafe extern "C" fn(*mut c_void, RawShapeId, *mut RawBounds) -> RawStatus,
    ) -> Result<Bounds, BridgeError> {
        self.validate_shape(shape)?;
        let mut bounds = RawBounds {
            min: RawVec3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            max: RawVec3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
        };
        // SAFETY: The session and output pointers are valid.
        self.check(unsafe { query(self.raw.as_ptr(), shape.id, &mut bounds) })?;
        Ok(Bounds {
            min: bounds.min.into(),
            max: bounds.max.into(),
        })
    }

    pub fn shape_type(&self, shape: &Shape<'_>) -> Result<ShapeType, BridgeError> {
        self.validate_shape(shape)?;
        let mut shape_type = 0;
        // SAFETY: The session and output pointers are valid.
        self.check(unsafe {
            occt_bridge_shape_type(self.raw.as_ptr(), shape.id, &mut shape_type)
        })?;
        match shape_type {
            1 => Ok(ShapeType::Compound),
            2 => Ok(ShapeType::CompSolid),
            3 => Ok(ShapeType::Solid),
            4 => Ok(ShapeType::Shell),
            5 => Ok(ShapeType::Face),
            6 => Ok(ShapeType::Wire),
            7 => Ok(ShapeType::Edge),
            8 => Ok(ShapeType::Vertex),
            value => Err(BridgeError {
                status: 8,
                category: "internal error".into(),
                message: format!("library returned unknown shape type {value}"),
                diagnostics: Vec::new(),
            }),
        }
    }

    pub fn duplicate<'a>(&'a self, shape: &Shape<'_>) -> Result<Shape<'a>, BridgeError> {
        self.derived_shape(shape, |out| unsafe {
            occt_bridge_shape_duplicate(self.raw.as_ptr(), shape.id, out)
        })
    }

    pub fn subshape_count(
        &self,
        shape: &Shape<'_>,
        subshape_type: ShapeType,
    ) -> Result<usize, BridgeError> {
        self.validate_shape(shape)?;
        let mut count = 0;
        // SAFETY: The session and output pointers are valid.
        self.check(unsafe {
            occt_bridge_shape_subshape_count(
                self.raw.as_ptr(),
                shape.id,
                subshape_type as c_int,
                &mut count,
            )
        })?;
        Ok(count)
    }

    pub fn subshape<'a>(
        &'a self,
        shape: &Shape<'_>,
        subshape_type: ShapeType,
        index: usize,
    ) -> Result<Shape<'a>, BridgeError> {
        self.validate_shape(shape)?;
        let mut subshape = 0;
        // SAFETY: The session and output pointers are valid; the C layer checks the index.
        self.check(unsafe {
            occt_bridge_shape_subshape_at(
                self.raw.as_ptr(),
                shape.id,
                subshape_type as c_int,
                index,
                &mut subshape,
            )
        })?;
        Ok(self.shape(subshape))
    }

    pub fn surface_area(&self, shape: &Shape<'_>) -> Result<f64, BridgeError> {
        self.validate_shape(shape)?;
        let mut area = 0.0;
        // SAFETY: The session and output pointers are valid.
        self.check(unsafe {
            occt_bridge_shape_surface_area(self.raw.as_ptr(), shape.id, &mut area)
        })?;
        Ok(area)
    }

    pub fn volume(&self, shape: &Shape<'_>) -> Result<f64, BridgeError> {
        self.validate_shape(shape)?;
        let mut volume = 0.0;
        // SAFETY: The session and output pointers are valid.
        self.check(unsafe { occt_bridge_shape_volume(self.raw.as_ptr(), shape.id, &mut volume) })?;
        Ok(volume)
    }

    pub fn center_of_mass(&self, shape: &Shape<'_>) -> Result<Vec3, BridgeError> {
        self.validate_shape(shape)?;
        let mut center = RawVec3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        };
        // SAFETY: The session and output pointers are valid.
        self.check(unsafe {
            occt_bridge_shape_center_of_mass(self.raw.as_ptr(), shape.id, &mut center)
        })?;
        Ok(center.into())
    }

    pub fn face_normal(&self, face: &Shape<'_>) -> Result<Vec3, BridgeError> {
        self.validate_shape(face)?;
        let mut normal = RawVec3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        };
        // SAFETY: The session, validated face handle, and output pointer are valid.
        self.check(unsafe {
            occt_bridge_shape_face_normal(self.raw.as_ptr(), face.id, &mut normal)
        })?;
        Ok(normal.into())
    }

    pub fn face_is_planar(&self, face: &Shape<'_>) -> Result<bool, BridgeError> {
        self.validate_shape(face)?;
        let mut planar = 0;
        // SAFETY: The session, validated face handle, and output pointer are valid.
        self.check(unsafe {
            occt_bridge_shape_face_is_planar(self.raw.as_ptr(), face.id, &mut planar)
        })?;
        Ok(planar != 0)
    }

    pub fn edge_length(&self, edge: &Shape<'_>) -> Result<f64, BridgeError> {
        self.validate_shape(edge)?;
        let mut length = 0.0;
        // SAFETY: The session, validated edge handle, and output pointer are valid.
        self.check(unsafe {
            occt_bridge_shape_edge_length(self.raw.as_ptr(), edge.id, &mut length)
        })?;
        Ok(length)
    }

    pub fn edge_circle_radius(&self, edge: &Shape<'_>) -> Result<Option<f64>, BridgeError> {
        self.validate_shape(edge)?;
        let mut is_circle = 0;
        let mut radius = 0.0;
        // SAFETY: The session, validated edge handle, and output pointers are valid.
        self.check(unsafe {
            occt_bridge_shape_edge_circle_radius(
                self.raw.as_ptr(),
                edge.id,
                &mut is_circle,
                &mut radius,
            )
        })?;
        Ok((is_circle != 0).then_some(radius))
    }

    pub fn edge_curvature(&self, edge: &Shape<'_>) -> Result<Option<f64>, BridgeError> {
        self.validate_shape(edge)?;
        let mut is_defined = 0;
        let mut curvature = 0.0;
        // SAFETY: The session, validated edge handle, and output pointers are valid.
        self.check(unsafe {
            occt_bridge_shape_edge_curvature(
                self.raw.as_ptr(),
                edge.id,
                &mut is_defined,
                &mut curvature,
            )
        })?;
        Ok((is_defined != 0).then_some(curvature))
    }

    /// Exact (line, conic) or error-bounded (Bezier, B-spline) curvature
    /// extrema over the full edge.
    pub fn edge_curvature_extrema(
        &self,
        edge: &Shape<'_>,
        relative_tolerance: f64,
    ) -> Result<CurvatureExtrema, BridgeError> {
        self.validate_shape(edge)?;
        let mut extrema = CurvatureExtrema {
            minimum: 0.0,
            minimum_lower_bound: 0.0,
            maximum: 0.0,
            maximum_upper_bound: 0.0,
            is_exact: false,
        };
        let mut is_exact = 0;
        // SAFETY: The session, validated edge handle, and output pointers are valid.
        self.check(unsafe {
            occt_bridge_shape_edge_curvature_extrema(
                self.raw.as_ptr(),
                edge.id,
                relative_tolerance,
                &mut extrema.minimum,
                &mut extrema.minimum_lower_bound,
                &mut extrema.maximum,
                &mut extrema.maximum_upper_bound,
                &mut is_exact,
            )
        })?;
        extrema.is_exact = is_exact != 0;
        Ok(extrema)
    }

    pub fn edge_curvature_range(
        &self,
        edge: &Shape<'_>,
        sample_count: usize,
    ) -> Result<(f64, f64), BridgeError> {
        self.validate_shape(edge)?;
        let mut minimum = 0.0;
        let mut maximum = 0.0;
        // SAFETY: The session, validated edge handle, and output pointers are valid.
        self.check(unsafe {
            occt_bridge_shape_edge_curvature_range(
                self.raw.as_ptr(),
                edge.id,
                sample_count,
                &mut minimum,
                &mut maximum,
            )
        })?;
        Ok((minimum, maximum))
    }

    pub fn is_adjacent(
        &self,
        parent: &Shape<'_>,
        first: &Shape<'_>,
        second: &Shape<'_>,
    ) -> Result<bool, BridgeError> {
        self.validate_shape(parent)?;
        self.validate_shape(first)?;
        self.validate_shape(second)?;
        let mut adjacent = 0;
        // SAFETY: The session, validated shape handles, and output pointer are valid.
        self.check(unsafe {
            occt_bridge_shape_is_adjacent(
                self.raw.as_ptr(),
                parent.id,
                first.id,
                second.id,
                &mut adjacent,
            )
        })?;
        Ok(adjacent != 0)
    }

    pub fn is_same(&self, first: &Shape<'_>, second: &Shape<'_>) -> Result<bool, BridgeError> {
        self.validate_shape(first)?;
        self.validate_shape(second)?;
        let mut same = 0;
        // SAFETY: The session, validated handles, and output pointer are valid.
        self.check(unsafe {
            occt_bridge_shape_is_same(self.raw.as_ptr(), first.id, second.id, &mut same)
        })?;
        Ok(same != 0)
    }

    pub fn faces_are_tangent(
        &self,
        parent: &Shape<'_>,
        first_face: &Shape<'_>,
        second_face: &Shape<'_>,
    ) -> Result<bool, BridgeError> {
        self.validate_shape(parent)?;
        self.validate_shape(first_face)?;
        self.validate_shape(second_face)?;
        let mut tangent = 0;
        // SAFETY: The session, validated handles, and output pointer are valid.
        self.check(unsafe {
            occt_bridge_shape_faces_are_tangent(
                self.raw.as_ptr(),
                parent.id,
                first_face.id,
                second_face.id,
                &mut tangent,
            )
        })?;
        Ok(tangent != 0)
    }

    pub fn history_count(
        &self,
        result: &Shape<'_>,
        source: &Shape<'_>,
        relation: HistoryRelation,
    ) -> Result<usize, BridgeError> {
        self.validate_shape(result)?;
        self.validate_shape(source)?;
        let mut count = 0;
        // SAFETY: The session and output pointers are valid.
        self.check(unsafe {
            occt_bridge_shape_history_count(
                self.raw.as_ptr(),
                result.id,
                source.id,
                relation as c_int,
                &mut count,
            )
        })?;
        Ok(count)
    }

    pub fn history<'a>(
        &'a self,
        result: &Shape<'_>,
        source: &Shape<'_>,
        relation: HistoryRelation,
        index: usize,
    ) -> Result<Shape<'a>, BridgeError> {
        self.validate_shape(result)?;
        self.validate_shape(source)?;
        let mut history_shape = 0;
        // SAFETY: The session and output pointers are valid; the C layer checks the index.
        self.check(unsafe {
            occt_bridge_shape_history_at(
                self.raw.as_ptr(),
                result.id,
                source.id,
                relation as c_int,
                index,
                &mut history_shape,
            )
        })?;
        Ok(self.shape(history_shape))
    }

    pub fn history_is_deleted(
        &self,
        result: &Shape<'_>,
        source: &Shape<'_>,
    ) -> Result<bool, BridgeError> {
        self.validate_shape(result)?;
        self.validate_shape(source)?;
        let mut deleted = 0;
        // SAFETY: The session and output pointers are valid.
        self.check(unsafe {
            occt_bridge_shape_history_is_deleted(
                self.raw.as_ptr(),
                result.id,
                source.id,
                &mut deleted,
            )
        })?;
        Ok(deleted != 0)
    }

    pub fn is_valid(&self, shape: &Shape<'_>) -> Result<bool, BridgeError> {
        self.validate_shape(shape)?;
        let mut valid = 0;
        // SAFETY: The session and output pointers are valid.
        self.check(unsafe { occt_bridge_shape_is_valid(self.raw.as_ptr(), shape.id, &mut valid) })?;
        Ok(valid != 0)
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

    fn boolean<'a>(
        &'a self,
        left: &Shape<'_>,
        right: &Shape<'_>,
        operation: unsafe extern "C" fn(
            *mut c_void,
            RawShapeId,
            RawShapeId,
            *mut RawShapeId,
        ) -> RawStatus,
    ) -> Result<Shape<'a>, BridgeError> {
        self.validate_shape(left)?;
        self.validate_shape(right)?;
        let mut shape = 0;
        // SAFETY: All pointers and handles are passed to the validating C boundary.
        self.check(unsafe { operation(self.raw.as_ptr(), left.id, right.id, &mut shape) })?;
        Ok(self.shape(shape))
    }

    fn transform<'a>(
        &'a self,
        shape: &Shape<'_>,
        operation: impl FnOnce(*mut RawShapeId) -> RawStatus,
    ) -> Result<Shape<'a>, BridgeError> {
        self.validate_shape(shape)?;
        let mut transformed = 0;
        self.check(operation(&mut transformed))?;
        Ok(self.shape(transformed))
    }

    fn derived_shape<'a>(
        &'a self,
        input: &Shape<'_>,
        operation: impl FnOnce(*mut RawShapeId) -> RawStatus,
    ) -> Result<Shape<'a>, BridgeError> {
        self.validate_shape(input)?;
        let mut result = 0;
        self.check(operation(&mut result))?;
        Ok(self.shape(result))
    }

    fn selected_operation<'a>(
        &'a self,
        shape: &Shape<'_>,
        selections: &[&Shape<'_>],
        operation: impl FnOnce(&[RawShapeId], *mut RawShapeId) -> RawStatus,
    ) -> Result<Shape<'a>, BridgeError> {
        self.validate_shape(shape)?;
        for selection in selections {
            self.validate_shape(selection)?;
        }
        let ids: Vec<RawShapeId> = selections.iter().map(|selection| selection.id).collect();
        let mut result = 0;
        self.check(operation(&ids, &mut result))?;
        Ok(self.shape(result))
    }

    fn shape(&self, id: RawShapeId) -> Shape<'_> {
        Shape {
            id,
            owner: self.raw,
            generation: self.generation.get(),
            _session: PhantomData,
        }
    }

    fn validate_shape(&self, shape: &Shape<'_>) -> Result<(), BridgeError> {
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

    fn check(&self, status: RawStatus) -> Result<(), BridgeError> {
        if status == OK {
            Ok(())
        } else {
            Err(self.error(status))
        }
    }

    fn error(&self, status: RawStatus) -> BridgeError {
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
    fn read_text_at(
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
    fn read_text(
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

    fn error_without_session(status: RawStatus) -> BridgeError {
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

impl Drop for Session {
    fn drop(&mut self) {
        // SAFETY: This is the unique owning Session and Drop runs once.
        unsafe { occt_bridge_session_destroy(self.raw.as_ptr()) };
    }
}

fn path_to_c_string(path: &Path) -> Result<CString, BridgeError> {
    CString::new(path.to_string_lossy().as_bytes()).map_err(|_| BridgeError {
        status: 1,
        category: "invalid argument".into(),
        message: "path contains an interior NUL byte".into(),
        diagnostics: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::PathBuf};

    fn unit_box(session: &Session, x: f64) -> Shape<'_> {
        session
            .create_box(Vec3::new(x, 0.0, 0.0), Vec3::new(1.0, 1.0, 1.0))
            .unwrap()
    }

    fn step_test_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "occt-bridge-{name}-{}-{}.step",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ))
    }

    fn binary_stl_triangle_count(path: &Path) -> u32 {
        let bytes = fs::read(path).unwrap();
        assert!(bytes.len() >= 84);
        let count = u32::from_le_bytes(bytes[80..84].try_into().unwrap());
        assert_eq!(bytes.len(), 84 + 50 * count as usize);
        count
    }

    #[test]
    fn step_round_trip_preserves_geometry_and_topology() {
        let session = Session::new().unwrap();
        let source = session
            .create_box(Vec3::new(-2.0, 3.0, 5.0), Vec3::new(10.0, 20.0, 30.0))
            .unwrap();
        let path = step_test_path("round-trip");
        session.save_step(&source, &path).unwrap();
        let loaded = session.load_step(&path).unwrap();

        assert!(session.is_valid(&loaded).unwrap());
        assert_eq!(session.shape_type(&loaded).unwrap(), ShapeType::Solid);
        assert_eq!(
            session.subshape_count(&loaded, ShapeType::Face).unwrap(),
            session.subshape_count(&source, ShapeType::Face).unwrap()
        );
        assert_eq!(
            session.subshape_count(&loaded, ShapeType::Edge).unwrap(),
            session.subshape_count(&source, ShapeType::Edge).unwrap()
        );
        let source_bounds = session.bounds(&source).unwrap();
        let loaded_bounds = session.bounds(&loaded).unwrap();
        for (actual, expected) in [
            (loaded_bounds.min.x, source_bounds.min.x),
            (loaded_bounds.min.y, source_bounds.min.y),
            (loaded_bounds.min.z, source_bounds.min.z),
            (loaded_bounds.max.x, source_bounds.max.x),
            (loaded_bounds.max.y, source_bounds.max.y),
            (loaded_bounds.max.z, source_bounds.max.z),
        ] {
            assert!((actual - expected).abs() < 1e-6);
        }
        assert!((session.volume(&loaded).unwrap() - session.volume(&source).unwrap()).abs() < 1e-6);
        assert_eq!(session.shape_count().unwrap(), 2);

        session.remove(loaded).unwrap();
        session.remove(source).unwrap();
        assert_eq!(session.shape_count().unwrap(), 0);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn step_exchange_reports_io_and_session_errors_without_leaking_handles() {
        let session = Session::new().unwrap();
        let missing = step_test_path("missing");
        let _ = fs::remove_file(&missing);
        let error = session.load_step(&missing).unwrap_err();
        assert_eq!(error.status, 5);
        assert_eq!(error.category, "I/O error");
        assert_eq!(session.shape_count().unwrap(), 0);

        let malformed = step_test_path("malformed");
        fs::write(&malformed, b"not a STEP file\n").unwrap();
        let error = session.load_step(&malformed).unwrap_err();
        assert_eq!(error.status, 5);
        assert_eq!(session.shape_count().unwrap(), 0);
        fs::remove_file(malformed).unwrap();

        let first = Session::new().unwrap();
        let second = Session::new().unwrap();
        let shape = unit_box(&first, 0.0);
        let output = step_test_path("wrong-session");
        assert_wrong_session(second.save_step(&shape, &output).unwrap_err());
        assert!(!output.exists());
    }

    #[test]
    fn stl_export_writes_verified_ascii_and_tessellated_binary_meshes() {
        let session = Session::new().unwrap();
        let box_shape = session
            .create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 20.0, 30.0))
            .unwrap();
        let ascii_path = step_test_path("ascii-stl").with_extension("stl");
        session
            .save_stl(
                &box_shape,
                &ascii_path,
                StlOptions {
                    format: StlFormat::Ascii,
                    ..StlOptions::default()
                },
            )
            .unwrap();
        let ascii = fs::read_to_string(&ascii_path).unwrap();
        assert!(ascii.starts_with("solid"));
        assert_eq!(ascii.matches("facet normal").count(), 12);

        let sphere = session
            .create_sphere(Vec3::new(0.0, 0.0, 0.0), 10.0)
            .unwrap();
        let coarse_path = step_test_path("coarse-stl").with_extension("stl");
        let fine_path = step_test_path("fine-stl").with_extension("stl");
        session
            .save_stl(
                &sphere,
                &coarse_path,
                StlOptions {
                    linear_deflection: 2.0,
                    angular_deflection_radians: 1.0,
                    format: StlFormat::Binary,
                },
            )
            .unwrap();
        session
            .save_stl(
                &sphere,
                &fine_path,
                StlOptions {
                    linear_deflection: 0.1,
                    angular_deflection_radians: 0.2,
                    format: StlFormat::Binary,
                },
            )
            .unwrap();
        let coarse_triangles = binary_stl_triangle_count(&coarse_path);
        let fine_triangles = binary_stl_triangle_count(&fine_path);
        assert!(coarse_triangles > 0);
        assert!(fine_triangles > coarse_triangles);
        assert_eq!(session.shape_count().unwrap(), 2);

        // Each export meshes independently: a coarse export after a fine one
        // must not reuse the finer triangulation.
        let recoarse_path = step_test_path("recoarse-stl").with_extension("stl");
        session
            .save_stl(
                &sphere,
                &recoarse_path,
                StlOptions {
                    linear_deflection: 2.0,
                    angular_deflection_radians: 1.0,
                    format: StlFormat::Binary,
                },
            )
            .unwrap();
        assert_eq!(binary_stl_triangle_count(&recoarse_path), coarse_triangles);

        // Exporting leaves no triangulation on the session's shape.
        let brep_path = step_test_path("after-stl").with_extension("brep");
        session.save_brep(&sphere, &brep_path).unwrap();
        let brep = fs::read_to_string(&brep_path).unwrap();
        assert!(
            brep.lines()
                .filter(|line| line.starts_with("Triangulations"))
                .all(|line| line == "Triangulations 0")
        );

        fs::remove_file(ascii_path).unwrap();
        fs::remove_file(coarse_path).unwrap();
        fs::remove_file(fine_path).unwrap();
        fs::remove_file(recoarse_path).unwrap();
        fs::remove_file(brep_path).unwrap();
    }

    #[test]
    fn stl_export_rejects_invalid_options_paths_and_sessions() {
        let first = Session::new().unwrap();
        let second = Session::new().unwrap();
        let shape = unit_box(&first, 0.0);
        let output = step_test_path("invalid-stl").with_extension("stl");
        assert_wrong_session(
            second
                .save_stl(&shape, &output, StlOptions::default())
                .unwrap_err(),
        );
        assert!(!output.exists());

        for options in [
            StlOptions {
                linear_deflection: 0.0,
                ..StlOptions::default()
            },
            StlOptions {
                linear_deflection: f64::NAN,
                ..StlOptions::default()
            },
            StlOptions {
                angular_deflection_radians: -1.0,
                ..StlOptions::default()
            },
        ] {
            let error = first.save_stl(&shape, &output, options).unwrap_err();
            assert_eq!(error.status, 1);
            assert!(!output.exists());
        }
        let error = first
            .save_stl(
                &shape,
                "/nonexistent-directory/shape.stl",
                StlOptions::default(),
            )
            .unwrap_err();
        assert_eq!(error.status, 5);
        assert_eq!(first.shape_count().unwrap(), 1);
    }

    fn assert_wrong_session(error: BridgeError) {
        assert_eq!(error.status, 1);
        assert_eq!(error.category, "invalid argument");
        assert_eq!(error.message, "shape belongs to a different session");
    }

    fn assert_cleared(error: BridgeError) {
        assert_eq!(error.status, 3);
        assert_eq!(error.category, "shape not found");
        assert_eq!(
            error.message,
            "shape was invalidated by clearing its session"
        );
    }

    fn curvature_fixture_edge<'a>(session: &'a Session, index: usize) -> Shape<'a> {
        let compound = session
            .load_brep(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../tests/fixtures/curvature_edges.brep"
            ))
            .unwrap();
        session.subshape(&compound, ShapeType::Edge, index).unwrap()
    }

    fn assert_bounded(extrema: CurvatureExtrema, tolerance: f64) {
        assert!(!extrema.is_exact);
        assert!(extrema.minimum_lower_bound <= extrema.minimum);
        assert!(extrema.maximum <= extrema.maximum_upper_bound);
        let gap = tolerance * extrema.maximum;
        assert!(
            extrema.minimum - extrema.minimum_lower_bound <= gap,
            "{extrema:?}"
        );
        assert!(
            extrema.maximum_upper_bound - extrema.maximum <= gap,
            "{extrema:?}"
        );
    }

    #[test]
    fn curvature_extrema_bound_polynomial_and_rational_edges() {
        let session = Session::new().unwrap();
        let tolerance = 1e-6;

        let parabola = session
            .edge_curvature_extrema(&curvature_fixture_edge(&session, 0), tolerance)
            .unwrap();
        assert_bounded(parabola, tolerance);
        // y = x^2 on [-1, 1]: vertex curvature 2 is attained at the midpoint split.
        assert!((parabola.maximum - 2.0).abs() < 1e-12);
        let end_curvature = 2.0 / 5.0_f64.powf(1.5);
        assert!((parabola.minimum - end_curvature).abs() < 1e-12);

        let circle = session
            .edge_curvature_extrema(&curvature_fixture_edge(&session, 1), tolerance)
            .unwrap();
        assert_bounded(circle, tolerance);
        assert!((circle.minimum_lower_bound - 0.5).abs() <= 0.5 * tolerance);
        assert!((circle.maximum_upper_bound - 0.5).abs() <= 0.5 * tolerance);

        let spline_edge = curvature_fixture_edge(&session, 2);
        let spline = session
            .edge_curvature_extrema(&spline_edge, tolerance)
            .unwrap();
        assert_bounded(spline, tolerance);
        let (sampled_minimum, sampled_maximum) =
            session.edge_curvature_range(&spline_edge, 100_000).unwrap();
        assert!(spline.minimum_lower_bound <= sampled_minimum + 1e-12);
        assert!(sampled_maximum <= spline.maximum_upper_bound + 1e-12);
        assert!(spline.minimum <= sampled_minimum + tolerance * spline.maximum);
        assert!(spline.maximum >= sampled_maximum - tolerance * spline.maximum);

        let arc = session
            .edge_curvature_extrema(&curvature_fixture_edge(&session, 3), tolerance)
            .unwrap();
        assert!(arc.is_exact);
        let ellipse = |t: f64| 8.0 / (16.0 * t.sin().powi(2) + 4.0 * t.cos().powi(2)).powf(1.5);
        assert!((arc.minimum - 0.125).abs() < 1e-12);
        assert!((arc.maximum - ellipse(0.3).max(ellipse(2.0))).abs() < 1e-12);

        let straight = session
            .edge_curvature_extrema(&curvature_fixture_edge(&session, 4), tolerance)
            .unwrap();
        assert!(!straight.is_exact);
        assert!(straight.maximum_upper_bound <= 1e-10);
    }

    #[test]
    fn curvature_extrema_are_exact_for_parabola_and_hyperbola_edges() {
        let session = Session::new().unwrap();
        let assert_exact = |extrema: CurvatureExtrema, minimum: f64, maximum: f64| {
            assert!(extrema.is_exact, "{extrema:?}");
            assert_eq!(extrema.minimum, extrema.minimum_lower_bound);
            assert_eq!(extrema.maximum, extrema.maximum_upper_bound);
            assert!((extrema.minimum - minimum).abs() < 1e-12, "{extrema:?}");
            assert!((extrema.maximum - maximum).abs() < 1e-12, "{extrema:?}");
        };

        // Focal 0.5 on u in [-1, 2]: the vertex (u = 0) is interior and the
        // flattest point is the far end u = 2.
        let parabola = session
            .edge_curvature_extrema(&curvature_fixture_edge(&session, 5), 1e-6)
            .unwrap();
        assert_exact(parabola, 1.0 / 5.0_f64.powf(1.5), 1.0);

        // a = 3, b = 2 on u in [-0.5, 1]: vertex curvature a / b^2, flattest at u = 1.
        let hyperbola = session
            .edge_curvature_extrema(&curvature_fixture_edge(&session, 6), 1e-6)
            .unwrap();
        let curvature = |u: f64| 6.0 / (9.0 * u.sinh().powi(2) + 4.0 * u.cosh().powi(2)).powf(1.5);
        assert_exact(hyperbola, curvature(1.0), 0.75);
    }

    #[test]
    fn sewing_independent_faces_builds_a_closed_solid() {
        let session = Session::new().unwrap();
        let corner = |x, y, z| Vec3::new(x, y, z);
        let square = |points: [Vec3; 4]| {
            let wire = session.create_polyline_wire(&points, true).unwrap();
            session.create_face_from_wire(&wire).unwrap()
        };
        let (p000, p100, p110, p010) = (
            corner(0.0, 0.0, 0.0),
            corner(2.0, 0.0, 0.0),
            corner(2.0, 3.0, 0.0),
            corner(0.0, 3.0, 0.0),
        );
        let (p001, p101, p111, p011) = (
            corner(0.0, 0.0, 4.0),
            corner(2.0, 0.0, 4.0),
            corner(2.0, 3.0, 4.0),
            corner(0.0, 3.0, 4.0),
        );
        let faces = [
            square([p000, p010, p110, p100]),
            square([p001, p101, p111, p011]),
            square([p000, p100, p101, p001]),
            square([p010, p011, p111, p110]),
            square([p000, p001, p011, p010]),
            square([p100, p110, p111, p101]),
        ];
        let face_refs = faces.iter().collect::<Vec<_>>();

        let shell = session.sew(&face_refs, 1e-6).unwrap();
        assert_eq!(session.shape_type(&shell).unwrap(), ShapeType::Shell);
        assert_eq!(session.subshape_count(&shell, ShapeType::Edge).unwrap(), 12);
        assert_eq!(
            session
                .history_count(&shell, &faces[2], HistoryRelation::Modified)
                .unwrap(),
            1
        );
        let solid = session.make_solid(&shell).unwrap();
        assert!((session.volume(&solid).unwrap() - 24.0).abs() < 1e-9);
        assert!(session.is_valid(&solid).unwrap());

        // Without the last face the shell stays open; far-apart faces do not join.
        let open = session.sew(&face_refs[..5], 1e-6).unwrap();
        assert_eq!(session.make_solid(&open).unwrap_err().status, 4);
        let apart = session
            .translate(&faces[1], Vec3::new(0.0, 0.0, 10.0))
            .unwrap();
        let loose = session.sew(&[&faces[0], &apart], 1e-6).unwrap();
        assert_eq!(session.make_solid(&loose).unwrap_err().status, 4);
        assert_eq!(session.sew(&[], 1e-6).unwrap_err().status, 1);
        assert_eq!(session.sew(&face_refs, 0.0).unwrap_err().status, 1);
    }

    #[test]
    fn multiple_shells_build_a_solid_with_an_internal_void() {
        let session = Session::new().unwrap();
        let outer = session
            .create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 10.0, 10.0))
            .unwrap();
        let void = session
            .create_box(Vec3::new(2.0, 2.0, 2.0), Vec3::new(2.0, 2.0, 2.0))
            .unwrap();
        let solid = session.make_solid_from_shells(&[&void, &outer]).unwrap();
        assert_eq!(session.shape_type(&solid).unwrap(), ShapeType::Solid);
        assert_eq!(session.subshape_count(&solid, ShapeType::Shell).unwrap(), 2);
        assert!((session.volume(&solid).unwrap() - 992.0).abs() < 1e-9);
        assert!(session.is_valid(&solid).unwrap());

        let crossing = session
            .create_box(Vec3::new(9.0, 9.0, 9.0), Vec3::new(2.0, 2.0, 2.0))
            .unwrap();
        assert_eq!(
            session
                .make_solid_from_shells(&[&outer, &crossing])
                .unwrap_err()
                .status,
            4
        );
        assert_eq!(session.make_solid_from_shells(&[]).unwrap_err().status, 1);
    }

    #[test]
    fn exact_bounds_follow_geometry_without_tolerance_padding() {
        let session = Session::new().unwrap();
        let box_shape = session
            .create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(120.0, 20.0, 30.0))
            .unwrap();
        let padded = session.bounds(&box_shape).unwrap();
        let exact = session.exact_bounds(&box_shape).unwrap();
        assert!(padded.max.x - padded.min.x > 120.0);
        assert!((exact.max.x - exact.min.x - 120.0).abs() < 1e-12);
        assert!((exact.max.z - exact.min.z - 30.0).abs() < 1e-12);

        let cylinder = session
            .create_cylinder(Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 1.0), 2.0, 5.0)
            .unwrap();
        let exact = session.exact_bounds(&cylinder).unwrap();
        assert!((exact.max.z - exact.min.z - 5.0).abs() < 1e-9);
        assert!((exact.max.x - exact.min.x - 4.0).abs() < 1e-9);
    }

    fn fixture(name: &str) -> String {
        format!("{}/../../tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))
    }

    #[test]
    fn session_options_round_trip_and_reject_invalid_values() {
        let session = Session::new().unwrap();
        assert_eq!(session.options().unwrap(), SessionOptions::default());
        let healing = SessionOptions {
            heal_invalid_results: true,
            boolean_fuzzy_tolerance: 1e-5,
            ..SessionOptions::default()
        };
        session.set_options(healing).unwrap();
        assert_eq!(session.options().unwrap(), healing);
        for invalid in [
            SessionOptions {
                boolean_fuzzy_tolerance: -1.0,
                ..healing
            },
            SessionOptions {
                boolean_fuzzy_tolerance: f64::NAN,
                ..healing
            },
            SessionOptions {
                validate_results: false,
                ..healing
            },
        ] {
            assert_eq!(session.set_options(invalid).unwrap_err().status, 1);
        }
        assert_eq!(session.options().unwrap(), healing);
    }

    #[test]
    fn invalid_results_are_rejected_healed_or_allowed_by_option() {
        let session = Session::new().unwrap();
        let error = session.load_brep(fixture("bowtie_face.brep")).unwrap_err();
        assert_eq!(error.status, 4);
        assert!(
            error
                .message
                .contains("BREP load produced an invalid shape"),
            "{error}"
        );
        // OCCT's STEP reader heals during transfer, so the same defect
        // arrives valid; validation still guards what it produces.
        let imported = session.load_step(fixture("bowtie_face.step")).unwrap();
        assert!(session.is_valid(&imported).unwrap());

        session
            .set_options(SessionOptions {
                validate_results: false,
                ..SessionOptions::default()
            })
            .unwrap();
        let unchecked = session.load_brep(fixture("bowtie_face.brep")).unwrap();
        assert!(!session.is_valid(&unchecked).unwrap());

        session
            .set_options(SessionOptions {
                heal_invalid_results: true,
                ..SessionOptions::default()
            })
            .unwrap();
        let healed = session.load_brep(fixture("bowtie_face.brep")).unwrap();
        assert_eq!(
            session.last_warnings(),
            ["BREP load result was invalid and was healed"]
        );
        assert!(session.is_valid(&healed).unwrap());
        session.shape_count().unwrap();
        assert!(
            session.last_warnings().is_empty(),
            "warnings clear on the next call"
        );

        let error = session.load_brep(fixture("gapped_face.brep")).unwrap_err();
        assert!(
            error.message.contains("healing could not repair"),
            "{error}"
        );
    }

    #[test]
    fn kernel_failures_name_their_cause_and_culprit() {
        let session = Session::new().unwrap();
        let block = session
            .create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 10.0, 10.0))
            .unwrap();
        let first = session.subshape(&block, ShapeType::Edge, 0).unwrap();
        let second = session.subshape(&block, ShapeType::Edge, 3).unwrap();
        let error = session
            .fillet(&block, &[&first, &second], 20.0)
            .unwrap_err();
        assert_eq!(error.status, 6);
        assert_eq!(error.diagnostics.len(), 2);
        let selected = error
            .diagnostics
            .iter()
            .map(|diagnostic| {
                assert_eq!(diagnostic.kind, DiagnosticKind::FilletEdge);
                assert_eq!(diagnostic.name, "ChFiDS_StartsolFailure");
                assert!(diagnostic.has_shape);
                diagnostic.input_index.unwrap()
            })
            .collect::<Vec<_>>();
        assert_eq!(selected, vec![0, 1]);
        assert!(
            error
                .message
                .contains("ChFiDS_StartsolFailure on selection 0 and 1 more")
        );
        let culprit = session.last_diagnostic_shape(1).unwrap().unwrap();
        assert_eq!(session.last_diagnostics(), error.diagnostics);
        assert!(session.is_same(&culprit, &second).unwrap());
        assert!(session.last_diagnostics().is_empty());
        assert!(session.last_diagnostic_shape(0).is_err());

        let error = session.fillet(&block, &[&first], 1.0).map(|_| ());
        assert!(error.is_ok());
        assert!(session.last_diagnostics().is_empty());
    }

    #[test]
    fn healing_carries_operation_history_and_booleans_report_warnings() {
        let session = Session::new().unwrap();
        session
            .set_options(SessionOptions {
                validate_results: false,
                ..SessionOptions::default()
            })
            .unwrap();
        let bowtie = session.load_brep(fixture("bowtie_face.brep")).unwrap();
        let shifted = session
            .translate(&bowtie, Vec3::new(1.0, 0.0, 0.0))
            .unwrap();
        session
            .set_options(SessionOptions {
                heal_invalid_results: true,
                ..SessionOptions::default()
            })
            .unwrap();

        // Sewing two overlapping bow-ties yields an invalid shell that
        // healing repairs; history must lead from each input face to the
        // repaired result.
        let sewn = session.sew(&[&bowtie, &shifted], 1e-3).unwrap();
        // Warnings describe the most recent call, so read them first.
        assert_eq!(
            session.last_warnings(),
            ["sewing result was invalid and was healed"]
        );
        assert!(session.is_valid(&sewn).unwrap());
        let input_face = session.subshape(&bowtie, ShapeType::Face, 0).unwrap();
        let healed_faces = session
            .history_count(&sewn, &input_face, HistoryRelation::Modified)
            .unwrap();
        assert!(healed_faces >= 1);
        for index in 0..healed_faces {
            let face = session
                .history(&sewn, &input_face, HistoryRelation::Modified, index)
                .unwrap();
            assert_eq!(session.shape_type(&face).unwrap(), ShapeType::Face);
            assert!(session.is_adjacent(&sewn, &face, &face).is_ok());
        }

        // OCCT boolean alerts surface as warnings, one per line with the
        // operation name, instead of being dropped.
        let block = session
            .create_box(Vec3::new(2.0, 2.0, -1.0), Vec3::new(3.0, 3.0, 2.0))
            .unwrap();
        session.common(&bowtie, &block).unwrap();
        let warnings = session.last_warnings();
        assert!(!warnings.is_empty());
        assert!(
            warnings
                .iter()
                .all(|warning| warning.starts_with("common: ")),
            "{warnings:?}"
        );
        let error = session.fuse(&bowtie, &block).unwrap_err();
        assert!(error.message.contains("BOPAlgo_Alert"), "{error}");
    }

    #[test]
    fn fuzzy_booleans_merge_nearly_touching_inputs() {
        let session = Session::new().unwrap();
        let left = session
            .create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 10.0, 10.0))
            .unwrap();
        let right = session
            .create_box(
                Vec3::new(10.0 + 1e-6, 0.0, 0.0),
                Vec3::new(10.0, 10.0, 10.0),
            )
            .unwrap();
        let exact = session.fuse(&left, &right).unwrap();
        assert_eq!(session.subshape_count(&exact, ShapeType::Solid).unwrap(), 2);
        session
            .set_options(SessionOptions {
                boolean_fuzzy_tolerance: 1e-5,
                ..SessionOptions::default()
            })
            .unwrap();
        let fuzzy = session.fuse(&left, &right).unwrap();
        assert_eq!(session.subshape_count(&fuzzy, ShapeType::Solid).unwrap(), 1);
        assert_eq!(session.subshape_count(&fuzzy, ShapeType::Face).unwrap(), 10);
        assert!(session.is_valid(&fuzzy).unwrap());
    }

    #[test]
    fn dropped_handles_release_their_shapes_without_losing_diagnostics() {
        let session = Session::new().unwrap();
        let block = session
            .create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 1.0, 1.0))
            .unwrap();
        {
            let _temporary = session.translate(&block, Vec3::new(5.0, 0.0, 0.0)).unwrap();
            let _faces = (0..6)
                .map(|index| session.subshape(&block, ShapeType::Face, index).unwrap())
                .collect::<Vec<_>>();
            assert_eq!(session.shape_count().unwrap(), 8);
        }
        assert_eq!(session.shape_count().unwrap(), 1);

        // Two handles to one geometry are independent.
        let copy = session.duplicate(&block).unwrap();
        drop(copy);
        assert!(session.is_valid(&block).unwrap());

        // Releasing temporaries between a call and reading its diagnostics
        // keeps both the warnings and the last error.
        session
            .set_options(SessionOptions {
                validate_results: false,
                ..SessionOptions::default()
            })
            .unwrap();
        let bowtie = session.load_brep(fixture("bowtie_face.brep")).unwrap();
        let cutter = session
            .create_box(Vec3::new(2.0, 2.0, -1.0), Vec3::new(3.0, 3.0, 2.0))
            .unwrap();
        let common = session.common(&bowtie, &cutter).unwrap();
        drop(common);
        drop(cutter);
        assert!(!session.last_warnings().is_empty());

        // Dropping handles invalidated by clearing is harmless.
        let stale = session.duplicate(&block).unwrap();
        session.clear().unwrap();
        drop(stale);
        drop(block);
        drop(bowtie);
        assert_eq!(session.shape_count().unwrap(), 0);
    }

    #[test]
    fn rigid_moves_share_geometry_and_keep_history() {
        let session = Session::new().unwrap();
        let block = session
            .create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 20.0, 30.0))
            .unwrap();
        let turned = session
            .rotate(
                &block,
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(0.0, 0.0, 1.0),
                std::f64::consts::FRAC_PI_2,
            )
            .unwrap();
        let placed = session
            .translate(&turned, Vec3::new(100.0, 0.0, 0.0))
            .unwrap();
        let bounds = session.exact_bounds(&placed).unwrap();
        assert!((bounds.min.x - 80.0).abs() < 1e-9 && (bounds.max.x - 100.0).abs() < 1e-9);

        // Each source face maps to exactly one moved face of the result.
        for index in 0..6 {
            let source = session.subshape(&turned, ShapeType::Face, index).unwrap();
            assert_eq!(
                session
                    .history_count(&placed, &source, HistoryRelation::Modified)
                    .unwrap(),
                1
            );
            let moved = session
                .history(&placed, &source, HistoryRelation::Modified, 0)
                .unwrap();
            assert!(session.is_adjacent(&placed, &moved, &moved).is_ok());
            assert!(!session.is_same(&moved, &source).unwrap());
            let source_area = session.surface_area(&source).unwrap();
            assert!((session.surface_area(&moved).unwrap() - source_area).abs() < 1e-9);
            assert!(!session.history_is_deleted(&placed, &source).unwrap());
        }
        let stranger = session
            .create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 1.0, 1.0))
            .unwrap();
        assert_eq!(
            session
                .history_count(&placed, &stranger, HistoryRelation::Modified)
                .unwrap_err()
                .status,
            1
        );

        // Copies sharing geometry stay independent: cutting one leaves the
        // original and the other copy unchanged.
        let tool = session
            .create_box(Vec3::new(85.0, 5.0, -1.0), Vec3::new(5.0, 5.0, 40.0))
            .unwrap();
        let cut = session.cut(&placed, &tool).unwrap();
        assert!(session.volume(&cut).unwrap() < 6000.0 - 1e-6);
        assert!((session.volume(&placed).unwrap() - 6000.0).abs() < 1e-6);
        assert!((session.volume(&block).unwrap() - 6000.0).abs() < 1e-6);

        // Scaling cannot be a location; it copies geometry and keeps
        // explicit history.
        let scaled = session
            .scale(&block, Vec3::new(0.0, 0.0, 0.0), 2.0)
            .unwrap();
        assert!((session.volume(&scaled).unwrap() - 48_000.0).abs() < 1e-6);
        let face = session.subshape(&block, ShapeType::Face, 0).unwrap();
        assert_eq!(
            session
                .history_count(&scaled, &face, HistoryRelation::Modified)
                .unwrap(),
            1
        );
    }

    #[test]
    fn box_face_and_edge_adjacency_follows_shared_topology() {
        let session = Session::new().unwrap();
        let box_shape = session
            .create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 2.0, 3.0))
            .unwrap();
        let subshapes = |kind| {
            (0..session.subshape_count(&box_shape, kind).unwrap())
                .map(|index| session.subshape(&box_shape, kind, index).unwrap())
                .collect::<Vec<_>>()
        };
        let adjacent_pairs = |shapes: &[Shape<'_>]| {
            let mut count = 0;
            for (index, first) in shapes.iter().enumerate() {
                assert!(!session.is_adjacent(&box_shape, first, first).unwrap());
                for second in &shapes[index + 1..] {
                    let forward = session.is_adjacent(&box_shape, first, second).unwrap();
                    assert_eq!(
                        forward,
                        session.is_adjacent(&box_shape, second, first).unwrap()
                    );
                    count += usize::from(forward);
                }
            }
            count
        };

        // Each of the 12 edges joins exactly two faces; only the 3 opposite
        // face pairs share nothing.
        let faces = subshapes(ShapeType::Face);
        assert_eq!(faces.len(), 6);
        assert_eq!(adjacent_pairs(&faces), 12);

        // Three edges meet at each of the 8 corners: 8 * C(3, 2) pairs.
        let edges = subshapes(ShapeType::Edge);
        assert_eq!(edges.len(), 12);
        assert_eq!(adjacent_pairs(&edges), 24);

        let vertex = session.subshape(&box_shape, ShapeType::Vertex, 0).unwrap();
        assert_eq!(
            session
                .is_adjacent(&box_shape, &faces[0], &vertex)
                .unwrap_err()
                .status,
            1
        );
    }

    #[test]
    fn constructs_and_inspects_shapes() {
        let session = Session::new().unwrap();
        let block = session
            .create_box(Vec3::new(1.0, 2.0, 3.0), Vec3::new(10.0, 20.0, 30.0))
            .unwrap();
        assert!(session.is_valid(&block).unwrap());
        let bounds = session.bounds(&block).unwrap();
        let close = |left: f64, right: f64| (left - right).abs() < 1e-6;
        assert!(close(bounds.min.x, 1.0));
        assert!(close(bounds.min.y, 2.0));
        assert!(close(bounds.min.z, 3.0));
        assert!(close(bounds.max.x, 11.0));
        assert!(close(bounds.max.y, 22.0));
        assert!(close(bounds.max.z, 33.0));

        let stone = session
            .create_polygon_prism(
                &[
                    Vec3::new(0.0, 0.0, 0.0),
                    Vec3::new(8.0, -1.0, 0.0),
                    Vec3::new(11.0, 5.0, 0.0),
                    Vec3::new(5.0, 10.0, 0.0),
                    Vec3::new(-2.0, 6.0, 0.0),
                ],
                Vec3::new(0.0, 0.0, 4.0),
            )
            .unwrap();
        assert!(session.is_valid(&stone).unwrap());
        assert_eq!(session.shape_count().unwrap(), 2);
    }

    #[test]
    fn constructs_generic_round_primitives() {
        let session = Session::new().unwrap();
        let cylinder = session
            .create_cylinder(Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 1.0), 2.0, 5.0)
            .unwrap();
        let cone = session
            .create_cone(
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(0.0, 1.0, 0.0),
                3.0,
                1.0,
                8.0,
            )
            .unwrap();
        let sphere = session
            .create_sphere(Vec3::new(5.0, 6.0, 7.0), 4.0)
            .unwrap();

        assert!(session.is_valid(&cylinder).unwrap());
        assert!(session.is_valid(&cone).unwrap());
        assert!(session.is_valid(&sphere).unwrap());
        let bounds = session.bounds(&sphere).unwrap();
        assert!((bounds.min.x - 1.0).abs() < 1e-6);
        assert!((bounds.max.z - 11.0).abs() < 1e-6);

        let error = session
            .create_cylinder(Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 0.0), 1.0, 1.0)
            .unwrap_err();
        assert_eq!(error.status, 1);
        assert_eq!(error.message, "cylinder axis must be nonzero");
    }

    #[test]
    fn transforms_create_new_shapes_without_modifying_the_source() {
        let session = Session::new().unwrap();
        let source = session
            .create_box(Vec3::new(1.0, 2.0, 3.0), Vec3::new(10.0, 20.0, 30.0))
            .unwrap();
        let translated = session
            .translate(&source, Vec3::new(100.0, -2.0, 7.0))
            .unwrap();
        let rotated = session
            .rotate(
                &source,
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(0.0, 0.0, 1.0),
                std::f64::consts::FRAC_PI_2,
            )
            .unwrap();
        let scaled = session
            .scale(&source, Vec3::new(0.0, 0.0, 0.0), 2.0)
            .unwrap();

        let source_bounds = session.bounds(&source).unwrap();
        let translated_bounds = session.bounds(&translated).unwrap();
        let rotated_bounds = session.bounds(&rotated).unwrap();
        let scaled_bounds = session.bounds(&scaled).unwrap();
        assert!((source_bounds.min.x - 1.0).abs() < 1e-6);
        assert!((source_bounds.max.z - 33.0).abs() < 1e-6);
        assert!((translated_bounds.min.x - 101.0).abs() < 1e-6);
        assert!((translated_bounds.min.y - 0.0).abs() < 1e-6);
        assert!((translated_bounds.min.z - 10.0).abs() < 1e-6);
        assert!((rotated_bounds.min.x - -22.0).abs() < 1e-6);
        assert!((rotated_bounds.max.y - 11.0).abs() < 1e-6);
        assert!((scaled_bounds.min.x - 2.0).abs() < 1e-6);
        assert!((scaled_bounds.max.z - 66.0).abs() < 1e-6);
        assert_eq!(session.shape_count().unwrap(), 4);
    }

    #[test]
    fn tracks_modified_subshapes_through_operation_history() {
        let session = Session::new().unwrap();
        let source = session
            .create_box(Vec3::new(1.0, 2.0, 3.0), Vec3::new(10.0, 20.0, 30.0))
            .unwrap();
        let source_face = session.subshape(&source, ShapeType::Face, 0).unwrap();
        let offset = Vec3::new(100.0, -2.0, 7.0);
        let translated = session.translate(&source, offset).unwrap();

        assert_eq!(
            session
                .history_count(&translated, &source_face, HistoryRelation::Generated)
                .unwrap(),
            0
        );
        assert_eq!(
            session
                .history_count(&translated, &source_face, HistoryRelation::Modified)
                .unwrap(),
            1
        );
        assert!(
            !session
                .history_is_deleted(&translated, &source_face)
                .unwrap()
        );
        let translated_face = session
            .history(&translated, &source_face, HistoryRelation::Modified, 0)
            .unwrap();
        assert_eq!(
            session.shape_type(&translated_face).unwrap(),
            ShapeType::Face
        );
        let before = session.bounds(&source_face).unwrap();
        let after = session.bounds(&translated_face).unwrap();
        assert!((after.min.x - before.min.x - offset.x).abs() < 1e-6);
        assert!((after.min.y - before.min.y - offset.y).abs() < 1e-6);
        assert!((after.min.z - before.min.z - offset.z).abs() < 1e-6);
        assert_eq!(
            session
                .history(&translated, &source_face, HistoryRelation::Modified, 1,)
                .unwrap_err()
                .status,
            1
        );
        assert_eq!(
            session
                .history_count(&source, &source_face, HistoryRelation::Modified)
                .unwrap_err()
                .status,
            1
        );
        assert_eq!(session.shape_count().unwrap(), 4);
    }

    #[test]
    fn rejects_invalid_generic_primitive_and_transform_parameters() {
        let session = Session::new().unwrap();
        let source = unit_box(&session, 0.0);

        assert_eq!(
            session
                .create_cone(
                    Vec3::new(0.0, 0.0, 0.0),
                    Vec3::new(0.0, 0.0, 1.0),
                    0.0,
                    0.0,
                    1.0,
                )
                .unwrap_err()
                .status,
            1
        );
        assert_eq!(
            session
                .create_sphere(Vec3::new(0.0, 0.0, 0.0), 0.0)
                .unwrap_err()
                .status,
            1
        );
        assert_eq!(
            session
                .translate(&source, Vec3::new(f64::NAN, 0.0, 0.0))
                .unwrap_err()
                .status,
            1
        );
        assert_eq!(
            session
                .rotate(
                    &source,
                    Vec3::new(0.0, 0.0, 0.0),
                    Vec3::new(0.0, 0.0, 0.0),
                    1.0,
                )
                .unwrap_err()
                .status,
            1
        );
        assert_eq!(
            session
                .scale(&source, Vec3::new(0.0, 0.0, 0.0), 0.0)
                .unwrap_err()
                .status,
            1
        );
        assert_eq!(session.shape_count().unwrap(), 1);
        assert!(session.is_valid(&source).unwrap());
    }

    #[test]
    fn mixed_segment_wires_preserve_exact_arcs() {
        let session = Session::new().unwrap();
        // Both orientations and a major arc: area of the circular segment.
        for (middle, end, area) in [
            (
                Vec3::new(0.0, 1.0, 0.0),
                Vec3::new(-1.0, 0.0, 0.0),
                std::f64::consts::PI / 2.0,
            ),
            (
                Vec3::new(0.0, -1.0, 0.0),
                Vec3::new(-1.0, 0.0, 0.0),
                std::f64::consts::PI / 2.0,
            ),
            (
                Vec3::new(-1.0, 0.0, 0.0),
                Vec3::new(0.0, -1.0, 0.0),
                3.0 * std::f64::consts::PI / 4.0 + 0.5,
            ),
        ] {
            let start = Vec3::new(1.0, 0.0, 0.0);
            let wire = session
                .create_segment_wire(
                    &[
                        WireSegment::Arc { start, middle, end },
                        WireSegment::Line {
                            start: end,
                            end: start,
                        },
                    ],
                    true,
                )
                .unwrap();
            let face = session.create_face_from_wire(&wire).unwrap();
            assert!(session.is_valid(&face).unwrap());
            assert!((session.surface_area(&face).unwrap() - area).abs() < 1e-9);
            let prism = session
                .create_prism_from_face(&face, Vec3::new(0.0, 0.0, 3.0))
                .unwrap();
            assert!((session.volume(&prism).unwrap() - 3.0 * area).abs() < 1e-9);
        }
        assert_eq!(session.shape_count().unwrap(), 0);
    }

    #[test]
    fn invalid_segment_wires_do_not_leak_handles() {
        let session = Session::new().unwrap();
        assert!(session.create_segment_wire(&[], false).is_err());
        let start = Vec3::new(1.0, 0.0, 0.0);
        let end = Vec3::new(-1.0, 0.0, 0.0);
        let arc = WireSegment::Arc {
            start,
            middle: Vec3::new(0.0, 1.0, 0.0),
            end,
        };
        assert!(session.create_segment_wire(&[arc], true).is_err());
        assert!(
            session
                .create_segment_wire(&[arc, WireSegment::Line { start, end }], false)
                .is_err()
        );
        assert!(
            session
                .create_segment_wire(
                    &[WireSegment::Arc {
                        start,
                        middle: Vec3::new(0.0, 0.0, 0.0),
                        end
                    }],
                    false
                )
                .is_err()
        );
        let open = session.create_segment_wire(&[arc], false).unwrap();
        assert!(session.is_valid(&open).unwrap());
        drop(open);
        assert_eq!(session.shape_count().unwrap(), 0);
    }

    #[test]
    fn reuses_wire_and_face_handles_to_build_geometry() {
        let session = Session::new().unwrap();
        let rectangle = [
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(6.0, 0.0, 0.0),
            Vec3::new(6.0, 4.0, 0.0),
            Vec3::new(0.0, 4.0, 0.0),
        ];
        let wire = session.create_polyline_wire(&rectangle, true).unwrap();
        let face = session.create_face_from_wire(&wire).unwrap();
        let source_edge = session.subshape(&face, ShapeType::Edge, 0).unwrap();
        let prism = session
            .create_prism_from_face(&face, Vec3::new(0.0, 0.0, 3.0))
            .unwrap();
        let circle = session
            .create_circle_wire(Vec3::new(10.0, 20.0, 30.0), Vec3::new(0.0, 0.0, 1.0), 5.0)
            .unwrap();
        let disk = session.create_face_from_wire(&circle).unwrap();

        assert!(session.is_valid(&wire).unwrap());
        assert!(session.is_valid(&face).unwrap());
        assert!(session.is_valid(&prism).unwrap());
        assert!(session.is_valid(&circle).unwrap());
        assert!(session.is_valid(&disk).unwrap());
        assert_eq!(
            session
                .history_count(&prism, &source_edge, HistoryRelation::Generated)
                .unwrap(),
            1
        );
        let generated_face = session
            .history(&prism, &source_edge, HistoryRelation::Generated, 0)
            .unwrap();
        assert_eq!(
            session.shape_type(&generated_face).unwrap(),
            ShapeType::Face
        );
        let prism_bounds = session.bounds(&prism).unwrap();
        assert!((prism_bounds.max.x - 6.0).abs() < 1e-6);
        assert!((prism_bounds.max.y - 4.0).abs() < 1e-6);
        assert!((prism_bounds.max.z - 3.0).abs() < 1e-6);
        let disk_bounds = session.bounds(&disk).unwrap();
        assert!((disk_bounds.min.x - 5.0).abs() < 1e-6);
        assert!((disk_bounds.max.y - 25.0).abs() < 1e-6);
        assert_eq!(session.shape_count().unwrap(), 7);
    }

    #[test]
    fn rejects_invalid_wire_and_face_inputs_without_creating_shapes() {
        let session = Session::new().unwrap();
        let box_shape = unit_box(&session, 0.0);
        let repeated_endpoint = [
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 0.0, 0.0),
        ];

        assert_eq!(
            session
                .create_polyline_wire(&repeated_endpoint, true)
                .unwrap_err()
                .status,
            1
        );
        assert_eq!(
            session
                .create_face_from_wire(&box_shape)
                .unwrap_err()
                .status,
            4
        );
        assert_eq!(
            session
                .create_prism_from_face(&box_shape, Vec3::new(0.0, 0.0, 1.0))
                .unwrap_err()
                .status,
            4
        );
        assert_eq!(session.shape_count().unwrap(), 1);
    }

    #[test]
    fn traverses_topology_and_measures_solid_geometry() {
        let session = Session::new().unwrap();
        let shape = session
            .create_box(Vec3::new(1.0, 2.0, 3.0), Vec3::new(10.0, 20.0, 30.0))
            .unwrap();

        assert_eq!(session.shape_type(&shape).unwrap(), ShapeType::Solid);
        assert_eq!(session.subshape_count(&shape, ShapeType::Face).unwrap(), 6);
        assert_eq!(session.subshape_count(&shape, ShapeType::Edge).unwrap(), 12);
        assert_eq!(
            session.subshape_count(&shape, ShapeType::Vertex).unwrap(),
            8
        );

        let face = session.subshape(&shape, ShapeType::Face, 0).unwrap();
        assert_eq!(session.shape_type(&face).unwrap(), ShapeType::Face);
        assert!(session.surface_area(&face).unwrap() > 0.0);
        assert!((session.surface_area(&shape).unwrap() - 2200.0).abs() < 1e-9);
        assert!((session.volume(&shape).unwrap() - 6000.0).abs() < 1e-9);
        let center = session.center_of_mass(&shape).unwrap();
        assert!((center.x - 6.0).abs() < 1e-9);
        assert!((center.y - 12.0).abs() < 1e-9);
        assert!((center.z - 18.0).abs() < 1e-9);
        assert_eq!(
            session
                .subshape(&shape, ShapeType::Face, 6)
                .unwrap_err()
                .status,
            1
        );
        assert_eq!(session.shape_count().unwrap(), 2);
    }

    #[test]
    fn reports_oriented_face_normals_and_topology_adjacency() {
        let session = Session::new().unwrap();
        let shape = session
            .create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 20.0, 30.0))
            .unwrap();
        let faces = (0..session.subshape_count(&shape, ShapeType::Face).unwrap())
            .map(|index| session.subshape(&shape, ShapeType::Face, index).unwrap())
            .collect::<Vec<_>>();
        let edges = (0..session.subshape_count(&shape, ShapeType::Edge).unwrap())
            .map(|index| session.subshape(&shape, ShapeType::Edge, index).unwrap())
            .collect::<Vec<_>>();
        let top = faces
            .iter()
            .find(|face| session.face_normal(face).unwrap().z > 0.999)
            .unwrap();

        let normal = session.face_normal(top).unwrap();
        assert!(normal.x.abs() < 1e-9);
        assert!(normal.y.abs() < 1e-9);
        assert!((normal.z - 1.0).abs() < 1e-9);
        assert_eq!(
            edges
                .iter()
                .filter(|edge| session.is_adjacent(&shape, top, edge).unwrap())
                .count(),
            4
        );
        assert_eq!(session.face_normal(&shape).unwrap_err().status, 4);
        assert!(session.is_same(top, top).unwrap());
        assert!(!session.is_same(&faces[0], &faces[1]).unwrap());
        assert_eq!(session.shape_count().unwrap(), 19);
    }

    #[test]
    fn reports_planarity_edge_length_and_circular_radius() {
        let session = Session::new().unwrap();
        let box_shape = session
            .create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 20.0, 30.0))
            .unwrap();
        let face = session.subshape(&box_shape, ShapeType::Face, 0).unwrap();
        let edge = session.subshape(&box_shape, ShapeType::Edge, 0).unwrap();
        assert!(session.face_is_planar(&face).unwrap());
        assert!(session.edge_length(&edge).unwrap() > 0.0);
        assert_eq!(session.edge_circle_radius(&edge).unwrap(), None);
        assert_eq!(session.edge_curvature(&edge).unwrap(), Some(0.0));

        let cylinder = session
            .create_cylinder(Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 1.0), 2.0, 5.0)
            .unwrap();
        let radii = (0..session.subshape_count(&cylinder, ShapeType::Edge).unwrap())
            .filter_map(|index| {
                let edge = session.subshape(&cylinder, ShapeType::Edge, index).unwrap();
                session.edge_circle_radius(&edge).unwrap()
            })
            .collect::<Vec<_>>();
        assert!(radii.len() >= 2);
        assert!(radii.iter().all(|radius| (*radius - 2.0).abs() < 1e-9));
        let circular_edge = (0..session.subshape_count(&cylinder, ShapeType::Edge).unwrap())
            .map(|index| session.subshape(&cylinder, ShapeType::Edge, index).unwrap())
            .find(|edge| session.edge_circle_radius(edge).unwrap().is_some())
            .unwrap();
        assert!((session.edge_curvature(&circular_edge).unwrap().unwrap() - 0.5).abs() < 1e-9);

        let ellipse = session
            .create_ellipse_wire(Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 1.0), 4.0, 2.0)
            .unwrap();
        let ellipse_edge = session.subshape(&ellipse, ShapeType::Edge, 0).unwrap();
        let (minimum, maximum) = session.edge_curvature_range(&ellipse_edge, 5).unwrap();
        assert!((minimum - 0.125).abs() < 1e-9);
        assert!((maximum - 1.0).abs() < 1e-9);
        assert_eq!(
            session
                .edge_curvature_range(&ellipse_edge, 1)
                .unwrap_err()
                .status,
            1
        );

        let exact = session.edge_curvature_extrema(&ellipse_edge, 1e-6).unwrap();
        assert!(exact.is_exact);
        assert_eq!(exact.minimum, exact.minimum_lower_bound);
        assert_eq!(exact.maximum, exact.maximum_upper_bound);
        assert!((exact.minimum - 0.125).abs() < 1e-12);
        assert!((exact.maximum - 1.0).abs() < 1e-12);
        let circle = session
            .edge_curvature_extrema(&circular_edge, 1e-6)
            .unwrap();
        assert!(circle.is_exact && (circle.maximum - 0.5).abs() < 1e-12);
        for tolerance in [0.0, -1.0, 1.5, f64::NAN] {
            assert_eq!(
                session
                    .edge_curvature_extrema(&ellipse_edge, tolerance)
                    .unwrap_err()
                    .status,
                1
            );
        }
        assert_eq!(
            session
                .edge_curvature_extrema(&box_shape, 1e-6)
                .unwrap_err()
                .status,
            4
        );

        assert_eq!(session.face_is_planar(&box_shape).unwrap_err().status, 4);
        assert_eq!(session.edge_length(&box_shape).unwrap_err().status, 4);
        assert_eq!(
            session.edge_circle_radius(&box_shape).unwrap_err().status,
            4
        );
        assert_eq!(session.edge_curvature(&box_shape).unwrap_err().status, 4);
        assert_eq!(
            session
                .edge_curvature_range(&box_shape, 5)
                .unwrap_err()
                .status,
            4
        );
    }

    #[test]
    fn reports_recorded_face_tangency_on_fillets() {
        let session = Session::new().unwrap();
        let box_shape = session
            .create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 20.0, 30.0))
            .unwrap();
        let box_faces = (0..session.subshape_count(&box_shape, ShapeType::Face).unwrap())
            .map(|index| {
                session
                    .subshape(&box_shape, ShapeType::Face, index)
                    .unwrap()
            })
            .collect::<Vec<_>>();
        for first in 0..box_faces.len() {
            for second in first + 1..box_faces.len() {
                assert!(
                    !session
                        .faces_are_tangent(&box_shape, &box_faces[first], &box_faces[second])
                        .unwrap()
                );
            }
        }

        let edge = session.subshape(&box_shape, ShapeType::Edge, 0).unwrap();
        let filleted = session.fillet(&box_shape, &[&edge], 1.0).unwrap();
        let faces = (0..session.subshape_count(&filleted, ShapeType::Face).unwrap())
            .map(|index| session.subshape(&filleted, ShapeType::Face, index).unwrap())
            .collect::<Vec<_>>();
        let tangent_pairs = (0..faces.len())
            .flat_map(|first| (first + 1..faces.len()).map(move |second| (first, second)))
            .filter(|(first, second)| {
                session
                    .faces_are_tangent(&filleted, &faces[*first], &faces[*second])
                    .unwrap()
            })
            .count();
        assert!(tangent_pairs >= 2);
        assert_eq!(
            session
                .faces_are_tangent(&box_shape, &box_faces[0], &edge)
                .unwrap_err()
                .status,
            1
        );
    }

    #[test]
    fn duplicate_handles_preserve_operation_history() {
        let session = Session::new().unwrap();
        let source = unit_box(&session, 0.0);
        let source_face = session.subshape(&source, ShapeType::Face, 0).unwrap();
        let translated = session
            .translate(&source, Vec3::new(10.0, 0.0, 0.0))
            .unwrap();
        let duplicate = session.duplicate(&translated).unwrap();

        assert_eq!(
            session
                .history_count(&duplicate, &source_face, HistoryRelation::Modified)
                .unwrap(),
            1
        );
        assert_eq!(
            session.bounds(&duplicate).unwrap(),
            session.bounds(&translated).unwrap()
        );
        assert_eq!(session.shape_count().unwrap(), 4);
    }

    #[test]
    fn rejects_measurements_unsupported_by_shape_dimension() {
        let session = Session::new().unwrap();
        let wire = session
            .create_polyline_wire(&[Vec3::new(0.0, 0.0, 0.0), Vec3::new(4.0, 0.0, 0.0)], false)
            .unwrap();

        assert_eq!(session.shape_type(&wire).unwrap(), ShapeType::Wire);
        assert_eq!(session.surface_area(&wire).unwrap_err().status, 4);
        assert_eq!(session.volume(&wire).unwrap_err().status, 4);
        let center = session.center_of_mass(&wire).unwrap();
        assert!((center.x - 2.0).abs() < 1e-9);
        assert!(center.y.abs() < 1e-9);
        assert!(center.z.abs() < 1e-9);
        assert_eq!(session.shape_count().unwrap(), 1);
    }

    #[test]
    fn applies_selected_edge_offset_hollow_and_common_operations() {
        let session = Session::new().unwrap();
        let source = session
            .create_box(Vec3::new(1.0, 2.0, 3.0), Vec3::new(10.0, 20.0, 30.0))
            .unwrap();
        let edge = session.subshape(&source, ShapeType::Edge, 0).unwrap();
        let face = session.subshape(&source, ShapeType::Face, 0).unwrap();

        let filleted = session.fillet(&source, &[&edge], 1.0).unwrap();
        let chamfered = session.chamfer(&source, &[&edge], 1.0).unwrap();
        let offset = session.offset(&source, 1.0, 1e-6).unwrap();
        let hollow = session.hollow(&source, &[&face], -1.0, 1e-6).unwrap();
        let overlap = session
            .create_box(Vec3::new(6.0, 12.0, 18.0), Vec3::new(10.0, 10.0, 20.0))
            .unwrap();
        let common = session.common(&source, &overlap).unwrap();

        assert!(session.is_valid(&filleted).unwrap());
        assert!(session.is_valid(&chamfered).unwrap());
        assert!(session.is_valid(&offset).unwrap());
        assert!(session.is_valid(&hollow).unwrap());
        assert!(session.is_valid(&common).unwrap());
        let offset_bounds = session.bounds(&offset).unwrap();
        assert!(offset_bounds.min.x < 1.0);
        assert!(offset_bounds.max.z > 33.0);
        assert!((session.volume(&common).unwrap() - 750.0).abs() < 1e-6);
        assert!(session.is_valid(&source).unwrap());
        assert_eq!(session.shape_count().unwrap(), 9);
    }

    #[test]
    fn operation_history_marks_fully_removed_topology_as_deleted() {
        let session = Session::new().unwrap();
        let object = session
            .create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 10.0, 10.0))
            .unwrap();
        let removed_face = session.subshape(&object, ShapeType::Face, 0).unwrap();
        let removed_bounds = session.bounds(&removed_face).unwrap();
        assert!(removed_bounds.max.x < 1e-6);
        let tool = session
            .create_box(Vec3::new(-1.0, -1.0, -1.0), Vec3::new(5.0, 12.0, 12.0))
            .unwrap();
        let result = session.cut(&object, &tool).unwrap();

        assert!(session.history_is_deleted(&result, &removed_face).unwrap());
        assert_eq!(
            session
                .history_count(&result, &removed_face, HistoryRelation::Modified)
                .unwrap(),
            0
        );
        assert_eq!(session.shape_count().unwrap(), 4);
    }

    #[test]
    fn rejects_invalid_or_unrelated_operation_selections() {
        let session = Session::new().unwrap();
        let first = unit_box(&session, 0.0);
        let second = unit_box(&session, 5.0);
        let first_edge = session.subshape(&first, ShapeType::Edge, 0).unwrap();
        let first_face = session.subshape(&first, ShapeType::Face, 0).unwrap();

        assert_eq!(
            session
                .fillet(&second, &[&first_edge], 0.5)
                .unwrap_err()
                .status,
            4
        );
        assert_eq!(
            session
                .chamfer(&second, &[&first_edge], 0.5)
                .unwrap_err()
                .status,
            4
        );
        assert_eq!(
            session
                .hollow(&second, &[&first_face], -0.5, 1e-6)
                .unwrap_err()
                .status,
            4
        );
        assert_eq!(session.fillet(&first, &[], 0.5).unwrap_err().status, 1);
        assert_eq!(session.offset(&first, 0.0, 1e-6).unwrap_err().status, 1);
        assert_eq!(session.shape_count().unwrap(), 4);
    }

    #[test]
    #[allow(deprecated)]
    fn constructs_a_chamfered_filleted_faceted_stone() {
        let session = Session::new().unwrap();
        let bottom = [
            Vec3::new(-20.0, -14.0, 0.0),
            Vec3::new(-7.0, -21.0, 0.0),
            Vec3::new(17.0, -18.0, 0.0),
            Vec3::new(23.0, -2.0, 0.0),
            Vec3::new(17.0, 17.0, 0.0),
            Vec3::new(-3.0, 22.0, 0.0),
            Vec3::new(-24.0, 9.0, 0.0),
        ];
        let top = [
            Vec3::new(-18.0, -12.5, 6.4),
            Vec3::new(-6.0, -19.0, 7.1),
            Vec3::new(15.0, -16.0, 6.7),
            Vec3::new(20.5, -1.5, 7.5),
            Vec3::new(15.0, 15.0, 6.8),
            Vec3::new(-2.5, 19.5, 7.8),
            Vec3::new(-21.0, 8.0, 6.6),
        ];
        let stone = session
            .create_faceted_stone(&bottom, &top, Vec3::new(0.0, 0.5, 9.0), 0.8, 0.7)
            .unwrap();
        assert!(session.is_valid(&stone).unwrap());
    }

    #[test]
    #[allow(deprecated)]
    fn creates_wall_torch_with_flame_anchored_light() {
        let session = Session::new().unwrap();
        let torch = session
            .create_wall_torch(Vec3::new(0.0, 120.0, 130.0), Vec3::new(1.0, 0.0, 0.0), 1.0)
            .unwrap();
        assert!(session.is_valid(&torch.fixture).unwrap());
        assert!(session.is_valid(&torch.flame).unwrap());
        assert!(torch.light.position.x > 40.0);
        assert!(torch.light.position.z > 160.0);
        assert_eq!(torch.light.light_type, LightType::Positional);
        assert_eq!(torch.light.direction, Vec3::new(0.0, 0.0, 0.0));
        assert!(!torch.light.cast_shadows);
    }

    #[test]
    fn sweeps_a_round_tube_along_a_polyline() {
        let session = Session::new().unwrap();
        let tube = session
            .create_polyline_tube(
                &[
                    Vec3::new(0.0, 0.0, 0.0),
                    Vec3::new(20.0, 0.0, 0.0),
                    Vec3::new(30.0, 10.0, 0.0),
                    Vec3::new(30.0, 25.0, 5.0),
                ],
                2.0,
            )
            .unwrap();
        assert!(session.is_valid(&tube).unwrap());
    }

    #[test]
    fn lofts_sections_and_builds_a_compound() {
        let session = Session::new().unwrap();
        let root = [
            Vec3::new(0.0, 0.0, -1.0),
            Vec3::new(10.0, 0.0, -1.0),
            Vec3::new(10.0, 0.0, 1.0),
            Vec3::new(0.0, 0.0, 1.0),
        ];
        let tip = [
            Vec3::new(2.0, 20.0, -0.5),
            Vec3::new(8.0, 20.0, -0.5),
            Vec3::new(8.0, 20.0, 0.5),
            Vec3::new(2.0, 20.0, 0.5),
        ];
        let loft = session.create_loft(&[&root, &tip], true, false).unwrap();
        assert!(session.is_valid(&loft).unwrap());
        let marker = session
            .create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 1.0, 1.0))
            .unwrap();
        let compound = session.create_compound(&[&loft, &marker]).unwrap();
        assert!(session.is_valid(&compound).unwrap());
        assert!(session.is_valid(&loft).unwrap());
        assert!(session.is_valid(&marker).unwrap());
    }

    #[test]
    fn rejects_shapes_from_another_session_for_every_shape_operation() {
        let first = Session::new().unwrap();
        let second = Session::new().unwrap();
        let first_shape = unit_box(&first, 0.0);
        let first_shape_to_remove = unit_box(&first, 2.0);
        let second_shape = unit_box(&second, 10.0);

        assert_wrong_session(second.create_compound(&[&first_shape]).unwrap_err());
        assert_wrong_session(second.sew(&[&first_shape], 1e-6).unwrap_err());
        assert_wrong_session(second.exact_bounds(&first_shape).unwrap_err());
        assert_wrong_session(second.make_solid(&first_shape).unwrap_err());
        assert_wrong_session(second.make_solid_from_shells(&[&first_shape]).unwrap_err());
        assert_wrong_session(second.fuse(&first_shape, &second_shape).unwrap_err());
        assert_wrong_session(second.cut(&second_shape, &first_shape).unwrap_err());
        assert_wrong_session(second.common(&second_shape, &first_shape).unwrap_err());
        assert_wrong_session(second.fillet(&first_shape, &[], 1.0).unwrap_err());
        assert_wrong_session(second.chamfer(&first_shape, &[], 1.0).unwrap_err());
        assert_wrong_session(second.offset(&first_shape, 1.0, 1e-6).unwrap_err());
        assert_wrong_session(second.hollow(&first_shape, &[], -1.0, 1e-6).unwrap_err());
        assert_wrong_session(
            second
                .fillet(&second_shape, &[&first_shape], 1.0)
                .unwrap_err(),
        );
        assert_wrong_session(second.shape_type(&first_shape).unwrap_err());
        assert_wrong_session(second.duplicate(&first_shape).unwrap_err());
        assert_wrong_session(
            second
                .subshape_count(&first_shape, ShapeType::Face)
                .unwrap_err(),
        );
        assert_wrong_session(
            second
                .subshape(&first_shape, ShapeType::Face, 0)
                .unwrap_err(),
        );
        assert_wrong_session(second.surface_area(&first_shape).unwrap_err());
        assert_wrong_session(second.volume(&first_shape).unwrap_err());
        assert_wrong_session(second.center_of_mass(&first_shape).unwrap_err());
        assert_wrong_session(second.face_normal(&first_shape).unwrap_err());
        assert_wrong_session(second.face_is_planar(&first_shape).unwrap_err());
        assert_wrong_session(second.edge_length(&first_shape).unwrap_err());
        assert_wrong_session(second.edge_circle_radius(&first_shape).unwrap_err());
        assert_wrong_session(second.edge_curvature(&first_shape).unwrap_err());
        assert_wrong_session(second.edge_curvature_range(&first_shape, 5).unwrap_err());
        assert_wrong_session(
            second
                .faces_are_tangent(&second_shape, &first_shape, &second_shape)
                .unwrap_err(),
        );
        assert_wrong_session(second.is_same(&first_shape, &second_shape).unwrap_err());
        assert_wrong_session(
            second
                .is_adjacent(&second_shape, &first_shape, &second_shape)
                .unwrap_err(),
        );
        assert_wrong_session(
            second
                .history_count(&second_shape, &first_shape, HistoryRelation::Modified)
                .unwrap_err(),
        );
        assert_wrong_session(
            second
                .history(&second_shape, &first_shape, HistoryRelation::Modified, 0)
                .unwrap_err(),
        );
        assert_wrong_session(
            second
                .history_is_deleted(&second_shape, &first_shape)
                .unwrap_err(),
        );
        assert_wrong_session(second.create_face_from_wire(&first_shape).unwrap_err());
        assert_wrong_session(
            second
                .create_prism_from_face(&first_shape, Vec3::new(0.0, 0.0, 1.0))
                .unwrap_err(),
        );
        assert_wrong_session(
            second
                .translate(&first_shape, Vec3::new(1.0, 0.0, 0.0))
                .unwrap_err(),
        );
        assert_wrong_session(
            second
                .rotate(
                    &first_shape,
                    Vec3::new(0.0, 0.0, 0.0),
                    Vec3::new(0.0, 0.0, 1.0),
                    1.0,
                )
                .unwrap_err(),
        );
        assert_wrong_session(
            second
                .scale(&first_shape, Vec3::new(0.0, 0.0, 0.0), 2.0)
                .unwrap_err(),
        );
        assert_wrong_session(second.bounds(&first_shape).unwrap_err());
        assert_wrong_session(second.is_valid(&first_shape).unwrap_err());
        assert_wrong_session(
            second
                .save_brep(&first_shape, "cross-session-must-not-exist.brep")
                .unwrap_err(),
        );
        assert_wrong_session(
            second
                .save_step(&first_shape, "cross-session-must-not-exist.step")
                .unwrap_err(),
        );
        assert_wrong_session(
            second
                .save_stl(
                    &first_shape,
                    "cross-session-must-not-exist.stl",
                    StlOptions::default(),
                )
                .unwrap_err(),
        );
        assert_wrong_session(second.remove(first_shape_to_remove).unwrap_err());

        // The rejected handle was moved into `remove`; dropping it releases
        // the shape in its own session rather than leaking it.
        assert_eq!(first.shape_count().unwrap(), 1);
        assert_eq!(second.shape_count().unwrap(), 1);
        assert!(first.is_valid(&first_shape).unwrap());
        assert!(second.is_valid(&second_shape).unwrap());
    }

    #[test]
    fn clear_invalidates_shapes_for_every_shape_operation() {
        let session = Session::new().unwrap();
        let old_shape = unit_box(&session, 0.0);
        let old_shape_to_remove = unit_box(&session, 2.0);

        session.clear().unwrap();

        let new_shape = unit_box(&session, 10.0);
        assert_cleared(session.create_compound(&[&old_shape]).unwrap_err());
        assert_cleared(session.fuse(&old_shape, &new_shape).unwrap_err());
        assert_cleared(session.cut(&new_shape, &old_shape).unwrap_err());
        assert_cleared(session.common(&new_shape, &old_shape).unwrap_err());
        assert_cleared(session.fillet(&old_shape, &[], 1.0).unwrap_err());
        assert_cleared(session.chamfer(&old_shape, &[], 1.0).unwrap_err());
        assert_cleared(session.offset(&old_shape, 1.0, 1e-6).unwrap_err());
        assert_cleared(session.hollow(&old_shape, &[], -1.0, 1e-6).unwrap_err());
        assert_cleared(session.shape_type(&old_shape).unwrap_err());
        assert_cleared(
            session
                .subshape_count(&old_shape, ShapeType::Face)
                .unwrap_err(),
        );
        assert_cleared(
            session
                .subshape(&old_shape, ShapeType::Face, 0)
                .unwrap_err(),
        );
        assert_cleared(session.surface_area(&old_shape).unwrap_err());
        assert_cleared(session.volume(&old_shape).unwrap_err());
        assert_cleared(session.center_of_mass(&old_shape).unwrap_err());
        assert_cleared(
            session
                .history_count(&new_shape, &old_shape, HistoryRelation::Modified)
                .unwrap_err(),
        );
        assert_cleared(
            session
                .history(&new_shape, &old_shape, HistoryRelation::Modified, 0)
                .unwrap_err(),
        );
        assert_cleared(
            session
                .history_is_deleted(&new_shape, &old_shape)
                .unwrap_err(),
        );
        assert_cleared(session.create_face_from_wire(&old_shape).unwrap_err());
        assert_cleared(
            session
                .create_prism_from_face(&old_shape, Vec3::new(0.0, 0.0, 1.0))
                .unwrap_err(),
        );
        assert_cleared(
            session
                .translate(&old_shape, Vec3::new(1.0, 0.0, 0.0))
                .unwrap_err(),
        );
        assert_cleared(
            session
                .rotate(
                    &old_shape,
                    Vec3::new(0.0, 0.0, 0.0),
                    Vec3::new(0.0, 0.0, 1.0),
                    1.0,
                )
                .unwrap_err(),
        );
        assert_cleared(
            session
                .scale(&old_shape, Vec3::new(0.0, 0.0, 0.0), 2.0)
                .unwrap_err(),
        );
        assert_cleared(session.bounds(&old_shape).unwrap_err());
        assert_cleared(session.is_valid(&old_shape).unwrap_err());
        assert_cleared(
            session
                .save_brep(&old_shape, "cleared-shape-must-not-exist.brep")
                .unwrap_err(),
        );
        assert_cleared(
            session
                .save_step(&old_shape, "cleared-shape-must-not-exist.step")
                .unwrap_err(),
        );
        assert_cleared(
            session
                .save_stl(
                    &old_shape,
                    "cleared-shape-must-not-exist.stl",
                    StlOptions::default(),
                )
                .unwrap_err(),
        );
        assert_cleared(session.remove(old_shape_to_remove).unwrap_err());

        assert_eq!(session.shape_count().unwrap(), 1);
        assert!(session.is_valid(&new_shape).unwrap());
    }

    #[test]
    fn non_consuming_operations_leave_inputs_usable() {
        let session = Session::new().unwrap();
        let left = unit_box(&session, 0.0);
        let right = unit_box(&session, 0.5);

        let fused = session.fuse(&left, &right).unwrap();
        let compound = session.create_compound(&[&left, &right, &fused]).unwrap();

        assert!(session.is_valid(&left).unwrap());
        assert!(session.is_valid(&right).unwrap());
        assert!(session.is_valid(&fused).unwrap());
        assert!(session.is_valid(&compound).unwrap());
    }

    #[test]
    fn remove_deletes_exactly_one_shape() {
        let session = Session::new().unwrap();
        let removed = unit_box(&session, 0.0);
        let retained = unit_box(&session, 2.0);

        session.remove(removed).unwrap();

        assert_eq!(session.shape_count().unwrap(), 1);
        assert!(session.is_valid(&retained).unwrap());
    }
}
