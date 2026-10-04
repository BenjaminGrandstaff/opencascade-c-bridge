//! Exact panel integrals for an explicitly supplied symmetric wing planform.
use super::*;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlanformStation {
    /// Distance outboard from the symmetry plane (zero at the root).
    pub span: Quantity,
    pub chord: Quantity,
    /// Leading-edge point in world coordinates. The reported MAC leading edge
    /// is the chord-area-weighted point on this representative half-wing.
    pub leading_edge: VectorQuantity,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SymmetricWingPlanform {
    /// Root to tip: piecewise-linear chord and leading edge; 2–10000 stations.
    pub stations: Vec<PlanformStation>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeanAerodynamicChord {
    pub length_mm: f64,
    pub leading_edge_mm: Vec3,
    /// Both symmetric halves, using the supplied span distances and chords.
    pub planform_area_mm2: f64,
}

impl MeanAerodynamicChord {
    pub fn reference(self, direction: VectorQuantity) -> ChordReference {
        let point = self.leading_edge_mm;
        ChordReference {
            leading_edge: VectorQuantity::lengths(
                point.x,
                point.y,
                point.z,
                LengthUnit::Millimeter,
            ),
            direction,
            length: Quantity::length(self.length_mm, LengthUnit::Millimeter),
        }
    }
}

struct Station {
    span: f64,
    chord: f64,
    leading: Vec3,
}

fn normalized(station: &PlanformStation) -> Result<Station, ModelError> {
    if station.span.dimension != Dimension::Length || station.chord.dimension != Dimension::Length {
        return Err(ModelError::new("planform span and chord must be lengths"));
    }
    let span = station.span.normalized()?;
    let chord = station.chord.normalized()?;
    let leading = station.leading_edge.normalized(Dimension::Length)?;
    if !(span.is_finite() && span >= 0.0 && chord.is_finite() && chord >= 0.0) {
        return Err(ModelError::new(
            "planform spans and chords must be finite and nonnegative",
        ));
    }
    Ok(Station {
        span,
        chord,
        leading,
    })
}

impl SymmetricWingPlanform {
    /// MAC = integral(c² dy) / integral(c dy), with exact linear-panel
    /// integrals. Leading edge = integral(LE c dy) / integral(c dy).
    /// O(stations) time, O(1) storage; no BREP or aerodynamic simulation.
    pub fn mean_aerodynamic_chord(&self) -> Result<MeanAerodynamicChord, ModelError> {
        if !(2..=10_000).contains(&self.stations.len()) {
            return Err(ModelError::new("planform needs 2–10000 stations"));
        }
        let mut previous = normalized(&self.stations[0])?;
        if previous.span != 0.0 {
            return Err(ModelError::new(
                "symmetric planform must begin at span zero",
            ));
        }
        let mut area = 0.0;
        let mut mac = 0.0;
        let mut leading = previous.leading;
        for station in &self.stations[1..] {
            let next = normalized(station)?;
            if next.span <= previous.span {
                return Err(ModelError::new("planform spans must increase strictly"));
            }
            let (panel_area, panel_mac, panel_leading) = panel(&previous, &next);
            let total_area = area + panel_area;
            if !(panel_area.is_finite() && panel_area > 0.0 && total_area.is_finite()) {
                return Err(ModelError::new(
                    "planform area exceeds finite positive measurement limits",
                ));
            }
            let fraction = panel_area / total_area;
            mac = (1.0 - fraction) * mac + fraction * panel_mac;
            leading = add(leading, scale(subtract(panel_leading, leading), fraction));
            area = total_area;
            previous = next;
        }
        let result = MeanAerodynamicChord {
            length_mm: mac,
            leading_edge_mm: leading,
            planform_area_mm2: 2.0 * area,
        };
        // Reuse chord validation for finite reference coordinates/length.
        result
            .reference(VectorQuantity::scalars(1.0, 0.0, 0.0))
            .locate(leading)?;
        if !result.planform_area_mm2.is_finite() {
            return Err(ModelError::new(
                "full planform area exceeds finite measurement limits",
            ));
        }
        Ok(result)
    }
}

fn panel(a: &Station, b: &Station) -> (f64, f64, Vec3) {
    let area = (b.span - a.span) * (a.chord * 0.5 + b.chord * 0.5);
    // Normalize before squaring: finite usable MACs should not overflow c².
    let maximum = a.chord.max(b.chord);
    let (ca, cb) = (a.chord / maximum, b.chord / maximum);
    let mac = maximum * ((2.0 / 3.0) * (ca * ca + ca * cb + cb * cb) / (ca + cb));
    let fraction = (ca + 2.0 * cb) / (3.0 * (ca + cb));
    let leading = add(a.leading, scale(subtract(b.leading, a.leading), fraction));
    (area, mac, leading)
}
