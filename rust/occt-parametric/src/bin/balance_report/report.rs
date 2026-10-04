use occt_parametric::*;
use serde_json::{Value, json};

pub fn properties(properties: &PhysicalMassProperties) -> Value {
    let center = properties.center_mm;
    json!({
        "mass_kg": properties.mass_kg, "volume_mm3": properties.volume_mm3,
        "center_mm": [center.x, center.y, center.z],
        "inertia_kg_mm2": properties.inertia_kg_mm2,
        "relative_volume_error": properties.relative_volume_error,
    })
}

pub fn document(
    mass: &AssemblyMassProperties,
    reference: ChordReference,
    mac: Option<MeanAerodynamicChord>,
) -> Result<Value, Box<dyn std::error::Error>> {
    let balance = mass.balance(reference)?;
    let percent = balance.chord_fraction * 100.0;
    if !percent.is_finite() {
        return Err("percent chord exceeds finite reporting limits".into());
    }
    let materials: Vec<_> = mass
        .material_totals()?
        .iter()
        .map(|group| {
            json!({
                "material": group.material, "components": group.components,
                "properties": properties(&group.properties),
            })
        })
        .collect();
    let components: Vec<_> = mass
        .components
        .iter()
        .map(|component| {
            json!({
                "output": component.output, "material": component.material,
                "properties": properties(&component.properties),
            })
        })
        .collect();
    let mac = mac.map(|mac| json!({
        "length_mm": mac.length_mm,
        "leading_edge_mm": [mac.leading_edge_mm.x, mac.leading_edge_mm.y, mac.leading_edge_mm.z],
        "planform_area_mm2": mac.planform_area_mm2,
    }));
    Ok(json!({
        "schema": "occb-balance-result-v1", "generated_variants": mass.generated_variants,
        "components": components, "materials": materials, "total": properties(&mass.total),
        "reference": reference, "mean_aerodynamic_chord": mac,
        "balance": {
            "distance_from_leading_edge_mm": balance.distance_from_leading_edge_mm,
            "chord_fraction": balance.chord_fraction,
            "percent_chord": percent,
        },
    }))
}
