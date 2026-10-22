//! Dimension annotations drawn over a finished render: overall width and
//! height with extension lines and arrows in the orthographic views, Ø
//! labels on circles seen end-on, and the overall size in the iso view.
//! Values come from exact bounds and analytic edges, never the mesh. Text is
//! a built-in 5 × 7 bitmap font. O(circles + label pixels) per image.
use serde_json::{Value, json};

/// Most distinct diameters labelled in one view; the rest are counted.
pub const MAX_DIAMETER_LABELS: usize = 8;
const INK: [u8; 3] = [20, 60, 140];
const PAPER: [u8; 3] = [255, 255, 255];
const AXES: [&str; 3] = ["x", "y", "z"];

/// A circular edge in model coordinates.
#[derive(Clone, Copy, Debug)]
pub struct Circle {
    pub center: [f64; 3],
    pub axis: [f64; 3],
    pub radius: f64,
}

/// What to dimension: the exact model bounds and the circular edges.
pub struct Dimensions {
    pub lo: [f64; 3],
    pub hi: [f64; 3],
    pub circles: Vec<Circle>,
}

/// Maps model points to output pixels for one view.
pub struct Camera<'a> {
    pub right: [f64; 3],
    pub up: [f64; 3],
    pub view: [f64; 3],
    pub pixel: &'a dyn Fn([f64; 3]) -> [f64; 2],
    /// Whether a model point lies on visible surface, not behind it.
    pub visible: &'a dyn Fn([f64; 3]) -> bool,
}

/// Shortest decimal for a millimeter value: at most two decimals, trailing
/// zeros dropped.
pub fn format_mm(value: f64) -> String {
    let text = format!("{value:.2}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" {
        "0".into()
    } else {
        text.into()
    }
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Draws the annotations into `rgb` (`size` × `size`) and returns what was
/// labelled, for the report.
pub fn annotate(rgb: &mut [u8], size: usize, camera: &Camera<'_>, dims: &Dimensions) -> Vec<Value> {
    let scale = (size / 256).max(1);
    let mut canvas = Canvas {
        rgb,
        size,
        taken: Vec::new(),
    };
    let mut labels = Vec::new();
    let axis_of = |v: [f64; 3]| (0..3).find(|&k| v[k].abs() > 0.999);
    let (Some(across), Some(upward)) = (axis_of(camera.right), axis_of(camera.up)) else {
        // Oblique (iso) view: the overall size as one line of text.
        let size_text: Vec<String> = (0..3).map(|k| format_mm(dims.hi[k] - dims.lo[k])).collect();
        let text = format!("{} x {} x {} mm", size_text[0], size_text[1], size_text[2]);
        let width = text_width(&text, scale);
        canvas.text(
            &text,
            [size as f64 - width - 6.0 * scale as f64, 6.0 * scale as f64],
            scale,
        );
        for (k, axis) in ["x", "y", "z"].iter().enumerate() {
            labels.push(json!({"kind": "size", "axis": axis, "value_mm": dims.hi[k] - dims.lo[k]}));
        }
        return labels;
    };
    // The bounds' silhouette in this view, in pixels: exact for the
    // axis-aligned views.
    let (mut min, mut max) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for k in 0..8 {
        let p = [0, 1, 2].map(|i| {
            if k >> i & 1 == 0 {
                dims.lo[i]
            } else {
                dims.hi[i]
            }
        });
        let q = (camera.pixel)(p);
        for i in 0..2 {
            min[i] = min[i].min(q[i]);
            max[i] = max[i].max(q[i]);
        }
    }
    let (bottom_left, bottom_right, top_right) =
        ([min[0], max[1]], [max[0], max[1]], [max[0], min[1]]);
    let gap = 14.0 * scale as f64;
    // Width below the part.
    let width = dims.hi[across] - dims.lo[across];
    let y = bottom_left[1] + gap;
    canvas.line(bottom_left, [bottom_left[0], y + 4.0 * scale as f64], scale);
    canvas.line(
        bottom_right,
        [bottom_right[0], y + 4.0 * scale as f64],
        scale,
    );
    canvas.dimension([bottom_left[0], y], [bottom_right[0], y], scale);
    let text = format_mm(width);
    canvas.label(
        &text,
        [
            (bottom_left[0] + bottom_right[0]) / 2.0,
            y + 3.0 * scale as f64,
        ],
        scale,
        false,
    );
    labels.push(json!({"kind": "width", "axis": AXES[across], "value_mm": width}));
    // Height to the right.
    let height = dims.hi[upward] - dims.lo[upward];
    let x = bottom_right[0] + gap;
    canvas.line(
        bottom_right,
        [x + 4.0 * scale as f64, bottom_right[1]],
        scale,
    );
    canvas.line(top_right, [x + 4.0 * scale as f64, top_right[1]], scale);
    canvas.dimension([x, bottom_right[1]], [x, top_right[1]], scale);
    let text = format_mm(height);
    canvas.label(
        &text,
        [
            x + 4.0 * scale as f64,
            (bottom_right[1] + top_right[1]) / 2.0,
        ],
        scale,
        true,
    );
    labels.push(json!({"kind": "height", "axis": AXES[upward], "value_mm": height}));
    // Visible circles seen end-on, grouped by diameter. Each group is
    // labelled once, at its circle nearest the top left, with a leader from
    // the circle's rim to a free spot so concentric labels stay apart.
    struct Group {
        diameter: f64,
        count: usize,
        center: [f64; 2],
        radius_px: f64,
    }
    let mut groups: Vec<Group> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for circle in &dims.circles {
        if dot(circle.axis, camera.view).abs() < 0.999
            || circle.radius.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater)
        {
            continue;
        }
        // Both ends of a through hole project onto each other: count once.
        let key = (
            [
                (dot(circle.center, camera.right) * 1e4).round() as i64,
                (dot(circle.center, camera.up) * 1e4).round() as i64,
            ],
            (circle.radius * 1e4).round() as i64,
        );
        if seen.contains(&key) {
            continue;
        }
        let rim = |angle: f64| {
            let (c, s) = (angle.cos() * circle.radius, angle.sin() * circle.radius);
            [0, 1, 2].map(|k| circle.center[k] + c * camera.right[k] + s * camera.up[k])
        };
        let visible = (0..8)
            .filter(|&k| (camera.visible)(rim(k as f64 * std::f64::consts::FRAC_PI_4)))
            .count();
        if visible < 3 {
            continue;
        }
        seen.insert(key);
        let center = (camera.pixel)(circle.center);
        let edge = (camera.pixel)(rim(0.0));
        let radius_px = (edge[0] - center[0]).hypot(edge[1] - center[1]);
        let diameter = 2.0 * circle.radius;
        match groups
            .iter_mut()
            .find(|g| (g.diameter - diameter).abs() <= 1e-6 * diameter.max(1.0))
        {
            Some(group) => {
                group.count += 1;
                if center[1] + center[0] < group.center[1] + group.center[0] {
                    (group.center, group.radius_px) = (center, radius_px);
                }
            }
            None => groups.push(Group {
                diameter,
                count: 1,
                center,
                radius_px,
            }),
        }
    }
    groups.sort_by(|a, b| b.diameter.total_cmp(&a.diameter));
    let shown = groups.len().min(MAX_DIAMETER_LABELS);
    let unit = scale as f64;
    let half = std::f64::consts::FRAC_1_SQRT_2;
    for group in &groups[..shown] {
        let text = if group.count > 1 {
            format!("@{} x{}", format_mm(group.diameter), group.count)
        } else {
            format!("@{}", format_mm(group.diameter))
        };
        let (width, height) = (text_width(&text, scale), 7.0 * unit);
        // Up-right first, then the other diagonals, further out each round.
        let mut placed = None;
        'search: for reach in [16.0, 30.0, 46.0, 64.0, 84.0] {
            for direction in [[half, -half], [-half, -half], [half, half], [-half, half]] {
                let rim = [
                    group.center[0] + direction[0] * group.radius_px,
                    group.center[1] + direction[1] * group.radius_px,
                ];
                let end = [
                    rim[0] + direction[0] * reach * unit,
                    rim[1] + direction[1] * reach * unit,
                ];
                let x = if direction[0] > 0.0 {
                    end[0] + 2.0 * unit
                } else {
                    end[0] - 2.0 * unit - width
                };
                let top = end[1] - height / 2.0;
                let rect = [x - unit, top - unit, x + width + unit, top + height + unit];
                if canvas.free(rect) {
                    placed = Some((rim, end, [x, top]));
                    break 'search;
                }
            }
        }
        let Some((rim, end, at)) = placed else {
            labels.push(json!({"kind": "diameter", "value_mm": group.diameter, "count": group.count, "drawn": false}));
            continue;
        };
        canvas.line(rim, end, scale);
        canvas.dot(rim, scale);
        canvas.text(&text, at, scale);
        labels.push(json!({"kind": "diameter", "value_mm": group.diameter, "count": group.count}));
    }
    if groups.len() > shown {
        labels.push(json!({"kind": "diameter_groups_omitted", "count": groups.len() - shown}));
    }
    labels
}

struct Canvas<'a> {
    rgb: &'a mut [u8],
    size: usize,
    /// Boxes of text drawn so far, as [x0, y0, x1, y1].
    taken: Vec<[f64; 4]>,
}

impl Canvas<'_> {
    /// Whether `rect` is inside the image and clear of earlier text.
    fn free(&self, rect: [f64; 4]) -> bool {
        rect[0] >= 0.0
            && rect[1] >= 0.0
            && rect[2] < self.size as f64
            && rect[3] < self.size as f64
            && self
                .taken
                .iter()
                .all(|t| rect[2] < t[0] || t[2] < rect[0] || rect[3] < t[1] || t[3] < rect[1])
    }
    fn put(&mut self, x: f64, y: f64, color: [u8; 3]) {
        if x < 0.0 || y < 0.0 || x >= self.size as f64 || y >= self.size as f64 {
            return;
        }
        let at = (y as usize * self.size + x as usize) * 3;
        self.rgb[at..at + 3].copy_from_slice(&color);
    }
    /// A line `scale` pixels wide, stepped at half a pixel.
    fn line(&mut self, a: [f64; 2], b: [f64; 2], scale: usize) {
        let length = (b[0] - a[0]).hypot(b[1] - a[1]);
        let steps = (length * 2.0).ceil().clamp(1.0, 1e5) as usize;
        for k in 0..=steps {
            let t = k as f64 / steps as f64;
            let (x, y) = (a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t);
            for dx in 0..scale {
                for dy in 0..scale {
                    self.put(
                        x + dx as f64 - (scale / 2) as f64,
                        y + dy as f64 - (scale / 2) as f64,
                        INK,
                    );
                }
            }
        }
    }
    fn dot(&mut self, at: [f64; 2], scale: usize) {
        let r = 2.0 * scale as f64;
        for dy in -(r as i64)..=r as i64 {
            for dx in -(r as i64)..=r as i64 {
                if ((dx * dx + dy * dy) as f64) <= r * r {
                    self.put(at[0] + dx as f64, at[1] + dy as f64, INK);
                }
            }
        }
    }
    /// A dimension line with filled arrowheads pointing at both ends.
    fn dimension(&mut self, a: [f64; 2], b: [f64; 2], scale: usize) {
        self.line(a, b, scale);
        let length = (b[0] - a[0]).hypot(b[1] - a[1]);
        if length < 1.0 {
            return;
        }
        let d = [(b[0] - a[0]) / length, (b[1] - a[1]) / length];
        let head = (5.0 * scale as f64).min(length / 3.0);
        for (tip, sign) in [(a, 1.0), (b, -1.0)] {
            let mut s = 0.0;
            while s <= head {
                let w = s * 0.4;
                let base = [tip[0] + sign * d[0] * s, tip[1] + sign * d[1] * s];
                self.line(
                    [base[0] - d[1] * w, base[1] + d[0] * w],
                    [base[0] + d[1] * w, base[1] - d[0] * w],
                    1,
                );
                s += 0.5;
            }
        }
    }
    /// Text on a white box, centred on `at` horizontally (or starting there
    /// when `left`) with its top at `at`.
    fn label(&mut self, text: &str, at: [f64; 2], scale: usize, left: bool) {
        let width = text_width(text, scale);
        let x = if left { at[0] } else { at[0] - width / 2.0 };
        self.text(text, [x, at[1]], scale);
    }
    fn text(&mut self, text: &str, at: [f64; 2], scale: usize) {
        let (width, height) = (text_width(text, scale), (7 * scale) as f64);
        let pad = scale as f64;
        self.taken.push([
            at[0] - pad,
            at[1] - pad,
            at[0] + width + pad,
            at[1] + height + pad,
        ]);
        for y in 0..(height + 2.0 * pad) as usize {
            for x in 0..(width + 2.0 * pad) as usize {
                self.put(at[0] - pad + x as f64, at[1] - pad + y as f64, PAPER);
            }
        }
        for (i, c) in text.chars().enumerate() {
            let rows = glyph(c);
            for (row, bits) in rows.iter().enumerate() {
                for col in 0..5 {
                    if bits & (0x10 >> col) != 0 {
                        for dy in 0..scale {
                            for dx in 0..scale {
                                self.put(
                                    at[0] + ((i * 6 + col) * scale + dx) as f64,
                                    at[1] + (row * scale + dy) as f64,
                                    INK,
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

fn text_width(text: &str, scale: usize) -> f64 {
    (text.chars().count() * 6 * scale).saturating_sub(scale) as f64
}

/// 5 × 7 rows, high bit on the left. `@` draws Ø.
fn glyph(c: char) -> [u8; 7] {
    match c {
        '0' => [0x0E, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0E],
        '1' => [0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E],
        '2' => [0x0E, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1F],
        '3' => [0x1F, 0x02, 0x04, 0x02, 0x01, 0x11, 0x0E],
        '4' => [0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02],
        '5' => [0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E],
        '6' => [0x06, 0x08, 0x10, 0x1E, 0x11, 0x11, 0x0E],
        '7' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
        '8' => [0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E],
        '9' => [0x0E, 0x11, 0x11, 0x0F, 0x01, 0x02, 0x0C],
        '.' => [0, 0, 0, 0, 0, 0x0C, 0x0C],
        '-' => [0, 0, 0, 0x1F, 0, 0, 0],
        'x' => [0, 0, 0x11, 0x0A, 0x04, 0x0A, 0x11],
        'm' => [0, 0, 0x1A, 0x15, 0x15, 0x11, 0x11],
        '@' => [0x01, 0x0E, 0x13, 0x15, 0x19, 0x0E, 0x10],
        _ => [0; 7],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_print_with_at_most_two_decimals() {
        assert_eq!(format_mm(60.0), "60");
        assert_eq!(format_mm(12.5), "12.5");
        assert_eq!(format_mm(0.254), "0.25");
        assert_eq!(format_mm(-0.001), "0");
        assert_eq!(format_mm(1e-9), "0");
    }
}
