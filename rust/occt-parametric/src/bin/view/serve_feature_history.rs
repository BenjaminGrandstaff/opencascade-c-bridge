//! Feature timeline edits share the studio's guarded acceptance and undo stack.
use super::*;
use occt_parametric::FeatureHistory;

impl Studio {
    pub(super) fn feature_history_edit(&mut self, body: &[u8]) -> Result<Value, String> {
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "snake_case")]
        enum Action {
            Checkout,
            Suppress,
            Move,
            Bookmark,
            DeleteBookmark,
            CheckoutBookmark,
            CreateBranch,
            SwitchBranch,
            DeleteBranch,
            MergeBranch,
        }
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Request {
            version: u64,
            expected: FeatureHistory,
            expected_order: Vec<String>,
            action: Action,
            #[serde(default)]
            head: Option<String>,
            #[serde(default)]
            feature: Option<String>,
            #[serde(default)]
            after: Option<String>,
            #[serde(default)]
            suppressed: Option<bool>,
            /// Bookmark or branch name (schema 185).
            #[serde(default)]
            name: Option<String>,
            /// The feature a new branch continues after; none branches from
            /// the start.
            #[serde(default)]
            base: Option<String>,
        }
        let request: Request = serde_json::from_slice(body)
            .map_err(|e| format!("invalid feature history edit: {e}"))?;
        let original = &self.document.family;
        if request.version != self.version
            || request.expected != original.history
            || request.expected_order
                != original
                    .features
                    .iter()
                    .map(|f| f.id.clone())
                    .collect::<Vec<_>>()
        {
            return Err("The feature history changed. Refresh before editing it.".into());
        }
        let mut document = self.document.clone();
        let family = &mut document.family;
        // Checkout-like edits show the last available output.
        let checkout = matches!(
            request.action,
            Action::Checkout | Action::CheckoutBookmark | Action::SwitchBranch
        );
        let name = || request.name.as_deref().ok_or("Name is required.");
        match request.action {
            Action::Bookmark => family.bookmark_timeline(name()?),
            Action::DeleteBookmark => family.delete_bookmark(name()?),
            Action::CheckoutBookmark => family.checkout_bookmark(name()?),
            Action::CreateBranch => family.create_branch(name()?, request.base.as_deref()),
            Action::SwitchBranch => family.switch_branch(name()?),
            Action::DeleteBranch => family.delete_branch(name()?),
            Action::MergeBranch => match family.merge_branch(name()?).map_err(|e| e.to_string())? {
                Ok(_) => Ok(()),
                Err(conflicts) => {
                    return Err(format!(
                        "Branch '{}' defines {} differently; resolve them on one branch first.",
                        name()?,
                        conflicts
                            .iter()
                            .map(|c| format!("'{}'", c.feature))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }
            },
            Action::Checkout => family.checkout_feature(request.head),
            Action::Suppress => family.set_feature_suppressed(
                request.feature.as_deref().ok_or("Feature is required.")?,
                request.suppressed.ok_or("Suppressed is required.")?,
            ),
            Action::Move => family.move_feature(
                request.feature.as_deref().ok_or("Feature is required.")?,
                request.after.as_deref(),
            ),
        }
        .map_err(|e| e.to_string())?;
        if family == original {
            return Err("Feature history edit makes no changes.".into());
        }
        // Keep the selected result when possible. Otherwise show the last
        // available feature at the checked-out position, never a missing body.
        let states = family.history_states().map_err(|e| e.to_string())?;
        let output = self
            .output
            .as_deref()
            .filter(|_| !checkout)
            .filter(|output| {
                states
                    .iter()
                    .any(|(id, state)| id == output && state.produces())
            })
            .or_else(|| {
                states
                    .iter()
                    .rev()
                    .find(|(_, state)| state.produces())
                    .map(|(id, _)| *id)
            })
            .ok_or(
                "No feature output remains available. Restore a body before applying this edit.",
            )?
            .to_owned();
        family.version = family
            .version
            .checked_add(1)
            .ok_or("Family version overflow.")?;
        let (gltf, requirements) = build(&document, Some(&output))?;
        self.accept(document, gltf, requirements, Some(output))?;
        Ok(self.feature_view())
    }
}
