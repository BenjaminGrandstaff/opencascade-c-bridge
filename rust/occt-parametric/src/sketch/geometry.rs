//! Exact analytic contacts shared by solver equations and viewer diagnostics.
use super::*;

pub(super) fn line_ellipse_contact(
    line: (SketchPoint2, SketchPoint2),
    center: SketchPoint2,
    major: SketchPoint2,
    minor: SketchPoint2,
    side: SketchLineSide,
) -> Result<(f64, [SketchPoint2; 2]), ModelError> {
    let (p, q) = line;
    let length = line_length(line);
    let a = line_length((center, major));
    let b = line_length((center, minor));
    if length <= f64::EPSILON || a <= f64::EPSILON || b <= f64::EPSILON {
        return Err(ModelError::new(
            "line-ellipse tangency requires a nonzero line and positive axes",
        ));
    }
    let nx = -(q.y - p.y) / length;
    let ny = (q.x - p.x) / length;
    let ax = major.x - center.x;
    let ay = major.y - center.y;
    // OCCT orients the minor axis perpendicular to the major axis. Its
    // length, rather than an unsolved orthogonality error, defines the curve.
    let bx = -ay * b / a;
    let by = ax * b / a;
    let an = ax * nx + ay * ny;
    let bn = bx * nx + by * ny;
    let support = an.hypot(bn);
    let sign = match side {
        SketchLineSide::Left => 1.,
        SketchLineSide::Right => -1.,
    };
    let residual = (center.x - p.x) * nx + (center.y - p.y) * ny - sign * support;
    let contact = SketchPoint2 {
        x: center.x - sign * (ax * an + bx * bn) / support,
        y: center.y - sign * (ay * an + by * bn) / support,
    };
    let projected = SketchPoint2 {
        x: contact.x - residual * nx,
        y: contact.y - residual * ny,
    };
    if [residual, contact.x, contact.y, projected.x, projected.y]
        .iter()
        .any(|v| !v.is_finite())
    {
        return Err(ModelError::new("line-ellipse tangency data is not finite"));
    }
    Ok((residual, [projected, contact]))
}
