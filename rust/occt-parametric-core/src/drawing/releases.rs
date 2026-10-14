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
    /// SHA-256 of the drawing as this approval signed it (schema 158):
    /// 64 lowercase hex digits from
    /// [`DrawingDefinition::approval_digest`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
}

/// The sign-off roles every release collects, in order (schema 158). A
/// release is released once each role has signed; until then it is pending.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DrawingApprovalWorkflow {
    pub roles: Vec<String>,
}

/// Where a release stands in the approval workflow.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum DrawingReleaseState {
    Released,
    /// Awaiting the sign-off of `next_role`.
    Pending {
        next_role: String,
    },
}

/// Whether an approval's digest still matches the drawing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalDigestStatus {
    /// The drawing is as the approval signed it.
    Valid,
    /// The drawing changed after the approval signed it.
    Changed,
    /// The approval carries no digest.
    Unsigned,
}

/// One approval of the latest release, checked against the drawing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ApprovalVerification {
    pub revision: String,
    pub role: String,
    pub name: String,
    pub status: ApprovalDigestStatus,
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
    /// The latest release awaits `next_role` in the approval workflow;
    /// `released` is the latest released revision, if any.
    Pending {
        revision: String,
        next_role: String,
        released: Option<String>,
    },
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
        let mut signed_before = None;
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
            if let Some(digest) = &approval.digest
                && (digest.len() != 64
                    || !digest
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
            {
                return Err(ModelError::new(
                    "approval digests must be 64 lowercase hexadecimal digits",
                ));
            }
            // A workflow's sign-offs follow one another.
            if drawing.approval_workflow.is_some() && signed_before.is_some_and(|d| signed < d) {
                return Err(ModelError::new("workflow approvals must be in date order"));
            }
            signed_before = Some(signed);
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
    if let Some(workflow) = &drawing.approval_workflow {
        validate_workflow(drawing, workflow)?;
    }
    validate_sheet_revisions(drawing)?;
    // The title block shows the latest released revision.
    if let Some(sheet) = &drawing.sheet {
        match drawing.latest_released() {
            Some(latest) if sheet.revision != latest.revision => {
                return Err(ModelError::new(format!(
                    "title block revision '{}' must match the latest release '{}'",
                    sheet.revision, latest.revision
                )));
            }
            None if !drawing.releases.is_empty() && !sheet.revision.is_empty() => {
                return Err(ModelError::new(
                    "the title block revision must be empty until a release is approved",
                ));
            }
            _ => {}
        }
    }
    Ok(())
}

/// Each release's approvals follow the workflow's roles in order; every
/// release but the latest has all of them. O(releases × roles).
fn validate_workflow(
    drawing: &DrawingDefinition,
    workflow: &DrawingApprovalWorkflow,
) -> Result<(), ModelError> {
    if workflow.roles.is_empty() || workflow.roles.len() > 8 {
        return Err(ModelError::new("an approval workflow needs 1-8 roles"));
    }
    let mut roles = HashSet::new();
    for role in &workflow.roles {
        text_field(role, 40, "workflow role")?;
        if !roles.insert(role.as_str()) {
            return Err(ModelError::new("approval workflow roles must be unique"));
        }
    }
    let last = drawing.releases.len().saturating_sub(1);
    for (index, release) in drawing.releases.iter().enumerate() {
        let in_order = release.approvals.len() <= workflow.roles.len()
            && release
                .approvals
                .iter()
                .zip(&workflow.roles)
                .all(|(approval, role)| approval.role == *role);
        if !in_order {
            return Err(ModelError::new(format!(
                "release '{}' approvals must follow the workflow roles {} in order",
                release.revision,
                workflow.roles.join(", ")
            )));
        }
        if index < last && release.approvals.len() < workflow.roles.len() {
            return Err(ModelError::new(format!(
                "release '{}' is still pending; only the latest release may await approval",
                release.revision
            )));
        }
    }
    Ok(())
}

/// Writes `value` as JSON with object keys sorted, so equal content gives
/// equal bytes whatever map order serde_json was built with.
fn canonical(value: &serde_json::Value, out: &mut String) {
    match value {
        serde_json::Value::Object(map) => {
            let mut entries: Vec<_> = map.iter().collect();
            entries.sort_unstable_by(|a, b| a.0.cmp(b.0));
            out.push('{');
            for (i, (key, item)) in entries.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::Value::String(key.clone()).to_string());
                out.push(':');
                canonical(item, out);
            }
            out.push('}');
        }
        serde_json::Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                canonical(item, out);
            }
            out.push(']');
        }
        scalar => out.push_str(&scalar.to_string()),
    }
}

impl DrawingDefinition {
    /// Where release `index` stands: released without a workflow, or once
    /// every workflow role has signed it.
    pub fn release_state(&self, index: usize) -> Result<DrawingReleaseState, ModelError> {
        let release = self
            .releases
            .get(index)
            .ok_or_else(|| ModelError::new("unknown release index"))?;
        Ok(match &self.approval_workflow {
            Some(workflow) if release.approvals.len() < workflow.roles.len() => {
                DrawingReleaseState::Pending {
                    next_role: workflow.roles[release.approvals.len()].clone(),
                }
            }
            _ => DrawingReleaseState::Released,
        })
    }

    /// The latest release that has been released (all of them without a
    /// workflow; only the latest may be pending).
    pub fn latest_released(&self) -> Option<&DrawingRelease> {
        let last = self.releases.len().checked_sub(1)?;
        match self.release_state(last) {
            Ok(DrawingReleaseState::Released) => self.releases.last(),
            _ => last.checked_sub(1).map(|i| &self.releases[i]),
        }
    }

    /// The SHA-256 digest approval `approval` of release `release` signs:
    /// the drawing's canonical JSON with only the releases up to that one,
    /// its approvals up to this one, this approval's own digest left out,
    /// and the title-block revision (which follows the releases) blank. So
    /// each digest covers the sign-offs before it, and any later change to
    /// views, annotations, sheet, links or earlier approvals changes it.
    /// O(drawing size).
    pub fn approval_digest(&self, release: usize, approval: usize) -> Result<String, ModelError> {
        let mut signed = self.clone();
        let target = signed
            .releases
            .get_mut(release)
            .filter(|r| approval < r.approvals.len())
            .ok_or_else(|| ModelError::new("unknown release approval"))?;
        target.approvals.truncate(approval + 1);
        target.approvals[approval].digest = None;
        signed.releases.truncate(release + 1);
        if let Some(sheet) = &mut signed.sheet {
            sheet.revision.clear();
        }
        let value = serde_json::to_value(&signed)
            .map_err(|error| ModelError::new(format!("approval digest: {error}")))?;
        let mut text = String::new();
        canonical(&value, &mut text);
        Ok(crate::digest::sha256_hex(text.as_bytes()))
    }

    /// Signs the latest release as `role`, with this drawing's digest. With
    /// a workflow, `role` must be the next one awaited; the approval must be
    /// dated on or after the previous one and on or before the release. A
    /// release completed this way becomes the title-block revision.
    pub fn sign_release(&mut self, role: &str, name: &str, date: &str) -> Result<(), ModelError> {
        let index = self
            .releases
            .len()
            .checked_sub(1)
            .ok_or_else(|| ModelError::new("there is no release to sign"))?;
        if self.approval_workflow.is_some() {
            match self.release_state(index)? {
                DrawingReleaseState::Pending { next_role } if next_role == role => {}
                DrawingReleaseState::Pending { next_role } => {
                    return Err(ModelError::new(format!(
                        "release '{}' awaits '{next_role}', not '{role}'",
                        self.releases[index].revision
                    )));
                }
                DrawingReleaseState::Released => {
                    return Err(ModelError::new(format!(
                        "release '{}' is already released",
                        self.releases[index].revision
                    )));
                }
            }
        }
        let mut signed = self.clone();
        signed.releases[index].approvals.push(DrawingApproval {
            role: role.to_owned(),
            name: name.to_owned(),
            date: date.to_owned(),
            digest: None,
        });
        let approval = signed.releases[index].approvals.len() - 1;
        let digest = signed.approval_digest(index, approval)?;
        signed.releases[index].approvals[approval].digest = Some(digest);
        let latest = signed.latest_released().map(|r| r.revision.clone());
        if let (Some(sheet), Some(latest)) = (&mut signed.sheet, latest) {
            sheet.revision = latest;
        }
        validate(&signed)?;
        *self = signed;
        Ok(())
    }

    /// Checks each approval of the latest release against this drawing.
    /// Earlier releases signed earlier states of the drawing, so only the
    /// latest is checked. O(approvals × drawing size).
    pub fn verify_approvals(&self) -> Result<Vec<ApprovalVerification>, ModelError> {
        let Some(index) = self.releases.len().checked_sub(1) else {
            return Ok(Vec::new());
        };
        let release = &self.releases[index];
        release
            .approvals
            .iter()
            .enumerate()
            .map(|(i, approval)| {
                let status = match &approval.digest {
                    None => ApprovalDigestStatus::Unsigned,
                    Some(digest) if *digest == self.approval_digest(index, i)? => {
                        ApprovalDigestStatus::Valid
                    }
                    Some(_) => ApprovalDigestStatus::Changed,
                };
                Ok(ApprovalVerification {
                    revision: release.revision.clone(),
                    role: approval.role.clone(),
                    name: approval.name.clone(),
                    status,
                })
            })
            .collect()
    }
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
        if let DrawingReleaseState::Pending { next_role } =
            drawing.release_state(drawing.releases.len() - 1)?
        {
            return Ok(DrawingReleaseStatus::Pending {
                revision,
                next_role,
                released: drawing.latest_released().map(|r| r.revision.clone()),
            });
        }
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

const STATUS_COLUMNS_MM: [f64; 2] = [16.0, 16.0];

/// Sheet 1's status table position, when it has one.
fn status_table(drawing: &DrawingDefinition) -> Option<(&DrawingSheet, [f64; 2])> {
    let sheet = drawing.sheet.as_ref()?;
    Some((sheet, sheet.revisions.as_ref()?.status_table_mm?))
}

/// The sheet's own revision rules and status table placement.
/// O(sheets).
fn validate_sheet_revisions(drawing: &DrawingDefinition) -> Result<(), ModelError> {
    let Some(sheet) = &drawing.sheet else {
        return Ok(());
    };
    let Some(revisions) = &sheet.revisions else {
        return Ok(());
    };
    if sheet.sheet_count == 0 || sheet.sheet_count > 1_000 || sheet.sheet_number > sheet.sheet_count
    {
        return Err(ModelError::new(
            "a revised sheet needs 1-1000 sheets and a sheet number among them",
        ));
    }
    if let Some([x, top]) = revisions.status_table_mm {
        if revisions.method != SheetRevisionMethod::PerSheet || sheet.sheet_number != 1 {
            return Err(ModelError::new(
                "only sheet 1 of a per-sheet revised drawing has a status table",
            ));
        }
        let [width, height] = drawing.effective_paper_size_mm();
        let bottom = top - ROW_MM * (sheet.sheet_count as f64 + 1.0);
        if !finite_pair([x, top])
            || x < 0.0
            || bottom < 0.0
            || x + STATUS_COLUMNS_MM.iter().sum::<f64>() > width
            || top > height
        {
            return Err(ModelError::new("sheet status table must fit on the paper"));
        }
    }
    Ok(())
}

/// Checks the sheets of each multi-sheet drawing in `drawings` together:
/// every sheet sharing a drawing number states the same revision method
/// and sheet count, and all sheets are present, numbered once each. With
/// the same revision on every sheet, only sheet 1 records releases and a
/// revision table, and every other title block shows sheet 1's latest
/// released revision. O(drawings).
pub fn validate_sheet_sets(drawings: &[DrawingDefinition]) -> Result<(), ModelError> {
    for (number, set) in sheet_sets(drawings) {
        let first = set[0].sheet.as_ref().expect("sheet sets hold sheets");
        let (method, count) = (
            first.revisions.as_ref().map(|r| r.method),
            first.sheet_count,
        );
        let mut numbers = HashSet::new();
        for drawing in &set {
            let sheet = drawing.sheet.as_ref().expect("sheet sets hold sheets");
            if sheet.revisions.as_ref().map(|r| r.method) != method || sheet.sheet_count != count {
                return Err(ModelError::new(format!(
                    "the sheets of drawing '{number}' must state one revision method and sheet count"
                )));
            }
            if !numbers.insert(sheet.sheet_number) {
                return Err(ModelError::new(format!(
                    "drawing '{number}' repeats sheet {}",
                    sheet.sheet_number
                )));
            }
        }
        if numbers.len() != count as usize {
            return Err(ModelError::new(format!(
                "drawing '{number}' has {} of its {count} sheets",
                numbers.len()
            )));
        }
        if method == Some(SheetRevisionMethod::SameRevision) {
            let lead = set
                .iter()
                .find(|d| d.sheet.as_ref().is_some_and(|s| s.sheet_number == 1))
                .expect("sheet 1 is present");
            let revision = lead
                .latest_released()
                .map_or("", |release| release.revision.as_str());
            for drawing in &set {
                let sheet = drawing.sheet.as_ref().expect("sheet sets hold sheets");
                if sheet.sheet_number == 1 {
                    continue;
                }
                if !drawing.releases.is_empty() || drawing.revision_table.is_some() {
                    return Err(ModelError::new(format!(
                        "sheet {} of drawing '{number}' shares sheet 1's revision; only sheet 1 records releases",
                        sheet.sheet_number
                    )));
                }
                if sheet.revision != revision {
                    return Err(ModelError::new(format!(
                        "sheet {} of drawing '{number}' must show revision '{revision}'",
                        sheet.sheet_number
                    )));
                }
            }
        }
    }
    Ok(())
}

/// Drawings with revised sheets, grouped by drawing number in order of
/// first appearance.
fn sheet_sets(drawings: &[DrawingDefinition]) -> Vec<(&str, Vec<&DrawingDefinition>)> {
    let mut index: HashMap<&str, usize> = HashMap::new();
    let mut sets: Vec<(&str, Vec<&DrawingDefinition>)> = Vec::new();
    for drawing in drawings {
        let Some(sheet) = drawing.sheet.as_ref().filter(|s| s.revisions.is_some()) else {
            continue;
        };
        let number = sheet.drawing_number.as_str();
        match index.get(number) {
            Some(&i) => sets[i].1.push(drawing),
            None => {
                index.insert(number, sets.len());
                sets.push((number, vec![drawing]));
            }
        }
    }
    sets
}

/// Each sheet's current revision, by sheet number, for every drawing in
/// `drawings` with a status table; the table's sheets must all be in
/// `drawings`. A sheet's revision is its latest released one, or `—`.
/// O(drawings).
pub fn sheet_statuses(
    drawings: &[DrawingDefinition],
) -> Result<HashMap<String, Vec<(u32, String)>>, ModelError> {
    let mut statuses = HashMap::new();
    for (number, set) in sheet_sets(drawings) {
        let Some(lead) = set.iter().find(|d| status_table(d).is_some()) else {
            continue;
        };
        let count = lead.sheet.as_ref().map_or(0, |s| s.sheet_count) as usize;
        let mut rows: Vec<(u32, String)> = set
            .iter()
            .filter_map(|d| {
                let sheet = d.sheet.as_ref()?;
                Some((
                    sheet.sheet_number,
                    d.latest_released()
                        .map_or_else(|| "—".to_owned(), |r| r.revision.clone()),
                ))
            })
            .collect();
        rows.sort_unstable_by_key(|(sheet, _)| *sheet);
        rows.dedup_by_key(|(sheet, _)| *sheet);
        if rows.len() != count {
            return Err(ModelError::new(format!(
                "the sheet status table of '{}' needs all {count} sheets of drawing '{number}'",
                lead.id
            )));
        }
        statuses.insert(lead.id.clone(), rows);
    }
    Ok(statuses)
}

/// Draws sheet 1's SHEET / REV status table with the sheet linework.
pub fn append_sheet_status(
    drawing: &DrawingDefinition,
    rows: &[(u32, String)],
    out: &mut GeneratedDrawing,
) {
    let Some((_, [x, top])) = status_table(drawing) else {
        return;
    };
    let width: f64 = STATUS_COLUMNS_MM.iter().sum();
    let bottom = top - ROW_MM * (rows.len() + 1) as f64;
    let thin = |points_mm| DrawingPolyline {
        points_mm,
        hidden: false,
        thin: true,
    };
    out.sheet_lines.push(thin(vec![
        [x, top],
        [x + width, top],
        [x + width, bottom],
        [x, bottom],
        [x, top],
    ]));
    for row in 1..=rows.len() {
        let y = top - ROW_MM * row as f64;
        out.sheet_lines.push(thin(vec![[x, y], [x + width, y]]));
    }
    let middle = x + STATUS_COLUMNS_MM[0];
    out.sheet_lines
        .push(thin(vec![[middle, top], [middle, bottom]]));
    let cells = std::iter::once(["SHEET".to_owned(), "REV".to_owned()]).chain(
        rows.iter()
            .map(|(sheet, revision)| [sheet.to_string(), revision.clone()]),
    );
    for (row, values) in cells.enumerate() {
        for (column, value) in values.into_iter().enumerate() {
            out.sheet_labels.push(DrawingLabel {
                position_mm: [
                    x + 1.5 + STATUS_COLUMNS_MM[0] * column as f64,
                    top - ROW_MM * (row as f64 + 1.0) + 2.0,
                ],
                text: value,
                stack: None,
                height_mm: out.lettering.notes,
            });
        }
    }
}

/// Outer rectangle 5, row separators 2 each, column separators 2 each;
/// each change symbol 4, and its leader 5; a sheet status table likewise.
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
        .saturating_add(
            status_table(drawing).map_or(0, |(sheet, _)| 5 + 2 * sheet.sheet_count as usize + 2),
        )
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
    let rows = std::iter::once(headers).chain(drawing.releases.iter().enumerate().map(
        |(index, release)| {
            let description = shorten(release.description.clone(), DESCRIPTION_CHARS);
            // The approver in an APPROVED role, else the last sign-off; a
            // release still awaiting a workflow sign-off is PENDING.
            let approved = match drawing.release_state(index) {
                Ok(DrawingReleaseState::Pending { .. }) => "PENDING".to_owned(),
                _ => release
                    .approvals
                    .iter()
                    .find(|a| a.role.eq_ignore_ascii_case("approved"))
                    .or(release.approvals.last())
                    .map_or_else(|| "—".to_owned(), |a| a.name.clone()),
            };
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
        },
    ));
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
