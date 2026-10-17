// Datum planes in the sketch studio: the six constructed kinds, the edits
// that create them with the parameters that drive them, and their outlines
// in the 3D view. Every plane is fully defined by its references and those
// parameters, so it never floats free.
import { BASE_PLANES, add, cross, dot, fromGltf, directionFromGltf, project, scale, sub, toGltf } from './sketch-studio-plane.mjs';

export const PLANE_TYPES = [
  { id: 'offset', label: 'Offset', hint: 'A base plane moved along its normal by a distance.' },
  { id: 'angle', label: 'Angle', hint: 'A base plane turned about one of its axes by an angle.' },
  { id: 'face', label: 'On face', hint: 'Click a flat face; the plane follows it, offset by a distance.' },
  { id: 'midplane', label: 'Midplane', hint: 'Click two parallel faces; the plane stays halfway between them.' },
  { id: 'three_point', label: '3 points', hint: 'Click three points on the model, or type them.' },
  { id: 'path', label: 'Normal to path', hint: 'Across a sweep path at a fraction of its length; sketches on it follow the path.' },
];

/** Features a path plane can sit across: sketch wires and helices. */
export const isPathFeature = (feature) =>
  ['sketch_open_wire', 'sketch_wire', 'helix'].some((key) => Object.hasOwn(feature?.operation ?? {}, key));

/** Icons in a 24 × 24 view box drawn with the current text color. */
export const PLANE_ICONS = {
  offset: '<path d="M3 15l6-4h12l-6 4z"/><path d="M3 9l6-4h12l-6 4z" opacity=".55"/><path d="M12 13V7M10.5 8.5L12 7l1.5 1.5"/>',
  angle: '<path d="M3 17l6-4h12l-6 4z"/><path d="M9 13L13 4h8l-4 9" opacity=".7"/>',
  face: '<path d="M4 10h16v10H4z" fill="currentColor" opacity=".25"/><path d="M4 10h16v10H4zM3 7h18"/><path d="M12 7V4"/>',
  midplane: '<path d="M4 4h16M4 20h16"/><path d="M4 12h16" stroke-dasharray="3 2"/><path d="M12 6v4M12 14v4"/>',
  path: '<path d="M3 19c6 0 6-14 18-14"/><path d="M8 9v10M5 11l3-2 3 2" opacity=".7"/>',
  three_point: '<path d="M4 17L12 5l8 12z" opacity=".55"/><circle cx="4" cy="17" r="2" fill="currentColor"/><circle cx="12" cy="5" r="2" fill="currentColor"/><circle cx="20" cy="17" r="2" fill="currentColor"/>',
};

export const DEGREE = Math.PI / 180;
const length = (value) => ({ value, dimension: 'length', unit: 'millimeter' });
const scalar = (value) => ({ value, dimension: 'scalar', unit: null });
const lengths = ([x, y, z]) => ({ x: length(x), y: length(y), z: length(z) });
const scalars = ([x, y, z]) => ({ x: scalar(x), y: scalar(y), z: scalar(z) });

/** A new length, angle (radians) or point parameter definition. */
export const lengthParameter = (id, mm) =>
  ({ id, parameter_type: { scalar: 'length' }, default: { scalar: length(mm) }, minimum: null, maximum: null });
export const angleParameter = (id, radians) =>
  ({ id, parameter_type: { scalar: 'scalar' }, default: { scalar: scalar(radians) }, minimum: null, maximum: null });
export const fractionParameter = (id, fraction) =>
  ({ id, parameter_type: { scalar: 'scalar' }, default: { scalar: scalar(fraction) }, minimum: scalar(0), maximum: scalar(1) });
export const pointParameter = (id, point) =>
  ({ id, parameter_type: { vector: 'length' }, default: { vector: lengths(point) }, minimum: null, maximum: null });

/** `wanted`, or `wanted-2`, `wanted-3`… when it is taken. */
export function freeName(wanted, taken) {
  const names = new Set(taken);
  if (!names.has(wanted)) return wanted;
  for (let i = 2; ; i += 1) if (!names.has(`${wanted}-${i}`)) return `${wanted}-${i}`;
}

/**
 * The plane a base choice stands for: `xy`, `xz`, `yz` or `datum:<id>`,
 * with that plane's frame (from the resolved datum planes) for hinges.
 */
export function baseOf(choice, planes = []) {
  if (BASE_PLANES[choice]) {
    const { x_axis, y_axis } = BASE_PLANES[choice];
    return { base: choice, frame: { origin_mm: [0, 0, 0], x_axis, y_axis } };
  }
  const id = choice?.startsWith('datum:') ? choice.slice(6) : null;
  const plane = planes.find((p) => p.id === id);
  if (!plane) throw new Error('Choose a base plane.');
  if (plane.status !== 'resolved') throw new Error(`Base plane ${id} does not resolve: ${plane.error}`);
  return { base: { datum: id }, frame: plane };
}

/**
 * The datum and the new parameters that drive it, for plane type `type`:
 * offset `distance` (mm); angle `angle` (degrees) about the base's `hinge`
 * axis ('x' or 'y'); face `distance` from `faces[0]`; midplane between
 * `faces[0]` and `faces[1]`; three_point through `points`; path across
 * `path` (a path feature among `features`) at `position`, a fraction of its
 * length (`<name>_position`). Faces are
 * results of the face pick ({face_support: {input, face}}). Parameters are
 * named after the plane (`<name>_distance`, `<name>_angle`, `<name>_p1`…),
 * skipping names in `taken`.
 */
export function planeEdit(type, values, planes = [], taken = [], features = []) {
  const id = values.name?.trim();
  if (!id || id !== values.name) throw new Error('Enter a plane name without surrounding spaces.');
  const names = [...taken];
  const parameter = (suffix) => {
    const name = freeName(`${id}_${suffix}`, names);
    names.push(name);
    return name;
  };
  const finite = (value, what) => {
    if (!Number.isFinite(value)) throw new Error(`${what} must be a finite number.`);
    return value;
  };
  const face = (i) => {
    const picked = values.faces?.[i];
    if (!picked) throw new Error(i ? 'Pick the second face.' : 'Pick a face.');
    return picked.face_support;
  };
  let kind;
  const parameters = [];
  if (type === 'offset') {
    const name = parameter('distance');
    parameters.push(lengthParameter(name, finite(values.distance, 'The distance')));
    kind = { offset_plane: { base: baseOf(values.base, planes).base, distance: { parameter: name } } };
  } else if (type === 'angle') {
    const { base, frame } = baseOf(values.base, planes);
    const name = parameter('angle');
    parameters.push(angleParameter(name, finite(values.angle, 'The angle') * DEGREE));
    const hinge = values.hinge === 'y' ? frame.y_axis : frame.x_axis;
    kind = { angle_plane: { base, hinge: { literal: scalars(hinge) }, angle_radians: { parameter: name } } };
  } else if (type === 'face') {
    const support = face(0);
    const name = parameter('distance');
    parameters.push(lengthParameter(name, finite(values.distance, 'The distance')));
    kind = { face_plane: { input: support.input, face: support.face, distance: { parameter: name } } };
  } else if (type === 'midplane') {
    const [first, second] = [face(0), face(1)];
    if (first.input !== second.input) throw new Error('Both faces must come from the same solid.');
    kind = { midplane: { input: first.input, first: first.face, second: second.face } };
  } else if (type === 'three_point') {
    const points = values.points ?? [];
    if (points.length !== 3 || points.some((p) => !p || p.some((v) => !Number.isFinite(v)))) {
      throw new Error('Give three points.');
    }
    const [first, second, third] = points.map((point, i) => {
      const name = parameter(`p${i + 1}`);
      parameters.push(pointParameter(name, point));
      return { parameter: name };
    });
    kind = { three_point_plane: { first, second, third } };
  } else if (type === 'path') {
    if (!features.some((f) => f.id === values.path && isPathFeature(f))) throw new Error('Choose a sweep path.');
    const position = finite(values.position, 'The position');
    if (position < 0 || position > 1) throw new Error('The position is a fraction of the path from 0 to 1.');
    const name = parameter('position');
    parameters.push(fractionParameter(name, position));
    kind = { path_plane: { input: values.path, position: { parameter: name } } };
  } else {
    throw new Error('Choose a plane type.');
  }
  return { datum: { id, kind }, parameters };
}

/** The type id of a datum kind, or null for points, axes and plain planes. */
export function planeType(kind) {
  const key = Object.keys(kind ?? {})[0];
  return { offset_plane: 'offset', angle_plane: 'angle', face_plane: 'face', midplane: 'midplane', three_point_plane: 'three_point', path_plane: 'path' }[key] ?? (key === 'plane' ? 'fixed' : null);
}

/**
 * The parameters that drive a datum's kind, in order, with how to show
 * them: `{id, label, unit, toView, fromView}`. Angles show in degrees.
 */
export function drivingParameters(kind) {
  const body = Object.values(kind ?? {})[0] ?? {};
  const driven = [];
  const named = (expression) => expression?.parameter ?? null;
  if (named(body.distance)) driven.push({ id: named(body.distance), label: 'Distance', unit: 'mm' });
  if (named(body.angle_radians)) {
    driven.push({ id: named(body.angle_radians), label: 'Angle', unit: '°', toView: (r) => r / DEGREE, fromView: (d) => d * DEGREE });
  }
  if (named(body.position)) driven.push({ id: named(body.position), label: 'Position along path', unit: 'fraction' });
  for (const [key, label] of [['first', 'Point 1'], ['second', 'Point 2'], ['third', 'Point 3']]) {
    if (named(body[key])) driven.push({ id: named(body[key]), label, unit: 'mm', vector: true });
  }
  return driven;
}

/** A one-line description of how a plane is defined. */
export function describePlane(definition) {
  const kind = definition?.kind ?? {};
  const body = Object.values(kind)[0] ?? {};
  const base = (b) => (typeof b === 'string' ? b.toUpperCase() : b?.datum ?? '?');
  switch (planeType(kind)) {
    case 'offset': return `Offset from ${base(body.base)}`;
    case 'angle': return `Angle from ${base(body.base)}`;
    case 'face': return `On a face of ${body.input}`;
    case 'midplane': return `Midplane of ${body.input}`;
    case 'three_point': return 'Through 3 points';
    case 'path': return `Normal to ${body.input}`;
    case 'fixed': return 'Fixed plane';
    default: return 'Datum';
  }
}

/**
 * Screen outline of a square `half` millimetres along the plane's axes
 * around `center` (model mm) dropped onto the plane, or around its origin,
 * as an SVG path; '' when a corner is behind the eye.
 */
export function planeOutline(plane, half, matrix, width, height, center = null) {
  const normal = cross(plane.x_axis, plane.y_axis);
  const middle = center
    ? sub(center, scale(normal, dot(sub(center, plane.origin_mm), normal)))
    : plane.origin_mm;
  const corners = [[-1, -1], [1, -1], [1, 1], [-1, 1]].map(([u, v]) =>
    project(matrix, toGltf(add(middle, add(scale(plane.x_axis, u * half), scale(plane.y_axis, v * half)))), width, height));
  if (corners.some((c) => !c)) return '';
  return `M${corners.map((c) => `${c[0].toFixed(1)} ${c[1].toFixed(1)}`).join('L')}Z`;
}

/**
 * The nearest point a renderer ray hits on the mesh (model millimetres),
 * snapped to the hit triangle's nearest corner within `snap` mm, or null.
 * O(triangles).
 */
export function pickPoint(triangles, { origin, direction }, snap = 0) {
  const o = fromGltf(origin);
  const d = directionFromGltf(direction);
  let best = null;
  for (const { points: [a, b, c] } of triangles) {
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
    if (t > 0 && (!best || t < best.t)) best = { t, corners: [a, b, c] };
  }
  if (!best) return null;
  const hit = add(o, scale(d, best.t));
  const corner = best.corners
    .map((p) => [p, Math.hypot(...sub(p, hit))])
    .sort((x, y) => x[1] - y[1])[0];
  return corner[1] <= snap ? [...corner[0]] : hit;
}
