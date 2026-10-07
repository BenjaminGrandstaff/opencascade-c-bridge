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
}
impl DrawingSheet {
    pub fn paper_size_mm(&self) -> [f64; 2] {
        let [short, long] = self.size.dimensions_mm();
        match self.orientation {
            DrawingSheetOrientation::Portrait => [short, long],
            DrawingSheetOrientation::Landscape => [long, short],
        }
    }
    pub(super) fn validate(&self, drawing: &DrawingDefinition) -> Result<(), ModelError> {
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
        Ok(())
    }
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
    }
}
fn label(position_mm: [f64; 2], text: String) -> DrawingLabel {
    DrawingLabel {
        position_mm,
        text,
        stack: None,
    }
}

pub(super) fn decorate(drawing: &mut GeneratedDrawing, sheet: &DrawingSheet, scales: &[f64]) {
    let [w, h] = sheet.paper_size_mm();
    let m = 12.7;
    let right = w - m;
    let left = right - 180.0;
    let top = m + 60.0 + 10.0 * drawing.metadata.len() as f64;
    drawing.sheet_lines.push(line(vec![
        [m, m],
        [right, m],
        [right, h - m],
        [m, h - m],
        [m, m],
    ]));
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
    ));
    drawing.sheet_labels.push(label(
        [right - 38.0, m + 53.0],
        format!("REV: {}", sheet.revision),
    ));
    let chars: Vec<_> = drawing.title.chars().collect();
    for (i, chunk) in chars.chunks(58).enumerate() {
        drawing.sheet_labels.push(label(
            [left + 2.0, m + 43.0 - i as f64 * 6.0],
            chunk.iter().collect(),
        ));
    }
    let scale = if scales.windows(2).all(|p| p[0] == p[1]) {
        format!("SCALE: {}:1", scale_text(scales[0]))
    } else {
        "SCALE: AS SHOWN".into()
    };
    drawing
        .sheet_labels
        .push(label([left + 2.0, m + 23.0], scale));
    drawing.sheet_labels.push(label(
        [left + 92.0, m + 23.0],
        format!("SIZE: {}", sheet.size.name()),
    ));
    drawing.sheet_labels.push(label(
        [left + 92.0, m + 8.0],
        format!("SHEET {} OF {}", sheet.sheet_number, sheet.sheet_count),
    ));
    if let Some(convention) = sheet.projection {
        projection(drawing, [left + 30.0, m + 10.0], convention);
    }
    for (i, (key, value)) in drawing.metadata.iter().enumerate() {
        let y = m + 60.0 + 10.0 * i as f64;
        drawing.sheet_lines.push(line(vec![[left, y], [right, y]]));
        drawing
            .sheet_labels
            .push(label([left + 2.0, y + 3.0], format!("{key}: {value}")));
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
    drawing.sheet_labels.push(label(
        [x + 26.0, y - 1.0],
        match convention {
            ProjectionConvention::FirstAngle => "1ST",
            ProjectionConvention::ThirdAngle => "3RD",
        }
        .into(),
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
