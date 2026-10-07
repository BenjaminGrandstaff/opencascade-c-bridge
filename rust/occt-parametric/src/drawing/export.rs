//! SVG and ASCII DXF exports in paper millimeters.
use super::*;
use std::fmt::Write;

fn xml(text: &str) -> String {
    text.chars()
        .filter(|value| {
            matches!(value, '\t' | '\n' | '\r')
                || (*value >= '\u{20}' && *value != '\u{fffe}' && *value != '\u{ffff}')
        })
        .collect::<String>()
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn dxf_text(text: &str) -> String {
    text.chars()
        .map(|value| if value.is_control() { ' ' } else { value })
        .collect()
}

impl GeneratedDrawing {
    /// Standalone SVG, physical paper size in mm and a matching viewBox.
    pub fn to_svg(&self) -> String {
        let [width, height] = self.paper_size_mm;
        let mut out = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}mm\" height=\"{height}mm\" viewBox=\"0 0 {width} {height}\"><title>{}</title>\n<rect width=\"{width}\" height=\"{height}\" fill=\"white\"/>\n",
            xml(&self.title)
        );
        for line in &self.hatches {
            append_svg_hatch(&mut out, line, height);
        }
        for hidden in [true, false] {
            for curve in self.curves.iter().filter(|curve| curve.hidden == hidden) {
                append_svg_curve(&mut out, curve, height);
            }
            for line in self.polylines.iter().filter(|line| line.hidden == hidden) {
                append_svg_polyline(&mut out, line, height);
            }
        }
        for line in self.frame_lines().iter().chain(&self.gdt_lines) {
            append_svg_polyline(&mut out, line, height);
        }
        for line in &self.guides {
            append_svg_guide(&mut out, line, height);
        }
        for label in self
            .labels
            .iter()
            .chain(&self.sheet_labels)
            .chain(&self.gdt_labels)
        {
            writeln!(out, "<g aria-label=\"{}\">", xml(&label.text)).unwrap();
            for (point, text, size) in label_parts(label) {
                writeln!(out,"<text x=\"{}\" y=\"{}\" font-family=\"sans-serif\" font-size=\"{size}\">{}</text>",point[0],height-point[1],xml(&text)).unwrap();
            }
            out.push_str("</g>\n");
        }
        if self.sheet_lines.is_empty() {
            writeln!(
                out,
                "<text x=\"10\" y=\"{}\" font-family=\"sans-serif\" font-size=\"4\">{}</text>",
                height - 10.0,
                xml(&self.title)
            )
            .unwrap();
            for (index, (key, value)) in self.metadata.iter().enumerate() {
                writeln!(
                out,
                "<text x=\"10\" y=\"{}\" font-family=\"sans-serif\" font-size=\"3\">{}: {}</text>",
                height - 16.0 - 5.0 * index as f64,
                xml(key),
                xml(value)
            )
            .unwrap();
            }
        }
        out.push_str("</svg>\n");
        out
    }

    /// DXF R2007 with UTF-8 text, mm insertion units, and visible/hidden layers.
    /// Exact analytic entities when requested; other curves use LWPOLYLINE.
    pub fn to_dxf(&self) -> String {
        let mut out =
            "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1021\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n"
                .to_owned();
        append_dxf_tables(&mut out);
        out.push_str("0\nSECTION\n2\nENTITIES\n");
        for line in &self.hatches {
            append_dxf_hatch(&mut out, line);
        }
        for hidden in [true, false] {
            let layer = if hidden { "HIDDEN" } else { "VISIBLE" };
            for curve in self.curves.iter().filter(|curve| curve.hidden == hidden) {
                append_dxf_curve(&mut out, curve);
            }
            for line in self.polylines.iter().filter(|line| line.hidden == hidden) {
                append_dxf_polyline(&mut out, line, layer);
            }
        }
        for line in self.frame_lines() {
            append_dxf_polyline(&mut out, &line, "VISIBLE");
        }
        for line in &self.gdt_lines {
            append_dxf_polyline(&mut out, line, "GD_T");
        }
        for line in &self.guides {
            append_dxf_guide(&mut out, line);
        }
        for label in self
            .labels
            .iter()
            .chain(&self.sheet_labels)
            .chain(&self.gdt_labels)
        {
            append_dxf_label(&mut out, label);
        }
        if self.sheet_lines.is_empty() {
            append_dxf_label(
                &mut out,
                &DrawingLabel {
                    position_mm: [10.0, 10.0],
                    text: self.title.clone(),
                    stack: None,
                },
            );
            for (index, (key, value)) in self.metadata.iter().enumerate() {
                append_dxf_label(
                    &mut out,
                    &DrawingLabel {
                        position_mm: [10.0, 16.0 + 5.0 * index as f64],
                        text: format!("{key}: {value}"),
                        stack: None,
                    },
                );
            }
        }
        out.push_str("0\nENDSEC\n0\nEOF\n");
        out
    }

    fn frame_lines(&self) -> Vec<DrawingPolyline> {
        if !self.sheet_lines.is_empty() {
            return self.sheet_lines.clone();
        }
        let [width, height] = self.paper_size_mm;
        let margin = 5.0_f64.min(width * 0.05).min(height * 0.05);
        let footer = (23.0 + 5.0 * self.metadata.len() as f64).min(height - margin);
        [
            vec![
                [margin, margin],
                [width - margin, margin],
                [width - margin, height - margin],
                [margin, height - margin],
                [margin, margin],
            ],
            vec![[margin, footer], [width - margin, footer]],
            vec![
                [margin, 15.0_f64.min(footer)],
                [width - margin, 15.0_f64.min(footer)],
            ],
        ]
        .into_iter()
        .map(|points_mm| DrawingPolyline {
            points_mm,
            hidden: false,
        })
        .collect()
    }
}

fn append_dxf_tables(out: &mut String) {
    out.push_str("0\nSECTION\n2\nTABLES\n0\nTABLE\n5\n10\n330\n0\n2\nLTYPE\n100\nAcDbSymbolTable\n70\n3\n0\nLTYPE\n100\nAcDbSymbolTableRecord\n100\nAcDbLinetypeTableRecord\n2\nCONTINUOUS\n70\n0\n3\nSolid line\n72\n65\n73\n0\n40\n0\n0\nLTYPE\n100\nAcDbSymbolTableRecord\n100\nAcDbLinetypeTableRecord\n2\nHIDDEN\n70\n0\n3\nHidden edges\n72\n65\n73\n2\n40\n3\n49\n2\n74\n0\n49\n-1\n74\n0\n0\nLTYPE\n100\nAcDbSymbolTableRecord\n100\nAcDbLinetypeTableRecord\n2\nCENTER\n70\n0\n3\nLong-short center line\n72\n65\n73\n4\n40\n9\n49\n6\n74\n0\n49\n-1\n74\n0\n49\n1\n74\n0\n49\n-1\n74\n0\n0\nENDTAB\n0\nTABLE\n5\n11\n330\n0\n2\nLAYER\n100\nAcDbSymbolTable\n70\n7\n");
    for (name, line_type) in [
        ("VISIBLE", "CONTINUOUS"),
        ("HIDDEN", "HIDDEN"),
        ("ANNOTATIONS", "CONTINUOUS"),
        ("CENTER", "CENTER"),
        ("CUTTING_PLANE", "CENTER"),
        ("SECTION_HATCH", "CONTINUOUS"),
        ("GD_T", "CONTINUOUS"),
    ] {
        write!(out,"0\nLAYER\n100\nAcDbSymbolTableRecord\n100\nAcDbLayerTableRecord\n2\n{name}\n70\n0\n62\n7\n6\n{line_type}\n").unwrap();
    }
    out.push_str("0\nENDTAB\n0\nENDSEC\n");
}

fn label_parts(label: &DrawingLabel) -> Vec<([f64; 2], String, f64)> {
    let [x, y] = label.position_mm;
    let Some(stack) = &label.stack else {
        return vec![(label.position_mm, label.text.clone(), 3.0)];
    };
    let column = x + stack.prefix.chars().count() as f64 * 1.8 + 1.0;
    let suffix_x =
        column + stack.upper.chars().count().max(stack.lower.chars().count()) as f64 * 1.4 + 1.0;
    vec![
        ([x, y], stack.prefix.clone(), 3.0),
        ([column, y + 1.8], stack.upper.clone(), 2.2),
        ([column, y - 1.8], stack.lower.clone(), 2.2),
        ([suffix_x, y], stack.suffix.clone(), 3.0),
    ]
}
fn append_dxf_label(out: &mut String, label: &DrawingLabel) {
    if label.stack.is_some() {
        writeln!(out, "999\n{}", dxf_text(&label.text)).unwrap();
    }
    for (point, text, size) in label_parts(label) {
        if text.is_empty() {
            continue;
        }
        write!(out,"0\nTEXT\n100\nAcDbEntity\n8\nANNOTATIONS\n100\nAcDbText\n10\n{}\n20\n{}\n30\n0\n40\n{size}\n1\n{}\n100\nAcDbText\n",point[0],point[1],dxf_text(&text)).unwrap();
    }
}

fn guide_style(kind: DrawingGuideLineKind) -> (&'static str, &'static str, u16) {
    match kind {
        DrawingGuideLineKind::Center => ("CENTER", "CENTER", 18),
        DrawingGuideLineKind::CenterMark => ("CENTER", "CONTINUOUS", 18),
        DrawingGuideLineKind::CuttingPlane => ("CUTTING_PLANE", "CENTER", 50),
        DrawingGuideLineKind::Arrow => ("CUTTING_PLANE", "CONTINUOUS", 50),
    }
}
fn append_svg_guide(out: &mut String, line: &DrawingGuideLine, height: f64) {
    let (_, line_type, width) = guide_style(line.kind);
    write!(
        out,
        "<polyline fill=\"none\" stroke=\"black\" stroke-width=\"{}\"",
        f64::from(width) / 100.0
    )
    .unwrap();
    if line_type == "CENTER" {
        out.push_str(" stroke-dasharray=\"6 1 1 1\"");
    }
    out.push_str(" points=\"");
    for point in &line.points_mm {
        write!(out, "{},{} ", point[0], height - point[1]).unwrap();
    }
    out.push_str("\"/>\n");
}
fn append_dxf_guide(out: &mut String, line: &DrawingGuideLine) {
    let (layer, line_type, width) = guide_style(line.kind);
    write!(out,"0\nLWPOLYLINE\n100\nAcDbEntity\n8\n{layer}\n6\n{line_type}\n370\n{width}\n100\nAcDbPolyline\n90\n{}\n70\n0\n",line.points_mm.len()).unwrap();
    for point in &line.points_mm {
        write!(out, "10\n{}\n20\n{}\n", point[0], point[1]).unwrap();
    }
}

fn append_svg_hatch(out: &mut String, line: &DrawingPolyline, height: f64) {
    out.push_str("<polyline fill=\"none\" stroke=\"black\" stroke-width=\"0.13\" points=\"");
    for point in &line.points_mm {
        write!(out, "{},{} ", point[0], height - point[1]).unwrap();
    }
    out.push_str("\"/>\n");
}
fn append_dxf_hatch(out: &mut String, line: &DrawingPolyline) {
    write!(out,"0\nLWPOLYLINE\n100\nAcDbEntity\n8\nSECTION_HATCH\n370\n13\n100\nAcDbPolyline\n90\n{}\n70\n0\n",line.points_mm.len()).unwrap();
    for point in &line.points_mm {
        write!(out, "10\n{}\n20\n{}\n", point[0], point[1]).unwrap();
    }
}

fn ellipse_point(center: [f64; 2], major: [f64; 2], minor: f64, t: f64) -> [f64; 2] {
    let radius = major[0].hypot(major[1]);
    let (sin, cos) = t.sin_cos();
    [
        center[0] + major[0] * cos - major[1] / radius * minor * sin,
        center[1] + major[1] * cos + major[0] / radius * minor * sin,
    ]
}
fn append_svg_curve(out: &mut String, curve: &DrawingCurve, height: f64) {
    use DrawingCurveGeometry::*;
    out.push_str("<path fill=\"none\" stroke=\"black\" stroke-width=\"0.25\"");
    if curve.hidden {
        out.push_str(" stroke-dasharray=\"2 1\"");
    }
    match curve.geometry {
        Bezier {
            ref poles_mm,
            ref svg_points_mm,
            ..
        } => {
            let first = poles_mm[0];
            write!(out, " d=\"M {} {}", first[0], height - first[1]).unwrap();
            if !svg_points_mm.is_empty() {
                for point in svg_points_mm.iter().skip(1) {
                    write!(out, " L {} {}", point[0], height - point[1]).unwrap();
                }
            } else {
                let command = match poles_mm.len() {
                    2 => "L",
                    3 => "Q",
                    4 => "C",
                    _ => unreachable!("generated polynomial SVG span has degree <=3"),
                };
                write!(out, " {command}").unwrap();
                for point in poles_mm.iter().skip(1) {
                    write!(out, " {} {}", point[0], height - point[1]).unwrap();
                }
            }
            out.push_str("\"/>\n");
        }

        Line {
            start_mm: a,
            end_mm: b,
        } => {
            writeln!(
                out,
                " d=\"M {} {} L {} {}\"/>",
                a[0],
                height - a[1],
                b[0],
                height - b[1]
            )
            .unwrap();
        }
        Ellipse {
            center_mm,
            major_axis_mm,
            minor_radius_mm,
            start_parameter,
            end_parameter,
        } => {
            let a = major_axis_mm[0].hypot(major_axis_mm[1]);
            let rotation = -major_axis_mm[1].atan2(major_axis_mm[0]).to_degrees();
            let start = ellipse_point(center_mm, major_axis_mm, minor_radius_mm, start_parameter);
            write!(out, " d=\"M {} {}", start[0], height - start[1]).unwrap();
            // Two arcs also represent full ellipses, whose coincident endpoints
            // would otherwise make SVG's single-arc command draw nothing.
            for parameter in [(start_parameter + end_parameter) * 0.5, end_parameter] {
                let point = ellipse_point(center_mm, major_axis_mm, minor_radius_mm, parameter);
                write!(
                    out,
                    " A {a} {minor_radius_mm} {rotation} 0 0 {} {}",
                    point[0],
                    height - point[1]
                )
                .unwrap();
            }
            out.push_str("\"/>\n");
        }
    }
}
fn append_dxf_curve(out: &mut String, curve: &DrawingCurve) {
    use DrawingCurveGeometry::*;
    let layer = if curve.hidden { "HIDDEN" } else { "VISIBLE" };
    match curve.geometry {
        Bezier {
            ref poles_mm,
            ref weights,
            ..
        } => {
            let rational = weights.iter().any(|w| *w != weights[0]);
            let flags = 8 + if rational { 4 } else { 0 };
            write!(out,"0\nSPLINE\n100\nAcDbEntity\n8\n{layer}\n100\nAcDbSpline\n210\n0\n220\n0\n230\n1\n70\n{flags}\n71\n{}\n72\n{}\n73\n{}\n74\n0\n",poles_mm.len()-1,2*poles_mm.len(),poles_mm.len()).unwrap();
            for knot in [0, 1] {
                for _ in poles_mm {
                    write!(out, "40\n{knot}\n").unwrap();
                }
            }
            if rational {
                for weight in weights {
                    write!(out, "41\n{weight}\n").unwrap();
                }
            }
            for p in poles_mm {
                write!(out, "10\n{}\n20\n{}\n30\n0\n", p[0], p[1]).unwrap();
            }
        }

        Line {
            start_mm: a,
            end_mm: b,
        } => {
            write!(out,"0\nLINE\n100\nAcDbEntity\n8\n{layer}\n100\nAcDbLine\n10\n{}\n20\n{}\n30\n0\n11\n{}\n21\n{}\n31\n0\n",a[0],a[1],b[0],b[1]).unwrap();
        }
        Ellipse {
            center_mm: c,
            major_axis_mm: m,
            minor_radius_mm: b,
            start_parameter: first,
            end_parameter: last,
        } => {
            let a = m[0].hypot(m[1]);
            let full = last - first == std::f64::consts::TAU;
            if (a - b).abs() <= 64.0 * f64::EPSILON * a {
                let entity = if full { "CIRCLE" } else { "ARC" };
                write!(out,"0\n{entity}\n100\nAcDbEntity\n8\n{layer}\n100\nAcDbCircle\n10\n{}\n20\n{}\n30\n0\n40\n{a}\n",c[0],c[1]).unwrap();
                if !full {
                    let rotation = m[1].atan2(m[0]);
                    write!(
                        out,
                        "100\nAcDbArc\n50\n{}\n51\n{}\n",
                        (first + rotation).to_degrees().rem_euclid(360.0),
                        (last + rotation).to_degrees().rem_euclid(360.0)
                    )
                    .unwrap();
                }
            } else {
                let last = if full {
                    last
                } else {
                    last.rem_euclid(std::f64::consts::TAU)
                };
                write!(out,"0\nELLIPSE\n100\nAcDbEntity\n8\n{layer}\n100\nAcDbEllipse\n10\n{}\n20\n{}\n30\n0\n11\n{}\n21\n{}\n31\n0\n40\n{}\n41\n{first}\n42\n{last}\n",c[0],c[1],m[0],m[1],b/a).unwrap();
            }
        }
    }
}

fn append_svg_polyline(out: &mut String, line: &DrawingPolyline, height: f64) {
    out.push_str("<polyline fill=\"none\" stroke=\"black\" stroke-width=\"0.25\"");
    if line.hidden {
        out.push_str(" stroke-dasharray=\"2 1\"");
    }
    out.push_str(" points=\"");
    for point in &line.points_mm {
        write!(out, "{},{} ", point[0], height - point[1]).unwrap();
    }
    out.push_str("\"/>\n");
}
fn append_dxf_polyline(out: &mut String, line: &DrawingPolyline, layer: &str) {
    write!(
        out,
        "0\nLWPOLYLINE\n100\nAcDbEntity\n8\n{layer}\n100\nAcDbPolyline\n90\n{}\n70\n0\n",
        line.points_mm.len()
    )
    .unwrap();
    for point in &line.points_mm {
        write!(out, "10\n{}\n20\n{}\n", point[0], point[1]).unwrap();
    }
}

#[cfg(test)]
mod curve_tests {
    use super::*;
    #[test]
    fn spline_entity_preserves_rational_geometry_and_svg_approximation() {
        let poles_mm = vec![[2.0, 0.0], [2.0, 2.0], [0.0, 2.0]];
        let weights = vec![1.0, std::f64::consts::FRAC_1_SQRT_2, 1.0];
        let svg_points_mm = bezier::svg_points(&poles_mm, &weights, 0.002, 1000, &mut 0).unwrap();
        let curve = DrawingCurve {
            hidden: true,
            geometry: DrawingCurveGeometry::Bezier {
                poles_mm,
                weights,
                svg_points_mm,
            },
        };
        let mut dxf = String::new();
        append_dxf_curve(&mut dxf, &curve);
        let fields: Vec<_> = dxf.lines().collect();
        let values = |code: &str| {
            fields
                .as_chunks::<2>()
                .0
                .iter()
                .filter(|pair| pair[0] == code)
                .map(|pair| pair[1].parse::<f64>().unwrap())
                .collect::<Vec<_>>()
        };
        assert!(dxf.contains("8\nHIDDEN\n"));
        assert_eq!(values("70"), vec![12.0]);
        assert_eq!(values("71"), vec![2.0]);
        assert_eq!(values("72"), vec![6.0]);
        assert_eq!(values("73"), vec![3.0]);
        assert_eq!(values("40"), vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0]);
        let (x, y, w) = (values("10"), values("20"), values("41"));
        for i in 0..=100 {
            let t = i as f64 / 100.0;
            let b = [(1.0 - t).powi(2), 2.0 * t * (1.0 - t), t * t];
            let denominator = (0..3).map(|j| b[j] * w[j]).sum::<f64>();
            let px = (0..3).map(|j| b[j] * w[j] * x[j]).sum::<f64>() / denominator;
            let py = (0..3).map(|j| b[j] * w[j] * y[j]).sum::<f64>() / denominator;
            assert!((px.hypot(py) - 2.0).abs() < 1e-12);
        }
        let mut svg = String::new();
        append_svg_curve(&mut svg, &curve, 100.0);
        assert!(svg.contains("stroke-dasharray=\"2 1\""));
        assert!(svg.contains("M 2 100 L"));
        assert!(svg.ends_with("0 98\"/>\n"));
    }
}
