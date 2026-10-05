//! Constant-width flange strips with circular bends and explicit neutral factors.
use super::*;
use crate::assembly::{add, cross, dot, scale};
use occt_bridge::WireSegment;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SheetMetalBend {
    /// Signed change of tangent direction, in dimensionless radians.
    pub angle_radians: ScalarExpr,
    pub inside_radius: ScalarExpr,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SheetMetalDefinition {
    /// Start of the thickness centerline; width extends along width_axis.
    pub origin: VectorExpr,
    pub width_axis: VectorExpr,
    pub start_direction: VectorExpr,
    pub width: ScalarExpr,
    pub thickness: ScalarExpr,
    /// Straight tangent-to-tangent lengths. One more flange than bend.
    pub flanges: Vec<ScalarExpr>,
    pub bends: Vec<SheetMetalBend>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FlatPatternMetrics {
    pub length_mm: f64,
    pub width_mm: f64,
    pub thickness_mm: f64,
    pub neutral_factor: f64,
    pub bend_allowances_mm: Vec<f64>,
    /// Centers of bend zones, measured from the start of the blank.
    pub bend_lines_mm: Vec<f64>,
}
struct EvaluatedSheet {
    origin: Vec3,
    width_axis: Vec3,
    tangent: Vec3,
    normal: Vec3,
    width: f64,
    thickness: f64,
    flanges: Vec<f64>,
    bends: Vec<(f64, f64)>,
}
fn positive(value: f64) -> Result<f64, ModelError> {
    if value.is_finite() && value > 0.0 {
        Ok(value)
    } else {
        Err(ModelError::new(
            "sheet dimensions must be finite positive lengths",
        ))
    }
}
fn direction(value: Vec3) -> Result<Vec3, ModelError> {
    let maximum = value.x.abs().max(value.y.abs()).max(value.z.abs());
    if !maximum.is_finite() || maximum == 0.0 {
        return Err(ModelError::new("sheet axes must be finite and nonzero"));
    }
    let value = Vec3::new(value.x / maximum, value.y / maximum, value.z / maximum);
    Ok(scale(value, 1.0 / dot(value, value).sqrt()))
}
impl SheetMetalDefinition {
    pub(crate) fn collect_parameters<'a>(&'a self, names: &mut HashSet<&'a str>) {
        for vector in [&self.origin, &self.width_axis, &self.start_direction] {
            crate::regeneration::collect_vector_parameters(vector, names);
        }
        for expression in [&self.width, &self.thickness]
            .into_iter()
            .chain(&self.flanges)
        {
            crate::regeneration::collect_scalar_parameters(expression, names);
        }
        for bend in &self.bends {
            crate::regeneration::collect_scalar_parameters(&bend.angle_radians, names);
            crate::regeneration::collect_scalar_parameters(&bend.inside_radius, names);
        }
    }
    fn evaluate(
        &self,
        parameters: &HashMap<String, ParameterValue>,
    ) -> Result<EvaluatedSheet, ModelError> {
        if self.flanges.is_empty()
            || self.flanges.len() > 1024
            || self.bends.len() != self.flanges.len() - 1
        {
            return Err(ModelError::new(
                "sheet requires 1–1024 flanges and exactly one bend between adjacent flanges",
            ));
        }
        let origin = vector(&self.origin, parameters, Dimension::Length)?;
        let width_axis = direction(vector(&self.width_axis, parameters, Dimension::Scalar)?)?;
        let tangent = direction(vector(
            &self.start_direction,
            parameters,
            Dimension::Scalar,
        )?)?;
        if dot(width_axis, tangent).abs() > 1e-9 {
            return Err(ModelError::new(
                "sheet width axis and start direction must be perpendicular",
            ));
        }
        let normal = direction(cross(width_axis, tangent))?;
        // Remove tolerated roundoff to make the frame exactly orthogonal.
        let tangent = direction(cross(normal, width_axis))?;
        let width = positive(scalar(&self.width, parameters, Dimension::Length)?)?;
        let thickness = positive(scalar(&self.thickness, parameters, Dimension::Length)?)?;
        let flanges = self
            .flanges
            .iter()
            .map(|value| positive(scalar(value, parameters, Dimension::Length)?))
            .collect::<Result<Vec<_>, ModelError>>()?;
        let bends = self
            .bends
            .iter()
            .map(|bend| {
                let angle = scalar(&bend.angle_radians, parameters, Dimension::Scalar)?;
                let radius = positive(scalar(&bend.inside_radius, parameters, Dimension::Length)?)?;
                if !angle.is_finite() || !(1e-6..std::f64::consts::PI - 1e-6).contains(&angle.abs())
                {
                    return Err(ModelError::new(
                        "sheet bend angle must have magnitude between 1e-6 and pi−1e-6 radians",
                    ));
                }
                positive(radius + thickness * 0.5)?;
                Ok((angle, radius))
            })
            .collect::<Result<Vec<_>, ModelError>>()?;
        let result = EvaluatedSheet {
            origin,
            width_axis,
            tangent,
            normal,
            width,
            thickness,
            flanges,
            bends,
        };
        result.metrics(0.5)?; // Reject overflowing accumulated dimensions before kernel calls.
        Ok(result)
    }
    /// Evaluates the blank without kernel handles. K is explicitly supplied:
    /// allowance = |angle| * (inside_radius + K * thickness).
    pub fn flat_pattern(
        &self,
        parameters: &HashMap<String, ParameterValue>,
        neutral_factor: f64,
    ) -> Result<FlatPatternMetrics, ModelError> {
        self.evaluate(parameters)?.metrics(neutral_factor)
    }
    pub(crate) fn generate<'a>(
        &self,
        session: &'a Session,
        parameters: &HashMap<String, ParameterValue>,
    ) -> Result<Shape<'a>, ModelError> {
        let sheet = self.evaluate(parameters)?;
        let left = sheet.boundary(sheet.thickness * 0.5);
        let right = sheet.boundary(-sheet.thickness * 0.5);
        let mut segments = left;
        let end_left = end(*segments.last().expect("nonempty flanges"));
        let end_right = end(*right.last().expect("nonempty flanges"));
        segments.push(WireSegment::Line {
            start: end_left,
            end: end_right,
        });
        segments.extend(right.iter().rev().copied().map(reverse));
        segments.push(WireSegment::Line {
            start: start(*right.first().unwrap()),
            end: start(*segments.first().unwrap()),
        });
        sheet.solid(session, &segments)
    }
    pub(crate) fn generate_flat<'a>(
        &self,
        session: &'a Session,
        parameters: &HashMap<String, ParameterValue>,
        neutral_factor: f64,
    ) -> Result<Shape<'a>, ModelError> {
        let sheet = self.evaluate(parameters)?;
        let metrics = sheet.metrics(neutral_factor)?;
        let half = sheet.thickness * 0.5;
        let points = [
            (0.0, half),
            (metrics.length_mm, half),
            (metrics.length_mm, -half),
            (0.0, -half),
        ];
        let segments: Vec<_> = (0..4)
            .map(|index| WireSegment::Line {
                start: sheet.point(points[index]),
                end: sheet.point(points[(index + 1) % 4]),
            })
            .collect();
        sheet.solid(session, &segments)
    }
}
impl EvaluatedSheet {
    fn metrics(&self, neutral_factor: f64) -> Result<FlatPatternMetrics, ModelError> {
        if !neutral_factor.is_finite() || !(0.0..=1.0).contains(&neutral_factor) {
            return Err(ModelError::new(
                "sheet neutral factor must be finite in [0, 1]",
            ));
        }
        let mut length = 0.0;
        let mut allowances = Vec::with_capacity(self.bends.len());
        let mut lines = Vec::with_capacity(self.bends.len());
        for (index, flange) in self.flanges.iter().enumerate() {
            length += flange;
            if let Some((angle, radius)) = self.bends.get(index) {
                let allowance = angle.abs() * (radius + neutral_factor * self.thickness);
                lines.push(length + allowance * 0.5);
                allowances.push(allowance);
                length += allowance;
            }
        }
        positive(length)?;
        positive(length * self.width * self.thickness)?;
        Ok(FlatPatternMetrics {
            length_mm: length,
            width_mm: self.width,
            thickness_mm: self.thickness,
            neutral_factor,
            bend_allowances_mm: allowances,
            bend_lines_mm: lines,
        })
    }
    fn point(&self, point: (f64, f64)) -> Vec3 {
        add(
            self.origin,
            add(scale(self.tangent, point.0), scale(self.normal, point.1)),
        )
    }
    fn boundary(&self, offset: f64) -> Vec<WireSegment> {
        let mut segments = Vec::with_capacity(self.flanges.len() + self.bends.len());
        let mut point = (0.0, 0.0);
        let mut angle: f64 = 0.0;
        for (index, flange) in self.flanges.iter().enumerate() {
            let next = (
                point.0 + flange * angle.cos(),
                point.1 + flange * angle.sin(),
            );
            let left = (-angle.sin(), angle.cos());
            segments.push(WireSegment::Line {
                start: self.point((point.0 + offset * left.0, point.1 + offset * left.1)),
                end: self.point((next.0 + offset * left.0, next.1 + offset * left.1)),
            });
            point = next;
            if let Some((turn, radius)) = self.bends.get(index) {
                let sign = turn.signum();
                let center_radius = radius + self.thickness * 0.5;
                let center = (
                    point.0 + sign * center_radius * left.0,
                    point.1 + sign * center_radius * left.1,
                );
                let boundary_radius = center_radius - sign * offset;
                let arc_point = |theta: f64| {
                    self.point((
                        center.0 + sign * boundary_radius * theta.sin(),
                        center.1 - sign * boundary_radius * theta.cos(),
                    ))
                };
                segments.push(WireSegment::Arc {
                    start: arc_point(angle),
                    middle: arc_point(angle + turn * 0.5),
                    end: arc_point(angle + turn),
                });
                angle += turn;
                point = (
                    center.0 + sign * center_radius * angle.sin(),
                    center.1 - sign * center_radius * angle.cos(),
                );
            }
        }
        segments
    }
    fn solid<'a>(
        &self,
        session: &'a Session,
        segments: &[WireSegment],
    ) -> Result<Shape<'a>, ModelError> {
        let wire = session.create_segment_wire(segments, true)?;
        let face = session.create_face_from_wire(&wire)?;
        if !session.is_valid(&face)? || !session.face_is_planar(&face)? {
            return Err(ModelError::new(
                "sheet outline is self-intersecting or invalid",
            ));
        }
        let solid = session.create_prism_from_face(&face, scale(self.width_axis, self.width))?;
        let volume = session.volume(&solid)?;
        if session.shape_type(&solid)? != ShapeType::Solid
            || !session.is_valid(&solid)?
            || !volume.is_finite()
            || volume <= 0.0
        {
            return Err(ModelError::new(
                "sheet outline did not produce one valid solid",
            ));
        }
        Ok(solid)
    }
}
fn start(segment: WireSegment) -> Vec3 {
    match segment {
        WireSegment::Line { start, .. } | WireSegment::Arc { start, .. } => start,
    }
}
fn end(segment: WireSegment) -> Vec3 {
    match segment {
        WireSegment::Line { end, .. } | WireSegment::Arc { end, .. } => end,
    }
}
fn reverse(segment: WireSegment) -> WireSegment {
    match segment {
        WireSegment::Line { start, end } => WireSegment::Line {
            start: end,
            end: start,
        },
        WireSegment::Arc { start, middle, end } => WireSegment::Arc {
            start: end,
            middle,
            end: start,
        },
    }
}
impl FlatPatternMetrics {
    /// Blank outline and dashed bend centerlines in a millimeter drawing.
    /// Bend lines mark the center of the allowance zone, not bend extents.
    pub fn drawing(&self, id: &str) -> Result<GeneratedDrawing, ModelError> {
        positive(self.length_mm)?;
        positive(self.width_mm)?;
        positive(self.thickness_mm)?;
        if id.is_empty()
            || id.len() > 256
            || id.chars().any(char::is_control)
            || !self.neutral_factor.is_finite()
            || !(0.0..=1.0).contains(&self.neutral_factor)
            || self.bend_allowances_mm.len() != self.bend_lines_mm.len()
            || self
                .bend_allowances_mm
                .iter()
                .any(|value| !value.is_finite() || *value <= 0.0)
            || self
                .bend_lines_mm
                .iter()
                .any(|value| !value.is_finite() || *value <= 0.0 || *value >= self.length_mm)
            || self.bend_lines_mm.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err(ModelError::new(
                "invalid flat-pattern metrics or drawing id",
            ));
        }
        let (x, y, length, width) = (10.0, 45.0, self.length_mm, self.width_mm);
        let paper_size_mm = [(length + 20.0).max(100.0), width + 65.0];
        if paper_size_mm.iter().any(|value| !value.is_finite()) {
            return Err(ModelError::new("flat-pattern paper dimensions overflow"));
        }
        let mut polylines = vec![DrawingPolyline {
            points_mm: vec![
                [x, y],
                [x + length, y],
                [x + length, y + width],
                [x, y + width],
                [x, y],
            ],
            hidden: false,
        }];
        polylines.extend(self.bend_lines_mm.iter().map(|line| DrawingPolyline {
            points_mm: vec![[x + line, y], [x + line, y + width]],
            hidden: true,
        }));
        Ok(GeneratedDrawing {
            gdt_lines: Vec::new(),
            gdt_labels: Vec::new(),
            sheet_lines: Vec::new(),
            sheet_labels: Vec::new(),
            id: id.into(),
            title: format!("{id} flat pattern"),
            paper_size_mm,
            polylines,
            guides: Vec::new(),
            hatches: Vec::new(),
            labels: vec![],
            metadata: BTreeMap::from([
                ("Thickness mm".into(), self.thickness_mm.to_string()),
                ("Neutral factor K".into(), self.neutral_factor.to_string()),
            ]),
            generated_variants: 0,
        })
    }
}
