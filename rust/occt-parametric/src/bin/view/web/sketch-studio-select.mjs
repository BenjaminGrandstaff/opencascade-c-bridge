// Selection in the sketch studio: what is under the cursor, how solved
// each piece of geometry is, which constraints a selection can take, and
// what a dimension would measure. Constraint kinds and their roles come
// from the classic editor's `constraintTools`.
import { constraintTools, geometryIds } from './constraint-controls.mjs';
import { pointPositions } from './sketch-studio-infer.mjs';
import { projectLocal } from './sketch-studio-plane.mjs';
import { simplify } from './sketch-studio-overlay.mjs';

const hypot = (a, b) => Math.hypot(a[0] - b[0], a[1] - b[1]);

/** Distance from p to segment ab in screen pixels. */
function segmentDistance(p, a, b) {
  const d = [b[0] - a[0], b[1] - a[1]];
  const length = d[0] * d[0] + d[1] * d[1];
  const t = length ? Math.max(0, Math.min(1, ((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / length)) : 0;
  return hypot(p, [a[0] + d[0] * t, a[1] + d[1] * t]);
}

/**
 * The point or curve nearest `at` (screen pixels) within `radius` pixels;
 * points win within the radius. O(points + drawn samples).
 */
export function hitTest({ scene, frame, matrix, width, height, at, radius = 6 }) {
  let best = null;
  for (const [id, point] of Object.entries(scene?.points ?? {})) {
    const screen = projectLocal(matrix, frame, point, width, height);
    if (!screen) continue;
    const d = hypot(at, screen);
    if (d <= radius && (!best || d < best.distance)) best = { kind: 'point', id, distance: d };
  }
  if (best) return best;
  for (const entity of scene?.entities ?? []) {
    let previous = null;
    for (const point of simplify(entity)) {
      const screen = projectLocal(matrix, frame, point, width, height);
      if (screen && previous) {
        const d = segmentDistance(at, previous, screen);
        if (d <= radius && (!best || d < best.distance)) best = { kind: 'curve', id: entity.id, distance: d };
      }
      previous = screen;
    }
  }
  return best;
}

/**
 * How each point and curve of a solved scene is held: `conflict` when a
 * conflicting constraint touches it, `free` when a free motion moves it
 * (or one of its defining points), else `fixed`. O(annotations + points).
 */
export function solveStates(scene) {
  const diagnosis = scene?.solver?.diagnosis ?? {};
  const free = new Set();
  for (const motion of diagnosis.free_motions ?? []) {
    for (const p of motion.points ?? []) if (Math.abs(p.dx) + Math.abs(p.dy) > 1e-12) free.add(p.point);
  }
  for (const coordinate of diagnosis.free_coordinates ?? []) free.add(coordinate.point);
  const targets = new Map((scene?.annotations ?? []).map((a) => [a.id, a.targets ?? []]));
  const conflict = new Set();
  // Sources are tagged by `kind`; failed constraints are plain indexes.
  const conflicted = [
    ...(diagnosis.conflict_groups ?? []).flatMap((group) => group.sources ?? []),
    ...(diagnosis.failed_constraints ?? []).map((index) => ({ kind: 'constraint', index })),
  ];
  for (const source of conflicted) {
    if (source.kind === 'constraint') {
      for (const id of targets.get(`constraint-${source.index}`) ?? []) conflict.add(id);
    } else if (source.entity) {
      conflict.add(source.entity);
    } else if (source.point) {
      conflict.add(source.point);
    }
  }
  const state = (id, points = []) => {
    if (conflict.has(id) || points.some((p) => conflict.has(p))) return 'conflict';
    if (free.has(id) || points.some((p) => free.has(p))) return 'free';
    return 'fixed';
  };
  return {
    point: (id) => state(id),
    curve: (entity) => state(entity.id, entity.point_ids ?? []),
    free,
    conflict,
  };
}

const groupCache = new WeakMap();
/** For each geometry group a constraint role can take, the ids in it. */
function groups(definition) {
  let cached = groupCache.get(definition);
  if (!cached) {
    cached = new Map();
    for (const group of ['points', 'lines', 'curves', 'round', 'centered', 'arcs', 'splines', 'ellipses']) {
      cached.set(group, new Set(geometryIds(definition, group)));
    }
    groupCache.set(definition, cached);
  }
  return cached;
}

/** Each curve's end points, for the shared point a tangency needs. */
function endpoints(definition) {
  const ends = new Map();
  for (const line of definition.lines ?? []) ends.set(line.id, [line.start, line.end]);
  for (const arc of definition.arcs ?? []) ends.set(arc.id, [arc.start, arc.end]);
  for (const spline of definition.splines ?? []) ends.set(spline.id, [spline.points[0], spline.points.at(-1)]);
  for (const circle of definition.circles ?? []) ends.set(circle.id, [circle.rim]);
  return ends;
}

/** Most natural first: what one key press applies. */
const ORDER = [
  'coincident', 'horizontal', 'vertical', 'parallel', 'perpendicular', 'tangent', 'equal_length', 'equal_radius',
  'concentric', 'midpoint', 'point_on_curve', 'collinear', 'symmetric', 'coradial', 'line_circle_tangent',
  'circle_circle_tangent', 'curvature_continuous', 'equal_curve_length',
];
const rank = (type) => {
  const index = ORDER.indexOf(type);
  return index < 0 ? ORDER.length : index;
};

/** Ways to assign the selected ids to a constraint's geometry roles, in order. */
function assignments(fields, ids, members) {
  if (fields.length !== ids.length) return [];
  const out = [];
  const permute = (rest, chosen) => {
    if (!rest.length) {
      out.push(chosen);
      return;
    }
    const [key, group] = fields[chosen.length];
    for (let i = 0; i < rest.length; i += 1) {
      if (members.get(group)?.has(rest[i])) permute([...rest.slice(0, i), ...rest.slice(i + 1)], [...chosen, [key, rest[i]]]);
    }
  };
  permute(ids, []);
  return out;
}

/**
 * Constraints the selected ids can take, most natural first. Each is
 * `{type, label, fields, options, dimension}`; a dimension still needs its
 * value. Tangency at a shared point is offered for two curves sharing one
 * end. Kinds that need a picking flow (crossing points, spline samples,
 * weighted links) are not offered here. O(kinds × selection!).
 */
export function suggestions(definition, ids) {
  if (!ids.length || ids.length > 3 || new Set(ids).size !== ids.length) return [];
  const members = groups(definition);
  const ends = endpoints(definition);
  const out = [];
  for (const [type, tool] of Object.entries(constraintTools)) {
    let fieldsList = assignments(tool.fields, ids, members);
    if (!fieldsList.length && tool.fields.at(-1)?.[0] === 'point' && tool.fields.length === ids.length + 1) {
      // Two curves and the end they share.
      const [a, b] = ids;
      const shared = (ends.get(a) ?? []).filter((p) => (ends.get(b) ?? []).includes(p));
      if (shared.length === 1) fieldsList = assignments(tool.fields, [...ids, shared[0]], members);
    }
    // One assignment per kind: the first in selection order.
    const fields = fieldsList[0];
    if (!fields) continue;
    const options = Object.entries(tool.options ?? {});
    const variants = options.length
      ? options[0][1].map((choice) => ({ [options[0][0]]: choice }))
      : [{}];
    for (const option of variants) {
      const suffix = Object.values(option).join(' ');
      out.push({
        type,
        label: suffix ? `${tool.label} (${suffix})` : tool.label,
        fields: Object.fromEntries(fields),
        options: option,
        dimension: tool.dimension ?? null,
      });
    }
  }
  return out.sort((a, b) => Number(Boolean(a.dimension)) - Number(Boolean(b.dimension)) || rank(a.type) - rank(b.type));
}

/**
 * The dimension the Dimension tool adds for a selection, with its current
 * value: a line's length (distance between its ends), a circle's diameter,
 * an arc's radius, two points' distance, a point's signed distance to a
 * line, or two lines' signed angle in degrees. Null when none applies.
 */
export function dimensionFor(definition, scene, ids) {
  const positions = pointPositions(definition, scene?.points);
  const line = (id) => (definition.lines ?? []).find((l) => l.id === id);
  const circle = (id) => (definition.circles ?? []).find((c) => c.id === id);
  const arc = (id) => (definition.arcs ?? []).find((a) => a.id === id);
  const isPoint = (id) => positions.has(id);
  const at = (id) => positions.get(id);
  const signedAngle = (l1, l2) => {
    const [a, b, c, d] = [at(l1.start), at(l1.end), at(l2.start), at(l2.end)];
    const u = Math.atan2(b[1] - a[1], b[0] - a[0]);
    const v = Math.atan2(d[1] - c[1], d[0] - c[0]);
    return (Math.atan2(Math.sin(v - u), Math.cos(v - u)) * 180) / Math.PI;
  };
  if (ids.length === 1) {
    const [id] = ids;
    if (line(id)) {
      const l = line(id);
      if (!at(l.start) || !at(l.end)) return null;
      return { type: 'distance', fields: { first: l.start, second: l.end }, value: hypot(at(l.start), at(l.end)), unit: 'mm', label: `Length of ${id}` };
    }
    if (circle(id)) {
      const c = circle(id);
      if (!at(c.center) || !at(c.rim)) return null;
      return { type: 'diameter', fields: { curve: id }, value: 2 * hypot(at(c.center), at(c.rim)), unit: 'mm', label: `Diameter of ${id}` };
    }
    if (arc(id)) {
      const a = arc(id);
      if (!at(a.center) || !at(a.start)) return null;
      return { type: 'radius', fields: { curve: id }, value: hypot(at(a.center), at(a.start)), unit: 'mm', label: `Radius of ${id}` };
    }
    return null;
  }
  if (ids.length !== 2) return null;
  const [p, q] = ids;
  if (isPoint(p) && isPoint(q)) {
    return { type: 'distance', fields: { first: p, second: q }, value: hypot(at(p), at(q)), unit: 'mm', label: `Distance ${p}–${q}` };
  }
  const pointLine = isPoint(p) && line(q) ? [p, line(q)] : isPoint(q) && line(p) ? [q, line(p)] : null;
  if (pointLine) {
    const [point, l] = pointLine;
    const [a, b, x] = [at(l.start), at(l.end), at(point)];
    const length = hypot(a, b);
    if (!a || !b || !length) return null;
    const signed = ((b[0] - a[0]) * (x[1] - a[1]) - (b[1] - a[1]) * (x[0] - a[0])) / length;
    return { type: 'point_line_distance', fields: { point, line: l.id }, value: signed, unit: 'mm', label: `Distance ${point} to ${l.id}` };
  }
  if (line(p) && line(q)) {
    const [l1, l2] = [line(p), line(q)];
    if (![l1.start, l1.end, l2.start, l2.end].every(at)) return null;
    return { type: 'angle', fields: { first: p, second: q }, value: signedAngle(l1, l2), unit: '°', label: `Angle ${p} to ${q}` };
  }
  return null;
}

/** A literal or parameter expression for a typed dimension value. */
export function dimensionExpression(text, unit) {
  const trimmed = text.trim();
  if (/^[A-Za-z_][\w.-]*$/.test(trimmed)) return { parameter: trimmed };
  const value = Number(trimmed);
  if (!trimmed || !Number.isFinite(value)) return null;
  return unit === '°'
    ? { literal: { value: (value * Math.PI) / 180, dimension: 'scalar', unit: null } }
    : { literal: { value, dimension: 'length', unit: 'millimeter' } };
}

/** Indexes of draft constraints that mention any of the ids. */
export function constraintsTouching(definition, ids) {
  const wanted = new Set(ids);
  return (definition.constraints ?? []).flatMap((constraint, index) => {
    const body = Object.values(constraint)[0] ?? {};
    return Object.values(body).some((value) => typeof value === 'string' && wanted.has(value)) ? [index] : [];
  });
}
