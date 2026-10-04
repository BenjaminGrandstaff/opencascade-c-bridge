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
