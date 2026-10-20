//! Requirements that combine geometry with the frozen material catalog:
//! mass, temperature limits and beam bending at a section (schema 179).

use super::*;
use occt_parametric_core::materials::{
    MaterialGrade, MaterialProperty, MaterialStrength, MaterialTemperatureLimit,
};

/// Most section edges sampled for the extreme fiber.
const MAX_SECTION_EDGES: usize = 20_000;
/// Samples per section edge, including both ends.
const EDGE_SAMPLES: usize = 64;

fn finite(value: f64, name: &str) -> Result<f64, ModelError> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(ModelError::new(format!("{name} must be finite")))
    }
}

/// Exact: volume × catalog density, in kilograms.
pub(crate) fn mass_range(
    session: &Session,
    id: &str,
    shape: &Shape<'_>,
    material: MaterialGrade,
    minimum: f64,
    maximum: f64,
) -> Result<VerificationResult, ModelError> {
    let (minimum, maximum) = (
        finite(minimum, "minimum mass")?,
        finite(maximum, "maximum mass")?,
    );
    if minimum < 0.0 || maximum < minimum {
        return Err(ModelError::new(
            "mass range needs 0 <= minimum_kilograms <= maximum_kilograms",
        ));
    }
    let density = material.property(MaterialProperty::DensityKgPerCubicMeter)?;
    let volume = session.volume(shape)?;
    let mass = volume * 1e-9 * density;
    let passed = (minimum..=maximum).contains(&mass);
    Ok(VerificationResult::exact(
        id,
        passed,
        format!(
            "mass {mass:.6} kg ({volume:.3} mm^3 of {} at {density:.1} kg/m^3); expected {minimum}..={maximum} kg",
            material.name()
        ),
    )
    .measured(Measurement {
        value: mass,
        unit: MeasurementUnit::Kilogram,
        minimum: Some(minimum),
        maximum: Some(maximum),
    }))
}

/// Exact: the catalog temperature limit against the service temperature.
pub(crate) fn temperature_limit(
    id: &str,
    material: MaterialGrade,
    limit: MaterialTemperatureLimit,
    service: f64,
    margin: f64,
) -> Result<VerificationResult, ModelError> {
    let (service, margin) = (
        finite(service, "service temperature")?,
        finite(margin, "temperature margin")?,
    );
    if margin < 0.0 {
        return Err(ModelError::new("temperature margin must not be negative"));
    }
    let property = limit.property();
    let rated = material.property(property)?;
    let passed = rated - service >= margin;
    Ok(VerificationResult::exact(
        id,
        passed,
        format!(
            "{} {} {rated:.1} C; service {service} C needs a margin of {margin} C, has {:.1} C",
            material.name(),
            property.name(),
            rated - service
        ),
    )
    .measured(Measurement {
        value: rated,
        unit: MeasurementUnit::DegreeCelsius,
        minimum: Some(service + margin),
        maximum: None,
    }))
}

/// A bending check at one section; see `VerificationRule::BendingStress`.
pub(crate) struct Bending {
    pub material: MaterialGrade,
    pub section_origin: VectorQuantity,
    pub beam_axis: VectorQuantity,
    pub bending_axis: VectorQuantity,
    pub moment_newton_millimeters: f64,
    pub minimum_safety_factor: f64,
    pub strength: MaterialStrength,
}

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(a.x - b.x, a.y - b.y, a.z - b.z)
}
fn dot(a: Vec3, b: Vec3) -> f64 {
    a.x * b.x + a.y * b.y + a.z * b.z
}
fn cross(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(
        a.y * b.z - a.z * b.y,
        a.z * b.x - a.x * b.z,
        a.x * b.y - a.y * b.x,
    )
}
fn scaled(a: Vec3, s: f64) -> Vec3 {
    Vec3::new(a.x * s, a.y * s, a.z * s)
}
fn add(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(a.x + b.x, a.y + b.y, a.z + b.z)
}
fn unit(a: Vec3, name: &str) -> Result<Vec3, ModelError> {
    let length = dot(a, a).sqrt();
    if length.is_finite() && length > 1e-12 {
        Ok(scaled(a, 1.0 / length))
    } else {
        Err(ModelError::new(format!(
            "{name} must be a nonzero finite direction"
        )))
    }
}

/// Section second moment from a thin slab's exact central inertia, extreme
/// fiber from edge samples. O(section edges × EDGE_SAMPLES) plus two kernel
/// operations; the slab and section handles are released on return.
pub(crate) fn bending_stress(
    session: &Session,
    id: &str,
    shape: &Shape<'_>,
    check: &Bending,
) -> Result<VerificationResult, ModelError> {
    let moment = finite(check.moment_newton_millimeters, "bending moment")?;
    let required = finite(check.minimum_safety_factor, "minimum safety factor")?;
    if moment <= 0.0 || required <= 0.0 {
        return Err(ModelError::new(
            "bending needs a positive moment and minimum safety factor",
        ));
    }
    let strength = check.material.property(check.strength.property())?;
    let origin = check.section_origin.normalized(Dimension::Length)?;
    let normal = unit(check.beam_axis.normalized(Dimension::Scalar)?, "beam axis")?;
    let axis = check.bending_axis.normalized(Dimension::Scalar)?;
    let axis = unit(
        sub(axis, scaled(normal, dot(axis, normal))),
        "bending axis across the beam",
    )?;
    let across = cross(normal, axis);

    // A square slab of thickness t centered on the section plane.
    let bounds = session.exact_bounds(shape)?;
    let (low, high) = (bounds.min, bounds.max);
    let diagonal = dot(sub(high, low), sub(high, low)).sqrt();
    let center = scaled(add(low, high), 0.5);
    let reach = diagonal + dot(sub(center, origin), sub(center, origin)).sqrt() + 1.0;
    let thickness = (diagonal * 1e-4).max(1e-6);
    let base = sub(origin, scaled(normal, thickness / 2.0));
    let corner = |a: f64, b: f64| {
        add(
            base,
            add(scaled(axis, a * reach), scaled(across, b * reach)),
        )
    };
    let slab = session.create_polygon_prism(
        &[
            corner(-1.0, -1.0),
            corner(1.0, -1.0),
            corner(1.0, 1.0),
            corner(-1.0, 1.0),
        ],
        scaled(normal, thickness),
    )?;
    let section = session.common(shape, &slab)?;
    let edges = session.subshapes(&section, ShapeType::Edge)?;
    if edges.is_empty() {
        return Err(ModelError::new("the section plane does not cut the output"));
    }
    if edges.len() > MAX_SECTION_EDGES {
        return Err(ModelError::new(format!(
            "the section has more than {MAX_SECTION_EDGES} edges"
        )));
    }
    let properties = session.mass_properties(&section)?;
    let area = properties.volume / thickness;
    // Central inertia about `axis` = t·I + A·t³/12 for a prismatic slab.
    let j = properties.inertia;
    let a = [axis.x, axis.y, axis.z];
    let about_axis = (0..3)
        .map(|r| (0..3).map(|c| a[r] * j[r][c] * a[c]).sum::<f64>())
        .sum::<f64>();
    let second_moment = (about_axis - area * thickness.powi(3) / 12.0) / thickness;
    if !(second_moment.is_finite() && second_moment > 0.0) {
        return Err(ModelError::new("the section has no bending stiffness"));
    }
    let centroid = properties.center;
    let (mut fiber, mut witness, mut samples) = (0.0f64, centroid, 0usize);
    for edge in &edges {
        for point in session.edge_sample_points(edge, EDGE_SAMPLES)? {
            samples += 1;
            let distance = dot(sub(point, centroid), across).abs();
            if distance > fiber {
                (fiber, witness) = (distance, point);
            }
        }
    }
    let stress = moment * fiber / second_moment;
    let factor = strength / stress;
    let passed = factor >= required;
    let mut result = VerificationResult::exact(
        id,
        passed,
        format!(
            "section A {area:.3} mm^2, I {second_moment:.3} mm^4, c {fiber:.4} mm; \
             stress {stress:.3} MPa under {moment} N*mm; {} {} {strength:.1} MPa; \
             safety factor {factor:.3}, required {required}",
            check.material.name(),
            check.strength.property().name(),
        ),
    )
    .measured(Measurement {
        value: factor,
        unit: MeasurementUnit::Ratio,
        minimum: Some(required),
        maximum: None,
    })
    .witnessed(Witness {
        subjects: vec!["extreme fiber".into()],
        points_mm: vec![centroid, witness],
    });
    // The extreme fiber is screened at edge samples.
    result.evidence = Evidence::Sampled {
        samples,
        unresolved: 0,
    };
    Ok(result)
}
