//! Unit-aware part families, instances, feature graphs, and verification.
//!
//! Model definitions, documents, solving and checks live in the kernel-free
//! `occt-parametric-core` crate and are re-exported here unchanged; this crate
//! regenerates them with OpenCASCADE through `occt-bridge`.

// Kernel modules extend the core modules of the same name: each imports its
// core counterpart's items and declares kernel-only submodules that shadow
// same-named core submodules, which is intended.
#![allow(hidden_glob_reexports)]

mod assembly;
mod drawing;
mod face_colors;
mod features;
mod graph;
mod mesh;
mod regeneration;
mod selection;
mod sheet_metal;
mod sketch;
mod verification;

pub use occt_parametric_core::*;

pub use occt_bridge::DiagnosticKind;
use occt_bridge::{CurvatureExtrema, HistoryRelation, Session, Shape, ShapeType, Vec3};
use std::collections::{HashMap, HashSet};

pub use assembly::*;
pub use drawing::*;
use face_colors::*;
use features::*;
pub use graph::*;
pub use mesh::*;
pub use regeneration::*;
use selection::*;
pub use sheet_metal::*;
pub use sketch::*;
pub use verification::*;

// Kernel operations on the core model types.
pub use assembly::InstanceGraphClosedMotion;
pub use assembly::InstanceGraphContinuousMotion;
pub use assembly::InstanceGraphDrawView;
pub use assembly::InstanceGraphMass;
pub use assembly::InstanceGraphMaterials;
pub use assembly::InstanceGraphMotion;
pub use assembly::InstanceGraphRequirements;
pub(crate) use assembly::InstanceGraphRequirementsInternal;
pub use assembly::InstanceGraphStepExport;
pub use drawing::{DrawingDefinitionInspection, DrawingDefinitionRendering};
pub use features::PartInstanceHoles;
pub use features::PartInstanceProjection;
pub use features::PartInstanceSupport;
pub use graph::InstanceGraphPatternEdits;
pub use graph::InstanceGraphRegeneration;
pub(crate) use graph::InstanceGraphRegenerationInternal;
pub use graph::ResolvedInstanceRegeneration;
pub use mesh::InstanceGraphExport;
pub use mesh::MeshExportDefinitionMeshExport;
pub use mesh::TaggedSurfaceMeshExport;
pub use regeneration::PartInstanceRegeneration;
pub use selection::PartInstanceSelection;
pub(crate) use sheet_metal::SheetMetalDefinitionUnfoldingInternal;
pub use sketch::SketchDefinitionChecks;
pub use sketch::SketchDefinitionEditing;
pub(crate) use sketch::SketchDefinitionEditingInternal;
pub use sketch::SketchDefinitionProfiles;
pub(crate) use sketch::SketchDefinitionProfilesInternal;
pub use sketch::SketchDefinitionSolving;

/// The kernel operations on core model types, as traits: import with
/// `use occt_parametric::prelude::*;` when not glob-importing the crate.
pub mod prelude {
    pub use super::{
        DrawingDefinitionInspection, DrawingDefinitionRendering, InstanceGraphClosedMotion,
        InstanceGraphContinuousMotion, InstanceGraphDrawView, InstanceGraphExport,
        InstanceGraphMass, InstanceGraphMaterials, InstanceGraphMotion, InstanceGraphPatternEdits,
        InstanceGraphRegeneration, InstanceGraphRequirements, InstanceGraphStepExport,
        MeshExportDefinitionMeshExport, PartInstanceHoles, PartInstanceProjection,
        PartInstanceRegeneration, PartInstanceSelection, PartInstanceSupport,
        ResolvedInstanceRegeneration, SketchDefinitionChecks, SketchDefinitionEditing,
        SketchDefinitionProfiles, SketchDefinitionSolving, TaggedSurfaceMeshExport,
    };
}

#[cfg(test)]
mod tests;
