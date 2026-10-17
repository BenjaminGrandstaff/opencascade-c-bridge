//! Bounded correspondence checks using the same exact alignment as construction.
use super::*;
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct LoftProfileIssue {
    pub code: &'static str,
    pub message: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct LoftProfileCheck {
    pub profile: String,
    pub station: usize,
    pub hole: Option<usize>,
    pub source_edges: Option<usize>,
    pub aligned_edges: Option<usize>,
    pub start_mm: Option<[f64; 3]>,
    pub reverse: bool,
    pub issues: Vec<LoftProfileIssue>,
}
#[derive(Clone, Debug, Serialize)]
pub struct LoftProfileReport {
    pub constrained: bool,
    pub checks_passed: bool,
    /// These checks do not construct or certify the resulting solid.
    pub solid_checked: bool,
    pub profiles: Vec<LoftProfileCheck>,
    pub issues: Vec<LoftProfileIssue>,
}
fn issue(code: &'static str, message: impl Into<String>) -> LoftProfileIssue {
    LoftProfileIssue {
        code,
        message: message.into(),
    }
}

/// Inspect an authored or draft loft by building only its source dependencies.
/// Unrelated/downstream geometry failures and requirements are outside this report's scope.
pub fn diagnose_loft_profiles(
    part: &PartInstance<'_>,
    session: &Session,
    operation: &FeatureOperation,
) -> Result<LoftProfileReport, ModelError> {
    let FeatureOperation::ProfileLoft {
        profiles,
        holes,
        start_constraint,
        end_constraint,
        ..
    } = operation
    else {
        return Err(ModelError::new("profile diagnosis requires a profile loft"));
    };
    if profiles.len() + holes.iter().map(Vec::len).sum::<usize>() > 8192 {
        return Ok(LoftProfileReport {
            constrained: start_constraint.is_some() || end_constraint.is_some(),
            checks_passed: false,
            solid_checked: false,
            profiles: Vec::new(),
            issues: vec![issue(
                "inspection_budget",
                "Profile diagnosis supports at most 8192 boundaries.",
            )],
        });
    }
    let inputs = part.definition.feature_inputs()?;
    let mut ids = HashSet::new();
    let mut stack = operation.dependencies();
    while let Some(id) = stack.pop() {
        if ids.insert(id)
            && let Some(dependencies) = inputs.get(id)
        {
            stack.extend(dependencies.iter().copied());
        }
    }
    let mut family = part.definition.clone();
    family.features.retain(|f| ids.contains(f.id.as_str()));
    let names = family
        .features
        .iter()
        .flat_map(|f| f.operation.named_reference_names())
        .map(str::to_owned)
        .collect::<HashSet<_>>();
    family.references.retain(|r| names.contains(&r.name));
    family
        .feature_colors
        .retain(|id, _| ids.contains(id.as_str()));
    family.requirements.clear();
    let source = PartInstance {
        id: part.id.clone(),
        definition: &family,
        overrides: part.overrides.clone(),
        provenance: part.provenance.clone(),
    };
    let parameters = part.resolved_parameters()?;
    let generated = source.regenerate(session)?;
    generated.loft_profile_report(session, operation, &parameters)
}
impl GeneratedResult<'_> {
    /// Check source boundaries, exact partitions, starts, edge counts and end modes.
    /// Matching counts do not prove correspondence, twist freedom or solid validity.
    pub fn loft_profile_report(
        &self,
        session: &Session,
        operation: &FeatureOperation,
        parameters: &HashMap<String, ParameterValue>,
    ) -> Result<LoftProfileReport, ModelError> {
        let FeatureOperation::ProfileLoft {
            profiles,
            holes,
            ruled,
            start_constraint,
            end_constraint,
            correspondence,
        } = operation
        else {
            return Err(ModelError::new("profile diagnosis requires a profile loft"));
        };
        let constrained = start_constraint.is_some() || end_constraint.is_some();
        let mut report = LoftProfileReport {
            constrained,
            checks_passed: false,
            solid_checked: false,
            profiles: Vec::new(),
            issues: Vec::new(),
        };
        if !(2..=if constrained { 64 } else { 1000 }).contains(&profiles.len())
            || holes.len() > 100
            || holes.iter().any(|h| h.len() != profiles.len())
        {
            report.issues.push(issue("station_tracks","Choose 2–64 constrained (2–1000 free) stations and at most 100 hole tracks with matching station counts."));
        }
        let tracks = std::iter::once(profiles)
            .chain(holes.iter())
            .collect::<Vec<_>>();
        if tracks.iter().map(|t| t.len()).sum::<usize>() > 8192 {
            report.issues.push(issue(
                "inspection_budget",
                "Profile diagnosis supports at most 8192 boundaries.",
            ));
            return Ok(report);
        }
        let mut seen = HashSet::new();
        for id in tracks.iter().flat_map(|t| t.iter()) {
            if !seen.insert(id) {
                report.issues.push(issue(
                    "duplicate_profile",
                    format!("Profile '{id}' occurs in more than one station or track."),
                ));
            }
        }
        if constrained && *ruled || !correspondence.is_empty() && (!constrained || *ruled) {
            report.issues.push(issue(
                "loft_mode",
                "End constraints and explicit correspondence require a smooth constrained loft.",
            ));
        }
        for id in correspondence.keys().filter(|id| !seen.contains(id)) {
            report.issues.push(issue(
                "unknown_alignment",
                format!("Alignment '{id}' is outside the loft station tracks."),
            ));
        }
        for (track, ids) in tracks.iter().enumerate() {
            let first = report.profiles.len();
            for (station, id) in ids.iter().enumerate() {
                let alignment = correspondence.get(id);
                let mut check = LoftProfileCheck {
                    profile: (*id).clone(),
                    station,
                    hole: track.checked_sub(1),
                    source_edges: None,
                    aligned_edges: None,
                    start_mm: None,
                    reverse: alignment.is_some_and(|a| a.reverse),
                    issues: Vec::new(),
                };
                let mut inspect = || -> Result<(usize, [f64; 3]), ModelError> {
                    let source = shape(&self.shapes, id)?;
                    let extracted;
                    let wire = match session.shape_type(source)? {
                        ShapeType::Wire => source,
                        ShapeType::Face
                            if session.face_is_planar(source)?
                                && session.is_valid(source)?
                                && session.subshape_count(source, ShapeType::Wire)? == 1 =>
                        {
                            extracted = session.subshape(source, ShapeType::Wire, 0)?;
                            &extracted
                        }
                        _ => {
                            return Err(ModelError::new(
                                "Choose a single-boundary planar face or closed wire.",
                            ));
                        }
                    };
                    check.source_edges = Some(session.subshape_count(wire, ShapeType::Edge)?);
                    let values = alignment
                        .map(|a| std::collections::BTreeMap::from([(id.clone(), a.clone())]))
                        .unwrap_or_default();
                    let resolved = resolve_correspondence(&values, parameters)?;
                    let aligned = align_profile(session, wire, resolved.get(id))?;
                    let edge = session.subshape(&aligned, ShapeType::Edge, 0)?;
                    let p = session.curve_point_at_fraction(&edge, 0.)?;
                    Ok((
                        session.subshape_count(&aligned, ShapeType::Edge)?,
                        [p.x, p.y, p.z],
                    ))
                };
                match inspect() {
                    Ok((aligned, start)) => {
                        check.aligned_edges = Some(aligned);
                        check.start_mm = Some(start);
                    }
                    Err(e) => check.issues.push(issue("profile_alignment", e.message)),
                }
                report.profiles.push(check);
            }
            if constrained {
                let counts = report.profiles[first..]
                    .iter()
                    .filter_map(|p| p.aligned_edges)
                    .collect::<HashSet<_>>();
                if counts.len() > 1 {
                    let maximum = counts.iter().max().copied().unwrap_or(0);
                    for p in &mut report.profiles[first..] {
                        if let Some(n) = p.aligned_edges {
                            p.issues.push(issue("edge_count_mismatch",format!("Track has differing ordered edge counts; '{}' has {n}, maximum is {maximum}. Add exact splits or revise the profiles; matching counts still require matching traversal.",p.profile)));
                        }
                    }
                }
            }
        }
        for (end, constraint) in [
            (false, start_constraint.as_deref()),
            (true, end_constraint.as_deref()),
        ] {
            if constraint.is_none() {
                continue;
            }
            let check = || -> Result<(), ModelError> {
                if profiles.len() < 2 {
                    return Err(ModelError::new(
                        "End conditions require at least two stations.",
                    ));
                }
                if let Some(d) =
                    end_derivative(session, profiles, &self.shapes, parameters, constraint, end)?
                {
                    let id = &profiles[if end { profiles.len() - 1 } else { 0 }];
                    let (normal, _) = profile_frame(session, &self.shapes, id)?;
                    if dot(d, normal).abs() <= length(d) * 1e-8 {
                        return Err(ModelError::new("End tangent must cross its profile plane."));
                    }
                }
                Ok(())
            };
            if let Err(e) = check() {
                report.issues.push(issue(
                    if end {
                        "end_constraint"
                    } else {
                        "start_constraint"
                    },
                    e.message,
                ));
            }
        }
        report.checks_passed =
            report.issues.is_empty() && report.profiles.iter().all(|p| p.issues.is_empty());
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> serde_json::Value {
        serde_json::from_str(include_str!(
            "../../../../tools/model/partitioned-profile-loft.request.json"
        ))
        .unwrap()
    }
    fn report(request: &serde_json::Value) -> LoftProfileReport {
        let doc = ModelDocument::from_json(&request["model"].to_string()).unwrap();
        let graph = doc.instance_graph().unwrap();
        let part = graph.resolve(doc.instances[0].id()).unwrap();
        let session = Session::new().unwrap();
        let report =
            diagnose_loft_profiles(&part, &session, &doc.family.features[0].operation).unwrap();
        assert_eq!(session.shape_count().unwrap(), 0);
        report
    }
    #[test]
    fn repeated_inspection_releases_scratch_handles_and_rejects_oversized_drafts_early() {
        let data = fixture();
        let doc = ModelDocument::from_json(&data["model"].to_string()).unwrap();
        let session = Session::new().unwrap();
        let graph = doc.instance_graph().unwrap();
        let part = graph.resolve("block").unwrap();
        let built = part.regenerate(&session).unwrap();
        let count = session.shape_count().unwrap();
        let parameters = part.resolved_parameters().unwrap();
        for _ in 0..20 {
            assert!(
                built
                    .loft_profile_report(&session, &doc.family.features[0].operation, &parameters)
                    .unwrap()
                    .checks_passed
            );
            assert_eq!(session.shape_count().unwrap(), count);
        }
        let mut operation = doc.family.features[0].operation.clone();
        if let FeatureOperation::ProfileLoft { profiles, .. } = &mut operation {
            *profiles = vec!["lower".into(); 8193];
        }
        let report = diagnose_loft_profiles(&part, &session, &operation).unwrap();
        assert!(!report.checks_passed);
        assert!(report.profiles.is_empty());
        assert_eq!(report.issues[0].code, "inspection_budget");
        assert_eq!(session.shape_count().unwrap(), count);
        assert!(
            diagnose_loft_profiles(&part, &session, &doc.family.features[1].operation).is_err()
        );
    }
    #[test]
    fn reports_exact_partition_counts_and_starts_without_constructing_the_loft() {
        let data = fixture();
        let r = report(&data);
        assert!(r.checks_passed);
        assert!(!r.solid_checked);
        assert!(r.constrained);
        assert_eq!(r.profiles.len(), 2);
        assert_eq!(
            (r.profiles[0].source_edges, r.profiles[0].aligned_edges),
            (Some(1), Some(4))
        );
        assert_eq!(r.profiles[1].source_edges, Some(4));
        let start = r.profiles[0].start_mm.unwrap();
        assert!((start[1] - 8.).abs() < 1e-7);
        assert!(start[0].abs() < 1e-7);
        let hollow = report(
            &serde_json::from_str(include_str!(
                "../../../../tools/model/hollow-loft.request.json"
            ))
            .unwrap(),
        );
        assert!(hollow.checks_passed);
        assert!(!hollow.constrained);
        assert_eq!(hollow.profiles.len(), 4);
        assert_eq!(
            (hollow.profiles[2].hole, hollow.profiles[2].station),
            (Some(0), 0)
        );
        assert_eq!(
            (hollow.profiles[3].hole, hollow.profiles[3].station),
            (Some(0), 1)
        );
    }
    #[test]
    fn failed_lofts_still_report_each_mismatched_boundary_and_release_handles() {
        let mut data = fixture();
        data["model"]["family"]["features"][0]["operation"]["profile_loft"]["correspondence"] =
            serde_json::json!({});
        let doc = ModelDocument::from_json(&data["model"].to_string()).unwrap();
        let session = Session::new().unwrap();
        assert!(
            doc.instance_graph()
                .unwrap()
                .resolve("block")
                .unwrap()
                .regenerate(&session)
                .is_err()
        );
        assert_eq!(session.shape_count().unwrap(), 0);
        let r = report(&data);
        assert!(!r.checks_passed);
        assert_eq!(r.profiles[0].aligned_edges, Some(1));
        assert_eq!(r.profiles[1].aligned_edges, Some(4));
        assert!(
            r.profiles
                .iter()
                .all(|p| p.issues.iter().any(|i| i.code == "edge_count_mismatch"))
        );
    }
    #[test]
    fn isolates_alignment_failures_from_other_profiles_and_checks_end_modes() {
        for mode in [
            "anchor",
            "split",
            "direction",
            "ruled",
            "unknown",
            "duplicate",
            "hole-track",
        ] {
            let mut data = fixture();
            let loft = &mut data["model"]["family"]["features"][0]["operation"]["profile_loft"];
            match mode {
                "anchor" => {
                    loft["correspondence"]["lower"]["start_point"] = serde_json::json!({"literal":{"x":{"value":999.,"dimension":"length","unit":"millimeter"},"y":{"value":0.,"dimension":"length","unit":"millimeter"},"z":{"value":0.,"dimension":"length","unit":"millimeter"}}})
                }
                "split" => {
                    loft["correspondence"]["lower"]["split_points"] =
                        serde_json::json!([{ "parameter":"absent"}])
                }
                "direction" => {
                    loft["start_constraint"] = serde_json::json!({"direction":{"literal":{"x":{"value":1.,"dimension":"scalar","unit":null},"y":{"value":0.,"dimension":"scalar","unit":null},"z":{"value":0.,"dimension":"scalar","unit":null}}},"magnitude":{"parameter":"height"}});
                }
                "ruled" => loft["ruled"] = serde_json::json!(true),
                "unknown" => {
                    loft["correspondence"]["outside"] = serde_json::json!({"reverse":true})
                }
                "duplicate" => loft["profiles"] = serde_json::json!(["lower", "lower"]),
                _ => loft["holes"] = serde_json::json!([["upper"]]),
            }
            let r = report(&data);
            assert!(!r.checks_passed, "{mode}");
            if ["anchor", "split"].contains(&mode) {
                assert_eq!(r.profiles[0].source_edges, Some(1));
                assert!(!r.profiles[0].issues.is_empty());
                assert!(r.profiles[1].issues.is_empty());
            }
        }
    }
}
