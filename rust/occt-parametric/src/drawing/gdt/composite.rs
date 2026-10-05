//! Initial two-segment composite position/profile controls, shared characteristic.
use super::*;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DrawingCompositeRefinement {
    pub tolerance: Quantity,
    #[serde(default)]
    pub datums: Vec<DrawingDatumReference>,
}
pub(super) fn refined(frame: &DrawingFeatureControlFrame) -> DrawingFeatureControlFrame {
    let refinement = frame.refinement.as_ref().expect("composite refinement");
    let mut lower = frame.clone();
    lower.tolerance = refinement.tolerance;
    lower.datums = refinement.datums.clone();
    lower.datum_reference_frame = None;
    lower.refinement = None;
    lower
}
pub(super) fn validate(
    frame: &DrawingFeatureControlFrame,
    index: &HashMap<&str, &DrawingDatumFeature>,
) -> Result<(), ModelError> {
    let Some(refinement) = &frame.refinement else {
        return Ok(());
    };
    if frame.characteristic != GeometricCharacteristic::Position && !frame.characteristic.profile()
    {
        return Err(ModelError::new(
            "composite refinement supports position and profile controls",
        ));
    }
    let lower = refined(frame);
    number(&lower)?;
    if lower.tolerance.normalized()? >= frame.tolerance.normalized()?
        || displayed(&lower)? >= displayed(frame)?
    {
        return Err(ModelError::new(
            "composite refinement must be tighter before and after display rounding",
        ));
    }
    if !frame.datums.starts_with(&refinement.datums) {
        return Err(ModelError::new(
            "initial composite refinement datums must be an unchanged prefix of upper datums",
        ));
    }
    validate_references(&refinement.datums, index)
}
fn displayed(f: &DrawingFeatureControlFrame) -> Result<f64, ModelError> {
    number(f)?
        .split_whitespace()
        .next()
        .unwrap_or("")
        .parse()
        .map_err(|_| ModelError::new("invalid displayed composite tolerance"))
}
pub(super) fn append(
    f: &DrawingFeatureControlFrame,
    index: &HashMap<&str, &DrawingDatumFeature>,
    point: [f64; 2],
    p: [f64; 2],
    d: &mut GeneratedDrawing,
) -> Result<(), ModelError> {
    let lower = refined(f);
    let upper_value = number(f)?;
    let lower_value = number(&lower)?;
    let upper_cells = cell_widths(f, &upper_value, index);
    let lower_cells = cell_widths(&lower, &lower_value, index);
    let upper_width = upper_cells.iter().sum::<f64>();
    let lower_width = lower_cells.iter().sum::<f64>();
    let [x, y] = p;
    line(
        d,
        vec![
            [x, y],
            [x + lower_width, y],
            [x + lower_width, y + 8.0],
            [x + upper_width, y + 8.0],
            [x + upper_width, y + 16.0],
            [x, y + 16.0],
            [x, y],
        ],
    );
    line(d, vec![[x + 8.0, y], [x + 8.0, y + 16.0]]);
    line(d, vec![[x + 8.0, y + 8.0], [x + lower_width, y + 8.0]]);
    leader(d, point, p, false);
    glyph(d, [x + 4.0, y + 8.0], f.characteristic);
    append_row(f, [x, y + 8.0], &upper_value, &upper_cells, index, d, false);
    append_row(&lower, p, &lower_value, &lower_cells, index, d, false);
    Ok(())
}
