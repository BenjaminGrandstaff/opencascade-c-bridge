//! Bend tables (schema 161): bend allowances or deductions a shop measured
//! for one sheet thickness, by inside radius and bend angle, interpolated
//! bilinearly in place of a neutral factor.
use super::*;

/// What a bend table's values are.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BendTableKind {
    /// The developed length of the bend zone, added between the flanges'
    /// tangent-to-tangent lengths.
    Allowance,
    /// What a bend subtracts from the sum of its two flanges' outside
    /// mold-line lengths. The blank uses the equivalent allowance
    /// `2 (r + t) tan(θ/2) − deduction`.
    Deduction,
}

/// Measured bend values for one thickness. Rows follow `inside_radii_mm`
/// and columns follow `angles_degrees`; the angle is the bend's change of
/// direction, as `SheetMetalBend::angle_radians` gives it, not the included
/// angle between the flanges.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BendTable {
    /// Shown on the flat-pattern drawing; 1–64 printable ASCII characters.
    pub name: String,
    pub kind: BendTableKind,
    pub thickness_mm: f64,
    /// Strictly increasing positive radii, 1–256 of them.
    pub inside_radii_mm: Vec<f64>,
    /// Strictly increasing angles, 1–256 of them, in (0, 360) for allowances
    /// and (0, 180) for deductions.
    pub angles_degrees: Vec<f64>,
    /// One row per radius with one value per angle, in millimetres.
    pub values_mm: Vec<Vec<f64>>,
}

/// Most entries on either axis of a table.
pub const MAX_BEND_TABLE_AXIS: usize = 256;
/// How far a sheet's thickness may differ from a table's.
const THICKNESS_TOLERANCE_MM: f64 = 1e-6;
/// How far a bend may lie outside a table axis and still use its end entry.
const RADIUS_TOLERANCE_MM: f64 = 1e-6;
const ANGLE_TOLERANCE_DEGREES: f64 = 1e-6;

/// The two entries of a sorted axis around `value` and the fraction of the
/// way from the first to the second, or `None` outside the axis. A one-entry
/// axis matches only its entry.
fn bracket(axis: &[f64], value: f64, tolerance: f64) -> Option<(usize, usize, f64)> {
    let (first, last) = (axis[0], axis[axis.len() - 1]);
    if !(value >= first - tolerance && value <= last + tolerance) {
        return None;
    }
    if axis.len() == 1 {
        return Some((0, 0, 0.0));
    }
    let value = value.clamp(first, last);
    let upper = axis
        .partition_point(|entry| *entry < value)
        .clamp(1, axis.len() - 1);
    let (a, b) = (axis[upper - 1], axis[upper]);
    Some((upper - 1, upper, (value - a) / (b - a)))
}

impl BendTable {
    /// Checks the table's shape and values. O(radii × angles).
    pub fn validate(&self) -> Result<(), ModelError> {
        let fail = |why: &str| Err(ModelError::new(format!("bend table '{}' {why}", self.name)));
        if self.name.trim().is_empty()
            || self.name.len() > 64
            || !self.name.bytes().all(|b| (0x20..0x7f).contains(&b))
        {
            return Err(ModelError::new(
                "bend table name must be 1-64 printable ASCII characters",
            ));
        }
        if !(self.thickness_mm.is_finite() && self.thickness_mm > 0.0) {
            return fail("needs a positive finite thickness");
        }
        let increasing = |axis: &[f64], low: f64, high: f64| {
            (1..=MAX_BEND_TABLE_AXIS).contains(&axis.len())
                && axis.iter().all(|v| v.is_finite() && *v > low && *v < high)
                && axis.windows(2).all(|pair| pair[0] < pair[1])
        };
        if !increasing(&self.inside_radii_mm, 0.0, f64::INFINITY) {
            return fail("needs 1-256 strictly increasing positive inside radii");
        }
        // Deductions need the outside setback, which 180° makes infinite;
        // allowance tables may cover hems.
        let most = match self.kind {
            BendTableKind::Allowance => 360.0,
            BendTableKind::Deduction => 180.0,
        };
        if !increasing(&self.angles_degrees, 0.0, most) {
            return fail(
                "needs 1-256 strictly increasing angles above 0 degrees and below 360 \
                 (180 for deductions)",
            );
        }
        if self.values_mm.len() != self.inside_radii_mm.len()
            || self
                .values_mm
                .iter()
                .any(|row| row.len() != self.angles_degrees.len())
        {
            return fail("needs one row per radius and one value per angle");
        }
        let values = self.values_mm.iter().flatten();
        let valid = match self.kind {
            BendTableKind::Allowance => values.into_iter().all(|v| v.is_finite() && *v > 0.0),
            BendTableKind::Deduction => values.into_iter().all(|v| v.is_finite()),
        };
        if !valid {
            return fail("values must be finite, and allowances positive");
        }
        Ok(())
    }

    /// The allowance for bend `index` of `angle` radians (either sign) at
    /// `radius` in a sheet `thickness` thick: the table's value interpolated
    /// bilinearly, converted from a deduction when the table holds those.
    /// Bends outside the table, or a sheet of another thickness, are errors
    /// rather than extrapolations. O(log radii + log angles).
    pub fn allowance(
        &self,
        index: usize,
        angle: f64,
        radius: f64,
        thickness: f64,
    ) -> Result<f64, ModelError> {
        if (thickness - self.thickness_mm).abs() > THICKNESS_TOLERANCE_MM {
            return Err(ModelError::new(format!(
                "bend table '{}' is for {} mm sheet, not {thickness} mm",
                self.name, self.thickness_mm
            )));
        }
        let degrees = angle.abs().to_degrees();
        let (Some((r0, r1, s)), Some((a0, a1, u))) = (
            bracket(&self.inside_radii_mm, radius, RADIUS_TOLERANCE_MM),
            bracket(&self.angles_degrees, degrees, ANGLE_TOLERANCE_DEGREES),
        ) else {
            return Err(ModelError::new(format!(
                "bend {index} ({degrees} degrees at {radius} mm inside radius) lies outside bend table '{}'",
                self.name
            )));
        };
        let v = &self.values_mm;
        let low = v[r0][a0] + (v[r0][a1] - v[r0][a0]) * u;
        let high = v[r1][a0] + (v[r1][a1] - v[r1][a0]) * u;
        let value = low + (high - low) * s;
        let allowance = match self.kind {
            BendTableKind::Allowance => value,
            BendTableKind::Deduction => {
                2.0 * (radius + thickness) * (angle.abs() * 0.5).tan() - value
            }
        };
        if !(allowance.is_finite() && allowance > 0.0) {
            return Err(ModelError::new(format!(
                "bend table '{}' gives bend {index} a nonpositive allowance",
                self.name
            )));
        }
        Ok(allowance)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(kind: BendTableKind) -> BendTable {
        BendTable {
            name: "AL 2 mm".into(),
            kind,
            thickness_mm: 2.0,
            inside_radii_mm: vec![1.0, 3.0],
            angles_degrees: vec![45.0, 90.0],
            values_mm: vec![vec![2.0, 4.0], vec![4.0, 8.0]],
        }
    }

    #[test]
    fn values_interpolate_bilinearly_and_never_extrapolate() {
        let t = table(BendTableKind::Allowance);
        t.validate().unwrap();
        let at = |degrees: f64, radius| t.allowance(0, degrees.to_radians(), radius, 2.0);
        assert!((at(90.0, 1.0).unwrap() - 4.0).abs() < 1e-12);
        assert!((at(-45.0, 3.0).unwrap() - 4.0).abs() < 1e-12);
        // Halfway on both axes: the mean of the four corners.
        assert!((at(67.5, 2.0).unwrap() - 4.5).abs() < 1e-12);
        // Roundoff in an angle computed from radians still hits the edge.
        assert!((at(90.000_000_000_01, 3.0).unwrap() - 8.0).abs() < 1e-9);
        assert!(at(91.0, 1.0).is_err());
        assert!(at(60.0, 0.5).is_err());
        assert!(t.allowance(0, 1.0, 2.0, 2.5).is_err());
    }

    #[test]
    fn deductions_convert_through_the_outside_setback() {
        let t = BendTable {
            values_mm: vec![vec![3.0], vec![3.0]],
            angles_degrees: vec![90.0],
            ..table(BendTableKind::Deduction)
        };
        t.validate().unwrap();
        // 90° at r = 1, t = 2: twice the 3 mm setback less the deduction.
        let allowance = t
            .allowance(0, std::f64::consts::FRAC_PI_2, 1.0, 2.0)
            .unwrap();
        assert!((allowance - 3.0).abs() < 1e-12, "{allowance}");
        // A one-entry axis matches only its own angle.
        assert!(t.allowance(0, 1.0, 1.0, 2.0).is_err());
        let large = BendTable {
            values_mm: vec![vec![6.0], vec![6.0]],
            ..t
        };
        assert!(
            large
                .allowance(0, std::f64::consts::FRAC_PI_2, 1.0, 2.0)
                .is_err()
        );
    }

    #[test]
    fn malformed_tables_are_rejected() {
        let mut bad = table(BendTableKind::Allowance);
        bad.angles_degrees = vec![90.0, 45.0];
        assert!(bad.validate().is_err());
        let mut hems = table(BendTableKind::Allowance);
        hems.angles_degrees = vec![45.0, 180.0];
        assert!(hems.validate().is_ok());
        let mut bad = table(BendTableKind::Deduction);
        bad.angles_degrees = vec![45.0, 180.0];
        assert!(bad.validate().is_err());
        let mut bad = table(BendTableKind::Allowance);
        bad.angles_degrees = vec![45.0, 360.0];
        assert!(bad.validate().is_err());
        let mut bad = table(BendTableKind::Allowance);
        bad.values_mm[1].pop();
        assert!(bad.validate().is_err());
        let mut bad = table(BendTableKind::Allowance);
        bad.values_mm[0][0] = 0.0;
        assert!(bad.validate().is_err());
        let mut deduction = table(BendTableKind::Deduction);
        deduction.values_mm[0][0] = -1.0;
        assert!(deduction.validate().is_ok());
        let mut bad = table(BendTableKind::Allowance);
        bad.inside_radii_mm = vec![1.0; MAX_BEND_TABLE_AXIS + 1];
        assert!(bad.validate().is_err());
        bad.name = String::new();
        assert!(bad.validate().is_err());
    }
}
