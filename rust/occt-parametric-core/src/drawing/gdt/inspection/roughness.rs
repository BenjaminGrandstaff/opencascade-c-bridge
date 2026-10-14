//! Roughness parameters from measured stylus profiles, by the Gaussian
//! profile filter of ASME B46.1 and ISO 16610-21, and the ISO 4288 16% rule
//! for accepting a set of readings.
use super::*;
use crate::drawing::gdt::texture::{RoughnessParameter, RoughnessUnit};

/// Most profile samples in one sampling length. ISO 3274's largest standard
/// spacing gives at most 1,667, and the filter costs this many operations
/// per evaluated sample.
pub const MAX_SAMPLES_PER_CUTOFF: usize = 2_000;
/// Fewest profile samples in one sampling length.
pub const MIN_SAMPLES_PER_CUTOFF: usize = 20;
/// Upper bound on profile samples in one record, which bounds filtering
/// time to about `MAX_PROFILE_SAMPLES` × `MAX_SAMPLES_PER_CUTOFF` operations.
pub const MAX_PROFILE_SAMPLES: usize = 1_000_000;
/// The roughness sampling length when a requirement names none.
pub const DEFAULT_CUTOFF_MM: f64 = 0.8;
/// Sampling lengths in the evaluation length.
pub const SAMPLING_LENGTHS: usize = 5;

/// A primary profile traced by a stylus instrument: heights at equal
/// spacing along the surface, after any short-wave (λs) filtering. It spans
/// at least six sampling lengths: five evaluated and half of one at each
/// end, which the filter needs as run-up and run-out.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeasuredProfile {
    /// Distance between samples, in micrometres.
    pub spacing_um: f64,
    /// Heights in micrometres.
    pub heights_um: Vec<f64>,
}

/// Samples per sampling length, from a profile's spacing and the cutoff.
fn samples_per_cutoff(profile: &MeasuredProfile, cutoff_mm: f64) -> Result<usize, ModelError> {
    let spacing = profile.spacing_um;
    if !(spacing.is_finite() && spacing > 0.0) {
        return Err(ModelError::new(
            "profile spacing must be positive and finite",
        ));
    }
    let ratio = cutoff_mm * 1000.0 / spacing;
    let k = ratio.round();
    if (ratio - k).abs() > 1e-6 * ratio
        || !(MIN_SAMPLES_PER_CUTOFF as f64..=MAX_SAMPLES_PER_CUTOFF as f64).contains(&k)
    {
        return Err(ModelError::new(format!(
            "profile spacing must divide the {cutoff_mm} mm cutoff into \
             {MIN_SAMPLES_PER_CUTOFF}-{MAX_SAMPLES_PER_CUTOFF} samples"
        )));
    }
    Ok(k as usize)
}

/// The Gaussian weighting function at `k` samples per cutoff, truncated at
/// half a cutoff each side to match the run-up and normalized to unit sum.
fn gaussian_weights(k: usize) -> Vec<f64> {
    let alpha = (std::f64::consts::LN_2 / std::f64::consts::PI).sqrt();
    let half = k / 2;
    let mut weights: Vec<f64> = (0..=2 * half)
        .map(|j| {
            let x = (j as f64 - half as f64) / (alpha * k as f64);
            (-std::f64::consts::PI * x * x).exp()
        })
        .collect();
    let sum: f64 = weights.iter().sum();
    weights.iter_mut().for_each(|w| *w /= sum);
    weights
}

/// The roughness profile over the evaluation length: each height minus the
/// Gaussian mean line through it, for the central five sampling lengths.
/// O(5k²) for `k` samples per cutoff.
pub fn roughness_profile(
    profile: &MeasuredProfile,
    cutoff_mm: f64,
) -> Result<(Vec<f64>, usize), ModelError> {
    let k = samples_per_cutoff(profile, cutoff_mm)?;
    let z = &profile.heights_um;
    let evaluated = SAMPLING_LENGTHS * k;
    if z.len() < evaluated + k {
        return Err(ModelError::new(format!(
            "a profile must span six {cutoff_mm} mm sampling lengths: {} samples",
            evaluated + k
        )));
    }
    if !z.iter().all(|h| h.is_finite()) {
        return Err(ModelError::new("profile heights must be finite"));
    }
    let weights = gaussian_weights(k);
    let half = k / 2;
    let start = (z.len() - evaluated) / 2;
    let roughness = (start..start + evaluated)
        .map(|i| {
            let window = &z[i - half..=i + half];
            let mean: f64 = window.iter().zip(&weights).map(|(h, w)| h * w).sum();
            z[i] - mean
        })
        .collect();
    Ok((roughness, k))
}

/// One parameter of a profile, in the requirement's unit: Ra and Rq over
/// the evaluation length, Rz the mean and Rmax the largest of the five
/// sampling lengths' peak-to-valley heights.
pub fn profile_parameter(
    profile: &MeasuredProfile,
    cutoff_mm: f64,
    parameter: RoughnessParameter,
    unit: RoughnessUnit,
) -> Result<f64, ModelError> {
    let (r, k) = roughness_profile(profile, cutoff_mm)?;
    let n = r.len() as f64;
    let heights = || {
        r.chunks(k).map(|s| {
            let (low, high) = s
                .iter()
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| {
                    (lo.min(*v), hi.max(*v))
                });
            high - low
        })
    };
    let micrometres = match parameter {
        RoughnessParameter::Ra => r.iter().map(|v| v.abs()).sum::<f64>() / n,
        RoughnessParameter::Rq => (r.iter().map(|v| v * v).sum::<f64>() / n).sqrt(),
        RoughnessParameter::Rz => heights().sum::<f64>() / SAMPLING_LENGTHS as f64,
        RoughnessParameter::Rmax => heights().fold(0.0, f64::max),
    };
    Ok(match unit {
        RoughnessUnit::Micrometer => micrometres,
        RoughnessUnit::Microinch => micrometres / 0.0254,
    })
}

/// The outcome of the 16% rule for one limit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Staged {
    Accepted,
    Rejected,
    /// The readings so far decide nothing; the rule needs this many.
    Needs(usize),
}

/// Readings allowed outside a limit among `n` readings, at least twelve.
fn allowed(n: usize) -> usize {
    (n * 16 / 100).max(2)
}

/// ISO 4288's staged 16% rule, in reading order, for one limit. `outside`
/// says whether a reading lies beyond the limit. For a maximum, a first
/// reading under 70% of it accepts at once. Otherwise the first three
/// readings accept when none is outside; failing that, twelve or more
/// readings accept when at most two of twelve (16% of more) are outside.
pub(super) fn sixteen_percent(
    readings: &[f64],
    outside: impl Fn(f64) -> bool,
    early: Option<f64>,
) -> Staged {
    if let (Some(threshold), Some(first)) = (early, readings.first())
        && *first < threshold
    {
        return Staged::Accepted;
    }
    if readings.len() < 3 {
        return Staged::Needs(3);
    }
    if !readings[..3].iter().any(|v| outside(*v)) {
        return Staged::Accepted;
    }
    if readings.len() < 12 {
        return Staged::Needs(12);
    }
    if readings.iter().filter(|v| outside(**v)).count() <= allowed(readings.len()) {
        Staged::Accepted
    } else {
        Staged::Rejected
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Six 0.8 mm sampling lengths at 0.5 µm: a sine of the given wavelength
    /// and amplitude on a sloped, offset mean line.
    fn sine(wavelength_um: f64, amplitude_um: f64) -> MeasuredProfile {
        MeasuredProfile {
            spacing_um: 0.5,
            heights_um: (0..9_600)
                .map(|i| {
                    let x = i as f64 * 0.5;
                    3.0 + 0.002 * x
                        + amplitude_um * (std::f64::consts::TAU * x / wavelength_um).sin()
                })
                .collect(),
        }
    }

    fn parameter(profile: &MeasuredProfile, parameter: RoughnessParameter) -> f64 {
        profile_parameter(profile, 0.8, parameter, RoughnessUnit::Micrometer).unwrap()
    }

    #[test]
    fn short_waves_pass_and_the_mean_line_follows_the_form() {
        let profile = sine(80.0, 1.0);
        // The truncated filter's side lobes leave a few parts in 10,000 of
        // the short wave in the mean line.
        let ra = parameter(&profile, RoughnessParameter::Ra);
        assert!((ra - 2.0 / std::f64::consts::PI).abs() < 1e-3, "{ra}");
        let rq = parameter(&profile, RoughnessParameter::Rq);
        assert!((rq - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-3, "{rq}");
        for p in [RoughnessParameter::Rz, RoughnessParameter::Rmax] {
            let height = parameter(&profile, p);
            assert!((height - 2.0).abs() < 2e-3, "{height}");
        }
        let microinches = profile_parameter(
            &profile,
            0.8,
            RoughnessParameter::Ra,
            RoughnessUnit::Microinch,
        )
        .unwrap();
        assert!((microinches * 0.0254 - ra).abs() < 1e-9);
    }

    #[test]
    fn the_cutoff_wavelength_transmits_about_half() {
        // Truncating the Gaussian at half a cutoff moves its 50% point
        // slightly: 48.9% of a cutoff-length wave remains in the roughness.
        let rq = parameter(&sine(800.0, 1.0), RoughnessParameter::Rq);
        let amplitude = rq * std::f64::consts::SQRT_2;
        assert!((amplitude - 0.4894).abs() < 2e-3, "{amplitude}");
        let long = parameter(&sine(8_000.0, 1.0), RoughnessParameter::Rq);
        assert!(long < 0.02, "{long}");
    }

    #[test]
    fn profiles_need_a_dividing_spacing_six_cutoffs_and_finite_heights() {
        let mut profile = sine(80.0, 1.0);
        profile.spacing_um = 0.3;
        assert!(roughness_profile(&profile, 0.8).is_err());
        profile.spacing_um = 0.01;
        assert!(roughness_profile(&profile, 0.8).is_err());
        let mut short = sine(80.0, 1.0);
        short.heights_um.truncate(9_599);
        assert!(roughness_profile(&short, 0.8).is_err());
        let mut invalid = sine(80.0, 1.0);
        invalid.heights_um[10] = f64::NAN;
        assert!(roughness_profile(&invalid, 0.8).is_err());
        assert_eq!(
            roughness_profile(&sine(80.0, 1.0), 0.8).unwrap().0.len(),
            8_000
        );
    }

    #[test]
    fn the_sixteen_percent_rule_decides_in_stages() {
        let above = |v: f64| v > 1.6;
        let run = |values: &[f64]| sixteen_percent(values, above, Some(1.12));
        assert_eq!(run(&[1.0]), Staged::Accepted);
        assert_eq!(run(&[1.5]), Staged::Needs(3));
        assert_eq!(run(&[1.5, 1.6, 1.4]), Staged::Accepted);
        assert_eq!(run(&[1.5, 1.7, 1.4]), Staged::Needs(12));
        let mut twelve = vec![1.5; 12];
        twelve[1] = 1.7;
        twelve[7] = 1.9;
        assert_eq!(run(&twelve), Staged::Accepted);
        twelve[9] = 1.8;
        assert_eq!(run(&twelve), Staged::Rejected);
        // Nineteen readings allow three outside.
        twelve.extend([1.5; 7]);
        assert_eq!(run(&twelve), Staged::Accepted);
    }
}
