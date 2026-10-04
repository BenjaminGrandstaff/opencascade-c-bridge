//! Explicit exclusions are validated against the selected solid outputs.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollisionPairRef {
    pub first: InstanceOutputRef,
    pub second: InstanceOutputRef,
}

pub(in crate::assembly) struct PairExclusions(HashSet<(usize, usize)>);
fn ordered(first: usize, second: usize) -> (usize, usize) {
    (first.min(second), first.max(second))
}
impl PairExclusions {
    pub(in crate::assembly) fn new(
        outputs: &[InstanceOutputRef],
        pairs: &[CollisionPairRef],
    ) -> Result<Self, ModelError> {
        if pairs.len() > 1_000_000 {
            return Err(ModelError::new(
                "at most one million collision exclusions are supported",
            ));
        }
        let mut selected = HashMap::new();
        let mut instances = HashSet::new();
        for (index, output) in outputs.iter().enumerate() {
            if !instances.insert(&output.instance) {
                return Err(ModelError::new(
                    "collision checks require one output per distinct instance",
                ));
            }
            selected.insert(output, index);
        }
        let mut excluded = HashSet::with_capacity(pairs.len());
        for pair in pairs {
            let resolve = |output: &InstanceOutputRef| {
                selected.get(output).copied().ok_or_else(|| {
                    ModelError::new(format!(
                        "excluded output '{}:{}' is not selected",
                        output.instance, output.output
                    ))
                })
            };
            let first = resolve(&pair.first)?;
            let second = resolve(&pair.second)?;
            if first == second || !excluded.insert(ordered(first, second)) {
                return Err(ModelError::new(
                    "collision exclusions must be distinct unordered pairs without self-pairs",
                ));
            }
        }
        Ok(Self(excluded))
    }
    pub(in crate::assembly) fn contains(&self, first: usize, second: usize) -> bool {
        self.0.contains(&ordered(first, second))
    }
}
