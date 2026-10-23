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

impl Studio {
    /// The shown output of `instance` as a DXF cutting profile: its largest
    /// planar face, with a flat pattern's bend lines. Read-only.
    pub(super) fn output_dxf(&self, body: &[u8]) -> Result<Vec<u8>, String> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Request {
            version: u64,
            instance: String,
        }
        let request: Request =
            serde_json::from_slice(body).map_err(|e| format!("invalid DXF export: {e}"))?;
        if request.version != self.version {
            return Err("The model changed. Export again.".into());
        }
        if !primary_instances(&self.document)?.contains(&request.instance) {
            return Err("Unknown instance.".into());
        }
        let output = match &self.output {
            Some(output) => output.clone(),
            None => self
                .document
                .family
                .history_output()
                .map_err(|e| e.to_string())?
                .ok_or("No output is available to export.")?
                .to_owned(),
        };
        let mut graph = self.document.instance_graph().map_err(|e| e.to_string())?;
        let session = Session::new().map_err(|e| e.to_string())?;
        let generation = graph.regenerate_all(&session).map_err(|e| e.to_string())?;
        let target = occt_parametric::InstanceOutputRef {
            instance: request.instance,
            output,
        };
        let (text, _) = occt_parametric::dxf_export::output_to_dxf(
            &session,
            &graph,
            &generation,
            &target,
            None,
            true,
        )
        .map_err(|e| e.message)?;
        Ok(text.into_bytes())
    }
}
