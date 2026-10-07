//! Versioned model documents: validation and schema migration.

use super::*;

/// A named assembly coordinate frame. Its placement maps frame-local
/// coordinates into the parent frame, or into model coordinates at the root.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AssemblyFrame {
    pub id: String,
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default)]
    pub placement: Placement,
    pub provenance: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GenerationRecord {
    pub instance_id: String,
    pub attempted_revision: u64,
    pub accepted_revision: Option<u64>,
    pub state: RegenerationState,
    pub last_error: Option<String>,
}

pub const CURRENT_SCHEMA_VERSION: u32 = 69;
