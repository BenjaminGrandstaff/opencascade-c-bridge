//! Drawing release records, approvals and the revision table.
//!
//! Releases are listed oldest first. Each may name the model document
//! revision it was released against, so a drawing can tell when the model has
//! moved on since its latest release.
use super::*;

/// One sign-off on a release, such as `CHECKED` or `APPROVED`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DrawingApproval {
    pub role: String,
    pub name: String,
    /// ISO date (`YYYY-MM-DD`), on or before the release date.
    pub date: String,
}

/// A released revision of the drawing.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DrawingRelease {
    /// Revision label, such as `A` or `02`; unique within the drawing.
    pub revision: String,
    pub description: String,
    /// ISO release date (`YYYY-MM-DD`); releases are in date order.
    pub date: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub approvals: Vec<DrawingApproval>,
    /// The model document revision (ledger ID) this release was made from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_revision: Option<String>,
}

/// A revision table drawn with its top-left corner at `position_mm`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DrawingRevisionTable {
    pub position_mm: [f64; 2],
}

/// Whether a drawing's latest release still matches the model's history.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum DrawingReleaseStatus {
    /// No releases are recorded.
    Unreleased,
    /// The latest release names the latest recorded model revision.
    Current { revision: String },
    /// The latest release names an older model revision, or none while the
    /// model has recorded revisions: the drawing should be re-released.
    ModelChanged {
        revision: String,
        released_against: Option<String>,
        latest_model_revision: String,
    },
    /// The latest release names no model revision and none are recorded.
    Untracked { revision: String },
}

const ROW_MM: f64 = 7.0;
const COLUMNS_MM: [f64; 4] = [12.0, 80.0, 24.0, 34.0];
const HEADERS: [&str; 4] = ["REV", "DESCRIPTION", "DATE", "APPROVED"];
/// Characters that fit the description column at about 2 mm each.
const DESCRIPTION_CHARS: usize = 38;

/// Parses a `YYYY-MM-DD` calendar date to a comparable ordinal.
fn iso_date(text: &str) -> Option<(u32, u32, u32)> {
    let bytes = text.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    let number = |range: std::ops::Range<usize>| text.get(range)?.parse::<u32>().ok();
    let (year, month, day) = (number(0..4)?, number(5..7)?, number(8..10)?);
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return None,
    };
    (year >= 1 && (1..=days).contains(&day)).then_some((year, month, day))
}

fn text_field(value: &str, maximum: usize, what: &str) -> Result<(), ModelError> {
    if value.trim().is_empty()
        || value.chars().count() > maximum
        || value.chars().any(char::is_control)
    {
        return Err(ModelError::new(format!(
            "release {what} must be 1-{maximum} characters without control characters"
        )));
    }
    Ok(())
}

/// Validates releases, approvals, the table and the title-block revision.
/// Model revision links are checked by the document. O(releases + approvals).
pub(super) fn validate(drawing: &DrawingDefinition) -> Result<(), ModelError> {
    let mut revisions = HashSet::new();
    let mut previous = None;
    for release in &drawing.releases {
        text_field(&release.revision, 8, "revision")?;
        if !release.revision.chars().all(|c| c.is_ascii_alphanumeric())
            || !revisions.insert(release.revision.as_str())
        {
            return Err(ModelError::new(
                "release revisions must be unique letters and digits",
            ));
        }
        text_field(&release.description, 200, "description")?;
        let date = iso_date(&release.date)
            .ok_or_else(|| ModelError::new("release dates must be valid YYYY-MM-DD dates"))?;
        if previous.is_some_and(|earlier| date < earlier) {
            return Err(ModelError::new("releases must be listed in date order"));
        }
        previous = Some(date);
        for approval in &release.approvals {
            text_field(&approval.role, 40, "approval role")?;
            text_field(&approval.name, 40, "approver name")?;
            let signed = iso_date(&approval.date)
                .ok_or_else(|| ModelError::new("approval dates must be valid YYYY-MM-DD dates"))?;
            if signed > date {
                return Err(ModelError::new(
                    "approvals must be dated on or before their release",
                ));
            }
        }
        if let Some(id) = &release.model_revision {
            text_field(id, 200, "model revision")?;
        }
    }
    if let Some(table) = &drawing.revision_table {
        if !finite_pair(table.position_mm) {
            return Err(ModelError::new("revision table position must be finite"));
        }
        if drawing.releases.is_empty() {
            return Err(ModelError::new(
                "a revision table needs at least one release",
            ));
        }
        let [x, top] = table.position_mm;
        let [width, height] = drawing.effective_paper_size_mm();
        let bottom = top - ROW_MM * (drawing.releases.len() + 1) as f64;
        if x < 0.0 || bottom < 0.0 || x + COLUMNS_MM.iter().sum::<f64>() > width || top > height {
            return Err(ModelError::new("revision table must fit on the paper"));
        }
    }
    // The title block shows the latest release's revision.
    if let (Some(sheet), Some(latest)) = (&drawing.sheet, drawing.releases.last())
        && sheet.revision != latest.revision
    {
        return Err(ModelError::new(format!(
            "title block revision '{}' must match the latest release '{}'",
            sheet.revision, latest.revision
        )));
    }
    Ok(())
}

/// Checks each release's model revision against the document ledger: it
/// must exist, and later releases may not name earlier revisions.
/// O(revisions + releases).
pub(crate) fn validate_model_links(
    drawing: &DrawingDefinition,
    ledger: &[DocumentRevision],
) -> Result<(), ModelError> {
    let positions: HashMap<&str, usize> = ledger
        .iter()
        .enumerate()
        .map(|(index, revision)| (revision.metadata.id.as_str(), index))
        .collect();
    let mut latest = None;
    for release in &drawing.releases {
        let Some(id) = &release.model_revision else {
            continue;
        };
        let position = *positions.get(id.as_str()).ok_or_else(|| {
            ModelError::new(format!(
                "release '{}' names unknown model revision '{id}'",
                release.revision
            ))
        })?;
        if latest.is_some_and(|earlier| position < earlier) {
            return Err(ModelError::new(
                "later releases may not name earlier model revisions",
            ));
        }
        latest = Some(position);
    }
    Ok(())
}

impl ModelDocument {
    /// The release status of drawing `id` against this document's revision
    /// ledger. O(1) after finding the drawing.
    pub fn drawing_release_status(&self, id: &str) -> Result<DrawingReleaseStatus, ModelError> {
        let drawing = self
            .drawings
            .iter()
            .find(|drawing| drawing.id == id)
            .ok_or_else(|| ModelError::new(format!("unknown drawing '{id}'")))?;
        let Some(release) = drawing.releases.last() else {
            return Ok(DrawingReleaseStatus::Unreleased);
        };
        let revision = release.revision.clone();
        let latest = self.revisions.last().map(|r| r.metadata.id.clone());
        Ok(match (latest, &release.model_revision) {
            (None, None) => DrawingReleaseStatus::Untracked { revision },
            (Some(latest), released) if released.as_ref() != Some(&latest) => {
                DrawingReleaseStatus::ModelChanged {
                    revision,
                    released_against: released.clone(),
                    latest_model_revision: latest,
                }
            }
            _ => DrawingReleaseStatus::Current { revision },
        })
    }
}

/// Outer rectangle 5, row separators 2 each, column separators 2 each.
pub(super) fn vertex_count(drawing: &DrawingDefinition) -> usize {
    if drawing.revision_table.is_some() {
        5 + 2 * drawing.releases.len() + 2 * (COLUMNS_MM.len() - 1)
    } else {
        0
    }
}

/// Draws the revision table with the sheet linework, oldest release first.
pub(super) fn append(drawing: &DrawingDefinition, out: &mut GeneratedDrawing) {
    let Some(table) = &drawing.revision_table else {
        return;
    };
    let [x, top] = table.position_mm;
    let width: f64 = COLUMNS_MM.iter().sum();
    let rows = drawing.releases.len() + 1;
    let bottom = top - ROW_MM * rows as f64;
    out.sheet_lines.push(DrawingPolyline {
        points_mm: vec![
            [x, top],
            [x + width, top],
            [x + width, bottom],
            [x, bottom],
            [x, top],
        ],
        hidden: false,
    });
    for row in 1..rows {
        let y = top - ROW_MM * row as f64;
        out.sheet_lines.push(DrawingPolyline {
            points_mm: vec![[x, y], [x + width, y]],
            hidden: false,
        });
    }
    let mut column_x = x;
    for width in &COLUMNS_MM[..COLUMNS_MM.len() - 1] {
        column_x += width;
        out.sheet_lines.push(DrawingPolyline {
            points_mm: vec![[column_x, top], [column_x, bottom]],
            hidden: false,
        });
    }
    let rows =
        std::iter::once(HEADERS.map(str::to_owned)).chain(drawing.releases.iter().map(|release| {
            let description = if release.description.chars().count() > DESCRIPTION_CHARS {
                let mut short: String = release
                    .description
                    .chars()
                    .take(DESCRIPTION_CHARS - 1)
                    .collect();
                short.push('…');
                short
            } else {
                release.description.clone()
            };
            // The approver in an APPROVED role, else the last sign-off.
            let approved = release
                .approvals
                .iter()
                .find(|a| a.role.eq_ignore_ascii_case("approved"))
                .or(release.approvals.last())
                .map_or_else(|| "—".to_owned(), |a| a.name.clone());
            [
                release.revision.clone(),
                description,
                release.date.clone(),
                approved,
            ]
        }));
    for (row, values) in rows.enumerate() {
        let mut column_x = x;
        for (value, width) in values.into_iter().zip(COLUMNS_MM) {
            out.sheet_labels.push(DrawingLabel {
                position_mm: [column_x + 1.5, top - ROW_MM * (row as f64 + 1.0) + 2.0],
                text: value,
                stack: None,
            });
            column_x += width;
        }
    }
}
