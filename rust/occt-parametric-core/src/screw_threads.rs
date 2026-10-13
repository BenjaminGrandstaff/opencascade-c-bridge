//! Unified inch screw threads (ASME B1.1): designations, the standard
//! coarse, fine and extra-fine series, limits of size per thread class, and
//! the fit of an external thread in an internal one.
//!
//! Limits come from B1.1's tolerance formulas as published by calculators
//! that reproduce its tables (machinecalcs.com and engineersedge.com, read
//! 2026-10-13). With `P = 1/n` and basic major diameter `D`, all in inches,
//! the class 2A pitch-diameter tolerance is
//! `T = 0.0015·∛D + 0.0015·√LE + 0.015·∛P²` for length of engagement `LE`
//! (the tables use `LE = D`). Classes 1A, 3A, 1B and 2B scale it by 1.5,
//! 0.75, 1.95 and 1.3. Classes 1A and 2A get the allowance `0.3·T`; class
//! 3A and internal threads none. External major-diameter tolerance is
//! `0.060·∛P²` (2A, 3A) or `0.090·∛P²` (1A). Internal 1B and 2B
//! minor-diameter tolerance is `0.05·∛P² + 0.03·P/D − 0.002`, at least
//! `0.25P − 0.4P²` and at most `0.394P`, above the basic minor diameter.
//!
//! Tolerances and allowances are rounded to 0.0001 in, then limits are
//! stacked from the basic diameters and rounded to 0.0001 in; the 1B and 2B
//! minor diameters round to 0.001 in from 0.138 in (#6) up. Checked against
//! published 2A and 2B tables for ten UNC and UNF sizes from #0 to 1 in, the
//! limits agree except four values one rounding step off on a rounding
//! boundary: #4-48 2A maximum pitch diameter (0.0978, table 0.0979), #2-64
//! and #10-24 2B maximum minor diameters (0.0752 and 0.155, tables 0.0753
//! and 0.156) and 1/4-20 2B maximum pitch diameter (0.2223, table 0.2224). Class 3B, whose minor diameter
//! follows other rules, is not supported, and B1.1's reference-only external
//! minor diameters are not given.
use super::*;

/// A Unified thread series.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum UnifiedSeries {
    /// Coarse: a standard diameter at its coarse pitch.
    Unc,
    /// Fine.
    Unf,
    /// Extra fine.
    Unef,
    /// Constant pitch, such as 8 UN, on any standard diameter.
    Un,
    /// Special: any diameter and pitch.
    Uns,
}

/// A Unified thread class: A external, B internal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub enum UnifiedClass {
    #[serde(rename = "1A")]
    OneA,
    #[serde(rename = "2A")]
    TwoA,
    #[serde(rename = "3A")]
    ThreeA,
    #[serde(rename = "1B")]
    OneB,
    #[serde(rename = "2B")]
    TwoB,
}

impl UnifiedClass {
    pub fn internal(self) -> bool {
        matches!(self, Self::OneB | Self::TwoB)
    }

    fn label(self) -> &'static str {
        match self {
            Self::OneA => "1A",
            Self::TwoA => "2A",
            Self::ThreeA => "3A",
            Self::OneB => "1B",
            Self::TwoB => "2B",
        }
    }

    /// Pitch-diameter tolerance as a multiple of class 2A's.
    fn pitch_factor(self) -> f64 {
        match self {
            Self::OneA => 1.5,
            Self::TwoA => 1.0,
            Self::ThreeA => 0.75,
            Self::OneB => 1.95,
            Self::TwoB => 1.3,
        }
    }
}

/// Standard sizes: label, basic major diameter (in), and the UNC, UNF and
/// UNEF threads per inch (0 when the series has none). From the Unified
/// Thread Standard size table
/// (<https://en.wikipedia.org/wiki/Unified_Thread_Standard>, read
/// 2026-10-13); 2 and 2-1/4 in coarse are 4.5 per inch.
const SIZES: [(&str, f64, f64, f64, f64); 34] = [
    ("#0", 0.0600, 0.0, 80.0, 0.0),
    ("#1", 0.0730, 64.0, 72.0, 0.0),
    ("#2", 0.0860, 56.0, 64.0, 0.0),
    ("#3", 0.0990, 48.0, 56.0, 0.0),
    ("#4", 0.1120, 40.0, 48.0, 0.0),
    ("#5", 0.1250, 40.0, 44.0, 0.0),
    ("#6", 0.1380, 32.0, 40.0, 0.0),
    ("#8", 0.1640, 32.0, 36.0, 0.0),
    ("#10", 0.1900, 24.0, 32.0, 0.0),
    ("#12", 0.2160, 24.0, 28.0, 32.0),
    ("1/4", 0.2500, 20.0, 28.0, 32.0),
    ("5/16", 0.3125, 18.0, 24.0, 32.0),
    ("3/8", 0.3750, 16.0, 24.0, 32.0),
    ("7/16", 0.4375, 14.0, 20.0, 28.0),
    ("1/2", 0.5000, 13.0, 20.0, 28.0),
    ("9/16", 0.5625, 12.0, 18.0, 24.0),
    ("5/8", 0.6250, 11.0, 18.0, 24.0),
    ("3/4", 0.7500, 10.0, 16.0, 20.0),
    ("7/8", 0.8750, 9.0, 14.0, 20.0),
    ("1", 1.0000, 8.0, 12.0, 20.0),
    ("1-1/8", 1.1250, 7.0, 12.0, 0.0),
    ("1-1/4", 1.2500, 7.0, 12.0, 0.0),
    ("1-3/8", 1.3750, 6.0, 12.0, 0.0),
    ("1-1/2", 1.5000, 6.0, 12.0, 0.0),
    ("1-3/4", 1.7500, 5.0, 0.0, 0.0),
    ("2", 2.0000, 4.5, 0.0, 0.0),
    ("2-1/4", 2.2500, 4.5, 0.0, 0.0),
    ("2-1/2", 2.5000, 4.0, 0.0, 0.0),
    ("2-3/4", 2.7500, 4.0, 0.0, 0.0),
    ("3", 3.0000, 4.0, 0.0, 0.0),
    ("3-1/4", 3.2500, 4.0, 0.0, 0.0),
    ("3-1/2", 3.5000, 4.0, 0.0, 0.0),
    ("3-3/4", 3.7500, 4.0, 0.0, 0.0),
    ("4", 4.0000, 4.0, 0.0, 0.0),
];

/// Inches per millimetre conversions are exact: 25.4 mm per inch.
const MM_PER_INCH: f64 = 25.4;

fn size_of(diameter_in: f64) -> Option<&'static (&'static str, f64, f64, f64, f64)> {
    SIZES
        .iter()
        .find(|size| (size.1 - diameter_in).abs() < 1e-6)
}

/// Rounds half away from zero to a multiple of `step`, absorbing the
/// floating-point error of the division.
fn round_to(value: f64, step: f64) -> f64 {
    ((value / step) * (1.0 + 1e-12)).round() * step
}

fn four(value: f64) -> f64 {
    round_to(value, 1e-4)
}

/// A Unified thread: basic major diameter and threads per inch, with its
/// series, class and whether an external thread has the rounded UNR root
/// (named in the designation; its root limits are not given).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UnifiedThread {
    pub diameter_in: f64,
    pub threads_per_inch: f64,
    pub series: UnifiedSeries,
    pub class: UnifiedClass,
    pub rounded_root: bool,
}

/// Limits of size in inches, each `[minimum, maximum]`. B1.1 sets no
/// internal major maximum; the minor diameter is given for internal threads.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UnifiedLimits {
    pub major_in: [Option<f64>; 2],
    pub pitch_in: [f64; 2],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minor_in: Option<[f64; 2]>,
    /// Allowance below basic of an external thread's maximum sizes.
    pub allowance_in: f64,
    pub length_of_engagement_in: f64,
}

impl UnifiedThread {
    /// A thread of `diameter_mm` and `pitch_mm` in `series` and `class`.
    /// Coarse, fine and extra-fine threads must be a standard size at its
    /// series pitch, constant-pitch (UN) threads a standard size, and only
    /// external threads may have the UNR root. O(sizes).
    pub fn new(
        diameter_mm: f64,
        pitch_mm: f64,
        series: UnifiedSeries,
        class: UnifiedClass,
        rounded_root: bool,
    ) -> Result<Self, ModelError> {
        if !(diameter_mm.is_finite() && diameter_mm > 0.0 && pitch_mm.is_finite() && pitch_mm > 0.0)
        {
            return Err(ModelError::new(
                "a Unified thread needs a positive finite diameter and pitch",
            ));
        }
        let diameter_in = diameter_mm / MM_PER_INCH;
        let threads_per_inch = MM_PER_INCH / pitch_mm;
        // Pitches are a whole or half number of threads per inch.
        let tpi = round_to(threads_per_inch, 0.5);
        if (threads_per_inch - tpi).abs() > 1e-6 * tpi {
            return Err(ModelError::new(format!(
                "a Unified thread's pitch must be a whole or half number of threads per inch, not {threads_per_inch:.4}"
            )));
        }
        if rounded_root && class.internal() {
            return Err(ModelError::new(
                "only external Unified threads have the UNR rounded root",
            ));
        }
        let size = size_of(diameter_in);
        let standard = |tpi_of: fn(&(&str, f64, f64, f64, f64)) -> f64, name: &str| match size {
            Some(size) if tpi_of(size) == tpi => Ok(()),
            Some(size) if tpi_of(size) > 0.0 => Err(ModelError::new(format!(
                "{} {name} is {} threads per inch, not {tpi}",
                size.0,
                tpi_of(size)
            ))),
            _ => Err(ModelError::new(format!(
                "{diameter_in:.4} in has no {name} size; use UN or UNS"
            ))),
        };
        match series {
            UnifiedSeries::Unc => standard(|s| s.2, "UNC")?,
            UnifiedSeries::Unf => standard(|s| s.3, "UNF")?,
            UnifiedSeries::Unef => standard(|s| s.4, "UNEF")?,
            UnifiedSeries::Un if size.is_none() => {
                return Err(ModelError::new(format!(
                    "{diameter_in:.4} in is not a standard Unified diameter; use UNS"
                )));
            }
            UnifiedSeries::Un | UnifiedSeries::Uns => {}
        }
        Ok(Self {
            diameter_in: size.map_or(diameter_in, |s| s.1),
            threads_per_inch: tpi,
            series,
            class,
            rounded_root,
        })
    }

    /// The designation, such as `1/4-20 UNC-2A` or `1/2-13 UNRC-2A`.
    pub fn designation(&self) -> String {
        let size = size_of(self.diameter_in)
            .map_or_else(|| format!("{:.4}", self.diameter_in), |s| s.0.to_owned());
        let tpi = if self.threads_per_inch.fract() == 0.0 {
            format!("{}", self.threads_per_inch)
        } else {
            format!("{:.1}", self.threads_per_inch)
        };
        let series = match self.series {
            UnifiedSeries::Unc => "C",
            UnifiedSeries::Unf => "F",
            UnifiedSeries::Unef => "EF",
            UnifiedSeries::Un => "",
            UnifiedSeries::Uns => "S",
        };
        let root = if self.rounded_root { "UNR" } else { "UN" };
        format!("{size}-{tpi} {root}{series}-{}", self.class.label())
    }

    fn pitch(&self) -> f64 {
        1.0 / self.threads_per_inch
    }

    /// Class 2A pitch-diameter tolerance for `engagement` inches.
    fn class_2a_tolerance(&self, engagement: f64) -> f64 {
        let p = self.pitch();
        0.0015 * self.diameter_in.cbrt() + 0.0015 * engagement.sqrt() + 0.015 * (p * p).cbrt()
    }

    /// Limits of size for a length of engagement in inches, or the
    /// tables' basic major diameter when none is given.
    pub fn limits(&self, engagement_in: Option<f64>) -> Result<UnifiedLimits, ModelError> {
        let engagement = engagement_in.unwrap_or(self.diameter_in);
        if !(engagement.is_finite() && engagement > 0.0) {
            return Err(ModelError::new(
                "a length of engagement must be positive and finite",
            ));
        }
        let p = self.pitch();
        let d = self.diameter_in;
        // The basic pitch diameter lies 3√3/8·P below the major, and the
        // basic minor √3/4·P below the pitch diameter.
        let basic_pitch = d - 3.0 * 3f64.sqrt() / 8.0 * p;
        let t2a = self.class_2a_tolerance(engagement);
        let tolerance = four(self.class.pitch_factor() * t2a);
        if self.class.internal() {
            let minimum_minor = basic_pitch - 3f64.sqrt() / 4.0 * p;
            let minor_tolerance = (0.05 * (p * p).cbrt() + 0.03 * p / d - 0.002)
                .max(0.25 * p - 0.4 * p * p)
                .min(0.394 * p);
            let minor_step = if d >= 0.138 { 1e-3 } else { 1e-4 };
            return Ok(UnifiedLimits {
                major_in: [Some(four(d)), None],
                pitch_in: [four(basic_pitch), four(basic_pitch + tolerance)],
                minor_in: Some([
                    round_to(minimum_minor, minor_step),
                    round_to(minimum_minor + minor_tolerance, minor_step),
                ]),
                allowance_in: 0.0,
                length_of_engagement_in: engagement,
            });
        }
        let allowance = match self.class {
            UnifiedClass::ThreeA => 0.0,
            _ => four(0.3 * t2a),
        };
        let major_tolerance = four(
            match self.class {
                UnifiedClass::OneA => 0.090,
                _ => 0.060,
            } * (p * p).cbrt(),
        );
        let major_max = four(d - allowance);
        let pitch_max = four(basic_pitch - allowance);
        Ok(UnifiedLimits {
            major_in: [Some(four(major_max - major_tolerance)), Some(major_max)],
            pitch_in: [four(pitch_max - tolerance), pitch_max],
            minor_in: None,
            allowance_in: allowance,
            length_of_engagement_in: engagement,
        })
    }
}

/// How an external thread fits an internal one, in inches: the least and
/// greatest pitch-diameter clearance, and the least major-diameter
/// clearance. The threads assemble at all limits when neither least
/// clearance is negative.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UnifiedFit {
    pub pitch_clearance_in: [f64; 2],
    pub major_clearance_in: f64,
}

impl UnifiedFit {
    pub fn assembles(&self) -> bool {
        self.pitch_clearance_in[0] >= -1e-12 && self.major_clearance_in >= -1e-12
    }
}

/// The fit of `external` in `internal` over a length of engagement in
/// inches (the basic major diameter when none). They must share diameter
/// and pitch, and be an A and a B class.
pub fn unified_fit(
    external: &UnifiedThread,
    internal: &UnifiedThread,
    engagement_in: Option<f64>,
) -> Result<UnifiedFit, ModelError> {
    if external.class.internal() || !internal.class.internal() {
        return Err(ModelError::new(
            "a Unified fit pairs an external (A) and an internal (B) thread",
        ));
    }
    if (external.diameter_in - internal.diameter_in).abs() > 1e-6
        || external.threads_per_inch != internal.threads_per_inch
    {
        return Err(ModelError::new(
            "a Unified fit needs threads of the same diameter and pitch",
        ));
    }
    let e = external.limits(engagement_in)?;
    let i = internal.limits(engagement_in)?;
    let major = i.major_in[0].unwrap_or(internal.diameter_in)
        - e.major_in[1].unwrap_or(external.diameter_in);
    Ok(UnifiedFit {
        pitch_clearance_in: [i.pitch_in[0] - e.pitch_in[1], i.pitch_in[1] - e.pitch_in[0]],
        major_clearance_in: major,
    })
}

/// A Unified thread specification on a modeled thread or tapped hole
/// (schema 152): its series, class and root, and optionally the length of
/// engagement its tolerances use. Regeneration checks the feature's
/// diameter and pitch against the series and its side against the class.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UnifiedThreadSpec {
    pub series: UnifiedSeries,
    pub class: UnifiedClass,
    /// The UNR rounded root of an external thread, named in its designation.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub rounded_root: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length_of_engagement: Option<ScalarExpr>,
}

impl UnifiedThreadSpec {
    /// The thread of a feature with `diameter_mm` and `pitch_mm` on the
    /// `internal` side.
    pub fn thread(
        &self,
        diameter_mm: f64,
        pitch_mm: f64,
        internal: bool,
    ) -> Result<UnifiedThread, ModelError> {
        if self.class.internal() != internal {
            return Err(ModelError::new(format!(
                "Unified class {} is for {} threads",
                self.class.label(),
                if self.class.internal() {
                    "internal"
                } else {
                    "external"
                }
            )));
        }
        UnifiedThread::new(
            diameter_mm,
            pitch_mm,
            self.series,
            self.class,
            self.rounded_root,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn thread(
        diameter_in: f64,
        tpi: f64,
        series: UnifiedSeries,
        class: UnifiedClass,
    ) -> UnifiedThread {
        UnifiedThread::new(diameter_in * 25.4, 25.4 / tpi, series, class, false).unwrap()
    }

    /// Published 2A and 2B limits (major max/min, pitch max/min; pitch
    /// max/min, minor max/min) that the formulas reproduce exactly.
    #[test]
    fn limits_match_published_tables() {
        type Row = (f64, f64, UnifiedSeries, [f64; 4], [f64; 4]);
        use UnifiedSeries::{Unc, Unf};
        let rows: [Row; 13] = [
            (
                0.19,
                24.0,
                Unc,
                [0.1890, 0.1818, 0.1619, 0.1586],
                [0.1672, 0.1629, f64::NAN, 0.1450],
            ),
            (
                0.25,
                20.0,
                Unc,
                [0.2489, 0.2408, 0.2164, 0.2127],
                [f64::NAN, 0.2175, 0.2070, 0.1960],
            ),
            (
                0.3125,
                18.0,
                Unc,
                [0.3113, 0.3026, 0.2752, 0.2712],
                [0.2817, 0.2764, 0.2650, 0.2520],
            ),
            (
                0.5,
                13.0,
                Unc,
                [0.4985, 0.4876, 0.4485, 0.4435],
                [0.4565, 0.4500, 0.4340, 0.4170],
            ),
            (
                0.75,
                10.0,
                Unc,
                [0.7482, 0.7353, 0.6832, 0.6773],
                [0.6927, 0.6850, 0.6630, 0.6420],
            ),
            (
                1.0,
                8.0,
                Unc,
                [0.9980, 0.9830, 0.9168, 0.9100],
                [0.9276, 0.9188, 0.8900, 0.8650],
            ),
            (
                0.06,
                80.0,
                Unf,
                [0.0595, 0.0563, 0.0514, 0.0496],
                [0.0542, 0.0519, 0.0514, 0.0465],
            ),
            (
                0.086,
                64.0,
                Unf,
                [0.0854, 0.0816, 0.0753, 0.0733],
                [0.0786, 0.0759, f64::NAN, 0.0691],
            ),
            (
                0.112,
                48.0,
                Unf,
                [0.1113, 0.1068, f64::NAN, 0.0954],
                [0.1016, 0.0985, 0.0968, 0.0894],
            ),
            (
                0.138,
                40.0,
                Unf,
                [0.1372, 0.1321, 0.1210, 0.1184],
                [0.1252, 0.1218, 0.1190, 0.1110],
            ),
            (
                0.19,
                32.0,
                Unf,
                [0.1891, 0.1831, 0.1688, 0.1658],
                [0.1736, 0.1697, 0.1640, 0.1560],
            ),
            (
                0.5,
                20.0,
                Unf,
                [0.4987, 0.4906, 0.4662, 0.4619],
                [0.4731, 0.4675, 0.4570, 0.4460],
            ),
            (
                1.0,
                12.0,
                Unf,
                [0.9982, 0.9868, 0.9441, 0.9382],
                [0.9535, 0.9459, 0.9280, 0.9100],
            ),
        ];
        let check = |actual: f64, expected: f64, what: &str| {
            assert!(
                expected.is_nan() || (actual - expected).abs() < 1e-9,
                "{what}: {actual} vs {expected}"
            );
        };
        for (d, tpi, series, external, internal) in rows {
            let a = thread(d, tpi, series, UnifiedClass::TwoA)
                .limits(None)
                .unwrap();
            let b = thread(d, tpi, series, UnifiedClass::TwoB)
                .limits(None)
                .unwrap();
            let what = format!("{d}-{tpi}");
            check(a.major_in[1].unwrap(), external[0], &what);
            check(a.major_in[0].unwrap(), external[1], &what);
            check(a.pitch_in[1], external[2], &what);
            check(a.pitch_in[0], external[3], &what);
            check(b.pitch_in[1], internal[0], &what);
            check(b.pitch_in[0], internal[1], &what);
            check(b.minor_in.unwrap()[1], internal[2], &what);
            check(b.minor_in.unwrap()[0], internal[3], &what);
            check(b.major_in[0].unwrap(), d, &what);
            assert!(a.minor_in.is_none() && b.major_in[1].is_none());
        }
        // The documented one-step misses, on rounding boundaries.
        let quarter_b = thread(0.25, 20.0, Unc, UnifiedClass::TwoB)
            .limits(None)
            .unwrap();
        assert!((quarter_b.pitch_in[1] - 0.2223).abs() < 1e-9);
        // Class scaling: 1A and 3A against 2A for 1/4-20.
        let one_a = thread(0.25, 20.0, Unc, UnifiedClass::OneA)
            .limits(None)
            .unwrap();
        let three_a = thread(0.25, 20.0, Unc, UnifiedClass::ThreeA)
            .limits(None)
            .unwrap();
        check(one_a.major_in[0].unwrap(), 0.2367, "1A major");
        check(one_a.pitch_in[0], 0.2108, "1A pitch");
        check(three_a.major_in[1].unwrap(), 0.2500, "3A major");
        check(three_a.pitch_in[0], 0.2147, "3A pitch");
        assert_eq!(three_a.allowance_in, 0.0);
        // A longer engagement widens the pitch tolerance.
        let long = thread(0.25, 20.0, Unc, UnifiedClass::TwoA)
            .limits(Some(1.0))
            .unwrap();
        assert!(long.pitch_in[1] - long.pitch_in[0] > 0.2164 - 0.2127);
    }

    #[test]
    fn designations_series_and_fits() {
        let bolt = thread(0.25, 20.0, UnifiedSeries::Unc, UnifiedClass::TwoA);
        assert_eq!(bolt.designation(), "1/4-20 UNC-2A");
        let nut = thread(0.25, 20.0, UnifiedSeries::Unc, UnifiedClass::TwoB);
        let fit = unified_fit(&bolt, &nut, None).unwrap();
        assert!((fit.pitch_clearance_in[0] - 0.0011).abs() < 1e-9);
        assert!(fit.assembles());
        // A 3A bolt in a 2B nut has no allowance but still assembles.
        let tight = thread(0.25, 20.0, UnifiedSeries::Unc, UnifiedClass::ThreeA);
        let fit = unified_fit(&tight, &nut, None).unwrap();
        assert!(fit.pitch_clearance_in[0].abs() < 1e-9 && fit.assembles());
        assert_eq!(
            thread(0.19, 32.0, UnifiedSeries::Unf, UnifiedClass::TwoB).designation(),
            "#10-32 UNF-2B"
        );
        let coarse = UnifiedThread::new(
            2.0 * 25.4,
            25.4 / 4.5,
            UnifiedSeries::Unc,
            UnifiedClass::TwoA,
            true,
        )
        .unwrap();
        assert_eq!(coarse.designation(), "2-4.5 UNRC-2A");
        let error = |d: f64, tpi: f64, series| {
            UnifiedThread::new(d * 25.4, 25.4 / tpi, series, UnifiedClass::TwoA, false)
                .unwrap_err()
                .message
        };
        assert!(error(0.25, 28.0, UnifiedSeries::Unc).contains("1/4 UNC is 20 threads per inch"));
        assert!(error(0.30, 20.0, UnifiedSeries::Unc).contains("has no UNC size"));
        assert!(error(0.30, 20.0, UnifiedSeries::Un).contains("not a standard Unified diameter"));
        assert!(error(0.25, 19.3, UnifiedSeries::Uns).contains("whole or half"));
        UnifiedThread::new(
            0.30 * 25.4,
            1.27,
            UnifiedSeries::Uns,
            UnifiedClass::TwoA,
            false,
        )
        .unwrap();
        assert!(unified_fit(&nut, &bolt, None).is_err());
        let spec = UnifiedThreadSpec {
            series: UnifiedSeries::Unc,
            class: UnifiedClass::TwoB,
            rounded_root: false,
            length_of_engagement: None,
        };
        assert!(
            spec.thread(6.35, 1.27, false)
                .unwrap_err()
                .message
                .contains("internal threads")
        );
    }
}
