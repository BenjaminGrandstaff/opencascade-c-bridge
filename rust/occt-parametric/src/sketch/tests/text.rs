//! Single-line text outlines in sketch face outputs (schema 129).

use super::*;
use serde_json::{Value, json};

fn mm(value: f64) -> Value {
    json!({"literal": {"value": value, "dimension": "length", "unit": "millimeter"}})
}

fn sketch(rest: Value) -> SketchDefinition {
    let mut value = json!({
        "id": "s",
        "origin": {"literal": {"x": {"value": 0, "dimension": "length", "unit": "millimeter"},
            "y": {"value": 0, "dimension": "length", "unit": "millimeter"},
            "z": {"value": 0, "dimension": "length", "unit": "millimeter"}}},
        "x_axis": {"literal": {"x": {"value": 1, "dimension": "scalar", "unit": null},
            "y": {"value": 0, "dimension": "scalar", "unit": null},
            "z": {"value": 0, "dimension": "scalar", "unit": null}}},
        "y_axis": {"literal": {"x": {"value": 0, "dimension": "scalar", "unit": null},
            "y": {"value": 1, "dimension": "scalar", "unit": null},
            "z": {"value": 0, "dimension": "scalar", "unit": null}}},
        "points": [],
        "lines": [],
    });
    for (key, item) in rest.as_object().unwrap() {
        value[key] = item.clone();
    }
    serde_json::from_value(value).unwrap()
}

fn p(id: &str, x: f64, y: f64) -> Value {
    json!({"id": id, "x": mm(x), "y": mm(y), "fixed": true})
}

/// The first installed common sans family, if any.
fn family() -> Option<&'static str> {
    let session = Session::new().unwrap();
    ["Liberation Sans", "DejaVu Sans", "Noto Sans", "FreeSans"]
        .into_iter()
        .find(|f| {
            session
                .font_file(f, occt_bridge::FontStyle::Regular)
                .is_ok()
        })
}

fn label(font: &str, extra: Value) -> Value {
    let mut text =
        json!({"id": "label", "text": "AB-12", "font": font, "height": mm(10.), "anchor": "t"});
    for (k, v) in extra.as_object().unwrap() {
        text[k] = v.clone();
    }
    text
}

fn bounds(s: &SketchDefinition) -> Result<(usize, occt_bridge::Bounds), ModelError> {
    let session = Session::new().unwrap();
    let face = s.face(&session, &HashMap::new())?;
    assert!(session.is_valid(&face).unwrap());
    Ok((
        session.subshape_count(&face, ShapeType::Face).unwrap(),
        session.exact_bounds(&face).unwrap(),
    ))
}

#[test]
fn text_only_sketches_place_align_and_rotate_their_glyphs() {
    let Some(font) = family() else {
        eprintln!("no common sans font installed; skipping");
        return;
    };
    let text =
        |extra: Value| sketch(json!({"points": [p("t", 5., 7.)], "texts": [label(font, extra)]}));
    let (faces, start) = bounds(&text(json!({}))).unwrap();
    // A, B, -, 1, 2: A has a hole, B two; every glyph is one face.
    assert_eq!(faces, 5);
    assert!(start.min.x >= 5. - 1e-6 && start.min.x < 6.5 && start.min.y > 6.9);
    let (_, centre) = bounds(&text(json!({"align": "center"}))).unwrap();
    let (_, end) = bounds(&text(json!({"align": "end"}))).unwrap();
    assert!(centre.min.x < 5. && centre.max.x > 5.);
    assert!(end.max.x <= 5. + 1e-6);
    // A quarter turn swaps the extents about the anchor.
    let (_, turned) = bounds(&text(json!({"angle": {"literal": {"value": std::f64::consts::FRAC_PI_2, "dimension": "scalar", "unit": null}}}))).unwrap();
    assert!(((turned.max.y - turned.min.y) - (start.max.x - start.min.x)).abs() < 1e-6);
    assert!(((turned.max.x - turned.min.x) - (start.max.y - start.min.y)).abs() < 1e-6);
    assert!(turned.max.x <= 5. + 1e-6);
}

#[test]
fn texts_join_profiles_but_may_not_overlap_them() {
    let Some(font) = family() else {
        return;
    };
    let with_plate = |anchor: (f64, f64)| {
        sketch(json!({
            "points": [p("a", 0., 0.), p("b", 60., 20.), p("t", anchor.0, anchor.1)],
            "shapes": [{"rectangle": {"id": "plate", "first": "a", "opposite": "b"}}],
            "profile": ["plate"],
            "texts": [label(font, json!({}))],
        }))
    };
    let (faces, _) = bounds(&with_plate((5., 25.))).unwrap();
    assert_eq!(faces, 6);
    let error = bounds(&with_plate((5., 5.))).unwrap_err();
    assert!(
        error.message.contains("profile and text 'label' overlap"),
        "{}",
        error.message
    );
}

#[test]
fn invalid_texts_are_rejected() {
    let reject = |text: Value, message: &str| {
        let s = sketch(json!({"points": [p("t", 0., 0.)], "texts": [text]}));
        let session = Session::new().unwrap();
        let error = s.face(&session, &HashMap::new()).unwrap_err();
        assert!(
            error.message.contains(message),
            "{message}: {}",
            error.message
        );
    };
    let base = json!({"id": "x", "text": "Hi", "font": "No Such Family 4711", "height": mm(5.), "anchor": "t"});
    reject(base.clone(), "not installed");
    let with = |k: &str, v: Value| {
        let mut t = base.clone();
        t[k] = v;
        t
    };
    reject(with("anchor", json!("nope")), "unknown point 'nope'");
    reject(with("text", json!("  ")), "visible characters");
    reject(with("height", mm(0.)), "height must be positive");
    // Wires reject texts.
    let s = sketch(json!({"points": [p("t", 0., 0.)], "texts": [base]}));
    let session = Session::new().unwrap();
    let error = s.wire(&session, &HashMap::new(), None).unwrap_err();
    assert!(error.message.contains("texts apply only to face outputs"));

    // The runtime limit also applies to Rust callers and direct JSON input,
    // independently of the generated schema's maxItems restriction.
    let texts: Vec<_> = (0..101)
        .map(|i| json!({"id": format!("t{i}"), "text": "Hi", "font": "No Such Family 4711", "height": mm(5.), "anchor": "t"}))
        .collect();
    let s = sketch(json!({"points": [p("t", 0., 0.)], "texts": texts}));
    let error = s.face(&session, &HashMap::new()).unwrap_err();
    assert!(error.message.contains("texts exceed 100"));
}
