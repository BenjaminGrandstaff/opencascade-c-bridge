use super::*;
use occt_parametric_core::sketch::geometry;
#[allow(unused_imports)]
pub use occt_parametric_core::sketch::*;
use occt_parametric_core::sparse::SparseJacobian;
use occt_types::CurveSegment;
mod diagnostics;
pub use diagnostics::SketchDefinitionChecks;
pub use diagnostics::{SketchEllipseCircleContact, SketchSplineCircleContact};
mod discovery;
pub(crate) use discovery::SketchDefinitionDiscoveryInternal;
pub(crate) mod editing;
pub use editing::SketchDefinitionEditing;
pub(crate) use editing::SketchDefinitionEditingInternal;
mod profile;
pub use profile::SketchDefinitionProfiles;
pub(crate) use profile::SketchDefinitionProfilesInternal;
mod solver;
pub use solver::SketchDefinitionSolving;

#[cfg(test)]
mod tests;
