//! Feature history as a commit timeline (schema 182): each feature is a
//! commit in declaration order. `head` checks out the timeline at a feature,
//! leaving later features inactive (rolled back), and `suppressed` reverts
//! features without deleting them. A suppressed modifier (a fillet, cut,
//! hole, pattern...) passes its input body through, so later features build
//! on the body before it, like replaying commits over a dropped one. A
//! suppressed feature that makes a new body (a primitive, sketch, extrusion,
//! sweep...) takes everything depending on it out of the build as well.

use super::*;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FeatureHistory {
    /// The feature the timeline is checked out at: features declared after it
    /// are rolled back. None builds every feature.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head: Option<String>,
    /// Features turned off without being deleted, in any order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suppressed: Vec<String>,
}

impl FeatureHistory {
    pub fn is_empty(&self) -> bool {
        self.head.is_none() && self.suppressed.is_empty()
    }
}

/// Where one feature stands in the timeline.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum FeatureState {
    Active,
    /// Suppressed; with `passes`, later features read that input instead.
    Suppressed {
        #[serde(skip_serializing_if = "Option::is_none")]
        passes: Option<String>,
    },
    /// Out of the build because a feature it depends on is.
    Blocked {
        by: String,
    },
    /// Declared after the checked-out head.
    RolledBack,
}

impl FeatureState {
    /// Whether the feature has an output under its own id in the build: an
    /// active feature, or a suppressed one passing its input through.
    pub fn produces(&self) -> bool {
        matches!(self, Self::Active | Self::Suppressed { passes: Some(_) })
    }
}

impl FeatureOperation {
    /// The body a modifier works on, which a suppressed modifier passes
    /// through; None for features that make a new body.
    pub fn primary_input(&self) -> Option<&str> {
        match self {
            Self::Rib { input, .. }
            | Self::Translate { input, .. }
            | Self::Mirror { input, .. }
            | Self::Scale { input, .. }
            | Self::Rotate { input, .. }
            | Self::Thread { input, .. }
            | Self::Hole { input, .. }
            | Self::CircularPattern { input, .. }
            | Self::LinearPattern { input, .. }
            | Self::Fillet { input, .. }
            | Self::VariableFillet { input, .. }
            | Self::Chamfer { input, .. }
            | Self::Unify { input, .. }
            | Self::Split { input, .. }
            | Self::Offset { input, .. }
            | Self::Hollow { input, .. }
            | Self::Draft { input, .. }
            | Self::RemoveFaces { input, .. }
            | Self::OffsetFaces { input, .. }
            | Self::MoveFaces { input, .. } => Some(input),
            Self::Fuse { left, .. } | Self::Common { left, .. } => Some(left),
            Self::Cut { object, .. } => Some(object),
            _ => None,
        }
    }
}

impl FamilyDefinition {
    /// Every feature's state in declaration order, in O(features + inputs).
    pub fn history_states(&self) -> Result<Vec<(&str, FeatureState)>, ModelError> {
        let history = &self.history;
        if history.is_empty() {
            return Ok(self
                .features
                .iter()
                .map(|f| (f.id.as_str(), FeatureState::Active))
                .collect());
        }
        let index_of: HashMap<&str, usize> = self
            .features
            .iter()
            .enumerate()
            .map(|(i, f)| (f.id.as_str(), i))
            .collect();
        let position = |id: &str| index_of.get(id).copied();
        let head = match &history.head {
            Some(id) => position(id).ok_or_else(|| {
                ModelError::new(format!("history head names unknown feature '{id}'"))
            })?,
            None => self.features.len().saturating_sub(1),
        };
        let mut suppressed = HashSet::new();
        for id in &history.suppressed {
            if position(id).is_none() {
                return Err(ModelError::new(format!(
                    "history suppresses unknown feature '{id}'"
                )));
            }
            if !suppressed.insert(id.as_str()) {
                return Err(ModelError::new(format!(
                    "history suppresses feature '{id}' twice"
                )));
            }
        }
        let references = reference_map(self);
        let datums = datum_index(&self.datums);
        let inputs: Vec<Vec<&str>> = self
            .features
            .iter()
            .map(|feature| dependencies_with_references(feature, &references, &datums))
            .collect();
        let mut states: Vec<FeatureState> = self
            .features
            .iter()
            .enumerate()
            .map(|(index, feature)| {
                if index > head {
                    FeatureState::RolledBack
                } else if suppressed.contains(feature.id.as_str()) {
                    FeatureState::Suppressed {
                        passes: feature.operation.primary_input().map(str::to_owned),
                    }
                } else {
                    FeatureState::Active
                }
            })
            .collect();
        // Propagate unavailable outputs once along each dependency edge.
        let mut users = vec![Vec::new(); states.len()];
        for (index, state) in states.iter().enumerate() {
            let needs = match state {
                FeatureState::Active => inputs[index].clone(),
                FeatureState::Suppressed {
                    passes: Some(input),
                } => vec![input.as_str()],
                _ => continue,
            };
            for input in needs {
                if let Some(&source) = index_of.get(input) {
                    users[source].push(index);
                }
            }
        }
        let mut queue: std::collections::VecDeque<_> = states
            .iter()
            .enumerate()
            .filter_map(|(i, s)| (!s.produces()).then_some(i))
            .collect();
        while let Some(source) = queue.pop_front() {
            for &user in &users[source] {
                if states[user].produces() {
                    states[user] = FeatureState::Blocked {
                        by: self.features[source].id.clone(),
                    };
                    queue.push_back(user);
                }
            }
        }
        Ok(self
            .features
            .iter()
            .map(|feature| feature.id.as_str())
            .zip(states)
            .collect())
    }

    /// Check out a feature, or restore the full timeline with None.
    pub fn checkout_feature(&mut self, head: Option<String>) -> Result<(), ModelError> {
        if let Some(id) = &head
            && !self.features.iter().any(|f| &f.id == id)
        {
            return Err(ModelError::new(format!("unknown feature '{id}'")));
        }
        self.history.head = head;
        Ok(())
    }

    /// Suppress or restore a feature without removing its declaration.
    pub fn set_feature_suppressed(
        &mut self,
        feature: &str,
        suppressed: bool,
    ) -> Result<(), ModelError> {
        if !self.features.iter().any(|f| f.id == feature) {
            return Err(ModelError::new(format!("unknown feature '{feature}'")));
        }
        if self.history.suppressed.iter().any(|id| id == feature) == suppressed {
            return Ok(());
        }
        self.history.suppressed.retain(|id| id != feature);
        if suppressed {
            self.history.suppressed.push(feature.to_owned());
        }
        Ok(())
    }

    /// Last feature with an available output at the current timeline position.
    pub fn history_output(&self) -> Result<Option<&str>, ModelError> {
        Ok(self
            .history_states()?
            .into_iter()
            .rev()
            .find(|(_, state)| state.produces())
            .map(|(id, _)| id))
    }

    /// Whether the history leaves `feature` out of the build entirely.
    pub fn history_excludes(&self, feature: &str) -> Result<bool, ModelError> {
        Ok(self
            .history_states()?
            .into_iter()
            .any(|(id, state)| id == feature && !state.produces()))
    }

    /// Moves `feature` to just after `after` (or first when None), checking
    /// that it still follows everything it uses and precedes everything
    /// that uses it, like reordering commits in an interactive rebase.
    pub fn move_feature(&mut self, feature: &str, after: Option<&str>) -> Result<(), ModelError> {
        let from = self
            .features
            .iter()
            .position(|f| f.id == feature)
            .ok_or_else(|| ModelError::new(format!("unknown feature '{feature}'")))?;
        if after == Some(feature) {
            return Err(ModelError::new("a feature cannot move after itself"));
        }
        let mut features = self.features.clone();
        let moved = features.remove(from);
        let to = match after {
            None => 0,
            Some(id) => {
                features
                    .iter()
                    .position(|f| f.id == id)
                    .ok_or_else(|| ModelError::new(format!("unknown feature '{id}'")))?
                    + 1
            }
        };
        features.insert(to, moved);
        let references = reference_map(self);
        let datums = datum_index(&self.datums);
        let order: HashMap<&str, usize> = features
            .iter()
            .enumerate()
            .map(|(index, f)| (f.id.as_str(), index))
            .collect();
        for (index, f) in features.iter().enumerate() {
            for input in dependencies_with_references(f, &references, &datums) {
                if let Some(&at) = order.get(input)
                    && at > index
                    && (f.id == feature || input == feature)
                {
                    return Err(ModelError::new(format!(
                        "moving '{feature}' would put '{}' before its input '{input}'",
                        f.id
                    )));
                }
            }
        }
        self.features = features;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feature(id: &str, operation: FeatureOperation) -> FeatureDefinition {
        FeatureDefinition {
            id: id.into(),
            operation,
        }
    }
    fn cube(id: &str) -> FeatureDefinition {
        let size = VectorExpr::Literal(VectorQuantity::lengths(
            10.0,
            10.0,
            10.0,
            LengthUnit::Millimeter,
        ));
        feature(
            id,
            FeatureOperation::Box {
                origin: size.clone(),
                size,
            },
        )
    }

    /// base → fillet → holes → shell, with `tool` cut from the base first.
    fn family() -> FamilyDefinition {
        let radius = ScalarExpr::Literal(Quantity::length(1.0, LengthUnit::Millimeter));
        let mut family = FamilyDefinition {
            id: "part".into(),
            version: 1,
            parameters: vec![],
            derived_parameters: vec![],
            derived_vector_parameters: vec![],
            constraints: vec![],
            features: vec![],
            requirements: vec![],
            datums: vec![],
            references: vec![],
            feature_colors: Default::default(),
            assumptions: vec![],
            history: FeatureHistory::default(),
        };
        family.features = vec![
            cube("base"),
            cube("tool"),
            feature(
                "notched",
                FeatureOperation::Cut {
                    object: "base".into(),
                    tool: "tool".into(),
                },
            ),
            feature(
                "rounded",
                FeatureOperation::Unify {
                    input: "notched".into(),
                    linear_tolerance: radius.clone(),
                    angular_tolerance: ScalarExpr::Literal(Quantity::scalar(0.01)),
                },
            ),
            feature(
                "moved",
                FeatureOperation::Translate {
                    input: "rounded".into(),
                    offset: VectorExpr::Literal(VectorQuantity::lengths(
                        1.0,
                        0.0,
                        0.0,
                        LengthUnit::Millimeter,
                    )),
                },
            ),
        ];
        family
    }

    fn states(family: &FamilyDefinition) -> Vec<(String, FeatureState)> {
        family
            .history_states()
            .unwrap()
            .into_iter()
            .map(|(id, state)| (id.to_owned(), state))
            .collect()
    }

    #[test]
    fn suppressed_modifiers_pass_their_input_and_new_bodies_block() {
        let mut family = family();
        assert!(
            states(&family)
                .iter()
                .all(|(_, s)| *s == FeatureState::Active)
        );
        family.history.suppressed = vec!["rounded".into()];
        let s = states(&family);
        assert_eq!(
            s[3].1,
            FeatureState::Suppressed {
                passes: Some("notched".into())
            }
        );
        assert_eq!(s[4].1, FeatureState::Active);
        // Suppressing the cutting tool, a new body, blocks the cut and
        // everything after it.
        family.history.suppressed = vec!["tool".into()];
        let s = states(&family);
        assert_eq!(s[1].1, FeatureState::Suppressed { passes: None });
        assert_eq!(s[2].1, FeatureState::Blocked { by: "tool".into() });
        assert_eq!(
            s[3].1,
            FeatureState::Blocked {
                by: "notched".into()
            }
        );
        assert_eq!(
            s[4].1,
            FeatureState::Blocked {
                by: "rounded".into()
            }
        );
        assert!(family.history_excludes("moved").unwrap());
    }

    #[test]
    fn head_rolls_back_later_features_and_names_are_checked() {
        let mut family = family();
        family.history.head = Some("notched".into());
        let s = states(&family);
        assert_eq!(s[2].1, FeatureState::Active);
        assert_eq!(s[3].1, FeatureState::RolledBack);
        assert_eq!(s[4].1, FeatureState::RolledBack);
        // A feature declared before the head that uses one after it is blocked.
        family.history.head = None;
        family.features.swap(0, 2);
        let s = states(&family);
        assert_eq!(s[0], ("notched".into(), FeatureState::Active));
        family.history.head = Some("tool".into());
        let s = states(&family);
        assert_eq!(s[0].1, FeatureState::Blocked { by: "base".into() });
        for history in [
            FeatureHistory {
                head: Some("nope".into()),
                suppressed: vec![],
            },
            FeatureHistory {
                head: None,
                suppressed: vec!["nope".into()],
            },
            FeatureHistory {
                head: None,
                suppressed: vec!["base".into(), "base".into()],
            },
        ] {
            family.history = history;
            assert!(family.history_states().is_err());
        }
    }

    #[test]
    fn moves_keep_inputs_before_their_users() {
        let mut family = family();
        // The tool can move first or just before the cut.
        family.move_feature("tool", None).unwrap();
        assert_eq!(family.features[0].id, "tool");
        family.move_feature("tool", Some("base")).unwrap();
        assert_eq!(family.features[1].id, "tool");
        // The cut cannot move before the base it cuts, nor the base after it.
        let error = family.move_feature("notched", None).unwrap_err().message;
        assert!(error.contains("before its input 'base'"), "{error}");
        assert!(family.move_feature("base", Some("moved")).is_err());
        assert!(family.move_feature("ghost", None).is_err());
        assert!(family.move_feature("base", Some("base")).is_err());
        let ids: Vec<_> = family.features.iter().map(|f| f.id.as_str()).collect();
        assert_eq!(ids, ["base", "tool", "notched", "rounded", "moved"]);
    }
}
