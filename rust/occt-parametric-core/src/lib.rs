//! Kernel-free part of the parametric engine: unit-aware definitions, model
//! documents, expressions, sketch and placement constraint solving, document
//! diff and merge, drawing and GD&T data, and validation. It describes and
//! checks models without linking the geometry kernel; `occt-parametric`
//! regenerates them with OpenCASCADE and re-exports this crate's API.
//!
//! Modules are public so the kernel crate can extend them; the supported API
//! is the root re-exports.

#[doc(hidden)]
pub mod assembly;
#[doc(hidden)]
pub mod change_impact;
#[doc(hidden)]
pub mod definition;
pub mod digest;
#[doc(hidden)]
pub mod document;
#[doc(hidden)]
pub mod document_diff;
#[doc(hidden)]
pub mod drawing;
#[doc(hidden)]
pub mod error;
#[doc(hidden)]
pub mod expressions;
#[doc(hidden)]
pub mod graph;
#[doc(hidden)]
pub mod hole_sizes;
#[doc(hidden)]
pub mod mesh;
#[doc(hidden)]
pub mod pattern;
#[doc(hidden)]
pub mod quantity;
#[doc(hidden)]
pub mod regeneration;
#[doc(hidden)]
pub mod revisions;
pub mod screw_threads;
#[doc(hidden)]
pub mod sheet_metal;
#[doc(hidden)]
pub mod sketch;
#[doc(hidden)]
pub mod solve;
#[doc(hidden)]
pub mod sparse;
#[doc(hidden)]
pub mod verification;

pub use assembly::{
    AssemblyJoint, AssemblyMassProperties, AssemblyRelationship, AssemblyRequirement,
    AssemblySemantics, AssemblyVerificationRule, BalancePosition, ChordReference, CollisionOptions,
    CollisionPairRef, ComponentMassProperties, Configuration, ContinuousCollisionOptions,
    ContinuousMotionResult, ContinuousPairResult, ContinuousStatus, DatumDefinition, DatumKind,
    DatumRef, InstanceOutputRef, JointDof, JointKind, JointPosition, JointScalar,
    MAX_MOTION_SAMPLES, Material, MaterialMassProperties, MeanAerodynamicChord, MotionResult,
    MotionSample, MotionSampleResult, MotionStudy, OutputSet, PairCheck, PairStatus,
    PhysicalMassProperties, PlanformStation, RELATIONSHIP_ANGULAR_TOLERANCE,
    RELATIONSHIP_LINEAR_TOLERANCE, RelationKind, RelationshipCheck, RelationshipTolerances,
    ResolvedDatum, SymmetricWingPlanform,
};
pub use assembly::{
    ClosedMotionOptions, ClosedMotionResult, JointBranchEquivalence, JointBranchSearchOptions,
    JointBranchSearchResult, JointBranchSearchStatus, JointMotionSolution, JointMotionStatus,
    JointSeedAxis, JointSolution, JointSolveOptions, JointVariable,
};
pub use assembly::{
    NoFaces, PlaneBase, PlaneFaces, StandInFaces, datum_chain, datum_dependencies, datum_index,
    datum_reads_faces, plane_x_axis, project_into_plane, resolve_datum,
};
pub use screw_threads::{
    ThreadStrength, UnifiedClass, UnifiedEngagement, UnifiedFit, UnifiedLimits, UnifiedSeries,
    UnifiedThread, UnifiedThreadSpec, unified_engagement, unified_fit,
};
pub use sketch::{
    ResolvedSketch, ResolvedSketches, SketchAcceptance, SketchArc, SketchBSplineBasis,
    SketchBoundaryOperation, SketchCircle, SketchCircleTangency, SketchConflictGroup,
    SketchConstraint, SketchConstraintCheck, SketchCoordinateAxis, SketchCurveEnd,
    SketchCurveOperation, SketchDefinition, SketchDiagnosis, SketchEditedCurve, SketchEllipse,
    SketchEquationSource, SketchFaceSupport, SketchFontStyle, SketchFreeCoordinate,
    SketchFreeMotion, SketchLine, SketchLineSide, SketchMeasure, SketchMeasurement,
    SketchOffsetJoin, SketchPattern, SketchPoint, SketchPoint2, SketchPointMotion,
    SketchProfileOperation, SketchProjection, SketchProjectionIntersection,
    SketchProjectionJunction, SketchProjectionKind, SketchProjectionRefinement, SketchRegion,
    SketchRegionDiscovery, SketchRegionEdit, SketchShape, SketchSolution, SketchSolverReport,
    SketchSpline, SketchSplineCircleContact, SketchSplineEndpoints, SketchSupportPlanes,
    SketchTermination, SketchText, SketchTextAlign, SketchTextOutlines, SketchTrimBound,
    SketchWeightedPointTerm,
};
pub use solve::PlacementSolution;

pub use occt_types::DiagnosticKind;
use occt_types::{BridgeError, Diagnostic, HistoryRelation, Vec3};
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
pub use expressions::*;
pub use graph::*;
pub use hole_sizes::clearance_scalar;
pub use hole_sizes::{
    ClearanceSeries, HoleCatalogSystem, SocketHeadDimension, SocketHeadRecess, TapSize,
    carr_lane_socket_dimension_v1, carr_lane_socket_head_v1, carr_lane_tap_drill_v1,
    carr_lane_tap_sizes_v1, iso273_clearance_v1,
};
pub use mesh::*;
pub use pattern::*;
pub use quantity::*;
pub use regeneration::*;
pub use revisions::*;
pub use sheet_metal::*;
pub use verification::*;
