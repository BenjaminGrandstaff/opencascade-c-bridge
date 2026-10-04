//! Conservative frame ancestry dependencies and bounds on sparse assembly work.
use super::*;

const MAXIMUM_WORK: usize = 1_000_000;

pub(super) struct Influence {
    pub relationships: Vec<Vec<usize>>,
    pub first_rows: Vec<usize>,
}
impl Influence {
    pub fn new(
        graph: &InstanceGraph<'_>,
        free: &[JointVariable],
        length: f64,
    ) -> Result<Self, ModelError> {
        let mut by_frame: HashMap<&str, Vec<usize>> = HashMap::new();
        for (column, variable) in free.iter().enumerate() {
            by_frame.entry(&variable.frame).or_default().push(column);
        }
        let mut instances: HashMap<&str, Vec<usize>> = HashMap::new();
        let mut traversal_work = 0;
        let mut normal_work: usize = 0;
        let mut row_count = 0;
        let mut result = Self {
            relationships: vec![Vec::new(); free.len()],
            first_rows: Vec::with_capacity(graph.assembly.relationships.len()),
        };
        for (index, relation) in graph.assembly.relationships.iter().enumerate() {
            let mut columns = BTreeSet::new();
            for reference in [&relation.first, &relation.second] {
                if !instances.contains_key(reference.instance.as_str()) {
                    let dependent = instance_columns(
                        graph,
                        &reference.instance,
                        &by_frame,
                        &mut traversal_work,
                    )?;
                    instances.insert(&reference.instance, dependent);
                }
                columns.extend(instances[reference.instance.as_str()].iter().copied());
            }
            let mut values = Vec::new();
            term_residuals(
                relation.kind,
                graph.datum(&relation.first.instance, &relation.first.datum)?,
                graph.datum(&relation.second.instance, &relation.second.datum)?,
                length,
                &mut values,
            );
            // Upper bound for J^T J accumulation; actual zero derivatives can
            // lower the cost, but the bound must hold before allocating it.
            normal_work = columns
                .len()
                .checked_mul(columns.len())
                .and_then(|work| work.checked_mul(values.len()))
                .and_then(|work| normal_work.checked_add(work))
                .filter(|work| *work <= MAXIMUM_WORK)
                .ok_or_else(|| ModelError::new("joint solver sparse work budget exceeded"))?;
            result.first_rows.push(row_count);
            row_count += values.len();
            for column in columns {
                result.relationships[column].push(index);
            }
        }
        Ok(result)
    }
}

fn instance_columns(
    graph: &InstanceGraph<'_>,
    instance: &str,
    by_frame: &HashMap<&str, Vec<usize>>,
    traversal_work: &mut usize,
) -> Result<Vec<usize>, ModelError> {
    let node = graph
        .nodes
        .get(instance)
        .ok_or_else(|| ModelError::new("joint relationship instance is missing"))?;
    let mut frame = node.frame();
    let mut dependent = Vec::new();
    while let Some(id) = frame {
        *traversal_work += 1;
        if *traversal_work > MAXIMUM_WORK {
            return Err(ModelError::new(
                "joint solver frame traversal budget exceeded",
            ));
        }
        if let Some(variables) = by_frame.get(id) {
            dependent.extend_from_slice(variables);
        }
        frame = graph
            .frames
            .get(id)
            .ok_or_else(|| ModelError::new("joint relationship frame is missing"))?
            .parent
            .as_deref();
    }
    Ok(dependent)
}
