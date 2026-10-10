//! Shared paper-space text layout for SVG/DXF and dimension clearance.
//! Work/storage are O(label characters), independent of graph size. Widths are
//! deterministic advance estimates, not font-specific glyph measurements.
use super::*;

// Approximate upright sans-serif advances. Wide unit letters must not use the
// numeric advance (notably `mm` next to a vertical dimension line).
fn width(text: &str, size: f64) -> f64 {
    text.chars()
        .map(|c| {
            size * match c {
                'm' | 'w' | 'M' | 'W' => 0.95,
                '0'..='9' => 0.6,
                ' ' => 0.35,
                '.' | ',' | '(' | ')' | '/' | '-' => 0.4,
                '±' | 'Ø' => 0.8,
                '°' => 0.5,
                c if c.is_ascii_uppercase() => 0.75,
                c if c.is_ascii() => 0.65,
                _ => 1.0,
            }
        })
        .sum()
}

pub(super) fn parts(label: &DrawingLabel) -> Vec<([f64; 2], String, f64)> {
    let [x, y] = label.position_mm;
    let Some(stack) = &label.stack else {
        return vec![(label.position_mm, label.text.clone(), 3.0)];
    };
    let column = x + width(&stack.prefix, 3.0) + 1.0;
    let suffix_x = column + width(&stack.upper, 2.2).max(width(&stack.lower, 2.2)) + 1.0;
    vec![
        ([x, y], stack.prefix.clone(), 3.0),
        ([column, y + 1.8], stack.upper.clone(), 2.2),
        ([column, y - 1.8], stack.lower.clone(), 2.2),
        ([suffix_x, y], stack.suffix.clone(), 3.0),
    ]
}

/// Estimated bounds relative to the label's left baseline; includes the lower
/// tolerance row and basic-frame padding when present.
pub(super) fn bounds(label: &DrawingLabel, framed: bool) -> ([f64; 2], [f64; 2]) {
    let mut local = label.clone();
    local.position_mm = [0.0, 0.0];
    let mut min = [0.0_f64, 0.0_f64];
    let mut max = [0.0_f64, 0.0_f64];
    for (point, text, size) in parts(&local) {
        if text.is_empty() {
            continue;
        }
        min[0] = min[0].min(point[0]);
        min[1] = min[1].min(point[1]);
        max[0] = max[0].max(point[0] + width(&text, size));
        max[1] = max[1].max(point[1] + size);
    }
    if framed {
        for axis in 0..2 {
            min[axis] -= 1.0;
            max[axis] += 1.0;
        }
    }
    (min, max)
}

/// Place an upright label outside the line/arc tangent through `anchor`, on
/// its unit `normal` side. Uses the rectangle's support distance, so vertical
/// and oblique dimensions account for label width as well as tolerance height.
pub(super) fn place(
    label: &mut DrawingLabel,
    anchor: [f64; 2],
    normal: [f64; 2],
    framed: bool,
) -> Result<(), ModelError> {
    let (min, max) = bounds(label, framed);
    let centre = [0.5 * (min[0] + max[0]), 0.5 * (min[1] + max[1])];
    let support = 0.5 * (normal[0].abs() * (max[0] - min[0]) + normal[1].abs() * (max[1] - min[1]));
    label.position_mm = [
        anchor[0] + normal[0] * (support + 2.0) - centre[0],
        anchor[1] + normal[1] * (support + 2.0) - centre[1],
    ];
    if !finite_pair(label.position_mm) {
        return Err(ModelError::new(
            "dimension label exceeds finite paper coordinates",
        ));
    }
    Ok(())
}
