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
/// Most images in one build, across all outputs.
pub const MAX_IMAGES: usize = 256;
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
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            views: default_views(),
            sections: Vec::new(),
            size_px: default_size(),
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
        if outputs.saturating_mul(self.views.len() + self.sections.len()) > MAX_IMAGES {
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
pub fn write(
    session: &Session,
    shape: &Shape<'_>,
    options: &RenderOptions,
    index: usize,
    destination: &Path,
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
            let image = draw(&triangles, None, direction, up, size)?;
            let file = format!("{:04}-{}.png", index + 1, view.name());
            std::fs::write(destination.join(&file), &image)?;
            entries.push(
                json!({"view": view.name(), "file": file, "width_px": size, "height_px": size}),
            );
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
            Some(draw(
                &triangles,
                Some((section.origin_mm, normal)),
                direction,
                up,
                size,
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

/// Rasterizes oriented triangles into a PNG. `section` marks triangles on
/// the cut plane as cap faces.
fn draw(
    triangles: &[MeshTriangle],
    section: Option<([f64; 3], [f64; 3])>,
    direction: [f64; 3],
    up_hint: [f64; 3],
    size: usize,
) -> Result<Vec<u8>, Box<dyn Error>> {
    let view = unit(direction).ok_or("zero view direction")?;
    let right = unit(cross(view, up_hint)).ok_or("view direction parallel to up")?;
    let up = cross(right, view);
    let project = |p: Vec3| {
        let p = [p.x, p.y, p.z];
        [dot(p, right), dot(p, up), dot(p, view)]
    };
    let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
    for t in triangles {
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
    // Toward the light: from the viewer, up and to the left.
    let light = unit([-0.35, 0.55, -0.75])
        .map(|l| [0, 1, 2].map(|k| l[0] * right[k] + l[1] * up[k] + l[2] * view[k]))
        .unwrap();
    let mut depth = vec![f32::INFINITY; n * n];
    // Face id + 1 per pixel; 0 is background, u32::MAX a section cap.
    let mut face = vec![0u32; n * n];
    let mut color = vec![BACKGROUND; n * n];
    let plane_tolerance = extent * 1e-5;
    for t in triangles {
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
            Some(BODY.map(|c| (c * base * (0.45 + 0.55 * lambert)).round() as u8))
        };
        let id = if cap {
            u32::MAX
        } else {
            t.face_index as u32 + 1
        };
        let s = t.points.map(|q| {
            let q = project(q);
            [
                q[0] * scale + offset[0],
                -q[1] * scale + offset[1],
                (q[2] - lo[2]) / depth_span,
            ]
        });
        let area = edge(s[0], s[1], s[2]);
        if area.abs() < 1e-12 {
            continue;
        }
        let x0 = s
            .iter()
            .map(|q| q[0])
            .fold(f64::INFINITY, f64::min)
            .floor()
            .max(0.0) as usize;
        let x1 = (s
            .iter()
            .map(|q| q[0])
            .fold(f64::NEG_INFINITY, f64::max)
            .ceil() as usize)
            .min(n - 1);
        let y0 = s
            .iter()
            .map(|q| q[1])
            .fold(f64::INFINITY, f64::min)
            .floor()
            .max(0.0) as usize;
        let y1 = (s
            .iter()
            .map(|q| q[1])
            .fold(f64::NEG_INFINITY, f64::max)
            .ceil() as usize)
            .min(n - 1);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let c = [x as f64 + 0.5, y as f64 + 0.5, 0.0];
                let w = [
                    edge(s[1], s[2], c) / area,
                    edge(s[2], s[0], c) / area,
                    edge(s[0], s[1], c) / area,
                ];
                if w.iter().any(|&w| w < -1e-9) {
                    continue;
                }
                let z = (w[0] * s[0][2] + w[1] * s[1][2] + w[2] * s[2][2]) as f32;
                let at = y * n + x;
                if z < depth[at] {
                    depth[at] = z;
                    face[at] = id;
                    color[at] = match shade {
                        Some(shade) => shade,
                        None if ((x + y) / (6 * SS)).is_multiple_of(2) => CAP_STRIPE,
                        None => CAP,
                    };
                }
            }
        }
    }
    // Outlines where the B-rep face changes or depth jumps (silhouettes).
    let jump = 0.02f32;
    let mut outline = vec![false; n * n];
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
                    let c = if outline[at] { EDGE } else { color[at] };
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
