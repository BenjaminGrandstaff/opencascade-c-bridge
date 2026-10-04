//! Conservative box propagation through unwrapped rigid-motion intervals.
use super::*;

pub(super) fn local_bounds(
    bounds: Bounds,
    inverse: &[NormalizedPlacement],
) -> Result<Bounds, ModelError> {
    inverse.iter().try_fold(bounds, |bounds, placement| {
        let points = corners(bounds).map(|point| transform_point(point, placement));
        enclosing(points)
    })
}
pub(super) fn interval_bounds(
    bounds: Bounds,
    steps: &[Step],
    start: f64,
    end: f64,
) -> Result<Bounds, ModelError> {
    steps
        .iter()
        .try_fold(bounds, |bounds, step| step_bounds(bounds, step, start, end))
}
fn step_bounds(bounds: Bounds, step: &Step, start: f64, end: f64) -> Result<Bounds, ModelError> {
    let axis_length = length(step.axis);
    if !axis_length.is_finite() || axis_length <= f64::EPSILON {
        return Err(ModelError::new(
            "continuous box rotation axis is not representable",
        ));
    }
    let rotation_axis = scale(step.axis, 1.0 / axis_length);
    let initial = step.start.rotation.map_or(0.0, |(_, _, angle)| angle);
    let first = initial + step.angular_delta * start;
    let last = initial + step.angular_delta * end;
    let a = add(step.start.translation, scale(step.translation_delta, start));
    let b = add(step.start.translation, scale(step.translation_delta, end));
    let mut minimum = Vec3::new(f64::INFINITY, f64::INFINITY, f64::INFINITY);
    let mut maximum = Vec3::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for point in corners(bounds) {
        let relative = subtract(point, step.pivot);
        let parallel = scale(rotation_axis, dot(rotation_axis, relative));
        let cosine = subtract(relative, parallel);
        let sine = cross(rotation_axis, relative);
        let center = add(step.pivot, parallel);
        for axis in 0..3 {
            let (low, high) = harmonic(component(cosine, axis), component(sine, axis), first, last);
            let low_value =
                component(center, axis) + low + component(a, axis).min(component(b, axis));
            let high_value =
                component(center, axis) + high + component(a, axis).max(component(b, axis));
            if !low_value.is_finite() || !high_value.is_finite() {
                return Err(ModelError::new("continuous interval box overflows"));
            }
            include(&mut minimum, axis, low_value, true);
            include(&mut maximum, axis, high_value, false);
        }
    }
    padded(Bounds {
        min: minimum,
        max: maximum,
    })
}
fn harmonic(a: f64, b: f64, first: f64, last: f64) -> (f64, f64) {
    let radius = a.hypot(b);
    let lower = first.min(last);
    let width = (last - first).abs();
    if width >= std::f64::consts::TAU {
        return (-radius, radius);
    }
    let value = |angle: f64| {
        let (sine, cosine) = angle.sin_cos();
        a * cosine + b * sine
    };
    let mut minimum = value(first).min(value(last));
    let mut maximum = value(first).max(value(last));
    let phase = b.atan2(a);
    if contains_extremum(lower, width, phase) {
        maximum = radius;
    }
    if contains_extremum(lower, width, phase + std::f64::consts::PI) {
        minimum = -radius;
    }
    (minimum, maximum)
}
fn contains_extremum(start: f64, width: f64, phase: f64) -> bool {
    let tau = std::f64::consts::TAU;
    let distance = (phase.rem_euclid(tau) - start.rem_euclid(tau)).rem_euclid(tau);
    // Include critical points near either boundary despite phase reduction error.
    let guard = 128.0 * f64::EPSILON * (start.abs() + width + tau);
    distance <= width + guard || tau - distance <= guard
}
fn corners(bounds: Bounds) -> [Vec3; 8] {
    std::array::from_fn(|index| {
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
}
fn enclosing(points: [Vec3; 8]) -> Result<Bounds, ModelError> {
    let mut min = Vec3::new(f64::INFINITY, f64::INFINITY, f64::INFINITY);
    let mut max = Vec3::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for point in points {
        if !finite(point) {
            return Err(ModelError::new("continuous inverse box overflows"));
        }
        for axis in 0..3 {
            include(&mut min, axis, component(point, axis), true);
            include(&mut max, axis, component(point, axis), false);
        }
    }
    padded(Bounds { min, max })
}
fn padded(bounds: Bounds) -> Result<Bounds, ModelError> {
    let margin = magnitude(bounds.min).max(magnitude(bounds.max)).max(1.0) * (128.0 * f64::EPSILON);
    let extent = Vec3::new(margin, margin, margin);
    let bounds = Bounds {
        min: subtract(bounds.min, extent),
        max: add(bounds.max, extent),
    };
    if !finite(bounds.min) || !finite(bounds.max) {
        return Err(ModelError::new("continuous interval box overflows"));
    }
    Ok(bounds)
}
fn component(point: Vec3, axis: usize) -> f64 {
    match axis {
        0 => point.x,
        1 => point.y,
        _ => point.z,
    }
}
fn set(point: &mut Vec3, axis: usize, value: f64) {
    match axis {
        0 => point.x = value,
        1 => point.y = value,
        _ => point.z = value,
    }
}

fn include(point: &mut Vec3, axis: usize, value: f64, minimum: bool) {
    let present = component(*point, axis);
    set(
        point,
        axis,
        if minimum {
            present.min(value)
        } else {
            present.max(value)
        },
    );
}
