//! Bounded SVG approximation of positive-weight rational Bezier spans.
use super::*;

const MAXIMUM_WORK: usize = 2_000_000;

fn distance(point: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let delta = [b[0] - a[0], b[1] - a[1]];
    let length = delta[0].hypot(delta[1]);
    if length == 0.0 {
        return (point[0] - a[0]).hypot(point[1] - a[1]);
    }
    let unit = delta.map(|v| v / length);
    let along = ((point[0] - a[0]) * unit[0] + (point[1] - a[1]) * unit[1]).clamp(0.0, length);
    (point[0] - a[0] - along * unit[0]).hypot(point[1] - a[1] - along * unit[1])
}

fn split(poles: &[[f64; 2]], weights: &[f64]) -> (Piece, Piece) {
    let mut p = poles.to_vec();
    let mut w = weights.to_vec();
    let mut left = Piece {
        poles: vec![p[0]],
        weights: vec![w[0]],
        depth: 0,
    };
    let mut right = Piece {
        poles: vec![*p.last().unwrap()],
        weights: vec![*w.last().unwrap()],
        depth: 0,
    };
    for count in (1..p.len()).rev() {
        for i in 0..count {
            let blend = w[i + 1] / (w[i] + w[i + 1]);
            p[i] = [
                p[i][0] * (1.0 - blend) + p[i + 1][0] * blend,
                p[i][1] * (1.0 - blend) + p[i + 1][1] * blend,
            ];
            w[i] = w[i] * 0.5 + w[i + 1] * 0.5;
        }
        left.poles.push(p[0]);
        left.weights.push(w[0]);
        right.poles.push(p[count - 1]);
        right.weights.push(w[count - 1]);
    }
    right.poles.reverse();
    right.weights.reverse();
    (left, right)
}
struct Piece {
    poles: Vec<[f64; 2]>,
    weights: Vec<f64>,
    depth: usize,
}

/// Positive-weight curves lie in their control hull. A hull within tolerance of
/// the endpoint segment bounds chord error, including loops and rational spans.
/// Subdivision costs O(d²) per node for degree d; work/depth and output are capped.
/// The bound includes a scale-aware floating-point margin, not interval arithmetic.
pub(super) fn svg_points(
    poles: &[[f64; 2]],
    weights: &[f64],
    tolerance: f64,
    maximum_points: usize,
    work: &mut usize,
) -> Result<Vec<[f64; 2]>, ModelError> {
    if poles.len() <= 4 && weights.iter().all(|w| *w == weights[0]) {
        return Ok(vec![]);
    }
    let maximum = weights.iter().copied().fold(0.0, f64::max);
    let weights: Vec<_> = weights.iter().map(|w| w / maximum).collect();
    let magnitude = poles
        .iter()
        .flatten()
        .copied()
        .map(f64::abs)
        .fold(1.0, f64::max);
    let margin = 128.0 * f64::EPSILON * magnitude;
    if !tolerance.is_finite()
        || tolerance <= margin
        || weights.iter().any(|w| !w.is_finite() || *w < 1e-300)
    {
        return Err(ModelError::new(
            "SVG curve tolerance or weights exceed numerical precision limits",
        ));
    }
    let mut stack = vec![Piece {
        poles: poles.to_vec(),
        weights,
        depth: 0,
    }];
    let mut points = vec![poles[0]];
    while let Some(piece) = stack.pop() {
        *work = work
            .checked_add(piece.poles.len() * piece.poles.len())
            .ok_or_else(|| ModelError::new("SVG curve work overflow"))?;
        if *work > MAXIMUM_WORK {
            return Err(ModelError::new(
                "SVG curves exceed 2000000 subdivision work budget",
            ));
        }
        let a = piece.poles[0];
        let b = *piece.poles.last().unwrap();
        let error = piece
            .poles
            .iter()
            .map(|p| distance(*p, a, b))
            .fold(0.0, f64::max);
        if error.is_finite() && error + margin <= tolerance {
            if points.len() >= maximum_points {
                return Err(ModelError::new("drawing exceeds export vertex budget"));
            }
            points.push(b);
        } else {
            if piece.depth >= 48 {
                return Err(ModelError::new(
                    "SVG curve subdivision exceeds precision/depth limit",
                ));
            }
            let (mut left, mut right) = split(&piece.poles, &piece.weights);
            left.depth = piece.depth + 1;
            right.depth = piece.depth + 1;
            stack.push(right);
            stack.push(left);
        }
    }
    Ok(points)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rational_svg_chords_bound_circle_error_and_high_degree_loops() {
        let poles = [[2.0, 0.0], [2.0, 2.0], [0.0, 2.0]];
        let weights = [1.0, std::f64::consts::FRAC_1_SQRT_2, 1.0];
        let mut work = 0;
        let points = svg_points(&poles, &weights, 0.002, 10000, &mut work).unwrap();
        assert!(points.len() > 10);
        for i in 0..=1000 {
            let angle = std::f64::consts::FRAC_PI_2 * i as f64 / 1000.0;
            let p = [2.0 * angle.cos(), 2.0 * angle.sin()];
            let error = points
                .windows(2)
                .map(|s| distance(p, s[0], s[1]))
                .fold(f64::INFINITY, f64::min);
            assert!(error <= 0.002, "{error}");
        }
        let coarse = svg_points(&poles, &weights, 0.02, 10000, &mut 0).unwrap();
        assert!(coarse.len() < points.len());
        assert!(svg_points(&poles, &weights, 0.002, 2, &mut 0).is_err());
        assert!(svg_points(&poles, &weights, 1e-20, 10000, &mut 0).is_err());
        assert!(svg_points(&poles, &[1.0, 1e-320, 1.0], 0.002, 10000, &mut 0).is_err());
        let mut exhausted = MAXIMUM_WORK;
        assert!(svg_points(&poles, &weights, 0.002, 10000, &mut exhausted).is_err());
        let loop_poles = [
            [0.0, 0.0],
            [2.0, 3.0],
            [5.0, 2.0],
            [-5.0, 2.0],
            [-2.0, 3.0],
            [0.0, 0.0],
        ];
        let polygon = svg_points(&loop_poles, &[1.0; 6], 0.005, 10000, &mut 0).unwrap();
        assert!(polygon.len() > 10);
        assert_eq!(polygon.first(), polygon.last());
        // Independent degree-five Bernstein evaluation, including coincident endpoints.
        for i in 0..=1000 {
            let t = i as f64 / 1000.0;
            let mut p = [0.0; 2];
            for (j, binomial) in [1.0, 5.0, 10.0, 10.0, 5.0, 1.0].iter().enumerate() {
                let b = binomial * t.powi(j as i32) * (1.0 - t).powi((5 - j) as i32);
                p[0] += b * loop_poles[j][0];
                p[1] += b * loop_poles[j][1];
            }
            assert!(
                polygon
                    .windows(2)
                    .map(|s| distance(p, s[0], s[1]))
                    .fold(f64::INFINITY, f64::min)
                    <= 0.005
            );
        }
    }
}
