//! ISO 286-1 tolerance classes H and h, grades 6 to 13, for sizes up to
//! 500 mm (schema 163): a frozen V1 snapshot of the standard tolerances.
//!
//! The values are the ISO 286-1 table as Wikipedia's "IT Grade" article
//! reproduces it (checked 2026-10-15). Each lies within 10% of the
//! standard's generating formula `k · i`, `i = 0.45 ∛D + 0.001 D` µm at the
//! geometric mean D of its size range, which the tests check to catch
//! transcription errors; the first range (to 3 mm) is the standard's own
//! exception. V1 values never change; corrections need a new version.
use super::*;

/// Grades IT6 to IT13 in micrometres, by size range: the range's upper
/// limit in millimetres (each range runs over the previous one's limit, up
/// to and including its own).
pub(crate) const IT_GRADES_V1: [(f64, [u32; 8]); 13] = [
    (3.0, [6, 10, 14, 25, 40, 60, 100, 140]),
    (6.0, [8, 12, 18, 30, 48, 75, 120, 180]),
    (10.0, [9, 15, 22, 36, 58, 90, 150, 220]),
    (18.0, [11, 18, 27, 43, 70, 110, 180, 270]),
    (30.0, [13, 21, 33, 52, 84, 130, 210, 330]),
    (50.0, [16, 25, 39, 62, 100, 160, 250, 390]),
    (80.0, [19, 30, 46, 74, 120, 190, 300, 460]),
    (120.0, [22, 35, 54, 87, 140, 220, 350, 540]),
    (180.0, [25, 40, 63, 100, 160, 250, 400, 630]),
    (250.0, [29, 46, 72, 115, 185, 290, 460, 720]),
    (315.0, [32, 52, 81, 130, 210, 320, 520, 810]),
    (400.0, [36, 57, 89, 140, 230, 360, 570, 890]),
    (500.0, [40, 63, 97, 155, 250, 400, 630, 970]),
];
/// Sizes may exceed a range limit by this much and stay in that range, for
/// unit-conversion roundoff.
const SIZE_TOLERANCE_MM: f64 = 1e-9;

/// An ISO 286 tolerance class: fundamental deviation H (holes, lower limit
/// at the nominal size) or h (shafts, upper limit at the nominal size) with
/// a grade from 6 to 13. Written as in drawings: "H7", "h6".
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(try_from = "String", into = "String")]
#[schemars(with = "String")]
pub struct ToleranceClass {
    /// True for a hole (H), false for a shaft (h).
    pub hole: bool,
    pub grade: u8,
}

impl TryFrom<String> for ToleranceClass {
    type Error = ModelError;
    fn try_from(text: String) -> Result<Self, ModelError> {
        let invalid = || {
            ModelError::new(format!(
                "tolerance class '{text}' must be H or h with a grade from 6 to 13"
            ))
        };
        let hole = match text.chars().next() {
            Some('H') => true,
            Some('h') => false,
            _ => return Err(invalid()),
        };
        let grade: u8 = text[1..].parse().map_err(|_| invalid())?;
        if !(6..=13).contains(&grade) || text[1..].starts_with('0') {
            return Err(invalid());
        }
        Ok(Self { hole, grade })
    }
}

impl From<ToleranceClass> for String {
    fn from(class: ToleranceClass) -> String {
        class.to_string()
    }
}

impl std::fmt::Display for ToleranceClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", if self.hole { 'H' } else { 'h' }, self.grade)
    }
}

impl ToleranceClass {
    /// The lower and upper deviations from `nominal_mm`, in millimetres:
    /// (0, IT) for H and (−IT, 0) for h. Sizes must be positive and at most
    /// 500 mm; there is no extrapolation. O(1).
    pub fn deviations_v1(self, nominal_mm: f64) -> Result<(f64, f64), ModelError> {
        if !(6..=13).contains(&self.grade) {
            return Err(ModelError::new("tolerance class grades run from 6 to 13"));
        }
        if !(nominal_mm.is_finite() && nominal_mm > 0.0) {
            return Err(ModelError::new(
                "a tolerance class needs a positive finite size",
            ));
        }
        let Some((_, grades)) = IT_GRADES_V1
            .iter()
            .find(|(limit, _)| nominal_mm <= limit + SIZE_TOLERANCE_MM)
        else {
            return Err(ModelError::new(format!(
                "tolerance class {self} covers sizes up to 500 mm, not {nominal_mm} mm"
            )));
        };
        let it = f64::from(grades[usize::from(self.grade) - 6]) / 1000.0;
        Ok(if self.hole { (0.0, it) } else { (-it, 0.0) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn class(text: &str) -> ToleranceClass {
        ToleranceClass::try_from(text.to_owned()).unwrap()
    }

    #[test]
    fn frozen_values_follow_the_generating_formula() {
        let multipliers = [10.0, 16.0, 25.0, 40.0, 64.0, 100.0, 160.0, 250.0];
        let mut lower = 0.0;
        for (upper, grades) in IT_GRADES_V1 {
            let d = f64::sqrt(f64::max(lower, 1.0) * upper);
            let i = 0.45 * d.cbrt() + 0.001 * d;
            for (k, it) in multipliers.iter().zip(grades) {
                let ratio = f64::from(it) / (k * i);
                // The first range is the standard's exception; it runs up
                // to 15% over the formula.
                let allowed = if lower == 0.0 { 0.16 } else { 0.10 };
                assert!((ratio - 1.0).abs() < allowed, "{upper} mm, {k}i: {it}");
            }
            // Grades and sizes both increase.
            assert!(grades.windows(2).all(|pair| pair[0] < pair[1]));
            lower = upper;
        }
        for pair in IT_GRADES_V1.windows(2) {
            assert!((0..8).all(|g| pair[0].1[g] <= pair[1].1[g]));
        }
    }

    #[test]
    fn classes_give_published_limits() {
        // Published H7 limits: +12 µm over 3-6 mm, +57 µm over 355-400 mm.
        assert_eq!(class("H7").deviations_v1(6.0).unwrap(), (0.0, 0.012));
        assert_eq!(class("H7").deviations_v1(380.0).unwrap(), (0.0, 0.057));
        assert_eq!(class("h6").deviations_v1(20.0).unwrap(), (-0.013, 0.0));
        assert_eq!(class("H13").deviations_v1(500.0).unwrap(), (0.0, 0.97));
        // Each range includes its upper limit, within unit roundoff.
        assert_eq!(class("H11").deviations_v1(3.0).unwrap(), (0.0, 0.06));
        assert_eq!(
            class("H11").deviations_v1(3.0 + 1e-12).unwrap(),
            (0.0, 0.06)
        );
        assert_eq!(class("H11").deviations_v1(3.001).unwrap(), (0.0, 0.075));
        assert!(class("H7").deviations_v1(500.1).is_err());
        assert!(class("H7").deviations_v1(0.0).is_err());
    }

    #[test]
    fn classes_parse_and_print_as_on_drawings() {
        for text in ["H6", "H13", "h6", "h11"] {
            assert_eq!(class(text).to_string(), text);
        }
        for text in ["H5", "H14", "G7", "H", "H07", "h 7", "", "H7 "] {
            assert!(ToleranceClass::try_from(text.to_owned()).is_err(), "{text}");
        }
        let json = serde_json::to_string(&class("H7")).unwrap();
        assert_eq!(json, "\"H7\"");
        assert!(serde_json::from_str::<ToleranceClass>("\"F7\"").is_err());
    }
}
