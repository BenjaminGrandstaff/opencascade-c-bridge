//! Planar splits at scale: a 400-hole plate split through a column of holes,
//! and split-height edits that reuse a bored housing and rebuild its halves.

use super::*;

fn mm(x: f64, y: f64, z: f64) -> VectorExpr {
    VectorExpr::Literal(VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter))
}

fn length(value: f64) -> ScalarExpr {
    ScalarExpr::Literal(Quantity::length(value, LengthUnit::Millimeter))
}

/// A 410 x 410 x 10 plate cut by a 20 x 20 grid of radius-4 holes, split by
/// the plane x = 210 through the centres of one hole column; both sides kept.
pub(crate) fn split_plate_case(definition: &FamilyDefinition) -> Outcome {
    timed(
        "split: 400-hole plate through a hole column, both sides".into(),
        Duration::from_secs(5),
        Expectation::Required,
        || {
            let mut definition = definition.clone();
            let feature = |id: &str, operation| FeatureDefinition {
                id: id.into(),
                operation,
            };
            definition.features = vec![
                feature(
                    "blank",
                    FeatureOperation::Box {
                        origin: mm(0.0, 0.0, 0.0),
                        size: mm(410.0, 410.0, 10.0),
                    },
                ),
                feature(
                    "pin",
                    FeatureOperation::Cylinder {
                        origin: mm(10.0, 10.0, -1.0),
                        axis: VectorExpr::Literal(VectorQuantity::scalars(0.0, 0.0, 1.0)),
                        radius: length(4.0),
                        height: length(12.0),
                    },
                ),
                feature(
                    "row",
                    FeatureOperation::LinearPattern {
                        skip: Vec::new(),
                        input: "pin".into(),
                        step: mm(20.0, 0.0, 0.0),
                        count: ScalarExpr::Literal(Quantity::scalar(20.0)),
                    },
                ),
                feature(
                    "grid",
                    FeatureOperation::LinearPattern {
                        skip: Vec::new(),
                        input: "row".into(),
                        step: mm(0.0, 20.0, 0.0),
                        count: ScalarExpr::Literal(Quantity::scalar(20.0)),
                    },
                ),
                feature(
                    "plate",
                    FeatureOperation::Cut {
                        object: "blank".into(),
                        tool: "grid".into(),
                    },
                ),
                feature(
                    "body",
                    FeatureOperation::Split {
                        input: "plate".into(),
                        origin: mm(210.0, 0.0, 0.0),
                        normal: VectorExpr::Literal(VectorQuantity::scalars(1.0, 0.0, 0.0)),
                        keep: SplitKeep::Both,
                    },
                ),
            ];
            let session = Session::new().map_err(|error| failure(error.to_string()))?;
            let mut graph = InstanceGraph::new(&definition);
            graph.add_base("part", HashMap::new(), "bench")?;
            let generation = graph.regenerate_all(&session)?;
            let result = generation
                .result("part")
                .ok_or_else(|| failure("split plate missing".into()))?;
            let body = result
                .shape("body")
                .ok_or_else(|| failure("split output missing".into()))?;
            let solids = session
                .subshape_count(body, ShapeType::Solid)
                .map_err(|error| failure(error.to_string()))?;
            let whole = result
                .shape("plate")
                .ok_or_else(|| failure("plate missing".into()))?;
            let (a, b) = (
                session.volume(whole).map_err(|e| failure(e.to_string()))?,
                session.volume(body).map_err(|e| failure(e.to_string()))?,
            );
            if solids != 2 || (a - b).abs() > 1e-7 * a {
                return Err(failure(format!("{solids} solids, volume {b} of {a}")));
            }
            drop(generation);
            Ok("two halves conserving volume through 20 split holes".into())
        },
    )
}

/// Split-height edits on the split housing: the bored block is reused and
/// only the cap and base splits rebuild (about 31 ms per edit: two cuts,
/// piece and input validity, and the example's two validity requirements).
pub(crate) fn split_edit_case() -> Outcome {
    timed(
        "split: 500 split-height edits of a bored housing".into(),
        Duration::from_secs(30),
        Expectation::Required,
        || {
            let request: serde_json::Value = serde_json::from_str(include_str!(
                "../../../../tools/model/split-housing.request.json"
            ))
            .map_err(|error| failure(error.to_string()))?;
            let document = ModelDocument::from_json(&request["model"].to_string())?;
            let session = Session::new().map_err(|error| failure(error.to_string()))?;
            let mut part = PartInstance {
                id: "housing".into(),
                definition: &document.family,
                overrides: HashMap::new(),
                provenance: "bench".into(),
            };
            let mut previous = part.regenerate(&session)?;
            for index in 0..500 {
                let height = [9.0, 15.0, 21.0][index % 3];
                part.overrides.insert(
                    "split_height".into(),
                    ParameterValue::Scalar(Quantity::length(height, LengthUnit::Millimeter)),
                );
                let built = part.regenerate_incremental(&session, &previous)?;
                if !built.regeneration.reused.contains(&"housing".to_string()) {
                    return Err(failure("the bored housing was rebuilt".into()));
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
            Ok("housing reused, halves rebuilt, no retained handles".into())
        },
    )
}
