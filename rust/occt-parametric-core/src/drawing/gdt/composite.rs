//! Composite position/profile controls: an upper segment and one or more
//! lower segments sharing one characteristic.
use super::*;
/// A lower segment of a composite control. Each refines the segment above
/// it; `refinement` holds the next lower segment (schema 141).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DrawingCompositeRefinement {
    pub tolerance: Quantity,
    #[serde(default)]
    pub datums: Vec<DrawingDatumReference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refinement: Option<Box<DrawingCompositeRefinement>>,
}
/// The control's next lower segment as a control of its own, carrying any
/// segments below it.
pub fn refined(frame: &DrawingFeatureControlFrame) -> DrawingFeatureControlFrame {
    let refinement = frame.refinement.as_ref().expect("composite refinement");
    let mut lower = frame.clone();
    lower.tolerance = refinement.tolerance;
    lower.datums = refinement.datums.clone();
    lower.datum_reference_frame = None;
    lower.refinement = refinement.refinement.as_deref().cloned();
    // The lower segment's zone is equally disposed.
    lower.unequal = None;
    lower
}
/// The control followed by each of its lower segments, top to bottom.
pub fn segments(frame: &DrawingFeatureControlFrame) -> Vec<DrawingFeatureControlFrame> {
    let mut out = vec![frame.clone()];
    while out.last().is_some_and(|f| f.refinement.is_some()) {
        let lower = refined(out.last().expect("segment"));
        out.push(lower);
    }
    out
}
/// Each lower segment must be tighter than the one above it, and its datums
/// an unchanged prefix of the upper segment's.
pub fn validate(
    frame: &DrawingFeatureControlFrame,
    index: &HashMap<&str, &DrawingDatumFeature>,
) -> Result<(), ModelError> {
    if frame.refinement.is_none() {
        return Ok(());
    }
    if frame.characteristic != GeometricCharacteristic::Position && !frame.characteristic.profile()
    {
        return Err(ModelError::new(
            "composite refinement supports position and profile controls",
        ));
    }
    let segments = segments(frame);
    for pair in segments.windows(2) {
        let (upper, lower) = (&pair[0], &pair[1]);
        number(lower)?;
        if lower.tolerance.normalized()? >= upper.tolerance.normalized()?
            || displayed(lower)? >= displayed(upper)?
        {
            return Err(ModelError::new(
                "composite refinement must be tighter before and after display rounding",
            ));
        }
        if !frame.datums.starts_with(&lower.datums) {
            return Err(ModelError::new(
                "composite refinement datums must be an unchanged prefix of upper datums",
            ));
        }
        validate_references(&lower.datums, index)?;
    }
    Ok(())
}
fn displayed(f: &DrawingFeatureControlFrame) -> Result<f64, ModelError> {
    number(f)?
        .split_whitespace()
        .next()
        .unwrap_or("")
        .parse()
        .map_err(|_| ModelError::new("invalid displayed composite tolerance"))
}
/// Draws the composite into `d[0]`, or with one drawing per segment, the
/// outline, characteristic and leader with the top row into `d[0]` and each
/// lower row with the line above it into its own.
pub fn append(
    f: &DrawingFeatureControlFrame,
    index: &HashMap<&str, &DrawingDatumFeature>,
    point: [f64; 2],
    p: [f64; 2],
    d: &mut [GeneratedDrawing],
) -> Result<(), ModelError> {
    // Rows stack downward from the upper segment, the lowest at `p`; the
    // characteristic cell spans them all and the right edge steps.
    let rows = segments(f)
        .into_iter()
        .map(|segment| {
            let value = number(&segment)?;
            let cells = cell_widths(&segment, &value, index);
            Ok((segment, value, cells))
        })
        .collect::<Result<Vec<_>, ModelError>>()?;
    let n = rows.len();
    let height = 8.0 * n as f64;
    let width = |i: usize| rows[i].2.iter().sum::<f64>();
    let [x, y] = p;
    let mut outline = vec![[x, y]];
    for i in (0..n).rev() {
        let bottom = y + 8.0 * (n - 1 - i) as f64;
        outline.extend([[x + width(i), bottom], [x + width(i), bottom + 8.0]]);
    }
    outline.extend([[x, y + height], [x, y]]);
    let split = d.len() > 1;
    let at = |i: usize| if split { i } else { 0 };
    line(&mut d[0], outline);
    line(&mut d[0], vec![[x + 8.0, y], [x + 8.0, y + height]]);
    for i in 1..n {
        let top = y + 8.0 * (n - i) as f64;
        line(&mut d[at(i)], vec![[x + 8.0, top], [x + width(i), top]]);
    }
    leader(&mut d[0], point, p, false);
    glyph(&mut d[0], [x + 4.0, y + 0.5 * height], f.characteristic);
    for (i, (segment, value, cells)) in rows.iter().enumerate() {
        let bottom = y + 8.0 * (n - 1 - i) as f64;
        append_row(
            segment,
            [x, bottom],
            value,
            cells,
            index,
            &mut d[at(i)],
            false,
        );
    }
    Ok(())
}
