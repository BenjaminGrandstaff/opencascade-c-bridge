//! Thickened sheets at scale: a 400-face corrugated sheet with mitred
//! corners, and thickness edits of the clamp strap that reuse its sheet.

use super::*;
use serde_json::{Value, json};

fn strap_request() -> Result<Value, ModelError> {
    serde_json::from_str(include_str!(
        "../../../../tools/model/thickened-strap.request.json"
    ))
    .map_err(|error| failure(error.to_string()))
}

fn base(document: &ModelDocument) -> PartInstance<'_> {
    PartInstance {
        id: "strap".into(),
        definition: &document.family,
        overrides: HashMap::new(),
        provenance: "bench".into(),
    }
}

/// The strap's open profile replaced by a 400-segment zigzag (5 mm pitch,
/// 3 mm amplitude), swept 20 mm and thickened 1 mm: 400 sheet faces meeting
/// at 399 alternating sharp edges.
pub(crate) fn thicken_sheet_case() -> Outcome {
    timed(
        "thicken: 400-face corrugated sheet with mitred corners".into(),
        Duration::from_secs(5),
        Expectation::Required,
        || {
            let mut request = strap_request()?;
            let family = &mut request["model"]["family"];
            let features = family["features"].as_array_mut().unwrap();
            features.truncate(4);
            let sketch = &mut features[0]["operation"]["sketch_open_wire"]["sketch"];
            let mm = |v: f64| json!({"literal": {"value": v, "dimension": "length", "unit": "millimeter"}});
            sketch["points"] = (0..=400)
                .map(|i| {
                    let z = if i % 2 == 0 { 0.0 } else { 3.0 };
                    json!({"id": format!("p{i}"), "x": mm(5.0 * i as f64), "y": mm(z), "fixed": true})
                })
                .collect();
            sketch["lines"] = (0..400)
                .map(|i| json!({"id": format!("l{i}"), "start": format!("p{i}"), "end": format!("p{}", i + 1)}))
                .collect();
            sketch["arcs"] = json!([]);
            sketch["profile"] = (0..400).map(|i| json!(format!("l{i}"))).collect();
            family["requirements"] = json!([]);
            let document = ModelDocument::from_json(&request["model"].to_string())?;
            let session = Session::new().map_err(|error| failure(error.to_string()))?;
            let mut part = base(&document);
            part.overrides.insert(
                "thickness".into(),
                ParameterValue::Scalar(Quantity::length(1.0, LengthUnit::Millimeter)),
            );
            let built = part.regenerate(&session)?;
            let (sheet, shell) = (
                built
                    .shape("sheet")
                    .ok_or_else(|| failure("sheet missing".into()))?,
                built
                    .shape("shell")
                    .ok_or_else(|| failure("shell missing".into()))?,
            );
            let check = |result: Result<usize, occt_bridge::BridgeError>| {
                result.map_err(|error| failure(error.to_string()))
            };
            let faces = check(session.subshape_count(sheet, ShapeType::Face))?;
            let solids = check(session.subshape_count(shell, ShapeType::Solid))?;
            let valid = session
                .is_valid(shell)
                .map_err(|error| failure(error.to_string()))?;
            let area = session
                .surface_area(sheet)
                .map_err(|error| failure(error.to_string()))?;
            let volume = session
                .volume(shell)
                .map_err(|error| failure(error.to_string()))?;
            // Alternating mitres cancel to within one corner's t² · width.
            if faces != 400 || solids != 1 || !valid || (volume - area).abs() > 20.0 {
                return Err(failure(format!(
                    "{faces} faces, {solids} solids, valid {valid}, volume {volume} vs {area}"
                )));
            }
            Ok("one valid solid; volume matches sheet area x thickness".into())
        },
    )
}

/// Thickness edits of the clamp strap: the swept sheet is reused and only
/// the thicken and its two bolt holes rebuild (about 16 ms per edit, mostly
/// the two hole cuts and their material checks).
pub(crate) fn thicken_edit_case() -> Outcome {
    timed(
        "thicken: 500 thickness edits of a clamp strap".into(),
        Duration::from_secs(15),
        Expectation::Required,
        || {
            let request = strap_request()?;
            let document = ModelDocument::from_json(&request["model"].to_string())?;
            let session = Session::new().map_err(|error| failure(error.to_string()))?;
            let mut part = base(&document);
            let mut previous = part.regenerate(&session)?;
            for index in 0..500 {
                let thickness = [1.0, 2.0, 3.5][index % 3];
                part.overrides.insert(
                    "thickness".into(),
                    ParameterValue::Scalar(Quantity::length(thickness, LengthUnit::Millimeter)),
                );
                let built = part.regenerate_incremental(&session, &previous)?;
                if !built.regeneration.reused.contains(&"sheet".to_string()) {
                    return Err(failure("the swept sheet was rebuilt".into()));
                }
                previous = built;
            }
            drop(previous);
            let retained = session
                .shape_count()
                .map_err(|error| failure(error.to_string()))?;
            if retained != 0 {
                return Err(failure(format!("{retained} retained handles")));
            }
            Ok("sheet reused, strap rebuilt, no retained handles".into())
        },
    )
}
