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
        let frame = self.frame_lines();
        for line in self
            .ordered_lines()
            .chain(frame.iter())
            .chain(&self.gdt_lines)
        {
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
    /// Curves are LWPOLYLINE approximations at the chosen sample resolution.
    pub fn to_dxf(&self) -> String {
        let mut out =
            "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1021\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n"
                .to_owned();
        append_dxf_tables(&mut out);
        out.push_str("0\nSECTION\n2\nENTITIES\n");
        for line in &self.hatches {
            append_dxf_hatch(&mut out, line);
        }
        let frame = self.frame_lines();
        for (line, layer) in self
            .ordered_lines()
            .chain(frame.iter())
            .map(|line| (line, if line.hidden { "HIDDEN" } else { "VISIBLE" }))
            .chain(self.gdt_lines.iter().map(|line| (line, "GD_T")))
        {
            write!(
                out,
                "0\nLWPOLYLINE\n100\nAcDbEntity\n8\n{}\n100\nAcDbPolyline\n90\n{}\n70\n0\n",
                layer,
                line.points_mm.len()
            )
            .unwrap();
            for point in &line.points_mm {
                write!(out, "10\n{}\n20\n{}\n", point[0], point[1]).unwrap();
            }
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

    fn ordered_lines(&self) -> impl Iterator<Item = &DrawingPolyline> {
        self.polylines
            .iter()
            .filter(|line| line.hidden)
            .chain(self.polylines.iter().filter(|line| !line.hidden))
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
