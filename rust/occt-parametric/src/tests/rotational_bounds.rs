use super::*;
fn placement(angle: f64, translation: Vec3, pivot: Vec3, axis: Vec3) -> Placement {
    Placement {
        translation: VectorQuantity::lengths(
            translation.x,
            translation.y,
            translation.z,
            LengthUnit::Millimeter,
        ),
        rotation: Some(AxisAngle {
            origin: VectorQuantity::lengths(pivot.x, pivot.y, pivot.z, LengthUnit::Millimeter),
            axis: VectorQuantity::scalars(axis.x, axis.y, axis.z),
            angle_radians: angle,
        }),
    }
}
#[test]
fn rotational_swept_and_speed_bounds_enclose_sampled_nested_paths() {
    let zero = Vec3::new(0.0, 0.0, 0.0);
    for turns in [0.2, -1.0, 3.0] {
        let start = vec![
            placement(
                0.3,
                Vec3::new(1.0, 2.0, 3.0),
                Vec3::new(2.0, 0.0, 0.0),
                Vec3::new(0.0, 0.0, 1.0),
            ),
            placement(
                -0.5,
                zero,
                Vec3::new(-3.0, 1.0, 0.0),
                Vec3::new(1.0, 1.0, 0.0),
            ),
        ];
        let end = vec![
            placement(
                0.3 + turns * std::f64::consts::TAU,
                Vec3::new(-2.0, 4.0, 1.0),
                Vec3::new(2.0, 0.0, 0.0),
                Vec3::new(0.0, 0.0, 1.0),
            ),
            placement(
                0.7,
                Vec3::new(3.0, -2.0, 1.0),
                Vec3::new(-3.0, 1.0, 0.0),
                Vec3::new(1.0, 1.0, 0.0),
            ),
        ];
        let local = Bounds {
            min: Vec3::new(-1.0, -2.0, -0.5),
            max: Vec3::new(2.0, 1.0, 1.5),
        };
        let corners = box_corners(local);
        let world = corners
            .iter()
            .map(|point| {
                start.iter().fold(*point, |point, step| {
                    transform_point(point, &step.normalized().unwrap())
                })
            })
            .collect::<Vec<_>>();
        let bounds = Bounds {
            min: Vec3::new(
                world
                    .iter()
                    .map(|point| point.x)
                    .fold(f64::INFINITY, f64::min),
                world
                    .iter()
                    .map(|point| point.y)
                    .fold(f64::INFINITY, f64::min),
                world
                    .iter()
                    .map(|point| point.z)
                    .fold(f64::INFINITY, f64::min),
            ),
            max: Vec3::new(
                world
                    .iter()
                    .map(|point| point.x)
                    .fold(f64::NEG_INFINITY, f64::max),
                world
                    .iter()
                    .map(|point| point.y)
                    .fold(f64::NEG_INFINITY, f64::max),
                world
                    .iter()
                    .map(|point| point.z)
                    .fold(f64::NEG_INFINITY, f64::max),
            ),
        };
        let path = RigidPath::new(&start, &end, bounds).unwrap();
        for point in corners {
            check_corner_path(&path, point);
        }
    }
}

fn box_corners(bounds: Bounds) -> Vec<Vec3> {
    (0..8)
        .map(|index| {
            Vec3::new(
                if index & 1 == 0 {
                    bounds.min.x
                } else {
                    bounds.max.x
                },
                if index & 2 == 0 {
                    bounds.min.y
                } else {
                    bounds.max.y
                },
                if index & 4 == 0 {
                    bounds.min.z
                } else {
                    bounds.max.z
                },
            )
        })
        .collect::<Vec<_>>()
}

fn check_corner_path(path: &RigidPath, point: Vec3) {
    let mut previous = None;
    for sample in 0..=256 {
        let fraction = sample as f64 / 256.0;
        let position = path.steps.iter().fold(point, |point, step| {
            transform_point(point, &step.at(fraction))
        });
        assert!(position.x >= path.swept.min.x && position.x <= path.swept.max.x);
        assert!(position.y >= path.swept.min.y && position.y <= path.swept.max.y);
        assert!(position.z >= path.swept.min.z && position.z <= path.swept.max.z);
        if let Some(previous) = previous {
            assert!(length(subtract(position, previous)) <= path.speed / 256.0 + 1e-10);
        }
        previous = Some(position);
    }
}

#[test]
fn interval_boxes_capture_interior_arc_extrema_and_preserve_axial_thickness() {
    let zero = Vec3::new(0.0, 0.0, 0.0);
    let axis = Vec3::new(0.0, 0.0, 1.0);
    let start = vec![placement(-std::f64::consts::FRAC_PI_2, zero, zero, axis)];
    let end = vec![placement(std::f64::consts::FRAC_PI_2, zero, zero, axis)];
    let point = Vec3::new(1.0, -1.0, 2.0);
    let path = RigidPath::new(
        &start,
        &end,
        Bounds {
            min: point,
            max: point,
        },
    )
    .unwrap();
    assert!((path.swept.max.x - 2.0_f64.sqrt()).abs() < 1e-10);
    assert!((path.swept.min.x + 1.0).abs() < 1e-10);
    assert!((path.swept.min.z - 2.0).abs() < 1e-10 && (path.swept.max.z - 2.0).abs() < 1e-10);
    for turns in [-3.0, -0.2, 0.2, 3.0] {
        let first = vec![placement(0.3, zero, zero, axis)];
        let last = vec![placement(
            0.3 + turns * std::f64::consts::TAU,
            zero,
            zero,
            axis,
        )];
        let bounds = Bounds {
            min: Vec3::new(-100.0, -100.0, 4.0),
            max: Vec3::new(100.0, 100.0, 5.0),
        };
        let path = RigidPath::new(&first, &last, bounds).unwrap();
        assert!(path.swept.max.z - path.swept.min.z < 1.0000001);
        for (lower, upper) in [(0.0, 0.1), (0.17, 0.39), (0.5, 0.5), (0.8, 1.0)] {
            let interval = path.interval_bounds(lower, upper).unwrap();
            for world in box_corners(bounds) {
                let local = path.inverse.iter().fold(world, transform_point);
                for sample in 0..=64 {
                    let fraction = lower + (upper - lower) * sample as f64 / 64.0;
                    let position = path.steps.iter().fold(local, |point, step| {
                        transform_point(point, &step.at(fraction))
                    });
                    assert!(
                        position.x >= interval.min.x && position.x <= interval.max.x,
                        "{position:?} vs {interval:?}"
                    );
                    assert!(position.y >= interval.min.y && position.y <= interval.max.y);
                    assert!(position.z >= interval.min.z && position.z <= interval.max.z);
                }
            }
        }
    }
}

#[test]
fn interval_boxes_reject_unrepresentable_coordinate_transforms() {
    let zero = Vec3::new(0.0, 0.0, 0.0);
    let axis = Vec3::new(0.0, 0.0, 1.0);
    let point = Vec3::new(f64::MAX, 0.0, 0.0);
    let bounds = Bounds {
        min: point,
        max: point,
    };
    let inverse_overflow = vec![placement(0.0, Vec3::new(-f64::MAX, 0.0, 0.0), zero, axis)];
    assert!(
        RigidPath::new(&inverse_overflow, &inverse_overflow, bounds)
            .err()
            .unwrap()
            .to_string()
            .contains("box overflows")
    );
    let start = vec![placement(0.0, zero, zero, axis)];
    let end = vec![placement(0.0, Vec3::new(f64::MAX, 0.0, 0.0), zero, axis)];
    assert!(
        RigidPath::new(&start, &end, bounds)
            .err()
            .unwrap()
            .to_string()
            .contains("box overflows")
    );
}

#[test]
fn interval_boxes_cover_nonunit_oblique_rotation_axes() {
    let zero = Vec3::new(0.0, 0.0, 0.0);
    let unrepresentable = Vec3::new(f64::MAX, f64::MAX, 0.0);
    let first = vec![placement(0.0, zero, zero, unrepresentable)];
    let last = vec![placement(1.0, zero, zero, unrepresentable)];
    assert!(
        RigidPath::new(
            &first,
            &last,
            Bounds {
                min: zero,
                max: zero
            }
        )
        .err()
        .unwrap()
        .to_string()
        .contains("rotation axis")
    );
    for axis in [
        Vec3::new(1.0, 1.0, 0.0),
        Vec3::new(3.0, -4.0, 12.0),
        Vec3::new(0.0, 0.0, 7.0),
    ] {
        let start = vec![placement(-0.4, zero, Vec3::new(5.0, 2.0, -1.0), axis)];
        let end = vec![placement(
            2.7,
            Vec3::new(1.0, -3.0, 2.0),
            Vec3::new(5.0, 2.0, -1.0),
            axis,
        )];
        let local = Vec3::new(2.0, -3.0, 4.0);
        let world = transform_point(local, &start[0].normalized().unwrap());
        let path = RigidPath::new(
            &start,
            &end,
            Bounds {
                min: world,
                max: world,
            },
        )
        .unwrap();
        for (lower, upper) in [(0.0, 1.0), (0.1, 0.3), (0.7, 1.0)] {
            let bounds = path.interval_bounds(lower, upper).unwrap();
            for sample in 0..=128 {
                let fraction = lower + (upper - lower) * sample as f64 / 128.0;
                let position = path.steps.iter().fold(local, |point, step| {
                    transform_point(point, &step.at(fraction))
                });
                assert!(
                    position.x >= bounds.min.x && position.x <= bounds.max.x,
                    "axis {axis:?}: {position:?} vs {bounds:?}"
                );
                assert!(position.y >= bounds.min.y && position.y <= bounds.max.y);
                assert!(position.z >= bounds.min.z && position.z <= bounds.max.z);
            }
        }
    }
}

#[test]
fn shared_motion_keys_cancel_only_static_inner_frames_and_keep_unwrapped_travel() {
    let zero = Vec3::new(0.0, 0.0, 0.0);
    let axis = Vec3::new(0.0, 0.0, 2.0);
    let bounds = Bounds {
        min: zero,
        max: Vec3::new(1.0, 1.0, 1.0),
    };
    let start = placement(0.3, zero, zero, axis);
    let end = placement(
        0.3 + std::f64::consts::TAU,
        Vec3::new(2.0, 3.0, 0.0),
        zero,
        axis,
    );
    let base = RigidPath::new(&[start], &[end], bounds).unwrap();
    let mount = placement(0.7, Vec3::new(4.0, 2.0, 1.0), zero, axis);
    let mounted = RigidPath::new(&[mount, start], &[mount, end], bounds).unwrap();
    assert_eq!(base.motion_key(), mounted.motion_key());
    let moving_mount = placement(0.8, mount.normalized().unwrap().translation, zero, axis);
    let independent = RigidPath::new(&[mount, start], &[moving_mount, end], bounds).unwrap();
    assert_ne!(base.motion_key(), independent.motion_key());
    let twice = placement(
        0.3 + 2.0 * std::f64::consts::TAU,
        Vec3::new(2.0, 3.0, 0.0),
        zero,
        axis,
    );
    let twice = RigidPath::new(&[start], &[twice], bounds).unwrap();
    assert_ne!(base.motion_key(), twice.motion_key());
    let outer = placement(0.1, Vec3::new(1.0, 0.0, 0.0), zero, axis);
    let framed = RigidPath::new(&[start, outer], &[end, outer], bounds).unwrap();
    assert_ne!(base.motion_key(), framed.motion_key());
}
