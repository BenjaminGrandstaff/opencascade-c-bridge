use super::linkage::{definition, mechanism};
use super::*;

fn axes() -> Vec<JointSeedAxis> {
    vec![
        JointSeedAxis::linear(
            JointVariable {
                frame: "rod".into(),
                coordinate: JointDof::Angle,
            },
            Quantity::scalar(-std::f64::consts::PI),
            Quantity::scalar(std::f64::consts::PI),
            7,
        )
        .unwrap(),
        JointSeedAxis::linear(
            JointVariable {
                frame: "slider".into(),
                coordinate: JointDof::Axial,
            },
            Quantity::length(-5.0, LengthUnit::Millimeter),
            Quantity::length(5.0, LengthUnit::Millimeter),
            3,
        )
        .unwrap(),
    ]
}

#[test]
fn branch_search_discovers_both_crank_slider_solutions_deterministically_without_mutation() {
    let definition = definition(1.0);
    let graph = mechanism(&definition, 1.0, 0.0);
    let before = ModelDocument::from_graph(&graph);
    let result = graph
        .search_joint_branches(&axes(), Default::default())
        .unwrap();
    assert_eq!(result.status, JointBranchSearchStatus::SeedsExhausted);
    assert_eq!(result.planned_starts, 22);
    assert_eq!(result.attempted_starts, 22);
    assert_eq!(result.branches.len(), 2, "{result:?}");
    assert_eq!(
        result.attempted_starts,
        result.branches.len() + result.repeated_solutions + result.failed_starts
    );
    let expected = [1.0 + 6.0_f64.sqrt(), 1.0 - 6.0_f64.sqrt()];
    for (branch, expected) in result.branches.iter().zip(expected) {
        assert!(branch.solved && branch.checks.iter().all(|check| check.satisfied));
        assert!((branch.positions[1].value.value - expected).abs() < 1e-6);
        let mut candidate = graph.clone();
        for position in &branch.positions {
            candidate
                .set_joint_coordinate(&position.frame, position.coordinate, position.value)
                .unwrap();
        }
        assert!(
            candidate
                .check_relationships()
                .unwrap()
                .iter()
                .all(|check| check.satisfied)
        );
    }
    assert_eq!(
        result,
        graph
            .search_joint_branches(&axes(), Default::default())
            .unwrap()
    );
    assert_eq!(ModelDocument::from_graph(&graph), before);
}

#[test]
fn branch_search_budget_and_branch_limit_report_partial_discovery() {
    let definition = definition(1.0);
    let graph = mechanism(&definition, 1.0, 0.0);
    let result = graph
        .search_joint_branches(
            &axes(),
            JointBranchSearchOptions {
                maximum_branches: 1,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(result.status, JointBranchSearchStatus::BranchLimitReached);
    assert_eq!(result.attempted_starts, 1);
    assert_eq!(result.branches.len(), 1);
    let result = graph
        .search_joint_branches(
            &axes(),
            JointBranchSearchOptions {
                maximum_total_iterations: 1,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(
        result.status,
        JointBranchSearchStatus::IterationBudgetExceeded
    );
    assert_eq!(result.attempted_starts, 1);
    assert_eq!(result.iterations, 1);
    assert_eq!(result.failed_starts, 1);
    assert!(!result.best_unsolved.unwrap().solved);
    let mut last = axes();
    last[0].seeds = vec![Quantity::scalar(0.0)];
    last[1].seeds = vec![Quantity::length(0.0, LengthUnit::Millimeter)];
    let result = graph
        .search_joint_branches(
            &last,
            JointBranchSearchOptions {
                include_current_pose: false,
                maximum_total_iterations: 1,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(result.planned_starts, 1);
    assert_eq!(result.attempted_starts, 1);
    assert_eq!(
        result.status,
        JointBranchSearchStatus::IterationBudgetExceeded
    );
}

#[test]
fn branch_search_obeys_physical_limits_and_keeps_best_failed_candidate() {
    let definition = definition(1.0);
    let mut graph = mechanism(&definition, 1.0, 0.0);
    if let JointKind::Prismatic { distance } =
        &mut graph.assembly.joints.get_mut("slider").unwrap().kind
    {
        distance.minimum = Some(Quantity::length(2.0, LengthUnit::Millimeter));
        distance.maximum = Some(Quantity::length(3.0, LengthUnit::Millimeter));
    }
    let mut grid = axes();
    grid[1] = JointSeedAxis::linear(
        grid[1].variable.clone(),
        Quantity::length(0.2, LengthUnit::Centimeter),
        Quantity::length(3.0, LengthUnit::Millimeter),
        3,
    )
    .unwrap();
    let before = ModelDocument::from_graph(&graph);
    let result = graph
        .search_joint_branches(&grid, Default::default())
        .unwrap();
    assert!(result.branches.is_empty());
    assert_eq!(result.failed_starts, result.attempted_starts);
    assert!(
        result
            .best_unsolved
            .unwrap()
            .positions
            .iter()
            .filter(|position| position.frame == "slider")
            .all(|position| position.value.normalized().unwrap() >= 2.0
                && position.value.normalized().unwrap() <= 3.0)
    );
    assert_eq!(ModelDocument::from_graph(&graph), before);
    grid[1]
        .seeds
        .push(Quantity::length(4.0, LengthUnit::Millimeter));
    assert!(
        graph
            .search_joint_branches(&grid, Default::default())
            .is_err()
    );
}

#[test]
fn branch_search_periodic_equivalence_preserves_unwrapped_pose_when_requested() {
    let definition = definition(1.0);
    let mut graph = mechanism(&definition, 1.0, 0.0);
    graph
        .solve_joint_coordinates(&super::linkage::variables(), Default::default())
        .unwrap();
    let rod = graph.assembly.joints.get("rod").unwrap().clone();
    let angle = match rod.kind {
        JointKind::Revolute { angle } => angle.value.value,
        _ => panic!(),
    };
    let grid = vec![JointSeedAxis {
        variable: JointVariable {
            frame: "rod".into(),
            coordinate: JointDof::Angle,
        },
        seeds: vec![
            Quantity::scalar(angle),
            Quantity::scalar(angle + std::f64::consts::TAU),
        ],
    }];
    let result = graph
        .search_joint_branches(&grid, Default::default())
        .unwrap();
    assert_eq!(result.branches.len(), 1);
    assert_eq!(result.repeated_solutions, 2);
    let result = graph
        .search_joint_branches(
            &grid,
            JointBranchSearchOptions {
                equivalence: JointBranchEquivalence::Coordinates,
                include_current_pose: false,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(result.branches.len(), 2);
    assert_eq!(result.attempted_starts, 2);
    assert!(
        (result.branches[1].positions[0].value.value
            - result.branches[0].positions[0].value.value
            - std::f64::consts::TAU)
            .abs()
            < 1e-9
    );
}

#[test]
fn branch_search_rejects_invalid_grid_units_duplicates_and_options() {
    let definition = definition(1.0);
    let graph = mechanism(&definition, 1.0, 0.0);
    let before = ModelDocument::from_graph(&graph);
    let mut duplicate = axes();
    duplicate.push(duplicate[0].clone());
    let mut empty = axes();
    empty[0].seeds.clear();
    let mut missing = axes();
    missing[0].variable.frame = "missing".into();
    let mut wrong = axes();
    wrong[0].seeds[0] = Quantity::length(1.0, LengthUnit::Millimeter);
    let mut duplicate_seed = axes();
    duplicate_seed[1].seeds = vec![
        Quantity::length(1.0, LengthUnit::Millimeter),
        Quantity::length(0.1, LengthUnit::Centimeter),
    ];
    let mut excessive = axes();
    excessive[0].seeds = vec![Quantity::scalar(1.0); 10001];
    for grid in [
        vec![],
        duplicate,
        empty,
        missing,
        wrong,
        duplicate_seed,
        excessive,
    ] {
        assert!(
            graph
                .search_joint_branches(&grid, Default::default())
                .is_err()
        );
    }
    for options in [
        JointBranchSearchOptions {
            maximum_branches: 0,
            ..Default::default()
        },
        JointBranchSearchOptions {
            maximum_branches: 129,
            ..Default::default()
        },
        JointBranchSearchOptions {
            maximum_total_iterations: 0,
            ..Default::default()
        },
        JointBranchSearchOptions {
            maximum_total_iterations: 1000001,
            ..Default::default()
        },
        JointBranchSearchOptions {
            distinct_normalized_distance: 0.0,
            ..Default::default()
        },
        JointBranchSearchOptions {
            distinct_normalized_distance: f64::NAN,
            ..Default::default()
        },
    ] {
        assert!(graph.search_joint_branches(&axes(), options).is_err());
    }
    let variable = axes()[0].variable.clone();
    for (a, b, count) in [
        (Quantity::scalar(0.0), Quantity::scalar(1.0), 1),
        (Quantity::scalar(0.0), Quantity::scalar(0.0), 2),
        (Quantity::scalar(f64::NAN), Quantity::scalar(1.0), 2),
        (
            Quantity::scalar(0.0),
            Quantity::length(1.0, LengthUnit::Millimeter),
            2,
        ),
    ] {
        assert!(JointSeedAxis::linear(variable.clone(), a, b, count).is_err());
    }
    assert_eq!(ModelDocument::from_graph(&graph), before);
}

#[test]
fn branch_search_escapes_singular_four_bar_seed_and_matches_circle_intersections() {
    let definition = definition(1.0);
    let mut graph = mechanism(&definition, 1.0, 0.0);
    graph
        .set_joint_coordinate("crank", JointDof::Angle, Quantity::scalar(0.0))
        .unwrap();
    graph
        .set_joint_coordinate("rod", JointDof::Angle, Quantity::scalar(0.0))
        .unwrap();
    graph.frames.get_mut("slider").unwrap().placement = Placement::translated(
        VectorQuantity::lengths(4.0, 0.0, 0.0, LengthUnit::Millimeter),
    );
    graph.assembly.joints.get_mut("slider").unwrap().kind = JointKind::Revolute {
        angle: JointScalar {
            value: Quantity::scalar(0.0),
            minimum: None,
            maximum: None,
        },
    };
    graph.assembly.joints.get_mut("slider").unwrap().axis = VectorQuantity::scalars(0.0, 0.0, 1.0);
    graph.assembly.joints.get_mut("slider").unwrap().origin =
        VectorQuantity::lengths(4.0, 0.0, 0.0, LengthUnit::Millimeter);
    graph.assembly.relationships[0].second.datum = "end".into();
    let grid: Vec<_> = ["rod", "slider"]
        .into_iter()
        .map(|frame| {
            JointSeedAxis::linear(
                JointVariable {
                    frame: frame.into(),
                    coordinate: JointDof::Angle,
                },
                Quantity::scalar(-std::f64::consts::PI),
                Quantity::scalar(std::f64::consts::PI),
                5,
            )
            .unwrap()
        })
        .collect();
    let free: Vec<_> = grid.iter().map(|axis| axis.variable.clone()).collect();
    let mut local = graph.clone();
    assert!(
        !local
            .solve_joint_coordinates(&free, Default::default())
            .unwrap()
            .solved
    );
    let before = ModelDocument::from_graph(&graph);
    let result = graph
        .search_joint_branches(&grid, Default::default())
        .unwrap();
    assert_eq!(result.branches.len(), 2, "{result:?}");
    assert!(result.failed_starts > 0);
    let mut heights = vec![];
    for branch in result.branches {
        let mut candidate = graph.clone();
        for position in branch.positions {
            candidate
                .set_joint_coordinate(&position.frame, position.coordinate, position.value)
                .unwrap();
        }
        match candidate.datum("rod", "end").unwrap() {
            ResolvedDatum::Point { origin } => {
                assert!((origin.x - 3.0).abs() < 1e-6);
                assert!((origin.y.abs() - 8.0_f64.sqrt()).abs() < 1e-6);
                heights.push(origin.y);
            }
            _ => panic!(),
        }
    }
    assert!(heights[0] * heights[1] < 0.0);
    assert_eq!(ModelDocument::from_graph(&graph), before);
}

#[test]
fn branch_search_bounds_total_coordinate_work_for_large_grids() {
    let definition = definition(1.0);
    let (graph, variables) = super::linkage::slider_array(&definition, 1000, false);
    let mut grid: Vec<_> = variables
        .into_iter()
        .map(|variable| JointSeedAxis {
            variable,
            seeds: vec![Quantity::length(0.5, LengthUnit::Millimeter)],
        })
        .collect();
    grid[0] = JointSeedAxis::linear(
        grid[0].variable.clone(),
        Quantity::length(0.0, LengthUnit::Millimeter),
        Quantity::length(1.0, LengthUnit::Millimeter),
        10000,
    )
    .unwrap();
    let before = ModelDocument::from_graph(&graph);
    let error = graph
        .search_joint_branches(&grid, Default::default())
        .unwrap_err();
    assert!(
        error.to_string().contains("coordinate work budget"),
        "{error}"
    );
    assert_eq!(ModelDocument::from_graph(&graph), before);
}
