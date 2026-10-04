//! Public value types, shape handles, and errors.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeshOptions {
    pub linear_deflection: f64,
    pub angular_deflection_radians: f64,
    pub maximum_triangles: usize,
}
impl Default for MeshOptions {
    fn default() -> Self {
        Self {
            linear_deflection: 0.1,
            angular_deflection_radians: 0.3,
            maximum_triangles: 1_000_000,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeshTriangle {
    /// Zero-based face index from `Session::subshapes(shape, ShapeType::Face)`.
    pub face_index: usize,
    pub points: [Vec3; 3],
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
pub(crate) struct RawDiagnostic {
    pub(crate) kind: i32,
    pub(crate) code: i32,
    pub(crate) input_index: i64,
    pub(crate) has_shape: c_int,
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
    /// Draft failure; code is `Draft_ErrorStatus`.
    Draft,
    /// A kind added by a newer library.
    Other(i32),
}

impl DiagnosticKind {
    pub(crate) fn from_raw(kind: i32) -> Self {
        match kind {
            1 => Self::FilletEdge,
            2 => Self::FilletVertex,
            3 => Self::IsolatedEdge,
            4 => Self::Offset,
            5 => Self::BooleanAlert,
            6 => Self::InvalidSubshape,
            7 => Self::Draft,
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

/// Neutral plane, pull direction, and signed angle for a draft operation.
#[derive(Clone, Copy, Debug)]
pub struct DraftOptions {
    pub neutral_origin: Vec3,
    pub neutral_normal: Vec3,
    pub pull_direction: Vec3,
    pub angle_radians: f64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct RawSessionOptions {
    pub(crate) validate_results: c_int,
    pub(crate) heal_invalid_results: c_int,
    pub(crate) boolean_fuzzy_tolerance: f64,
}

/// How a session treats kernel results.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SessionOptions {
    /// Check results of booleans, fillets, chamfers, offsets, hollowing,
    /// draft, sewing, and STEP and BREP import; invalid results become errors.
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

/// Radius sample on OCCT's relative contour parameter, from 0 to 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FilletRadiusStation {
    pub position: f64,
    pub radius: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum FilletSpineDirection {
    #[default]
    Kernel,
    Reversed,
    /// The endpoint nearest this point is the beginning; equidistant ties fail.
    FromPoint(Vec3),
}

/// Unit-density inertial properties in model coordinates. Length units u give
/// volume u^3, center u, and central inertia u^5 in model XYZ axes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MassProperties {
    pub volume: f64,
    pub center: Vec3,
    pub inertia: [[f64; 3]; 3],
    pub relative_volume_error: f64,
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

/// How a swept profile turns as it follows its path.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SweepOrientation {
    /// Least twist; the usual choice.
    CorrectedFrenet,
    /// Follows the path's curvature frame.
    Frenet,
    /// Keeps the profile's normal-side axis along this direction.
    Binormal(Vec3),
    /// Never rotates: every section is parallel to the first.
    Fixed,
}

/// One piece of [`Session::create_curve_wire`]: like [`WireSegment`], plus
/// B-splines interpolated through any number of points.
#[derive(Clone, Debug, PartialEq)]
pub enum CurveSegment {
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
    /// Passes through `points` in order (at least two). Optional end
    /// tangents fix the curve's direction at its ends; their length is
    /// ignored. A periodic spline is a smooth closed loop through at least
    /// three points, ending where it starts.
    Spline {
        points: Vec<Vec3>,
        start_tangent: Option<Vec3>,
        end_tangent: Option<Vec3>,
        periodic: bool,
    },
}

/// Smallest principal radii of one face, signed by its outward normal. A
/// convex face curves away from the outward normal (outside of a cylinder or
/// fillet); a concave face curves toward it (a bore or inside fillet). `None`
/// when the face never curves that way. Each radius has the point where it is
/// attained.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FaceRadiusBounds {
    pub convex: Option<(f64, Vec3)>,
    pub concave: Option<(f64, Vec3)>,
    /// True on planes, cylinders, cones, spheres, and tori.
    pub exact: bool,
    /// In-face samples evaluated on other surfaces; zero when exact.
    pub samples: u32,
}

/// Exact range of a face's outward unit normal along a pull direction, with
/// a point where each end is attained.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FacePullRange {
    pub minimum: (f64, Vec3),
    pub maximum: (f64, Vec3),
}

/// How the faces on either side of an edge meet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeConcavity {
    /// Tangent within the requested angle.
    Smooth,
    /// A sharp outside corner.
    Convex,
    /// A sharp inside corner.
    Concave,
    /// Convex along part of the edge and concave along another part.
    Mixed,
    /// A free boundary, degenerate, or non-manifold edge.
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DistanceResult {
    pub distance: f64,
    pub first: Vec3,
    pub second: Vec3,
}

/// Image XY frame; direction points toward the viewer, x_axis toward image right.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProjectionFrame {
    pub origin: Vec3,
    pub direction: Vec3,
    pub x_axis: Vec3,
}

#[derive(Debug)]
pub struct ProjectedEdges<'session> {
    pub visible: Shape<'session>,
    pub hidden: Shape<'session>,
}
