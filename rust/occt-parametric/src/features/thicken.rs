//! Thickened sheets: faces or shells closed into a solid by one native
//! intersection-joined skin offset (two when symmetric). O(sheet faces)
//! native work; sheet faces generate their offset faces and free edges their
//! side walls, so later selectors can follow both.
use super::*;

/// Offset confusion tolerance for exact model geometry in millimetres.
const TOLERANCE: f64 = 1e-6;

/// Largest sheet thickened in one feature.
const MAX_FACES: usize = 10_000;

pub(super) fn execute<'session>(
    session: &'session Session,
    input: &Shape<'_>,
    thickness: f64,
    side: ThickenSide,
) -> Result<Shape<'session>, ModelError> {
    if !thickness.is_finite() || thickness <= 0.0 {
        return Err(ModelError::new(
            "thicken thickness must be a positive finite length",
        ));
    }
    let faces = session.subshape_count(input, ShapeType::Face)?;
    if faces == 0 || session.subshape_count(input, ShapeType::Solid)? > 0 {
        return Err(ModelError::new(
            "thicken input must be a sheet: faces or shells without solids",
        ));
    }
    if faces > MAX_FACES {
        return Err(ModelError::new(format!(
            "thicken is limited to {MAX_FACES} sheet faces; the input has {faces}"
        )));
    }
    let (signed, symmetric) = match side {
        ThickenSide::Normal => (thickness, false),
        ThickenSide::Reverse => (-thickness, false),
        ThickenSide::Symmetric => (thickness, true),
    };
    session
        .thicken(input, signed, symmetric, TOLERANCE)
        .map_err(|error| ModelError::from(error).context("thicken"))
}
