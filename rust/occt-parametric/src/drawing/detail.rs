//! Rectangular detail clipping of sampled view-local curves.
use super::*;

impl DrawingDetail {
    pub(super) fn validate(self) -> Result<(), ModelError> {
        if !finite_pair(self.minimum_mm)
            || !finite_pair(self.maximum_mm)
            || (0..2).any(|axis| self.minimum_mm[axis] >= self.maximum_mm[axis])
        {
            return Err(ModelError::new(
                "drawing detail needs finite increasing XY bounds",
            ));
        }
        Ok(())
    }
}

fn clip_segment(
    first: [f64; 2],
    second: [f64; 2],
    window: DrawingDetail,
) -> Result<Option<[[f64; 2]; 2]>, ModelError> {
    let delta = [second[0] - first[0], second[1] - first[1]];
    let mut low: f64 = 0.0;
    let mut high: f64 = 1.0;
    for axis in 0..2 {
        if !delta[axis].is_finite() {
            return Err(ModelError::new(
                "detail clipping exceeds finite coordinates",
            ));
        }
        if delta[axis] == 0.0 {
            if first[axis] < window.minimum_mm[axis] || first[axis] > window.maximum_mm[axis] {
                return Ok(None);
            }
        } else {
            let start = (window.minimum_mm[axis] - first[axis]) / delta[axis];
            let end = (window.maximum_mm[axis] - first[axis]) / delta[axis];
            if start.is_nan() || end.is_nan() {
                return Err(ModelError::new("detail clipping has invalid coordinates"));
            }
            low = low.max(start.min(end));
            high = high.min(start.max(end));
        }
    }
    if low > high {
        return Ok(None);
    }
    let point = |fraction: f64| {
        [
            first[0] + delta[0] * fraction,
            first[1] + delta[1] * fraction,
        ]
    };
    Ok(Some([point(low), point(high)]))
}

/// O(sampled segments + clipped vertices) time/storage. Each input segment
/// contributes at most two output vertices; disconnected fragments stay separate.
pub(super) fn clip_polyline(
    points: &[[f64; 2]],
    window: DrawingDetail,
) -> Result<Vec<Vec<[f64; 2]>>, ModelError> {
    let mut paths: Vec<Vec<[f64; 2]>> = Vec::new();
    for segment in points.windows(2) {
        if let Some([first, second]) = clip_segment(segment[0], segment[1], window)? {
            if let Some(path) = paths.last_mut().filter(|path| path.last() == Some(&first)) {
                path.push(second);
            } else {
                paths.push(vec![first, second]);
            }
        }
    }
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clipping_preserves_crossings_parallel_segments_and_disconnected_paths() {
        let window = DrawingDetail {
            minimum_mm: [0.0, 0.0],
            maximum_mm: [10.0, 10.0],
        };
        let paths = clip_polyline(
            &[
                [-5.0, 5.0],
                [15.0, 5.0],
                [15.0, 20.0],
                [5.0, 20.0],
                [5.0, -5.0],
            ],
            window,
        )
        .unwrap();
        assert_eq!(
            paths,
            vec![vec![[0.0, 5.0], [10.0, 5.0]], vec![[5.0, 10.0], [5.0, 0.0]]]
        );
        assert!(
            clip_polyline(&[[-5.0, -1.0], [15.0, -1.0]], window)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            clip_polyline(&[[0.0, 0.0], [10.0, 0.0]], window).unwrap(),
            vec![vec![[0.0, 0.0], [10.0, 0.0]]]
        );
        assert!(clip_polyline(&[[-f64::MAX, 0.0], [f64::MAX, 0.0]], window).is_err());
    }
}
