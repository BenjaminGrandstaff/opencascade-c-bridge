//! Constructed datum planes at scale: a chain of 500 offset planes built on
//! one face plane, each carrying its own sketch, regenerated and then
//! edited through the face they all hang from.

use super::*;
use occt_parametric::GeneratedResult;
use serde_json::{Value, json};

const PLANES: usize = 500;

/// The face-pocket part with `PLANES` planes stacked 0.1 mm apart above the
/// block's top face and the pocket outline sketched on every one.
fn stacked() -> Result<ModelDocument, ModelError> {
    let mut request: Value = serde_json::from_str(include_str!(
        "../../../../tools/model/face-pocket.request.json"
    ))
    .map_err(|error| failure(error.to_string()))?;
    let family = &mut request["model"]["family"];
    let scalar = |v: f64| json!({"value": v, "dimension": "scalar", "unit": null});
    let mm = |v: f64| json!({"literal": {"value": v, "dimension": "length", "unit": "millimeter"}});
    let mut datums = vec![json!({"id": "plane0", "kind": {"face_plane": {
        "input": "block",
        "face": {"normal_aligned": {
            "direction": {"literal": {"x": scalar(0.0), "y": scalar(0.0), "z": scalar(1.0)}},
            "minimum_dot": {"literal": scalar(0.999999)}
        }},
        "distance": mm(0.0)
    }}})];
    datums.extend((1..PLANES).map(|i| {
        json!({"id": format!("plane{i}"), "kind": {"offset_plane": {
            "base": {"datum": format!("plane{}", i - 1)}, "distance": mm(0.1)
        }}})
    }));
    family["datums"] = json!(datums);
    let features = family["features"].as_array_mut().unwrap();
    let mut profile = features[1].clone();
    let sketch = &mut profile["operation"]["sketch_face"]["sketch"];
    sketch.as_object_mut().unwrap().remove("face_support");
    let sketches = (0..PLANES)
        .map(|i| {
            let mut feature = profile.clone();
            feature["id"] = json!(format!("outline{i}"));
            feature["operation"]["sketch_face"]["sketch"]["datum_plane"] =
                json!(format!("plane{i}"));
            feature
        })
        .collect::<Vec<_>>();
    features.truncate(1);
    features.extend(sketches);
    family["requirements"] = json!([]);
    request["model"]["family"] = family.clone();
    ModelDocument::from_json(&request["model"].to_string())
}

/// 500 sketches on a 500-deep plane chain: each plane resolves once per
/// regeneration (O(planes + sketches)), not once per sketch per level.
/// Both regenerations together take about 0.3 s.
pub(crate) fn datum_plane_chain_case() -> Outcome {
    timed(
        format!(
            "datum planes: {PLANES} sketches on a {PLANES}-deep chain over one face, then a height edit"
        ),
        Duration::from_secs(2),
        Expectation::Required,
        || {
            let document = stacked()?;
            let session = Session::new()?;
            let mut part = PartInstance {
                id: "part".into(),
                definition: &document.family,
                overrides: HashMap::new(),
                provenance: "bench".into(),
            };
            let top = |result: &GeneratedResult<'_>| -> Result<f64, ModelError> {
                let shape = result
                    .shape(&format!("outline{}", PLANES - 1))
                    .ok_or_else(|| failure("last outline missing".into()))?;
                Ok(session.exact_bounds(shape)?.min.z)
            };
            let first = part.regenerate(&session)?;
            let expected = 20.0 + 0.1 * (PLANES - 1) as f64;
            part.overrides.insert(
                "height".into(),
                ParameterValue::Scalar(Quantity::length(30.0, LengthUnit::Millimeter)),
            );
            let edited = part.regenerate_incremental(&session, &first)?;
            let (before, after) = (top(&first)?, top(&edited)?);
            if (before - expected).abs() > 1e-6 || (after - expected - 10.0).abs() > 1e-6 {
                return Err(failure(format!(
                    "top outline at {before} then {after}, expected {expected} then {}",
                    expected + 10.0
                )));
            }
            if edited.regeneration.rebuilt.len() != PLANES + 1 {
                return Err(failure(format!(
                    "{} features rebuilt",
                    edited.regeneration.rebuilt.len()
                )));
            }
            Ok(format!(
                "top outline follows the face from {before:.1} to {after:.1} mm; all {PLANES} sketches rebuilt"
            ))
        },
    )
}
