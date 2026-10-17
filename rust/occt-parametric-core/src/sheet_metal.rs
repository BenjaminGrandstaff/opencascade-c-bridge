//! Constant-width flange strips with circular bends and explicit neutral
//! factors, cutouts through their flanges (schema 160), bend tables
//! (schema 161), hems at either end (schema 162), and edge flanges with
//! bend reliefs on the width edges (schema 176).
use super::*;
use crate::assembly::{cross, dot, scale};
use occt_types::WireSegment;

mod bend_table;
pub use bend_table::*;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SheetMetalBend {
    /// Signed change of tangent direction, in dimensionless radians.
    pub angle_radians: ScalarExpr,
    pub inside_radius: ScalarExpr,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SheetMetalDefinition {
    /// Start of the thickness centerline; width extends along width_axis.
    pub origin: VectorExpr,
    pub width_axis: VectorExpr,
    pub start_direction: VectorExpr,
    pub width: ScalarExpr,
    pub thickness: ScalarExpr,
    /// Straight tangent-to-tangent lengths. One more flange than bend.
    pub flanges: Vec<ScalarExpr>,
    pub bends: Vec<SheetMetalBend>,
    /// Holes, rectangles and slots through flanges (schema 160).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cutouts: Vec<SheetMetalCutout>,
    /// A hem before the first flange, at `origin` (schema 162).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_hem: Option<SheetMetalHem>,
    /// A hem after the last flange (schema 162).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_hem: Option<SheetMetalHem>,
    /// Flanges folded up from the width edges of the strip's flanges
    /// (schema 176).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edge_flanges: Vec<SheetMetalEdgeFlange>,
}

/// A width edge of the strip: `origin` at the start of the width, `far` at
/// its end.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SheetEdge {
    Origin,
    Far,
}

/// A flange folded up from a width edge of one of the strip's flanges
/// (schema 176): over `length` of the base flange's straight region from
/// `start` (measured like a cutout's `along`), a bend of `angle_radians`
/// (positive toward the side the strip's positive bends turn to) at
/// `inside_radius`, then a straight leg of `height`. Where it reaches the
/// end of its base flange at a bend, `relief_depth` cuts a rectangular bend
/// relief that deep into the strip from that edge, through the whole bend
/// zone, so the bend can be formed without tearing.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SheetMetalEdgeFlange {
    pub id: String,
    /// The base flange, counted from 0 at the sheet's origin.
    pub flange: usize,
    pub edge: SheetEdge,
    pub start: ScalarExpr,
    pub length: ScalarExpr,
    pub angle_radians: ScalarExpr,
    pub inside_radius: ScalarExpr,
    pub height: ScalarExpr,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relief_depth: Option<ScalarExpr>,
}

/// An edge flange with its dimensions evaluated, in millimeters, on the
/// sheet's internal flange numbering (a start hem counts as flange 0).
#[derive(Clone, Debug, PartialEq)]
pub struct EvaluatedEdgeFlange {
    pub id: String,
    pub flange: usize,
    pub edge: SheetEdge,
    pub start: f64,
    pub length: f64,
    pub angle: f64,
    pub radius: f64,
    pub height: f64,
}

/// A bend relief: a rectangular notch `depth` deep from `edge` through the
/// whole zone of bend `bend` (internal numbering).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EvaluatedRelief {
    pub bend: usize,
    pub edge: SheetEdge,
    pub depth: f64,
}

/// An edge flange unfolded on the blank: the tab's extent along the blank
/// and across it (from the strip's edge out to the tip), and its bend line.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FlatEdgeTab {
    pub id: String,
    pub along_mm: [f64; 2],
    /// Low and high `across` coordinates of the tab.
    pub across_mm: [f64; 2],
    /// `across` of the center of the tab's bend zone.
    pub bend_line_mm: f64,
    pub allowance_mm: f64,
}

/// A bend relief notch on the blank.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FlatRelief {
    pub along_mm: [f64; 2],
    pub across_mm: [f64; 2],
}

/// Which side of its flange a hem folds onto: `positive` is the side a
/// positive bend turns toward.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HemSide {
    Positive,
    Negative,
}

/// A sheet end folded back on itself (schema 162).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum SheetMetalHem {
    /// Folded through 180° to run back along its flange, `gap` from it (at
    /// least a tenth of the thickness), for `length`.
    Open {
        side: HemSide,
        gap: ScalarExpr,
        length: ScalarExpr,
    },
    /// Rolled through more than 180° at `inside_radius`, ending in the
    /// roll. The roll may not reach back into its own flange.
    Rolled {
        side: HemSide,
        inside_radius: ScalarExpr,
        angle_radians: ScalarExpr,
    },
}

impl SheetMetalHem {
    fn expressions(&self) -> [&ScalarExpr; 2] {
        match self {
            Self::Open { gap, length, .. } => [gap, length],
            Self::Rolled {
                inside_radius,
                angle_radians,
                ..
            } => [inside_radius, angle_radians],
        }
    }

    /// The hem's straight length (zero for a roll), signed turn and inside
    /// radius.
    fn evaluate(
        &self,
        parameters: &HashMap<String, ParameterValue>,
        thickness: f64,
    ) -> Result<(f64, f64, f64), ModelError> {
        let length = |e: &ScalarExpr| scalar(e, parameters, Dimension::Length);
        let sign = |side: &HemSide| match side {
            HemSide::Positive => 1.0,
            HemSide::Negative => -1.0,
        };
        match self {
            Self::Open {
                side,
                gap,
                length: l,
            } => {
                let gap = positive(length(gap)?)?;
                if gap < 0.1 * thickness {
                    return Err(ModelError::new(
                        "an open hem's gap must be at least a tenth of the sheet thickness",
                    ));
                }
                Ok((
                    positive(length(l)?)?,
                    sign(side) * std::f64::consts::PI,
                    gap * 0.5,
                ))
            }
            Self::Rolled {
                side,
                inside_radius,
                angle_radians,
            } => {
                let radius = positive(length(inside_radius)?)?;
                let angle = scalar(angle_radians, parameters, Dimension::Scalar)?;
                // Past this the roll's outer surface re-enters its flange.
                let limit = std::f64::consts::TAU - (radius / (radius + thickness)).acos();
                if !(angle.is_finite()
                    && angle > std::f64::consts::PI + 1e-6
                    && angle < limit - 1e-6)
                {
                    return Err(ModelError::new(format!(
                        "a rolled hem's angle must lie between pi and {limit} radians, \
                         where the roll would reach back into its flange"
                    )));
                }
                Ok((0.0, sign(side) * angle, radius))
            }
        }
    }
}

/// A cutout through one flange, cut square to it in the folded sheet and in
/// its flat blank.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SheetMetalCutout {
    pub id: String,
    /// The flange, counted from 0 at the sheet's origin.
    pub flange: usize,
    /// Center distance along the flange's midplane from its start tangent
    /// point.
    pub along: ScalarExpr,
    /// Center distance across the width from the origin edge.
    pub across: ScalarExpr,
    pub shape: SheetMetalCutoutShape,
}

/// A cutout's outline; lengths run along the flange and widths across it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum SheetMetalCutoutShape {
    Circle {
        diameter: ScalarExpr,
    },
    Rectangle {
        length: ScalarExpr,
        width: ScalarExpr,
    },
    /// An obround: `length` overall, with semicircular ends of `width`.
    Slot {
        length: ScalarExpr,
        width: ScalarExpr,
    },
}

/// A cutout with its dimensions evaluated, in millimeters.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum CutoutOutline {
    Circle { diameter: f64 },
    Rectangle { length: f64, width: f64 },
    Slot { length: f64, width: f64 },
}

impl CutoutOutline {
    /// Half extents along and across the flange.
    pub fn half_extents(self) -> (f64, f64) {
        match self {
            Self::Circle { diameter } => (diameter * 0.5, diameter * 0.5),
            Self::Rectangle { length, width } | Self::Slot { length, width } => {
                (length * 0.5, width * 0.5)
            }
        }
    }

    /// Closed outline segments around `(along, across)` in flange
    /// coordinates: lines as point pairs and arcs as start, middle and end.
    pub fn segments(self, along: f64, across: f64) -> Vec<OutlineSegment> {
        let (a, b) = self.half_extents();
        let p = |u: f64, v: f64| [along + u, across + v];
        match self {
            Self::Circle { .. } => vec![
                OutlineSegment::Arc(p(a, 0.0), p(0.0, a), p(-a, 0.0)),
                OutlineSegment::Arc(p(-a, 0.0), p(0.0, -a), p(a, 0.0)),
            ],
            Self::Rectangle { .. } => vec![
                OutlineSegment::Line(p(-a, -b), p(a, -b)),
                OutlineSegment::Line(p(a, -b), p(a, b)),
                OutlineSegment::Line(p(a, b), p(-a, b)),
                OutlineSegment::Line(p(-a, b), p(-a, -b)),
            ],
            Self::Slot { .. } => {
                // Straight sides between semicircles of radius b.
                let straight = a - b;
                if straight <= 0.0 {
                    return Self::Circle { diameter: 2.0 * b }.segments(along, across);
                }
                vec![
                    OutlineSegment::Line(p(-straight, -b), p(straight, -b)),
                    OutlineSegment::Arc(p(straight, -b), p(a, 0.0), p(straight, b)),
                    OutlineSegment::Line(p(straight, b), p(-straight, b)),
                    OutlineSegment::Arc(p(-straight, b), p(-a, 0.0), p(-straight, -b)),
                ]
            }
        }
    }
}

/// One piece of a cutout outline in flange or blank coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OutlineSegment {
    Line([f64; 2], [f64; 2]),
    Arc([f64; 2], [f64; 2], [f64; 2]),
}

/// An evaluated cutout on its flange.
#[derive(Clone, Debug, PartialEq)]
pub struct EvaluatedCutout {
    pub id: String,
    pub flange: usize,
    pub along: f64,
    pub across: f64,
    pub outline: CutoutOutline,
}

/// A cutout's place on the flat blank: its center measured from the start of
/// the blank and from its origin edge.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FlatCutout {
    pub id: String,
    pub center_mm: [f64; 2],
    pub outline: CutoutOutline,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FlatPatternMetrics {
    pub length_mm: f64,
    pub width_mm: f64,
    pub thickness_mm: f64,
    pub neutral_factor: f64,
    pub bend_allowances_mm: Vec<f64>,
    /// Centers of bend zones, measured from the start of the blank.
    pub bend_lines_mm: Vec<f64>,
    /// Cutouts placed on the blank (schema 160).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cutouts: Vec<FlatCutout>,
    /// The bend table that gave the allowances instead of the neutral factor
    /// (schema 161).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bend_table: Option<String>,
    /// Edge flanges unfolded as tabs off the blank's long edges (schema 176).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edge_tabs: Vec<FlatEdgeTab>,
    /// Bend relief notches (schema 176).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reliefs: Vec<FlatRelief>,
}
fn positive(value: f64) -> Result<f64, ModelError> {
    if value.is_finite() && value > 0.0 {
        Ok(value)
    } else {
        Err(ModelError::new(
            "sheet dimensions must be finite positive lengths",
        ))
    }
}
fn direction(value: Vec3) -> Result<Vec3, ModelError> {
    let maximum = value.x.abs().max(value.y.abs()).max(value.z.abs());
    if !maximum.is_finite() || maximum == 0.0 {
        return Err(ModelError::new("sheet axes must be finite and nonzero"));
    }
    let value = Vec3::new(value.x / maximum, value.y / maximum, value.z / maximum);
    Ok(scale(value, 1.0 / dot(value, value).sqrt()))
}
pub fn start(segment: WireSegment) -> Vec3 {
    match segment {
        WireSegment::Line { start, .. } | WireSegment::Arc { start, .. } => start,
    }
}
pub fn end(segment: WireSegment) -> Vec3 {
    match segment {
        WireSegment::Line { end, .. } | WireSegment::Arc { end, .. } => end,
    }
}
pub fn reverse(segment: WireSegment) -> WireSegment {
    match segment {
        WireSegment::Line { start, end } => WireSegment::Line {
            start: end,
            end: start,
        },
        WireSegment::Arc { start, middle, end } => WireSegment::Arc {
            start: end,
            middle,
            end: start,
        },
    }
}
impl FlatPatternMetrics {
    /// The blank's outline as a closed polygon (first point not repeated):
    /// the strip's rectangle with tabs out from, and relief notches into,
    /// its long edges. Tabs and notches on one edge do not overlap.
    /// O(tabs + reliefs) after sorting.
    pub fn outline(&self) -> Vec<[f64; 2]> {
        let (length, width) = (self.length_mm, self.width_mm);
        // Each feature on an edge: its along range and the edge's new y.
        let mut low: Vec<([f64; 2], f64)> = Vec::new();
        let mut high: Vec<([f64; 2], f64)> = Vec::new();
        for tab in &self.edge_tabs {
            if tab.across_mm[1] <= 0.0 {
                low.push((tab.along_mm, tab.across_mm[0]));
            } else {
                high.push((tab.along_mm, tab.across_mm[1]));
            }
        }
        for relief in &self.reliefs {
            if relief.across_mm[0] <= 0.0 {
                low.push((relief.along_mm, relief.across_mm[1]));
            } else {
                high.push((relief.along_mm, relief.across_mm[0]));
            }
        }
        low.sort_by(|a, b| a.0[0].total_cmp(&b.0[0]));
        high.sort_by(|a, b| b.0[0].total_cmp(&a.0[0]));
        // Features meeting end to end (a tab beside a relief) agree only to
        // rounding, so points this close are one point.
        let tolerance = 1e-9 * (1.0 + length + width);
        let mut points: Vec<[f64; 2]> = vec![[0.0, 0.0]];
        let push = |p: [f64; 2], points: &mut Vec<[f64; 2]>| {
            if points
                .last()
                .is_none_or(|q| (p[0] - q[0]).abs() + (p[1] - q[1]).abs() > tolerance)
            {
                points.push(p);
            }
        };
        for ([x0, x1], y) in low {
            for p in [[x0, 0.0], [x0, y], [x1, y], [x1, 0.0]] {
                push(p, &mut points);
            }
        }
        push([length, 0.0], &mut points);
        push([length, width], &mut points);
        for ([x0, x1], y) in high {
            for p in [[x1, width], [x1, y], [x0, y], [x0, width]] {
                push(p, &mut points);
            }
        }
        push([0.0, width], &mut points);
        if points.len() > 1
            && points[points.len() - 1][0].abs() + points[points.len() - 1][1].abs() <= tolerance
        {
            points.pop();
        }
        points
    }

    /// Blank outline and dashed bend centerlines in a millimeter drawing.
    /// Bend lines mark the center of the allowance zone, not bend extents.
    pub fn drawing(&self, id: &str) -> Result<GeneratedDrawing, ModelError> {
        positive(self.length_mm)?;
        positive(self.width_mm)?;
        positive(self.thickness_mm)?;
        if id.is_empty()
            || id.len() > 256
            || id.chars().any(char::is_control)
            || !self.neutral_factor.is_finite()
            || !(0.0..=1.0).contains(&self.neutral_factor)
            || self.bend_allowances_mm.len() != self.bend_lines_mm.len()
            || self
                .bend_allowances_mm
                .iter()
                .any(|value| !value.is_finite() || *value <= 0.0)
            || self
                .bend_lines_mm
                .iter()
                .any(|value| !value.is_finite() || *value <= 0.0 || *value >= self.length_mm)
            || self.bend_lines_mm.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err(ModelError::new(
                "invalid flat-pattern metrics or drawing id",
            ));
        }
        let outline = self.outline();
        let (below, above) = outline.iter().fold((0.0f64, 0.0f64), |(lo, hi), p| {
            (lo.min(p[1]), hi.max(p[1] - self.width_mm))
        });
        let (x, y, length, width) = (10.0, 45.0 - below, self.length_mm, self.width_mm);
        let paper_size_mm = [(length + 20.0).max(100.0), width + 65.0 - below + above];
        if paper_size_mm.iter().any(|value| !value.is_finite()) {
            return Err(ModelError::new("flat-pattern paper dimensions overflow"));
        }
        let mut closed: Vec<[f64; 2]> = outline.iter().map(|p| [x + p[0], y + p[1]]).collect();
        closed.push(closed[0]);
        let mut polylines = vec![DrawingPolyline {
            points_mm: closed,
            hidden: false,
            thin: false,
        }];
        polylines.extend(self.bend_lines_mm.iter().map(|line| DrawingPolyline {
            points_mm: vec![[x + line, y], [x + line, y + width]],
            hidden: true,
            thin: true,
        }));
        polylines.extend(self.edge_tabs.iter().map(|tab| DrawingPolyline {
            points_mm: vec![
                [x + tab.along_mm[0], y + tab.bend_line_mm],
                [x + tab.along_mm[1], y + tab.bend_line_mm],
            ],
            hidden: true,
            thin: true,
        }));
        // Cutouts as closed polylines, arcs at 32 chords per half circle.
        for cutout in &self.cutouts {
            let [u, v] = cutout.center_mm;
            let mut points = Vec::new();
            for segment in cutout.outline.segments(u, v) {
                match segment {
                    OutlineSegment::Line(a, b) => {
                        if points.is_empty() {
                            points.push([x + a[0], y + a[1]]);
                        }
                        points.push([x + b[0], y + b[1]]);
                    }
                    OutlineSegment::Arc(a, m, b) => {
                        let arc = arc_points(a, m, b, 32);
                        let skip = usize::from(!points.is_empty());
                        points.extend(arc.into_iter().skip(skip).map(|p| [x + p[0], y + p[1]]));
                    }
                }
            }
            polylines.push(DrawingPolyline {
                points_mm: points,
                hidden: false,
                thin: false,
            });
        }
        Ok(GeneratedDrawing {
            curves: Vec::new(),
            gdt_lines: Vec::new(),
            gdt_labels: Vec::new(),
            sheet_lines: Vec::new(),
            sheet_labels: Vec::new(),
            id: id.into(),
            title: format!("{id} flat pattern"),
            paper_size_mm,
            polylines,
            guides: Vec::new(),
            hatches: Vec::new(),
            labels: vec![],
            metadata: BTreeMap::from([
                ("Thickness mm".into(), self.thickness_mm.to_string()),
                match &self.bend_table {
                    Some(table) => ("Bend table".into(), table.clone()),
                    None => ("Neutral factor K".into(), self.neutral_factor.to_string()),
                },
            ]),
            generated_variants: 0,
            lettering: DrawingLettering::default(),
            line_weights: None,
            zones: None,
        })
    }
}

/// Points along the circular arc through `a`, `m` and `b`, `chords` chords
/// long, from `a` to `b`.
pub fn arc_points(a: [f64; 2], m: [f64; 2], b: [f64; 2], chords: usize) -> Vec<[f64; 2]> {
    // Circumcenter of the three points.
    let d = 2.0 * (a[0] * (m[1] - b[1]) + m[0] * (b[1] - a[1]) + b[0] * (a[1] - m[1]));
    let sq = |p: [f64; 2]| p[0] * p[0] + p[1] * p[1];
    let cx = (sq(a) * (m[1] - b[1]) + sq(m) * (b[1] - a[1]) + sq(b) * (a[1] - m[1])) / d;
    let cy = (sq(a) * (b[0] - m[0]) + sq(m) * (a[0] - b[0]) + sq(b) * (m[0] - a[0])) / d;
    let r = (a[0] - cx).hypot(a[1] - cy);
    let angle = |p: [f64; 2]| (p[1] - cy).atan2(p[0] - cx);
    let (start, middle, end) = (angle(a), angle(m), angle(b));
    let tau = std::f64::consts::TAU;
    // Sweep from start through middle to end, in whichever sense holds it.
    let ccw = |from: f64, to: f64| (to - from).rem_euclid(tau);
    let sweep = if ccw(start, middle) < ccw(start, end) {
        ccw(start, end)
    } else {
        -ccw(end, start)
    };
    // The ends are the given points exactly, so outlines close exactly.
    (0..=chords)
        .map(|i| match i {
            0 => a,
            i if i == chords => b,
            i => {
                let t = start + sweep * i as f64 / chords as f64;
                [cx + r * t.cos(), cy + r * t.sin()]
            }
        })
        .collect()
}

impl SheetMetalDefinition {
    pub fn collect_parameters<'a>(&'a self, names: &mut HashSet<&'a str>) {
        for vector in [&self.origin, &self.width_axis, &self.start_direction] {
            crate::regeneration::collect_vector_parameters(vector, names);
        }
        for expression in [&self.width, &self.thickness]
            .into_iter()
            .chain(&self.flanges)
        {
            crate::regeneration::collect_scalar_parameters(expression, names);
        }
        for bend in &self.bends {
            crate::regeneration::collect_scalar_parameters(&bend.angle_radians, names);
            crate::regeneration::collect_scalar_parameters(&bend.inside_radius, names);
        }
        for cutout in &self.cutouts {
            let shape = match &cutout.shape {
                SheetMetalCutoutShape::Circle { diameter } => vec![diameter],
                SheetMetalCutoutShape::Rectangle { length, width }
                | SheetMetalCutoutShape::Slot { length, width } => vec![length, width],
            };
            for expression in [&cutout.along, &cutout.across].into_iter().chain(shape) {
                crate::regeneration::collect_scalar_parameters(expression, names);
            }
        }
        for hem in self.start_hem.iter().chain(&self.end_hem) {
            for expression in hem.expressions() {
                crate::regeneration::collect_scalar_parameters(expression, names);
            }
        }
        for flange in &self.edge_flanges {
            for expression in [
                &flange.start,
                &flange.length,
                &flange.angle_radians,
                &flange.inside_radius,
                &flange.height,
            ]
            .into_iter()
            .chain(&flange.relief_depth)
            {
                crate::regeneration::collect_scalar_parameters(expression, names);
            }
        }
    }

    pub fn evaluate(
        &self,
        parameters: &HashMap<String, ParameterValue>,
    ) -> Result<EvaluatedSheet, ModelError> {
        if self.flanges.is_empty()
            || self.flanges.len() > 1024
            || self.bends.len() != self.flanges.len() - 1
        {
            return Err(ModelError::new(
                "sheet requires 1–1024 flanges and exactly one bend between adjacent flanges",
            ));
        }
        let origin = vector(&self.origin, parameters, Dimension::Length)?;
        let width_axis = direction(vector(&self.width_axis, parameters, Dimension::Scalar)?)?;
        let tangent = direction(vector(
            &self.start_direction,
            parameters,
            Dimension::Scalar,
        )?)?;
        if dot(width_axis, tangent).abs() > 1e-9 {
            return Err(ModelError::new(
                "sheet width axis and start direction must be perpendicular",
            ));
        }
        let normal = direction(cross(width_axis, tangent))?;
        // Remove tolerated roundoff to make the frame exactly orthogonal.
        let tangent = direction(cross(normal, width_axis))?;
        let width = positive(scalar(&self.width, parameters, Dimension::Length)?)?;
        let thickness = positive(scalar(&self.thickness, parameters, Dimension::Length)?)?;
        let flanges = self
            .flanges
            .iter()
            .map(|value| positive(scalar(value, parameters, Dimension::Length)?))
            .collect::<Result<Vec<_>, ModelError>>()?;
        let bends = self
            .bends
            .iter()
            .map(|bend| {
                let angle = scalar(&bend.angle_radians, parameters, Dimension::Scalar)?;
                let radius = positive(scalar(&bend.inside_radius, parameters, Dimension::Length)?)?;
                if !angle.is_finite() || !(1e-6..std::f64::consts::PI - 1e-6).contains(&angle.abs())
                {
                    return Err(ModelError::new(
                        "sheet bend angle must have magnitude between 1e-6 and pi−1e-6 radians",
                    ));
                }
                positive(radius + thickness * 0.5)?;
                Ok((angle, radius))
            })
            .collect::<Result<Vec<_>, ModelError>>()?;
        let mut cutouts = self.evaluate_cutouts(parameters, &flanges, width)?;
        let mut edge_flanges = self.evaluate_edge_flanges(parameters, &flanges, width)?;
        let (mut flanges, mut bends) = (flanges, bends);
        let mut start = ((0.0, 0.0), 0.0);
        if let Some(hem) = &self.start_hem {
            let (length, turn, radius) = hem.evaluate(parameters, thickness)?;
            // Walk the hem from a trial start, then shift it to end at the
            // origin heading along the first flange.
            let (end, _) = advance(((0.0, 0.0), -turn), length, Some((turn, radius)), thickness);
            start = ((-end.0, -end.1), -turn);
            flanges.insert(0, length);
            bends.insert(0, (turn, radius));
            cutouts.iter_mut().for_each(|cutout| cutout.flange += 1);
            edge_flanges
                .iter_mut()
                .for_each(|(flange, _)| flange.flange += 1);
        }
        if let Some(hem) = &self.end_hem {
            let (length, turn, radius) = hem.evaluate(parameters, thickness)?;
            bends.push((turn, radius));
            flanges.push(length);
        }
        let reliefs = relieve(&edge_flanges, &flanges, bends.len(), width)?;
        let edge_flanges = edge_flanges.into_iter().map(|(flange, _)| flange).collect();
        let result = EvaluatedSheet {
            edge_flanges,
            reliefs,
            start,
            origin,
            width_axis,
            tangent,
            normal,
            width,
            thickness,
            flanges,
            bends,
            cutouts,
        };
        result.metrics(0.5)?; // Reject overflowing accumulated dimensions before kernel calls.
        Ok(result)
    }

    /// Cutouts lie inside one flange's straight region and across the width,
    /// with unique IDs. At most 10,000. O(cutouts).
    fn evaluate_cutouts(
        &self,
        parameters: &HashMap<String, ParameterValue>,
        flanges: &[f64],
        width: f64,
    ) -> Result<Vec<EvaluatedCutout>, ModelError> {
        if self.cutouts.len() > 10_000 {
            return Err(ModelError::new("a sheet has at most 10000 cutouts"));
        }
        let mut ids = HashSet::new();
        self.cutouts
            .iter()
            .map(|cutout| {
                if cutout.id.is_empty() || !ids.insert(cutout.id.as_str()) {
                    return Err(ModelError::new("sheet cutouts need unique nonempty IDs"));
                }
                let length = |e: &ScalarExpr| scalar(e, parameters, Dimension::Length);
                let outline = match &cutout.shape {
                    SheetMetalCutoutShape::Circle { diameter } => CutoutOutline::Circle {
                        diameter: positive(length(diameter)?)?,
                    },
                    SheetMetalCutoutShape::Rectangle { length: l, width: w } => {
                        CutoutOutline::Rectangle {
                            length: positive(length(l)?)?,
                            width: positive(length(w)?)?,
                        }
                    }
                    SheetMetalCutoutShape::Slot { length: l, width: w } => {
                        let (l, w) = (positive(length(l)?)?, positive(length(w)?)?);
                        if l < w {
                            return Err(ModelError::new(format!(
                                "slot cutout '{}' must be at least as long as it is wide",
                                cutout.id
                            )));
                        }
                        CutoutOutline::Slot { length: l, width: w }
                    }
                };
                let flange = *flanges.get(cutout.flange).ok_or_else(|| {
                    ModelError::new(format!(
                        "cutout '{}' names flange {}, but the sheet has {}",
                        cutout.id,
                        cutout.flange,
                        flanges.len()
                    ))
                })?;
                let (along, across) = (length(&cutout.along)?, length(&cutout.across)?);
                let (half_along, half_across) = outline.half_extents();
                let inside = along.is_finite()
                    && across.is_finite()
                    && along - half_along > 0.0
                    && along + half_along < flange
                    && across - half_across > 0.0
                    && across + half_across < width;
                if !inside {
                    return Err(ModelError::new(format!(
                        "cutout '{}' must lie inside its flange's straight region and the sheet width",
                        cutout.id
                    )));
                }
                Ok(EvaluatedCutout {
                    id: cutout.id.clone(),
                    flange: cutout.flange,
                    along,
                    across,
                    outline,
                })
            })
            .collect()
    }

    /// Edge flanges inside their base flange's straight region, not
    /// overlapping on one edge, with unique IDs and valid bends; each with
    /// its relief depth. At most 10,000. O(edge flanges log edge flanges).
    fn evaluate_edge_flanges(
        &self,
        parameters: &HashMap<String, ParameterValue>,
        flanges: &[f64],
        width: f64,
    ) -> Result<Vec<(EvaluatedEdgeFlange, Option<f64>)>, ModelError> {
        if self.edge_flanges.len() > 10_000 {
            return Err(ModelError::new("a sheet has at most 10000 edge flanges"));
        }
        let mut ids = HashSet::new();
        let mut evaluated = self
            .edge_flanges
            .iter()
            .map(|edge| {
                if edge.id.is_empty() || !ids.insert(edge.id.as_str()) {
                    return Err(ModelError::new(
                        "sheet edge flanges need unique nonempty IDs",
                    ));
                }
                let length = |e: &ScalarExpr| scalar(e, parameters, Dimension::Length);
                let base = *flanges.get(edge.flange).ok_or_else(|| {
                    ModelError::new(format!(
                        "edge flange '{}' names flange {}, but the sheet has {}",
                        edge.id,
                        edge.flange,
                        flanges.len()
                    ))
                })?;
                let start = length(&edge.start)?;
                let span = positive(length(&edge.length)?)?;
                let tolerance = 1e-9 * (1.0 + base);
                if !(start.is_finite() && start >= -tolerance && start + span <= base + tolerance) {
                    return Err(ModelError::new(format!(
                        "edge flange '{}' must lie along its base flange's straight region",
                        edge.id
                    )));
                }
                let angle = scalar(&edge.angle_radians, parameters, Dimension::Scalar)?;
                if !angle.is_finite()
                    || !(1e-6..std::f64::consts::PI - 1e-6).contains(&angle.abs())
                {
                    return Err(ModelError::new(format!(
                        "edge flange '{}' bend angle must have magnitude between 1e-6 and pi−1e-6 radians",
                        edge.id
                    )));
                }
                let relief = edge
                    .relief_depth
                    .as_ref()
                    .map(|depth| {
                        let depth = positive(length(depth)?)?;
                        if depth >= width {
                            return Err(ModelError::new(format!(
                                "edge flange '{}' relief must be shallower than the sheet width",
                                edge.id
                            )));
                        }
                        Ok(depth)
                    })
                    .transpose()?;
                Ok((
                    EvaluatedEdgeFlange {
                        id: edge.id.clone(),
                        flange: edge.flange,
                        edge: edge.edge,
                        start: start.max(0.0),
                        length: span,
                        angle,
                        radius: positive(length(&edge.inside_radius)?)?,
                        height: positive(length(&edge.height)?)?,
                    },
                    relief,
                ))
            })
            .collect::<Result<Vec<_>, ModelError>>()?;
        // Edge flanges on one edge of one flange may touch but not overlap.
        evaluated.sort_by(|(a, _), (b, _)| {
            (a.flange, a.edge == SheetEdge::Far, a.start)
                .partial_cmp(&(b.flange, b.edge == SheetEdge::Far, b.start))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for pair in evaluated.windows(2) {
            let (a, b) = (&pair[0].0, &pair[1].0);
            if a.flange == b.flange && a.edge == b.edge && b.start < a.start + a.length - 1e-9 {
                return Err(ModelError::new(format!(
                    "edge flanges '{}' and '{}' overlap on one edge",
                    a.id, b.id
                )));
            }
        }
        Ok(evaluated)
    }

    /// Evaluates the blank without kernel handles. K is explicitly supplied:
    /// allowance = |angle| * (inside_radius + K * thickness).
    pub fn flat_pattern(
        &self,
        parameters: &HashMap<String, ParameterValue>,
        neutral_factor: f64,
    ) -> Result<FlatPatternMetrics, ModelError> {
        self.evaluate(parameters)?.metrics(neutral_factor)
    }

    /// The blank with each bend's allowance from `table` (schema 161).
    pub fn flat_pattern_with_table(
        &self,
        parameters: &HashMap<String, ParameterValue>,
        neutral_factor: f64,
        table: &BendTable,
    ) -> Result<FlatPatternMetrics, ModelError> {
        self.evaluate(parameters)?
            .metrics_with(neutral_factor, Some(table))
    }
}

/// One step of a sheet's midplane walk in its (tangent, normal) plane:
/// from `point` heading at `angle`, along a flange of `length` and then
/// round `bend` (signed turn, inside radius) if any.
pub fn advance(
    (point, angle): ((f64, f64), f64),
    length: f64,
    bend: Option<(f64, f64)>,
    thickness: f64,
) -> ((f64, f64), f64) {
    let point = (
        point.0 + length * angle.cos(),
        point.1 + length * angle.sin(),
    );
    let Some((turn, radius)) = bend else {
        return (point, angle);
    };
    let sign = turn.signum();
    let center_radius = radius + thickness * 0.5;
    let center = (
        point.0 - sign * center_radius * angle.sin(),
        point.1 + sign * center_radius * angle.cos(),
    );
    let angle = angle + turn;
    (
        (
            center.0 + sign * center_radius * angle.sin(),
            center.1 - sign * center_radius * angle.cos(),
        ),
        angle,
    )
}

/// The reliefs edge flanges ask for: one per bend and edge that an edge
/// flange with a relief depth reaches, the deepest asked for. Reliefs at one
/// bend from both edges must leave material between them. O(edge flanges).
fn relieve(
    edge_flanges: &[(EvaluatedEdgeFlange, Option<f64>)],
    flanges: &[f64],
    bend_count: usize,
    width: f64,
) -> Result<Vec<EvaluatedRelief>, ModelError> {
    let mut reliefs: Vec<EvaluatedRelief> = Vec::new();
    for (edge, depth) in edge_flanges {
        let Some(depth) = *depth else {
            continue;
        };
        let tolerance = 1e-9 * (1.0 + flanges[edge.flange]);
        let mut bends = Vec::new();
        if edge.start <= tolerance && edge.flange > 0 {
            bends.push(edge.flange - 1);
        }
        if edge.start + edge.length >= flanges[edge.flange] - tolerance && edge.flange < bend_count
        {
            bends.push(edge.flange);
        }
        if bends.is_empty() {
            return Err(ModelError::new(format!(
                "edge flange '{}' has a relief depth but reaches no bend",
                edge.id
            )));
        }
        for bend in bends {
            match reliefs
                .iter_mut()
                .find(|r| r.bend == bend && r.edge == edge.edge)
            {
                Some(relief) => relief.depth = relief.depth.max(depth),
                None => reliefs.push(EvaluatedRelief {
                    bend,
                    edge: edge.edge,
                    depth,
                }),
            }
        }
    }
    for relief in &reliefs {
        let other = reliefs
            .iter()
            .find(|r| r.bend == relief.bend && r.edge != relief.edge);
        if other.is_some_and(|other| other.depth + relief.depth >= width) {
            return Err(ModelError::new(
                "reliefs from both edges of a bend must leave material between them",
            ));
        }
    }
    Ok(reliefs)
}

pub struct EvaluatedSheet {
    /// Where the midplane walk starts in the (tangent, normal) plane and its
    /// heading: the origin heading along the tangent, unless a start hem
    /// comes first.
    pub start: ((f64, f64), f64),
    pub origin: Vec3,
    pub width_axis: Vec3,
    pub tangent: Vec3,
    pub normal: Vec3,
    pub width: f64,
    pub thickness: f64,
    /// Flanges and bends including any hems; a rolled hem's flange is zero.
    pub flanges: Vec<f64>,
    pub bends: Vec<(f64, f64)>,
    pub cutouts: Vec<EvaluatedCutout>,
    /// Edge flanges on the internal flange numbering (schema 176).
    pub edge_flanges: Vec<EvaluatedEdgeFlange>,
    pub reliefs: Vec<EvaluatedRelief>,
}

impl EvaluatedSheet {
    pub fn metrics(&self, neutral_factor: f64) -> Result<FlatPatternMetrics, ModelError> {
        self.metrics_with(neutral_factor, None)
    }

    /// The blank's metrics, with allowances from `table` when given and from
    /// the neutral factor otherwise. A table must cover every bend.
    /// O(flanges × log table size + cutouts).
    pub fn metrics_with(
        &self,
        neutral_factor: f64,
        table: Option<&BendTable>,
    ) -> Result<FlatPatternMetrics, ModelError> {
        if let Some(table) = table {
            table.validate()?;
        }
        if !neutral_factor.is_finite() || !(0.0..=1.0).contains(&neutral_factor) {
            return Err(ModelError::new(
                "sheet neutral factor must be finite in [0, 1]",
            ));
        }
        let mut length = 0.0;
        let mut allowances = Vec::with_capacity(self.bends.len());
        let mut lines = Vec::with_capacity(self.bends.len());
        let mut starts = Vec::with_capacity(self.flanges.len());
        for (index, flange) in self.flanges.iter().enumerate() {
            starts.push(length);
            length += flange;
            if let Some((angle, radius)) = self.bends.get(index) {
                let allowance = match table {
                    Some(table) => table.allowance(index, *angle, *radius, self.thickness)?,
                    None => angle.abs() * (radius + neutral_factor * self.thickness),
                };
                lines.push(length + allowance * 0.5);
                allowances.push(allowance);
                length += allowance;
            }
        }
        positive(length)?;
        positive(length * self.width * self.thickness)?;
        let edge_tabs = self
            .edge_flanges
            .iter()
            .enumerate()
            .map(|(index, edge)| {
                let allowance = match table {
                    Some(table) => table.allowance(
                        self.bends.len() + index,
                        edge.angle,
                        edge.radius,
                        self.thickness,
                    )?,
                    None => edge.angle.abs() * (edge.radius + neutral_factor * self.thickness),
                };
                let x0 = starts[edge.flange] + edge.start;
                let reach = allowance + edge.height;
                let (across_mm, bend_line_mm) = match edge.edge {
                    SheetEdge::Far => (
                        [self.width, self.width + reach],
                        self.width + allowance * 0.5,
                    ),
                    SheetEdge::Origin => ([-reach, 0.0], -allowance * 0.5),
                };
                Ok(FlatEdgeTab {
                    id: edge.id.clone(),
                    along_mm: [x0, x0 + edge.length],
                    across_mm,
                    bend_line_mm,
                    allowance_mm: allowance,
                })
            })
            .collect::<Result<Vec<_>, ModelError>>()?;
        let reliefs = self
            .reliefs
            .iter()
            .map(|relief| {
                let (line, half) = (lines[relief.bend], allowances[relief.bend] * 0.5);
                FlatRelief {
                    along_mm: [line - half, line + half],
                    across_mm: match relief.edge {
                        SheetEdge::Far => [self.width - relief.depth, self.width],
                        SheetEdge::Origin => [0.0, relief.depth],
                    },
                }
            })
            .collect();
        Ok(FlatPatternMetrics {
            edge_tabs,
            reliefs,
            length_mm: length,
            width_mm: self.width,
            thickness_mm: self.thickness,
            neutral_factor,
            bend_allowances_mm: allowances,
            bend_lines_mm: lines,
            cutouts: self
                .cutouts
                .iter()
                .map(|cutout| FlatCutout {
                    id: cutout.id.clone(),
                    center_mm: [starts[cutout.flange] + cutout.along, cutout.across],
                    outline: cutout.outline,
                })
                .collect(),
            bend_table: table.map(|table| table.name.clone()),
        })
    }
}
