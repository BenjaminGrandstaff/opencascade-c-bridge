//! Exact native planar regions, assembled from immutable simple profiles.
use super::*;
use occt_bridge::Bounds;
const CLEARANCE: f64 = 1e-7;

fn face<'a>(session: &'a Session, input: &Shape<'_>) -> Result<Shape<'a>, ModelError> {
    let result = match session.shape_type(input)? {
        ShapeType::Wire => session.create_face_from_wire(input)?,
        ShapeType::Face => session.duplicate(input)?,
        _ => {
            return Err(ModelError::new(
                "region boundaries must be closed planar wires or faces",
            ));
        }
    };
    let area = session.surface_area(&result)?;
    if !session.face_is_planar(&result)?
        || !session.is_valid(&result)?
        || session.subshape_count(&result, ShapeType::Wire)? != 1
        || !(area.is_finite() && area > 0.0)
    {
        return Err(ModelError::new(
            "region boundaries must each define one valid planar region without holes",
        ));
    }
    Ok(result)
}

pub(super) fn boxes_separated(a: &Bounds, b: &Bounds) -> bool {
    (a.max.x < b.min.x - CLEARANCE || b.max.x < a.min.x - CLEARANCE)
        || (a.max.y < b.min.y - CLEARANCE || b.max.y < a.min.y - CLEARANCE)
        || (a.max.z < b.min.z - CLEARANCE || b.max.z < a.min.z - CLEARANCE)
}

pub(super) fn execute<'a>(
    session: &'a Session,
    outer: &str,
    holes: &[String],
    shapes: &HashMap<String, Shape<'a>>,
) -> Result<Shape<'a>, ModelError> {
    if holes.is_empty()
        || holes.len() > 100
        || holes.iter().any(|h| h == outer)
        || holes.iter().collect::<HashSet<_>>().len() != holes.len()
    {
        return Err(ModelError::new(
            "planar region needs 1-100 distinct inner profiles, separate from its outer profile",
        ));
    }
    let outer_face =
        face(session, shape(shapes, outer)?).map_err(|e| e.context("outer region profile"))?;
    let outer_wire = session.subshape(&outer_face, ShapeType::Wire, 0)?;
    let outer_area = session.surface_area(&outer_face)?;
    let mut inner_faces = Vec::new();
    let mut bounds = Vec::new();
    let mut removed_area = 0.0;
    for id in holes {
        let hole = face(session, shape(shapes, id)?)
            .map_err(|e| e.context(&format!("inner profile '{id}'")))?;
        let area = session.surface_area(&hole)?;
        let common = session.common(&outer_face, &hole).map_err(|e| {
            ModelError::from(e).context(&format!(
                "inner profile '{id}' must be coplanar and contained"
            ))
        })?;
        let covered = session.surface_area(&common)?;
        if (covered - area).abs() > 1e-12_f64.max(area * 1e-7)
            || session.distance(&outer_wire, &hole)?.distance <= CLEARANCE
        {
            return Err(ModelError::new(format!(
                "inner profile '{id}' must be coplanar and strictly contained with boundary clearance"
            )));
        }
        let box_ = session.exact_bounds(&hole)?;
        for (previous, previous_bounds) in inner_faces.iter().zip(&bounds) {
            if !boxes_separated(&box_, previous_bounds)
                && session.distance(previous, &hole)?.distance <= CLEARANCE
            {
                return Err(ModelError::new(format!(
                    "inner profile '{id}' overlaps, contains or touches another hole"
                )));
            }
        }
        removed_area += area;
        bounds.push(box_);
        inner_faces.push(hole);
    }
    let tools = inner_faces.iter().collect::<Vec<_>>();
    let compound = session.create_compound(&tools)?;
    let cut = session.cut(&outer_face, &compound)?;
    if session.subshape_count(&cut, ShapeType::Face)? != 1 {
        return Err(ModelError::new(
            "planar region must produce exactly one connected face",
        ));
    }
    let region = session.subshape_with_history(&cut, ShapeType::Face, 0)?;
    let area = session.surface_area(&region)?;
    if !session.face_is_planar(&region)?
        || !session.is_valid(&region)?
        || session.subshape_count(&region, ShapeType::Wire)? != holes.len() + 1
        || !area.is_finite()
        || area <= 0.0
        || (area - (outer_area - removed_area)).abs() > 1e-12_f64.max(outer_area * 1e-7)
    {
        return Err(ModelError::new(
            "native planar region has invalid topology or unexpected area",
        ));
    }
    Ok(region)
}

#[cfg(test)]
mod tests {
    use super::*;
    use occt_bridge::HistoryRelation;
    #[test]
    fn multiple_displaced_holes_are_exact_and_invalid_boundaries_release_scratch_handles() {
        let session = Session::new().unwrap();
        let normal = Vec3::new(0.0, 0.0, 1.0);
        let mut shapes = HashMap::new();
        for (id, x, y, z, radius) in [
            ("outer", 0.0, 0.0, 0.0, 10.0),
            ("a", -4.0, 0.0, 0.0, 1.0),
            ("b", 4.0, 0.0, 0.0, 2.0),
            ("outside", 20.0, 0.0, 0.0, 1.0),
            ("cross", 9.5, 0.0, 0.0, 1.0),
            ("touch", 9.0, 0.0, 0.0, 1.0),
            ("above", 0.0, 0.0, 1.0, 1.0),
            ("overlap", -4.0, 0.5, 0.0, 1.0),
            ("nested", -4.0, 0.0, 0.0, 0.5),
            ("touch_hole", -2.0, 0.0, 0.0, 1.0),
        ] {
            shapes.insert(
                id.into(),
                session
                    .create_circle_wire(Vec3::new(x, y, z), normal, radius)
                    .unwrap(),
            );
        }
        shapes.insert(
            "tilted".into(),
            session
                .create_circle_wire(Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0), 1.0)
                .unwrap(),
        );
        shapes.insert(
            "open".into(),
            session
                .create_polyline_wire(
                    &[
                        Vec3::new(0.0, 0.0, 0.0),
                        Vec3::new(1.0, 0.0, 0.0),
                        Vec3::new(1.0, 1.0, 0.0),
                    ],
                    false,
                )
                .unwrap(),
        );
        shapes.insert(
            "solid".into(),
            session
                .create_sphere(Vec3::new(0.0, 0.0, 0.0), 1.0)
                .unwrap(),
        );
        let baseline = session.shape_count().unwrap();
        let region = execute(&session, "outer", &["a".into(), "b".into()], &shapes).unwrap();
        assert_eq!(session.subshape_count(&region, ShapeType::Wire).unwrap(), 3);
        assert!(
            (session.surface_area(&region).unwrap() - 95.0 * std::f64::consts::PI).abs() < 1e-7
        );
        let solid = session
            .create_prism_from_face(&region, Vec3::new(0.0, 0.0, 5.0))
            .unwrap();
        assert!((session.volume(&solid).unwrap() - 475.0 * std::f64::consts::PI).abs() < 1e-6);
        for id in ["outer", "a", "b"] {
            let edge = session.subshape(&shapes[id], ShapeType::Edge, 0).unwrap();
            assert!(
                session
                    .history_count(&solid, &edge, HistoryRelation::Generated)
                    .unwrap()
                    > 0,
                "source {id} edge lost"
            );
        }
        shapes.insert("already_holed".into(), session.duplicate(&region).unwrap());
        drop((region, solid));
        let baseline = baseline + 1;
        for holes in [
            vec![],
            vec!["missing"],
            vec!["outer"],
            vec!["a", "a"],
            vec!["outside"],
            vec!["cross"],
            vec!["touch"],
            vec!["above"],
            vec!["tilted"],
            vec!["open"],
            vec!["solid"],
            vec!["a", "overlap"],
            vec!["a", "nested"],
            vec!["a", "touch_hole"],
            vec!["already_holed"],
        ] {
            let ids = holes.into_iter().map(str::to_owned).collect::<Vec<_>>();
            assert!(
                execute(&session, "outer", &ids, &shapes).is_err(),
                "accepted {ids:?}"
            );
            assert_eq!(
                session.shape_count().unwrap(),
                baseline,
                "scratch handles for {ids:?}"
            );
        }
        drop(shapes);
        assert_eq!(session.shape_count().unwrap(), 0);
    }
}
