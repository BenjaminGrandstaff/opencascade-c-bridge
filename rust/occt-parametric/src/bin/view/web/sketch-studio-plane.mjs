// Sketch planes for the sketch studio. Sketches live in model millimetres
// (Z up); the renderer works in glTF metres (Y up), as `modelToGltf` maps.

export const add = (a, b) => [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
export const sub = (a, b) => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
export const scale = (a, s) => [a[0] * s, a[1] * s, a[2] * s];
export const dot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
export const cross = (a, b) => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
export const unit = (a) => {
  const length = Math.hypot(...a);
  if (!(length > 0)) throw new Error('A direction must be nonzero.');
  return scale(a, 1 / length);
};

/** A model point (mm, Z up) in glTF metres (Y up). */
export const toGltf = ([x, y, z]) => [x / 1000, z / 1000, -y / 1000];
/** A glTF point back in model millimetres. */
export const fromGltf = ([x, y, z]) => [x * 1000, -z * 1000, y * 1000];
/** Directions convert without the unit change. */
export const directionToGltf = ([x, y, z]) => [x, z, -y];
export const directionFromGltf = ([x, y, z]) => [x, -z, y];

/**
 * The three base planes, with the axes and normals the existing New sketch
 * flow uses: XY faces +Z, XZ faces −Y, YZ faces +X. An offset moves the
 * origin along the normal.
 */
export const BASE_PLANES = {
  xy: { label: 'XY', x_axis: [1, 0, 0], y_axis: [0, 1, 0] },
  xz: { label: 'XZ', x_axis: [1, 0, 0], y_axis: [0, 0, 1] },
  yz: { label: 'YZ', x_axis: [0, 1, 0], y_axis: [0, 0, 1] },
};
export function basePlane(name, offset = 0) {
  const base = BASE_PLANES[name];
  if (!base) throw new Error('Choose the XY, XZ or YZ plane.');
  if (!Number.isFinite(offset)) throw new Error('The plane offset must be finite.');
  const frame = { origin_mm: [0, 0, 0], x_axis: base.x_axis, y_axis: base.y_axis };
  return { ...frame, origin_mm: scale(planeNormal(frame), offset) };
}

export const planeNormal = (frame) => unit(cross(frame.x_axis, frame.y_axis));

/** A sketch-local point [u, v] in model millimetres. */
export const toWorld = (frame, [u, v]) =>
  add(frame.origin_mm, add(scale(frame.x_axis, u), scale(frame.y_axis, v)));

/** A model point's sketch-local coordinates (its foot on the plane). */
export function toLocal(frame, point) {
  const offset = sub(point, frame.origin_mm);
  return [dot(offset, frame.x_axis), dot(offset, frame.y_axis)];
}

/**
 * Where a renderer ray (glTF origin and direction) meets the plane, in
 * sketch-local millimetres, or null when it runs parallel to the plane or
 * meets it behind the eye.
 */
export function rayToLocal(frame, { origin, direction }) {
  const o = fromGltf(origin);
  const d = directionFromGltf(direction);
  const n = planeNormal(frame);
  const denominator = dot(d, n);
  if (Math.abs(denominator) < 1e-9) return null;
  const t = dot(sub(frame.origin_mm, o), n) / denominator;
  if (!(t > 0) || !Number.isFinite(t)) return null;
  return toLocal(frame, add(o, scale(d, t)));
}

/**
 * Screen pixels [x, y] of a glTF point under a column-major view-projection
 * matrix, for a view `width` × `height` pixels, or null behind the eye.
 */
export function project(matrix, [x, y, z], width, height) {
  const m = matrix;
  const w = m[3] * x + m[7] * y + m[11] * z + m[15];
  if (!(w > 1e-12)) return null;
  const nx = (m[0] * x + m[4] * y + m[8] * z + m[12]) / w;
  const ny = (m[1] * x + m[5] * y + m[9] * z + m[13]) / w;
  return [((nx + 1) / 2) * width, ((1 - ny) / 2) * height];
}

export const projectLocal = (matrix, frame, local, width, height) =>
  project(matrix, toGltf(toWorld(frame, local)), width, height);

/**
 * An orbit camera square to the plane, looking at sketch-local `center`
 * from the plane's normal side, with the sketch x axis pointing right.
 */
export function lookAt(camera, frame, center = [0, 0]) {
  const n = directionToGltf(planeNormal(frame));
  const limit = Math.PI / 2 - 0.01; // the orbit controls' limit
  const pitch = Math.max(-limit, Math.min(limit, Math.asin(Math.max(-1, Math.min(1, n[1])))));
  let yaw;
  if (Math.hypot(n[0], n[2]) > 1e-6 && Math.abs(n[1]) < 0.999) {
    yaw = Math.atan2(n[0], n[2]);
  } else {
    // Looking straight down or up: turn so the sketch x axis is screen right,
    // which for an orbit camera is (cos yaw, 0, −sin yaw) in glTF.
    const x = directionToGltf(frame.x_axis);
    yaw = Math.atan2(-x[2], x[0]);
  }
  return { ...camera, target: toGltf(toWorld(frame, center)), yaw, pitch };
}

/**
 * A round grid spacing in millimetres that puts at least `minimumPixels`
 * between lines, given how many pixels one millimetre covers.
 */
export function gridSpacing(pixelsPerMm, minimumPixels = 24) {
  if (!(pixelsPerMm > 0) || !Number.isFinite(pixelsPerMm)) return 1;
  const raw = minimumPixels / pixelsPerMm;
  const power = 10 ** Math.floor(Math.log10(raw));
  for (const step of [1, 2, 5, 10]) if (step * power >= raw) return step * power;
  return 10 * power;
}

/**
 * The face index of the nearest mesh triangle a renderer ray hits, or -1.
 * Triangles are `{face, points}` in model millimetres, as solid scenes list
 * them. O(triangles).
 */
export function pickFace(triangles, { origin, direction }) {
  const o = fromGltf(origin);
  const d = directionFromGltf(direction);
  let best = Infinity;
  let face = -1;
  for (const { face: index, points: [a, b, c] } of triangles) {
    const e1 = sub(b, a);
    const e2 = sub(c, a);
    const q = cross(d, e2);
    const determinant = dot(e1, q);
    if (Math.abs(determinant) < 1e-20) continue;
    const s = sub(o, a);
    const u = dot(s, q) / determinant;
    if (u < 0 || u > 1) continue;
    const r = cross(s, e1);
    const v = dot(d, r) / determinant;
    if (v < 0 || u + v > 1) continue;
    const t = dot(e2, r) / determinant;
    if (t > 0 && t < best) {
      best = t;
      face = index;
    }
  }
  return face;
}
