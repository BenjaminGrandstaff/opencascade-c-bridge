//! Unit-aware part families, instances, feature graphs, and verification.

mod assembly;
mod change_impact;
mod definition;
mod document;
mod document_diff;
mod drawing;
mod error;
mod expressions;
mod features;
mod graph;
mod hole_sizes;
mod mesh;
mod pattern;
mod quantity;
mod regeneration;
mod revisions;
mod selection;
mod sheet_metal;
mod sketch;
mod solve;
mod sparse;

pub use assembly::{
    AssemblyJoint, AssemblyMassProperties, AssemblyRelationship, AssemblyRequirement,
    AssemblySemantics, AssemblyVerificationRule, CollisionOptions, ComponentMassProperties,
    Configuration, ContinuousCollisionOptions, ContinuousMotionResult, ContinuousPairResult,
    ContinuousStatus, DatumDefinition, DatumKind, DatumRef, InstanceOutputRef, JointDof, JointKind,
    JointPosition, JointScalar, MAX_MOTION_SAMPLES, Material, MotionResult, MotionSample,
    MotionSampleResult, MotionStudy, PairCheck, PairStatus, PhysicalMassProperties,
    RELATIONSHIP_ANGULAR_TOLERANCE, RELATIONSHIP_LINEAR_TOLERANCE, RelationKind, RelationshipCheck,
    RelationshipTolerances, ResolvedDatum,
};
pub use assembly::{JointSolution, JointSolveOptions, JointVariable};
pub use sketch::{
    SketchArc, SketchCircle, SketchConstraint, SketchDefinition, SketchLine, SketchPoint,
    SketchPoint2, SketchSolution,
};
pub use solve::PlacementSolution;

pub use occt_bridge::DiagnosticKind;
use occt_bridge::{
    BridgeError, CurvatureExtrema, Diagnostic, HistoryRelation, Session, Shape, ShapeType, Vec3,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    error::Error,
    fmt,
};

pub use change_impact::*;
pub use definition::*;
pub use document::*;
pub use document_diff::*;
pub use drawing::*;
pub use error::*;
use expressions::*;
use features::*;
pub use graph::*;
use hole_sizes::clearance_scalar;
pub use hole_sizes::{
    ClearanceSeries, HoleCatalogSystem, SocketHeadDimension, SocketHeadRecess,
    carr_lane_socket_dimension_v1, carr_lane_socket_head_v1, carr_lane_tap_drill_v1,
    iso273_clearance_v1,
};
pub use mesh::*;
pub use pattern::*;
pub use quantity::*;
pub use regeneration::*;
pub use revisions::*;
use selection::*;
pub use sheet_metal::*;

#[cfg(test)]
mod tests;
