//! Location-only part patterns. Bookkeeping O(N), topology validation O(N*T).
use super::*;

const MAX_COPIES: usize = 10_000;
const MAX_ESTIMATED_TOPOLOGY: usize = 1_000_000;

pub(super) fn linear<'a>(
    session: &'a Session,
    source: &Shape<'_>,
    step: Vec3,
    count: f64,
) -> Result<Shape<'a>, ModelError> {
    if !count.is_finite() || count.fract() != 0.0 || !(1.0..=MAX_COPIES as f64).contains(&count) {
        return Err(ModelError::new(
            "linear pattern count must be an integer-valued scalar from 1 to 10000",
        ));
    }
    let count = count as usize;
    if count > 1 && step.x == 0.0 && step.y == 0.0 && step.z == 0.0 {
        return Err(ModelError::new(
            "linear pattern step must be nonzero for multiple copies",
        ));
    }
    // Bound nested patterns before placing copies. This is a conservative
    // sum of unique subshape counts; shared/coincident topology can cost less.
    let mut topology = 0usize;
    for kind in [
        ShapeType::Compound,
        ShapeType::CompSolid,
        ShapeType::Solid,
        ShapeType::Shell,
        ShapeType::Face,
        ShapeType::Wire,
        ShapeType::Edge,
        ShapeType::Vertex,
    ] {
        topology = topology.saturating_add(session.subshape_count(source, kind)?);
    }
    if topology.saturating_add(1).saturating_mul(count) > MAX_ESTIMATED_TOPOLOGY {
        return Err(ModelError::new(
            "linear pattern exceeds the 1000000 estimated topology limit",
        ));
    }
    let mut copies = Vec::with_capacity(count);
    for i in 0..count {
        let i = i as f64;
        let offset = Vec3::new(step.x * i, step.y * i, step.z * i);
        if !offset.x.is_finite() || !offset.y.is_finite() || !offset.z.is_finite() {
            return Err(ModelError::new("linear pattern placement overflow"));
        }
        copies.push(session.translate(source, offset)?);
    }
    let refs = copies.iter().collect::<Vec<_>>();
    let result = session.create_compound(&refs)?;
    if !session.is_valid(&result)? {
        return Err(ModelError::new("linear pattern contains invalid geometry"));
    }
    Ok(result)
}
