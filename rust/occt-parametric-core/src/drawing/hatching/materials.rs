use super::*;

pub fn validate_materials(view: &DrawingView, known: &HashSet<&str>) -> Result<(), ModelError> {
    if view.material_hatching.len() > 10_000 {
        return Err(ModelError::new(
            "material hatching allows at most 10000 materials",
        ));
    }
    for (material, patterns) in &view.material_hatching {
        if !known.contains(material.as_str()) {
            return Err(ModelError::new(format!(
                "drawing hatching references unknown material '{material}'"
            )));
        }
        if patterns.len() > 8 {
            return Err(ModelError::new(
                "material hatching allows at most 8 line families per material",
            ));
        }
        for pattern in patterns {
            validate_pattern(*pattern)?;
        }
    }
    Ok(())
}
