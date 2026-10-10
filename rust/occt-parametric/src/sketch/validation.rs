//! Structural and geometric validation of sketch entities, constraints, and profiles.

use super::*;

impl SketchDefinition {
    pub(super) fn validate_curve_geometry(
        &self,
        points: &HashMap<String, SketchPoint2>,
    ) -> Result<(), ModelError> {
        for (center, rim) in
            self.circles
                .iter()
                .map(|c| (&c.center, &c.rim))
                .chain(self.arcs.iter().flat_map(|a| {
                    [
                        (&a.center, &a.start),
                        (&a.center, &a.end),
                        (&a.start, &a.end),
                    ]
                }))
        {
            let distance = line_length((points[center], points[rim]));
            if !distance.is_finite() || distance <= RESIDUAL_TOLERANCE {
                return Err(ModelError::new(
                    "sketch curve has coincident or non-finite defining points",
                ));
            }
        }
        for ellipse in &self.ellipses {
            let c = points[&ellipse.center];
            let a = line_length((c, points[&ellipse.major]));
            let b = line_length((c, points[&ellipse.minor]));
            if !a.is_finite()
                || !b.is_finite()
                || b <= RESIDUAL_TOLERANCE
                || a + RESIDUAL_TOLERANCE < b
            {
                return Err(ModelError::new(
                    "ellipse needs positive radii with major >= minor",
                ));
            }
        }
        for spline in &self.splines {
            for pair in spline.points.windows(2) {
                let distance = line_length((points[&pair[0]], points[&pair[1]]));
                if !distance.is_finite() || distance <= RESIDUAL_TOLERANCE {
                    return Err(ModelError::new(format!(
                        "sketch spline '{}' has coincident consecutive points",
                        spline.id
                    )));
                }
            }
        }
        Ok(())
    }

    pub(crate) fn validate(
        &self,
        parameters: &HashMap<String, ParameterValue>,
    ) -> Result<(), ModelError> {
        let (point_ids, line_ids) = self.validate_structure()?;
        for constraint in &self.constraints {
            constraint.validate(&point_ids, &line_ids, parameters)?;
        }
        for op in &self.profile_operations {
            match op {
                SketchProfileOperation::Trim { first, last, .. } => {
                    let a = scalar(first, parameters, Dimension::Scalar)?;
                    let b = scalar(last, parameters, Dimension::Scalar)?;
                    if a < 0.0 || b > 1.0 || a >= b {
                        return Err(ModelError::new(
                            "trim fractions need 0 <= first < last <= 1",
                        ));
                    }
                }
                SketchProfileOperation::Extend { start, end, .. } => {
                    let a = scalar(start, parameters, Dimension::Length)?;
                    let b = scalar(end, parameters, Dimension::Length)?;
                    if a < 0.0 || b < 0.0 || a + b <= 0.0 {
                        return Err(ModelError::new(
                            "extension needs nonnegative distances, at least one positive",
                        ));
                    }
                }
                SketchProfileOperation::Offset { distance, .. } => {
                    if scalar(distance, parameters, Dimension::Length)?.abs() <= RESIDUAL_TOLERANCE
                    {
                        return Err(ModelError::new("profile offset must be nonzero"));
                    }
                }
            }
        }
        Ok(())
    }

    pub(crate) fn validate_structure(&self) -> Result<(HashSet<&str>, HashSet<&str>), ModelError> {
        if self.id.is_empty() {
            return Err(ModelError::new("sketch id must be nonempty"));
        }
        let point_ids = self.validate_points()?;
        let line_ids = self.validate_lines(&point_ids)?;
        let entity_ids = self.validate_curves(&point_ids, &line_ids)?;
        self.validate_profile(&entity_ids)?;
        if self.profile_operations.len() > 1024 {
            return Err(ModelError::new("sketch profile operations exceed 1024"));
        }
        let mut offset = false;
        for op in &self.profile_operations {
            match op {
                SketchProfileOperation::Offset { .. } => offset = true,
                _ if offset => {
                    return Err(ModelError::new(
                        "entity trim/extend operations must precede whole-profile offsets",
                    ));
                }
                _ => {}
            }
        }
        for op in &self.profile_operations {
            match op {
                SketchProfileOperation::Trim { entity, .. }
                | SketchProfileOperation::Extend { entity, .. }
                    if !entity_ids.contains(entity.as_str()) =>
                {
                    return Err(ModelError::new(
                        "profile operation refers to an unknown entity",
                    ));
                }
                _ => {}
            }
        }
        let entities = self.entities();
        for constraint in &self.constraints {
            constraint.validate_references(&point_ids, &line_ids)?;
            validate_tangency(constraint, &entities)?;
            match constraint {
                SketchConstraint::Radius { curve, .. }
                | SketchConstraint::Diameter { curve, .. }
                    if !matches!(
                        entities.get(curve.as_str()),
                        Some(Entity::Circle(_) | Entity::Arc(_))
                    ) =>
                {
                    return Err(ModelError::new(
                        "radius/diameter constraints require a circle or arc",
                    ));
                }
                SketchConstraint::EqualRadius { first, second } => {
                    if first == second
                        || [first, second].iter().any(|id| {
                            !matches!(
                                entities.get(id.as_str()),
                                Some(Entity::Circle(_) | Entity::Arc(_))
                            )
                        })
                    {
                        return Err(ModelError::new(
                            "equal-radius constraints require two distinct circles or arcs",
                        ));
                    }
                }
                SketchConstraint::PointOnCurve { curve, .. }
                    if !entities.contains_key(curve.as_str()) =>
                {
                    return Err(ModelError::new(format!("unknown sketch curve '{curve}'")));
                }
                _ => {}
            }
        }
        Ok((point_ids, line_ids))
    }
    fn validate_points(&self) -> Result<HashSet<&str>, ModelError> {
        let mut point_ids = HashSet::new();
        for point in &self.points {
            if point.id.is_empty() || !point_ids.insert(point.id.as_str()) {
                return Err(ModelError::new(
                    "sketch point ids must be nonempty and unique",
                ));
            }
        }
        Ok(point_ids)
    }
    fn validate_lines<'a>(
        &'a self,
        point_ids: &HashSet<&str>,
    ) -> Result<HashSet<&'a str>, ModelError> {
        let mut line_ids = HashSet::new();
        for line in &self.lines {
            if line.id.is_empty() || !line_ids.insert(line.id.as_str()) {
                return Err(ModelError::new(
                    "sketch line ids must be nonempty and unique",
                ));
            }
            if line.start == line.end
                || !point_ids.contains(line.start.as_str())
                || !point_ids.contains(line.end.as_str())
            {
                return Err(ModelError::new(format!(
                    "sketch line '{}' has invalid point references",
                    line.id
                )));
            }
        }
        Ok(line_ids)
    }
    fn validate_curves<'a>(
        &'a self,
        point_ids: &HashSet<&str>,
        line_ids: &HashSet<&'a str>,
    ) -> Result<HashSet<&'a str>, ModelError> {
        let mut entity_ids = line_ids.clone();
        for (id, refs) in self
            .circles
            .iter()
            .map(|c| (c.id.as_str(), vec![c.center.as_str(), c.rim.as_str()]))
            .chain(self.arcs.iter().map(|a| {
                (
                    a.id.as_str(),
                    vec![a.center.as_str(), a.start.as_str(), a.end.as_str()],
                )
            }))
        {
            if id.is_empty() || !entity_ids.insert(id) {
                return Err(ModelError::new(
                    "sketch entity ids must be nonempty and unique",
                ));
            }
            let unique = refs.iter().copied().collect::<HashSet<_>>();
            if unique.len() != refs.len() || refs.iter().any(|id| !point_ids.contains(id)) {
                return Err(ModelError::new(format!(
                    "sketch curve '{id}' has invalid point references"
                )));
            }
        }
        for ellipse in &self.ellipses {
            if ellipse.id.is_empty() || !entity_ids.insert(&ellipse.id) {
                return Err(ModelError::new(
                    "sketch entity ids must be nonempty and unique",
                ));
            }
            let refs = [
                ellipse.center.as_str(),
                ellipse.major.as_str(),
                ellipse.minor.as_str(),
            ];
            if refs.iter().any(|id| !point_ids.contains(id))
                || refs.into_iter().collect::<HashSet<_>>().len() != 3
            {
                return Err(ModelError::new(
                    "ellipse needs three distinct known defining points",
                ));
            }
        }
        for spline in &self.splines {
            if spline.id.is_empty() || !entity_ids.insert(spline.id.as_str()) {
                return Err(ModelError::new(
                    "sketch entity ids must be nonempty and unique",
                ));
            }
            validate_spline_points(spline, point_ids)?;
        }
        Ok(entity_ids)
    }
    fn validate_profile(&self, entity_ids: &HashSet<&str>) -> Result<(), ModelError> {
        let mut profile_ids = HashSet::new();
        for id in &self.profile {
            if !entity_ids.contains(id.as_str()) || !profile_ids.insert(id.as_str()) {
                return Err(ModelError::new(
                    "sketch profile has unknown or repeated entities",
                ));
            }
        }
        Ok(())
    }
}

impl SketchConstraint {
    fn validate_references(
        &self,
        points: &HashSet<&str>,
        lines: &HashSet<&str>,
    ) -> Result<(), ModelError> {
        let point = |id: &str| {
            points
                .contains(id)
                .then_some(())
                .ok_or_else(|| ModelError::new(format!("unknown sketch point '{id}'")))
        };
        let line = |id: &str| {
            lines
                .contains(id)
                .then_some(())
                .ok_or_else(|| ModelError::new(format!("unknown sketch line '{id}'")))
        };
        match self {
            Self::Radius { .. } | Self::Diameter { .. } | Self::EqualRadius { .. } => Ok(()),
            Self::PointOnCurve { point: id, .. } | Self::Tangent { point: id, .. } => point(id),
            Self::Symmetric {
                first,
                second,
                axis,
            } => {
                point(first)?;
                point(second)?;
                line(axis)
            }
            Self::Coincident { first, second } | Self::Distance { first, second, .. } => {
                point(first)?;
                point(second)
            }
            Self::Horizontal { line: id } | Self::Vertical { line: id } => line(id),
            Self::Angle { first, second, .. }
            | Self::Parallel { first, second }
            | Self::Perpendicular { first, second }
            | Self::EqualLength { first, second } => {
                line(first)?;
                line(second)
            }
        }
    }

    fn validate(
        &self,
        points: &HashSet<&str>,
        lines: &HashSet<&str>,
        parameters: &HashMap<String, ParameterValue>,
    ) -> Result<(), ModelError> {
        self.validate_references(points, lines)?;
        match self {
            Self::Angle { value, .. } => {
                let angle = scalar(value, parameters, Dimension::Scalar)?;
                if angle.abs() > std::f64::consts::PI {
                    return Err(ModelError::new("sketch angle must be in [-pi, pi] radians"));
                }
                Ok(())
            }
            Self::Radius { value, .. } | Self::Diameter { value, .. } => {
                if scalar(value, parameters, Dimension::Length)? <= 0.0 {
                    return Err(ModelError::new("sketch radius/diameter must be positive"));
                }
                Ok(())
            }
            Self::Distance { value, .. } => {
                let value = scalar(value, parameters, Dimension::Length)?;
                if value >= 0.0 {
                    Ok(())
                } else {
                    Err(ModelError::new("sketch distance must be nonnegative"))
                }
            }
            _ => Ok(()),
        }
    }
}

pub(super) fn validate_open_endpoints(
    profile: &[Entity<'_>],
    solution: &SketchSolution,
) -> Result<(), ModelError> {
    let start = solution.points[profile[0].endpoints().0];
    let end = solution.points[profile[profile.len() - 1].endpoints().1];
    if line_length((start, end)) <= 1e-7 {
        return Err(ModelError::new(
            "open sketch profile must have distinct endpoints",
        ));
    }
    Ok(())
}

pub(super) fn validate_curve_entities(profile: &[Entity<'_>]) -> Result<(), ModelError> {
    if profile.is_empty()
        || profile
            .iter()
            .any(|entity| matches!(entity, Entity::Circle(_) | Entity::Ellipse(_)))
    {
        return Err(ModelError::new(
            "profile must contain connected lines, arcs, and splines, or one circle",
        ));
    }
    if profile.len() > 1
        && profile
            .iter()
            .any(|entity| matches!(entity, Entity::Spline(s) if s.closed()))
    {
        return Err(ModelError::new(
            "a closed spline must be the only entity in its profile",
        ));
    }
    Ok(())
}

fn validate_spline_points(
    spline: &SketchSpline,
    point_ids: &HashSet<&str>,
) -> Result<(), ModelError> {
    // A closed spline repeats only its first point, at the end.
    let open = if spline.closed() {
        &spline.points[..spline.points.len() - 1]
    } else {
        &spline.points[..]
    };
    let unique = open.iter().map(String::as_str).collect::<HashSet<_>>();
    let minimum = if spline.closed() { 3 } else { 2 };
    if open.len() < minimum
        || unique.len() != open.len()
        || open.iter().any(|id| !point_ids.contains(id.as_str()))
    {
        return Err(ModelError::new(format!(
            "sketch spline '{}' needs {minimum} or more distinct known points",
            spline.id
        )));
    }
    Ok(())
}

fn validate_spline_tangency(kinds: [Option<Entity<'_>>; 2]) -> Result<(), ModelError> {
    let spline = |entity: Option<Entity<'_>>| matches!(entity, Some(Entity::Spline(_)));
    if kinds.iter().any(|entity| spline(*entity)) {
        let other = if spline(kinds[0]) { kinds[1] } else { kinds[0] };
        if !matches!(other, Some(Entity::Line(_) | Entity::Arc(_))) {
            return Err(ModelError::new(
                "a spline can be tangent only to a line or an arc",
            ));
        }
        if kinds
            .iter()
            .any(|entity| matches!(entity, Some(Entity::Spline(s)) if s.closed()))
        {
            return Err(ModelError::new(
                "a closed spline has no free ends for tangency",
            ));
        }
    }
    Ok(())
}

fn validate_tangency(
    constraint: &SketchConstraint,
    entities: &HashMap<&str, Entity<'_>>,
) -> Result<(), ModelError> {
    if let SketchConstraint::Tangent {
        first,
        second,
        point,
    } = constraint
    {
        if first == second {
            return Err(ModelError::new("tangency requires two distinct entities"));
        }
        let kinds = [first, second].map(|id| entities.get(id.as_str()).copied());
        validate_spline_tangency(kinds)?;
        for id in [first, second] {
            let entity = entities
                .get(id.as_str())
                .ok_or_else(|| ModelError::new(format!("unknown sketch entity '{id}'")))?;
            let (start, end) = entity.endpoints();
            if point != start && point != end {
                return Err(ModelError::new(
                    "tangency point must be a shared endpoint or circle rim",
                ));
            }
        }
    }
    Ok(())
}
