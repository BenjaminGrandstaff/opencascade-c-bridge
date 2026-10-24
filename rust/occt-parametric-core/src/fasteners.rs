//! Fastened joints (schema 186): a socket head cap screw through a stack of
//! parts, held by a nut or by a tapped hole in the last part, with the screw
//! length, hardware seats and matching holes planned from the stack's
//! measured thickness along the screw axis.
//!
//! Planning is kernel-free: the kernel measures where each stack part's
//! material starts and ends along each screw's axis line, and
//! [`plan_joint`] turns those spans into a screw length, hardware seats and
//! hole cuts, or refuses a joint that cannot be assembled.
//!
//! Standard screw lengths per size are ISO 4762's, as tabulated by Engineers
//! Edge (<https://engineersedge.com/iso_socket_head_screw.htm>, read
//! 2026-10-23). That table has no M14 row, so M14 joints need an explicit
//! `length`.
use crate::error::ModelError;
use crate::hole_sizes::{
    ClearanceSeries, HoleCatalogSystem, carr_lane_socket_head_v1, carr_lane_tap_sizes_v1,
    iso273_clearance_v1,
};
use crate::quantity::{LengthUnit, Quantity};
use crate::standard_parts::{StandardPartData, StandardPartKind};
use serde::{Deserialize, Serialize};

/// Most parts one joint clamps.
pub const MAX_JOINT_STACK: usize = 8;
/// Most screws one joint places.
pub const MAX_JOINT_POSITIONS: usize = 1000;
/// Largest gap or overlap between neighboring stack parts, in millimeters,
/// still treated as contact.
pub const JOINT_CONTACT_TOLERANCE: f64 = 1e-3;
/// Material left beyond the tip of a blind tapped hole, in millimeters.
const TAPPED_FLOOR: f64 = 0.5;

/// How the screw is held at the far end of the stack.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum JointFastening {
    /// A hex nut (ISO 4032) behind the last part.
    #[default]
    Nut,
    /// A tapped hole in the last part, which is not clamped.
    Tapped,
}

/// Where the screw head sits.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum JointHead {
    /// On the first part's surface, on a washer when the joint has washers.
    #[default]
    Surface,
    /// Sunk flush in a counterbore in the first part (Carr Lane V1 metric
    /// socket-head recess), with no washer under it.
    Counterbored,
}

/// A joint's evaluated settings, in millimeters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct JointSettings {
    pub nominal_diameter: f64,
    pub fastening: JointFastening,
    pub head: JointHead,
    pub washers: bool,
    pub clearance: ClearanceSeries,
    /// Screw length under the head; None picks the shortest standard length.
    pub length: Option<f64>,
    /// Least thread engagement in a tapped part; None means 1.5 diameters.
    pub engagement: Option<f64>,
}

/// Where one stack part's material starts and ends along a screw's axis
/// line, as distances along the unit axis from the screw's position.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StackSpan {
    pub entry: f64,
    pub exit: f64,
}

/// What a hole in one stack part cuts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum JointHoleKind {
    /// A through hole, with a counterbore (diameter, depth) at its entry.
    Clearance {
        diameter: f64,
        counterbore: Option<(f64, f64)>,
    },
    /// A blind tap-drill hole of full-diameter `depth` and a 118° drill
    /// point; the screw's thread is not modeled.
    Tapped { drill: f64, depth: f64, pitch: f64 },
}

/// The hole one stack part needs at one screw.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct JointHole {
    pub kind: JointHoleKind,
    pub entry: f64,
    pub exit: f64,
}

/// Hardware seats along one screw's axis, as distances from its position.
#[derive(Clone, Debug, PartialEq)]
pub struct JointScrew {
    /// The underside of the screw head.
    pub seat: f64,
    pub head_washer: Option<f64>,
    pub nut_washer: Option<f64>,
    pub nut: Option<f64>,
    /// One hole per stack part, in stack order.
    pub holes: Vec<JointHole>,
    /// Thread engaged in a tapped part, or past the nut's far face.
    pub thread_engagement: f64,
    /// Thickness clamped between the head (or its washer) and the nut (or
    /// its washer) or tapped part.
    pub grip: f64,
}

/// A planned joint: one screw size and length for every position.
#[derive(Clone, Debug, PartialEq)]
pub struct JointPlan {
    pub nominal_diameter: f64,
    pub pitch: f64,
    pub length: f64,
    /// Whether `length` was picked from the standard lengths.
    pub chosen: bool,
    /// The least length every position needs.
    pub minimum_length: f64,
    pub screws: Vec<JointScrew>,
    /// Largest hardware or hole diameter, for spacing checks.
    pub footprint: f64,
}

/// A line of the joint's parts list.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct HardwareItem {
    pub part: StandardPartKind,
    pub designation: String,
    pub quantity: usize,
}

/// A run of nominal lengths: first, last and step, in millimeters.
type LengthRun = (f64, f64, f64);

/// ISO 4762 nominal lengths: size, then its runs.
const SCREW_LENGTHS: [(f64, &[LengthRun]); 9] = [
    (
        3.0,
        &[
            (5.0, 5.0, 1.0),
            (6.0, 12.0, 2.0),
            (16.0, 16.0, 1.0),
            (20.0, 30.0, 5.0),
        ],
    ),
    (
        4.0,
        &[(6.0, 12.0, 2.0), (16.0, 16.0, 1.0), (20.0, 40.0, 5.0)],
    ),
    (
        5.0,
        &[(8.0, 12.0, 2.0), (16.0, 16.0, 1.0), (20.0, 50.0, 5.0)],
    ),
    (
        6.0,
        &[(10.0, 12.0, 2.0), (16.0, 16.0, 1.0), (20.0, 60.0, 5.0)],
    ),
    (
        8.0,
        &[
            (12.0, 12.0, 1.0),
            (16.0, 16.0, 1.0),
            (20.0, 70.0, 5.0),
            (80.0, 80.0, 1.0),
        ],
    ),
    (
        10.0,
        &[(16.0, 16.0, 1.0), (20.0, 70.0, 5.0), (80.0, 100.0, 10.0)],
    ),
    (12.0, &[(20.0, 70.0, 5.0), (80.0, 120.0, 10.0)]),
    (16.0, &[(25.0, 70.0, 5.0), (80.0, 160.0, 10.0)]),
    (
        20.0,
        &[(30.0, 70.0, 5.0), (80.0, 160.0, 10.0), (180.0, 200.0, 20.0)],
    ),
];

/// The ISO 4762 nominal lengths of one screw size, ascending; empty for a
/// size the length table does not cover.
pub fn standard_screw_lengths(nominal: f64) -> Vec<f64> {
    SCREW_LENGTHS
        .iter()
        .find(|(size, _)| (size - nominal).abs() < 1e-9)
        .map(|(_, runs)| {
            runs.iter()
                .flat_map(|&(first, last, step)| {
                    let count = ((last - first) / step).round() as usize;
                    (0..=count).map(move |k| first + step * k as f64)
                })
                .collect()
        })
        .unwrap_or_default()
}

fn mm(value: f64) -> Quantity {
    Quantity::length(value, LengthUnit::Millimeter)
}

fn number(value: f64) -> String {
    let rounded = (value * 1000.0).round() / 1000.0;
    format!("{rounded}")
}

/// Plans a joint from the measured `spans` (per position, one per stack
/// part, in stack order). `names` are the stack's feature ids, for messages.
/// O(positions · stack + standard lengths); spacing is checked separately by
/// [`check_spacing`].
pub fn plan_joint(
    settings: &JointSettings,
    names: &[&str],
    spans: &[Vec<StackSpan>],
) -> Result<JointPlan, ModelError> {
    let d = settings.nominal_diameter;
    let screw = StandardPartKind::SocketHeadCapScrewIso4762;
    let StandardPartData::Screw {
        head_diameter,
        thread_length,
        ..
    } = screw.data(d)?
    else {
        unreachable!("screw rows are screws");
    };
    let tapped = settings.fastening == JointFastening::Tapped;
    if names.is_empty() || names.len() > MAX_JOINT_STACK {
        return Err(ModelError::new(format!(
            "a fastened joint stacks 1 to {MAX_JOINT_STACK} parts"
        )));
    }
    if tapped && names.len() < 2 {
        return Err(ModelError::new(
            "a tapped joint needs at least two parts: the clamped ones and the tapped one",
        ));
    }
    let tap = carr_lane_tap_sizes_v1(HoleCatalogSystem::Metric)
        .into_iter()
        .find(|tap| (tap.nominal_mm - d).abs() < 1e-9)
        .ok_or_else(|| ModelError::new(format!("no metric tap drill for M{}", number(d))))?;
    let pitch = tap.pitch_mm;
    let clearance = iso273_clearance_v1(mm(d), settings.clearance)?.value;
    let washer = StandardPartKind::PlainWasherIso7089.data(d)?;
    let StandardPartData::Washer {
        thickness: washer_t,
        outer_diameter: washer_od,
        ..
    } = washer
    else {
        unreachable!("washer rows are washers");
    };
    let StandardPartData::Nut {
        height: nut_m,
        across_corners,
        ..
    } = StandardPartKind::HexNutIso4032.data(d)?
    else {
        unreachable!("nut rows are nuts");
    };
    let counterbore = match settings.head {
        JointHead::Surface => None,
        JointHead::Counterbored => {
            let recess = carr_lane_socket_head_v1(HoleCatalogSystem::Metric, mm(d))?;
            Some((
                recess.counterbore_diameter.value,
                recess.counterbore_depth.value,
            ))
        }
    };
    let engagement = settings.engagement.unwrap_or(1.5 * d);
    if !engagement.is_finite() || engagement <= 0.0 {
        return Err(ModelError::new(
            "joint thread engagement must be a positive finite length",
        ));
    }
    let head_washer = settings.washers && settings.head == JointHead::Surface;
    let nut_washer = settings.washers && !tapped;
    let tip_depth = |drill: f64| drill / 2.0 / 59f64.to_radians().tan();

    // Seats and the least length at each position.
    struct Layout {
        seat: f64,
        clamp_end: f64,
        minimum: f64,
    }
    let mut layouts = Vec::with_capacity(spans.len());
    for (index, stack) in spans.iter().enumerate() {
        let at = || format!("at joint position {}", index + 1);
        if stack.len() != names.len() {
            return Err(ModelError::new("joint spans must cover every stack part"));
        }
        for (k, pair) in stack.windows(2).enumerate() {
            let gap = pair[1].entry - pair[0].exit;
            if gap.abs() > JOINT_CONTACT_TOLERANCE {
                let how = if gap > 0.0 {
                    format!("are {} mm apart", number(gap))
                } else {
                    format!("overlap by {} mm", number(-gap))
                };
                return Err(ModelError::new(format!(
                    "parts '{}' and '{}' {how} along the screw axis {}; a joint clamps parts that touch, in stack order",
                    names[k],
                    names[k + 1],
                    at()
                )));
            }
        }
        let first = stack[0];
        let seat = match counterbore {
            Some((_, depth)) => {
                if depth >= first.exit - first.entry - JOINT_CONTACT_TOLERANCE {
                    return Err(ModelError::new(format!(
                        "the {} mm counterbore does not fit in '{}', which is {} mm thick {}",
                        number(depth),
                        names[0],
                        number(first.exit - first.entry),
                        at()
                    )));
                }
                first.entry + depth
            }
            None if head_washer => first.entry - washer_t,
            None => first.entry,
        };
        let last = stack[stack.len() - 1];
        let (clamp_end, minimum) = if tapped {
            // The thread starts at the tapped part's face.
            (last.entry, last.entry + engagement - seat)
        } else {
            let nut_face = last.exit + if nut_washer { washer_t } else { 0.0 };
            // At least two threads past the nut.
            (nut_face, nut_face + nut_m + 2.0 * pitch - seat)
        };
        layouts.push(Layout {
            seat,
            clamp_end,
            minimum,
        });
    }
    let minimum_length = layouts
        .iter()
        .map(|layout| layout.minimum)
        .fold(0.0, f64::max);
    let (length, chosen) = match settings.length {
        Some(length) => {
            if !length.is_finite() || length <= 0.0 {
                return Err(ModelError::new(
                    "joint screw length must be a positive finite length",
                ));
            }
            if length < minimum_length - 1e-9 {
                return Err(ModelError::new(format!(
                    "an M{} × {} screw is too short for this joint: it needs at least {} mm",
                    number(d),
                    number(length),
                    number(minimum_length)
                )));
            }
            (length, false)
        }
        None => {
            let lengths = standard_screw_lengths(d);
            if lengths.is_empty() {
                return Err(ModelError::new(format!(
                    "no standard lengths are tabulated for M{} screws; give the joint a length",
                    number(d)
                )));
            }
            let length = lengths
                .iter()
                .copied()
                .find(|&l| l >= minimum_length - 1e-9)
                .ok_or_else(|| {
                    ModelError::new(format!(
                        "this joint needs an M{} screw at least {} mm long; the longest standard one is {} mm",
                        number(d),
                        number(minimum_length),
                        number(*lengths.last().expect("nonempty"))
                    ))
                })?;
            (length, true)
        }
    };
    let name = format!("M{} × {}", number(d), number(length));
    let mut screws = Vec::with_capacity(spans.len());
    for (index, (stack, layout)) in spans.iter().zip(&layouts).enumerate() {
        let at = || format!("at joint position {}", index + 1);
        let end = layout.seat + length;
        // The thread runs b from the screw's end; it must start before the
        // nut or tapped part, or the screw cannot clamp.
        let thread_start = end - thread_length;
        if thread_start > layout.clamp_end + JOINT_CONTACT_TOLERANCE {
            return Err(ModelError::new(format!(
                "the {name} screw's thread ({} mm) starts {} mm short of the {} {}; {}",
                number(thread_length),
                number(thread_start - layout.clamp_end),
                if tapped { "tapped part" } else { "nut" },
                at(),
                // A chosen length is already the shortest the joint allows.
                match (chosen, tapped) {
                    (false, _) => "use a shorter screw",
                    (true, true) => "ask for less thread engagement",
                    (true, false) => "no standard screw is threaded far enough for this stack",
                }
            )));
        }
        let mut holes = Vec::with_capacity(stack.len());
        let clamped = if tapped { stack.len() - 1 } else { stack.len() };
        for (k, span) in stack.iter().take(clamped).enumerate() {
            holes.push(JointHole {
                kind: JointHoleKind::Clearance {
                    diameter: clearance,
                    counterbore: if k == 0 { counterbore } else { None },
                },
                entry: span.entry,
                exit: span.exit,
            });
        }
        let thread_engagement;
        if tapped {
            let last = stack[stack.len() - 1];
            thread_engagement = end - last.entry;
            // Two pitches of tapped thread beyond the screw's end.
            let depth = thread_engagement + 2.0 * pitch;
            let bottom = last.entry + depth + tip_depth(tap.drill_mm);
            if bottom > last.exit - TAPPED_FLOOR {
                return Err(ModelError::new(format!(
                    "the {name} screw's tapped hole in '{}' needs {} mm but the part is {} mm thick {}; use a shorter screw or a nut",
                    names[names.len() - 1],
                    number(bottom - last.entry + TAPPED_FLOOR),
                    number(last.exit - last.entry),
                    at()
                )));
            }
            holes.push(JointHole {
                kind: JointHoleKind::Tapped {
                    drill: tap.drill_mm,
                    depth,
                    pitch,
                },
                entry: last.entry,
                exit: last.exit,
            });
        } else {
            thread_engagement = end - (layout.clamp_end + nut_m);
        }
        let first = stack[0];
        let last = stack[stack.len() - 1];
        screws.push(JointScrew {
            seat: layout.seat,
            head_washer: head_washer.then_some(first.entry - washer_t),
            nut_washer: nut_washer.then_some(last.exit),
            nut: (!tapped).then_some(layout.clamp_end),
            holes,
            thread_engagement,
            grip: layout.clamp_end - layout.seat,
        });
    }
    let mut footprint = head_diameter.max(clearance);
    if let Some((diameter, _)) = counterbore {
        footprint = footprint.max(diameter);
    }
    if settings.washers {
        footprint = footprint.max(washer_od);
    }
    if !tapped {
        footprint = footprint.max(across_corners);
    }
    Ok(JointPlan {
        nominal_diameter: d,
        pitch,
        length,
        chosen,
        minimum_length,
        screws,
        footprint,
    })
}

/// Refuses screws whose hardware or holes would collide: `offsets` are
/// each position's coordinates across the axis, in millimeters. Sorting by
/// one coordinate keeps this near O(n log n) for spread-out layouts.
pub fn check_spacing(offsets: &[[f64; 2]], footprint: f64) -> Result<(), ModelError> {
    let mut order: Vec<usize> = (0..offsets.len()).collect();
    order.sort_by(|&a, &b| offsets[a][0].total_cmp(&offsets[b][0]));
    for (i, &a) in order.iter().enumerate() {
        for &b in &order[i + 1..] {
            if offsets[b][0] - offsets[a][0] >= footprint {
                break;
            }
            let distance = (offsets[b][0] - offsets[a][0]).hypot(offsets[b][1] - offsets[a][1]);
            if distance < footprint - 1e-9 {
                let (first, second) = (a.min(b) + 1, a.max(b) + 1);
                return Err(ModelError::new(format!(
                    "joint positions {first} and {second} are {} mm apart; their hardware and holes need {} mm",
                    number(distance),
                    number(footprint)
                )));
            }
        }
    }
    Ok(())
}

impl JointPlan {
    /// The joint as build reports and the studio show it: the screw, its
    /// fit and the parts list.
    pub fn report(&self, feature: &str, stack: &[String]) -> serde_json::Value {
        let range = |pick: fn(&JointScrew) -> f64| {
            let values = self.screws.iter().map(pick);
            let min = values.clone().fold(f64::INFINITY, f64::min);
            let max = values.fold(f64::NEG_INFINITY, f64::max);
            serde_json::json!({"min": min, "max": max})
        };
        serde_json::json!({
            "feature": feature,
            "stack": stack,
            "positions": self.screws.len(),
            "screw": {
                "nominal_diameter_mm": self.nominal_diameter,
                "pitch_mm": self.pitch,
                "length_mm": self.length,
                "chosen": self.chosen,
                "minimum_length_mm": self.minimum_length,
            },
            "grip_mm": range(|s| s.grip),
            "thread_engagement_mm": range(|s| s.thread_engagement),
            "hardware": self.hardware(),
        })
    }

    /// The joint's parts list, screws first.
    pub fn hardware(&self) -> Vec<HardwareItem> {
        let d = number(self.nominal_diameter);
        let count = self.screws.len();
        let mut items = vec![HardwareItem {
            part: StandardPartKind::SocketHeadCapScrewIso4762,
            designation: format!("ISO 4762 M{d} × {}", number(self.length)),
            quantity: count,
        }];
        let washers: usize = self
            .screws
            .iter()
            .map(|s| usize::from(s.head_washer.is_some()) + usize::from(s.nut_washer.is_some()))
            .sum();
        if washers > 0 {
            items.push(HardwareItem {
                part: StandardPartKind::PlainWasherIso7089,
                designation: format!("ISO 7089 {d}"),
                quantity: washers,
            });
        }
        let nuts = self.screws.iter().filter(|s| s.nut.is_some()).count();
        if nuts > 0 {
            items.push(HardwareItem {
                part: StandardPartKind::HexNutIso4032,
                designation: format!("ISO 4032 M{d}"),
                quantity: nuts,
            });
        }
        items
    }
}

#[cfg(test)]
mod tests;
