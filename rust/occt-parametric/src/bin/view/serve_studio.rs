//! The sketch studio page (`/studio`) and its modules. They use the same
//! JSON API as the classic viewer; only the page and its scripts are new.
use super::Response;

/// Each studio module, served at `/<name>` from `web/<name>`.
macro_rules! studio_assets {
    ($($name:literal),* $(,)?) => {
        &[$((concat!("/", $name), include_str!(concat!("web/", $name)))),*]
    };
}

const STUDIO_ASSETS: &[(&str, &str)] = studio_assets!(
    "sketch-studio-app.mjs",
    "sketch-studio-plane.mjs",
    "sketch-studio-layers.mjs",
    "sketch-studio-overlay.mjs",
    "sketch-studio-infer.mjs",
    "sketch-studio-geometry.mjs",
    "sketch-studio-tools.mjs",
    "sketch-studio-primitives.mjs",
    "sketch-studio-draw.mjs",
    "sketch-studio-select.mjs",
    "sketch-studio-constrain.mjs",
    "sketch-studio-labels.mjs",
    "sketch-studio-extras.mjs",
    "sketch-studio-planes.mjs",
    "sketch-studio-plane-panel.mjs",
    "sketch-studio-delete.mjs",
    "sketch-studio-picks.mjs",
    "sketch-studio-weighted.mjs",
    "sketch-studio-profile.mjs",
    "sketch-studio-pattern.mjs",
    "sketch-studio-links.mjs",
    "sketch-studio-sources.mjs",
    "sketch-studio-modify.mjs",
    "sketch-studio-trim.mjs",
);

/// The studio page or one of its scripts, or `None` for other paths.
pub(super) fn asset(path: &str) -> Option<Response> {
    if path == "/studio" || path == "/studio/" {
        return Some(Response::text(
            200,
            "text/html; charset=utf-8",
            include_str!("web/sketch-studio.html"),
        ));
    }
    STUDIO_ASSETS
        .iter()
        .find(|(name, _)| *name == path)
        .map(|(_, body)| Response::text(200, "text/javascript", body))
}
