use super::*;

fn at(x: f64) -> VectorExpr {
    VectorExpr::Literal(VectorQuantity::lengths(x, 0.0, 0.0, LengthUnit::Millimeter))
}
fn cube(id: &str) -> FeatureDefinition {
    FeatureDefinition {
        id: id.into(),
        operation: FeatureOperation::Box {
            origin: at(0.0),
            size: VectorExpr::Literal(VectorQuantity::lengths(
                10.0,
                10.0,
                10.0,
                LengthUnit::Millimeter,
            )),
        },
    }
}
fn moved(id: &str, input: &str, x: f64) -> FeatureDefinition {
    FeatureDefinition {
        id: id.into(),
        operation: FeatureOperation::Translate {
            input: input.into(),
            offset: at(x),
        },
    }
}
/// base → a → b, each moved along X.
fn family() -> FamilyDefinition {
    FamilyDefinition {
        id: "part".into(),
        version: 1,
        parameters: vec![],
        derived_parameters: vec![],
        derived_vector_parameters: vec![],
        constraints: vec![],
        features: vec![cube("base"), moved("a", "base", 1.0), moved("b", "a", 2.0)],
        requirements: vec![],
        datums: vec![],
        references: vec![],
        feature_colors: Default::default(),
        assumptions: vec![],
        history: FeatureHistory::default(),
    }
}
fn ids(family: &FamilyDefinition) -> Vec<&str> {
    family.features.iter().map(|f| f.id.as_str()).collect()
}

#[test]
fn bookmarks_save_and_restore_the_timeline_state() {
    let mut family = family();
    family.checkout_feature(Some("a".into())).unwrap();
    family.set_feature_suppressed("base", true).unwrap();
    family.bookmark_timeline("before b").unwrap();
    family.checkout_feature(None).unwrap();
    family.set_feature_suppressed("base", false).unwrap();
    assert!(family.history.builds_everything());
    family.checkout_bookmark("before b").unwrap();
    assert_eq!(family.history.head.as_deref(), Some("a"));
    assert_eq!(family.history.suppressed, ["base"]);
    family.validate_timeline().unwrap();
    let message = |result: Result<(), ModelError>| result.unwrap_err().message;
    assert!(message(family.bookmark_timeline("before b")).contains("already exists"));
    assert!(message(family.bookmark_timeline(" padded")).contains("surrounding spaces"));
    assert!(message(family.checkout_bookmark("nope")).contains("unknown bookmark"));
    family.delete_bookmark("before b").unwrap();
    assert!(message(family.delete_bookmark("before b")).contains("unknown bookmark"));
}

#[test]
fn branches_switch_tails_and_share_edits_before_their_base() {
    let mut family = family();
    // Branch "wide" after `a`, then make it differ: c replaces b.
    family.create_branch("wide", Some("a")).unwrap();
    family.switch_branch("wide").unwrap();
    assert_eq!(family.history.branch_name(), "wide");
    assert_eq!(ids(&family), ["base", "a", "b"]);
    family.features.pop();
    family.features.push(moved("c", "a", 30.0));
    family.bookmark_timeline("wide v1").unwrap();
    family.validate_timeline().unwrap();
    // Back on main: b returns, wide keeps only c.
    family.switch_branch("main").unwrap();
    assert_eq!(family.history.branch, None);
    assert_eq!(ids(&family), ["base", "a", "b"]);
    let wide = &family.history.branches[0];
    assert_eq!(
        (wide.name.as_str(), wide.base.as_deref()),
        ("wide", Some("a"))
    );
    assert_eq!(
        wide.features
            .iter()
            .map(|f| f.id.as_str())
            .collect::<Vec<_>>(),
        ["c"]
    );
    assert_eq!(wide.bookmarks[0].name, "wide v1");
    assert!(family.history.bookmarks.is_empty());
    // An edit before the base reaches the other branch when it is active.
    family.features[1] = moved("a", "base", 5.0);
    family.switch_branch("wide").unwrap();
    assert_eq!(family.features[1], moved("a", "base", 5.0));
    assert_eq!(ids(&family), ["base", "a", "c"]);
    // The document keeps every branch through JSON.
    let text = serde_json::to_string(&family.history).unwrap();
    let back: FeatureHistory = serde_json::from_str(&text).unwrap();
    assert_eq!(back, family.history);
    // A branch from the start shares nothing.
    family.create_branch("fresh", None).unwrap();
    assert_eq!(family.history.branches.last().unwrap().features.len(), 3);
    family.validate_timeline().unwrap();
}

#[test]
fn merging_appends_what_the_active_branch_lacks_or_reports_conflicts() {
    let mut family = family();
    family.create_branch("extra", Some("b")).unwrap();
    family.switch_branch("extra").unwrap();
    family.features.push(moved("d", "b", 4.0));
    family.switch_branch("main").unwrap();
    family.features.push(moved("e", "b", 5.0));
    let added = family.merge_branch("extra").unwrap().unwrap();
    assert_eq!(added, ["d"]);
    assert_eq!(ids(&family), ["base", "a", "b", "e", "d"]);
    // Merging again adds nothing.
    assert!(family.merge_branch("extra").unwrap().unwrap().is_empty());
    // The same id defined differently on both sides is a conflict.
    family.create_branch("clash", Some("b")).unwrap();
    family.switch_branch("clash").unwrap();
    family.features.retain(|f| f.id != "e");
    family.features.push(moved("e", "b", 50.0));
    family.switch_branch("main").unwrap();
    let before = family.features.clone();
    let conflicts = family.merge_branch("clash").unwrap().unwrap_err();
    assert_eq!(
        conflicts,
        [BranchConflict {
            feature: "e".into()
        }]
    );
    assert_eq!(family.features, before);
}

#[test]
fn branch_and_bookmark_problems_are_refused() {
    let mut family = family();
    let message = |result: Result<(), ModelError>| result.unwrap_err().message;
    assert!(message(family.create_branch("main", None)).contains("already exists"));
    assert!(message(family.create_branch("x", Some("ghost"))).contains("unknown feature"));
    assert!(message(family.switch_branch("ghost")).contains("unknown branch"));
    family.create_branch("after-a", Some("a")).unwrap();
    assert!(message(family.create_branch("after-a", None)).contains("already exists"));
    // Deleting a later feature is fine; deleting the base breaks the branch.
    family.features.retain(|f| f.id != "b");
    family.validate_timeline().unwrap();
    let mut broken = family.clone();
    broken.features.retain(|f| f.id != "a");
    assert!(message(broken.validate_timeline()).contains("delete or merge the branch first"));
    // A stored branch may not redeclare a shared feature.
    let mut shadow = family.clone();
    shadow.history.branches[0].features.push(cube("base"));
    assert!(message(shadow.validate_timeline()).contains("over a shared one"));
    // Bookmarks must name existing features.
    let mut stale = family.clone();
    stale.history.bookmarks.push(TimelineBookmark {
        name: "old".into(),
        head: Some("b".into()),
        suppressed: vec![],
    });
    assert!(message(stale.validate_timeline()).contains("unknown feature 'b'"));
    family.delete_branch("after-a").unwrap();
    assert!(message(family.delete_branch("after-a")).contains("unknown branch"));
}
