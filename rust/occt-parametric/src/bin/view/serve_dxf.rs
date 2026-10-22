//! DXF files read into a new sketch layer's first draft. Read-only: the
//! draft is previewed and applied like any drawn sketch.
use super::*;
use occt_parametric::dxf::{DxfImportOptions, import_dxf};

/// Largest DXF upload, in bytes (the request body limit for this path).
pub(super) const MAX_DXF_UPLOAD_BYTES: usize = 32 * 1024 * 1024;

impl Studio {
    pub(super) fn dxf_import(&self, body: &[u8]) -> Result<Value, String> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Request {
            dxf: String,
            #[serde(default)]
            options: Option<DxfImportOptions>,
        }
        let request: Request =
            serde_json::from_slice(body).map_err(|e| format!("invalid DXF import: {e}"))?;
        let import = import_dxf(&request.dxf, &request.options.unwrap_or_default())
            .map_err(|e| e.message)?;
        Ok(json!({"version": self.version, "sketch": import.sketch, "report": import.report}))
    }
}
