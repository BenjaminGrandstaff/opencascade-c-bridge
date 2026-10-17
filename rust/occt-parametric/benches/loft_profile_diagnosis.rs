//! Isolated failed-loft diagnosis: source regeneration, exact alignment and release.
use occt_bridge::Session;
use occt_parametric::*;
use std::time::{Duration, Instant};
fn main() {
    let mut request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tools/model/partitioned-profile-loft.request.json"
    ))
    .unwrap();
    request["model"]["family"]["features"][0]["operation"]["profile_loft"]["correspondence"] =
        serde_json::json!({});
    let document = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    let graph = document.instance_graph().unwrap();
    let part = graph.resolve("block").unwrap();
    let session = Session::new().unwrap();
    let start = Instant::now();
    for _ in 0..1000 {
        let report =
            diagnose_loft_profiles(&part, &session, &document.family.features[0].operation)
                .unwrap();
        assert!(!report.checks_passed && !report.solid_checked);
        assert_eq!(report.profiles.len(), 2);
        assert_eq!(
            (
                report.profiles[0].source_edges,
                report.profiles[0].aligned_edges
            ),
            (Some(1), Some(1))
        );
        assert_eq!(report.profiles[1].aligned_edges, Some(4));
        assert!(
            report
                .profiles
                .iter()
                .all(|p| p.issues.iter().any(|i| i.code == "edge_count_mismatch"))
        );
        assert_eq!(session.shape_count().unwrap(), 0);
    }
    let elapsed = start.elapsed();
    println!("1000 isolated failed-loft profile diagnoses: {elapsed:?} (budget 10s)");
    assert!(
        elapsed < Duration::from_secs(10),
        "Loft profile diagnosis exceeded its scale budget."
    );
    let hollow: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tools/model/hollow-loft.request.json"
    ))
    .unwrap();
    regions(&hollow, 500, [48., 12.], 1, "hollow stations");
    let mut many = hollow.clone();
    for p in many["model"]["family"]["parameters"]
        .as_array_mut()
        .unwrap()
    {
        if ["lower_bore_radius", "upper_bore_radius"]
            .iter()
            .any(|id| p["id"] == *id)
        {
            p["default"]["scalar"]["value"] = serde_json::json!(0.1);
        }
    }
    let mut holes = Vec::new();
    for i in 0..100 {
        let mut track = Vec::new();
        for (index, z) in [(3, 0.), (4, 20.)] {
            let id = format!("hole-{i}-{index}");
            let mut feature = many["model"]["family"]["features"][index].clone();
            feature["id"] = serde_json::json!(id);
            let sketch = &mut feature["operation"]["sketch_wire"]["sketch"];
            sketch["id"] = serde_json::json!(id);
            let xyz = [
                (i % 10) as f64 * 0.5 - 2.25,
                (i / 10) as f64 * 0.5 - 2.25,
                z,
            ];
            sketch["origin"] = serde_json::json!({"literal":std::collections::BTreeMap::from_iter(["x","y","z"].into_iter().zip(xyz).map(|(k,v)|(k,serde_json::json!({"value":v,"dimension":"length","unit":"millimeter"}))))});
            many["model"]["family"]["features"]
                .as_array_mut()
                .unwrap()
                .push(feature);
            track.push(id);
        }
        holes.push(track);
    }
    many["model"]["family"]["features"][0]["operation"]["profile_loft"]["holes"] =
        serde_json::json!(holes);
    regions(&many, 1, [63., 15.], 100, "100-hole stations");
}

fn regions(
    request: &serde_json::Value,
    iterations: usize,
    areas: [f64; 2],
    holes: usize,
    label: &str,
) {
    let doc = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    let graph = doc.instance_graph().unwrap();
    let part = graph.resolve("duct").unwrap();
    let session = Session::new().unwrap();
    let start = Instant::now();
    for _ in 0..iterations {
        let report =
            diagnose_loft_profiles(&part, &session, &doc.family.features[0].operation).unwrap();
        assert!(report.checks_passed && !report.solid_checked);
        assert_eq!(report.stations.len(), 2);
        assert_eq!(report.profiles.len(), 2 + holes * 2);
        for (station, area) in report.stations.iter().zip(areas) {
            assert!(station.issues.is_empty());
            assert_eq!(station.holes.len(), holes);
            assert!((station.area_mm2.unwrap() - area * std::f64::consts::PI).abs() < 1e-6);
        }
        assert_eq!(session.shape_count().unwrap(), 0);
    }
    let elapsed = start.elapsed();
    println!("{iterations} {label} diagnoses: {elapsed:?} (budget 10s)");
    assert!(
        elapsed < Duration::from_secs(10),
        "Station material diagnosis exceeded its scale budget."
    );
}
