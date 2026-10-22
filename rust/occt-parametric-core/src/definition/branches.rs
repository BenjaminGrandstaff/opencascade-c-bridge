//! Timeline bookmarks and branches (schema 185), like git tags and
//! branches over the feature timeline.
//!
//! A bookmark names a timeline state: a checkout head and suppressions. A
//! branch is an alternative continuation of the timeline: "after feature
//! `base`, the features go on like this instead". The active branch is the
//! family's own feature list; each stored branch keeps only the features
//! after its base, with its own head, suppressions and bookmarks. Switching
//! swaps the active tail with the stored one, so features before the base
//! are shared and edits to them reach every branch. Merging appends the
//! branch's features the active branch lacks.

use super::*;

/// Most bookmarks per branch, stored branches per family, and features per
/// stored branch.
pub const MAX_BOOKMARKS: usize = 1000;
pub const MAX_BRANCHES: usize = 100;
pub const MAX_BRANCH_FEATURES: usize = 10_000;
/// Name of the active branch when none is named.
pub const MAIN_BRANCH: &str = "main";

/// A named timeline state.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TimelineBookmark {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suppressed: Vec<String>,
}

/// A stored, inactive branch.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TimelineBranch {
    pub name: String,
    /// The last feature it shares with the active branch; none when it
    /// diverges before the first feature.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    /// Its features after `base`, in timeline order.
    #[serde(default)]
    #[schemars(length(max = 10000))]
    pub features: Vec<FeatureDefinition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suppressed: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bookmarks: Vec<TimelineBookmark>,
}

/// A feature both branches define differently, which a merge cannot combine.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BranchConflict {
    pub feature: String,
}

fn check_name(name: &str, what: &str) -> Result<(), ModelError> {
    if name.is_empty()
        || name.len() > 64
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ' '))
        || name.trim() != name
    {
        return Err(ModelError::new(format!(
            "{what} name '{name}' needs 1 to 64 letters, digits, spaces, '-', '_' or '.', without surrounding spaces"
        )));
    }
    Ok(())
}

impl FeatureHistory {
    /// The active branch's name.
    pub fn branch_name(&self) -> &str {
        self.branch.as_deref().unwrap_or(MAIN_BRANCH)
    }
}

impl FamilyDefinition {
    /// Checks bookmarks and branches against the features. O(features +
    /// stored features + bookmarks).
    pub fn validate_timeline(&self) -> Result<(), ModelError> {
        let history = &self.history;
        let active: HashSet<&str> = self.features.iter().map(|f| f.id.as_str()).collect();
        validate_bookmarks(&history.bookmarks, &|id| active.contains(id), "")?;
        if let Some(name) = &history.branch {
            check_name(name, "branch")?;
        }
        if history.branches.len() > MAX_BRANCHES {
            return Err(ModelError::new(format!(
                "a family keeps at most {MAX_BRANCHES} stored branches"
            )));
        }
        let mut names = HashSet::from([history.branch_name()]);
        for branch in &history.branches {
            check_name(&branch.name, "branch")?;
            if !names.insert(branch.name.as_str()) {
                return Err(ModelError::new(format!(
                    "branch '{}' is named twice",
                    branch.name
                )));
            }
            let prefix = self.shared_prefix(branch)?;
            if branch.features.len() > MAX_BRANCH_FEATURES {
                return Err(ModelError::new(format!(
                    "branch '{}' holds more than {MAX_BRANCH_FEATURES} features",
                    branch.name
                )));
            }
            let mut ids: HashSet<&str> = prefix.iter().map(|f| f.id.as_str()).collect();
            for feature in &branch.features {
                if !ids.insert(feature.id.as_str()) {
                    return Err(ModelError::new(format!(
                        "branch '{}' declares feature '{}' twice or over a shared one",
                        branch.name, feature.id
                    )));
                }
            }
            let known = |id: &str| ids.contains(id);
            let context = format!("branch '{}' ", branch.name);
            if let Some(head) = &branch.head
                && !known(head)
            {
                return Err(ModelError::new(format!(
                    "{context}head names unknown feature '{head}'"
                )));
            }
            if let Some(id) = branch.suppressed.iter().find(|id| !known(id)) {
                return Err(ModelError::new(format!(
                    "{context}suppresses unknown feature '{id}'"
                )));
            }
            validate_bookmarks(&branch.bookmarks, &known, &context)?;
        }
        Ok(())
    }

    /// The active features a stored branch shares: those up to its base.
    fn shared_prefix(&self, branch: &TimelineBranch) -> Result<&[FeatureDefinition], ModelError> {
        match &branch.base {
            None => Ok(&self.features[..0]),
            Some(base) => {
                let at = self.features.iter().position(|f| &f.id == base).ok_or_else(|| {
                    ModelError::new(format!(
                        "branch '{}' continues after feature '{base}', which this branch no longer has; delete or merge the branch first",
                        branch.name
                    ))
                })?;
                Ok(&self.features[..=at])
            }
        }
    }

    /// Records the current head and suppressions under `name`.
    pub fn bookmark_timeline(&mut self, name: &str) -> Result<(), ModelError> {
        check_name(name, "bookmark")?;
        if self.history.bookmarks.iter().any(|b| b.name == name) {
            return Err(ModelError::new(format!("bookmark '{name}' already exists")));
        }
        if self.history.bookmarks.len() >= MAX_BOOKMARKS {
            return Err(ModelError::new(format!(
                "a branch keeps at most {MAX_BOOKMARKS} bookmarks"
            )));
        }
        self.history.bookmarks.push(TimelineBookmark {
            name: name.into(),
            head: self.history.head.clone(),
            suppressed: self.history.suppressed.clone(),
        });
        Ok(())
    }

    pub fn delete_bookmark(&mut self, name: &str) -> Result<(), ModelError> {
        let before = self.history.bookmarks.len();
        self.history.bookmarks.retain(|b| b.name != name);
        if self.history.bookmarks.len() == before {
            return Err(ModelError::new(format!("unknown bookmark '{name}'")));
        }
        Ok(())
    }

    /// Restores a bookmark's head and suppressions.
    pub fn checkout_bookmark(&mut self, name: &str) -> Result<(), ModelError> {
        let bookmark = self
            .history
            .bookmarks
            .iter()
            .find(|b| b.name == name)
            .cloned()
            .ok_or_else(|| ModelError::new(format!("unknown bookmark '{name}'")))?;
        self.history.head = bookmark.head;
        self.history.suppressed = bookmark.suppressed;
        Ok(())
    }

    /// Stores a new branch that continues after `base` (or from the start)
    /// exactly as the active branch does now, with its head and suppressions.
    pub fn create_branch(&mut self, name: &str, base: Option<&str>) -> Result<(), ModelError> {
        check_name(name, "branch")?;
        if name == self.history.branch_name()
            || self.history.branches.iter().any(|b| b.name == name)
        {
            return Err(ModelError::new(format!("branch '{name}' already exists")));
        }
        if self.history.branches.len() >= MAX_BRANCHES {
            return Err(ModelError::new(format!(
                "a family keeps at most {MAX_BRANCHES} stored branches"
            )));
        }
        let start = match base {
            None => 0,
            Some(id) => {
                self.features
                    .iter()
                    .position(|f| f.id == id)
                    .ok_or_else(|| ModelError::new(format!("unknown feature '{id}'")))?
                    + 1
            }
        };
        if self.features.len() - start > MAX_BRANCH_FEATURES {
            return Err(ModelError::new(format!(
                "a branch holds at most {MAX_BRANCH_FEATURES} features"
            )));
        }
        self.history.branches.push(TimelineBranch {
            name: name.into(),
            base: base.map(str::to_owned),
            features: self.features[start..].to_vec(),
            head: self.history.head.clone(),
            suppressed: self.history.suppressed.clone(),
            bookmarks: Vec::new(),
        });
        Ok(())
    }

    /// Makes stored branch `name` active: the active features after its base
    /// and the active head, suppressions and bookmarks are stored as a branch
    /// under the active name, and the branch's take their place.
    pub fn switch_branch(&mut self, name: &str) -> Result<(), ModelError> {
        let index = self
            .history
            .branches
            .iter()
            .position(|b| b.name == name)
            .ok_or_else(|| ModelError::new(format!("unknown branch '{name}'")))?;
        let start = self.shared_prefix(&self.history.branches[index])?.len();
        let target = self.history.branches.remove(index);
        let stored = TimelineBranch {
            name: self.history.branch_name().to_owned(),
            base: target.base.clone(),
            features: self.features.split_off(start),
            head: self.history.head.take(),
            suppressed: std::mem::take(&mut self.history.suppressed),
            bookmarks: std::mem::take(&mut self.history.bookmarks),
        };
        self.features.extend(target.features);
        self.history.head = target.head;
        self.history.suppressed = target.suppressed;
        self.history.bookmarks = target.bookmarks;
        self.history.branch = (target.name != MAIN_BRANCH).then_some(target.name);
        self.history.branches.insert(index, stored);
        Ok(())
    }

    pub fn delete_branch(&mut self, name: &str) -> Result<(), ModelError> {
        let before = self.history.branches.len();
        self.history.branches.retain(|b| b.name != name);
        if self.history.branches.len() == before {
            return Err(ModelError::new(format!("unknown branch '{name}'")));
        }
        Ok(())
    }

    /// Appends the stored branch's features the active branch lacks, in the
    /// branch's order, after the active features (their inputs all precede
    /// them). A feature both define differently is a conflict, and nothing
    /// changes. Returns the features added. The branch stays stored.
    pub fn merge_branch(
        &mut self,
        name: &str,
    ) -> Result<Result<Vec<String>, Vec<BranchConflict>>, ModelError> {
        let branch = self
            .history
            .branches
            .iter()
            .find(|b| b.name == name)
            .ok_or_else(|| ModelError::new(format!("unknown branch '{name}'")))?;
        self.shared_prefix(branch)?;
        let active: HashMap<&str, &FeatureDefinition> =
            self.features.iter().map(|f| (f.id.as_str(), f)).collect();
        let mut conflicts = Vec::new();
        let mut added = Vec::new();
        for feature in &branch.features {
            match active.get(feature.id.as_str()) {
                Some(existing) if *existing != feature => conflicts.push(BranchConflict {
                    feature: feature.id.clone(),
                }),
                Some(_) => {}
                None => added.push(feature.clone()),
            }
        }
        if !conflicts.is_empty() {
            return Ok(Err(conflicts));
        }
        let ids = added.iter().map(|f| f.id.clone()).collect();
        self.features.extend(added);
        Ok(Ok(ids))
    }
}

fn validate_bookmarks(
    bookmarks: &[TimelineBookmark],
    known: &dyn Fn(&str) -> bool,
    context: &str,
) -> Result<(), ModelError> {
    if bookmarks.len() > MAX_BOOKMARKS {
        return Err(ModelError::new(format!(
            "{context}keeps more than {MAX_BOOKMARKS} bookmarks"
        )));
    }
    let mut names = HashSet::new();
    for bookmark in bookmarks {
        check_name(&bookmark.name, "bookmark")?;
        if !names.insert(bookmark.name.as_str()) {
            return Err(ModelError::new(format!(
                "{context}bookmark '{}' is named twice",
                bookmark.name
            )));
        }
        let unknown = bookmark
            .head
            .iter()
            .chain(&bookmark.suppressed)
            .find(|id| !known(id));
        if let Some(id) = unknown {
            return Err(ModelError::new(format!(
                "{context}bookmark '{}' names unknown feature '{id}'",
                bookmark.name
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "branches/tests.rs"]
mod tests;
