//! Structured drawing GD&T intent. No inspection or datum-simulator solving.
use super::*;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DrawingGdtAttachment {
    pub view: String,
    /// Selected solid output; anchor supplies the current leader attachment.
    pub output: InstanceOutputRef,
    pub anchor: DatumRef,
    pub offset_mm: [f64; 2],
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DrawingDatumFeature {
    pub id: String,
    pub label: String,
    /// Explicit manufacturing declaration, not inferred from the anchor kind.
    pub feature_of_size: bool,
    pub attachment: DrawingGdtAttachment,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeometricCharacteristic {
    Straightness,
    Flatness,
    Circularity,
    Cylindricity,
    ProfileLine,
    ProfileSurface,
    Parallelism,
    Perpendicularity,
    Angularity,
    Position,
    CircularRunout,
    TotalRunout,
}
impl GeometricCharacteristic {
    fn form(self) -> bool {
        matches!(
            self,
            Self::Straightness | Self::Flatness | Self::Circularity | Self::Cylindricity
        )
    }
    fn profile(self) -> bool {
        matches!(self, Self::ProfileLine | Self::ProfileSurface)
    }
    fn diameter(self) -> bool {
        matches!(
            self,
            Self::Straightness
                | Self::Parallelism
                | Self::Perpendicularity
                | Self::Angularity
                | Self::Position
        )
    }
    fn material(self) -> bool {
        matches!(
            self,
            Self::Straightness
                | Self::Flatness
                | Self::Parallelism
                | Self::Perpendicularity
                | Self::Angularity
                | Self::Position
        )
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeometricToleranceZone {
    #[default]
    Characteristic,
    Diameter,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToleranceMaterialCondition {
    #[default]
    Regardless,
    Maximum,
    Least,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DatumMaterialBoundary {
    #[default]
    Regardless,
    Maximum,
    Least,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DrawingDatumReference {
    pub datum_feature: String,
    #[serde(default)]
    pub boundary: DatumMaterialBoundary,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DrawingFeatureControlFrame {
    pub id: String,
    pub attachment: DrawingGdtAttachment,
    pub characteristic: GeometricCharacteristic,
    pub tolerance: Quantity,
    pub display_unit: LengthUnit,
    pub precision: u8,
    #[serde(default)]
    pub zone: GeometricToleranceZone,
    #[serde(default)]
    pub material: ToleranceMaterialCondition,
    pub feature_of_size: bool,
    #[serde(default)]
    pub datums: Vec<DrawingDatumReference>,
}
fn anchor(
    a: &DrawingGdtAttachment,
    views: &HashMap<&str, &DrawingView>,
    graph: &InstanceGraph<'_>,
) -> Result<([f64; 2], [f64; 2]), ModelError> {
    let view = views
        .get(a.view.as_str())
        .ok_or_else(|| ModelError::new("GD&T attachment references an unknown view"))?;
    if !view.outputs.contains(&a.output)
        || a.anchor.instance != a.output.instance
        || graph.is_suppressed(&a.output.instance)
    {
        return Err(ModelError::new(
            "GD&T attachment needs a selected unsuppressed output and its own instance anchor",
        ));
    }
    let offset_length = a.offset_mm[0].hypot(a.offset_mm[1]);
    if !finite_pair(a.offset_mm) || !offset_length.is_finite() || offset_length < 8.0 {
        return Err(ModelError::new(
            "GD&T leader needs a finite paper offset at least 8 mm long",
        ));
    }
    let point = view.paper(view.project(datum_origin(graph, &a.anchor)?)?)?;
    let box_origin = [point[0] + a.offset_mm[0], point[1] + a.offset_mm[1]];
    if !finite_pair(box_origin) || !finite_pair([box_origin[0] + 160.0, box_origin[1] + 8.0]) {
        return Err(ModelError::new(
            "GD&T frame exceeds finite paper coordinates",
        ));
    }
    Ok((point, box_origin))
}
fn number(frame: &DrawingFeatureControlFrame) -> Result<String, ModelError> {
    if frame.tolerance.dimension != Dimension::Length {
        return Err(ModelError::new("geometric tolerance must be a length"));
    }
    let mm = frame.tolerance.normalized()?;
    if !mm.is_finite() || mm <= 0.0 || frame.precision > 8 {
        return Err(ModelError::new(
            "geometric tolerance needs a positive finite length and precision at most eight",
        ));
    }
    let value = mm / frame.display_unit.millimeter_factor();
    let text = format!("{value:.precision$}", precision = frame.precision as usize);
    if text.len() > 20 || text.parse::<f64>().unwrap_or(0.0) <= 0.0 {
        return Err(ModelError::new(
            "geometric tolerance exceeds its display cell or rounds to zero",
        ));
    }
    let unit = match frame.display_unit {
        LengthUnit::Millimeter => "mm",
        LengthUnit::Centimeter => "cm",
        LengthUnit::Meter => "m",
        LengthUnit::Inch => "in",
    };
    Ok(format!("{text} {unit}"))
}
pub(super) fn validate(
    features: &[DrawingDatumFeature],
    frames: &[DrawingFeatureControlFrame],
    views: &HashMap<&str, &DrawingView>,
    graph: &InstanceGraph<'_>,
) -> Result<(), ModelError> {
    let mut index = HashMap::new();
    let mut labels = HashSet::new();
    for feature in features {
        if feature.id.is_empty()
            || index.insert(feature.id.as_str(), feature).is_some()
            || !labels.insert(&feature.label)
            || feature.label.is_empty()
            || feature.label.len() > 3
            || !feature
                .label
                .bytes()
                .all(|c| c.is_ascii_uppercase() && !matches!(c, b'I' | b'O' | b'Q'))
        {
            return Err(ModelError::new(
                "datum features need unique IDs and unique one-to-three-letter uppercase labels excluding I/O/Q",
            ));
        }
        anchor(&feature.attachment, views, graph)?;
    }
    let mut ids = HashSet::new();
    for frame in frames {
        if frame.id.is_empty() || !ids.insert(&frame.id) {
            return Err(ModelError::new(
                "feature control frames need unique nonempty IDs",
            ));
        }
        anchor(&frame.attachment, views, graph)?;
        number(frame)?;
        validate_control(frame)?;
        validate_references(&frame.datums, &index)?;
    }
    Ok(())
}
fn validate_references(
    datums: &[DrawingDatumReference],
    index: &HashMap<&str, &DrawingDatumFeature>,
) -> Result<(), ModelError> {
    let mut references = HashSet::new();
    for reference in datums {
        let feature = index.get(reference.datum_feature.as_str()).ok_or_else(|| {
            ModelError::new("feature control frame references an unknown datum feature")
        })?;
        if !references.insert(&reference.datum_feature) {
            return Err(ModelError::new(
                "feature control frame repeats a datum feature",
            ));
        }
        if reference.boundary != DatumMaterialBoundary::Regardless && !feature.feature_of_size {
            return Err(ModelError::new(
                "datum material boundary requires a declared feature of size",
            ));
        }
    }
    Ok(())
}
fn validate_control(frame: &DrawingFeatureControlFrame) -> Result<(), ModelError> {
    let c = frame.characteristic;
    if frame.datums.len() > 3
        || (c.form() && !frame.datums.is_empty())
        || (!c.form() && !c.profile() && frame.datums.is_empty())
    {
        return Err(ModelError::new(
            "geometric control has unsupported datum count; form controls have none, orientation/location/runout require one to three",
        ));
    }
    if frame.zone == GeometricToleranceZone::Diameter && (!c.diameter() || !frame.feature_of_size) {
        return Err(ModelError::new(
            "diameter tolerance zone requires an axis control on a declared feature of size",
        ));
    }
    if c == GeometricCharacteristic::Position && frame.zone != GeometricToleranceZone::Diameter {
        return Err(ModelError::new(
            "initial position controls require a diameter zone",
        ));
    }
    if frame.material != ToleranceMaterialCondition::Regardless
        && (!c.material() || !frame.feature_of_size)
    {
        return Err(ModelError::new(
            "tolerance material modifier requires a supported feature-of-size control",
        ));
    }
    Ok(())
}
fn line(d: &mut GeneratedDrawing, p: Vec<[f64; 2]>) {
    d.gdt_lines.push(DrawingPolyline {
        points_mm: p,
        hidden: false,
    });
}
fn text(d: &mut GeneratedDrawing, p: [f64; 2], value: String) {
    d.gdt_labels.push(DrawingLabel {
        position_mm: p,
        text: value,
        stack: None,
    });
}
fn rectangle(d: &mut GeneratedDrawing, p: [f64; 2], w: f64) {
    let [x, y] = p;
    line(
        d,
        vec![[x, y], [x + w, y], [x + w, y + 8.0], [x, y + 8.0], [x, y]],
    );
}
fn leader(d: &mut GeneratedDrawing, point: [f64; 2], target: [f64; 2], closed: bool) {
    let delta = [target[0] - point[0], target[1] - point[1]];
    let n = delta[0].hypot(delta[1]);
    let u = [delta[0] / n, delta[1] / n];
    let base = [point[0] + 3.0 * u[0], point[1] + 3.0 * u[1]];
    let a = [base[0] - u[1], base[1] + u[0]];
    let b = [base[0] + u[1], base[1] - u[0]];
    line(
        d,
        if closed {
            vec![a, point, b, a]
        } else {
            vec![a, point, b]
        },
    );
    line(d, vec![base, target]);
}
fn circle(d: &mut GeneratedDrawing, p: [f64; 2], r: f64, start: f64, sweep: f64) {
    line(
        d,
        (0..=32)
            .map(|i| {
                let a = start + sweep * i as f64 / 32.0;
                [p[0] + r * a.cos(), p[1] + r * a.sin()]
            })
            .collect(),
    );
}
fn modifier(d: &mut GeneratedDrawing, p: [f64; 2], maximum: bool) {
    circle(d, p, 2.3, 0.0, std::f64::consts::TAU);
    text(
        d,
        [p[0] - 1.1, p[1] - 1.0],
        if maximum { "M" } else { "L" }.into(),
    );
}
fn glyph(d: &mut GeneratedDrawing, p: [f64; 2], c: GeometricCharacteristic) {
    let [x, y] = p;
    let mut draw =
        |points: &[[f64; 2]]| line(d, points.iter().map(|a| [x + a[0], y + a[1]]).collect());
    match c {
        GeometricCharacteristic::Straightness => draw(&[[-2.5, 0.0], [2.5, 0.0]]),
        GeometricCharacteristic::Flatness => draw(&[
            [-2.5, -1.5],
            [1.0, -1.5],
            [2.5, 1.5],
            [-1.0, 1.5],
            [-2.5, -1.5],
        ]),
        GeometricCharacteristic::Parallelism => {
            draw(&[[-2.5, -2.0], [-0.5, 2.0]]);
            draw(&[[0.5, -2.0], [2.5, 2.0]]);
        }
        GeometricCharacteristic::Perpendicularity => {
            draw(&[[-2.5, -2.0], [2.5, -2.0]]);
            draw(&[[0.0, -2.0], [0.0, 2.0]]);
        }
        GeometricCharacteristic::Angularity => draw(&[[2.5, -2.0], [-2.5, -2.0], [1.0, 2.0]]),
        GeometricCharacteristic::CircularRunout | GeometricCharacteristic::TotalRunout => {
            draw(&[[-2.0, -2.0], [1.5, 2.0], [0.0, 1.7], [1.5, 2.0], [1.3, 0.5]]);
            if c == GeometricCharacteristic::TotalRunout {
                draw(&[[-0.5, -2.0], [3.0, 2.0], [1.5, 1.7], [3.0, 2.0], [2.8, 0.5]]);
                draw(&[[-2.0, -2.0], [-0.5, -2.0]]);
            }
        }
        _ => curved_glyph(d, p, c),
    }
}
fn curved_glyph(d: &mut GeneratedDrawing, p: [f64; 2], c: GeometricCharacteristic) {
    let [x, y] = p;
    match c {
        GeometricCharacteristic::ProfileLine | GeometricCharacteristic::ProfileSurface => {
            circle(d, [x, y - 1.5], 2.5, 0.0, std::f64::consts::PI);
            if c == GeometricCharacteristic::ProfileSurface {
                line(d, vec![[x - 2.5, y - 1.5], [x + 2.5, y - 1.5]]);
            }
        }
        _ => {
            circle(d, p, 2.0, 0.0, std::f64::consts::TAU);
            if c == GeometricCharacteristic::Position {
                line(d, vec![[x - 3.0, y], [x + 3.0, y]]);
                line(d, vec![[x, y - 3.0], [x, y + 3.0]]);
            }
            if c == GeometricCharacteristic::Cylindricity {
                for dx in [-2.3, 2.3] {
                    line(d, vec![[x + dx - 0.6, y - 2.5], [x + dx + 0.6, y + 2.5]]);
                }
            }
        }
    }
}
pub(super) fn append(
    features: &[DrawingDatumFeature],
    frames: &[DrawingFeatureControlFrame],
    views: &HashMap<&str, &DrawingView>,
    graph: &InstanceGraph<'_>,
    d: &mut GeneratedDrawing,
) -> Result<(), ModelError> {
    let index: HashMap<_, _> = features.iter().map(|f| (f.id.as_str(), f)).collect();
    for f in features {
        let (point, p) = anchor(&f.attachment, views, graph)?;
        let width = 4.0 + 3.0 * f.label.len() as f64;
        rectangle(d, p, width);
        text(d, [p[0] + 2.0, p[1] + 2.5], f.label.clone());
        leader(d, point, [p[0] + width / 2.0, p[1]], true);
    }
    for f in frames {
        append_frame(f, &index, views, graph, d)?;
    }
    Ok(())
}
fn append_frame(
    f: &DrawingFeatureControlFrame,
    index: &HashMap<&str, &DrawingDatumFeature>,
    views: &HashMap<&str, &DrawingView>,
    graph: &InstanceGraph<'_>,
    d: &mut GeneratedDrawing,
) -> Result<(), ModelError> {
    let (point, p) = anchor(&f.attachment, views, graph)?;
    let value = number(f)?;
    let diameter = f.zone == GeometricToleranceZone::Diameter;
    let material = f.material != ToleranceMaterialCondition::Regardless;
    let value_width = 4.0
        + value.len() as f64 * 2.0
        + if diameter { 6.0 } else { 0.0 }
        + if material { 6.0 } else { 0.0 };
    let mut cells = vec![8.0, value_width];
    cells.extend(f.datums.iter().map(|r| {
        4.0 + index[r.datum_feature.as_str()].label.len() as f64 * 3.0
            + if r.boundary != DatumMaterialBoundary::Regardless {
                6.0
            } else {
                0.0
            }
    }));
    rectangle(d, p, cells.iter().sum());
    leader(d, point, p, false);
    glyph(d, [p[0] + 4.0, p[1] + 4.0], f.characteristic);
    let mut x = p[0] + 8.0;
    let mut tx = x + 2.0;
    if diameter {
        circle(d, [tx + 2.0, p[1] + 4.0], 1.8, 0.0, std::f64::consts::TAU);
        line(d, vec![[tx, p[1] + 1.5], [tx + 4.0, p[1] + 6.5]]);
        tx += 6.0;
    }
    text(d, [tx, p[1] + 2.5], value.clone());
    if material {
        modifier(
            d,
            [tx + value.len() as f64 * 2.0 + 3.0, p[1] + 4.0],
            f.material == ToleranceMaterialCondition::Maximum,
        );
    }
    for (i, width) in cells.iter().enumerate().skip(1) {
        line(d, vec![[x, p[1]], [x, p[1] + 8.0]]);
        x += width;
        if let Some(r) = f.datums.get(i - 1) {
            let label = &index[r.datum_feature.as_str()].label;
            text(d, [x + 2.0, p[1] + 2.5], label.clone());
            if r.boundary != DatumMaterialBoundary::Regardless {
                modifier(
                    d,
                    [x + label.len() as f64 * 3.0 + 5.0, p[1] + 4.0],
                    r.boundary == DatumMaterialBoundary::Maximum,
                );
            }
        }
    }
    Ok(())
}

/// O(annotations + datum references), reserves the exact generated line vertices.
pub(crate) fn vertex_count(
    features: &[DrawingDatumFeature],
    frames: &[DrawingFeatureControlFrame],
) -> Result<usize, ModelError> {
    let base = features
        .len()
        .checked_mul(11)
        .ok_or_else(|| ModelError::new("GD&T vertex count overflow"))?;
    frames.iter().try_fold(base, |sum, f| {
        let glyph = match f.characteristic {
            GeometricCharacteristic::Straightness => 2,
            GeometricCharacteristic::Flatness => 5,
            GeometricCharacteristic::Circularity | GeometricCharacteristic::ProfileLine => 33,
            GeometricCharacteristic::Cylindricity | GeometricCharacteristic::Position => 37,
            GeometricCharacteristic::ProfileSurface => 35,
            GeometricCharacteristic::Parallelism | GeometricCharacteristic::Perpendicularity => 4,
            GeometricCharacteristic::Angularity => 3,
            GeometricCharacteristic::CircularRunout => 5,
            GeometricCharacteristic::TotalRunout => 12,
        };
        let count = 10
            + 2 * (1 + f.datums.len())
            + glyph
            + if f.zone == GeometricToleranceZone::Diameter {
                35
            } else {
                0
            }
            + if f.material != ToleranceMaterialCondition::Regardless {
                33
            } else {
                0
            }
            + 33 * f
                .datums
                .iter()
                .filter(|r| r.boundary != DatumMaterialBoundary::Regardless)
                .count();
        sum.checked_add(count)
            .ok_or_else(|| ModelError::new("GD&T vertex count overflow"))
    })
}
