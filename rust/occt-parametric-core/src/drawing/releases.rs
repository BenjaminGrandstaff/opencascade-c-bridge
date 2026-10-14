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
    /// Revision symbols marking where this release changed the drawing
    /// (schema 156).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changes: Vec<DrawingChangeMarker>,
}

/// A revision symbol in the field of the drawing: the release's revision
/// in a triangle (delta), optionally with a leader to the changed item.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DrawingChangeMarker {
    /// The symbol's centroid on the paper, in millimeters.
    pub position_mm: [f64; 2],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub leader: Option<DrawingChangeLeader>,
}

/// A leader from a revision symbol to a datum's projection in a view.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DrawingChangeLeader {
    pub view: String,
    /// A datum on an unsuppressed instance the view shows.
    pub anchor: DatumRef,
}

/// A revision table drawn with its top-left corner at `position_mm`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DrawingRevisionTable {
    pub position_mm: [f64; 2],
    /// Adds a ZONE column listing the sheet zones of each release's change
    /// markers (schema 156); the sheet must have zones.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub zone_column: bool,
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
const ZONE_MM: f64 = 24.0;
/// Characters that fit the description column at about 2 mm each.
const DESCRIPTION_CHARS: usize = 38;
/// Characters that fit the zone column at about 2 mm each.
const ZONE_CHARS: usize = 10;

/// The table's columns: an optional ZONE column, then the fixed four.
fn columns(table: &DrawingRevisionTable) -> Vec<(&'static str, f64)> {
    let zone = table.zone_column.then_some(("ZONE", ZONE_MM));
    zone.into_iter()
        .chain(HEADERS.into_iter().zip(COLUMNS_MM))
        .collect()
}

/// A revision symbol's triangle, apex up, around its centroid: the side
/// leaves room for the label at the lettering height.
fn symbol(marker: &DrawingChangeMarker, revision: &str, height: f64) -> [[f64; 2]; 3] {
    let side = (text_layout::width(revision, height) + 1.0 + 1.155 * (1.0 + height) + 0.5).max(8.0);
    let tall = side * 3f64.sqrt() / 2.0;
    let [x, y] = marker.position_mm;
    let base = y - tall / 3.0;
    [
        [x - side / 2.0, base],
        [x + side / 2.0, base],
        [x, base + tall],
    ]
}

/// Where the ray from the centroid toward `target` leaves the triangle.
fn rim(corners: &[[f64; 2]; 3], center: [f64; 2], target: [f64; 2]) -> [f64; 2] {
    let d = [target[0] - center[0], target[1] - center[1]];
    let mut nearest = f64::INFINITY;
    for i in 0..3 {
        let (a, b) = (corners[i], corners[(i + 1) % 3]);
        let e = [b[0] - a[0], b[1] - a[1]];
        let denominator = d[0] * e[1] - d[1] * e[0];
        if denominator.abs() < 1e-12 {
            continue;
        }
        let w = [a[0] - center[0], a[1] - center[1]];
        let t = (w[0] * e[1] - w[1] * e[0]) / denominator;
        let u = (w[0] * d[1] - w[1] * d[0]) / denominator;
        if t > 0.0 && (-1e-9..=1.0 + 1e-9).contains(&u) {
            nearest = nearest.min(t);
        }
    }
    [center[0] + nearest * d[0], center[1] + nearest * d[1]]
}

/// The paper point a change leader points at.
fn leader_target(
    leader: &DrawingChangeLeader,
    views: &HashMap<&str, &DrawingView>,
    graph: &InstanceGraph<'_>,
) -> Result<[f64; 2], ModelError> {
    let view = views
        .get(leader.view.as_str())
        .ok_or_else(|| ModelError::new("change leader references an unknown view"))?;
    if !view
        .outputs
        .iter()
        .any(|o| o.instance == leader.anchor.instance)
        || graph.is_suppressed(&leader.anchor.instance)
    {
        return Err(ModelError::new(
            "change leader anchor must be on an unsuppressed instance shown in its view",
        ));
    }
    view.paper(view.project(datum_origin(graph, &leader.anchor)?)?)
}

/// The distinct zones of a release's markers, in marker order, as the
/// ZONE column shows them.
fn release_zones(release: &DrawingRelease, grid: &DrawingZoneGrid) -> Result<String, ModelError> {
    let mut zones: Vec<String> = Vec::new();
    for marker in &release.changes {
        let zone = grid.zone_at(marker.position_mm).ok_or_else(|| {
            ModelError::new(format!(
                "a change marker of revision '{}' lies outside the sheet's zones",
                release.revision
            ))
        })?;
        if !zones.contains(&zone) {
            zones.push(zone);
        }
    }
    Ok(if zones.is_empty() {
        "—".to_owned()
    } else {
        zones.join(", ")
    })
}

/// Validates change markers and the zone column. O(markers).
pub fn validate_changes(
    drawing: &DrawingDefinition,
    views: &HashMap<&str, &DrawingView>,
    graph: &InstanceGraph<'_>,
) -> Result<(), ModelError> {
    let height = DrawingLettering::for_sheet(drawing.sheet.as_ref()).notes;
    let [width, tall] = drawing.effective_paper_size_mm();
    for release in &drawing.releases {
        for marker in &release.changes {
            let corners = symbol(marker, &release.revision, height);
            if !finite_pair(marker.position_mm)
                || corners
                    .iter()
                    .any(|[x, y]| !(0.0..=width).contains(x) || !(0.0..=tall).contains(y))
            {
                return Err(ModelError::new(format!(
                    "change markers of revision '{}' must lie on the paper",
                    release.revision
                )));
            }
            if let Some(leader) = &marker.leader {
                let target = leader_target(leader, views, graph)?;
                let reach = (corners[1][0] - corners[0][0]) / 3f64.sqrt() + 4.0;
                let [dx, dy] = [
                    target[0] - marker.position_mm[0],
                    target[1] - marker.position_mm[1],
                ];
                let distance = dx.hypot(dy);
                if !distance.is_finite() || distance < reach {
                    return Err(ModelError::new(format!(
                        "a change leader of revision '{}' must reach at least 4 mm past its symbol",
                        release.revision
                    )));
                }
            }
        }
    }
    if drawing
        .revision_table
        .as_ref()
        .is_some_and(|t| t.zone_column)
    {
        let grid = drawing
            .sheet
            .as_ref()
            .and_then(DrawingSheet::zone_grid)
            .ok_or_else(|| ModelError::new("a revision table zone column needs sheet zones"))?;
        for release in &drawing.releases {
            release_zones(release, &grid)?;
        }
    }
    Ok(())
}

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
pub fn validate(drawing: &DrawingDefinition) -> Result<(), ModelError> {
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
        let total: f64 = columns(table).iter().map(|(_, w)| w).sum();
        if x < 0.0 || bottom < 0.0 || x + total > width || top > height {
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
pub fn validate_model_links(
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

/// Outer rectangle 5, row separators 2 each, column separators 2 each;
/// each change symbol 4, and its leader 5.
pub fn vertex_count(drawing: &DrawingDefinition) -> usize {
    let table = drawing.revision_table.as_ref().map_or(0, |t| {
        5 + 2 * drawing.releases.len() + 2 * (columns(t).len() - 1)
    });
    drawing
        .releases
        .iter()
        .flat_map(|r| &r.changes)
        .map(|m| if m.leader.is_some() { 9 } else { 4 })
        .fold(table, usize::saturating_add)
}

/// Draws each release's revision symbols, with their leaders, as
/// annotation linework. O(markers).
pub fn append_changes(
    drawing: &DrawingDefinition,
    views: &HashMap<&str, &DrawingView>,
    graph: &InstanceGraph<'_>,
    out: &mut GeneratedDrawing,
) -> Result<(), ModelError> {
    let height = out.lettering.notes;
    for release in &drawing.releases {
        for marker in &release.changes {
            let corners = symbol(marker, &release.revision, height);
            gdt::line(out, vec![corners[0], corners[1], corners[2], corners[0]]);
            let width = text_layout::width(&release.revision, height);
            gdt::text(
                out,
                [marker.position_mm[0] - width / 2.0, corners[0][1] + 1.0],
                release.revision.clone(),
            );
            if let Some(leader) = &marker.leader {
                let target = leader_target(leader, views, graph)?;
                gdt::leader(
                    out,
                    target,
                    rim(&corners, marker.position_mm, target),
                    false,
                );
            }
        }
    }
    Ok(())
}

/// Draws the revision table with the sheet linework, oldest release first.
pub fn append(drawing: &DrawingDefinition, out: &mut GeneratedDrawing) {
    let Some(table) = &drawing.revision_table else {
        return;
    };
    let [x, top] = table.position_mm;
    let columns = columns(table);
    let width: f64 = columns.iter().map(|(_, w)| w).sum();
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
        thin: true,
    });
    for row in 1..rows {
        let y = top - ROW_MM * row as f64;
        out.sheet_lines.push(DrawingPolyline {
            points_mm: vec![[x, y], [x + width, y]],
            hidden: false,
            thin: true,
        });
    }
    let mut column_x = x;
    for (_, width) in &columns[..columns.len() - 1] {
        column_x += width;
        out.sheet_lines.push(DrawingPolyline {
            points_mm: vec![[column_x, top], [column_x, bottom]],
            hidden: false,
            thin: true,
        });
    }
    let grid = drawing.sheet.as_ref().and_then(DrawingSheet::zone_grid);
    let shorten = |text: String, limit: usize| {
        if text.chars().count() > limit {
            let mut short: String = text.chars().take(limit - 1).collect();
            short.push('…');
            short
        } else {
            text
        }
    };
    let headers: Vec<String> = columns.iter().map(|(h, _)| (*h).to_owned()).collect();
    let rows = std::iter::once(headers).chain(drawing.releases.iter().map(|release| {
        let description = shorten(release.description.clone(), DESCRIPTION_CHARS);
        // The approver in an APPROVED role, else the last sign-off.
        let approved = release
            .approvals
            .iter()
            .find(|a| a.role.eq_ignore_ascii_case("approved"))
            .or(release.approvals.last())
            .map_or_else(|| "—".to_owned(), |a| a.name.clone());
        // Validation has placed every marker in a zone.
        let zones = grid
            .as_ref()
            .filter(|_| table.zone_column)
            .map(|grid| shorten(release_zones(release, grid).unwrap_or_default(), ZONE_CHARS));
        zones
            .into_iter()
            .chain([
                release.revision.clone(),
                description,
                release.date.clone(),
                approved,
            ])
            .collect()
    }));
    for (row, values) in rows.enumerate() {
        let mut column_x = x;
        for (value, (_, width)) in values.into_iter().zip(columns.iter().copied()) {
            // Standard lettering is larger: cut cells to their measured width.
            let value = if out.lettering == DrawingLettering::default() {
                value
            } else {
                text_layout::fit(&value, out.lettering.notes, width - 3.0)
            };
            out.sheet_labels.push(DrawingLabel {
                position_mm: [column_x + 1.5, top - ROW_MM * (row as f64 + 1.0) + 2.0],
                text: value,
                stack: None,
                height_mm: out.lettering.notes,
            });
            column_x += width;
        }
    }
}
