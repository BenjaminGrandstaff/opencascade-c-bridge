//! Feature operations and their hole, thread, rib, fillet, extrude, revolve, sweep, and loft options.

use super::*;

/// A full-diameter blind bore depth or a bore through the complete input along its
/// axis line. Through-all covers both directions from the supplied position.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HoleExtent {
    Blind {
        depth: ScalarExpr,
    },
    ThroughAll,
    /// One selected bounded face, resolved against the hole input.
    UpToFace {
        face: Box<FaceSelector>,
    },
    /// Nearest whole-profile forward cutoff among the hole input's faces.
    UpToNext,
}

/// Shape below a blind hole's full-diameter bore depth.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HoleBottom {
    #[default]
    Flat,
    /// Included point angle in scalar radians (0 < angle < pi).
    /// The conical tip adds diameter / (2*tan(angle/2)) to the bore depth.
    /// The complete tip must remain within the input material.
    DrillPoint { angle_radians: ScalarExpr },
}

/// Entry recess, starting at the hole position along its axis.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HoleFinish {
    #[default]
    Plain,
    Counterbore {
        diameter: ScalarExpr,
        depth: ScalarExpr,
        /// Diameter of an optional 60° countersink at the counterbore's floor,
        /// narrowing into the bore, that clears the fillet under a socket
        /// head (schema 164). No wider than the bore, it cuts nothing.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        relief: Option<ScalarExpr>,
    },
    /// Included cone angle in scalar radians, strictly between zero and pi.
    Countersink {
        diameter: ScalarExpr,
        angle_radians: ScalarExpr,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ThreadHandedness {
    Right,
    Left,
}

/// Caller-supplied internal-thread intent, not a standards lookup. The hole
/// diameter remains the explicit cylindrical bore (tap drill) diameter.
/// With `modeled`, the hole also cuts the thread geometry.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ThreadSpecification {
    pub designation: String,
    pub nominal_diameter: ScalarExpr,
    pub pitch: ScalarExpr,
    pub handedness: ThreadHandedness,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modeled: Option<ModeledThread>,
    /// A Unified (ASME B1.1) internal class (schema 152): the nominal
    /// diameter and pitch must be a size of its series, the designation must
    /// be the standard one (`1/4-20 UNC-2B`, with `-LH` when left-handed),
    /// and the bore must lie within the class's minor-diameter limits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unified: Option<crate::screw_threads::UnifiedThreadSpec>,
}

/// Cuts an ISO 68-1 basic-profile internal thread of the callout's nominal
/// diameter, pitch and handedness into the bore, from the hole entry along
/// its axis. The bore must lie between the basic minor and nominal
/// diameters. Blind holes keep the groove at least half a pitch above the
/// bore bottom; omitted `length` threads the whole usable bore.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModeledThread {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length: Option<ScalarExpr>,
    /// Axial length after `length` over which the groove fades out into the
    /// bore (schema 114). Blind holes fit length plus runout above the floor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runout: Option<ScalarExpr>,
}

/// Which side of a sheet a `Thicken` grows toward.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ThickenSide {
    /// Along the sheet's face normals.
    #[default]
    Normal,
    /// Against the face normals.
    Reverse,
    /// Half the thickness to each side of the sheet.
    Symmetric,
}

/// How curve pattern copies follow their path.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum CurvePatternOrientation {
    /// Copies keep the input's orientation and only move.
    #[default]
    Fixed,
    /// Copies turn with the path's tangent, carried along it without twist.
    Tangent,
}

/// How fill pattern sites are laid out.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum FillPatternLayout {
    /// Rows directly above one another.
    #[default]
    Grid,
    /// Every other row shifted by half a spacing.
    Staggered,
}

/// Which side of a split plane a `Split` keeps.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SplitKeep {
    /// The side the plane normal points to.
    #[default]
    Positive,
    Negative,
    /// Both sides, as one compound of their solids.
    Both,
}

/// How a rib's total thickness is placed relative to its profile plane.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum RibThicknessMode {
    /// Extrude from the profile plane along the supplied direction.
    #[default]
    OneSided,
    /// Place half the total thickness on each side of the profile plane.
    Centered,
}

/// The bounded planar region used before applying rib thickness.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RibProfileMode {
    #[default]
    Closed,
    /// Close an open chain with its translated reversed copy and straight end
    /// bridges. The length-valued offset must produce a simple planar boundary.
    OpenStrip { offset: VectorExpr },
    /// Advance a straight open chain perpendicularly to its first body contact.
    /// Direction is dimensionless; maximum_length is a positive bounded reach.
    /// The whole translated chain must meet the first contact.
    OpenToNext {
        direction: VectorExpr,
        maximum_length: ScalarExpr,
    },
}

fn is_false(value: &bool) -> bool {
    !value
}

/// Terminal derivative of a cubic Hermite loft, along increasing station order.
/// Direction is dimensionless and transverse to the terminal profile plane;
/// magnitude is a positive length per station interval.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LoftEndConstraint {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<VectorExpr>,
    /// Derive the forward normal of the terminal outer profile (schema 170).
    /// Exactly one of normal_to_profile or direction must be supplied.
    #[serde(default, skip_serializing_if = "is_false")]
    pub normal_to_profile: bool,
    pub magnitude: ScalarExpr,
}

/// Explicit boundary correspondence for one saved loft profile (schema 171).
/// Start points are family-space lengths. Multi-edge profiles require a vertex;
/// one full periodic edge also supports an exact continuous seam anchor.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LoftProfileAlignment {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_point: Option<VectorExpr>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub reverse: bool,
    /// Exact family-space boundary partitions, applied before the start anchor (schema 172).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schemars(length(max = 128))]
    pub split_points: Vec<VectorExpr>,
}

/// An interior sample of a smooth radius law. Position is dimensionless and
/// strictly between 0 and 1; radius is a positive length.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FilletRadiusStation {
    pub position: ScalarExpr,
    pub radius: ScalarExpr,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FilletSpineDirection {
    #[default]
    Kernel,
    Reversed,
    /// Start at the contour endpoint nearest this length-valued point.
    FromPoint {
        point: VectorExpr,
    },
}

/// End condition for a planar profile extrusion. Geometric limits must
/// terminate the entire profile strictly forward, using their finite boundaries.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExtrudeExtent {
    /// The direction vector is the full length and direction of travel.
    #[default]
    Distance,
    /// Total length is split equally about the sketch plane.
    Symmetric,
    /// The vector supplies orientation; its magnitude is ignored.
    UpToFace {
        target: String,
        face: Box<FaceSelector>,
    },
    /// Nearest forward face terminating the complete profile. Crossing
    /// competing limits are ambiguous and require an explicit face selector.
    /// The vector supplies orientation; its magnitude is ignored.
    UpToNext { target: String },
}

/// Angular extent of a profile revolution. Angles are signed radians.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RevolveExtent {
    /// Begin at the source sketch plane and sweep through the signed angle.
    #[default]
    Angle,
    /// Sweep from minus half the angle to plus half about the sketch plane.
    Symmetric,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FeatureOperation {
    /// Constant-width sheet with tangent-length flanges and circular bends.
    SheetMetal {
        definition: Box<SheetMetalDefinition>,
    },
    /// Flat blank linked to a directly named SheetMetal feature.
    SheetMetalFlat {
        input: String,
        neutral_factor: ScalarExpr,
        /// Bend allowances from a shop's table instead of the neutral factor
        /// (schema 161).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        bend_table: Option<Box<crate::sheet_metal::BendTable>>,
    },
    Box {
        origin: VectorExpr,
        size: VectorExpr,
    },
    /// Sweeps the `profile` output (a wire, or a planar face including holes) along
    /// the `path` output (an edge or wire). Place the profile at the path's
    /// start, usually across it (a sketch on a `path_plane` datum at
    /// position 0 stays there as the path changes). A face or closed wire
    /// makes a solid. Holed faces sweep each boundary and subtract the
    /// contained inner volumes. Since schema 175 the profile may twist about
    /// the path, scale along it, follow a guide rail, or blend through
    /// further sections (see [`SweepTwist`], [`SweepScale`], [`SweepGuide`]
    /// and [`SweepSection`]).
    Sweep {
        profile: String,
        path: String,
        #[serde(default)]
        orientation: SweepOrientation,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        twist: Option<SweepTwist>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scale: Option<SweepScale>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        guide: Option<SweepGuide>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        #[schemars(length(max = 100))]
        sections: Vec<SweepSection>,
    },
    /// A solid through two or more sections with equal point counts. `smooth`
    /// interpolates each section as one B-spline with a corner only at its
    /// first point (an airfoil trailing edge); otherwise sections are
    /// polygons. `ruled` keeps straight lines between sections.
    Loft {
        sections: Vec<LoftSection>,
        smooth: bool,
        ruled: bool,
    },
    /// Solid loft through ordered saved planar sketch faces/wires, retaining
    /// native boundary curves. Faces must have exactly one boundary wire.
    ProfileLoft {
        #[schemars(length(min = 2, max = 1000))]
        profiles: Vec<String>,
        /// Each inner track supplies one simple profile for every outer station.
        #[serde(default)]
        #[schemars(length(max = 100))]
        holes: Vec<Vec<String>>,
        #[serde(default)]
        ruled: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        start_constraint: Option<Box<LoftEndConstraint>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        end_constraint: Option<Box<LoftEndConstraint>>,
        /// Profile IDs mapped to exact seam/traversal overrides. Requires end constraints.
        #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
        correspondence: std::collections::BTreeMap<String, LoftProfileAlignment>,
    },
    Cylinder {
        origin: VectorExpr,
        axis: VectorExpr,
        radius: ScalarExpr,
        height: ScalarExpr,
    },
    /// A catalog part from `occb_standard_parts_v1` (schema 180) at its
    /// nominal dimensions: `nominal_diameter` picks the size (an M size, or a
    /// dowel diameter), and screws and dowel pins need a positive `length`
    /// (under the head for screws). `origin` is the seat: the underside of a
    /// screw head, or the bearing face of a nut, washer or pin end; the part
    /// extends along the dimensionless `axis`, with a screw head behind it.
    /// Threads are not modeled: shanks and bores are plain at the nominal
    /// diameter.
    StandardPart {
        part: crate::standard_parts::StandardPartKind,
        nominal_diameter: ScalarExpr,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        length: Option<ScalarExpr>,
        origin: VectorExpr,
        axis: VectorExpr,
    },
    /// Full cone/frustum. Radii are nonnegative (not both zero), height positive.
    /// Equal radii produce a cylinder; origin is the base center.
    Cone {
        origin: VectorExpr,
        axis: VectorExpr,
        base_radius: ScalarExpr,
        top_radius: ScalarExpr,
        height: ScalarExpr,
    },
    /// Full sphere with length-valued center and positive radius.
    Sphere {
        center: VectorExpr,
        radius: ScalarExpr,
    },
    SketchFace {
        sketch: Box<SketchDefinition>,
    },
    SketchWire {
        sketch: Box<SketchDefinition>,
    },
    SketchOpenWire {
        sketch: Box<SketchDefinition>,
    },
    /// One planar face with strictly contained, disjoint inner boundaries.
    /// Inputs are closed planar wires or single-boundary planar faces.
    PlanarRegion {
        outer: String,
        #[schemars(length(min = 1, max = 100))]
        holes: Vec<String>,
    },
    /// Sweeps a planar face or closed planar wire by a length-valued vector.
    Extrude {
        input: String,
        direction: VectorExpr,
        #[serde(default)]
        extent: ExtrudeExtent,
    },
    /// Extrudes a planar rib region normally by positive thickness into one solid.
    /// OpenStrip explicitly closes an open profile; direction is dimensionless.
    Rib {
        input: String,
        profile: String,
        thickness: ScalarExpr,
        direction: VectorExpr,
        #[serde(default)]
        thickness_mode: RibThicknessMode,
        #[serde(default)]
        profile_mode: RibProfileMode,
    },
    /// Revolves a planar face or closed planar wire about a local axis.
    Revolve {
        input: String,
        origin: VectorExpr,
        axis: VectorExpr,
        angle_radians: ScalarExpr,
        #[serde(default)]
        extent: RevolveExtent,
    },
    Translate {
        input: String,
        offset: VectorExpr,
    },
    /// Reflect the input geometry across a plane; origin is length-valued,
    /// normal dimensionless and finite/nonzero. Produces the reflected part only.
    Mirror {
        input: String,
        origin: VectorExpr,
        normal: VectorExpr,
    },
    /// Uniform positive scaling about a length-valued centre. Factor is
    /// dimensionless; geometry is scaled while family datums stay explicit.
    Scale {
        input: String,
        center: VectorExpr,
        factor: ScalarExpr,
    },
    Rotate {
        input: String,
        origin: VectorExpr,
        axis: VectorExpr,
        angle_radians: ScalarExpr,
    },
    /// A modeled 60° screw thread (ISO 68-1 basic profile) cut into `input`
    /// about the axis through the length-valued `origin`, running the length
    /// `length` along the dimensionless `axis`. External threads cut a rod of
    /// `major_diameter`; internal threads cut a hole of the minor diameter
    /// (`major_diameter` − 2 · 5H/8, H = √3/2 · `pitch`) out to the major
    /// diameter. Right-handed unless `left_handed` (schema 84). An optional
    /// `runout` adds that axial length after `length` over which the groove
    /// leaves the material, fading to zero depth (schema 114).
    Thread {
        input: String,
        origin: VectorExpr,
        axis: VectorExpr,
        major_diameter: ScalarExpr,
        pitch: ScalarExpr,
        length: ScalarExpr,
        #[serde(default)]
        internal: bool,
        #[serde(default)]
        left_handed: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        runout: Option<ScalarExpr>,
        /// A Unified (ASME B1.1) series and class the thread represents
        /// (schema 152): the major diameter and pitch must be a size of its
        /// series and the class must match the side. The cut profile stays
        /// the basic profile.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        unified: Option<crate::screw_threads::UnifiedThreadSpec>,
    },
    /// An open helical wire about the axis through the length-valued `origin`
    /// along the dimensionless `axis`, starting at `origin + radius * start`
    /// (`start` is made perpendicular to the axis) and rising the length
    /// `pitch` per turn for the dimensionless `turns` (fractional allowed, at
    /// most 10,000). Right-handed unless `left_handed`. Use it as a `Sweep`
    /// path with `SweepOrientation::Binormal` along the axis for springs and
    /// coils (schema 82).
    Helix {
        origin: VectorExpr,
        axis: VectorExpr,
        start: VectorExpr,
        radius: ScalarExpr,
        pitch: ScalarExpr,
        turns: ScalarExpr,
        #[serde(default)]
        left_handed: bool,
    },
    Fuse {
        left: String,
        right: String,
    },
    Cut {
        object: String,
        tool: String,
    },
    /// Removes a cylindrical bore from one solid. Position is length-valued,
    /// axis is dimensionless; diameter and blind depth are positive lengths.
    Hole {
        input: String,
        position: VectorExpr,
        axis: VectorExpr,
        diameter: ScalarExpr,
        extent: HoleExtent,
        #[serde(default)]
        bottom: HoleBottom,
        #[serde(default)]
        finish: HoleFinish,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        thread: Option<Box<ThreadSpecification>>,
    },
    Common {
        left: String,
        right: String,
    },
    /// Rotates copies by i*angle_step_radians, including the seed. Count is
    /// an integer-valued scalar; origin is length-valued and axis scalar.
    CircularPattern {
        input: String,
        origin: VectorExpr,
        axis: VectorExpr,
        count: ScalarExpr,
        angle_step_radians: ScalarExpr,
        /// Copy indices to leave out (schema 184); 0 is the seed.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        #[schemars(length(max = 10000))]
        skip: Vec<usize>,
    },
    /// Repeats the input at i*step, including i=0, into an unfused compound.
    /// Step is a length vector. Count is an integer-valued scalar in 1..=10000.
    LinearPattern {
        input: String,
        step: VectorExpr,
        count: ScalarExpr,
        /// Copy indices to leave out (schema 184); 0 is the seed.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        #[schemars(length(max = 10000))]
        skip: Vec<usize>,
    },
    /// Copies spaced evenly by arc length along `path` (schema 184), a
    /// feature whose output is one edge or wire, such as a sketch open wire.
    /// The input is modeled at the path's start; copy i moves to the point
    /// i/(count-1) of the way along an open path, or i/count around a closed
    /// one. With `tangent` orientation each copy also turns with the path,
    /// carried along it without twist; `fixed` copies only move. Count is in
    /// 1..=10000; `skip` leaves out copy indices.
    CurvePattern {
        input: String,
        path: String,
        count: ScalarExpr,
        #[serde(default)]
        orientation: CurvePatternOrientation,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        #[schemars(length(max = 10000))]
        skip: Vec<usize>,
    },
    /// Copies at the sites of a grid that lie inside a planar region
    /// (schema 184): the face of `region` (its only face, or the one `face`
    /// selects). The grid passes through `anchor` projected onto the face's
    /// plane, the point of the input that marks its site; its rows run along
    /// `direction` (projected into the plane) every `spacing`, and rows are
    /// `row_spacing` apart. `staggered` shifts every other row by half a
    /// spacing. A site is kept when it lies inside the face at least
    /// `margin` from its boundary, holes included. At most 10,000 sites.
    FillPattern {
        input: String,
        region: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        face: Option<FaceSelector>,
        anchor: VectorExpr,
        direction: VectorExpr,
        spacing: ScalarExpr,
        row_spacing: ScalarExpr,
        #[serde(default)]
        layout: FillPatternLayout,
        margin: ScalarExpr,
    },
    /// Copies moved by each listed offset (schema 184), for uneven spacing.
    /// Include a zero offset to keep the input's own place.
    TablePattern {
        input: String,
        #[schemars(length(min = 1, max = 10000))]
        offsets: Vec<VectorExpr>,
    },
    /// Groups child shapes without fusing or sewing. Useful as a multi-tool
    /// input to Cut. Children keep geometry and locations; overlaps remain.
    Compound {
        #[schemars(length(min = 1, max = 10000))]
        inputs: Vec<String>,
    },
    Sew {
        inputs: Vec<String>,
        tolerance: ScalarExpr,
    },
    MakeSolid {
        shells: Vec<String>,
    },
    Fillet {
        input: String,
        edges: Vec<EdgeSelector>,
        radius: ScalarExpr,
    },
    /// Radius evolution over open tangent contours. With no interior stations,
    /// the law is linear. Otherwise OCCT smoothly interpolates the samples.
    /// Positive length-valued radii and dimensionless station positions.
    VariableFillet {
        input: String,
        edges: Vec<EdgeSelector>,
        start_radius: ScalarExpr,
        end_radius: ScalarExpr,
        #[serde(default)]
        stations: Vec<FilletRadiusStation>,
        #[serde(default)]
        spine_direction: FilletSpineDirection,
    },
    Chamfer {
        input: String,
        edges: Vec<EdgeSelector>,
        distance: ScalarExpr,
    },
    /// Merges adjacent faces on the same surface, and edges on the same
    /// curve, that booleans left split, so later shells, drafts, and
    /// selectors see one face per surface. Surfaces match within
    /// `linear_tolerance` (a length) and `angular_tolerance` (radians, in
    /// (0, pi/2)). Merged faces are recorded as modified, so persistent
    /// references follow them.
    Unify {
        input: String,
        linear_tolerance: ScalarExpr,
        angular_tolerance: ScalarExpr,
    },
    /// Splits `input` by the infinite plane through the length-valued `origin`
    /// with the dimensionless `normal` (schema 120). `keep` selects the side
    /// the normal points to (`positive`, the default), the other side, or
    /// `both` as one compound. The plane must cross the input: each kept side
    /// has positive volume less than the input's, and `both` conserves volume.
    Split {
        input: String,
        origin: VectorExpr,
        normal: VectorExpr,
        #[serde(default)]
        keep: SplitKeep,
    },
    /// Thickens a sheet, the `input` faces or shells with no solids, into a
    /// solid by the positive length `thickness` (schema 121). `side` grows it
    /// along the face normals (`normal`, the default), against them
    /// (`reverse`) or half to each side (`symmetric`). Sharp edges between
    /// faces are mitred. Sheet faces generate their offset faces and free
    /// edges their side walls. At most 10,000 sheet faces.
    Thicken {
        input: String,
        thickness: ScalarExpr,
        #[serde(default)]
        side: ThickenSide,
    },
    /// Joined skin offset. Distance is a signed, nonzero length; tolerance is
    /// a positive length. Uses native join semantics and retained ancestry.
    Offset {
        input: String,
        distance: ScalarExpr,
        tolerance: ScalarExpr,
    },
    Hollow {
        input: String,
        faces: Vec<FaceSelector>,
        thickness: ScalarExpr,
        tolerance: ScalarExpr,
    },
    Draft {
        input: String,
        faces: Vec<FaceSelector>,
        neutral_origin: VectorExpr,
        neutral_normal: VectorExpr,
        pull_direction: VectorExpr,
        angle_radians: ScalarExpr,
    },
    /// Direct edit (schema 183): deletes the selected faces of a solid and
    /// closes the gap from the faces around them, so holes, pockets, bosses,
    /// fillets and chamfers vanish as if never modeled. A selection the
    /// kernel cannot remove completely is an error, never a partial edit.
    RemoveFaces {
        input: String,
        #[schemars(length(min = 1))]
        faces: Vec<FaceSelector>,
    },
    /// Direct edit (schema 183): moves the selected faces along their
    /// outward normals by `distance`, a signed nonzero length (positive adds
    /// material; on a hole's wall it narrows the hole), and re-intersects
    /// the faces around them, which keep their surfaces. `tolerance` is a
    /// positive length. Faces keep their identity for later selectors.
    OffsetFaces {
        input: String,
        #[schemars(length(min = 1))]
        faces: Vec<FaceSelector>,
        distance: ScalarExpr,
        tolerance: ScalarExpr,
    },
    /// Direct edit (schema 183): moves the material the selected faces bound
    /// by `offset`, such as a hole, pocket, boss or rib. The faces are
    /// removed, then the region they took away is cut, and the region they
    /// added is fused, at the moved place.
    MoveFaces {
        input: String,
        #[schemars(length(min = 1))]
        faces: Vec<FaceSelector>,
        offset: VectorExpr,
    },
}

impl FeatureOperation {
    /// Names of family references used by this operation, for authoring tools.
    pub fn named_reference_names(&self) -> Vec<&str> {
        self.reference_names()
            .into_iter()
            .map(|(name, _)| name)
            .collect()
    }

    /// Named references used by this operation's selectors.
    pub fn reference_names(&self) -> Vec<(&str, ReferenceUse)> {
        let mut names = Vec::new();
        match self {
            Self::Extrude {
                extent: ExtrudeExtent::UpToFace { face, .. },
                ..
            }
            | Self::Hole {
                extent: HoleExtent::UpToFace { face },
                ..
            } => face.names(&mut names),
            Self::Fillet { edges, .. }
            | Self::VariableFillet { edges, .. }
            | Self::Chamfer { edges, .. } => {
                for selector in edges {
                    selector.names(&mut names);
                }
            }
            Self::SketchFace { sketch }
            | Self::SketchWire { sketch }
            | Self::SketchOpenWire { sketch } => {
                if let Some(support) = &sketch.face_support {
                    support.face.names(&mut names);
                }
                for projection in &sketch.projections {
                    projection.edge.names(&mut names);
                }
            }
            Self::FillPattern {
                face: Some(face), ..
            } => face.names(&mut names),
            Self::Hollow { faces, .. }
            | Self::Draft { faces, .. }
            | Self::RemoveFaces { faces, .. }
            | Self::OffsetFaces { faces, .. }
            | Self::MoveFaces { faces, .. } => {
                for selector in faces {
                    selector.names(&mut names);
                }
            }
            _ => {}
        }
        names
    }

    /// Inputs whose topology contributes to this output. Selector targets,
    /// face support and projection sources are build dependencies only.
    pub fn topology_inputs(&self) -> Vec<&str> {
        match self {
            Self::SketchFace { .. } | Self::SketchWire { .. } | Self::SketchOpenWire { .. } => {
                Vec::new()
            }
            Self::Extrude { input, .. }
            | Self::Hole { input, .. }
            | Self::Fillet { input, .. }
            | Self::VariableFillet { input, .. }
            | Self::Chamfer { input, .. }
            | Self::Hollow { input, .. }
            | Self::Draft { input, .. }
            | Self::RemoveFaces { input, .. }
            | Self::OffsetFaces { input, .. }
            | Self::MoveFaces { input, .. }
            | Self::CurvePattern { input, .. }
            | Self::FillPattern { input, .. } => vec![input],
            _ => self.dependencies(),
        }
    }

    /// Direct output dependencies, including embedded selectors.
    /// Use `FamilyDefinition::feature_inputs` to expand named references.
    pub fn dependencies(&self) -> Vec<&str> {
        match self {
            Self::Extrude { input, extent, .. } => {
                let mut dependencies = vec![input.as_str()];
                match extent {
                    ExtrudeExtent::UpToFace { target, face } => {
                        dependencies.push(target);
                        face.dependencies(&mut dependencies);
                    }
                    ExtrudeExtent::UpToNext { target } => dependencies.push(target),
                    _ => {}
                }
                dependencies
            }
            Self::Hole { input, extent, .. } => {
                let mut dependencies = vec![input.as_str()];
                if let HoleExtent::UpToFace { face } = extent {
                    face.dependencies(&mut dependencies);
                }
                dependencies
            }
            Self::ProfileLoft {
                profiles, holes, ..
            } => profiles
                .iter()
                .chain(holes.iter().flatten())
                .map(String::as_str)
                .collect(),
            Self::PlanarRegion { outer, holes } => std::iter::once(outer.as_str())
                .chain(holes.iter().map(String::as_str))
                .collect(),
            Self::Sweep {
                profile,
                path,
                guide,
                sections,
                ..
            } => [profile.as_str(), path.as_str()]
                .into_iter()
                .chain(guide.iter().map(|guide| guide.input.as_str()))
                .chain(sections.iter().map(|section| section.profile.as_str()))
                .collect(),
            Self::SheetMetalFlat { input, .. }
            | Self::Translate { input, .. }
            | Self::Rotate { input, .. }
            | Self::Mirror { input, .. }
            | Self::Scale { input, .. }
            | Self::Thread { input, .. }
            | Self::Revolve { input, .. }
            | Self::CircularPattern { input, .. }
            | Self::LinearPattern { input, .. }
            | Self::TablePattern { input, .. }
            | Self::Offset { input, .. }
            | Self::Split { input, .. }
            | Self::Thicken { input, .. }
            | Self::Unify { input, .. } => vec![input],
            Self::CurvePattern { input, path, .. } => vec![input, path],
            Self::FillPattern {
                input,
                region,
                face,
                ..
            } => {
                let mut dependencies = vec![input.as_str(), region.as_str()];
                if let Some(face) = face {
                    face.dependencies(&mut dependencies);
                }
                dependencies
            }
            Self::Fillet { input, edges, .. }
            | Self::VariableFillet { input, edges, .. }
            | Self::Chamfer { input, edges, .. } => {
                let mut dependencies = vec![input.as_str()];
                for selector in edges {
                    selector.dependencies(&mut dependencies);
                }
                dependencies
            }
            Self::Hollow { input, faces, .. }
            | Self::Draft { input, faces, .. }
            | Self::RemoveFaces { input, faces }
            | Self::OffsetFaces { input, faces, .. }
            | Self::MoveFaces { input, faces, .. } => {
                let mut dependencies = vec![input.as_str()];
                for selector in faces {
                    selector.dependencies(&mut dependencies);
                }
                dependencies
            }
            Self::Fuse { left, right } | Self::Common { left, right } => vec![left, right],
            Self::Rib { input, profile, .. } => vec![input, profile],
            Self::Cut { object, tool } => vec![object, tool],
            Self::Compound { inputs } | Self::Sew { inputs, .. } => {
                inputs.iter().map(String::as_str).collect()
            }
            Self::MakeSolid { shells } => shells.iter().map(String::as_str).collect(),
            Self::SketchFace { sketch }
            | Self::SketchWire { sketch }
            | Self::SketchOpenWire { sketch } => {
                let mut inputs = Vec::new();
                if let Some(support) = &sketch.face_support {
                    inputs.push(support.input.as_str());
                    support.face.dependencies(&mut inputs);
                }
                for projection in &sketch.projections {
                    inputs.push(projection.input.as_str());
                    projection.edge.dependencies(&mut inputs);
                }
                inputs
            }
            Self::SheetMetal { .. }
            | Self::Loft { .. }
            | Self::Box { .. }
            | Self::Helix { .. }
            | Self::Cylinder { .. }
            | Self::StandardPart { .. }
            | Self::Cone { .. }
            | Self::Sphere { .. } => Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FeatureDefinition {
    pub id: String,
    pub operation: FeatureOperation,
}

/// How a sweep's twist or scale goes from its start to its end value along
/// the path's length.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SweepLaw {
    /// In proportion to the length swept.
    #[default]
    Linear,
    /// 3t² − 2t³ of the fraction swept: level at both ends.
    Smooth,
}

/// A sweep's turn about its path (schema 175): `angle_radians` (finite,
/// dimensionless) from the start to the end, relative to least-twist
/// transport. Needs the corrected-Frenet orientation and a path without
/// sharp corners.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SweepTwist {
    pub angle_radians: ScalarExpr,
    #[serde(default)]
    pub law: SweepLaw,
}

/// A sweep's size along its path (schema 175): from 1 at the start to the
/// positive dimensionless `end` factor at the end, about the path.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SweepScale {
    pub end: ScalarExpr,
    #[serde(default)]
    pub law: SweepLaw,
}

/// A guide rail (schema 175): the edge or wire output `input`, beside the
/// path and crossing every plane normal to it, which the profile turns to
/// keep facing. With `scales`, the profile is also resized about the path
/// in proportion to the rail's distance, so a profile touching the rail at
/// the start keeps touching it. Needs the corrected-Frenet orientation, and
/// excludes a twist (and, when scaling, a scale law).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SweepGuide {
    pub input: String,
    #[serde(default)]
    pub scales: bool,
}

/// A further profile of a sweep (schema 175): output `profile`, placed
/// across the path at `position`, a dimensionless fraction of the path's
/// length (increasing along the list, the last at 1). The sweep blends
/// smoothly from the profile through each section; a sketch on a
/// `path_plane` datum at the same position keeps a section in place.
/// Sections exclude a scale and a scaling guide, and holed profiles.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SweepSection {
    pub profile: String,
    pub position: ScalarExpr,
}

/// How a swept profile turns as it follows its path.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SweepOrientation {
    /// Least twist; the usual choice.
    #[default]
    CorrectedFrenet,
    /// Follows the path's curvature frame.
    Frenet,
    /// Keeps the profile's normal-side axis along a dimensionless direction.
    Binormal { direction: VectorExpr },
    /// Never rotates: every section stays parallel to the first.
    Fixed,
}

/// A closed planar outline for a loft, in dimensionless profile units (for an
/// airfoil, fractions of chord). Each profile point (u, v) is rotated by
/// `rotation_radians` about `pivot`, counterclockwise from `x_axis` toward
/// `y_axis`, scaled by the length `scale`, and placed at
/// `origin + scale * (u * x_axis + v * y_axis)`. The axes are dimensionless,
/// nonzero, and perpendicular. The first point is not repeated at the end.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct LoftSection {
    pub profile: Vec<[f64; 2]>,
    pub origin: VectorExpr,
    pub x_axis: VectorExpr,
    pub y_axis: VectorExpr,
    pub scale: ScalarExpr,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation_radians: Option<ScalarExpr>,
    #[serde(default, skip_serializing_if = "is_origin")]
    pub pivot: [f64; 2],
}

fn is_origin(point: &[f64; 2]) -> bool {
    *point == [0.0, 0.0]
}
