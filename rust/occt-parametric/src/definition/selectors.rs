//! Edge and face selectors and the history relations they resolve through.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SemanticHistoryRelation {
    Generated,
    Modified,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CoordinateAxis {
    X,
    Y,
    Z,
}

impl CoordinateAxis {
    pub(crate) fn component(self, point: Vec3) -> f64 {
        match self {
            Self::X => point.x,
            Self::Y => point.y,
            Self::Z => point.z,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Extremum {
    Minimum,
    Maximum,
}

impl From<SemanticHistoryRelation> for HistoryRelation {
    fn from(value: SemanticHistoryRelation) -> Self {
        match value {
            SemanticHistoryRelation::Generated => Self::Generated,
            SemanticHistoryRelation::Modified => Self::Modified,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EdgeSelector {
    NearestCenter {
        target: VectorExpr,
        maximum_distance: ScalarExpr,
    },
    AtExtreme {
        axis: CoordinateAxis,
        extremum: Extremum,
        tolerance: ScalarExpr,
    },
    Longest {
        allow_ties: bool,
        relative_tolerance: ScalarExpr,
    },
    CircularRadius {
        minimum: ScalarExpr,
        maximum: ScalarExpr,
    },
    CurvatureRadius {
        minimum: ScalarExpr,
        maximum: ScalarExpr,
    },
    CurvatureRadiusRange {
        minimum: ScalarExpr,
        maximum: ScalarExpr,
        sample_count: usize,
        require_entire_edge: bool,
    },
    /// Curvature-radius range decided from exact or error-bounded extrema.
    /// Edges whose bounds straddle the range fail instead of guessing.
    CurvatureRadiusBounds {
        minimum: ScalarExpr,
        maximum: ScalarExpr,
        relative_tolerance: ScalarExpr,
        require_entire_edge: bool,
    },
    Union(Vec<EdgeSelector>),
    Intersection(Vec<EdgeSelector>),
    Difference {
        base: Box<EdgeSelector>,
        subtract: Box<EdgeSelector>,
    },
    History {
        source_feature: String,
        source: Box<EdgeSelector>,
        relation: SemanticHistoryRelation,
    },
    /// The family's named edge reference (see [`FamilyDefinition::references`]).
    Named(String),
    /// Edges chosen by `select` on `feature`'s own output, followed through
    /// every later feature like [`FaceSelector::Persistent`].
    Persistent {
        feature: String,
        select: Box<EdgeSelector>,
    },
}

impl EdgeSelector {
    /// Named references used anywhere in this selector.
    pub(crate) fn names<'a>(&'a self, names: &mut Vec<(&'a str, ReferenceUse)>) {
        match self {
            Self::Named(name) => names.push((name, ReferenceUse::Edges)),
            Self::Union(selectors) | Self::Intersection(selectors) => {
                for selector in selectors {
                    selector.names(names);
                }
            }
            Self::Difference { base, subtract } => {
                base.names(names);
                subtract.names(names);
            }
            Self::History { source, .. } => source.names(names),
            Self::Persistent { select, .. } => select.names(names),
            _ => {}
        }
    }

    pub(crate) fn dependencies<'a>(&'a self, dependencies: &mut Vec<&'a str>) {
        match self {
            Self::Persistent { feature, select } => {
                dependencies.push(feature);
                select.dependencies(dependencies);
            }
            Self::History {
                source_feature,
                source,
                ..
            } => {
                dependencies.push(source_feature);
                source.dependencies(dependencies);
            }
            Self::Union(selectors) | Self::Intersection(selectors) => {
                for selector in selectors {
                    selector.dependencies(dependencies);
                }
            }
            Self::Difference { base, subtract } => {
                base.dependencies(dependencies);
                subtract.dependencies(dependencies);
            }
            _ => {}
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FaceSelector {
    NearestCenter {
        target: VectorExpr,
        maximum_distance: ScalarExpr,
    },
    AtExtreme {
        axis: CoordinateAxis,
        extremum: Extremum,
        tolerance: ScalarExpr,
    },
    NormalAligned {
        direction: VectorExpr,
        minimum_dot: ScalarExpr,
    },
    LargestArea {
        planar_only: bool,
        allow_ties: bool,
        relative_tolerance: ScalarExpr,
    },
    AdjacentToEdges {
        edges: Box<EdgeSelector>,
        minimum_count: usize,
    },
    /// Faces generated from selected edges of an earlier feature, including
    /// rib profile edges traced through extrusion, placement, and fusion.
    GeneratedFromEdges {
        source_feature: String,
        source: Box<EdgeSelector>,
    },
    /// Faces meeting at least `minimum_count` of `faces` with G1 or better
    /// continuity. Continuity recorded on the shared edge is used as is; with
    /// an `angular_tolerance` (radians), an unrecorded edge is measured too,
    /// which finds tangent junctions that booleans create.
    TangentTo {
        faces: Box<FaceSelector>,
        minimum_count: usize,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        angular_tolerance: Option<ScalarExpr>,
    },
    Union(Vec<FaceSelector>),
    Intersection(Vec<FaceSelector>),
    Difference {
        base: Box<FaceSelector>,
        subtract: Box<FaceSelector>,
    },
    History {
        source_feature: String,
        source: Box<FaceSelector>,
        relation: SemanticHistoryRelation,
    },
    /// The family's named face reference (see [`FamilyDefinition::references`]).
    Named(String),
    /// Faces chosen by `select` on `feature`'s own output, where the rule is
    /// unambiguous, then followed through every later feature to the one
    /// being built: unchanged faces carry over, modified faces map to their
    /// replacements (a split face yields every piece), and a face that a later
    /// feature removes fails, naming that feature. A stable reference that
    /// survives parameter edits, booleans, and placements downstream.
    Persistent {
        feature: String,
        select: Box<FaceSelector>,
    },
}

impl FaceSelector {
    /// Named references used anywhere in this selector.
    pub(crate) fn names<'a>(&'a self, names: &mut Vec<(&'a str, ReferenceUse)>) {
        match self {
            Self::Named(name) => names.push((name, ReferenceUse::Faces)),
            Self::AdjacentToEdges { edges, .. } => edges.names(names),
            Self::GeneratedFromEdges { source, .. } => source.names(names),
            Self::TangentTo { faces, .. } => faces.names(names),
            Self::Union(selectors) | Self::Intersection(selectors) => {
                for selector in selectors {
                    selector.names(names);
                }
            }
            Self::Difference { base, subtract } => {
                base.names(names);
                subtract.names(names);
            }
            Self::History { source, .. } => source.names(names),
            Self::Persistent { select, .. } => select.names(names),
            Self::NearestCenter { .. }
            | Self::AtExtreme { .. }
            | Self::NormalAligned { .. }
            | Self::LargestArea { .. } => {}
        }
    }

    pub(crate) fn dependencies<'a>(&'a self, dependencies: &mut Vec<&'a str>) {
        match self {
            // Added by dependencies_with_references, which sees the family.
            Self::Named(_) => {}
            Self::Persistent { feature, select } => {
                dependencies.push(feature);
                select.dependencies(dependencies);
            }
            Self::History {
                source_feature,
                source,
                ..
            } => {
                dependencies.push(source_feature);
                source.dependencies(dependencies);
            }
            Self::AdjacentToEdges { edges, .. } => edges.dependencies(dependencies),
            Self::GeneratedFromEdges {
                source_feature,
                source,
            } => {
                dependencies.push(source_feature);
                source.dependencies(dependencies);
            }
            Self::TangentTo { faces, .. } => faces.dependencies(dependencies),
            Self::Union(selectors) | Self::Intersection(selectors) => {
                for selector in selectors {
                    selector.dependencies(dependencies);
                }
            }
            Self::Difference { base, subtract } => {
                base.dependencies(dependencies);
                subtract.dependencies(dependencies);
            }
            Self::NearestCenter { .. }
            | Self::AtExtreme { .. }
            | Self::NormalAligned { .. }
            | Self::LargestArea { .. } => {}
        }
    }
}
