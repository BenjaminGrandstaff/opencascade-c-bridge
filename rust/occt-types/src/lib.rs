//! Plain value types shared by the opencascade-c-bridge Rust crates. They hold
//! no kernel handles, so crates that only describe or check models can use them
//! without linking the native library.

use std::{error::Error, fmt};

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

/// A point's nearest location on a set of faces (ABI 69).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FaceProjection {
    pub point: Vec3,
    /// Unit normal along the face's orientation: out of a solid's material.
    pub normal: Vec3,
    pub distance: f64,
    /// Index into the faces passed to the projection.
    pub face: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FacePointState {
    Outside,
    Inside,
    Boundary,
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

/// A helix about the axis through `origin`, starting at
/// `origin + radius * start_direction` (made perpendicular to the axis) and
/// rising `pitch` per turn for `turns` (fractional allowed, at most 10,000).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HelixOptions {
    pub origin: Vec3,
    pub axis: Vec3,
    pub start_direction: Vec3,
    pub radius: f64,
    pub pitch: f64,
    pub turns: f64,
    pub left_handed: bool,
}

/// Neutral plane, pull direction, and signed angle for a draft operation.
#[derive(Clone, Copy, Debug)]
pub struct DraftOptions {
    pub neutral_origin: Vec3,
    pub neutral_normal: Vec3,
    pub pull_direction: Vec3,
    pub angle_radians: f64,
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

/// A named sub-assembly of [`Session::save_step_assembly_tree`], placed in
/// its parent by the rigid row-major 3x4 transform `[R | t]`, `p' = R p + t`.
/// `parent` is `None` at the top level or an earlier node's index.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StepNode<'a> {
    pub name: &'a str,
    pub parent: Option<usize>,
    pub transform: [f64; 12],
}

/// A color for face `face` (in [`Session::subshapes`] order) of component
/// `component`'s part, as sRGB channels in [0, 1]. Components sharing a part
/// share its face colors.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StepFaceColor {
    pub component: usize,
    pub face: usize,
    pub color: [f64; 3],
}

/// Faces `faces` (in [`Session::subshapes`] order) of component
/// `component`'s part, which a semantic PMI item annotates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StepPmiFaces<'a> {
    pub component: usize,
    pub faces: &'a [usize],
}

/// A datum feature. A label names one datum in the whole file.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StepDatum<'a> {
    pub label: &'a str,
    pub target: StepPmiFaces<'a>,
}

/// Material condition of a tolerance, or material boundary of a datum
/// reference.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StepMaterial {
    #[default]
    Regardless,
    Maximum,
    Least,
}

/// One datum of a tolerance's datum reference frame: an index into
/// [`StepPmi::datums`], in compartment 0 (primary), 1 or 2. Several datums in
/// one compartment form a common datum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StepDatumReference {
    pub datum: usize,
    pub compartment: u32,
    pub boundary: StepMaterial,
    pub translation: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepCharacteristic {
    Straightness,
    Flatness,
    Circularity,
    Cylindricity,
    ProfileLine,
    ProfileSurface,
    Parallelism,
    Perpendicularity,
    Angularity,
    Position,
    CircularRunout,
    TotalRunout,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StepToleranceModifiers {
    pub free_state: bool,
    pub statistical: bool,
    pub tangent_plane: bool,
}

/// A geometric tolerance on its target faces.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StepTolerance<'a> {
    pub target: StepPmiFaces<'a>,
    pub characteristic: StepCharacteristic,
    pub value_mm: f64,
    pub diameter: bool,
    pub material: StepMaterial,
    pub modifiers: StepToleranceModifiers,
    /// Height of a projected tolerance zone.
    pub projected_mm: Option<f64>,
    pub datums: &'a [StepDatumReference],
}

/// A diameter over cylindrical faces of one radius, or a width between two
/// parallel planar faces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepSizeKind {
    Diameter,
    Width,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StepSize<'a> {
    pub target: StepPmiFaces<'a>,
    pub kind: StepSizeKind,
    pub lower_mm: f64,
    pub upper_mm: f64,
}

/// Semantic PMI for [`Session::save_step_assembly_pmi`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StepPmi<'a> {
    pub datums: &'a [StepDatum<'a>],
    pub tolerances: &'a [StepTolerance<'a>],
    pub sizes: &'a [StepSize<'a>],
}

/// What reading a written PMI item back did not recover: the whole item, or
/// the listed properties of the item read on the same faces.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StepPmiLoss {
    pub item: bool,
    pub value: bool,
    pub zone: bool,
    pub material: bool,
    pub modifiers: bool,
    pub projected: bool,
    pub datums: bool,
}

impl StepPmiLoss {
    pub fn is_none(&self) -> bool {
        *self == Self::default()
    }
}

/// One loss per written datum, tolerance and size, in input order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StepPmiVerification {
    pub datums: Vec<StepPmiLoss>,
    pub tolerances: Vec<StepPmiLoss>,
    pub sizes: Vec<StepPmiLoss>,
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
    /// Exact elliptical arc (ABI 63): `major` and `minor` are axis vertices
    /// (perpendicular semi-axes, either may be longer) and `start`/`end` lie
    /// on the ellipse. Counterclockwise about `(major − center) × (minor −
    /// center)` unless `clockwise`.
    EllipseArc {
        center: Vec3,
        major: Vec3,
        minor: Vec3,
        start: Vec3,
        end: Vec3,
        clockwise: bool,
    },
    /// Explicit clamped, non-clamped, or periodic B-spline. Empty weights mean all ones.
    BSpline {
        non_clamped: bool,
        periodic: bool,
        poles: Vec<Vec3>,
        degree: i32,
        knots: Vec<f64>,
        multiplicities: Vec<i32>,
        weights: Vec<f64>,
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

/// Installed font style for [`Session::font_file`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FontStyle {
    #[default]
    Regular = 0,
    Bold = 1,
    Italic = 2,
    BoldItalic = 3,
}

/// Where a line of text sits relative to its origin along the baseline.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextAlign {
    #[default]
    Start = 0,
    Center = 1,
    End = 2,
}

/// Exact located line or conic. Conic points are center + major*cos(t) +
/// minor*sin(t); parameters follow edge orientation (possibly descending).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AnalyticCurve {
    Line {
        start: Vec3,
        end: Vec3,
    },
    Conic {
        center: Vec3,
        major: Vec3,
        minor: Vec3,
        first: f64,
        last: f64,
    },
}

/// One exact rational Bezier span, parameterized on [0, 1].
#[derive(Clone, Debug, PartialEq)]
pub struct BezierSpan {
    pub poles: Vec<Vec3>,
    pub weights: Vec<f64>,
}
