//! Application-level geometry recipes composed over `occt-bridge`.

use occt_bridge::{BridgeError, Session, Shape, Vec3};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PositionalLight {
    pub position: Vec3,
    pub color: Vec3,
    pub intensity: f64,
    pub range: f64,
    pub cast_shadows: bool,
}

#[derive(Debug, PartialEq)]
pub struct WallTorch<'session> {
    pub fixture: Shape<'session>,
    pub flame: Shape<'session>,
    pub light: PositionalLight,
}

/// Builds a wall torch by composing generic cylinders, cones, and compounds.
pub fn create_wall_torch<'a>(
    session: &'a Session,
    wall_anchor: Vec3,
    wall_normal: Vec3,
    scale: f64,
) -> Result<WallTorch<'a>, BridgeError> {
    if !finite(wall_anchor) || !finite(wall_normal) || !scale.is_finite() || scale <= 0.0 {
        return Err(invalid_argument("invalid wall-torch parameters"));
    }
    let horizontal_length = wall_normal.x.hypot(wall_normal.y);
    if horizontal_length <= f64::EPSILON {
        return Err(invalid_argument("wall normal must have an XY component"));
    }
    let outward = Vec3::new(
        wall_normal.x / horizontal_length,
        wall_normal.y / horizontal_length,
        0.0,
    );
    let up = Vec3::new(0.0, 0.0, 1.0);

    let plate = session.create_cylinder(wall_anchor, outward, 16.0 * scale, 6.0 * scale)?;
    let arm_start = Vec3::new(
        wall_anchor.x + outward.x * 5.0 * scale,
        wall_anchor.y + outward.y * 5.0 * scale,
        wall_anchor.z - 7.0 * scale,
    );
    let arm_end = Vec3::new(
        wall_anchor.x + outward.x * 49.0 * scale,
        wall_anchor.y + outward.y * 49.0 * scale,
        wall_anchor.z + 4.0 * scale,
    );
    let arm_axis = subtract(arm_end, arm_start);
    let arm = session.create_cylinder(arm_start, arm_axis, 4.0 * scale, magnitude(arm_axis))?;
    let stem_start = Vec3::new(arm_end.x, arm_end.y, arm_end.z - 18.0 * scale);
    let stem = session.create_cylinder(stem_start, up, 4.5 * scale, 35.0 * scale)?;
    let cup_base = Vec3::new(arm_end.x, arm_end.y, wall_anchor.z + 12.0 * scale);
    let cup = session.create_cone(cup_base, up, 8.0 * scale, 15.0 * scale, 18.0 * scale)?;
    let fixture = session.create_compound(&[&plate, &arm, &stem, &cup])?;
    session.remove(plate)?;
    session.remove(arm)?;
    session.remove(stem)?;
    session.remove(cup)?;

    let flame_base = Vec3::new(arm_end.x, arm_end.y, wall_anchor.z + 30.0 * scale);
    let lower_flame =
        session.create_cone(flame_base, up, 11.0 * scale, 4.0 * scale, 25.0 * scale)?;
    let upper_flame_base = Vec3::new(flame_base.x, flame_base.y, flame_base.z + 17.0 * scale);
    let upper_flame = session.create_cone(upper_flame_base, up, 6.0 * scale, 0.0, 22.0 * scale)?;
    let flame = session.create_compound(&[&lower_flame, &upper_flame])?;
    session.remove(lower_flame)?;
    session.remove(upper_flame)?;

    Ok(WallTorch {
        fixture,
        flame,
        light: PositionalLight {
            position: Vec3::new(flame_base.x, flame_base.y, flame_base.z + 14.0 * scale),
            color: Vec3::new(1.0, 0.32, 0.06),
            intensity: 2_000_000.0,
            range: 0.0,
            cast_shadows: false,
        },
    })
}

/// Compatibility recipe for the bridge's legacy faceted-stone constructor.
///
/// This remains backed by the ABI until generic sewing and shell-to-solid
/// operations are available in the kernel layer.
#[allow(deprecated)]
pub fn create_faceted_stone<'a>(
    session: &'a Session,
    bottom_points: &[Vec3],
    top_points: &[Vec3],
    top_center: Vec3,
    bottom_chamfer: f64,
    top_fillet: f64,
) -> Result<Shape<'a>, BridgeError> {
    session.create_faceted_stone(
        bottom_points,
        top_points,
        top_center,
        bottom_chamfer,
        top_fillet,
    )
}

fn finite(value: Vec3) -> bool {
    value.x.is_finite() && value.y.is_finite() && value.z.is_finite()
}

fn subtract(left: Vec3, right: Vec3) -> Vec3 {
    Vec3::new(left.x - right.x, left.y - right.y, left.z - right.z)
}

fn magnitude(value: Vec3) -> f64 {
    value.x.hypot(value.y.hypot(value.z))
}

fn invalid_argument(message: &str) -> BridgeError {
    BridgeError {
        status: 1,
        category: "invalid argument".into(),
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composes_wall_torch_from_generic_bridge_operations() {
        let session = Session::new().unwrap();
        let torch = create_wall_torch(
            &session,
            Vec3::new(0.0, 120.0, 130.0),
            Vec3::new(1.0, 0.0, 0.0),
            1.0,
        )
        .unwrap();

        assert!(session.is_valid(&torch.fixture).unwrap());
        assert!(session.is_valid(&torch.flame).unwrap());
        assert!(torch.light.position.x > 40.0);
        assert!(torch.light.position.z > 160.0);
        assert!(!torch.light.cast_shadows);
        assert_eq!(session.shape_count().unwrap(), 2);
    }

    #[test]
    fn rejects_invalid_torch_parameters_before_creating_geometry() {
        let session = Session::new().unwrap();
        let error = create_wall_torch(
            &session,
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
            1.0,
        )
        .unwrap_err();

        assert_eq!(error.status, 1);
        assert_eq!(session.shape_count().unwrap(), 0);
    }

    #[test]
    fn exposes_faceted_stone_as_a_recipe() {
        let session = Session::new().unwrap();
        let bottom = [
            Vec3::new(-20.0, -14.0, 0.0),
            Vec3::new(-7.0, -21.0, 0.0),
            Vec3::new(17.0, -18.0, 0.0),
            Vec3::new(23.0, -2.0, 0.0),
            Vec3::new(17.0, 17.0, 0.0),
            Vec3::new(-3.0, 22.0, 0.0),
            Vec3::new(-24.0, 9.0, 0.0),
        ];
        let top = [
            Vec3::new(-18.0, -12.5, 6.4),
            Vec3::new(-6.0, -19.0, 7.1),
            Vec3::new(15.0, -16.0, 6.7),
            Vec3::new(20.5, -1.5, 7.5),
            Vec3::new(15.0, 15.0, 6.8),
            Vec3::new(-2.5, 19.5, 7.8),
            Vec3::new(-21.0, 8.0, 6.6),
        ];
        let stone =
            create_faceted_stone(&session, &bottom, &top, Vec3::new(0.0, 0.5, 9.0), 0.8, 0.7)
                .unwrap();

        assert!(session.is_valid(&stone).unwrap());
    }
}
