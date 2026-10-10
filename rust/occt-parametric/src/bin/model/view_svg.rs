//! Standalone annotated snapshots, with the same scene identities as the viewer.
use serde_json::Value;
use std::fmt::Write;
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
fn p(value: &Value) -> [f64; 3] {
    std::array::from_fn(|i| value[i].as_f64().unwrap_or(0.0))
}
fn color(status: &str) -> &str {
    match status {
        "failed" => "#b52b36",
        "passed" => "#24734d",
        "driving" => "#315fb3",
        "measured" => "#487087",
        "fixed" => "#566a82",
        _ => "#956512",
    }
}
pub fn render(scene: &Value) -> String {
    let mut result = String::from(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1000\" height=\"720\" viewBox=\"0 0 1000 720\"><rect width=\"1000\" height=\"720\" fill=\"#f8fafc\"/>",
    );
    write!(result,"<text x=\"30\" y=\"35\" font-family=\"sans-serif\" font-size=\"20\">{}</text><text x=\"30\" y=\"58\" font-family=\"sans-serif\" font-size=\"12\" fill=\"#66758a\">Diagnostic snapshot · {} · inspect linked controls in viewer.html</text>",escape(scene["title"].as_str().unwrap_or("Geometry")),escape(scene["coordinate_system"].as_str().unwrap_or("local coordinates"))).unwrap();
    if let Some(error) = scene["error"].as_str() {
        write!(result,"<text x=\"30\" y=\"150\" fill=\"#b52b36\" font-family=\"sans-serif\" font-size=\"14\">{}</text></svg>",escape(error)).unwrap();
        return result;
    }
    let min = p(&scene["bounds"][0]);
    let max = p(&scene["bounds"][1]);
    let center = std::array::from_fn::<_, 3, _>(|i| min[i] * 0.5 + max[i] * 0.5);
    let span = (0..3).map(|i| max[i] - min[i]).fold(1e-12, f64::max);
    let sketch = scene["kind"] == "sketch";
    let project = |point: [f64; 3]| {
        let v = std::array::from_fn::<_, 3, _>(|i| (point[i] - center[i]) / span);
        let (x, y) = if sketch {
            (v[0], v[1])
        } else {
            (
                (v[0] + v[1]) / 2f64.sqrt(),
                (-v[0] + v[1] + 2.0 * v[2]) / 6f64.sqrt(),
            )
        };
        [350.0 + x * 530.0, 360.0 - y * 530.0]
    };
    if let Some(mesh) = scene["mesh"].as_array() {
        let mut triangles = mesh.iter().collect::<Vec<_>>();
        let depth = |t: &&Value| {
            t["points"]
                .as_array()
                .unwrap()
                .iter()
                .map(|point| {
                    let point = p(point);
                    point[0] - point[1] + point[2]
                })
                .sum::<f64>()
        };
        triangles.sort_by(|a, b| depth(a).total_cmp(&depth(b)));
        for t in triangles {
            let points = t["points"]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| {
                    let q = project(p(value));
                    format!("{},{}", q[0], q[1])
                })
                .collect::<Vec<_>>()
                .join(" ");
            write!(result,"<polygon points=\"{points}\" fill=\"#bccddd\" stroke=\"#a7bbcf\" stroke-width=\"0.4\"/>").unwrap();
        }
    }
    let external_ids: std::collections::HashSet<_> = scene["entities"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|e| e["external"] == true)
        .filter_map(|e| e["id"].as_str())
        .collect();
    let mut lines = Vec::new();
    if let Some(entities) = scene["entities"].as_array() {
        for entity in entities {
            if let Some(ids) = entity["bspline"]["control_points"].as_array() {
                let points = ids
                    .iter()
                    .map(|id| {
                        let q = project(p(&scene["points"][id.as_str().unwrap()]));
                        format!("{},{}", q[0], q[1])
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                write!(result, "<polyline points=\"{points}\" fill=\"none\" stroke=\"#8894a4\" stroke-dasharray=\"5 4\"/>").unwrap();
            }
            lines.push((&entity["points"], entity["id"].as_str().unwrap_or("")));
        }
    }
    if let Some(edges) = scene["lines"].as_array() {
        for edge in edges {
            lines.push((edge, ""));
        }
    }
    if let Some(edited) = scene["edited_profile"].as_array() {
        for line in edited {
            lines.push((line, "edited-profile"));
        }
    }
    if let Some(annotations) = scene["annotations"].as_array() {
        for a in annotations {
            if let Some(paths) = a["detail"]["dimension_paths"].as_array() {
                for path in paths {
                    lines.push((path, "route-dimension"));
                }
            }
            if a["detail"]["angular_arc"]
                .as_array()
                .is_some_and(|p| !p.is_empty())
            {
                lines.push((&a["detail"]["angular_arc"], "angular-dimension"));
            }
        }
    }
    for (points, id) in lines {
        let points = points
            .as_array()
            .unwrap()
            .iter()
            .map(|value| {
                let q = project(p(value));
                format!("{},{}", q[0], q[1])
            })
            .collect::<Vec<_>>()
            .join(" ");
        let stroke = if id == "edited-profile" {
            "#8254bc"
        } else if external_ids.contains(id) {
            "#16857a"
        } else {
            "#365472"
        };
        let opacity = if id != "edited-profile"
            && id != "angular-dimension"
            && scene["edited_profile"]
                .as_array()
                .is_some_and(|p| !p.is_empty())
        {
            0.35
        } else {
            1.0
        };
        write!(result,"<polyline opacity=\"{opacity}\" data-entity=\"{}\" points=\"{points}\" fill=\"none\" stroke=\"{stroke}\" stroke-width=\"1.3\"/>",escape(id)).unwrap();
    }
    if let Some(points) = scene["points"].as_object() {
        for (id, point) in points {
            let q = project(p(point));
            write!(result,"<circle cx=\"{}\" cy=\"{}\" r=\"3\" fill=\"white\" stroke=\"#426d90\"><title>{}</title></circle>",q[0],q[1],escape(id)).unwrap();
        }
    }
    let annotations = scene["annotations"].as_array().unwrap();
    let mut positions = Vec::<[f64; 2]>::new();
    for (index, a) in annotations.iter().enumerate() {
        let status = a["status"].as_str().unwrap_or("");
        let c = color(status);
        if index < 22 {
            let y = 100 + index * 26;
            write!(result,"<text x=\"710\" y=\"{y}\" fill=\"{c}\" font-family=\"sans-serif\" font-size=\"12\">{} · {status}</text>",escape(a["label"].as_str().unwrap_or(""))).unwrap();
        }
        let anchors = a["anchors"].as_array().unwrap();
        let points = anchors.iter().map(|v| project(p(v))).collect::<Vec<_>>();
        if points.is_empty() {
            continue;
        }
        if a["kind"] == "dimension"
            && points.len() >= 2
            && !a["detail"]["angular_arc"]
                .as_array()
                .is_some_and(|p| !p.is_empty())
        {
            write!(
                result,
                "<path d=\"M{},{} L{},{}\" stroke=\"{c}\" stroke-width=\"1.6\" fill=\"none\"/>",
                points[0][0], points[0][1], points[1][0], points[1][1]
            )
            .unwrap();
        }
        if a["kind"] == "requirement" && status == "failed" {
            for q in &points {
                write!(
                    result,
                    "<circle cx=\"{}\" cy=\"{}\" r=\"6\" fill=\"{c}\" stroke=\"white\"/>",
                    q[0], q[1]
                )
                .unwrap();
            }
        }
        let mut at = [
            points.iter().map(|p| p[0]).sum::<f64>() / points.len() as f64,
            points.iter().map(|p| p[1]).sum::<f64>() / points.len() as f64 - 18.0,
        ];
        for _ in 0..8 {
            if positions
                .iter()
                .any(|q| (q[0] - at[0]).abs() < 140.0 && (q[1] - at[1]).abs() < 24.0)
            {
                at[1] -= 25.0;
            } else {
                break;
            }
        }
        at[0] = at[0].clamp(90.0, 600.0);
        at[1] = at[1].clamp(90.0, 680.0);
        positions.push(at);
        if a["kind"] != "parameter" {
            write!(result,"<g data-annotation=\"{}\" fill=\"{c}\" font-family=\"sans-serif\" font-size=\"12\"><title>{}</title><rect x=\"{}\" y=\"{}\" width=\"160\" height=\"23\" rx=\"4\" fill=\"white\" stroke=\"{c}\"/><text x=\"{}\" y=\"{}\" text-anchor=\"middle\">{}</text></g>",escape(a["id"].as_str().unwrap_or("")),escape(&a["detail"].to_string()),at[0]-80.0,at[1]-15.0,at[0],at[1],escape(a["label"].as_str().unwrap_or(""))).unwrap();
        }
    }
    result.push_str("</svg>");
    result
}
