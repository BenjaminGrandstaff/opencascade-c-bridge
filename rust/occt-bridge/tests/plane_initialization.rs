use occt_bridge::{ProjectionFrame, Session, ShapeType, Vec3};
use std::{process::Command, sync::Barrier};

#[test]
fn concurrent_first_sessions_project_safely() {
    const CHILD: &str = "OCCB_TEST_FIRST_PROJECTION_CHILD";
    if std::env::var_os(CHILD).is_none() {
        for _ in 0..20 {
            let status = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "concurrent_first_sessions_project_safely"])
                .env(CHILD, "1")
                .output()
                .unwrap();
            assert!(
                status.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&status.stdout),
                String::from_utf8_lossy(&status.stderr)
            );
        }
        return;
    }
    let barrier = Barrier::new(16);
    std::thread::scope(|scope| {
        for _ in 0..16 {
            scope.spawn(|| {
                barrier.wait();
                let session = Session::new().unwrap();
                let shape = session
                    .create_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 20.0, 30.0))
                    .unwrap();
                let projection = session
                    .orthographic_projection(
                        &shape,
                        ProjectionFrame {
                            origin: Vec3::new(0.0, 0.0, 0.0),
                            direction: Vec3::new(0.0, 0.0, 1.0),
                            x_axis: Vec3::new(1.0, 0.0, 0.0),
                        },
                    )
                    .unwrap();
                assert!(
                    session
                        .subshape_count(&projection.visible, ShapeType::Edge)
                        .unwrap()
                        >= 4
                );
                drop(projection);
                drop(shape);
                assert_eq!(session.shape_count().unwrap(), 0);
            });
        }
    });
}
