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
        let frame = self.frame_lines();
        for line in self.ordered_lines().chain(frame.iter()) {
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
        for label in &self.labels {
            writeln!(
                out,
                "<text x=\"{}\" y=\"{}\" font-family=\"sans-serif\" font-size=\"3\">{}</text>",
                label.position_mm[0],
                height - label.position_mm[1],
                xml(&label.text)
            )
            .unwrap();
        }
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
        let frame = self.frame_lines();
        for line in self.ordered_lines().chain(frame.iter()) {
            write!(
                out,
                "0\nLWPOLYLINE\n100\nAcDbEntity\n8\n{}\n100\nAcDbPolyline\n90\n{}\n70\n0\n",
                if line.hidden { "HIDDEN" } else { "VISIBLE" },
                line.points_mm.len()
            )
            .unwrap();
            for point in &line.points_mm {
                write!(out, "10\n{}\n20\n{}\n", point[0], point[1]).unwrap();
            }
        }
        for label in &self.labels {
            append_dxf_label(&mut out, label);
        }
        append_dxf_label(
            &mut out,
            &DrawingLabel {
                position_mm: [10.0, 10.0],
                text: self.title.clone(),
            },
        );
        for (index, (key, value)) in self.metadata.iter().enumerate() {
            append_dxf_label(
                &mut out,
                &DrawingLabel {
                    position_mm: [10.0, 16.0 + 5.0 * index as f64],
                    text: format!("{key}: {value}"),
                },
            );
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
    out.push_str("0\nSECTION\n2\nTABLES\n0\nTABLE\n5\n10\n330\n0\n2\nLTYPE\n100\nAcDbSymbolTable\n70\n2\n0\nLTYPE\n100\nAcDbSymbolTableRecord\n100\nAcDbLinetypeTableRecord\n2\nCONTINUOUS\n70\n0\n3\nSolid line\n72\n65\n73\n0\n40\n0\n0\nLTYPE\n100\nAcDbSymbolTableRecord\n100\nAcDbLinetypeTableRecord\n2\nHIDDEN\n70\n0\n3\nHidden edges\n72\n65\n73\n2\n40\n3\n49\n2\n74\n0\n49\n-1\n74\n0\n0\nENDTAB\n0\nTABLE\n5\n11\n330\n0\n2\nLAYER\n100\nAcDbSymbolTable\n70\n3\n");
    for (name, line_type) in [
        ("VISIBLE", "CONTINUOUS"),
        ("HIDDEN", "HIDDEN"),
        ("ANNOTATIONS", "CONTINUOUS"),
    ] {
        write!(out,"0\nLAYER\n100\nAcDbSymbolTableRecord\n100\nAcDbLayerTableRecord\n2\n{name}\n70\n0\n62\n7\n6\n{line_type}\n").unwrap();
    }
    out.push_str("0\nENDTAB\n0\nENDSEC\n");
}

fn append_dxf_label(out: &mut String, label: &DrawingLabel) {
    write!(out,"0\nTEXT\n100\nAcDbEntity\n8\nANNOTATIONS\n100\nAcDbText\n10\n{}\n20\n{}\n30\n0\n40\n3\n1\n{}\n100\nAcDbText\n",label.position_mm[0],label.position_mm[1],dxf_text(&label.text)).unwrap();
}
