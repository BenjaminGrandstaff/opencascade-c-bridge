//! Standard paper sizes and bounded sheet furniture; no conformity certification.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DrawingSheetSize {
    AnsiA,
    AnsiB,
    AnsiC,
    AnsiD,
    AnsiE,
    IsoA0,
    IsoA1,
    IsoA2,
    IsoA3,
    IsoA4,
}
impl DrawingSheetSize {
    /// Trimmed dimensions, shorter side first, in millimeters.
    pub fn dimensions_mm(self) -> [f64; 2] {
        match self {
            Self::AnsiA => [215.9, 279.4],
            Self::AnsiB => [279.4, 431.8],
            Self::AnsiC => [431.8, 558.8],
            Self::AnsiD => [558.8, 863.6],
            Self::AnsiE => [863.6, 1117.6],
            Self::IsoA0 => [841.0, 1189.0],
            Self::IsoA1 => [594.0, 841.0],
            Self::IsoA2 => [420.0, 594.0],
            Self::IsoA3 => [297.0, 420.0],
            Self::IsoA4 => [210.0, 297.0],
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::AnsiA => "ANSI A",
            Self::AnsiB => "ANSI B",
            Self::AnsiC => "ANSI C",
            Self::AnsiD => "ANSI D",
            Self::AnsiE => "ANSI E",
            Self::IsoA0 => "ISO A0",
            Self::IsoA1 => "ISO A1",
            Self::IsoA2 => "ISO A2",
            Self::IsoA3 => "ISO A3",
            Self::IsoA4 => "ISO A4",
        }
    }
}
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum DrawingSheetOrientation {
    Portrait,
    #[default]
    Landscape,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionConvention {
    FirstAngle,
    ThirdAngle,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DrawingSheet {
    pub size: DrawingSheetSize,
    #[serde(default)]
    pub orientation: DrawingSheetOrientation,
    pub drawing_number: String,
    #[serde(default)]
    pub revision: String,
    pub sheet_number: u32,
    pub sheet_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projection: Option<ProjectionConvention>,
    /// Text heights by role from public drafting references (schema 134);
    /// `None` keeps uniform 3 mm text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lettering: Option<SheetLettering>,
    /// Grid reference zones in the margin (schema 134).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zones: Option<SheetZones>,
    /// Thick/thin line widths in exports (schema 134); `None` keeps the
    /// uniform legacy widths.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_weights: Option<SheetLineWeights>,
    /// How the sheets sharing this drawing number are revised (schema 159).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revisions: Option<SheetRevisions>,
}

/// The revision practice of a multi-sheet drawing. Every sheet of the
/// drawing states the same method.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SheetRevisions {
    pub method: SheetRevisionMethod,
    /// Top-left corner of sheet 1's SHEET / REV status table, listing each
    /// sheet's current revision; per-sheet revision only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_table_mm: Option<[f64; 2]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SheetRevisionMethod {
    /// Every sheet shows the drawing's revision; sheet 1 records the
    /// releases and revision table, and a change to any sheet revises all.
    SameRevision,
    /// Each sheet keeps its own releases and revision; sheet 1 may list
    /// every sheet's revision in a status table.
    PerSheet,
}

/// Lettering preset. `Standard` takes inch-sheet heights from the DOD-STD-100D
/// lettering table (minimum of each range) and ISO-sheet heights from the
/// ASME Y14.2-2014 minimums as summarized in public training material.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SheetLettering {
    Standard,
}

/// Grid reference zones. `Iso5457` uses 50 mm fields from the sheet's
/// centring axes with the ISO 5457 field counts (ISO sizes only); `Grid`
/// divides any sheet into equal columns and rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum SheetZones {
    Iso5457,
    Grid { columns: u32, rows: u32 },
}

/// Line-width preset. `Y14_2m` uses the ANSI Y14.2M thick (0.7 mm) and thin
/// (0.35 mm) widths: visible outlines, the sheet frame and cutting planes are
/// thick; hidden, dimension, leader, GD&T, hatch, centre, table and zone lines
/// are thin.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub enum SheetLineWeights {
    #[serde(rename = "y14_2m")]
    Y14_2m,
}
impl SheetLineWeights {
    /// `(thick, thin)` in millimeters.
    pub fn widths_mm(self) -> (f64, f64) {
        match self {
            Self::Y14_2m => (0.7, 0.35),
        }
    }
}

/// Text heights in millimeters for each role on a generated sheet.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DrawingLettering {
    pub drawing_number: f64,
    pub title: f64,
    pub revision: f64,
    /// Title-block and supplementary data entries (scale, size, sheet, metadata).
    pub block: f64,
    /// Dimensions, notes, GD&T, tables and other annotations.
    pub notes: f64,
    /// SECTION/VIEW/DETAIL captions.
    pub caption: f64,
    /// Section and view letters at cutting planes.
    pub section_letter: f64,
    pub zone: f64,
}
pub const TEXT_MM: f64 = 3.0;
impl Default for DrawingLettering {
    fn default() -> Self {
        Self {
            drawing_number: TEXT_MM,
            title: TEXT_MM,
            revision: TEXT_MM,
            block: TEXT_MM,
            notes: TEXT_MM,
            caption: TEXT_MM,
            section_letter: TEXT_MM,
            zone: 3.5,
        }
    }
}
const INCH: f64 = 25.4;
impl DrawingLettering {
    pub fn for_sheet(sheet: Option<&DrawingSheet>) -> Self {
        let Some(sheet) = sheet.filter(|sheet| sheet.lettering.is_some()) else {
            return Self::default();
        };
        use DrawingSheetSize::*;
        match sheet.size {
            // DOD-STD-100D Figure 100-1, A and B sizes.
            AnsiA | AnsiB => Self {
                drawing_number: 0.24 * INCH,
                title: 0.19 * INCH,
                revision: 0.14 * INCH,
                block: 0.19 * INCH,
                notes: 0.14 * INCH,
                caption: 0.14 * INCH,
                section_letter: 0.19 * INCH,
                zone: 6.0,
            },
            // DOD-STD-100D Figure 100-1, C and larger.
            AnsiC | AnsiD | AnsiE => Self {
                drawing_number: 0.24 * INCH,
                title: 0.24 * INCH,
                revision: 0.16 * INCH,
                block: 0.24 * INCH,
                notes: 0.16 * INCH,
                caption: 0.19 * INCH,
                section_letter: 0.24 * INCH,
                zone: 6.0,
            },
            // Y14.2-2014 minimums: title, number and revision 6 mm on A0/A1
            // and 3 mm on A2-A4; section/view and zone letters 6 mm; other
            // characters 3 mm.
            IsoA0 | IsoA1 => Self {
                drawing_number: 6.0,
                title: 6.0,
                revision: 6.0,
                block: 3.0,
                notes: 3.0,
                caption: 6.0,
                section_letter: 6.0,
                zone: 6.0,
            },
            IsoA2 | IsoA3 | IsoA4 => Self {
                drawing_number: 3.0,
                title: 3.0,
                revision: 3.0,
                block: 3.0,
                notes: 3.0,
                caption: 6.0,
                section_letter: 6.0,
                zone: 6.0,
            },
        }
    }
}

/// Field boundaries of a sheet's reference zones, in paper millimeters.
/// Columns are numbered from 1 left to right; rows are lettered from `A` at
/// the top, skipping `I` and `O`.
#[derive(Clone, Debug, PartialEq)]
pub struct DrawingZoneGrid {
    /// Ascending x boundaries, `columns + 1` values from frame to frame.
    pub columns_mm: Vec<f64>,
    /// Descending y boundaries, `rows + 1` values from top to bottom.
    pub rows_mm: Vec<f64>,
}
const ZONE_LETTERS: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ";
/// Inset of the drawing frame from the trimmed edge.
pub const FRAME_MM: f64 = 12.7;
impl DrawingZoneGrid {
    pub fn column_label(index: usize) -> String {
        (index + 1).to_string()
    }
    pub fn row_label(index: usize) -> String {
        char::from(ZONE_LETTERS[index]).to_string()
    }
    /// Zone reference such as `C4` for a point inside the frame. O(log n).
    pub fn zone_at(&self, point_mm: [f64; 2]) -> Option<String> {
        let [x, y] = point_mm;
        let (first, last) = (self.columns_mm[0], *self.columns_mm.last()?);
        let (top, bottom) = (self.rows_mm[0], *self.rows_mm.last()?);
        if !(first..=last).contains(&x) || !(bottom..=top).contains(&y) {
            return None;
        }
        let column = self
            .columns_mm
            .partition_point(|&b| b <= x)
            .clamp(1, self.columns_mm.len() - 1)
            - 1;
        let row = self
            .rows_mm
            .partition_point(|&b| b >= y)
            .clamp(1, self.rows_mm.len() - 1)
            - 1;
        Some(format!(
            "{}{}",
            Self::row_label(row),
            Self::column_label(column)
        ))
    }
}
/// ISO 5457 field counts per side, (long side, short side).
fn iso_counts(size: DrawingSheetSize) -> Option<(usize, usize)> {
    use DrawingSheetSize::*;
    Some(match size {
        IsoA0 => (24, 16),
        IsoA1 => (16, 12),
        IsoA2 => (12, 8),
        IsoA3 => (8, 6),
        IsoA4 => (6, 4),
        _ => return None,
    })
}
/// Boundaries for `count` fields between `low` and `high`: 50 mm fields from
/// the centre outward, outer fields taking the remainder (ISO 5457).
fn iso_boundaries(low: f64, high: f64, count: usize) -> Vec<f64> {
    let center = 0.5 * (low + high);
    let half = count / 2;
    let mut out = vec![low];
    for i in (1..half).rev() {
        out.push(center - 50.0 * i as f64);
    }
    out.push(center);
    for i in 1..half {
        out.push(center + 50.0 * i as f64);
    }
    out.push(high);
    out
}
fn even_boundaries(low: f64, high: f64, count: usize) -> Vec<f64> {
    (0..=count)
        .map(|i| low + (high - low) * i as f64 / count as f64)
        .collect()
}
impl DrawingSheet {
    /// The reference-zone grid inside the frame, if zones are enabled.
    pub fn zone_grid(&self) -> Option<DrawingZoneGrid> {
        let [w, h] = self.paper_size_mm();
        let (left, right, bottom, top) = (FRAME_MM, w - FRAME_MM, FRAME_MM, h - FRAME_MM);
        match self.zones? {
            SheetZones::Iso5457 => {
                let (long, short) = iso_counts(self.size)?;
                let (columns, rows) = if w >= h { (long, short) } else { (short, long) };
                Some(DrawingZoneGrid {
                    columns_mm: iso_boundaries(left, right, columns),
                    rows_mm: iso_boundaries(bottom, top, rows)
                        .into_iter()
                        .rev()
                        .collect(),
                })
            }
            SheetZones::Grid { columns, rows } => Some(DrawingZoneGrid {
                columns_mm: even_boundaries(left, right, columns as usize),
                rows_mm: even_boundaries(bottom, top, rows as usize)
                    .into_iter()
                    .rev()
                    .collect(),
            }),
        }
    }
    pub fn paper_size_mm(&self) -> [f64; 2] {
        let [short, long] = self.size.dimensions_mm();
        match self.orientation {
            DrawingSheetOrientation::Portrait => [short, long],
            DrawingSheetOrientation::Landscape => [long, short],
        }
    }
    pub fn validate(&self, drawing: &DrawingDefinition) -> Result<(), ModelError> {
        if self.drawing_number.trim().is_empty()
            || self.sheet_number == 0
            || self.sheet_number > self.sheet_count
        {
            return Err(ModelError::new(
                "drawing sheet needs a drawing number and valid 1-based sheet numbering",
            ));
        }
        check_field(&self.drawing_number, 36)?;
        check_field(&self.revision, 7)?;
        check_field(&drawing.title, 116)?;
        if drawing.metadata.len() > 8 {
            return Err(ModelError::new(
                "standard sheet allows at most eight metadata rows",
            ));
        }
        for (key, value) in &drawing.metadata {
            check_field(&format!("{key}: {value}"), 58)?;
        }
        match self.zones {
            Some(SheetZones::Iso5457) if iso_counts(self.size).is_none() => {
                return Err(ModelError::new("ISO 5457 zones need an ISO sheet size"));
            }
            Some(SheetZones::Grid { columns, rows })
                if !(1..=40).contains(&columns)
                    || !(1..=ZONE_LETTERS.len() as u32).contains(&rows) =>
            {
                return Err(ModelError::new(
                    "zone grid needs 1-40 columns and 1-24 rows",
                ));
            }
            _ => {}
        }
        if self.lettering.is_some() {
            let text = DrawingLettering::for_sheet(Some(self));
            let fits =
                |value: &str, height: f64, cell: f64| text_layout::width(value, height) <= cell;
            if !fits(
                &format!("DRAWING: {}", self.drawing_number),
                text.drawing_number,
                136.0,
            ) || !fits(&format!("REV: {}", self.revision), text.revision, 36.0)
                || title_lines(&drawing.title, text.title).is_none_or(|lines| lines.len() > 2)
                || drawing
                    .metadata
                    .iter()
                    .any(|(key, value)| !fits(&format!("{key}: {value}"), text.block, 176.0))
            {
                return Err(ModelError::new(
                    "title block text does not fit its cell at the standard lettering height",
                ));
            }
        }
        Ok(())
    }
}
/// Greedy word wrap to the 176 mm title cell at `height`; `None` when a
/// single word is wider than the cell.
fn title_lines(title: &str, height: f64) -> Option<Vec<String>> {
    let mut lines: Vec<String> = Vec::new();
    for word in title.split(' ') {
        if text_layout::width(word, height) > 176.0 {
            return None;
        }
        match lines.last_mut() {
            Some(line) if text_layout::width(&format!("{line} {word}"), height) <= 176.0 => {
                line.push(' ');
                line.push_str(word);
            }
            _ => lines.push(word.to_owned()),
        }
    }
    Some(lines)
}
fn check_field(text: &str, maximum: usize) -> Result<(), ModelError> {
    if text.chars().count() > maximum || text.chars().any(char::is_control) {
        return Err(ModelError::new(
            "standard sheet field exceeds its cell capacity or contains control characters",
        ));
    }
    Ok(())
}
fn line(points_mm: Vec<[f64; 2]>) -> DrawingPolyline {
    DrawingPolyline {
        points_mm,
        hidden: false,
        thin: true,
    }
}
fn label(position_mm: [f64; 2], text: String, height_mm: f64) -> DrawingLabel {
    DrawingLabel {
        position_mm,
        text,
        stack: None,
        height_mm,
    }
}

pub fn decorate(drawing: &mut GeneratedDrawing, sheet: &DrawingSheet, scales: &[f64]) {
    let [w, h] = sheet.paper_size_mm();
    let m = FRAME_MM;
    let text = drawing.lettering;
    let right = w - m;
    let left = right - 180.0;
    let top = m + 60.0 + 10.0 * drawing.metadata.len() as f64;
    let mut frame = line(vec![[m, m], [right, m], [right, h - m], [m, h - m], [m, m]]);
    frame.thin = false;
    drawing.sheet_lines.push(frame);
    drawing
        .sheet_lines
        .push(line(vec![[left, m], [left, top], [right, top]]));
    for y in [m + 20.0, m + 30.0, m + 50.0] {
        drawing.sheet_lines.push(line(vec![[left, y], [right, y]]));
    }
    drawing.sheet_lines.push(line(vec![
        [right - 40.0, m + 50.0],
        [right - 40.0, m + 60.0],
    ]));
    drawing
        .sheet_lines
        .push(line(vec![[left + 90.0, m + 20.0], [left + 90.0, m + 30.0]]));
    drawing
        .sheet_lines
        .push(line(vec![[left + 90.0, m], [left + 90.0, m + 20.0]]));
    drawing.sheet_labels.push(label(
        [left + 2.0, m + 53.0],
        format!("DRAWING: {}", sheet.drawing_number),
        text.drawing_number,
    ));
    drawing.sheet_labels.push(label(
        [right - 38.0, m + 53.0],
        format!("REV: {}", sheet.revision),
        text.revision,
    ));
    if sheet.lettering.is_some() {
        // Lines spaced by at least half the letter height (Y14.2).
        let first = m + 48.0 - text.title;
        let rows = title_lines(&drawing.title, text.title).unwrap_or_default();
        for (i, row) in rows.into_iter().enumerate() {
            drawing.sheet_labels.push(label(
                [left + 2.0, first - i as f64 * 1.5 * text.title],
                row,
                text.title,
            ));
        }
    } else {
        let chars: Vec<_> = drawing.title.chars().collect();
        for (i, chunk) in chars.chunks(58).enumerate() {
            drawing.sheet_labels.push(label(
                [left + 2.0, m + 43.0 - i as f64 * 6.0],
                chunk.iter().collect(),
                text.title,
            ));
        }
    }
    let scale = if scales.windows(2).all(|p| p[0] == p[1]) {
        format!("SCALE: {}:1", scale_text(scales[0]))
    } else {
        "SCALE: AS SHOWN".into()
    };
    drawing
        .sheet_labels
        .push(label([left + 2.0, m + 23.0], scale, text.block));
    drawing.sheet_labels.push(label(
        [left + 92.0, m + 23.0],
        format!("SIZE: {}", sheet.size.name()),
        text.block,
    ));
    drawing.sheet_labels.push(label(
        [left + 92.0, m + 8.0],
        format!("SHEET {} OF {}", sheet.sheet_number, sheet.sheet_count),
        text.block,
    ));
    if let Some(convention) = sheet.projection {
        projection(drawing, [left + 30.0, m + 10.0], convention);
    }
    for (i, (key, value)) in drawing.metadata.iter().enumerate() {
        let y = m + 60.0 + 10.0 * i as f64;
        drawing.sheet_lines.push(line(vec![[left, y], [right, y]]));
        drawing.sheet_labels.push(label(
            [left + 2.0, y + 3.0],
            format!("{key}: {value}"),
            text.block,
        ));
    }
    if let Some(grid) = sheet.zone_grid() {
        zones(drawing, sheet, &grid);
        drawing.zones = Some(grid);
    }
}

/// Zone ticks and labels in the margin band between the frame and a border
/// 4 mm inside the trimmed edge; ISO centring marks run 10 mm into the
/// frame. ISO A4 sheets are zoned on the top and right only (ISO 5457).
/// O(columns + rows).
fn zones(drawing: &mut GeneratedDrawing, sheet: &DrawingSheet, grid: &DrawingZoneGrid) {
    const BORDER: f64 = 4.0;
    let [w, h] = sheet.paper_size_mm();
    let size = drawing.lettering.zone;
    let a4 = sheet.size == DrawingSheetSize::IsoA4;
    let (top, bottom, left, right) = (true, !a4, !a4, true);
    drawing.sheet_lines.push(line(vec![
        [BORDER, BORDER],
        [w - BORDER, BORDER],
        [w - BORDER, h - BORDER],
        [BORDER, h - BORDER],
        [BORDER, BORDER],
    ]));
    let band = 0.5 * (BORDER + FRAME_MM);
    let (x0, x1) = (grid.columns_mm[0], *grid.columns_mm.last().unwrap());
    let (y0, y1) = (*grid.rows_mm.last().unwrap(), grid.rows_mm[0]);
    for (i, pair) in grid.columns_mm.windows(2).enumerate() {
        let text = DrawingZoneGrid::column_label(i);
        let x = 0.5 * (pair[0] + pair[1]) - 0.5 * text_layout::width(&text, size);
        for (enabled, edge, y) in [(top, h - BORDER, h - FRAME_MM), (bottom, BORDER, FRAME_MM)] {
            if !enabled {
                continue;
            }
            if i > 0 {
                drawing
                    .sheet_lines
                    .push(line(vec![[pair[0], edge], [pair[0], y]]));
            }
            let baseline = if edge > y { h - band } else { band } - 0.5 * size;
            drawing
                .sheet_labels
                .push(label([x, baseline], text.clone(), size));
        }
    }
    for (i, pair) in grid.rows_mm.windows(2).enumerate() {
        let text = DrawingZoneGrid::row_label(i);
        let y = 0.5 * (pair[0] + pair[1]) - 0.5 * size;
        for (enabled, edge, x) in [(left, BORDER, FRAME_MM), (right, w - BORDER, w - FRAME_MM)] {
            if !enabled {
                continue;
            }
            if i > 0 {
                drawing
                    .sheet_lines
                    .push(line(vec![[edge, pair[0]], [x, pair[0]]]));
            }
            let centre = if edge < x { band } else { w - band };
            let x = centre - 0.5 * text_layout::width(&text, size);
            drawing.sheet_labels.push(label([x, y], text.clone(), size));
        }
    }
    if matches!(sheet.zones, Some(SheetZones::Iso5457)) {
        let (cx, cy) = (0.5 * (x0 + x1), 0.5 * (y0 + y1));
        for points in [
            [[cx, BORDER], [cx, FRAME_MM + 10.0]],
            [[cx, h - BORDER], [cx, h - FRAME_MM - 10.0]],
            [[BORDER, cy], [FRAME_MM + 10.0, cy]],
            [[w - BORDER, cy], [w - FRAME_MM - 10.0, cy]],
        ] {
            let mut mark = line(points.to_vec());
            mark.thin = false;
            drawing.sheet_lines.push(mark);
        }
    }
}
fn projection(drawing: &mut GeneratedDrawing, center: [f64; 2], convention: ProjectionConvention) {
    let [x, y] = center;
    // Cone small end at left. First angle places its end view at right;
    // third angle places it at left, next to the narrow end.
    let (cone, circle) = match convention {
        ProjectionConvention::FirstAngle => (x - 8.0, x + 14.0),
        ProjectionConvention::ThirdAngle => (x + 8.0, x - 14.0),
    };
    drawing.sheet_lines.push(line(vec![
        [cone - 6.0, y - 2.5],
        [cone + 6.0, y - 5.0],
        [cone + 6.0, y + 5.0],
        [cone - 6.0, y + 2.5],
        [cone - 6.0, y - 2.5],
    ]));
    for radius in [5.0, 2.5] {
        drawing.sheet_lines.push(line(
            (0..=64)
                .map(|i| {
                    let a = i as f64 * std::f64::consts::TAU / 64.0;
                    [circle + radius * a.cos(), y + radius * a.sin()]
                })
                .collect(),
        ));
    }
    drawing
        .sheet_lines
        .push(line(vec![[x - 22.0, y], [x + 22.0, y]]));
    let size = drawing.lettering.block;
    drawing.sheet_labels.push(label(
        [x + 26.0, y - 1.0],
        match convention {
            ProjectionConvention::FirstAngle => "1ST",
            ProjectionConvention::ThirdAngle => "3RD",
        }
        .into(),
        size,
    ));
}

fn scale_text(scale: f64) -> String {
    if (0.001..=1000.0).contains(&scale) {
        format!("{scale:.6}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_owned()
    } else {
        format!("{scale:.6e}")
    }
}
