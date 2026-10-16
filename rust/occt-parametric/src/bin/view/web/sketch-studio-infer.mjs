// Snapping and automatic constraints while drawing in the sketch studio.
// Everything is in sketch-local millimetres; `tolerance` is the snap radius
// in millimetres (a few pixels at the current zoom).

/** Lines within this angle of horizontal, vertical, parallel or perpendicular snap to it. */
export const ANGLE_TOLERANCE = (3 * Math.PI) / 180;
/** Lengths and radii within this fraction of a hovered one snap equal to it. */
export const EQUAL_TOLERANCE = 0.01;

const distance = (a, b) => Math.hypot(a[0] - b[0], a[1] - b[1]);
const literal = (point) =>
  point?.x?.literal?.unit === 'millimeter' && point?.y?.literal?.unit === 'millimeter'
    ? [point.x.literal.value, point.y.literal.value]
    : null;

/**
 * Current positions of a sketch's points: solved positions from its scene
 * where it has them, else the draft's literal coordinates. O(points).
 */
export function pointPositions(definition, scenePoints = {}) {
  const positions = new Map();
  for (const [id, xy] of Object.entries(scenePoints)) {
    if (Array.isArray(xy) && Number.isFinite(xy[0]) && Number.isFinite(xy[1])) positions.set(id, [xy[0], xy[1]]);
  }
  for (const point of definition?.points ?? []) {
    if (!positions.has(point.id)) {
      const xy = literal(point);
      if (xy) positions.set(point.id, xy);
    }
  }
  return positions;
}

/** The draft's lines with their endpoint positions, skipping unresolved ones. */
export function lineSegments(definition, positions) {
  return (definition?.lines ?? []).flatMap((line) => {
    const a = positions.get(line.start);
    const b = positions.get(line.end);
    return a && b && distance(a, b) > 1e-12 ? [{ id: line.id, start: line.start, end: line.end, a, b }] : [];
  });
}

/** Circles and arcs with their centre and radius, skipping unresolved ones. */
export function roundCurves(definition, positions) {
  const out = [];
  for (const circle of definition?.circles ?? []) {
    const c = positions.get(circle.center);
    const r = positions.get(circle.rim);
    if (c && r) out.push({ id: circle.id, kind: 'circle', center: c, radius: distance(c, r) });
  }
  for (const arc of definition?.arcs ?? []) {
    const c = positions.get(arc.center);
    const s = positions.get(arc.start);
    if (c && s) out.push({ id: arc.id, kind: 'arc', center: c, radius: distance(c, s), start: arc.start, end: arc.end, arcCenter: arc.center });
  }
  return out;
}

/** Cells a grid query may visit before it falls back to a full scan. */
const MAX_QUERY_CELLS = 4096;
/** Cells one segment may occupy; longer ones are checked on every query. */
const MAX_SEGMENT_CELLS = 256;

/**
 * A uniform grid over a sketch's points, line segments and sampled curves,
 * built in O(items) and cached per draft definition and scene, so a query
 * near the cursor visits only nearby cells.
 */
class SpatialIndex {
  constructor(definition, scene) {
    this.positions = pointPositions(definition, scene.points);
    this.lines = lineSegments(definition, this.positions);
    this.rounds = roundCurves(definition, this.positions);
    this.samples = [];
    for (const entity of scene.entities ?? []) {
      const points = entity.points ?? [];
      for (let i = 1; i < points.length; i += 1) this.samples.push({ curve: entity.id, a: points[i - 1], b: points[i] });
    }
    const min = [Infinity, Infinity];
    const max = [-Infinity, -Infinity];
    const grow = (p) => {
      min[0] = Math.min(min[0], p[0]); min[1] = Math.min(min[1], p[1]);
      max[0] = Math.max(max[0], p[0]); max[1] = Math.max(max[1], p[1]);
    };
    this.positions.forEach(grow);
    this.samples.forEach((s) => { grow(s.a); grow(s.b); });
    const count = this.positions.size + this.samples.length + this.lines.length;
    const extent = Math.max(max[0] - min[0], max[1] - min[1], 1e-9);
    this.cell = Number.isFinite(extent) ? extent / Math.max(1, Math.sqrt(count)) : 1;
    this.cells = new Map();
    this.wide = { points: [], samples: [], lines: [] };
    for (const [id, xy] of this.positions) this.add('points', { id, xy }, xy, xy);
    for (const sample of this.samples) this.add('samples', sample, sample.a, sample.b);
    for (const line of this.lines) this.add('lines', line, line.a, line.b);
  }

  key(ix, iy) {
    return `${ix},${iy}`;
  }

  add(kind, item, a, b) {
    const [x0, x1] = [Math.floor(Math.min(a[0], b[0]) / this.cell), Math.floor(Math.max(a[0], b[0]) / this.cell)];
    const [y0, y1] = [Math.floor(Math.min(a[1], b[1]) / this.cell), Math.floor(Math.max(a[1], b[1]) / this.cell)];
    if ((x1 - x0 + 1) * (y1 - y0 + 1) > MAX_SEGMENT_CELLS) {
      this.wide[kind].push(item);
      return;
    }
    for (let ix = x0; ix <= x1; ix += 1) {
      for (let iy = y0; iy <= y1; iy += 1) {
        const key = this.key(ix, iy);
        let cell = this.cells.get(key);
        if (!cell) this.cells.set(key, (cell = { points: [], samples: [], lines: [] }));
        cell[kind].push(item);
      }
    }
  }

  /** Items of `kind` whose cells meet the square of half-width r around p. */
  near(kind, p, r) {
    const [x0, x1] = [Math.floor((p[0] - r) / this.cell), Math.floor((p[0] + r) / this.cell)];
    const [y0, y1] = [Math.floor((p[1] - r) / this.cell), Math.floor((p[1] + r) / this.cell)];
    if ((x1 - x0 + 1) * (y1 - y0 + 1) > MAX_QUERY_CELLS) {
      return kind === 'points' ? [...this.positions].map(([id, xy]) => ({ id, xy })) : this[kind];
    }
    const out = new Set(this.wide[kind]);
    for (let ix = x0; ix <= x1; ix += 1) {
      for (let iy = y0; iy <= y1; iy += 1) {
        for (const item of this.cells.get(this.key(ix, iy))?.[kind] ?? []) out.add(item);
      }
    }
    return out;
  }
}

const indexes = new WeakMap();
/** The cached spatial index of a draft definition and its scene. */
export function spatialIndex(definition, scene = {}) {
  let byScene = indexes.get(definition);
  if (!byScene) indexes.set(definition, (byScene = new WeakMap()));
  let index = byScene.get(scene);
  if (!index) byScene.set(scene, (index = new SpatialIndex(definition, scene)));
  return index;
}

/** The nearest point on segment ab to p and its distance. */
function nearestOnSegment(p, a, b) {
  const d = [b[0] - a[0], b[1] - a[1]];
  const t = Math.max(0, Math.min(1, ((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / (d[0] * d[0] + d[1] * d[1])));
  const q = [a[0] + d[0] * t, a[1] + d[1] * t];
  return { xy: q, distance: distance(p, q) };
}

const angleOf = (a, b) => Math.atan2(b[1] - a[1], b[0] - a[0]);
/** Smallest difference between two line directions (modulo π). */
function directionGap(a, b) {
  let gap = Math.abs(a - b) % Math.PI;
  if (gap > Math.PI / 2) gap = Math.PI - gap;
  return gap;
}

/**
 * Where a drawing click lands and what it implies.
 *
 * - `xy`: the snapped sketch-local position.
 * - `point`: an existing point to reuse (which makes them coincident), or null.
 * - `rules`: constraints to add once the shape exists. Point rules
 *   (`on_curve`, `midpoint`) apply to this click's point; shape rules
 *   (`horizontal`, `vertical`, `parallel`, `perpendicular`, `equal_length`,
 *   `equal_radius`, `tangent`) apply to the shape the click completes.
 * - `hint`: a short description of the snap for the status bar.
 * - `hovered`: the line or round curve under the cursor, remembered by the
 *   caller as a reference for parallel, perpendicular and equal snaps.
 *
 * `anchor` is the previous click of a line or circle gesture ({xy, point}),
 * `reference` the last hovered curve, and `auto` false disables inference
 * (snapping to points still applies). O(points + curve samples).
 */
export function infer({
  xy, tool, definition, scene = {}, tolerance, anchor = null, reference = null, auto = true, exclude = [],
}) {
  const index = spatialIndex(definition, scene);
  const skip = new Set(exclude);
  const reach = Math.max(tolerance, 0);
  let result = { xy: [...xy], point: null, rules: [], hint: '', hovered: null };

  // What is under the cursor, for later parallel and equal references.
  let hoveredBest = Infinity;
  for (const line of index.near('lines', xy, reach)) {
    if (skip.has(line.id)) continue;
    const hit = nearestOnSegment(xy, line.a, line.b);
    if (hit.distance <= tolerance && hit.distance < hoveredBest) {
      hoveredBest = hit.distance;
      result.hovered = { kind: 'line', id: line.id, angle: angleOf(line.a, line.b), length: distance(line.a, line.b) };
    }
  }
  for (const round of index.rounds) {
    if (skip.has(round.id)) continue;
    const gap = Math.abs(distance(xy, round.center) - round.radius);
    if (gap <= tolerance && gap < hoveredBest) {
      hoveredBest = gap;
      result.hovered = { kind: 'round', id: round.id, radius: round.radius };
    }
  }

  // 1. Existing points win.
  let pointBest = null;
  for (const { id, xy: position } of index.near('points', xy, reach)) {
    if (skip.has(id)) continue;
    const d = distance(xy, position);
    if (d <= tolerance && (!pointBest || d < pointBest.d)) pointBest = { id, position, d };
  }
  if (pointBest) {
    result = { ...result, xy: [...pointBest.position], point: pointBest.id, hint: `on point ${pointBest.id}` };
  } else if (auto) {
    // 2. Line midpoints, then 3. any curve.
    let mid = null;
    for (const line of index.near('lines', xy, reach)) {
      if (skip.has(line.id)) continue;
      const m = [(line.a[0] + line.b[0]) / 2, (line.a[1] + line.b[1]) / 2];
      const d = distance(xy, m);
      if (d <= tolerance && (!mid || d < mid.d)) mid = { line: line.id, xy: m, d };
    }
    if (mid) {
      result = { ...result, xy: mid.xy, rules: [{ kind: 'midpoint', line: mid.line }], hint: `midpoint of ${mid.line}` };
    } else {
      let on = null;
      for (const sample of index.near('samples', xy, reach)) {
        if (skip.has(sample.curve)) continue;
        const hit = nearestOnSegment(xy, sample.a, sample.b);
        if (hit.distance <= tolerance && (!on || hit.distance < on.distance)) on = { ...hit, curve: sample.curve };
      }
      if (on) result = { ...result, xy: on.xy, rules: [{ kind: 'on_curve', curve: on.curve }], hint: `on ${on.curve}` };
    }
  }
  if (!auto || !anchor) return result;

  // Shape rules from the gesture's previous click.
  const a = anchor.xy;
  const span = distance(a, result.xy);
  if (span <= 1e-9) return result;
  // A snapped end cannot move, so its rules must already hold exactly; a
  // free end moves onto them.
  const free = !result.point && !result.rules.length;
  const angleLimit = free ? ANGLE_TOLERANCE : 1e-9;
  const sizeLimit = free ? EQUAL_TOLERANCE : 1e-9;
  if (tool === 'line') {
    const angle = angleOf(a, result.xy);
    const along = (direction, length = span) => [a[0] + Math.cos(direction) * length, a[1] + Math.sin(direction) * length];
    let direction = null;
    if (directionGap(angle, 0) <= angleLimit) {
      direction = Math.abs(angle) < Math.PI / 2 ? 0 : Math.PI;
      result.rules.push({ kind: 'horizontal' });
      result.hint = joinHint(result.hint, 'horizontal');
    } else if (directionGap(angle, Math.PI / 2) <= angleLimit) {
      direction = angle > 0 ? Math.PI / 2 : -Math.PI / 2;
      result.rules.push({ kind: 'vertical' });
      result.hint = joinHint(result.hint, 'vertical');
    } else if (reference?.kind === 'line' && !skip.has(reference.id)) {
      if (directionGap(angle, reference.angle) <= angleLimit) {
        direction = nearestDirection(angle, reference.angle);
        result.rules.push({ kind: 'parallel', line: reference.id });
        result.hint = joinHint(result.hint, `parallel to ${reference.id}`);
      } else if (directionGap(angle, reference.angle + Math.PI / 2) <= angleLimit) {
        direction = nearestDirection(angle, reference.angle + Math.PI / 2);
        result.rules.push({ kind: 'perpendicular', line: reference.id });
        result.hint = joinHint(result.hint, `perpendicular to ${reference.id}`);
      }
    }
    let length = span;
    if (reference?.kind === 'line' && !skip.has(reference.id) && Math.abs(span - reference.length) <= sizeLimit * reference.length) {
      length = reference.length;
      result.rules.push({ kind: 'equal_length', line: reference.id });
      result.hint = joinHint(result.hint, `equal to ${reference.id}`);
    }
    // Only a free end moves to honour the rules; a snapped end keeps its
    // place and the solver reconciles the small difference.
    if (free && (direction !== null || length !== span)) result.xy = along(direction ?? angle, length);
  } else if ((tool === 'circle' || tool === 'arc') && reference?.kind === 'round' && !skip.has(reference.id)) {
    if (Math.abs(span - reference.radius) <= sizeLimit * reference.radius) {
      if (free) {
        const angle = angleOf(a, result.xy);
        result.xy = [a[0] + Math.cos(angle) * reference.radius, a[1] + Math.sin(angle) * reference.radius];
      }
      result.rules.push({ kind: 'equal_radius', curve: reference.id });
      result.hint = joinHint(result.hint, `equal radius to ${reference.id}`);
    }
  }
  return result;
}

function nearestDirection(angle, base) {
  // base or base + π, whichever is closer to angle.
  const options = [base, base + Math.PI, base - Math.PI];
  return options.reduce((best, option) =>
    Math.abs(Math.atan2(Math.sin(angle - option), Math.cos(angle - option))) <
    Math.abs(Math.atan2(Math.sin(angle - best), Math.cos(angle - best))) ? option : best);
}

const joinHint = (hint, text) => (hint ? `${hint}, ${text}` : text);

/**
 * An arc starting at the end of a line is tangent to it when its centre
 * lies on the line's normal there (within ANGLE_TOLERANCE). Returns the
 * line id or null.
 */
export function tangentLineAt(definition, scene, startPoint, center) {
  if (!startPoint) return null;
  const positions = pointPositions(definition, scene?.points);
  const start = positions.get(startPoint);
  if (!start) return null;
  for (const line of lineSegments(definition, positions)) {
    if (line.start !== startPoint && line.end !== startPoint) continue;
    const along = angleOf(line.a, line.b);
    const toCenter = angleOf(start, center);
    if (directionGap(toCenter, along + Math.PI / 2) <= ANGLE_TOLERANCE) return line.id;
  }
  return null;
}

/**
 * Constraints for the rules gathered while drawing a shape. `clicks` are the
 * gesture's clicks with their `rules` and the point ids they became;
 * `shape` is the new boundary entity (line, circle or arc id). Returns the
 * constraint JSON objects in the core schema.
 */
export function rulesToConstraints(clicks, shape) {
  const out = [];
  for (const click of clicks) {
    for (const rule of click.rules ?? []) {
      if (rule.kind === 'on_curve') out.push({ point_on_curve: { point: click.id, curve: rule.curve } });
      else if (rule.kind === 'midpoint') out.push({ midpoint: { point: click.id, line: rule.line } });
      else if (!shape) continue;
      else if (rule.kind === 'horizontal') out.push({ horizontal: { line: shape } });
      else if (rule.kind === 'vertical') out.push({ vertical: { line: shape } });
      else if (rule.kind === 'parallel') out.push({ parallel: { first: rule.line, second: shape } });
      else if (rule.kind === 'perpendicular') out.push({ perpendicular: { first: rule.line, second: shape } });
      else if (rule.kind === 'equal_length') out.push({ equal_length: { first: rule.line, second: shape } });
      else if (rule.kind === 'equal_radius') out.push({ equal_radius: { first: rule.curve, second: shape } });
      else if (rule.kind === 'tangent') out.push({ tangent: { first: rule.line, second: shape, point: click.id } });
    }
  }
  return out;
}
