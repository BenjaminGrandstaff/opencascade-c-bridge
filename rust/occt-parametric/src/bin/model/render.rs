//! Shaded PNG renders of build outputs from standard views and through
//! section planes, so a multimodal client can check what it built. Faces are
//! flat-shaded with a z-buffer at 2x supersampling, B-rep face boundaries and
//! silhouettes are outlined, and section caps are hatched. Visual review only;
//! measurements stay in the report. O(triangles + pixels) time per image and
//! about 12 bytes per supersampled pixel (50 MB at the 1024 px maximum).
use occt_bridge::{MeshOptions, MeshTriangle, Session, Shape, ShapeType, Vec3};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{error::Error, path::Path};

#[path = "render/png.rs"]
pub mod png;

/// Most standard views per output.
pub const MAX_VIEWS: usize = 8;
/// Most section planes per output.
pub const MAX_SECTIONS: usize = 4;
/// Most images in one build, across all outputs and the scene.
pub const MAX_IMAGES: usize = 256;
/// Most outputs in one scene; interference checks are pairwise.
pub const MAX_SCENE_PARTS: usize = 64;
/// Most triangles tessellated for one output (the bridge's limit).
const MAX_TRIANGLES: usize = 1_000_000;
/// Supersampling factor per axis.
const SS: usize = 2;

/// Images written for each selected output. Each view and each section is
/// one square PNG named `NNNN-<view>.png` or `NNNN-section-<id>.png`.
#[derive(Clone, Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RenderOptions {
    /// Standard views, at most 8; default iso, front, top and right.
    #[serde(default = "default_views")]
    pub views: Vec<RenderView>,
    /// Section planes, at most 4. Material on the side the normal points to
    /// is removed and the cut face is hatched; the camera looks back along
    /// the normal.
    #[serde(default)]
    pub sections: Vec<SectionPlane>,
    /// Image width and height in pixels, 64 to 1024; default 512.
    #[serde(default = "default_size")]
    pub size_px: u32,
    /// Also render every selected output together, at most 64, from the
    /// same views and sections: each output in its own color, and any
    /// volume two outputs share drawn in red through everything. The
    /// report's `scene` lists each output's color and each interference.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub scene: bool,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            views: default_views(),
            sections: Vec::new(),
            size_px: default_size(),
            scene: false,
        }
    }
}

fn default_views() -> Vec<RenderView> {
    vec![
        RenderView::Iso,
        RenderView::Front,
        RenderView::Top,
        RenderView::Right,
    ]
}
fn default_size() -> u32 {
    512
}

/// A view in model coordinates with +Z up: `front` looks along +Y, `top`
/// along -Z, `right` along -X, and `iso` from the front-right-top corner.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RenderView {
    Iso,
    Front,
    Back,
    Top,
    Bottom,
    Left,
    Right,
}

impl RenderView {
    fn name(self) -> &'static str {
        match self {
            Self::Iso => "iso",
            Self::Front => "front",
            Self::Back => "back",
            Self::Top => "top",
            Self::Bottom => "bottom",
            Self::Left => "left",
            Self::Right => "right",
        }
    }
    /// Viewing direction (into the scene) and screen-up hint.
    fn camera(self) -> ([f64; 3], [f64; 3]) {
        let z = [0.0, 0.0, 1.0];
        match self {
            Self::Iso => ([-1.0, 1.0, -1.0], z),
            Self::Front => ([0.0, 1.0, 0.0], z),
            Self::Back => ([0.0, -1.0, 0.0], z),
            Self::Top => ([0.0, 0.0, -1.0], [0.0, 1.0, 0.0]),
            Self::Bottom => ([0.0, 0.0, 1.0], [0.0, -1.0, 0.0]),
            Self::Left => ([1.0, 0.0, 0.0], z),
            Self::Right => ([-1.0, 0.0, 0.0], z),
        }
    }
}

#[derive(Clone, Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SectionPlane {
    /// Letters, digits, `-` and `_`, at most 32; names the image file.
    pub id: String,
    /// A point on the plane, in millimeters.
    pub origin_mm: [f64; 3],
    /// Plane normal; need not be unit length.
    pub normal: [f64; 3],
}

impl RenderOptions {
    /// Checks the options for a build of `outputs` selected outputs.
    pub fn validate(&self, outputs: usize) -> Result<(), String> {
        if self.scene && outputs > MAX_SCENE_PARTS {
            return Err(format!(
                "a render scene holds at most {MAX_SCENE_PARTS} outputs"
            ));
        }
        let images = (outputs + usize::from(self.scene))
            .saturating_mul(self.views.len() + self.sections.len());
        if images > MAX_IMAGES {
            return Err(format!(
                "renders are limited to {MAX_IMAGES} images per build; select fewer outputs, views or sections"
            ));
        }
        if self.views.is_empty() && self.sections.is_empty() {
            return Err("renders need at least one view or section".into());
        }
        if self.views.len() > MAX_VIEWS || self.sections.len() > MAX_SECTIONS {
            return Err(format!(
                "renders take at most {MAX_VIEWS} views and {MAX_SECTIONS} sections"
            ));
        }
        for (i, view) in self.views.iter().enumerate() {
            if self.views[..i].contains(view) {
                return Err(format!("render view '{}' is repeated", view.name()));
            }
        }
        if !(64..=1024).contains(&self.size_px) {
            return Err("render size_px must be 64 to 1024".into());
        }
        for (i, section) in self.sections.iter().enumerate() {
            let id = &section.id;
            if id.is_empty()
                || id.len() > 32
                || !id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            {
                return Err(format!(
                    "section id '{id}' needs 1 to 32 letters, digits, '-' or '_'"
                ));
            }
            if self.sections[..i].iter().any(|s| &s.id == id) {
                return Err(format!("section id '{id}' is repeated"));
            }
            if section
                .origin_mm
                .iter()
                .any(|v| !v.is_finite() || v.abs() > 1e9)
                || unit(section.normal).is_none()
            {
                return Err(format!(
                    "section '{id}' needs a finite origin and a nonzero normal"
                ));
            }
        }
        Ok(())
    }
}

/// Writes the requested images for output `index` into `destination` and
/// returns their report entries.
/// `bend_lines` are drawn dashed over the standard views where visible: a
/// sheet-metal flat blank's bend lines, in model coordinates.
pub fn write(
    session: &Session,
    shape: &Shape<'_>,
    options: &RenderOptions,
    index: usize,
    destination: &Path,
    bend_lines: &[[Vec3; 2]],
) -> Result<Vec<Value>, Box<dyn Error>> {
    let bounds = session.exact_bounds(shape)?;
    let (lo, hi) = (
        [bounds.min.x, bounds.min.y, bounds.min.z],
        [bounds.max.x, bounds.max.y, bounds.max.z],
    );
    let span = (0..3).map(|k| hi[k] - lo[k]).fold(0.0, f64::max);
    if !span.is_finite() || span <= 0.0 {
        return Err("output has no renderable extent".into());
    }
    let mesh_options = MeshOptions {
        linear_deflection: span * 2e-3,
        angular_deflection_radians: 0.25,
        maximum_triangles: MAX_TRIANGLES,
    };
    let size = options.size_px as usize;
    let mut entries = Vec::new();
    if !options.views.is_empty() {
        let triangles = session.surface_mesh(shape, mesh_options)?;
        for &view in &options.views {
            let (direction, up) = view.camera();
            let layer = Layer {
                triangles: &triangles,
                color: BODY,
            };
            let image = draw(&[layer], &[], None, direction, up, size, bend_lines)?;
            let file = format!("{:04}-{}.png", index + 1, view.name());
            std::fs::write(destination.join(&file), &image)?;
            let mut entry =
                json!({"view": view.name(), "file": file, "width_px": size, "height_px": size});
            if !bend_lines.is_empty() {
                entry["bend_lines"] = json!(bend_lines.len());
            }
            entries.push(entry);
        }
    }
    for section in &options.sections {
        let normal = unit(section.normal).ok_or("section normal is zero")?;
        let cut = half_space(session, section.origin_mm, normal, lo, hi)?;
        let kept = session.cut(shape, &cut)?;
        // A plane past the part leaves nothing to mesh.
        let triangles = if session.subshapes(&kept, ShapeType::Face)?.is_empty() {
            Vec::new()
        } else {
            session.surface_mesh(&kept, mesh_options)?
        };
        let file = format!("{:04}-section-{}.png", index + 1, section.id);
        let up = if normal[2].abs() > 0.9 {
            [0.0, 1.0, 0.0]
        } else {
            [0.0, 0.0, 1.0]
        };
        let direction = normal.map(|v| -v);
        let image = if triangles.is_empty() {
            None
        } else {
            let layer = Layer {
                triangles: &triangles,
                color: BODY,
            };
            Some(draw(
                &[layer],
                &[],
                Some((section.origin_mm, normal)),
                direction,
                up,
                size,
                &[],
            )?)
        };
        let mut entry = json!({"section": section.id, "width_px": size, "height_px": size});
        match image {
            Some(image) => {
                std::fs::write(destination.join(&file), &image)?;
                entry["file"] = json!(file);
            }
            None => entry["empty"] = json!(true),
        }
        entries.push(entry);
    }
    Ok(entries)
}

/// Renders `parts` (label and shape) together into `scene-<view>.png` and
/// `scene-section-<id>.png`, with interference volumes in red, and returns
/// the report's `scene`. Each pair of parts whose bounding boxes overlap is
/// intersected once: O(parts² box tests + overlapping pairs' booleans).
pub fn write_scene(
    session: &Session,
    parts: &[(String, &Shape<'_>)],
    options: &RenderOptions,
    destination: &Path,
) -> Result<Value, Box<dyn Error>> {
    let boxes = parts
        .iter()
        .map(|(_, shape)| session.exact_bounds(shape))
        .collect::<Result<Vec<_>, _>>()?;
    let lo = [0, 1, 2].map(|k| {
        boxes
            .iter()
            .map(|b| [b.min.x, b.min.y, b.min.z][k])
            .fold(f64::INFINITY, f64::min)
    });
    let hi = [0, 1, 2].map(|k| {
        boxes
            .iter()
            .map(|b| [b.max.x, b.max.y, b.max.z][k])
            .fold(f64::NEG_INFINITY, f64::max)
    });
    let span = (0..3).map(|k| hi[k] - lo[k]).fold(0.0, f64::max);
    if !span.is_finite() || span <= 0.0 {
        return Err("scene has no renderable extent".into());
    }
    let mesh_options = MeshOptions {
        linear_deflection: span * 2e-3,
        angular_deflection_radians: 0.25,
        maximum_triangles: MAX_TRIANGLES,
    };
    let overlaps = |a: &occt_bridge::Bounds, b: &occt_bridge::Bounds| {
        a.min.x <= b.max.x
            && b.min.x <= a.max.x
            && a.min.y <= b.max.y
            && b.min.y <= a.max.y
            && a.min.z <= b.max.z
            && b.min.z <= a.max.z
    };
    let tolerance = (span.powi(3) * 1e-9).max(1e-6);
    let mut clashes = Vec::new();
    let mut interferences = Vec::new();
    for i in 0..parts.len() {
        for j in i + 1..parts.len() {
            if !overlaps(&boxes[i], &boxes[j]) {
                continue;
            }
            let shared = session.common(parts[i].1, parts[j].1)?;
            // Disjoint or touching parts share no solid.
            if session.subshapes(&shared, ShapeType::Solid)?.is_empty() {
                continue;
            }
            let volume = session.volume(&shared)?;
            if volume > tolerance {
                let b = session.exact_bounds(&shared)?;
                interferences.push(json!({
                    "first": parts[i].0, "second": parts[j].0, "volume_mm3": volume,
                    "bounds_mm": {"min": [b.min.x, b.min.y, b.min.z], "max": [b.max.x, b.max.y, b.max.z]},
                }));
                clashes.push(shared);
            }
        }
    }
    let size = options.size_px as usize;
    let colors = |index: usize| PALETTE[index % PALETTE.len()];
    let mut renders = Vec::new();
    if !options.views.is_empty() {
        let meshes = parts
            .iter()
            .map(|(_, shape)| session.surface_mesh(shape, mesh_options))
            .collect::<Result<Vec<_>, _>>()?;
        let xray = clashes
            .iter()
            .map(|shape| session.surface_mesh(shape, mesh_options))
            .collect::<Result<Vec<_>, _>>()?
            .concat();
        let layers: Vec<Layer<'_>> = meshes
            .iter()
            .enumerate()
            .map(|(index, triangles)| Layer {
                triangles,
                color: colors(index),
            })
            .collect();
        for &view in &options.views {
            let (direction, up) = view.camera();
            let image = draw(&layers, &xray, None, direction, up, size, &[])?;
            let file = format!("scene-{}.png", view.name());
            std::fs::write(destination.join(&file), &image)?;
            renders.push(
                json!({"view": view.name(), "file": file, "width_px": size, "height_px": size}),
            );
        }
    }
    for section in &options.sections {
        let normal = unit(section.normal).ok_or("section normal is zero")?;
        let cut = half_space(session, section.origin_mm, normal, lo, hi)?;
        let mesh_kept = |shape: &Shape<'_>| -> Result<Vec<MeshTriangle>, Box<dyn Error>> {
            let kept = session.cut(shape, &cut)?;
            Ok(if session.subshapes(&kept, ShapeType::Face)?.is_empty() {
                Vec::new()
            } else {
                session.surface_mesh(&kept, mesh_options)?
            })
        };
        let meshes = parts
            .iter()
            .map(|(_, shape)| mesh_kept(shape))
            .collect::<Result<Vec<_>, _>>()?;
        let xray = clashes
            .iter()
            .map(mesh_kept)
            .collect::<Result<Vec<_>, _>>()?
            .concat();
        let mut entry = json!({"section": section.id, "width_px": size, "height_px": size});
        if meshes.iter().all(Vec::is_empty) {
            entry["empty"] = json!(true);
        } else {
            let layers: Vec<Layer<'_>> = meshes
                .iter()
                .enumerate()
                .map(|(index, triangles)| Layer {
                    triangles,
                    color: colors(index),
                })
                .collect();
            let up = if normal[2].abs() > 0.9 {
                [0.0, 1.0, 0.0]
            } else {
                [0.0, 0.0, 1.0]
            };
            let image = draw(
                &layers,
                &xray,
                Some((section.origin_mm, normal)),
                normal.map(|v| -v),
                up,
                size,
                &[],
            )?;
            let file = format!("scene-section-{}.png", section.id);
            std::fs::write(destination.join(&file), &image)?;
            entry["file"] = json!(file);
        }
        renders.push(entry);
    }
    let legend: Vec<Value> = parts
        .iter()
        .enumerate()
        .map(|(index, (label, _))| {
            let c = colors(index);
            json!({"output": label, "color": format!("#{:02x}{:02x}{:02x}", c[0] as u8, c[1] as u8, c[2] as u8)})
        })
        .collect();
    Ok(json!({
        "parts": legend,
        "interferences": interferences,
        "interference_color": "#e41e24",
        "renders": renders,
    }))
}

/// A prism covering the half of the bounding box the normal points into,
/// with margin, to cut away.
fn half_space<'a>(
    session: &'a Session,
    origin: [f64; 3],
    normal: [f64; 3],
    lo: [f64; 3],
    hi: [f64; 3],
) -> Result<Shape<'a>, Box<dyn Error>> {
    let center = [0, 1, 2].map(|k| (lo[k] + hi[k]) / 2.0);
    let reach = (0..3).map(|k| (hi[k] - lo[k]).powi(2)).sum::<f64>().sqrt()
        + norm(sub(center, origin))
        + 1.0;
    let seed = if normal[0].abs() < 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let u = unit(cross(normal, seed)).ok_or("degenerate section frame")?;
    let v = cross(normal, u);
    let corner = |a: f64, b: f64| {
        let p = [0, 1, 2].map(|k| origin[k] + reach * (a * u[k] + b * v[k]));
        Vec3::new(p[0], p[1], p[2])
    };
    let square = [
        corner(-2.0, -2.0),
        corner(2.0, -2.0),
        corner(2.0, 2.0),
        corner(-2.0, 2.0),
    ];
    let depth = normal.map(|k| k * 4.0 * reach);
    Ok(session.create_polygon_prism(&square, Vec3::new(depth[0], depth[1], depth[2]))?)
}

const BACKGROUND: [u8; 3] = [244, 246, 248];
const BODY: [f32; 3] = [150.0, 172.0, 204.0];
const CAP: [u8; 3] = [214, 104, 74];
const CAP_STRIPE: [u8; 3] = [176, 72, 52];
const EDGE: [u8; 3] = [34, 40, 52];
const BEND: [u8; 3] = [24, 92, 214];
const CLASH: [u8; 3] = [228, 30, 36];
const CLASH_EDGE: [u8; 3] = [110, 0, 8];
/// Base colors of scene parts, in output order (cycled).
pub const PALETTE: [[f32; 3]; 6] = [
    [150.0, 172.0, 204.0],
    [148.0, 196.0, 150.0],
    [214.0, 190.0, 120.0],
    [186.0, 160.0, 206.0],
    [120.0, 190.0, 196.0],
    [200.0, 160.0, 140.0],
];
/// Dash and gap length of bend lines, in output pixels.
const DASH_PX: f64 = 6.0;

/// One part of a scene: its triangles and base color.
pub struct Layer<'a> {
    pub triangles: &'a [MeshTriangle],
    pub color: [f32; 3],
}

/// Calls `pixel(x, y, depth)` for each supersampled pixel center the screen
/// triangle `s` covers, with depth interpolated. O(bounding box).
fn fill(s: [[f64; 3]; 3], n: usize, mut pixel: impl FnMut(usize, usize, f32)) {
    let area = edge(s[0], s[1], s[2]);
    if area.abs() < 1e-12 {
        return;
    }
    let (mut x0, mut x1, mut y0, mut y1) = (
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
    );
    for q in &s {
        (x0, x1, y0, y1) = (x0.min(q[0]), x1.max(q[0]), y0.min(q[1]), y1.max(q[1]));
    }
    if !(x0.is_finite() && x1.is_finite() && y0.is_finite() && y1.is_finite())
        || x1 < 0.0
        || y1 < 0.0
    {
        return;
    }
    let (x0, y0) = (x0.floor().max(0.0) as usize, y0.floor().max(0.0) as usize);
    let (x1, y1) = (
        (x1.ceil() as usize).min(n - 1),
        (y1.ceil() as usize).min(n - 1),
    );
    for y in y0..=y1 {
        for x in x0..=x1 {
            let c = [x as f64 + 0.5, y as f64 + 0.5, 0.0];
            let w = [
                edge(s[1], s[2], c) / area,
                edge(s[2], s[0], c) / area,
                edge(s[0], s[1], c) / area,
            ];
            if w.iter().all(|&w| w >= -1e-9) {
                pixel(
                    x,
                    y,
                    (w[0] * s[0][2] + w[1] * s[1][2] + w[2] * s[2][2]) as f32,
                );
            }
        }
    }
}

/// Rasterizes oriented triangles into a PNG: each layer in its color with
/// B-rep faces outlined, `xray` triangles (interference volumes) tinted red
/// through everything, and `overlays` dashed where they lie on visible
/// surface. `section` marks triangles on the cut plane as hatched caps.
fn draw(
    parts: &[Layer<'_>],
    xray: &[MeshTriangle],
    section: Option<([f64; 3], [f64; 3])>,
    direction: [f64; 3],
    up_hint: [f64; 3],
    size: usize,
    overlays: &[[Vec3; 2]],
) -> Result<Vec<u8>, Box<dyn Error>> {
    let view = unit(direction).ok_or("zero view direction")?;
    let right = unit(cross(view, up_hint)).ok_or("view direction parallel to up")?;
    let up = cross(right, view);
    let project = |p: Vec3| {
        let p = [p.x, p.y, p.z];
        [dot(p, right), dot(p, up), dot(p, view)]
    };
    let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
    for t in parts.iter().flat_map(|layer| layer.triangles) {
        for &p in &t.points {
            let q = project(p);
            for k in 0..3 {
                lo[k] = lo[k].min(q[k]);
                hi[k] = hi[k].max(q[k]);
            }
        }
    }
    if !(0..3).all(|k| lo[k].is_finite() && hi[k].is_finite()) {
        return Err("nonfinite render coordinates".into());
    }
    let n = size * SS;
    let extent = (hi[0] - lo[0]).max(hi[1] - lo[1]).max(1e-9);
    let scale = n as f64 * 0.86 / extent;
    let offset = [
        n as f64 / 2.0 - (lo[0] + hi[0]) / 2.0 * scale,
        n as f64 / 2.0 + (lo[1] + hi[1]) / 2.0 * scale,
    ];
    let depth_span = (hi[2] - lo[2]).max(extent * 1e-6);
    let screen = |p: Vec3| {
        let q = project(p);
        [
            q[0] * scale + offset[0],
            -q[1] * scale + offset[1],
            (q[2] - lo[2]) / depth_span,
        ]
    };
    // Toward the light: from the viewer, up and to the left.
    let light = unit([-0.35, 0.55, -0.75])
        .map(|l| [0, 1, 2].map(|k| l[0] * right[k] + l[1] * up[k] + l[2] * view[k]))
        .unwrap();
    let mut depth = vec![f32::INFINITY; n * n];
    // Face id per pixel, unique across layers; 0 is background, u32::MAX a
    // section cap.
    let mut face = vec![0u32; n * n];
    let mut color = vec![BACKGROUND; n * n];
    let plane_tolerance = extent * 1e-5;
    let mut first_id = 1u32;
    for layer in parts {
        // One part keeps the orange hatch; several hatch their caps in
        // darker shades of their own color so the parts stay apart.
        let (cap_light, cap_dark) = if parts.len() == 1 {
            (CAP, CAP_STRIPE)
        } else {
            (
                layer.color.map(|c| (c * 0.78) as u8),
                layer.color.map(|c| (c * 0.56) as u8),
            )
        };
        let mut next_id = first_id;
        for t in layer.triangles {
            let p = t.points.map(|p| [p.x, p.y, p.z]);
            let Some(normal) = unit(cross(sub(p[1], p[0]), sub(p[2], p[0]))) else {
                continue;
            };
            let cap = section.is_some_and(|(origin, plane)| {
                p.iter()
                    .all(|&q| dot(sub(q, origin), plane).abs() <= plane_tolerance)
            });
            let facing = -dot(normal, view);
            let shade = if cap {
                None
            } else {
                let lambert = dot(normal, light).abs() as f32;
                let base = if facing >= 0.0 { 1.0 } else { 0.6 };
                Some(
                    layer
                        .color
                        .map(|c| (c * base * (0.45 + 0.55 * lambert)).round() as u8),
                )
            };
            let id = if cap {
                u32::MAX
            } else {
                first_id + t.face_index as u32
            };
            next_id = next_id.max(id.saturating_add(1).min(u32::MAX - 1));
            fill(t.points.map(&screen), n, |x, y, z| {
                let at = y * n + x;
                if z < depth[at] {
                    depth[at] = z;
                    face[at] = id;
                    color[at] = match shade {
                        Some(shade) => shade,
                        None if ((x + y) / (6 * SS)).is_multiple_of(2) => cap_dark,
                        None => cap_light,
                    };
                }
            });
        }
        first_id = next_id;
    }
    // Interference volumes seen through every part.
    let mut clash = vec![false; n * n];
    for t in xray {
        fill(t.points.map(&screen), n, |x, y, _| clash[y * n + x] = true);
    }
    // Dashed overlay lines where they lie on visible surface; two
    // supersampled pixels wide, stepped at half a pixel. O(line length).
    let mut overlay = vec![false; n * n];
    for [a, b] in overlays {
        let (a, b) = (screen(*a), screen(*b));
        let length = (b[0] - a[0]).hypot(b[1] - a[1]);
        let steps = (length * 2.0).ceil().clamp(1.0, 1e6) as usize;
        for k in 0..=steps {
            let t = k as f64 / steps as f64;
            if ((t * length) / (DASH_PX * SS as f64)) as usize % 2 == 1 {
                continue;
            }
            let p = [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t);
            for (dx, dy) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)] {
                let (x, y) = ((p[0] + dx - 0.5).floor(), (p[1] + dy - 0.5).floor());
                if x < 0.0 || y < 0.0 || x >= n as f64 || y >= n as f64 {
                    continue;
                }
                let at = y as usize * n + x as usize;
                // Only over the part, and not behind its surface.
                if face[at] != 0 && p[2] as f32 <= depth[at] + 2e-3 {
                    overlay[at] = true;
                }
            }
        }
    }
    // Outlines where the B-rep face changes or depth jumps (silhouettes).
    let jump = 0.02f32;
    let mut outline = vec![false; n * n];
    let mut clash_edge = vec![false; n * n];
    for y in 0..n {
        for x in 0..n {
            let at = y * n + x;
            for (dx, dy) in [(1, 0), (0, 1)] {
                let (nx, ny) = (x + dx, y + dy);
                if nx >= n || ny >= n {
                    continue;
                }
                let other = ny * n + nx;
                if face[at] != face[other]
                    || (face[at] != 0 && (depth[at] - depth[other]).abs() > jump)
                {
                    outline[at] = true;
                    outline[other] = true;
                }
                if clash[at] != clash[other] {
                    clash_edge[at] = true;
                    clash_edge[other] = true;
                }
            }
        }
    }
    let mut rgb = vec![0u8; size * size * 3];
    for y in 0..size {
        for x in 0..size {
            let mut sum = [0.0f32; 3];
            for sy in 0..SS {
                for sx in 0..SS {
                    let at = (y * SS + sy) * n + x * SS + sx;
                    let mut c = if overlay[at] {
                        BEND
                    } else if outline[at] {
                        EDGE
                    } else {
                        color[at]
                    };
                    if clash_edge[at] {
                        c = CLASH_EDGE;
                    } else if clash[at] {
                        c = [0, 1, 2].map(|k| {
                            (0.35 * f32::from(c[k]) + 0.65 * f32::from(CLASH[k])).round() as u8
                        });
                    }
                    for k in 0..3 {
                        sum[k] += f32::from(c[k]);
                    }
                }
            }
            for k in 0..3 {
                rgb[(y * size + x) * 3 + k] = (sum[k] / (SS * SS) as f32).round() as u8;
            }
        }
    }
    triad(&mut rgb, size, right, up);
    Ok(png::encode(size as u32, size as u32, &rgb))
}

/// Draws model X (red), Y (green) and Z (blue) axes in the lower-left corner.
fn triad(rgb: &mut [u8], size: usize, right: [f64; 3], up: [f64; 3]) {
    let length = (size as f64 * 0.06).max(10.0);
    let origin = [length * 1.4, size as f64 - length * 1.4];
    for (axis, c) in [
        ([1.0, 0.0, 0.0], [200, 40, 40]),
        ([0.0, 1.0, 0.0], [40, 150, 60]),
        ([0.0, 0.0, 1.0], [40, 80, 210]),
    ] {
        let d = [dot(axis, right), -dot(axis, up)];
        let steps = (length * 2.0) as usize;
        for i in 0..=steps {
            let t = i as f64 / steps as f64 * length;
            let (x, y) = (origin[0] + d[0] * t, origin[1] + d[1] * t);
            for (ox, oy) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0)] {
                let (px, py) = ((x + ox) as isize, (y + oy) as isize);
                if px >= 0 && py >= 0 && (px as usize) < size && (py as usize) < size {
                    let at = (py as usize * size + px as usize) * 3;
                    rgb[at..at + 3].copy_from_slice(&c);
                }
            }
        }
    }
}

fn edge(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}
fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn norm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}
fn unit(a: [f64; 3]) -> Option<[f64; 3]> {
    let l = norm(a);
    (l.is_finite() && l > 1e-12).then(|| a.map(|v| v / l))
}
