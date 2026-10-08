//! Exact face splitting, closest complete cutoff selection, and ray witnesses.
use occt_bridge::{Session, Vec3};
use occt_parametric::*;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
fn main() {
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tools/model/curved-extrusions.request.json"
    ))
    .unwrap();
    let model = ModelDocument::from_json(&request["model"].to_string()).unwrap();
    let session = Session::new().unwrap();
    let start = Instant::now();
    for _ in 0..250 {
        let result = PartInstance {
            id: "scale".into(),
            definition: &model.family,
            overrides: HashMap::new(),
            provenance: "benchmark".into(),
        }
        .regenerate(&session)
        .unwrap();
        let rounded = result.shape("body").unwrap();
        let inclined = result.shape("selected").unwrap();
        assert!(session.is_valid(rounded).unwrap());
        assert!(session.is_valid(inclined).unwrap());
        assert!((session.volume(inclined).unwrap() - 12000.).abs() < 1e-6);
        let volume = session.volume(rounded).unwrap();
        assert!(volume > 20000. && volume < 32000.);
        let (_, length) = session
            .ray_first_hit(
                rounded,
                Vec3::new(20., 12.5, 0.),
                Vec3::new(0., 0., 1.),
                100.,
            )
            .unwrap()
            .unwrap();
        assert!((length - 20.).abs() < 1e-7);
        drop(result);
        assert_eq!(session.shape_count().unwrap(), 0);
    }
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(10),
        "curved cap gate exceeded: {elapsed:?}"
    );
    println!(
        "PASS 250 inclined/spherical extent regenerations: {:.3}s / 10s",
        elapsed.as_secs_f64()
    );
}
