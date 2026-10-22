//! DXF import: 2D outlines from an ASCII DXF file into an ordinary sketch.
//!
//! Reads LINE, ARC, CIRCLE, ELLIPSE, LWPOLYLINE and POLYLINE (with bulges),
//! SPLINE (clamped control-point and fit-point forms) and INSERT of blocks
//! (uniform scale and rotation). Coordinates are converted to millimeters
//! from `$INSUNITS` or an explicit unit. Endpoints closer than the tolerance
//! are merged into one sketch point, closed loops are found and nested
//! into the outer boundary, its holes and further regions, and everything
//! the importer could not use is counted in the report. The result is a
//! plain [`SketchDefinition`] in the XY plane: no constraints and no link
//! back to the file, so the document stays self-contained.
//!
//! Time is O(file + curves + loops² · polygon) with bounding-box rejection
//! for nesting; storage is O(file).
use crate::{ModelError, SketchDefinition};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, HashMap};
use std::f64::consts::{PI, TAU};

/// Largest DXF text accepted, in bytes.
pub const MAX_DXF_BYTES: usize = 64 << 20;
/// Most curves one import may produce, after expanding blocks.
pub const MAX_DXF_CURVES: usize = 10_000;
/// Deepest nesting of block inserts.
const MAX_INSERT_DEPTH: usize = 8;
/// Most entities visited while expanding block inserts.
const MAX_VISITS: usize = 1_000_000;
/// Most holes per region and regions per sketch (the sketch schema's limits).
const MAX_LOOPS: usize = 1000;

/// Length unit of the DXF coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DxfUnit {
    Millimeter,
    Centimeter,
    Meter,
    Inch,
    Foot,
}

impl DxfUnit {
    fn millimeters(self) -> f64 {
        match self {
            Self::Millimeter => 1.0,
            Self::Centimeter => 10.0,
            Self::Meter => 1000.0,
            Self::Inch => 25.4,
            Self::Foot => 304.8,
        }
    }
    /// The `$INSUNITS` code's unit, when it names one of these.
    fn from_insunits(code: i64) -> Option<Self> {
        match code {
            1 => Some(Self::Inch),
            2 => Some(Self::Foot),
            4 => Some(Self::Millimeter),
            5 => Some(Self::Centimeter),
            6 => Some(Self::Meter),
            _ => None,
        }
    }
}

/// How to read the file.
#[derive(Clone, Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DxfImportOptions {
    /// Sketch id; default `dxf`.
    #[serde(default = "default_id")]
    pub id: String,
    /// Only entities on these layers; empty imports every layer.
    #[serde(default)]
    pub layers: Vec<String>,
    /// Coordinate unit; default from the file's `$INSUNITS`, else millimeters.
    #[serde(default)]
    pub unit: Option<DxfUnit>,
    /// Endpoints closer than this, in millimeters, become one point; in
    /// (0, 1], default 0.001.
    #[serde(default = "default_tolerance")]
    pub tolerance_mm: f64,
}

fn default_id() -> String {
    "dxf".into()
}
fn default_tolerance() -> f64 {
    1e-3
}

impl Default for DxfImportOptions {
    fn default() -> Self {
        Self {
            id: default_id(),
            layers: Vec::new(),
            unit: None,
            tolerance_mm: default_tolerance(),
        }
    }
}

/// What the import found and did.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct DxfImportReport {
    /// Unit the coordinates were read in, and where it came from.
    pub unit: String,
    pub unit_source: String,
    /// Imported curves by DXF entity type.
    pub imported: BTreeMap<String, usize>,
    /// Entities not imported, by `TYPE: reason`.
    pub skipped: BTreeMap<String, usize>,
    /// Layers that held imported curves.
    pub layers: Vec<String>,
    /// Sketch curves made: lines, arcs, circles, ellipses and splines.
    pub curves: usize,
    pub points: usize,
    /// Endpoints merged into an existing point.
    pub merged_endpoints: usize,
    /// Closed loops, and how they were used.
    pub loops: usize,
    pub profile_curves: usize,
    pub holes: usize,
    pub regions: usize,
    /// Curves in no closed loop (open or branching chains); they stay in
    /// the sketch as construction geometry.
    pub open_curves: usize,
    /// Entities whose nonzero Z was dropped.
    pub flattened: usize,
    pub bounds_mm: Option<[[f64; 2]; 2]>,
}

/// The imported sketch and its report.
#[derive(Clone, Debug)]
pub struct DxfImport {
    pub sketch: SketchDefinition,
    pub report: DxfImportReport,
}

type P = [f64; 2];

/// A curve in millimeters in the XY plane.
#[derive(Clone, Debug)]
enum Curve {
    Line(P, P),
    /// Counterclockwise from `start` to `end` radians.
    Arc {
        center: P,
        radius: f64,
        start: f64,
        end: f64,
    },
    Circle {
        center: P,
        radius: f64,
    },
    /// `center + major cos t + ratio · perp(major) sin t`, t from `start`
    /// to `end` (end > start; a full turn is closed).
    Ellipse {
        center: P,
        major: P,
        ratio: f64,
        start: f64,
        end: f64,
    },
    /// Clamped B-spline: control poles, weights (empty for all ones).
    Spline {
        degree: i32,
        knots: Vec<f64>,
        poles: Vec<P>,
        weights: Vec<f64>,
    },
    /// Interpolates its fit points.
    Fit(Vec<P>),
}

/// One DXF entity: its type, layer and group codes in file order.
#[derive(Clone, Debug)]
struct Entity {
    kind: String,
    codes: Vec<(i32, String)>,
}

impl Entity {
    fn value(&self, code: i32) -> Option<&str> {
        self.codes
            .iter()
            .find(|(c, _)| *c == code)
            .map(|(_, v)| v.as_str())
    }
    fn number(&self, code: i32) -> Option<f64> {
        self.value(code)?.parse().ok()
    }
    fn number_or(&self, code: i32, default: f64) -> f64 {
        self.number(code).unwrap_or(default)
    }
    fn layer(&self) -> &str {
        self.value(8).unwrap_or("0")
    }
    fn numbers(&self, code: i32) -> Vec<f64> {
        self.codes
            .iter()
            .filter(|(c, _)| *c == code)
            .filter_map(|(_, v)| v.parse().ok())
            .collect()
    }
}

/// A placement from block or object coordinates to the drawing's.
#[derive(Clone, Copy, Debug)]
struct Placement {
    origin: P,
    /// Uniform scale times the rotation's cosine and sine.
    cos: f64,
    sin: f64,
    scale: f64,
}

impl Placement {
    const IDENTITY: Self = Self {
        origin: [0.0, 0.0],
        cos: 1.0,
        sin: 0.0,
        scale: 1.0,
    };
    fn apply(&self, p: P) -> P {
        [
            self.origin[0] + self.scale * (self.cos * p[0] - self.sin * p[1]),
            self.origin[1] + self.scale * (self.sin * p[0] + self.cos * p[1]),
        ]
    }
    fn vector(&self, v: P) -> P {
        [
            self.scale * (self.cos * v[0] - self.sin * v[1]),
            self.scale * (self.sin * v[0] + self.cos * v[1]),
        ]
    }
    fn angle(&self) -> f64 {
        self.sin.atan2(self.cos)
    }
    /// `self` after `inner` (inner coordinates to outer).
    fn then(&self, inner: &Self) -> Self {
        Self {
            origin: self.apply(inner.origin),
            cos: self.cos * inner.cos - self.sin * inner.sin,
            sin: self.sin * inner.cos + self.cos * inner.sin,
            scale: self.scale * inner.scale,
        }
    }
}

/// Group-code pairs of the file, rejecting binary DXF.
fn pairs(text: &str) -> Result<Vec<(i32, String)>, ModelError> {
    if text.starts_with("AutoCAD Binary DXF") {
        return Err(ModelError::new(
            "binary DXF is not supported; save the file as ASCII DXF",
        ));
    }
    let mut lines = text.lines();
    let mut pairs = Vec::new();
    while let Some(code) = lines.next() {
        let code = code.trim();
        if code.is_empty() {
            continue;
        }
        let code: i32 = code.parse().map_err(|_| {
            ModelError::new(format!(
                "DXF group code '{code}' is not a number (pair {})",
                pairs.len() + 1
            ))
        })?;
        let value = lines
            .next()
            .ok_or_else(|| ModelError::new("DXF ends inside a group code pair"))?;
        pairs.push((code, value.trim().to_string()));
    }
    Ok(pairs)
}

/// The file's sections: header `$INSUNITS`, block entities by name, and the
/// drawing's entities.
struct Drawing {
    insunits: Option<i64>,
    blocks: HashMap<String, Vec<Entity>>,
    entities: Vec<Entity>,
}

fn sections(pairs: &[(i32, String)]) -> Result<Drawing, ModelError> {
    let mut drawing = Drawing {
        insunits: None,
        blocks: HashMap::new(),
        entities: Vec::new(),
    };
    let mut i = 0;
    let mut found_entities = false;
    while i < pairs.len() {
        if !(pairs[i].0 == 0 && pairs[i].1 == "SECTION") {
            i += 1;
            continue;
        }
        let name = pairs.get(i + 1).map(|(_, v)| v.as_str()).unwrap_or("");
        let end = pairs[i..]
            .iter()
            .position(|(c, v)| *c == 0 && v == "ENDSEC")
            .map(|k| i + k)
            .ok_or_else(|| ModelError::new(format!("DXF section {name} has no ENDSEC")))?;
        let body = &pairs[i + 2..end];
        match name {
            "HEADER" => {
                if let Some(at) = body.iter().position(|(c, v)| *c == 9 && v == "$INSUNITS") {
                    drawing.insunits = body.get(at + 1).and_then(|(_, v)| v.parse().ok());
                }
            }
            "BLOCKS" => {
                let entities = entities(body);
                let mut current: Option<(String, Vec<Entity>)> = None;
                for entity in entities {
                    match entity.kind.as_str() {
                        "BLOCK" => {
                            current = Some((entity.value(2).unwrap_or("").to_string(), Vec::new()))
                        }
                        "ENDBLK" => {
                            if let Some((name, list)) = current.take() {
                                drawing.blocks.insert(name, list);
                            }
                        }
                        _ => {
                            if let Some((_, list)) = current.as_mut() {
                                list.push(entity);
                            }
                        }
                    }
                }
            }
            "ENTITIES" => {
                found_entities = true;
                drawing.entities = entities(body);
            }
            _ => {}
        }
        i = end + 1;
    }
    if !found_entities {
        return Err(ModelError::new("DXF has no ENTITIES section"));
    }
    Ok(drawing)
}

/// Splits a section body at its `0` codes into entities, folding POLYLINE
/// vertices into their POLYLINE.
fn entities(body: &[(i32, String)]) -> Vec<Entity> {
    let mut list: Vec<Entity> = Vec::new();
    for (code, value) in body {
        if *code == 0 {
            list.push(Entity {
                kind: value.clone(),
                codes: Vec::new(),
            });
        } else if let Some(last) = list.last_mut() {
            last.codes.push((*code, value.clone()));
        }
    }
    // A POLYLINE's VERTEX entities follow it until SEQEND; keep their
    // coordinates and bulges on the POLYLINE as a vertex list (code 10 marks
    // each vertex, as in LWPOLYLINE).
    let mut folded = Vec::new();
    let mut open: Option<Entity> = None;
    for entity in list {
        match entity.kind.as_str() {
            "POLYLINE" => {
                let mut head = entity;
                // The POLYLINE's own 10/20/30 is a dummy elevation point.
                head.codes.retain(|(c, _)| !matches!(c, 10 | 20 | 30 | 42));
                open = Some(head);
            }
            "VERTEX" if open.is_some() => {
                let head = open.as_mut().unwrap();
                let flags = entity.number_or(70, 0.0) as i64;
                // Spline frame points (16) are not on the curve.
                if flags & 16 == 0 {
                    head.codes
                        .push((10, entity.value(10).unwrap_or("0").into()));
                    head.codes
                        .push((20, entity.value(20).unwrap_or("0").into()));
                    head.codes
                        .push((42, entity.value(42).unwrap_or("0").into()));
                }
            }
            "SEQEND" => {
                if let Some(head) = open.take() {
                    folded.push(head);
                }
            }
            _ => {
                if let Some(head) = open.take() {
                    folded.push(head);
                }
                folded.push(entity);
            }
        }
    }
    if let Some(head) = open {
        folded.push(head);
    }
    folded
}

/// Reads the drawing's curves in drawing units, expanding inserts.
struct Reader<'a> {
    drawing: &'a Drawing,
    layers: &'a [String],
    curves: Vec<(Curve, String, String)>,
    skipped: BTreeMap<String, usize>,
    flattened: usize,
    visits: usize,
}

impl<'a> Reader<'a> {
    fn skip(&mut self, kind: &str, reason: &str) {
        *self.skipped.entry(format!("{kind}: {reason}")).or_default() += 1;
    }
    fn push(&mut self, curve: Curve, entity: &Entity, layer: &str) -> Result<(), ModelError> {
        if self.curves.len() >= MAX_DXF_CURVES {
            return Err(ModelError::new(format!(
                "DXF import is limited to {MAX_DXF_CURVES} curves; filter layers or split the drawing"
            )));
        }
        self.curves
            .push((curve, entity.kind.clone(), layer.to_string()));
        Ok(())
    }
    fn read(
        &mut self,
        list: &'a [Entity],
        placement: Placement,
        depth: usize,
        inherited: Option<&str>,
    ) -> Result<(), ModelError> {
        for entity in list {
            self.visits += 1;
            if self.visits > MAX_VISITS {
                return Err(ModelError::new(format!(
                    "DXF block inserts expand to more than {MAX_VISITS} entities"
                )));
            }
            let layer = match (entity.layer(), inherited) {
                ("0", Some(outer)) => outer,
                (own, _) => own,
            };
            // An insert is followed whatever its layer; its contents are
            // filtered by theirs.
            if !self.layers.is_empty()
                && entity.kind != "INSERT"
                && !self.layers.iter().any(|l| l == layer)
            {
                continue;
            }
            self.entity(entity, placement, depth, layer)?;
        }
        Ok(())
    }
    fn entity(
        &mut self,
        e: &'a Entity,
        placement: Placement,
        depth: usize,
        layer: &str,
    ) -> Result<(), ModelError> {
        let kind = e.kind.as_str();
        // Object coordinates: only the XY plane, seen from either side.
        let normal_z = e.number_or(230, 1.0);
        if (e.number_or(210, 0.0)).abs() > 1e-9 || (e.number_or(220, 0.0)).abs() > 1e-9 {
            self.skip(kind, "not in the XY plane");
            return Ok(());
        }
        let mirrored = normal_z < 0.0;
        // Arbitrary-axis rule for a -Z extrusion: object X is world -X.
        let ocs = |p: P| if mirrored { [-p[0], p[1]] } else { p };
        let point = |x: i32| [e.number_or(x, 0.0), e.number_or(x + 10, 0.0)];
        let z_used = [30, 31, 38]
            .iter()
            .any(|c| e.numbers(*c).iter().any(|z| z.abs() > 1e-9));
        if z_used
            && matches!(
                kind,
                "LINE" | "CIRCLE" | "ARC" | "ELLIPSE" | "SPLINE" | "LWPOLYLINE" | "POLYLINE"
            )
        {
            self.flattened += 1;
        }
        match kind {
            "LINE" => {
                let (a, b) = (placement.apply(point(10)), placement.apply(point(11)));
                self.push(Curve::Line(a, b), e, layer)?;
            }
            "CIRCLE" => {
                let radius = e.number_or(40, 0.0) * placement.scale;
                if radius.is_nan() || radius <= 0.0 {
                    self.skip(kind, "zero radius");
                    return Ok(());
                }
                let center = placement.apply(ocs(point(10)));
                self.push(Curve::Circle { center, radius }, e, layer)?;
            }
            "ARC" => {
                let radius = e.number_or(40, 0.0) * placement.scale;
                if radius.is_nan() || radius <= 0.0 {
                    self.skip(kind, "zero radius");
                    return Ok(());
                }
                let (mut start, mut end) = (
                    e.number_or(50, 0.0).to_radians(),
                    e.number_or(51, 0.0).to_radians(),
                );
                if mirrored {
                    (start, end) = (PI - end, PI - start);
                }
                let turn = placement.angle();
                let center = placement.apply(ocs(point(10)));
                let (start, mut end) = (start + turn, end + turn);
                while end <= start {
                    end += TAU;
                }
                self.push(
                    Curve::Arc {
                        center,
                        radius,
                        start,
                        end,
                    },
                    e,
                    layer,
                )?;
            }
            "ELLIPSE" => {
                let ratio = e.number_or(40, 1.0);
                let axis = point(11);
                let (mut start, mut end) = (e.number_or(41, 0.0), e.number_or(42, TAU));
                if !(ratio > 0.0 && ratio <= 1.0 + 1e-12) || axis[0].hypot(axis[1]) == 0.0 {
                    self.skip(kind, "invalid axes");
                    return Ok(());
                }
                // ELLIPSE is in world coordinates; under a -Z normal its
                // minor axis is reversed, so the parameter runs backwards.
                if mirrored {
                    (start, end) = (-end, -start);
                }
                while end <= start {
                    end += TAU;
                }
                let center = placement.apply(point(10));
                self.push(
                    Curve::Ellipse {
                        center,
                        major: placement.vector(axis),
                        ratio,
                        start,
                        end,
                    },
                    e,
                    layer,
                )?;
            }
            "LWPOLYLINE" | "POLYLINE" => {
                let flags = e.number_or(70, 0.0) as i64;
                if kind == "POLYLINE" && flags & (8 | 16 | 64) != 0 {
                    self.skip(kind, "3D polyline or mesh");
                    return Ok(());
                }
                let xs = e.numbers(10);
                let ys = e.numbers(20);
                if xs.len() != ys.len() || xs.len() < 2 {
                    self.skip(kind, "fewer than two vertices");
                    return Ok(());
                }
                // Bulges follow their vertex; a vertex without one is straight.
                let mut bulges = vec![0.0f64; xs.len()];
                let mut vertex = None::<usize>;
                for (code, value) in &e.codes {
                    match code {
                        10 => vertex = Some(vertex.map_or(0, |v| v + 1)),
                        42 => {
                            if let Some(v) = vertex {
                                bulges[v] = value.parse().unwrap_or(0.0);
                            }
                        }
                        _ => {}
                    }
                }
                let closed = flags & 1 != 0;
                let count = xs.len();
                let segments = if closed { count } else { count - 1 };
                for k in 0..segments {
                    let (a, b) = ([xs[k], ys[k]], [xs[(k + 1) % count], ys[(k + 1) % count]]);
                    let bulge = if mirrored { -bulges[k] } else { bulges[k] };
                    let (a, b) = (ocs(a), ocs(b));
                    if (b[0] - a[0]).hypot(b[1] - a[1]) == 0.0 {
                        continue;
                    }
                    let curve = if bulge.abs() < 1e-12 {
                        Curve::Line(placement.apply(a), placement.apply(b))
                    } else {
                        bulge_arc(a, b, bulge, &placement)
                    };
                    self.push(curve, e, layer)?;
                }
            }
            "SPLINE" => {
                let degree = e.number_or(71, 3.0) as i32;
                let knots = e.numbers(40);
                let weights = e.numbers(41);
                let poles: Vec<P> = e
                    .numbers(10)
                    .into_iter()
                    .zip(e.numbers(20))
                    .map(|(x, y)| placement.apply([x, y]))
                    .collect();
                let fits: Vec<P> = e
                    .numbers(11)
                    .into_iter()
                    .zip(e.numbers(21))
                    .map(|(x, y)| placement.apply([x, y]))
                    .collect();
                if !poles.is_empty() {
                    let d = degree as usize;
                    if !(1..=25).contains(&degree) || knots.len() != poles.len() + d + 1 {
                        self.skip(kind, "inconsistent degree, knots and control points");
                        return Ok(());
                    }
                    let clamped = knots[..=d].iter().all(|k| (k - knots[0]).abs() < 1e-12)
                        && knots[knots.len() - d - 1..]
                            .iter()
                            .all(|k| (k - knots[knots.len() - 1]).abs() < 1e-12);
                    if !clamped {
                        self.skip(kind, "periodic or unclamped knots");
                        return Ok(());
                    }
                    if !weights.is_empty() && weights.len() != poles.len() {
                        self.skip(kind, "weights do not match control points");
                        return Ok(());
                    }
                    let weights = if weights.iter().all(|w| (w - 1.0).abs() < 1e-12) {
                        Vec::new()
                    } else {
                        weights
                    };
                    self.push(
                        Curve::Spline {
                            degree,
                            knots,
                            poles,
                            weights,
                        },
                        e,
                        layer,
                    )?;
                } else if fits.len() >= 2 {
                    self.push(Curve::Fit(fits), e, layer)?;
                } else {
                    self.skip(kind, "no control or fit points");
                }
            }
            "INSERT" => {
                if depth >= MAX_INSERT_DEPTH {
                    self.skip(kind, "blocks nested too deeply");
                    return Ok(());
                }
                let name = e.value(2).unwrap_or("");
                let drawing = self.drawing;
                let Some(block) = drawing.blocks.get(name) else {
                    self.skip(kind, "unknown block");
                    return Ok(());
                };
                let (sx, sy) = (e.number_or(41, 1.0), e.number_or(42, 1.0));
                if (sx - sy).abs() > 1e-9 * sx.abs().max(1.0) || sx <= 0.0 {
                    self.skip(kind, "non-uniform or mirrored scale");
                    return Ok(());
                }
                if mirrored {
                    self.skip(kind, "drawn with a -Z normal");
                    return Ok(());
                }
                if e.number_or(70, 1.0) > 1.0 || e.number_or(71, 1.0) > 1.0 {
                    self.skip(kind, "arrayed insert (MINSERT)");
                    return Ok(());
                }
                let angle = e.number_or(50, 0.0).to_radians();
                let inner = Placement {
                    origin: ocs(point(10)),
                    cos: angle.cos(),
                    sin: angle.sin(),
                    scale: sx,
                };
                let next = placement.then(&inner);
                // Block entities on layer 0 take the insert's layer.
                let layer = layer.to_string();
                self.read(block, next, depth + 1, Some(&layer))?;
            }
            "POINT" | "TEXT" | "MTEXT" | "DIMENSION" | "HATCH" | "SOLID" | "LEADER" | "MLEADER"
            | "VIEWPORT" | "ATTDEF" | "ATTRIB" | "IMAGE" | "WIPEOUT" => {
                self.skip(kind, "not geometry for a sketch");
            }
            _ => self.skip(kind, "unsupported entity"),
        }
        Ok(())
    }
}

/// The arc a polyline bulge (tan of a quarter of the included angle,
/// positive counterclockwise) makes from `a` to `b`, placed.
fn bulge_arc(a: P, b: P, bulge: f64, placement: &Placement) -> Curve {
    let included = 4.0 * bulge.atan();
    let chord = (b[0] - a[0]).hypot(b[1] - a[1]);
    let radius = chord / (2.0 * (included / 2.0).sin().abs());
    // The center lies on the chord's bisector, toward the arc's left for a
    // counterclockwise bulge.
    let mid = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
    let along = [(b[0] - a[0]) / chord, (b[1] - a[1]) / chord];
    let left = [-along[1], along[0]];
    let offset = radius * (included / 2.0).cos() * included.signum();
    let center = [mid[0] + left[0] * offset, mid[1] + left[1] * offset];
    let angle = |p: P| (p[1] - center[1]).atan2(p[0] - center[0]);
    let (from, to) = if bulge > 0.0 { (a, b) } else { (b, a) };
    let (start, mut end) = (angle(from), angle(to));
    while end <= start {
        end += TAU;
    }
    let turn = placement.angle();
    Curve::Arc {
        center: placement.apply(center),
        radius: radius * placement.scale,
        start: start + turn,
        end: end + turn,
    }
}

fn at_angle(center: P, radius: f64, angle: f64) -> P {
    [
        center[0] + radius * angle.cos(),
        center[1] + radius * angle.sin(),
    ]
}

fn ellipse_point(center: P, major: P, ratio: f64, t: f64) -> P {
    let minor = [-major[1] * ratio, major[0] * ratio];
    [
        center[0] + major[0] * t.cos() + minor[0] * t.sin(),
        center[1] + major[1] * t.cos() + minor[1] * t.sin(),
    ]
}

/// The rational B-spline's point at `u` by de Boor's algorithm.
fn spline_point(degree: usize, knots: &[f64], poles: &[P], weights: &[f64], u: f64) -> P {
    let n = poles.len();
    let mut span = degree;
    while span + 1 < n && knots[span + 1] <= u {
        span += 1;
    }
    let w = |i: usize| if weights.is_empty() { 1.0 } else { weights[i] };
    let mut d: Vec<[f64; 3]> = (0..=degree)
        .map(|j| {
            let i = span - degree + j;
            [poles[i][0] * w(i), poles[i][1] * w(i), w(i)]
        })
        .collect();
    for r in 1..=degree {
        for j in (r..=degree).rev() {
            let i = span - degree + j;
            let denominator = knots[i + degree + 1 - r] - knots[i];
            let alpha = if denominator == 0.0 {
                0.0
            } else {
                (u - knots[i]) / denominator
            };
            let previous = d[j - 1];
            for (value, before) in d[j].iter_mut().zip(previous) {
                *value = (1.0 - alpha) * before + alpha * *value;
            }
        }
    }
    let p = d[degree];
    [p[0] / p[2], p[1] / p[2]]
}

impl Curve {
    fn ends(&self) -> Option<(P, P)> {
        match self {
            Self::Line(a, b) => Some((*a, *b)),
            Self::Arc {
                center,
                radius,
                start,
                end,
            } => Some((
                at_angle(*center, *radius, *start),
                at_angle(*center, *radius, *end),
            )),
            Self::Circle { .. } => None,
            Self::Ellipse {
                center,
                major,
                ratio,
                start,
                end,
            } => ((end - start - TAU).abs() > 1e-9).then(|| {
                (
                    ellipse_point(*center, *major, *ratio, *start),
                    ellipse_point(*center, *major, *ratio, *end),
                )
            }),
            Self::Spline { poles, .. } => Some((poles[0], poles[poles.len() - 1])),
            Self::Fit(points) => Some((points[0], points[points.len() - 1])),
        }
    }
    /// Points along the curve from its start, for nesting and bounds.
    fn samples(&self) -> Vec<P> {
        const STEPS: usize = 32;
        match self {
            Self::Line(a, b) => vec![*a, *b],
            Self::Arc {
                center,
                radius,
                start,
                end,
            } => (0..=STEPS)
                .map(|k| {
                    at_angle(
                        *center,
                        *radius,
                        start + (end - start) * k as f64 / STEPS as f64,
                    )
                })
                .collect(),
            Self::Circle { center, radius } => (0..STEPS)
                .map(|k| at_angle(*center, *radius, TAU * k as f64 / STEPS as f64))
                .collect(),
            Self::Ellipse {
                center,
                major,
                ratio,
                start,
                end,
            } => (0..=STEPS)
                .map(|k| {
                    ellipse_point(
                        *center,
                        *major,
                        *ratio,
                        start + (end - start) * k as f64 / STEPS as f64,
                    )
                })
                .collect(),
            Self::Spline {
                degree,
                knots,
                poles,
                weights,
            } => {
                let d = *degree as usize;
                let (lo, hi) = (knots[d], knots[knots.len() - d - 1]);
                let steps = STEPS.max(poles.len() * 4);
                (0..=steps)
                    .map(|k| {
                        spline_point(
                            d,
                            knots,
                            poles,
                            weights,
                            lo + (hi - lo) * k as f64 / steps as f64,
                        )
                    })
                    .collect()
            }
            Self::Fit(points) => points.clone(),
        }
    }
}

/// Endpoint merging on a grid of tolerance-sized cells: O(1) per point.
struct Points {
    tolerance: f64,
    grid: HashMap<(i64, i64), Vec<usize>>,
    at: Vec<P>,
    merged: usize,
}

impl Points {
    fn cell(&self, p: P) -> (i64, i64) {
        (
            (p[0] / self.tolerance).floor() as i64,
            (p[1] / self.tolerance).floor() as i64,
        )
    }
    /// The index of an existing point within the tolerance, or a new one.
    fn endpoint(&mut self, p: P) -> usize {
        let (cx, cy) = self.cell(p);
        for dx in -1..=1 {
            for dy in -1..=1 {
                if let Some(list) = self.grid.get(&(cx + dx, cy + dy)) {
                    for &i in list {
                        let q = self.at[i];
                        if (q[0] - p[0]).hypot(q[1] - p[1]) <= self.tolerance {
                            self.merged += 1;
                            return i;
                        }
                    }
                }
            }
        }
        self.free(p)
    }
    /// A new point that later endpoints may merge with.
    fn free(&mut self, p: P) -> usize {
        let i = self.at.len();
        self.at.push(p);
        let cell = self.cell(p);
        self.grid.entry(cell).or_default().push(i);
        i
    }
    /// A point no endpoint merges with (centers, rims and poles).
    fn private(&mut self, p: P) -> usize {
        let i = self.at.len();
        self.at.push(p);
        i
    }
}

/// A closed loop: its curves in order, and a sampled polygon.
struct Loop {
    curves: Vec<usize>,
    polygon: Vec<P>,
    area: f64,
    bounds: [P; 2],
}

fn polygon_area(polygon: &[P]) -> f64 {
    let n = polygon.len();
    (0..n)
        .map(|i| {
            let (a, b) = (polygon[i], polygon[(i + 1) % n]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum::<f64>()
        / 2.0
}

fn inside(point: P, polygon: &[P]) -> bool {
    let n = polygon.len();
    let mut result = false;
    for i in 0..n {
        let (a, b) = (polygon[i], polygon[(i + n - 1) % n]);
        if (a[1] > point[1]) != (b[1] > point[1])
            && point[0] < (b[0] - a[0]) * (point[1] - a[1]) / (b[1] - a[1]) + a[0]
        {
            result = !result;
        }
    }
    result
}

/// Imports `text` as a sketch in the XY plane.
pub fn import_dxf(text: &str, options: &DxfImportOptions) -> Result<DxfImport, ModelError> {
    if text.len() > MAX_DXF_BYTES {
        return Err(ModelError::new(format!(
            "DXF files are limited to {} MiB",
            MAX_DXF_BYTES >> 20
        )));
    }
    if !(options.tolerance_mm > 0.0 && options.tolerance_mm <= 1.0) {
        return Err(ModelError::new("DXF tolerance_mm must be in (0, 1]"));
    }
    if options.id.is_empty() {
        return Err(ModelError::new("DXF sketch id must not be empty"));
    }
    let drawing = sections(&pairs(text)?)?;
    let (unit, unit_source) = match (options.unit, drawing.insunits) {
        (Some(unit), _) => (unit, "option"),
        (None, Some(code)) => match DxfUnit::from_insunits(code) {
            Some(unit) => (unit, "$INSUNITS"),
            None if code == 0 => (DxfUnit::Millimeter, "unitless file; millimeters assumed"),
            None => {
                return Err(ModelError::new(format!(
                    "DXF $INSUNITS {code} is not a supported unit; pass unit explicitly"
                )));
            }
        },
        (None, None) => (DxfUnit::Millimeter, "no $INSUNITS; millimeters assumed"),
    };
    let mut reader = Reader {
        drawing: &drawing,
        layers: &options.layers,
        curves: Vec::new(),
        skipped: BTreeMap::new(),
        flattened: 0,
        visits: 0,
    };
    let scale = DxfUnit::millimeters(unit);
    reader.read(
        &drawing.entities,
        Placement {
            scale,
            ..Placement::IDENTITY
        },
        0,
        None,
    )?;
    let mut report = DxfImportReport {
        unit: serde_json::to_value(unit)
            .unwrap()
            .as_str()
            .unwrap()
            .to_string(),
        unit_source: unit_source.into(),
        skipped: std::mem::take(&mut reader.skipped),
        flattened: reader.flattened,
        ..Default::default()
    };
    let curves = std::mem::take(&mut reader.curves);
    if curves.is_empty() {
        return Err(ModelError::new(format!(
            "DXF has no importable curves{}",
            if options.layers.is_empty() {
                ""
            } else {
                " on the selected layers"
            }
        )));
    }
    let tolerance = options.tolerance_mm;
    let mut points = Points {
        tolerance,
        grid: HashMap::new(),
        at: Vec::new(),
        merged: 0,
    };
    let mut layers = Vec::<String>::new();
    // Each kept curve: its sketch id, start/end point indices (none when
    // closed by itself) and samples along it.
    type Kept = (String, Option<(usize, usize)>, Vec<P>);
    let mut kept: Vec<Kept> = Vec::new();
    let (mut lines, mut arcs, mut circles, mut ellipses, mut splines) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let name = |i: usize| format!("p{}", i + 1);
    for (curve, kind, layer) in &curves {
        // Degenerate pieces after unit scaling and merging are dropped.
        if let Some((a, b)) = curve.ends()
            && matches!(curve, Curve::Line(..))
            && (b[0] - a[0]).hypot(b[1] - a[1]) <= tolerance
        {
            *report
                .skipped
                .entry(format!("{kind}: shorter than the tolerance"))
                .or_default() += 1;
            continue;
        }
        let ends = curve
            .ends()
            .map(|(a, b)| (points.endpoint(a), points.endpoint(b)));
        let id = match curve {
            Curve::Line(..) => {
                let (a, b) = ends.unwrap();
                if a == b {
                    *report
                        .skipped
                        .entry(format!("{kind}: shorter than the tolerance"))
                        .or_default() += 1;
                    continue;
                }
                let id = format!("l{}", lines.len() + 1);
                lines.push(json!({"id": id, "start": name(a), "end": name(b)}));
                id
            }
            Curve::Arc { center, .. } => {
                let (a, b) = ends.unwrap();
                let c = points.private(*center);
                let id = format!("a{}", arcs.len() + 1);
                arcs.push(json!({"id": id, "center": name(c), "start": name(a), "end": name(b)}));
                id
            }
            Curve::Circle { center, radius } => {
                let c = points.private(*center);
                let r = points.private([center[0] + radius, center[1]]);
                let id = format!("c{}", circles.len() + 1);
                circles.push(json!({"id": id, "center": name(c), "rim": name(r)}));
                id
            }
            Curve::Ellipse {
                center,
                major,
                ratio,
                ..
            } => {
                let c = points.private(*center);
                let m = points.private([center[0] + major[0], center[1] + major[1]]);
                let n =
                    points.private([center[0] - major[1] * ratio, center[1] + major[0] * ratio]);
                let id = format!("e{}", ellipses.len() + 1);
                let mut value =
                    json!({"id": id, "center": name(c), "major": name(m), "minor": name(n)});
                if let Some((a, b)) = ends {
                    value["start"] = json!(name(a));
                    value["end"] = json!(name(b));
                }
                ellipses.push(value);
                id
            }
            Curve::Spline {
                degree,
                knots,
                poles,
                weights,
            } => {
                let (a, b) = ends.unwrap();
                let mut names = vec![name(a)];
                for pole in &poles[1..poles.len() - 1] {
                    names.push(name(points.private(*pole)));
                }
                names.push(name(b));
                let mut unique: Vec<f64> = Vec::new();
                let mut multiplicities: Vec<i32> = Vec::new();
                for &k in knots {
                    if unique.last().is_some_and(|u| (k - u).abs() < 1e-12) {
                        *multiplicities.last_mut().unwrap() += 1;
                    } else {
                        unique.push(k);
                        multiplicities.push(1);
                    }
                }
                let mut basis =
                    json!({"degree": degree, "knots": unique, "multiplicities": multiplicities});
                if !weights.is_empty() {
                    basis["weights"] = json!(weights
                        .iter()
                        .map(|w| json!({"literal": {"value": w, "dimension": "scalar", "unit": null}}))
                        .collect::<Vec<_>>());
                }
                let id = format!("s{}", splines.len() + 1);
                splines.push(json!({"id": id, "points": names, "basis": basis}));
                id
            }
            Curve::Fit(fits) => {
                let (a, b) = ends.unwrap();
                let mut names = vec![name(a)];
                for fit in &fits[1..fits.len() - 1] {
                    names.push(name(points.private(*fit)));
                }
                names.push(name(b));
                let id = format!("s{}", splines.len() + 1);
                splines.push(json!({"id": id, "points": names}));
                id
            }
        };
        *report.imported.entry(kind.clone()).or_default() += 1;
        if !layers.contains(layer) {
            layers.push(layer.clone());
        }
        kept.push((id, ends, curve.samples()));
    }
    report.layers = layers;
    report.points = points.at.len();
    report.merged_endpoints = points.merged;
    report.curves = kept.len();
    let all: Vec<P> = kept
        .iter()
        .flat_map(|(_, _, s)| s.iter().copied())
        .collect();
    if let Some(first) = all.first() {
        let mut b = [*first, *first];
        for p in &all {
            b[0] = [b[0][0].min(p[0]), b[0][1].min(p[1])];
            b[1] = [b[1][0].max(p[0]), b[1][1].max(p[1])];
        }
        report.bounds_mm = Some(b);
    }

    // Loops: closed curves alone, and cycles of curves meeting two at each
    // endpoint. A branching or open chain is left out.
    let mut at_point: HashMap<usize, Vec<usize>> = HashMap::new();
    for (k, (_, ends, _)) in kept.iter().enumerate() {
        if let Some((a, b)) = ends
            && a != b
        {
            at_point.entry(*a).or_default().push(k);
            at_point.entry(*b).or_default().push(k);
        }
    }
    let mut loops: Vec<Loop> = Vec::new();
    let mut used = vec![false; kept.len()];
    let mut on_walk = vec![false; kept.len()];
    let mut reverse: Vec<usize> = Vec::new();
    let make_loop = |curves: Vec<usize>, polygon: Vec<P>| {
        let mut b = [polygon[0], polygon[0]];
        for p in &polygon {
            b[0] = [b[0][0].min(p[0]), b[0][1].min(p[1])];
            b[1] = [b[1][0].max(p[0]), b[1][1].max(p[1])];
        }
        Loop {
            area: polygon_area(&polygon).abs(),
            curves,
            polygon,
            bounds: b,
        }
    };
    for (k, (_, ends, samples)) in kept.iter().enumerate() {
        let closed = match ends {
            None => true,
            Some((a, b)) => a == b,
        };
        if closed {
            used[k] = true;
            loops.push(make_loop(vec![k], samples.clone()));
        }
    }
    for start in 0..kept.len() {
        if used[start] {
            continue;
        }
        let Some((a, b)) = kept[start].1 else {
            continue;
        };
        if at_point[&a].len() != 2 || at_point[&b].len() != 2 {
            continue;
        }
        // Walk from `b` until back at `a`, every point of degree two.
        let mut order = vec![start];
        on_walk[start] = true;
        let mut polygon = kept[start].2.clone();
        let (mut current, mut at) = (start, b);
        let mut backwards = Vec::new();
        let closed = loop {
            if at == a {
                break true;
            }
            let list = &at_point[&at];
            if list.len() != 2 {
                break false;
            }
            let next = if list[0] == current { list[1] } else { list[0] };
            if used[next] || on_walk[next] {
                break false;
            }
            on_walk[next] = true;
            let (na, nb) = kept[next].1.unwrap();
            let mut samples = kept[next].2.clone();
            let far = if na == at {
                nb
            } else {
                samples.reverse();
                backwards.push(next);
                na
            };
            polygon.extend(samples.into_iter().skip(1));
            order.push(next);
            (current, at) = (next, far);
        };
        if closed {
            for &k in &order {
                used[k] = true;
            }
            reverse.extend(backwards);
            loops.push(make_loop(order, polygon));
        } else {
            for &k in &order {
                on_walk[k] = false;
            }
        }
    }
    // The profile runs each curve from the previous one's end: turn the
    // curves walked backwards around.
    for k in reverse {
        let id = &kept[k].0;
        let index: usize = id[1..].parse::<usize>().unwrap() - 1;
        let swap = |value: &mut serde_json::Value| {
            let start = value["start"].take();
            value["start"] = value["end"].take();
            value["end"] = start;
        };
        match &id[..1] {
            "l" => swap(&mut lines[index]),
            "a" | "e" => {
                let value = if id.starts_with('a') {
                    &mut arcs[index]
                } else {
                    &mut ellipses[index]
                };
                swap(value);
                let clockwise = value["clockwise"].as_bool().unwrap_or(false);
                value["clockwise"] = json!(!clockwise);
            }
            _ => {
                let value = &mut splines[index];
                let mut points: Vec<serde_json::Value> =
                    value["points"].as_array().unwrap().clone();
                points.reverse();
                value["points"] = json!(points);
                if value.get("basis").is_some() {
                    let basis = &mut value["basis"];
                    let knots: Vec<f64> = basis["knots"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|k| k.as_f64().unwrap())
                        .collect();
                    let (lo, hi) = (knots[0], knots[knots.len() - 1]);
                    basis["knots"] =
                        json!(knots.iter().rev().map(|k| lo + hi - k).collect::<Vec<_>>());
                    let mut multiplicities = basis["multiplicities"].as_array().unwrap().clone();
                    multiplicities.reverse();
                    basis["multiplicities"] = json!(multiplicities);
                    if let Some(weights) = basis.get_mut("weights").and_then(|w| w.as_array_mut()) {
                        weights.reverse();
                    }
                }
            }
        }
    }
    report.loops = loops.len();
    report.open_curves = used.iter().filter(|u| !**u).count();

    // Nesting: each loop's parent is the smallest loop containing it.
    let mut parent: Vec<Option<usize>> = vec![None; loops.len()];
    for i in 0..loops.len() {
        let probe = loops[i].polygon[0];
        let mut best: Option<usize> = None;
        for j in 0..loops.len() {
            if i == j || loops[j].area <= loops[i].area {
                continue;
            }
            let (bi, bj) = (loops[i].bounds, loops[j].bounds);
            if bi[0][0] < bj[0][0]
                || bi[0][1] < bj[0][1]
                || bi[1][0] > bj[1][0]
                || bi[1][1] > bj[1][1]
            {
                continue;
            }
            // A vertex of loop i inside loop j; shared-point touching loops
            // test their next vertex too.
            let midpoint = loops[i].polygon[loops[i].polygon.len() / 2];
            if (inside(probe, &loops[j].polygon) || inside(midpoint, &loops[j].polygon))
                && best.is_none_or(|b| loops[j].area < loops[b].area)
            {
                best = Some(j);
            }
        }
        parent[i] = best;
    }
    let depth = |mut i: usize| {
        let mut d = 0;
        while let Some(p) = parent[i] {
            d += 1;
            i = p;
        }
        d
    };
    let ids = |l: &Loop| -> Vec<String> { l.curves.iter().map(|&k| kept[k].0.clone()).collect() };
    let mut outers: Vec<usize> = (0..loops.len()).filter(|&i| depth(i) % 2 == 0).collect();
    outers.sort_by(|&a, &b| loops[b].area.total_cmp(&loops[a].area));
    let holes_of = |o: usize| -> Vec<Vec<String>> {
        (0..loops.len())
            .filter(|&i| parent[i] == Some(o))
            .map(|i| ids(&loops[i]))
            .collect()
    };
    let mut profile = Vec::new();
    let mut holes = Vec::new();
    let mut regions = Vec::new();
    for (rank, &o) in outers.iter().enumerate() {
        let inner = holes_of(o);
        if inner.len() > MAX_LOOPS {
            return Err(ModelError::new(format!(
                "a DXF boundary holds more than {MAX_LOOPS} holes"
            )));
        }
        report.holes += inner.len();
        if rank == 0 {
            profile = ids(&loops[o]);
            holes = inner;
        } else {
            regions.push(json!({"profile": ids(&loops[o]), "holes": inner}));
        }
    }
    if regions.len() > MAX_LOOPS {
        return Err(ModelError::new(format!(
            "a DXF drawing holds more than {MAX_LOOPS} separate regions"
        )));
    }
    report.profile_curves = profile.len();
    report.regions = regions.len();
    let length =
        |v: f64| json!({"literal": {"value": v, "dimension": "length", "unit": "millimeter"}});
    let point_values: Vec<_> = points
        .at
        .iter()
        .enumerate()
        .map(|(i, p)| json!({"id": name(i), "x": length(p[0]), "y": length(p[1])}))
        .collect();
    let scalar = |v: f64| json!({"value": v, "dimension": "scalar", "unit": null});
    let mm = |v: f64| json!({"value": v, "dimension": "length", "unit": "millimeter"});
    let sketch = json!({
        "id": options.id,
        "origin": {"literal": {"x": mm(0.0), "y": mm(0.0), "z": mm(0.0)}},
        "x_axis": {"literal": {"x": scalar(1.0), "y": scalar(0.0), "z": scalar(0.0)}},
        "y_axis": {"literal": {"x": scalar(0.0), "y": scalar(1.0), "z": scalar(0.0)}},
        "points": point_values,
        "lines": lines,
        "arcs": arcs,
        "circles": circles,
        "ellipses": ellipses,
        "splines": splines,
        "profile": profile,
        "holes": holes,
        "regions": regions,
        "constraints": [],
    });
    let sketch: SketchDefinition = serde_json::from_value(sketch)
        .map_err(|e| ModelError::new(format!("imported sketch is malformed: {e}")))?;
    Ok(DxfImport { sketch, report })
}

#[cfg(test)]
#[path = "dxf/tests.rs"]
mod tests;
