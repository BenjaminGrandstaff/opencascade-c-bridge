use occt_bridge::Session;
use occt_parametric::*;
use std::{collections::HashMap, time::Instant};

fn vector(x: f64, y: f64, z: f64) -> VectorQuantity {
    VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter)
}
fn mm(value: f64) -> Quantity {
    Quantity::length(value, LengthUnit::Millimeter)
}

fn planform() {
    let wing = SymmetricWingPlanform {
        stations: (0..10000)
            .map(|index| PlanformStation {
                span: mm(index as f64),
                chord: mm(100.0),
                leading_edge: vector(index as f64 * 0.5, index as f64, 0.0),
            })
            .collect(),
    };
    let started = Instant::now();
    for _ in 0..100 {
        let mac = wing.mean_aerodynamic_chord().unwrap();
        assert!((mac.length_mm - 100.0).abs() < 1e-8);
        assert!((mac.leading_edge_mm.x - 2499.75).abs() < 1e-8);
        assert!((mac.planform_area_mm2 - 1_999_800.0).abs() < 1e-6);
    }
    assert!(started.elapsed().as_secs_f64() < 5.0);
    println!(
        "balance 100 MAC integrations of 10000 stations: {:?} (5s budget)",
        started.elapsed()
    );
}

fn assembly() {
    let family = FamilyDefinition {
        references: Vec::new(),
        id: "unit".into(),
        version: 1,
        parameters: vec![],
        derived_parameters: vec![],
        derived_vector_parameters: vec![],
        constraints: vec![],
        requirements: vec![],
        datums: vec![],
        features: vec![FeatureDefinition {
            id: "body".into(),
            operation: FeatureOperation::Box {
                origin: VectorExpr::Literal(vector(0.0, 0.0, 0.0)),
                size: VectorExpr::Literal(vector(1.0, 1.0, 1.0)),
            },
        }],
    };
    let mut graph = InstanceGraph::new(&family);
    graph
        .add_base("prototype", HashMap::new(), "bench")
        .unwrap();
    for (id, density) in [("light", 1e6), ("heavy", 2e6)] {
        graph
            .add_material(Material {
                id: id.into(),
                name: id.into(),
                density_kg_per_cubic_meter: density,
            })
            .unwrap();
    }
    let mut outputs = vec![];
    for index in 0..10000 {
        let id = format!("part-{index}");
        graph
            .add_clone(&id, "prototype", HashMap::new(), "bench")
            .unwrap();
        graph
            .assign_material(&id, Some(if index % 2 == 0 { "light" } else { "heavy" }))
            .unwrap();
        graph
            .set_placement(
                &id,
                Placement::translated(vector(1e6 + 3.0 * index as f64, 0.0, 0.0)),
            )
            .unwrap();
        outputs.push(InstanceOutputRef {
            instance: id,
            output: "body".into(),
        });
    }
    let session = Session::new().unwrap();
    let started = Instant::now();
    let mass = graph.mass_properties(&session, &outputs).unwrap();
    let materials = mass.material_totals().unwrap();
    assert_eq!(mass.generated_variants, 1);
    assert_eq!(materials.len(), 2);
    assert!(materials.iter().all(|material| material.components == 5000));
    assert!((mass.total.mass_kg - 15.0).abs() < 1e-8);
    assert!((materials[0].properties.mass_kg - 10.0).abs() < 1e-8);
    assert!((materials[1].properties.mass_kg - 5.0).abs() < 1e-8);
    let balance = mass
        .balance(ChordReference {
            leading_edge: vector(1e6, 0.0, 0.0),
            direction: VectorQuantity::scalars(1.0, 0.0, 0.0),
            length: mm(30000.0),
        })
        .unwrap();
    assert!((balance.distance_from_leading_edge_mm - 14999.5).abs() < 1e-5);
    assert_eq!(session.shape_count().unwrap(), 0);
    assert!(started.elapsed().as_secs_f64() < 10.0);
    println!(
        "balance 10000 selected components and material totals: {:?} (10s budget), one variant",
        started.elapsed()
    );
}

fn main() {
    planform();
    assembly();
}
