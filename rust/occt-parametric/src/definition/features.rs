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

/// Caller-supplied internal-thread intent, not a standards lookup or modeled
/// helix. The hole diameter remains the explicit cylindrical bore diameter.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ThreadSpecification {
    pub designation: String,
    pub nominal_diameter: ScalarExpr,
    pub pitch: ScalarExpr,
    pub handedness: ThreadHandedness,
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
    },
    Box {
        origin: VectorExpr,
        size: VectorExpr,
    },
    /// Sweeps the `profile` output (a wire, or a planar face including holes) along
    /// the `path` output (an edge or wire). Place the profile at the path's
    /// start, usually across it. A face or closed wire makes a solid. Holed
    /// faces sweep each boundary and subtract the contained inner volumes.
    Sweep {
        profile: String,
        path: String,
        #[serde(default)]
        orientation: SweepOrientation,
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
    },
    Cylinder {
        origin: VectorExpr,
        axis: VectorExpr,
        radius: ScalarExpr,
        height: ScalarExpr,
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
    /// diameter. Right-handed unless `left_handed` (schema 84).
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
    },
    /// Repeats the input at i*step, including i=0, into an unfused compound.
    /// Step is a length vector. Count is an integer-valued scalar in 1..=10000.
    LinearPattern {
        input: String,
        step: VectorExpr,
        count: ScalarExpr,
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
}

impl FeatureOperation {
    /// Named references used by this operation's selectors.
    pub(crate) fn reference_names(&self) -> Vec<(&str, ReferenceUse)> {
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
            Self::Hollow { faces, .. } | Self::Draft { faces, .. } => {
                for selector in faces {
                    selector.names(&mut names);
                }
            }
            _ => {}
        }
        names
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
            Self::Sweep { profile, path, .. } => vec![profile, path],
            Self::SheetMetalFlat { input, .. }
            | Self::Translate { input, .. }
            | Self::Rotate { input, .. }
            | Self::Mirror { input, .. }
            | Self::Scale { input, .. }
            | Self::Thread { input, .. }
            | Self::Revolve { input, .. }
            | Self::CircularPattern { input, .. }
            | Self::LinearPattern { input, .. }
            | Self::Offset { input, .. }
            | Self::Unify { input, .. } => vec![input],
            Self::Fillet { input, edges, .. }
            | Self::VariableFillet { input, edges, .. }
            | Self::Chamfer { input, edges, .. } => {
                let mut dependencies = vec![input.as_str()];
                for selector in edges {
                    selector.dependencies(&mut dependencies);
                }
                dependencies
            }
            Self::Hollow { input, faces, .. } | Self::Draft { input, faces, .. } => {
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
