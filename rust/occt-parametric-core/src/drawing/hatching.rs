pub mod materials;
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct SectionHatching {
    pub angle_radians: f64,
    /// Perpendicular line spacing and phase in paper millimeters.
    pub spacing_mm: f64,
    pub phase_mm: f64,
}

pub fn validate(view: &DrawingView) -> Result<(), ModelError> {
    if view.hatching.is_none() && view.material_hatching.is_empty() {
        return Ok(());
    }
    if let Some(pattern) = view.hatching {
        validate_pattern(pattern)?;
    }
    match view.kind {
        DrawingViewKind::Orthographic => {
            Err(ModelError::new("hatching requires a Slice or Section view"))
        }
        DrawingViewKind::Section { normal, .. } => {
            if dot(axis(normal)?, axis(view.direction)?).abs() < 1.0 - 1e-10 {
                return Err(ModelError::new(
                    "hatched section must look normal to its cut plane",
                ));
            }
            Ok(())
        }
        DrawingViewKind::Slice => Ok(()),
    }
}

fn validate_pattern(pattern: SectionHatching) -> Result<(), ModelError> {
    if !pattern.angle_radians.is_finite()
        || !pattern.spacing_mm.is_finite()
        || pattern.spacing_mm <= 0.0
        || !pattern.phase_mm.is_finite()
    {
        return Err(ModelError::new(
            "section hatching needs a finite angle/phase and positive spacing",
        ));
    }
    Ok(())
}

pub fn section_plane_view(view: &DrawingView) -> Result<DrawingView, ModelError> {
    let DrawingViewKind::Section { origin, .. } = view.kind else {
        return Err(ModelError::new("hatching cut-plane view requires Section"));
    };
    let mut plane = view.clone();
    plane.kind = DrawingViewKind::Slice;
    plane.origin = origin;
    plane.hatching = None;
    plane.material_hatching.clear();
    Ok(plane)
}
impl Default for SectionHatching {
    fn default() -> Self {
        Self {
            angle_radians: std::f64::consts::FRAC_PI_4,
            spacing_mm: 3.0,
            phase_mm: 0.0,
        }
    }
}
