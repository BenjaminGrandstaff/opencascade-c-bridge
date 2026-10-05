//! Parameter resolution and scalar and vector expression evaluation.

use super::*;

pub(crate) fn resolve_parameters(
    definition: &FamilyDefinition,
    overrides: &HashMap<String, ParameterValue>,
) -> Result<HashMap<String, ParameterValue>, ModelError> {
    let mut resolved = HashMap::new();
    for parameter in &definition.parameters {
        let value = overrides.get(&parameter.id).unwrap_or(&parameter.default);
        validate_parameter(parameter, value)?;
        resolved.insert(parameter.id.clone(), value.clone());
    }
    for name in overrides.keys() {
        if !resolved.contains_key(name) {
            return Err(ModelError::new(format!(
                "unknown parameter override '{name}'"
            )));
        }
    }
    let derived = definition
        .derived_parameters
        .iter()
        .map(|parameter| (parameter.id.as_str(), parameter))
        .collect::<HashMap<_, _>>();
    for parameter in &definition.derived_parameters {
        resolve_derived_parameter(&parameter.id, &derived, &mut resolved, &mut Vec::new())?;
    }
    let derived_vectors = definition
        .derived_vector_parameters
        .iter()
        .map(|parameter| (parameter.id.as_str(), parameter))
        .collect::<HashMap<_, _>>();
    for parameter in &definition.derived_vector_parameters {
        resolve_derived_vector_parameter(
            &parameter.id,
            &derived_vectors,
            &mut resolved,
            &mut Vec::new(),
        )?;
    }
    for constraint in &definition.constraints {
        validate_constraint(constraint, &resolved)?;
    }
    Ok(resolved)
}

#[derive(Clone, Copy)]
pub(crate) struct EvaluatedScalar {
    pub(crate) value: f64,
    pub(crate) dimension: Dimension,
}

impl EvaluatedScalar {
    pub(crate) fn from_quantity(quantity: Quantity) -> Result<Self, ModelError> {
        Ok(Self {
            value: quantity.normalized()?,
            dimension: quantity.dimension,
        })
    }

    pub(crate) fn into_parameter_value(self) -> ParameterValue {
        let quantity = match self.dimension {
            Dimension::Scalar => Quantity::scalar(self.value),
            Dimension::Length => Quantity::length(self.value, LengthUnit::Millimeter),
        };
        ParameterValue::Scalar(quantity)
    }
}

pub(crate) fn resolve_derived_parameter(
    name: &str,
    definitions: &HashMap<&str, &DerivedParameterDefinition>,
    resolved: &mut HashMap<String, ParameterValue>,
    visiting: &mut Vec<String>,
) -> Result<EvaluatedScalar, ModelError> {
    if let Some(value) = resolved.get(name) {
        return evaluated_parameter(name, value);
    }
    if let Some(position) = visiting.iter().position(|item| item == name) {
        let mut cycle = visiting[position..].to_vec();
        cycle.push(name.into());
        return Err(ModelError::new(format!(
            "derived parameter cycle: {}",
            cycle.join(" -> ")
        )));
    }
    let definition = definitions
        .get(name)
        .ok_or_else(|| ModelError::new(format!("unknown parameter '{name}'")))?;
    visiting.push(name.into());
    let value =
        evaluate_derived_expression(&definition.expression, definitions, resolved, visiting)?;
    visiting.pop();
    if value.dimension != definition.dimension {
        return Err(ModelError::new(format!(
            "derived parameter '{}' has the wrong dimension",
            definition.id
        )));
    }
    resolved.insert(definition.id.clone(), value.into_parameter_value());
    Ok(value)
}

pub(crate) fn evaluate_derived_expression(
    expression: &ScalarExpr,
    definitions: &HashMap<&str, &DerivedParameterDefinition>,
    resolved: &mut HashMap<String, ParameterValue>,
    visiting: &mut Vec<String>,
) -> Result<EvaluatedScalar, ModelError> {
    match expression {
        ScalarExpr::Literal(value) => EvaluatedScalar::from_quantity(*value),
        ScalarExpr::Iso273ClearanceV1 {
            nominal_diameter,
            series,
        } => {
            let nominal =
                evaluate_derived_expression(nominal_diameter, definitions, resolved, visiting)?;
            clearance_scalar(nominal, *series)
        }
        ScalarExpr::CarrLaneTapDrillV1 {
            nominal_diameter,
            pitch,
            system,
        } => {
            let nominal =
                evaluate_derived_expression(nominal_diameter, definitions, resolved, visiting)?;
            let pitch = evaluate_derived_expression(pitch, definitions, resolved, visiting)?;
            crate::hole_sizes::tap_scalar(*system, nominal, pitch)
        }
        ScalarExpr::CarrLaneSocketHeadV1 {
            nominal_diameter,
            system,
            dimension,
        } => {
            let nominal =
                evaluate_derived_expression(nominal_diameter, definitions, resolved, visiting)?;
            crate::hole_sizes::socket_scalar(*system, nominal, *dimension)
        }
        ScalarExpr::Parameter(name) => {
            resolve_derived_parameter(name, definitions, resolved, visiting)
        }
        ScalarExpr::Negate(value) => negate_scalar(evaluate_derived_expression(
            value,
            definitions,
            resolved,
            visiting,
        )?),
        ScalarExpr::Absolute(value) => absolute_scalar(evaluate_derived_expression(
            value,
            definitions,
            resolved,
            visiting,
        )?),
        ScalarExpr::Add(left, right) => combine_scalars(
            evaluate_derived_expression(left, definitions, resolved, visiting)?,
            evaluate_derived_expression(right, definitions, resolved, visiting)?,
            ScalarOperator::Add,
        ),
        ScalarExpr::Subtract(left, right) => combine_scalars(
            evaluate_derived_expression(left, definitions, resolved, visiting)?,
            evaluate_derived_expression(right, definitions, resolved, visiting)?,
            ScalarOperator::Subtract,
        ),
        ScalarExpr::Multiply(left, right) => combine_scalars(
            evaluate_derived_expression(left, definitions, resolved, visiting)?,
            evaluate_derived_expression(right, definitions, resolved, visiting)?,
            ScalarOperator::Multiply,
        ),
        ScalarExpr::Divide(left, right) => combine_scalars(
            evaluate_derived_expression(left, definitions, resolved, visiting)?,
            evaluate_derived_expression(right, definitions, resolved, visiting)?,
            ScalarOperator::Divide,
        ),
        ScalarExpr::Minimum(left, right) => min_or_max_scalars(
            evaluate_derived_expression(left, definitions, resolved, visiting)?,
            evaluate_derived_expression(right, definitions, resolved, visiting)?,
            false,
        ),
        ScalarExpr::Maximum(left, right) => min_or_max_scalars(
            evaluate_derived_expression(left, definitions, resolved, visiting)?,
            evaluate_derived_expression(right, definitions, resolved, visiting)?,
            true,
        ),
        ScalarExpr::Clamp {
            value,
            minimum,
            maximum,
        } => clamp_scalar(
            evaluate_derived_expression(value, definitions, resolved, visiting)?,
            evaluate_derived_expression(minimum, definitions, resolved, visiting)?,
            evaluate_derived_expression(maximum, definitions, resolved, visiting)?,
        ),
        ScalarExpr::Conditional {
            left,
            relation,
            right,
            when_true,
            when_false,
        } => conditional_scalar(
            evaluate_derived_expression(left, definitions, resolved, visiting)?,
            relation,
            evaluate_derived_expression(right, definitions, resolved, visiting)?,
            evaluate_derived_expression(when_true, definitions, resolved, visiting)?,
            evaluate_derived_expression(when_false, definitions, resolved, visiting)?,
        ),
        // Vector operands of derived scalars may use only input vector
        // parameters: derived vectors are resolved after derived scalars.
        function @ (ScalarExpr::VectorLength(_) | ScalarExpr::DotProduct(..)) => evaluate_function(
            function,
            &mut |_| Err(ModelError::new("vector function has no scalar operand")),
            &mut |operand| {
                evaluate_resolved_vector_expression(operand, resolved).map_err(|error| {
                    ModelError::new(format!(
                        "{error} (derived scalar parameters may use only input vector parameters)"
                    ))
                })
            },
        )
        .expect("vector functions are evaluated"),
        function => evaluate_function(
            function,
            &mut |operand| evaluate_derived_expression(operand, definitions, resolved, visiting),
            &mut |_| Err(ModelError::new("scalar function has no vector operand")),
        )
        .expect("every other expression is matched above"),
    }
}

#[derive(Clone, Copy)]
pub(crate) enum ScalarOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
}

pub(crate) fn combine_scalars(
    left: EvaluatedScalar,
    right: EvaluatedScalar,
    operator: ScalarOperator,
) -> Result<EvaluatedScalar, ModelError> {
    let (value, dimension) = match operator {
        ScalarOperator::Add | ScalarOperator::Subtract => {
            if left.dimension != right.dimension {
                return Err(ModelError::new(
                    "addition and subtraction require matching dimensions",
                ));
            }
            let value = if matches!(operator, ScalarOperator::Add) {
                left.value + right.value
            } else {
                left.value - right.value
            };
            (value, left.dimension)
        }
        ScalarOperator::Multiply => match (left.dimension, right.dimension) {
            (Dimension::Scalar, dimension) => (left.value * right.value, dimension),
            (dimension, Dimension::Scalar) => (left.value * right.value, dimension),
            _ => {
                return Err(ModelError::new(
                    "multiplication requires at least one scalar operand",
                ));
            }
        },
        ScalarOperator::Divide => {
            if right.value == 0.0 {
                return Err(ModelError::new("division by zero in scalar expression"));
            }
            match (left.dimension, right.dimension) {
                (dimension, Dimension::Scalar) => (left.value / right.value, dimension),
                (left_dimension, right_dimension) if left_dimension == right_dimension => {
                    (left.value / right.value, Dimension::Scalar)
                }
                _ => {
                    return Err(ModelError::new(
                        "division requires a scalar divisor or matching dimensions",
                    ));
                }
            }
        }
    };
    if !value.is_finite() {
        return Err(ModelError::new("scalar expression result is not finite"));
    }
    Ok(EvaluatedScalar { value, dimension })
}

pub(crate) fn negate_scalar(value: EvaluatedScalar) -> Result<EvaluatedScalar, ModelError> {
    let negated = -value.value;
    if !negated.is_finite() {
        return Err(ModelError::new("scalar expression result is not finite"));
    }
    Ok(EvaluatedScalar {
        value: negated,
        dimension: value.dimension,
    })
}

pub(crate) fn absolute_scalar(value: EvaluatedScalar) -> Result<EvaluatedScalar, ModelError> {
    let absolute = value.value.abs();
    if !absolute.is_finite() {
        return Err(ModelError::new("scalar expression result is not finite"));
    }
    Ok(EvaluatedScalar {
        value: absolute,
        dimension: value.dimension,
    })
}

pub(crate) fn min_or_max_scalars(
    left: EvaluatedScalar,
    right: EvaluatedScalar,
    maximum: bool,
) -> Result<EvaluatedScalar, ModelError> {
    if left.dimension != right.dimension {
        return Err(ModelError::new(
            "minimum and maximum require matching dimensions",
        ));
    }
    Ok(EvaluatedScalar {
        value: if maximum {
            left.value.max(right.value)
        } else {
            left.value.min(right.value)
        },
        dimension: left.dimension,
    })
}

pub(crate) fn clamp_scalar(
    value: EvaluatedScalar,
    minimum: EvaluatedScalar,
    maximum: EvaluatedScalar,
) -> Result<EvaluatedScalar, ModelError> {
    if value.dimension != minimum.dimension || value.dimension != maximum.dimension {
        return Err(ModelError::new("clamp requires matching dimensions"));
    }
    if minimum.value > maximum.value {
        return Err(ModelError::new("clamp minimum must not exceed its maximum"));
    }
    Ok(EvaluatedScalar {
        value: value.value.clamp(minimum.value, maximum.value),
        dimension: value.dimension,
    })
}

pub(crate) fn compare_scalars(
    left: EvaluatedScalar,
    relation: &ConstraintRelation,
    right: EvaluatedScalar,
    subject: &str,
) -> Result<bool, ModelError> {
    if left.dimension != right.dimension {
        return Err(ModelError::new(format!(
            "{subject} compares different dimensions"
        )));
    }
    match relation {
        ConstraintRelation::LessOrEqual => Ok(left.value <= right.value),
        ConstraintRelation::GreaterOrEqual => Ok(left.value >= right.value),
        ConstraintRelation::Equal { tolerance } => {
            if tolerance.dimension != left.dimension {
                return Err(ModelError::new(format!(
                    "{subject} tolerance has the wrong dimension"
                )));
            }
            let tolerance = tolerance.normalized()?;
            if tolerance < 0.0 {
                return Err(ModelError::new(format!("{subject} tolerance is negative")));
            }
            Ok((left.value - right.value).abs() <= tolerance)
        }
    }
}

pub(crate) fn conditional_scalar(
    left: EvaluatedScalar,
    relation: &ConstraintRelation,
    right: EvaluatedScalar,
    when_true: EvaluatedScalar,
    when_false: EvaluatedScalar,
) -> Result<EvaluatedScalar, ModelError> {
    if when_true.dimension != when_false.dimension {
        return Err(ModelError::new(
            "conditional expression branches require matching dimensions",
        ));
    }
    if compare_scalars(left, relation, right, "conditional expression")? {
        Ok(when_true)
    } else {
        Ok(when_false)
    }
}

pub(crate) fn evaluated_parameter(
    name: &str,
    value: &ParameterValue,
) -> Result<EvaluatedScalar, ModelError> {
    match value {
        ParameterValue::Scalar(quantity) => EvaluatedScalar::from_quantity(*quantity),
        _ => Err(ModelError::new(format!("parameter '{name}' is not scalar"))),
    }
}

#[derive(Clone, Copy)]
pub(crate) struct EvaluatedVector {
    pub(crate) value: Vec3,
    pub(crate) dimension: Dimension,
}

impl EvaluatedVector {
    pub(crate) fn from_quantity(vector: VectorQuantity) -> Result<Self, ModelError> {
        let dimension = vector.x.dimension;
        Ok(Self {
            value: vector.normalized(dimension)?,
            dimension,
        })
    }

    pub(crate) fn into_parameter_value(self) -> ParameterValue {
        let vector = match self.dimension {
            Dimension::Scalar => VectorQuantity::scalars(self.value.x, self.value.y, self.value.z),
            Dimension::Length => VectorQuantity::lengths(
                self.value.x,
                self.value.y,
                self.value.z,
                LengthUnit::Millimeter,
            ),
        };
        ParameterValue::Vector(vector)
    }
}

pub(crate) fn evaluated_vector_parameter(
    name: &str,
    value: &ParameterValue,
) -> Result<EvaluatedVector, ModelError> {
    match value {
        ParameterValue::Vector(vector) => EvaluatedVector::from_quantity(*vector),
        _ => Err(ModelError::new(format!(
            "parameter '{name}' is not a vector"
        ))),
    }
}

pub(crate) fn resolve_derived_vector_parameter(
    name: &str,
    definitions: &HashMap<&str, &DerivedVectorParameterDefinition>,
    resolved: &mut HashMap<String, ParameterValue>,
    visiting: &mut Vec<String>,
) -> Result<EvaluatedVector, ModelError> {
    if let Some(value) = resolved.get(name) {
        return evaluated_vector_parameter(name, value);
    }
    if let Some(position) = visiting.iter().position(|item| item == name) {
        let mut cycle = visiting[position..].to_vec();
        cycle.push(name.into());
        return Err(ModelError::new(format!(
            "derived vector parameter cycle: {}",
            cycle.join(" -> ")
        )));
    }
    let definition = definitions
        .get(name)
        .ok_or_else(|| ModelError::new(format!("unknown vector parameter '{name}'")))?;
    visiting.push(name.into());
    let value = evaluate_derived_vector_expression(
        &definition.expression,
        definitions,
        resolved,
        visiting,
    )?;
    visiting.pop();
    if value.dimension != definition.dimension {
        return Err(ModelError::new(format!(
            "derived vector parameter '{}' has the wrong dimension",
            definition.id
        )));
    }
    resolved.insert(definition.id.clone(), value.into_parameter_value());
    Ok(value)
}

pub(crate) fn evaluate_derived_vector_expression(
    expression: &VectorExpr,
    definitions: &HashMap<&str, &DerivedVectorParameterDefinition>,
    resolved: &mut HashMap<String, ParameterValue>,
    visiting: &mut Vec<String>,
) -> Result<EvaluatedVector, ModelError> {
    match expression {
        VectorExpr::Literal(value) => EvaluatedVector::from_quantity(*value),
        VectorExpr::Parameter(name) => match resolved.get(name) {
            Some(value) => evaluated_vector_parameter(name, value),
            None => resolve_derived_vector_parameter(name, definitions, resolved, visiting),
        },
        VectorExpr::Components { x, y, z } => vector_from_components(
            evaluate_resolved_expression(x, resolved)?,
            evaluate_resolved_expression(y, resolved)?,
            evaluate_resolved_expression(z, resolved)?,
        ),
        VectorExpr::Add(left, right) => combine_vectors(
            evaluate_derived_vector_expression(left, definitions, resolved, visiting)?,
            evaluate_derived_vector_expression(right, definitions, resolved, visiting)?,
            false,
        ),
        VectorExpr::Subtract(left, right) => combine_vectors(
            evaluate_derived_vector_expression(left, definitions, resolved, visiting)?,
            evaluate_derived_vector_expression(right, definitions, resolved, visiting)?,
            true,
        ),
        VectorExpr::Scale { vector, factor } => scale_vector(
            evaluate_derived_vector_expression(vector, definitions, resolved, visiting)?,
            evaluate_resolved_expression(factor, resolved)?,
        ),
        VectorExpr::Normalize(vector) => normalize_vector(evaluate_derived_vector_expression(
            vector,
            definitions,
            resolved,
            visiting,
        )?),
    }
}

pub(crate) fn vector_from_components(
    x: EvaluatedScalar,
    y: EvaluatedScalar,
    z: EvaluatedScalar,
) -> Result<EvaluatedVector, ModelError> {
    if x.dimension != y.dimension || x.dimension != z.dimension {
        return Err(ModelError::new(
            "vector components require matching dimensions",
        ));
    }
    Ok(EvaluatedVector {
        value: Vec3::new(x.value, y.value, z.value),
        dimension: x.dimension,
    })
}

pub(crate) fn combine_vectors(
    left: EvaluatedVector,
    right: EvaluatedVector,
    subtract: bool,
) -> Result<EvaluatedVector, ModelError> {
    if left.dimension != right.dimension {
        return Err(ModelError::new(
            "vector addition and subtraction require matching dimensions",
        ));
    }
    let sign = if subtract { -1.0 } else { 1.0 };
    Ok(EvaluatedVector {
        value: Vec3::new(
            left.value.x + sign * right.value.x,
            left.value.y + sign * right.value.y,
            left.value.z + sign * right.value.z,
        ),
        dimension: left.dimension,
    })
}

pub(crate) fn scale_vector(
    vector: EvaluatedVector,
    factor: EvaluatedScalar,
) -> Result<EvaluatedVector, ModelError> {
    if factor.dimension != Dimension::Scalar {
        return Err(ModelError::new("vector scale factor must be scalar"));
    }
    let value = Vec3::new(
        vector.value.x * factor.value,
        vector.value.y * factor.value,
        vector.value.z * factor.value,
    );
    if !value.x.is_finite() || !value.y.is_finite() || !value.z.is_finite() {
        return Err(ModelError::new("vector expression result is not finite"));
    }
    Ok(EvaluatedVector {
        value,
        dimension: vector.dimension,
    })
}

pub(crate) fn normalize_vector(vector: EvaluatedVector) -> Result<EvaluatedVector, ModelError> {
    let magnitude = vector.value.x.hypot(vector.value.y.hypot(vector.value.z));
    if magnitude <= f64::EPSILON {
        return Err(ModelError::new("cannot normalize a zero vector"));
    }
    Ok(EvaluatedVector {
        value: Vec3::new(
            vector.value.x / magnitude,
            vector.value.y / magnitude,
            vector.value.z / magnitude,
        ),
        dimension: Dimension::Scalar,
    })
}

pub(crate) fn validate_constraint(
    constraint: &ParameterConstraint,
    parameters: &HashMap<String, ParameterValue>,
) -> Result<(), ModelError> {
    let left = evaluate_resolved_expression(&constraint.left, parameters)?;
    let right = evaluate_resolved_expression(&constraint.right, parameters)?;
    let passed = compare_scalars(
        left,
        &constraint.relation,
        right,
        &format!("constraint '{}'", constraint.id),
    )?;
    if passed {
        Ok(())
    } else {
        Err(ModelError::new(format!(
            "constraint '{}' failed: {}",
            constraint.id, constraint.statement
        )))
    }
}

pub(crate) fn validate_parameter(
    definition: &ParameterDefinition,
    value: &ParameterValue,
) -> Result<(), ModelError> {
    match (&definition.parameter_type, value) {
        (ParameterType::Scalar(dimension), ParameterValue::Scalar(quantity)) => {
            if quantity.dimension != *dimension {
                return Err(ModelError::new(format!(
                    "parameter '{}' has the wrong dimension",
                    definition.id
                )));
            }
            let normalized = quantity.normalized()?;
            check_parameter_bound(
                definition,
                *dimension,
                definition.minimum,
                "minimum",
                |bound| (normalized < bound).then_some("below"),
            )?;
            check_parameter_bound(
                definition,
                *dimension,
                definition.maximum,
                "maximum",
                |bound| (normalized > bound).then_some("above"),
            )
        }
        (ParameterType::Vector(dimension), ParameterValue::Vector(vector)) => {
            vector.normalized(*dimension).map(|_| ())
        }
        (ParameterType::Integer, ParameterValue::Integer(_))
        | (ParameterType::Boolean, ParameterValue::Boolean(_)) => Ok(()),
        (ParameterType::Choice(options), ParameterValue::Choice(choice))
            if options.contains(choice) =>
        {
            Ok(())
        }
        _ => Err(ModelError::new(format!(
            "parameter '{}' has the wrong value type",
            definition.id
        ))),
    }
}

/// Checks one optional bound; `violated` returns "below" or "above" when the
/// normalized value lies outside the normalized bound.
pub(crate) fn check_parameter_bound(
    definition: &ParameterDefinition,
    dimension: Dimension,
    bound: Option<Quantity>,
    name: &str,
    violated: impl Fn(f64) -> Option<&'static str>,
) -> Result<(), ModelError> {
    let Some(bound) = bound else {
        return Ok(());
    };
    if bound.dimension != dimension {
        return Err(ModelError::new(format!(
            "parameter '{}' {name} has the wrong dimension",
            definition.id
        )));
    }
    match violated(bound.normalized()?) {
        Some(side) => Err(ModelError::new(format!(
            "parameter '{}' is {side} its {name}",
            definition.id
        ))),
        None => Ok(()),
    }
}

pub(crate) fn scalar(
    expression: &ScalarExpr,
    parameters: &HashMap<String, ParameterValue>,
    dimension: Dimension,
) -> Result<f64, ModelError> {
    let value = evaluate_resolved_expression(expression, parameters)?;
    if value.dimension != dimension {
        return Err(ModelError::new("expression has the wrong dimension"));
    }
    Ok(value.value)
}

pub(crate) fn evaluate_resolved_expression(
    expression: &ScalarExpr,
    parameters: &HashMap<String, ParameterValue>,
) -> Result<EvaluatedScalar, ModelError> {
    match expression {
        ScalarExpr::Literal(value) => EvaluatedScalar::from_quantity(*value),
        ScalarExpr::Iso273ClearanceV1 {
            nominal_diameter,
            series,
        } => clearance_scalar(
            evaluate_resolved_expression(nominal_diameter, parameters)?,
            *series,
        ),
        ScalarExpr::CarrLaneTapDrillV1 {
            nominal_diameter,
            pitch,
            system,
        } => crate::hole_sizes::tap_scalar(
            *system,
            evaluate_resolved_expression(nominal_diameter, parameters)?,
            evaluate_resolved_expression(pitch, parameters)?,
        ),
        ScalarExpr::CarrLaneSocketHeadV1 {
            nominal_diameter,
            system,
            dimension,
        } => crate::hole_sizes::socket_scalar(
            *system,
            evaluate_resolved_expression(nominal_diameter, parameters)?,
            *dimension,
        ),
        ScalarExpr::Parameter(name) => parameters
            .get(name)
            .ok_or_else(|| ModelError::new(format!("unknown parameter '{name}'")))
            .and_then(|value| evaluated_parameter(name, value)),
        ScalarExpr::Negate(value) => {
            negate_scalar(evaluate_resolved_expression(value, parameters)?)
        }
        ScalarExpr::Absolute(value) => {
            absolute_scalar(evaluate_resolved_expression(value, parameters)?)
        }
        ScalarExpr::Add(left, right) => combine_scalars(
            evaluate_resolved_expression(left, parameters)?,
            evaluate_resolved_expression(right, parameters)?,
            ScalarOperator::Add,
        ),
        ScalarExpr::Subtract(left, right) => combine_scalars(
            evaluate_resolved_expression(left, parameters)?,
            evaluate_resolved_expression(right, parameters)?,
            ScalarOperator::Subtract,
        ),
        ScalarExpr::Multiply(left, right) => combine_scalars(
            evaluate_resolved_expression(left, parameters)?,
            evaluate_resolved_expression(right, parameters)?,
            ScalarOperator::Multiply,
        ),
        ScalarExpr::Divide(left, right) => combine_scalars(
            evaluate_resolved_expression(left, parameters)?,
            evaluate_resolved_expression(right, parameters)?,
            ScalarOperator::Divide,
        ),
        ScalarExpr::Minimum(left, right) => min_or_max_scalars(
            evaluate_resolved_expression(left, parameters)?,
            evaluate_resolved_expression(right, parameters)?,
            false,
        ),
        ScalarExpr::Maximum(left, right) => min_or_max_scalars(
            evaluate_resolved_expression(left, parameters)?,
            evaluate_resolved_expression(right, parameters)?,
            true,
        ),
        ScalarExpr::Clamp {
            value,
            minimum,
            maximum,
        } => clamp_scalar(
            evaluate_resolved_expression(value, parameters)?,
            evaluate_resolved_expression(minimum, parameters)?,
            evaluate_resolved_expression(maximum, parameters)?,
        ),
        ScalarExpr::Conditional {
            left,
            relation,
            right,
            when_true,
            when_false,
        } => conditional_scalar(
            evaluate_resolved_expression(left, parameters)?,
            relation,
            evaluate_resolved_expression(right, parameters)?,
            evaluate_resolved_expression(when_true, parameters)?,
            evaluate_resolved_expression(when_false, parameters)?,
        ),
        function => evaluate_function(
            function,
            &mut |operand| evaluate_resolved_expression(operand, parameters),
            &mut |operand| evaluate_resolved_vector_expression(operand, parameters),
        )
        .expect("every other expression is matched above"),
    }
}

pub(crate) fn vector(
    expression: &VectorExpr,
    parameters: &HashMap<String, ParameterValue>,
    dimension: Dimension,
) -> Result<Vec3, ModelError> {
    let value = evaluate_resolved_vector_expression(expression, parameters)?;
    if value.dimension != dimension {
        return Err(ModelError::new("vector expression has the wrong dimension"));
    }
    Ok(value.value)
}

pub(crate) fn evaluate_resolved_vector_expression(
    expression: &VectorExpr,
    parameters: &HashMap<String, ParameterValue>,
) -> Result<EvaluatedVector, ModelError> {
    match expression {
        VectorExpr::Literal(value) => EvaluatedVector::from_quantity(*value),
        VectorExpr::Parameter(name) => parameters
            .get(name)
            .ok_or_else(|| ModelError::new(format!("unknown parameter '{name}'")))
            .and_then(|value| evaluated_vector_parameter(name, value)),
        VectorExpr::Components { x, y, z } => vector_from_components(
            evaluate_resolved_expression(x, parameters)?,
            evaluate_resolved_expression(y, parameters)?,
            evaluate_resolved_expression(z, parameters)?,
        ),
        VectorExpr::Add(left, right) => combine_vectors(
            evaluate_resolved_vector_expression(left, parameters)?,
            evaluate_resolved_vector_expression(right, parameters)?,
            false,
        ),
        VectorExpr::Subtract(left, right) => combine_vectors(
            evaluate_resolved_vector_expression(left, parameters)?,
            evaluate_resolved_vector_expression(right, parameters)?,
            true,
        ),
        VectorExpr::Scale { vector, factor } => scale_vector(
            evaluate_resolved_vector_expression(vector, parameters)?,
            evaluate_resolved_expression(factor, parameters)?,
        ),
        VectorExpr::Normalize(vector) => {
            normalize_vector(evaluate_resolved_vector_expression(vector, parameters)?)
        }
    }
}

/// Evaluates the mathematical functions shared by both expression contexts;
/// `scalar` and `vector` evaluate operands in the caller's context. `None`
/// for any other expression.
pub(crate) fn evaluate_function(
    expression: &ScalarExpr,
    scalar: &mut dyn FnMut(&ScalarExpr) -> Result<EvaluatedScalar, ModelError>,
    vector: &mut dyn FnMut(&VectorExpr) -> Result<EvaluatedVector, ModelError>,
) -> Option<Result<EvaluatedScalar, ModelError>> {
    let dimensionless = |value: EvaluatedScalar, what: &str| {
        if value.dimension == Dimension::Scalar {
            Ok(value.value)
        } else {
            Err(ModelError::new(format!(
                "{what} requires a dimensionless value"
            )))
        }
    };
    let same = |left: EvaluatedScalar, right: EvaluatedScalar, what: &str| {
        if left.dimension == right.dimension {
            Ok(left.dimension)
        } else {
            Err(ModelError::new(format!(
                "{what} requires matching dimensions"
            )))
        }
    };
    let result = (|| -> Result<Option<(f64, Dimension)>, ModelError> {
        Ok(Some(match expression {
            ScalarExpr::SquareRoot(value) => {
                let value = dimensionless(scalar(value)?, "square root")?;
                if value < 0.0 {
                    return Err(ModelError::new("square root of a negative value"));
                }
                (value.sqrt(), Dimension::Scalar)
            }
            ScalarExpr::Power { base, exponent } => (
                dimensionless(scalar(base)?, "power")?
                    .powf(dimensionless(scalar(exponent)?, "power")?),
                Dimension::Scalar,
            ),
            ScalarExpr::Hypotenuse(left, right) => {
                let (left, right) = (scalar(left)?, scalar(right)?);
                (
                    left.value.hypot(right.value),
                    same(left, right, "hypotenuse")?,
                )
            }
            ScalarExpr::Sine(angle) => (
                dimensionless(scalar(angle)?, "sine")?.sin(),
                Dimension::Scalar,
            ),
            ScalarExpr::Cosine(angle) => (
                dimensionless(scalar(angle)?, "cosine")?.cos(),
                Dimension::Scalar,
            ),
            ScalarExpr::Tangent(angle) => (
                dimensionless(scalar(angle)?, "tangent")?.tan(),
                Dimension::Scalar,
            ),
            ScalarExpr::ArcSine(value) | ScalarExpr::ArcCosine(value) => {
                let sine = matches!(expression, ScalarExpr::ArcSine(_));
                let value = dimensionless(scalar(value)?, "inverse sine or cosine")?;
                if !(-1.0..=1.0).contains(&value) {
                    return Err(ModelError::new(
                        "inverse sine or cosine requires a value in [-1, 1]",
                    ));
                }
                (
                    if sine { value.asin() } else { value.acos() },
                    Dimension::Scalar,
                )
            }
            ScalarExpr::ArcTangent2 { y, x } => {
                let (y, x) = (scalar(y)?, scalar(x)?);
                same(y, x, "two-argument arctangent")?;
                if y.value == 0.0 && x.value == 0.0 {
                    return Err(ModelError::new(
                        "two-argument arctangent of a zero direction",
                    ));
                }
                (y.value.atan2(x.value), Dimension::Scalar)
            }
            ScalarExpr::Interpolate { from, to, fraction } => {
                let (from, to) = (scalar(from)?, scalar(to)?);
                let dimension = same(from, to, "interpolation")?;
                let fraction = dimensionless(scalar(fraction)?, "interpolation fraction")?;
                (from.value + (to.value - from.value) * fraction, dimension)
            }
            ScalarExpr::RoundToStep { value, step, mode } => {
                let (value, step) = (scalar(value)?, scalar(step)?);
                let dimension = same(value, step, "rounding to a step")?;
                if step.value.is_nan() || step.value <= 0.0 {
                    return Err(ModelError::new("rounding step must be positive"));
                }
                let steps = value.value / step.value;
                // Within roundoff of a multiple, that multiple, for any mode.
                let nearest = steps.round();
                let count = if (steps - nearest).abs() <= 1e-9 * nearest.abs().max(1.0) {
                    nearest
                } else {
                    match mode {
                        RoundingMode::Nearest => nearest,
                        RoundingMode::Down => steps.floor(),
                        RoundingMode::Up => steps.ceil(),
                    }
                };
                (count * step.value, dimension)
            }
            ScalarExpr::VectorLength(value) => {
                let value = vector(value)?;
                let v = value.value;
                (v.x.hypot(v.y.hypot(v.z)), value.dimension)
            }
            ScalarExpr::DotProduct(left, right) => {
                let (left, right) = (vector(left)?, vector(right)?);
                let dimension = match (left.dimension, right.dimension) {
                    (Dimension::Scalar, dimension) | (dimension, Dimension::Scalar) => dimension,
                    _ => {
                        return Err(ModelError::new(
                            "dot product requires at least one dimensionless vector",
                        ));
                    }
                };
                let (a, b) = (left.value, right.value);
                (a.x * b.x + a.y * b.y + a.z * b.z, dimension)
            }
            _ => return Ok(None),
        }))
    })();
    match result {
        Ok(None) => None,
        Ok(Some((value, dimension))) if value.is_finite() => {
            Some(Ok(EvaluatedScalar { value, dimension }))
        }
        Ok(Some(_)) => Some(Err(ModelError::new(
            "scalar expression result is not finite",
        ))),
        Err(error) => Some(Err(error)),
    }
}
