//! Initial-position trees within rigid motion groups, swept trees between them.
use super::*;

pub(super) fn numbers<const N: usize>(values: [f64; N]) -> Vec<u64> {
    values
        .into_iter()
        .map(|value| if value == 0.0 { 0 } else { value.to_bits() })
        .collect()
}
struct Group {
    initial: Node,
    swept: Node,
}
pub(super) struct Index {
    groups: Vec<Group>,
    membership: Vec<usize>,
    tree: Node,
}
fn joined(first: Bounds, second: Bounds) -> Bounds {
    Bounds {
        min: Vec3::new(
            first.min.x.min(second.min.x),
            first.min.y.min(second.min.y),
            first.min.z.min(second.min.z),
        ),
        max: Vec3::new(
            first.max.x.max(second.max.x),
            first.max.y.max(second.max.y),
            first.max.z.max(second.max.z),
        ),
    }
}
impl Index {
    pub(super) fn new(
        bodies: &[Body<'_, '_>],
        swept: &[Body<'_, '_>],
        movements: &[Movement],
    ) -> Self {
        let mut keys = HashMap::new();
        let mut members: Vec<Vec<usize>> = Vec::new();
        let mut membership = Vec::with_capacity(bodies.len());
        for (index, movement) in movements.iter().enumerate() {
            let group = *keys.entry(movement.key()).or_insert_with(|| {
                members.push(Vec::new());
                members.len() - 1
            });
            members[group].push(index);
            membership.push(group);
        }
        let aggregates = members
            .iter()
            .map(|indices| {
                let body = &swept[indices[0]];
                Body {
                    reference: body.reference,
                    shape: body.shape,
                    volume: body.volume,
                    bounds: indices
                        .iter()
                        .map(|&index| swept[index].bounds)
                        .reduce(joined)
                        .unwrap(),
                }
            })
            .collect::<Vec<_>>();
        let groups = members
            .iter_mut()
            .map(|indices| Group {
                initial: Node::build(bodies, indices),
                swept: Node::build(swept, indices),
            })
            .collect();
        let tree = Node::build(&aggregates, &mut (0..members.len()).collect::<Vec<_>>());
        Self {
            groups,
            membership,
            tree,
        }
    }
    pub(super) fn query(
        &self,
        first: usize,
        bodies: &[Body<'_, '_>],
        swept: &[Body<'_, '_>],
        margin: f64,
        exclusions: &PairExclusions,
        output: &mut Vec<usize>,
    ) {
        let own = self.membership[first];
        self.groups[own].initial.query(
            bodies[first].bounds,
            margin,
            &|second| second > first && !exclusions.contains(first, second),
            output,
        );
        let mut groups = Vec::new();
        self.tree.query(
            swept[first].bounds,
            margin,
            &|group| group != own,
            &mut groups,
        );
        for group in groups {
            self.groups[group].swept.query(
                swept[first].bounds,
                margin,
                &|second| second > first && !exclusions.contains(first, second),
                output,
            );
        }
    }
}
