//! Feature colors carried to faces through later features.

use super::*;

fn mm(x: f64, y: f64, z: f64) -> VectorExpr {
    VectorExpr::Literal(VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter))
}

const RED: [f64; 3] = [1.0, 0.0, 0.0];
const BLUE: [f64; 3] = [0.0, 0.0, 1.0];
const GREEN: [f64; 3] = [0.0, 1.0, 0.0];

/// A red 40 x 20 x 10 block, notched across its top by a blue tool, then
/// filleted green along its four +x end edges.
fn painted_family() -> FamilyDefinition {
    let mut family = family(RequirementPriority::Advisory, 1e12);
    family.requirements.clear();
    family.features = vec![
        FeatureDefinition {
            id: "block".into(),
            operation: FeatureOperation::Box {
                origin: mm(0.0, 0.0, 0.0),
                size: mm(40.0, 20.0, 10.0),
            },
        },
        FeatureDefinition {
            id: "notch".into(),
            operation: FeatureOperation::Box {
                origin: mm(15.0, -1.0, 5.0),
                size: mm(10.0, 22.0, 6.0),
            },
        },
        FeatureDefinition {
            id: "slotted".into(),
            operation: FeatureOperation::Cut {
                object: "block".into(),
                tool: "notch".into(),
            },
        },
        FeatureDefinition {
            id: "eased".into(),
            operation: FeatureOperation::Fillet {
                input: "slotted".into(),
                edges: vec![EdgeSelector::AtExtreme {
                    axis: CoordinateAxis::X,
                    extremum: Extremum::Maximum,
                    tolerance: ScalarExpr::Literal(Quantity::length(1e-6, LengthUnit::Millimeter)),
                }],
                radius: ScalarExpr::Literal(Quantity::length(1.0, LengthUnit::Millimeter)),
            },
        },
    ];
    family.feature_colors = BTreeMap::from([
        ("block".into(), RED),
        ("notch".into(), BLUE),
        ("eased".into(), GREEN),
    ]);
    family
}

fn part(family: &FamilyDefinition) -> PartInstance<'_> {
    PartInstance {
        id: "part".into(),
        definition: family,
        overrides: HashMap::new(),
        provenance: "test".into(),
    }
}

fn count(colors: &[(usize, [f64; 3])], color: [f64; 3]) -> usize {
    colors.iter().filter(|(_, found)| *found == color).count()
}

#[test]
fn feature_colors_follow_faces_through_cuts_and_fillets() {
    let session = Session::new().unwrap();
    let family = painted_family();
    let first = part(&family).regenerate(&session).unwrap();
    let faces = |name: &str| {
        session
            .subshape_count(first.shape(name).unwrap(), ShapeType::Face)
            .unwrap()
    };

    // The block is all red; the cut keeps red on the block's faces (the top
    // split in two) and carries blue onto the notch's three walls.
    assert_eq!(first.face_colors("block").len(), 6);
    assert_eq!(count(first.face_colors("block"), RED), 6);
    let slotted = first.face_colors("slotted");
    assert_eq!(slotted.len(), faces("slotted"));
    assert_eq!(count(slotted, BLUE), 3, "{slotted:?}");
    assert_eq!(count(slotted, RED), faces("slotted") - 3);

    // The fillet's own faces are green; every other face keeps its color.
    let eased = first.face_colors("eased");
    assert_eq!(eased.len(), faces("eased"));
    assert_eq!(count(eased, BLUE), 3);
    let rounds = session
        .subshapes(first.shape("eased").unwrap(), ShapeType::Face)
        .unwrap();
    for (index, face) in rounds.iter().enumerate() {
        let curved = !session.face_is_planar(face).unwrap();
        let green = eased.contains(&(index, GREEN));
        assert_eq!(curved, green, "face {index}");
    }
    assert_eq!(count(eased, GREEN), 4);
    cleanup_shapes(&session, rounds);

    // Recoloring the tool reuses every output and repaints only its walls.
    let mut recolored = painted_family();
    recolored
        .feature_colors
        .insert("notch".into(), [0.0, 1.0, 1.0]);
    let second = part(&recolored)
        .regenerate_incremental(&session, &first)
        .unwrap();
    assert!(second.regeneration.rebuilt.is_empty());
    assert_eq!(count(second.face_colors("eased"), [0.0, 1.0, 1.0]), 3);
    assert_eq!(count(second.face_colors("eased"), BLUE), 0);
    drop((first, second));
    assert_eq!(session.shape_count().unwrap(), 0);
}

#[test]
fn feature_colors_validate_and_persist() {
    let session = Session::new().unwrap();
    let mut unknown = painted_family();
    unknown.feature_colors.insert("paint".into(), RED);
    let error = part(&unknown).regenerate(&session).err().unwrap();
    assert!(
        error.message.contains("unknown feature 'paint'"),
        "{}",
        error.message
    );
    let mut bright = painted_family();
    bright
        .feature_colors
        .insert("block".into(), [1.5, 0.0, 0.0]);
    let error = part(&bright).regenerate(&session).err().unwrap();
    assert!(error.message.contains("[0, 1]"), "{}", error.message);

    // Uncolored families generate no face colors and omit the field.
    let mut plain = painted_family();
    plain.feature_colors.clear();
    let generated = part(&plain).regenerate(&session).unwrap();
    assert!(generated.face_colors("eased").is_empty());
    assert!(
        !serde_json::to_string(&plain)
            .unwrap()
            .contains("feature_colors")
    );
    let json = serde_json::to_string(&painted_family()).unwrap();
    let back: FamilyDefinition = serde_json::from_str(&json).unwrap();
    assert_eq!(back, painted_family());
}

#[test]
fn feature_colors_reach_step_face_colors() {
    let family = painted_family();
    let mut graph = InstanceGraph::new(&family);
    graph.add_base("left", HashMap::new(), "test").unwrap();
    graph.add_base("right", HashMap::new(), "test").unwrap();
    graph
        .set_placement(
            "right",
            Placement::translated(VectorQuantity::lengths(
                0.0,
                50.0,
                0.0,
                LengthUnit::Millimeter,
            )),
        )
        .unwrap();
    let session = Session::new().unwrap();
    let generation = graph.regenerate_all(&session).unwrap();
    // Placement keeps the face colors' indices.
    assert_eq!(
        generation.result("right").unwrap().face_colors("eased"),
        generation.result("left").unwrap().face_colors("eased")
    );
    let path = std::env::temp_dir().join(format!("occb-colors-{}.step", std::process::id()));
    graph
        .export_step(
            &session,
            &generation,
            &path,
            "painted",
            &OutputSet::AllWithOutput("eased".into()),
        )
        .unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    let styled = text.matches("STYLED_ITEM(").count();
    let faces = session
        .subshape_count(
            generation.result("left").unwrap().shape("eased").unwrap(),
            ShapeType::Face,
        )
        .unwrap();
    assert!(styled >= faces, "{styled} styled items for {faces} faces");
}
