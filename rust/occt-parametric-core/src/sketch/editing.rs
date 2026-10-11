//! Named native curve edits and seed-selected material boundary edits.
use super::*;

impl SketchDefinition {
    pub fn validate_edit_references<'a>(
        &'a self,
        originals: &HashSet<&'a str>,
        points: &HashSet<&str>,
    ) -> Result<HashSet<&'a str>, ModelError> {
        if self.curve_operations.len() > 1024
            || self.region_edits.len() > 1000
            || self
                .region_edits
                .iter()
                .map(|edit| edit.operations.len())
                .sum::<usize>()
                > 1024
        {
            return Err(ModelError::new(
                "sketch edits exceed 1024 operations or 1000 region selections",
            ));
        }
        let mut known = originals.clone();
        let mut active = originals.clone();
        for operation in &self.curve_operations {
            match operation {
                SketchCurveOperation::Split {
                    entity,
                    at,
                    before,
                    after,
                } => {
                    if !active.remove(entity.as_str()) {
                        return Err(ModelError::new("split needs an active known curve"));
                    }
                    match at {
                        SketchTrimBound::Point { point } if points.contains(point.as_str()) => {}
                        SketchTrimBound::Crossing { curve, near }
                            if known.contains(curve.as_str())
                                && curve != entity
                                && points.contains(near.as_str()) => {}
                        _ => {
                            return Err(ModelError::new(
                                "split needs an interior known point or another known curve and a hint point",
                            ));
                        }
                    }
                    for id in [before, after] {
                        if id.is_empty() || !known.insert(id.as_str()) {
                            return Err(ModelError::new(
                                "split piece names must be nonempty, unique and unused",
                            ));
                        }
                        active.insert(id.as_str());
                    }
                }
                SketchCurveOperation::ExtendTo {
                    entity,
                    target,
                    near,
                    ..
                } => {
                    if !active.contains(entity.as_str())
                        || !known.contains(target.as_str())
                        || entity == target
                        || near
                            .as_ref()
                            .is_some_and(|id| !points.contains(id.as_str()))
                    {
                        return Err(ModelError::new(
                            "extend-to needs an active curve, a different known target and a known optional hint",
                        ));
                    }
                }
            }
        }
        for edit in &self.region_edits {
            if !points.contains(edit.seed.as_str()) || edit.operations.is_empty() {
                return Err(ModelError::new(
                    "region edit needs a known seed point and nonempty operations",
                ));
            }
            let mut offset = false;
            let mut corners = HashSet::new();
            for operation in &edit.operations {
                match operation {
                    SketchProfileOperation::Offset { .. } => offset = true,
                    SketchProfileOperation::Fillet { point, .. }
                    | SketchProfileOperation::Chamfer { point, .. } => {
                        if offset || !points.contains(point.as_str()) || !corners.insert(point) {
                            return Err(ModelError::new(
                                "region corner edits need unique known points and must precede offsets",
                            ));
                        }
                    }
                    _ => {
                        return Err(ModelError::new(
                            "region edits support fillet, chamfer and offset only",
                        ));
                    }
                }
            }
        }
        Ok(known)
    }
}
