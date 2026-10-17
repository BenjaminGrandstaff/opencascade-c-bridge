// Trim and Split in the sketch studio. Both edit the authored geometry, as
// other sketchers do: a trimmed curve gets a new end at the crossing, held
// on the crossing curve by a point-on-curve constraint (or sharing a point
// already there), so the result stays parametric and its corners are named
// points that Fillet and Chamfer can use. Trimming the middle of a line or
// arc leaves two curves kept collinear or on one radius; trimming a circle
// leaves an arc with the circle's name; a curve no other curve crosses is
// deleted. Split cuts a line or arc in two at a new shared point.
// Constraints that measured the edited curve's old extent (its midpoint,
// length or sweep) are dropped; constraints on its direction stay.
import { nextId } from './canvas-tools.mjs';
import { continueOutline, deletion } from './sketch-studio-delete.mjs';
import { simplify } from './sketch-studio-overlay.mjs';
import { hitTest } from './sketch-studio-select.mjs';

const sub = (a, b) => [a[0] - b[0], a[1] - b[1]];
const cross = (a, b) => a[0] * b[1] - a[1] * b[0];
const dist = (a, b) => Math.hypot(a[0] - b[0], a[1] - b[1]);
const literal = (value) => ({ literal: { value, dimension: 'length', unit: 'millimeter' } });

/** Kind of authored curve `id`: 'line', 'arc', 'circle', or null. */
function kindOf(definition, id) {
  if ((definition.lines ?? []).some((l) => l.id === id)) return 'line';
  if ((definition.arcs ?? []).some((a) => a.id === id)) return 'arc';
  if ((definition.circles ?? []).some((c) => c.id === id)) return 'circle';
  return null;
}

/** Cumulative lengths along a polyline. */
function lengths(points) {
  const result = [0];
  for (let i = 1; i < points.length; i += 1) result.push(result[i - 1] + dist(points[i - 1], points[i]));
  return result;
}

/** The point at distance `s` along a polyline. */
function pointAt(points, cumulative, s) {
  let i = 1;
  while (i < points.length - 1 && cumulative[i] < s) i += 1;
  const span = cumulative[i] - cumulative[i - 1] || 1;
  const t = Math.max(0, Math.min(1, (s - cumulative[i - 1]) / span));
  return [points[i - 1][0] + t * (points[i][0] - points[i - 1][0]), points[i - 1][1] + t * (points[i][1] - points[i - 1][1])];
}

/** Distance along a polyline of its point nearest `at`, and that point. */
function nearest(points, cumulative, at) {
  let best = { s: 0, point: points[0], d: Infinity };
  for (let i = 1; i < points.length; i += 1) {
    const [a, b] = [points[i - 1], points[i]];
    const ab = sub(b, a);
    const t = Math.max(0, Math.min(1, ((at[0] - a[0]) * ab[0] + (at[1] - a[1]) * ab[1]) / ((ab[0] ** 2 + ab[1] ** 2) || 1)));
    const p = [a[0] + t * ab[0], a[1] + t * ab[1]];
    const d = dist(p, at);
    if (d < best.d) best = { s: cumulative[i - 1] + t * (cumulative[i] - cumulative[i - 1]), point: p, d };
  }
  return best;
}

/** The part of a polyline between distances `from` and `to` (wrapping when closed and from > to). */
function slice(points, cumulative, from, to) {
  const total = cumulative.at(-1);
  const run = (a, b) => [pointAt(points, cumulative, a), ...points.filter((_, i) => cumulative[i] > a && cumulative[i] < b), pointAt(points, cumulative, b)];
  return from <= to ? run(from, to) : [...run(from, total), ...run(0, to).slice(1)];
}

/**
 * Where other curves cross scene curve `id`: `{s, at, other, point?}`
 * sorted by the distance `s` along it, without crossings at its own ends
 * (touching neighbours) and with repeats merged. Curve ends and points
 * held on it (point-on-curve constraints in `definition`) count as
 * crossings at that `point`. O(its segments × all segments).
 */
export function crossings(scene, id, definition = null) {
  const entity = (scene?.entities ?? []).find((e) => e.id === id);
  if (!entity) return [];
  const points = entity.points;
  const cumulative = lengths(points);
  const total = cumulative.at(-1);
  const closed = dist(points[0], points.at(-1)) <= 1e-9 * Math.max(1, total);
  const eps = 1e-7 * Math.max(1, total);
  const found = [];
  for (const other of scene.entities) {
    if (other.id === id || other.id.startsWith('edited:')) continue;
    const others = simplify(other);
    for (let i = 1; i < points.length; i += 1) {
      const [p, q] = [points[i - 1], points[i]];
      const d = sub(q, p);
      for (let j = 1; j < others.length; j += 1) {
        const [a, b] = [others[j - 1], others[j]];
        const e = sub(b, a);
        const denominator = cross(d, e);
        if (Math.abs(denominator) < 1e-15) continue;
        const t = cross(sub(a, p), e) / denominator;
        const u = cross(sub(a, p), d) / denominator;
        if (t < -1e-12 || t > 1 + 1e-12 || u < -1e-12 || u > 1 + 1e-12) continue;
        const s = cumulative[i - 1] + t * (cumulative[i] - cumulative[i - 1]);
        if (!closed && (s <= eps || s >= total - eps)) continue;
        found.push({ s, at: [p[0] + t * d[0], p[1] + t * d[1]], other: other.id });
      }
    }
  }
  // Curves ending on this one (a trimmed edge on a circle) may stop short
  // of its sampled chords, so their end points count when they lie within
  // the chords' sag from the true curve; so do points held on it. The sag
  // of a chord is about its length times the turn between chords over 8.
  let sag = 0;
  for (let i = 1; i < points.length - 1; i += 1) {
    const [u, v] = [sub(points[i], points[i - 1]), sub(points[i + 1], points[i])];
    const turn = Math.abs(Math.atan2(cross(u, v), u[0] * v[0] + u[1] * v[1]));
    sag = Math.max(sag, (Math.max(Math.hypot(...u), Math.hypot(...v)) * turn) / 8);
  }
  const near = Math.max(eps, 2 * sag);
  const box = points.reduce((b, p) => [Math.min(b[0], p[0]), Math.min(b[1], p[1]), Math.max(b[2], p[0]), Math.max(b[3], p[1])], [Infinity, Infinity, -Infinity, -Infinity]);
  const inBox = (p) => p[0] >= box[0] - near && p[1] >= box[1] - near && p[0] <= box[2] + near && p[1] <= box[3] + near;
  const held = new Set((definition?.constraints ?? []).filter((c) => c.point_on_curve?.curve === id).map((c) => c.point_on_curve.point));
  const own = new Set(entity.point_ids ?? []);
  const on = (other) => (definition ? pointsOn(definition, scene, other.id) : []);
  for (const other of scene.entities) {
    if (other.id === id || other.id.startsWith('edited:')) continue;
    for (const pid of on(other)) {
      const at = scene.points?.[pid];
      if (!at || own.has(pid) || !inBox(at)) continue;
      const hit = nearest(points, cumulative, at);
      if (hit.d > near || (!closed && (hit.s <= eps || hit.s >= total - eps))) continue;
      found.push({ s: hit.s, at: [at[0], at[1]], other: other.id, point: pid });
    }
  }
  for (const pid of held) {
    const at = scene.points?.[pid];
    if (!at || own.has(pid)) continue;
    const hit = nearest(points, cumulative, at);
    if (hit.d > near || (!closed && (hit.s <= eps || hit.s >= total - eps))) continue;
    const owner = scene.entities.find((e) => e.id !== id && !e.id.startsWith('edited:') && on(e).includes(pid));
    if (owner) found.push({ s: hit.s, at: [at[0], at[1]], other: owner.id, point: pid });
  }
  found.sort((x, y) => x.s - y.s || (y.point ? 1 : 0) - (x.point ? 1 : 0));
  // Merge repeats, keeping a crossing that names a point.
  const merged = [];
  for (const c of found) {
    const last = merged.at(-1);
    if (last && c.s - last.s <= Math.max(eps, near)) {
      if (!last.point && c.point) merged[merged.length - 1] = c;
    } else {
      merged.push(c);
    }
  }
  if (closed && merged.length > 1 && total - merged.at(-1).s + merged[0].s <= Math.max(eps, near)) {
    const last = merged.pop();
    if (!merged[0].point && last.point) merged[0] = last;
  }
  return merged;
}

const cache = new WeakMap();
/** `crossings`, kept per scene, curve and definition so hovering one curve computes them once. */
function cachedCrossings(scene, id, definition) {
  let byCurve = cache.get(scene);
  if (!byCurve) cache.set(scene, (byCurve = new Map()));
  const hit = byCurve.get(id);
  if (hit?.definition === definition) return hit.cuts;
  const cuts = crossings(scene, id, definition);
  byCurve.set(id, { definition, cuts });
  return cuts;
}

/**
 * The piece of curve `id` a click at sketch point `at` would trim: `{kind,
 * from, to, path}` where kind is 'whole', 'start' (from the curve's start
 * to its first crossing), 'end', 'middle' or 'span' (a circle between two
 * crossings), `from`/`to` are the bounding crossings and `path` the piece's
 * points; or `{unsupported}` for curves Trim cannot change.
 */
export function trimPiece(definition, scene, id, at) {
  const kind = kindOf(definition, id);
  if (!kind) return { unsupported: `${id} is not a line, arc or circle of this sketch; Trim changes those (Delete removes others).` };
  const entity = scene.entities.find((e) => e.id === id);
  const points = entity.points;
  const cumulative = lengths(points);
  const { s } = nearest(points, cumulative, at);
  const cuts = cachedCrossings(scene, id, definition);
  if (kind === 'circle' ? cuts.length < 2 : !cuts.length) return { kind: 'whole', path: points };
  if (kind === 'circle') {
    const from = [...cuts].reverse().find((c) => c.s < s) ?? cuts.at(-1);
    const to = cuts.find((c) => c.s > s) ?? cuts[0];
    return { kind: 'span', from, to, path: slice(points, cumulative, from.s, to.s) };
  }
  const from = [...cuts].reverse().find((c) => c.s < s) ?? null;
  const to = cuts.find((c) => c.s > s) ?? null;
  const total = cumulative.at(-1);
  const piece = !from ? 'start' : !to ? 'end' : 'middle';
  return { kind: piece, from, to, path: slice(points, cumulative, from?.s ?? 0, to?.s ?? total) };
}

/** Points a sketch's geometry (not its constraints) uses. */
function geometryPoints(definition) {
  const used = new Set();
  const add = (id) => typeof id === 'string' && used.add(id);
  for (const l of definition.lines ?? []) [l.start, l.end].forEach(add);
  for (const a of definition.arcs ?? []) [a.center, a.start, a.end].forEach(add);
  for (const c of definition.circles ?? []) [c.center, c.rim].forEach(add);
  for (const e of definition.ellipses ?? []) [e.center, e.major, e.minor, e.start, e.end].forEach(add);
  for (const s of definition.splines ?? []) (s.points ?? []).forEach(add);
  for (const record of [...(definition.shapes ?? []), ...(definition.texts ?? []), ...(definition.patterns ?? [])]) {
    const stack = [record];
    while (stack.length) {
      const value = stack.pop();
      if (typeof value === 'string') add(value);
      else if (value && typeof value === 'object') stack.push(...Object.values(value));
    }
  }
  return used;
}

/** End point ids of authored lines and arcs. */
function endsOf(definition, id) {
  const record = [...(definition.lines ?? []), ...(definition.arcs ?? [])].find((r) => r.id === id);
  return record ? [record.start, record.end] : null;
}

/**
 * Tangent and curvature constraints at a point that is no longer an end of
 * one of their curves: moved to `replacement` when that curve has it as an
 * end, else dropped.
 */
function retarget(definition, curve, replacement) {
  definition.constraints = (definition.constraints ?? []).flatMap((constraint) => {
    const [type, body] = Object.entries(constraint)[0] ?? [];
    if (!['tangent', 'curvature_continuous'].includes(type) || !body?.point) return [constraint];
    const fixed = { ...body };
    for (const field of ['first', 'second']) {
      if (fixed[field] !== curve || endsOf(definition, curve)?.includes(body.point)) continue;
      if (replacement && endsOf(definition, replacement)?.includes(body.point)) fixed[field] = replacement;
      else return [];
    }
    return [{ [type]: fixed }];
  });
}

/**
 * Without constraints that measure the extent of curve `id`, whose ends
 * moved: midpoints, curve lengths, equal lengths and arc sweeps. Returns
 * how many went. Directions (horizontal, parallel, collinear…) stay.
 */
function dropExtent(definition, id) {
  const before = (definition.constraints ?? []).length;
  definition.constraints = (definition.constraints ?? []).filter((constraint) => {
    const [type, body] = Object.entries(constraint)[0] ?? [];
    if (type === 'midpoint') return body.line !== id;
    if (type === 'curve_length') return body.curve !== id;
    if (type === 'arc_angle') return body.arc !== id;
    if (type === 'equal_curve_length') return body.first !== id && body.second !== id;
    return true;
  });
  return before - definition.constraints.length;
}

/** Without point-on-curve constraints on `curves` for points now on them by definition. */
function dropImplied(definition, scene, curves) {
  const on = new Map(curves.map((id) => [id, new Set(pointsOn(definition, scene, id))]));
  definition.constraints = (definition.constraints ?? []).filter((c) => !(c.point_on_curve && on.get(c.point_on_curve.curve)?.has(c.point_on_curve.point)));
}

/** Removes points in `candidates` that no geometry uses, with constraints on them. */
function dropUnused(definition, candidates) {
  const used = geometryPoints(definition);
  const unused = candidates.filter((id) => id && !used.has(id) && (definition.points ?? []).some((p) => p.id === id));
  return unused.length ? deletion(definition, unused).definition : definition;
}

/**
 * A point at crossing `crossing` of the curve being edited: an authored
 * point already there (within `tolerance`, one on the crossing curve by
 * definition first), else a new free point, held on the crossing curve
 * unless it lies on it by definition or is already held on it.
 */
function pointFor(definition, scene, crossing, tolerance) {
  const ends = pointsOn(definition, scene, crossing.other);
  // A point already there, preferring one that lies on the crossing curve by definition.
  const there = (definition.points ?? []).filter((p) => {
    const at = scene.points?.[p.id];
    return at && dist(at, crossing.at) <= tolerance;
  });
  const existing = there.find((p) => p.id === crossing.point) ?? there.find((p) => ends.includes(p.id)) ?? there[0];
  const id = existing?.id ?? nextId(definition, 'P');
  if (!existing) (definition.points ??= []).push({ id, x: literal(crossing.at[0]), y: literal(crossing.at[1]), fixed: false });
  const held = (definition.constraints ?? []).some((c) => c.point_on_curve?.point === id && c.point_on_curve?.curve === crossing.other);
  if (!ends.includes(id) && !held) (definition.constraints ??= []).push({ point_on_curve: { point: id, curve: crossing.other } });
  return id;
}

/**
 * Points that lie on curve `id` by definition: a line's or arc's ends, a
 * circle's rim, an ellipse's axis points, a spline's interpolated ends, or
 * a generated curve's scene ends.
 */
function pointsOn(definition, scene, id) {
  const find = (group) => (definition[group] ?? []).find((r) => r.id === id);
  const line = find('lines') ?? find('arcs');
  if (line) return [line.start, line.end];
  const circle = find('circles');
  if (circle) return [circle.rim];
  const ellipse = find('ellipses');
  if (ellipse) return [ellipse.major, ellipse.minor, ellipse.start, ellipse.end].filter(Boolean);
  const spline = find('splines');
  if (spline) return spline.basis ? [] : [spline.points[0], spline.points.at(-1)];
  return scene.entities.find((e) => e.id === id)?.point_ids ?? [];
}

/** `loop` with `added` placed right after `curve`, starting at `added` when they leave a gap. */
function insertAfter(loop, curve, added, gap) {
  const i = loop.indexOf(curve);
  if (i < 0) return loop;
  const next = [...loop.slice(0, i + 1), added, ...loop.slice(i + 1)];
  return gap ? [...next.slice(i + 1), ...next.slice(0, i + 1)] : next;
}

function eachLoop(definition, change) {
  if (Array.isArray(definition.profile)) definition.profile = change(definition.profile);
  if (Array.isArray(definition.holes)) definition.holes = definition.holes.map(change);
  for (const region of definition.regions ?? []) {
    region.profile = change(region.profile ?? []);
    region.holes = (region.holes ?? []).map(change);
  }
}

/** Sketch tolerance for reusing a point at a crossing. */
function toleranceOf(scene) {
  let size = 1;
  for (const p of Object.values(scene.points ?? {})) size = Math.max(size, Math.abs(p[0]), Math.abs(p[1]));
  return 1e-6 * size;
}

/**
 * `definition` with the piece of curve `id` under sketch point `at` trimmed
 * away (see `trimPiece`), as `{definition, summary}`. The input is not
 * changed. Throws for curves Trim cannot change.
 */
export function trimmed(definition, scene, id, at) {
  const piece = trimPiece(definition, scene, id, at);
  if (piece.unsupported) throw new Error(piece.unsupported);
  if (piece.kind === 'whole') {
    const result = deletion(definition, [id]);
    return { definition: result.definition, summary: `${id} crosses nothing, so Trim deleted it (${result.summary}).` };
  }
  let next = structuredClone(definition);
  const tolerance = toleranceOf(scene);
  const kind = kindOf(next, id);
  const orphans = [];
  let summary;
  if (kind === 'circle') {
    const circle = next.circles.find((c) => c.id === id);
    const points = scene.entities.find((e) => e.id === id).points;
    const center = scene.points[circle.center];
    const ccw = cross(sub(points[0], center), sub(points[1], center)) > 0;
    const start = pointFor(next, scene, piece.to, tolerance);
    const end = pointFor(next, scene, piece.from, tolerance);
    next.circles = next.circles.filter((c) => c !== circle);
    (next.arcs ??= []).push({ id, center: circle.center, start, end, clockwise: !ccw });
    orphans.push(circle.rim);
    summary = `${id} is now an arc from ${start} to ${end}.`;
  } else {
    const record = (kind === 'line' ? next.lines : next.arcs).find((r) => r.id === id);
    if (piece.kind === 'start') {
      orphans.push(record.start);
      record.start = pointFor(next, scene, piece.to, tolerance);
      summary = `${id} now starts at ${record.start}.`;
    } else if (piece.kind === 'end') {
      orphans.push(record.end);
      record.end = pointFor(next, scene, piece.from, tolerance);
      summary = `${id} now ends at ${record.end}.`;
    } else {
      const end = record.end;
      record.end = pointFor(next, scene, piece.from, tolerance);
      const added = nextId(next, kind === 'line' ? 'L' : 'A');
      const start = pointFor(next, scene, piece.to, tolerance);
      if (kind === 'line') {
        next.lines.push({ id: added, start, end });
        (next.constraints ??= []).push({ collinear: { first: id, second: added } });
      } else {
        next.arcs.push({ id: added, center: record.center, start, end, clockwise: record.clockwise });
        (next.constraints ??= []).push({ equal_radius: { first: id, second: added } });
      }
      eachLoop(next, (loop) => insertAfter(loop, id, added, true));
      retarget(next, id, added);
      summary = `${id} split around the trimmed piece; the rest is ${added}.`;
    }
    retarget(next, id, null);
  }
  const dropped = dropExtent(next, id);
  if (dropped) summary += ` ${dropped} constraint${dropped === 1 ? '' : 's'} on its old length went.`;
  dropImplied(next, scene, [id, ...(next.lines ?? []).concat(next.arcs ?? []).map((r) => r.id).filter((r) => !kindOf(definition, r))]);
  next = dropUnused(next, orphans);
  // A circle trimmed to an arc that meets an outline left open by an
  // earlier trim joins it, as a drawn curve would.
  next = continueOutline(definition, next);
  return { definition: next, summary };
}

/**
 * `definition` with line or arc `id` split at its point nearest sketch
 * point `at` into two curves sharing a new point (the second keeps the
 * name `<id>` end), as `{definition, summary}`. Lines stay collinear; arcs
 * share their center and radius. The input is not changed.
 */
export function splitAt(definition, scene, id, at) {
  const kind = kindOf(definition, id);
  if (kind !== 'line' && kind !== 'arc') throw new Error(`Split cuts lines and arcs; ${id} is not one. Trim a circle at two crossings instead.`);
  const entity = scene.entities.find((e) => e.id === id);
  const cumulative = lengths(entity.points);
  const { s, point } = nearest(entity.points, cumulative, at);
  const total = cumulative.at(-1);
  if (s <= 0.02 * total || s >= 0.98 * total) throw new Error('Click inside the curve, away from its ends.');
  const next = structuredClone(definition);
  const record = (kind === 'line' ? next.lines : next.arcs).find((r) => r.id === id);
  const middle = nextId(next, 'P');
  next.points.push({ id: middle, x: literal(point[0]), y: literal(point[1]), fixed: false });
  const end = record.end;
  record.end = middle;
  const added = nextId(next, kind === 'line' ? 'L' : 'A');
  if (kind === 'line') {
    next.lines.push({ id: added, start: middle, end });
    (next.constraints ??= []).push({ collinear: { first: id, second: added } });
  } else {
    next.arcs.push({ id: added, center: record.center, start: middle, end, clockwise: record.clockwise });
  }
  eachLoop(next, (loop) => insertAfter(loop, id, added, false));
  retarget(next, id, added);
  const dropped = dropExtent(next, id);
  const note = dropped ? ` ${dropped} constraint${dropped === 1 ? '' : 's'} on its old length went.` : '';
  return { definition: next, summary: `${id} split at ${middle}; its second part is ${added}.${note}`, point: middle };
}

/**
 * Installs Trim and Split: hovering a curve marks the piece Trim would
 * remove (in the conflict color) or the point Split would cut at, and a
 * click records the edit in the draft.
 */
export function installTrim(studio, document) {
  const view = document.getElementById('view');
  let hover = null; // { id, piece } | { id, point }
  const layer = () => studio.activeLayer();
  const scene = () => (layer() ? studio.sceneOf(layer().feature) : null);
  const definition = () => (studio.draft?.feature === layer()?.feature ? studio.draft.definition : studio.definitions[layer()?.feature] ?? null);

  function pick(event) {
    const s = scene();
    const l = layer();
    const local = studio.localAt(event.clientX, event.clientY);
    if (!s || !l?.frame || !local) return null;
    const { matrix, width, height } = studio.viewport();
    const rect = view.getBoundingClientRect();
    const curves = { entities: (s.entities ?? []).filter((e) => !e.id.startsWith('edited:')) };
    const target = hitTest({ scene: curves, frame: l.frame, matrix, width, height, at: [event.clientX - rect.left, event.clientY - rect.top], radius: 8 });
    if (!target) return null;
    if (studio.tool === 'trim') return { id: target.id, local, piece: trimPiece(definition(), s, target.id, local) };
    const entity = curves.entities.find((e) => e.id === target.id);
    return { id: target.id, local, point: nearest(entity.points, lengths(entity.points), local).point };
  }

  function run(target) {
    const draft = studio.ensureDraft();
    if (!draft) {
      studio.say(`Apply or discard the draft of ${studio.draft?.feature} first.`, true);
      return;
    }
    try {
      const edit = studio.tool === 'trim' ? trimmed : splitAt;
      const result = edit(draft.definition, studio.sceneOf(draft.feature), target.id, target.local);
      if (studio.changeDraft(result.definition) !== false) studio.say(`${result.summary} Previewing… Undo (Ctrl+Z) restores it.`);
    } catch (error) {
      studio.say(error.message, true);
    }
  }

  for (const id of ['trim', 'split']) {
    studio.tools.set(id, {
      down(event) {
        if (event.button !== 0 || event.shiftKey) return false;
        const target = pick(event);
        if (!target) studio.say(id === 'trim' ? 'Click the piece of a curve to trim.' : 'Click a line or arc where it should be split.', true);
        else if (target.piece?.unsupported) studio.say(target.piece.unsupported, true);
        else run(target);
        hover = null;
        return true;
      },
      move(event) {
        hover = pick(event);
        studio.redraw();
      },
    });
  }

  const decorate = studio.decorate;
  studio.decorate = (context) => {
    const nodes = decorate(context);
    if (!hover || !['trim', 'split'].includes(studio.tool)) return nodes;
    const { element } = context;
    if (hover.piece?.path) {
      const points = hover.piece.path.map((p) => studio.toScreen(p));
      if (points.every(Boolean)) {
        const d = points.map((p, i) => `${i ? 'L' : 'M'}${p[0].toFixed(1)} ${p[1].toFixed(1)}`).join('');
        nodes.push(element('path', { d, fill: 'none', stroke: 'var(--conflict)', 'stroke-width': 4, 'stroke-dasharray': '6 4', 'stroke-linecap': 'round' }));
      }
    } else if (hover.point) {
      const at = studio.toScreen(hover.point);
      if (at) nodes.push(element('circle', { cx: at[0], cy: at[1], r: 6, fill: 'none', stroke: 'var(--ghost)', 'stroke-width': 2.5 }));
    }
    return nodes;
  };
  const setTool = studio.setTool;
  studio.setTool = (id) => {
    hover = null;
    setTool(id);
  };
}
