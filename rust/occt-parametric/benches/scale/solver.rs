//! Relationship solving for large stacks, grids, and distant parts.

use super::*;

pub(crate) fn solver_cases(definition: &'static FamilyDefinition) -> Vec<Outcome> {
    let cases = [
        (50, 1_000.0, false, None, ms(1_000)),
        (20, 1_000.0, true, None, ms(500)),
        (50, 1_000.0, true, None, ms(1_000)),
        (1_000, 1_000.0, false, None, ms(2_000)),
        (1_000, 1_000.0, true, None, ms(5_000)),
        (50, 1_000_000.0, false, None, ms(1_000)),
        (50, 1_000_000.0, false, Some(1e-8), ms(1_000)),
    ];
    cases
        .into_iter()
        .map(|(count, offset, constrained, linear_tolerance, budget)| {
            let mut graph = stacked_blocks(definition, count, offset, constrained);
            if let Some(linear_millimeters) = linear_tolerance {
                graph
                    .set_relationship_tolerances(RelationshipTolerances {
                        linear_millimeters,
                        ..RelationshipTolerances::default()
                    })
                    .unwrap();
            }
            let ids = (1..=count)
                .map(|index| format!("i{index}"))
                .collect::<Vec<_>>();
            let ids = ids.iter().map(String::as_str).collect::<Vec<_>>();
            let kind = if constrained { "constrained" } else { "seated" };
            let tolerance = linear_tolerance
                .map(|value| format!(" at {value:.0e} mm tolerance"))
                .unwrap_or_default();
            timed(
                format!("solve {count} {kind} parts at {offset} mm{tolerance}"),
                budget,
                Expectation::Required,
                || {
                    let solution = graph.solve_placements(&ids)?;
                    if solution.solved {
                        Ok(format!("{} iterations", solution.iterations))
                    } else {
                        Err(failure(format!(
                            "unsolved, max residual {:.1e}",
                            solution.max_residual
                        )))
                    }
                },
            )
        })
        .collect()
}

pub(crate) const GRID_ROWS: usize = 30;

pub(crate) const GRID_COLUMNS: usize = 34;

pub(crate) const GRID_PITCH: f64 = 50.0;

/// A grid of blocks, each tied to its left and upper neighbors (coplanar
/// tops, parallel sides, axis distances), so elimination meets
/// two-dimensional fill-in rather than a chain.
pub(crate) fn solver_grid(definition: &'static FamilyDefinition) -> Outcome {
    let at = |x: f64, y: f64, z: f64| VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter);
    let id = |row: usize, column: usize| format!("g{row}_{column}");
    let mut graph = InstanceGraph::new(definition);
    for row in 0..GRID_ROWS {
        for column in 0..GRID_COLUMNS {
            let name = id(row, column);
            let (x, y) = (column as f64 * GRID_PITCH, row as f64 * GRID_PITCH);
            if row == 0 && column == 0 {
                graph
                    .add_base(name.clone(), HashMap::new(), "bench")
                    .unwrap();
                continue;
            }
            graph
                .add_clone(name.clone(), id(0, 0), HashMap::new(), "bench")
                .unwrap();
            // Start a few millimeters and a small turn away from the solution.
            let wobble = ((row * 31 + column * 17) % 7) as f64 - 3.0;
            graph
                .set_placement(
                    &name,
                    Placement {
                        translation: at(x + wobble, y - wobble, wobble),
                        rotation: Some(AxisAngle {
                            origin: at(0.0, 0.0, 0.0),
                            axis: VectorQuantity::scalars(0.0, 0.0, 1.0),
                            angle_radians: 0.01 * wobble,
                        }),
                    },
                )
                .unwrap();
            let mut relate = |suffix: &str, kind, other: String, first: &str, second: &str| {
                graph
                    .add_relationship(AssemblyRelationship {
                        id: format!("{name}-{suffix}"),
                        kind,
                        first: DatumRef::new(other, first),
                        second: DatumRef::new(name.clone(), second),
                    })
                    .unwrap();
            };
            let pitch =
                RelationKind::Distance(Quantity::length(GRID_PITCH, LengthUnit::Millimeter));
            if column > 0 {
                relate(
                    "level",
                    RelationKind::Coincident,
                    id(row, column - 1),
                    "top",
                    "top",
                );
                relate("left", pitch, id(row, column - 1), "axis", "axis");
                relate(
                    "square",
                    RelationKind::Parallel,
                    id(row, column - 1),
                    "right",
                    "right",
                );
            }
            if row > 0 {
                relate("up", pitch, id(row - 1, column), "axis", "axis");
                if column == 0 {
                    relate(
                        "level",
                        RelationKind::Coincident,
                        id(row - 1, column),
                        "top",
                        "top",
                    );
                    relate(
                        "square",
                        RelationKind::Parallel,
                        id(row - 1, column),
                        "right",
                        "right",
                    );
                }
            }
        }
    }
    let free = (0..GRID_ROWS)
        .flat_map(|row| (0..GRID_COLUMNS).map(move |column| (row, column)))
        .filter(|&cell| cell != (0, 0))
        .map(|(row, column)| id(row, column))
        .collect::<Vec<_>>();
    let ids = free.iter().map(String::as_str).collect::<Vec<_>>();
    timed(
        format!(
            "solve {}x{} grid ({} parts)",
            GRID_ROWS,
            GRID_COLUMNS,
            ids.len()
        ),
        ms(20_000),
        Expectation::Required,
        || {
            let solution = graph.solve_placements(&ids)?;
            if solution.solved {
                Ok(format!(
                    "{} iterations, {} free degrees",
                    solution.iterations, solution.free_degrees
                ))
            } else {
                Err(failure(format!(
                    "unsolved, max residual {:.1e}",
                    solution.max_residual
                )))
            }
        },
    )
}

/// A fixed base block and `count` free clones stacked on it, offset from the
/// origin and displaced from their solved positions; constrained stacks also
/// align axes and side faces, after a small turn about Z.
pub(crate) fn stacked_blocks(
    definition: &FamilyDefinition,
    count: usize,
    offset: f64,
    constrained: bool,
) -> InstanceGraph<'_> {
    let at = |x: f64, y: f64, z: f64| VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter);
    let mut graph = InstanceGraph::new(definition);
    graph.add_base("i0", HashMap::new(), "bench").unwrap();
    graph
        .set_placement("i0", Placement::translated(at(offset, -offset / 2.0, 0.0)))
        .unwrap();
    for index in 1..=count {
        let id = format!("i{index}");
        let previous = format!("i{}", index - 1);
        graph
            .add_clone(id.clone(), "i0", HashMap::new(), "bench")
            .unwrap();
        let rotation = constrained.then(|| AxisAngle {
            origin: at(0.0, 0.0, 0.0),
            axis: VectorQuantity::scalars(0.0, 0.0, 1.0),
            angle_radians: 0.2,
        });
        graph
            .set_placement(
                &id,
                Placement {
                    translation: at(
                        offset + index as f64 * 3.0,
                        -offset / 2.0 - 2.0,
                        index as f64 * 25.0,
                    ),
                    rotation,
                },
            )
            .unwrap();
        let mut relate = |suffix: &str, kind, first: &str, second: &str| {
            graph
                .add_relationship(AssemblyRelationship {
                    id: format!("{id}-{suffix}"),
                    kind,
                    first: DatumRef::new(previous.clone(), first),
                    second: DatumRef::new(id.clone(), second),
                })
                .unwrap();
        };
        relate("seat", RelationKind::Coincident, "top", "bottom");
        if constrained {
            relate("axis", RelationKind::Coincident, "axis", "axis");
            relate("square", RelationKind::Parallel, "right", "right");
        }
    }
    graph
}
