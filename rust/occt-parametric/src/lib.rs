//! Unit-aware part families, instances, feature graphs, and verification.

mod assembly;
mod definition;
mod document;
mod document_diff;
mod drawing;
mod error;
mod expressions;
mod features;
mod graph;
mod hole_sizes;
mod pattern;
mod quantity;
mod regeneration;
mod selection;
mod sketch;
mod solve;
mod sparse;

pub use assembly::{
    AssemblyJoint, AssemblyMassProperties, AssemblyRelationship, AssemblyRequirement,
    AssemblySemantics, AssemblyVerificationRule, CollisionOptions, ComponentMassProperties,
    Configuration, DatumDefinition, DatumKind, DatumRef, InstanceOutputRef, JointDof, JointKind,
    JointPosition, JointScalar, MAX_MOTION_SAMPLES, Material, MotionResult, MotionSample,
    MotionSampleResult, MotionStudy, PairCheck, PairStatus, PhysicalMassProperties,
    RELATIONSHIP_ANGULAR_TOLERANCE, RELATIONSHIP_LINEAR_TOLERANCE, RelationKind, RelationshipCheck,
    RelationshipTolerances, ResolvedDatum,
};
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

pub use definition::*;
pub use document::*;
pub use document_diff::*;
pub use drawing::*;
pub use error::*;
use expressions::*;
use features::*;
pub use graph::*;
use hole_sizes::clearance_scalar;
pub use hole_sizes::{ClearanceSeries, iso273_clearance_v1};
pub use pattern::*;
pub use quantity::*;
pub use regeneration::*;
use selection::*;

#[cfg(test)]
mod tests;
