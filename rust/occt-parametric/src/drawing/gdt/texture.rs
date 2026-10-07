//! Surface texture requirements (ASME B46.1 parameters) drawn with the
//! ASME Y14.36 symbol: roughness limits, sampling length, waviness, lay,
//! material-removal requirement and production method.
use super::*;

/// Roughness parameters of ASME B46.1, each a height in the stated unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RoughnessParameter {
    /// Arithmetic mean deviation of the profile.
    Ra,
    /// Root-mean-square deviation of the profile.
    Rq,
    /// Mean peak-to-valley height over sampling lengths.
    Rz,
    /// Largest single peak-to-valley height in the evaluation length.
    Rmax,
}

impl RoughnessParameter {
    fn symbol(self) -> &'static str {
        match self {
            Self::Ra => "Ra",
            Self::Rq => "Rq",
            Self::Rz => "Rz",
            Self::Rmax => "Rmax",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RoughnessUnit {
    Micrometer,
    Microinch,
}

impl RoughnessUnit {
    fn symbol(self) -> &'static str {
        match self {
            Self::Micrometer => "µm",
            Self::Microinch => "µin",
        }
    }
}

/// Direction of the predominant surface pattern (Y14.36 lay symbols).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceLay {
    /// `=`: parallel to the line representing the surface in the view.
    Parallel,
    /// `⊥`: perpendicular to that line.
    Perpendicular,
    /// `X`: angular in both directions.
    Crossed,
    /// `M`: multidirectional.
    Multidirectional,
    /// `C`: approximately circular about the surface center.
    Circular,
    /// `R`: approximately radial from the surface center.
    Radial,
    /// `P`: particulate, nondirectional or protuberant.
    Particulate,
}

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum MaterialRemoval {
    /// Basic symbol: any production method.
    #[default]
    Any,
    /// Bar across the symbol: material removal by machining is required.
    Required,
    /// Circle in the symbol: material removal is prohibited.
    Prohibited,
}

/// Roughness limits. A single value is a maximum; with a minimum it is a
/// range. Values use the requirement's unit.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RoughnessLimits {
    pub parameter: RoughnessParameter,
    pub maximum: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum: Option<f64>,
}

/// Waviness height (maximum) and spacing, both in millimeters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Waviness {
    pub height_mm: f64,
    pub spacing_mm: f64,
}

/// A surface texture requirement shown on a drawing, attached like a GD&T
/// control: the leader points at the anchor datum on the selected output.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DrawingSurfaceTexture {
    pub id: String,
    pub attachment: DrawingGdtAttachment,
    pub unit: RoughnessUnit,
    pub roughness: RoughnessLimits,
    /// Roughness sampling length (cutoff); one of B46.1's standard values.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cutoff_mm: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waviness: Option<Waviness>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lay: Option<SurfaceLay>,
    #[serde(default)]
    pub material_removal: MaterialRemoval,
    /// Production method note, such as `GRIND`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    /// The requirement applies all around the outline in the view.
    #[serde(default)]
    pub all_around: bool,
}

/// B46.1 standard roughness sampling lengths (cutoffs), in millimeters.
pub const STANDARD_CUTOFFS_MM: [f64; 5] = [0.08, 0.25, 0.8, 2.5, 8.0];

fn positive(value: f64, what: &str) -> Result<(), ModelError> {
    if value.is_finite() && value > 0.0 && value <= 1e6 {
        Ok(())
    } else {
        Err(ModelError::new(format!(
            "surface texture {what} must be positive, finite and at most 1e6"
        )))
    }
}

/// Validates ids, attachments and values. O(textures).
pub(in crate::drawing) fn validate(
    textures: &[DrawingSurfaceTexture],
    views: &HashMap<&str, &DrawingView>,
    graph: &InstanceGraph<'_>,
) -> Result<(), ModelError> {
    let mut ids = HashSet::new();
    for texture in textures {
        if texture.id.is_empty() || !ids.insert(texture.id.as_str()) {
            return Err(ModelError::new("surface textures need unique nonempty IDs"));
        }
        anchor(&texture.attachment, views, graph)?;
        let limits = &texture.roughness;
        positive(limits.maximum, "maximum")?;
        if let Some(minimum) = limits.minimum {
            positive(minimum, "minimum")?;
            if minimum >= limits.maximum {
                return Err(ModelError::new(
                    "surface texture minimum must be below its maximum",
                ));
            }
        }
        if let Some(cutoff) = texture.cutoff_mm
            && !STANDARD_CUTOFFS_MM.contains(&cutoff)
        {
            return Err(ModelError::new(
                "surface texture cutoff must be a B46.1 standard value: 0.08, 0.25, 0.8, 2.5 or 8 mm",
            ));
        }
        if let Some(waviness) = &texture.waviness {
            positive(waviness.height_mm, "waviness height")?;
            positive(waviness.spacing_mm, "waviness spacing")?;
        }
        if let Some(method) = &texture.method
            && (method.trim().is_empty()
                || method.len() > 40
                || !method.chars().all(|c| c.is_ascii_graphic() || c == ' '))
        {
            return Err(ModelError::new(
                "surface texture method must be 1-40 printable ASCII characters",
            ));
        }
    }
    Ok(())
}

/// Shortest decimal text of a value.
fn value(value: f64) -> String {
    format!("{value}")
}

fn roughness_text(texture: &DrawingSurfaceTexture) -> String {
    let limits = &texture.roughness;
    let range = match limits.minimum {
        Some(minimum) => format!("{}-{}", value(minimum), value(limits.maximum)),
        None => value(limits.maximum),
    };
    format!(
        "{} {range} {}",
        limits.parameter.symbol(),
        texture.unit.symbol()
    )
}

/// Whether the symbol needs its horizontal extension for notes.
fn extended(texture: &DrawingSurfaceTexture) -> bool {
    texture.method.is_some()
        || texture.waviness.is_some()
        || texture.cutoff_mm.is_some()
        || texture.lay.is_some()
        || texture.all_around
}

// Symbol geometry in paper millimeters, relative to the vertex: the short
// leg rises 60° to the left, the long leg 60° to the right, twice as long.
const SHORT: [f64; 2] = [-2.5, 4.33];
const LONG: [f64; 2] = [5.0, 8.66];
/// Approximate label character width (as GD&T cells assume), for layout.
const CHAR_MM: f64 = 2.0;

fn width(text: &str) -> f64 {
    text.chars().count() as f64 * CHAR_MM
}

/// Notes under the extension bar: sampling length and waviness.
fn below_bar(texture: &DrawingSurfaceTexture) -> String {
    let mut below = Vec::new();
    if let Some(cutoff) = texture.cutoff_mm {
        below.push(format!("Lc {}", value(cutoff)));
    }
    if let Some(waviness) = &texture.waviness {
        below.push(format!(
            "W {}-{}",
            value(waviness.height_mm),
            value(waviness.spacing_mm)
        ));
    }
    below.join("  ")
}

/// Draws each symbol at its attachment's paper offset, with a leader from
/// the anchored surface point to the symbol's vertex. O(textures).
pub(in crate::drawing) fn append(
    textures: &[DrawingSurfaceTexture],
    views: &HashMap<&str, &DrawingView>,
    graph: &InstanceGraph<'_>,
    d: &mut GeneratedDrawing,
) -> Result<(), ModelError> {
    for texture in textures {
        let (point, [x, y]) = anchor(&texture.attachment, views, graph)?;
        let at = |p: [f64; 2]| [x + p[0], y + p[1]];
        leader(d, point, [x, y], false);
        line(d, vec![at(SHORT), [x, y], at(LONG)]);
        match texture.material_removal {
            MaterialRemoval::Any => {}
            MaterialRemoval::Required => line(d, vec![at(SHORT), at([-SHORT[0], SHORT[1]])]),
            MaterialRemoval::Prohibited => {
                circle(d, at([0.0, 1.44]), 1.44, 0.0, std::f64::consts::TAU)
            }
        }
        // Roughness limits sit above the short leg, ending before the long leg.
        let roughness = roughness_text(texture);
        text(d, at([2.0 - width(&roughness), 5.5]), roughness);
        if extended(texture) {
            let top = at(LONG);
            // The bar spans the method above it and the notes and lay below.
            let below = below_bar(texture);
            let lay_room = if texture.lay.is_some() { 5.0 } else { 0.0 };
            let bar = [
                14.0,
                width(texture.method.as_deref().unwrap_or("")) + 3.0,
                width(&below) + 3.0 + lay_room,
            ]
            .into_iter()
            .fold(0.0, f64::max);
            line(d, vec![top, [top[0] + bar, top[1]]]);
            if texture.all_around {
                circle(d, top, 1.2, 0.0, std::f64::consts::TAU);
            }
            if let Some(method) = &texture.method {
                text(d, [top[0] + 1.5, top[1] + 1.2], method.clone());
            }
            if !below.is_empty() {
                text(d, [top[0] + 1.5, top[1] - 3.2], below);
            }
            if let Some(lay) = texture.lay {
                let lay_at = [top[0] + bar - 3.5, top[1] - 3.2];
                lay_symbol(d, lay_at, lay);
            }
        }
    }
    Ok(())
}

fn lay_symbol(d: &mut GeneratedDrawing, p: [f64; 2], lay: SurfaceLay) {
    let letter = match lay {
        SurfaceLay::Parallel => "=",
        SurfaceLay::Crossed => "X",
        SurfaceLay::Multidirectional => "M",
        SurfaceLay::Circular => "C",
        SurfaceLay::Radial => "R",
        SurfaceLay::Particulate => "P",
        SurfaceLay::Perpendicular => {
            // ⊥ drawn as strokes, independent of the text font.
            line(d, vec![[p[0], p[1]], [p[0] + 2.4, p[1]]]);
            line(d, vec![[p[0] + 1.2, p[1]], [p[0] + 1.2, p[1] + 2.4]]);
            return;
        }
    };
    text(d, p, letter.into());
}

/// Exact line vertices `append` adds, for the drawing's vertex budget.
pub(in crate::drawing) fn vertex_count(
    textures: &[DrawingSurfaceTexture],
) -> Result<usize, ModelError> {
    textures.iter().try_fold(0usize, |sum, texture| {
        // Leader (arrowhead 3 + shaft 2) and the V (3).
        let mut count = 8;
        count += match texture.material_removal {
            MaterialRemoval::Any => 0,
            MaterialRemoval::Required => 2,
            MaterialRemoval::Prohibited => 33,
        };
        if extended(texture) {
            count += 2;
            if texture.all_around {
                count += 33;
            }
            if texture.lay == Some(SurfaceLay::Perpendicular) {
                count += 4;
            }
        }
        sum.checked_add(count)
            .ok_or_else(|| ModelError::new("surface texture vertex count overflow"))
    })
}
