// Modify tools of the sketch studio: Fillet and Chamfer round or cut a
// corner of the outer profile, and Extend runs a curve's end on to the next
// curve it meets. They record the model's own edits (profile fillet and
// chamfer operations, extend-to curve operations), and Offset moves the
// whole outline in or out (a profile offset operation), so the source geometry
// and its constraints stay and the edited outline is derived from them.
import { localBounds, simplify } from './sketch-studio-overlay.mjs';
import { dimensionExpression, hitTest } from './sketch-studio-select.mjs';

const sub = (a, b) => [a[0] - b[0], a[1] - b[1]];
const len = (v) => Math.hypot(v[0], v[1]);
const cross = (a, b) => a[0] * b[1] - a[1] * b[0];

/** Curve names in the outer profile: its list, or every line when empty. */
function profileCurves(definition) {
  const listed = definition?.profile ?? [];
  return listed.length ? listed : (definition?.lines ?? []).map((l) => l.id);
}

/**
 * Points where two curves of the outer profile meet, from the scene's
 * curve end points, each with whether a fillet or chamfer is already there.
 * O(profile + operations).
 */
export function profileCorners(definition, scene) {
  // Curve ends from the definition (a scene lists an arc's center too);
  // generated sides fall back to their two scene points.
  const ends = new Map();
  for (const r of [...(definition?.lines ?? []), ...(definition?.arcs ?? [])]) ends.set(r.id, [r.start, r.end]);
  for (const s of definition?.splines ?? []) if (!s.basis && s.points?.length) ends.set(s.id, [s.points[0], s.points.at(-1)]);
  for (const entity of scene?.entities ?? []) if (!ends.has(entity.id) && entity.point_ids?.length === 2) ends.set(entity.id, entity.point_ids);
  const uses = new Map();
  for (const id of profileCurves(definition)) {
    const [a, b] = ends.get(id) ?? [];
    if (!a || a === b) continue;
    for (const point of [a, b]) uses.set(point, (uses.get(point) ?? 0) + 1);
  }
  const edited = new Map();
  for (const operation of definition?.profile_operations ?? []) {
    const [kind, body] = Object.entries(operation)[0] ?? [];
    if ((kind === 'fillet' || kind === 'chamfer') && body?.point) edited.set(body.point, kind);
  }
  return new Map([...uses].filter(([, n]) => n >= 2).map(([id]) => [id, edited.get(id) ?? null]));
}

/**
 * Expressions from typed text: numbers in millimetres or parameter names,
 * separated by commas or spaces; `count` of them, the last repeated when
 * fewer are typed. Null when any is not a number or name.
 */
export function cornerValues(text, count) {
  const parts = text.split(/[,\s]+/).filter(Boolean);
  if (!parts.length || parts.length > count) return null;
  const values = parts.map((part) => dimensionExpression(part, 'mm'));
  if (values.some((v) => !v || (v.literal && !(v.literal.value > 0)))) return null;
  while (values.length < count) values.push(values.at(-1));
  return values;
}

/**
 * `definition` with a fillet (`[radius]`) or chamfer (`[incoming,
 * outgoing]`) at corner `point`, replacing any corner edit already there.
 * The input is not changed.
 */
export function withCornerEdit(definition, kind, point, values) {
  const next = structuredClone(definition);
  const operation = kind === 'fillet'
    ? { fillet: { point, radius: values[0] } }
    : { chamfer: { point, incoming: values[0], outgoing: values[1] } };
  const operations = (next.profile_operations ?? []).filter((op) => {
    const [k, body] = Object.entries(op)[0] ?? [];
    return !((k === 'fillet' || k === 'chamfer') && body?.point === point);
  });
  // Corner edits must precede whole-profile offsets.
  const at = operations.findIndex((op) => op.offset);
  operations.splice(at < 0 ? operations.length : at, 0, operation);
  next.profile_operations = operations;
  return next;
}

/** The end ('start' or 'end') of a scene curve nearer sketch point `at`. */
export function nearerEnd(entity, at) {
  const points = entity.points;
  return len(sub(at, points[0])) <= len(sub(at, points.at(-1))) ? 'start' : 'end';
}

/**
 * Where curve `id` would run if extended from `end`: the first other curve
 * its continuation meets, as `{target, at, path, length}` (sketch-local
 * points; `length` along the continuation), or null when it meets none.
 * Arcs continue round their circle (center from `definition`); other
 * curves continue straight along their end tangent. Curves touching the
 * end itself do not count. O(curve segments × continuation samples).
 */
export function extension(definition, scene, id, end) {
  const entity = (scene?.entities ?? []).find((e) => e.id === id);
  const samples = entity?.points ?? [];
  if (samples.length < 2) return null;
  const from = end === 'end' ? samples.at(-1) : samples[0];
  const before = end === 'end' ? samples.at(-2) : samples[1];
  const tangent = sub(from, before);
  if (!len(tangent)) return null;
  const bounds = localBounds(scene) ?? { min: [0, 0], max: [0, 0] };
  const reach = 2 * Math.hypot(bounds.max[0] - bounds.min[0], bounds.max[1] - bounds.min[1]) + 1;
  const arc = (definition?.arcs ?? []).find((a) => a.id === id);
  const center = arc && scene.points?.[arc.center];
  let path;
  if (center) {
    const radius = len(sub(from, center));
    const start = Math.atan2(from[1] - center[1], from[0] - center[0]);
    const sense = Math.sign(cross(sub(from, center), tangent)) || 1;
    const steps = 128;
    path = Array.from({ length: steps + 1 }, (_, i) => {
      const angle = start + (sense * 2 * Math.PI * i * 0.999) / steps;
      return [center[0] + radius * Math.cos(angle), center[1] + radius * Math.sin(angle)];
    });
  } else {
    const unit = [tangent[0] / len(tangent), tangent[1] / len(tangent)];
    path = [[from[0], from[1]], [from[0] + unit[0] * reach, from[1] + unit[1] * reach]];
  }
  const scale = Math.max(1, reach);
  let best = null;
  let along = 0;
  for (let i = 1; i < path.length && !best; i += 1) {
    const [p, q] = [path[i - 1], path[i]];
    const d = sub(q, p);
    for (const other of scene.entities) {
      if (other.id === id || other.id.startsWith('edited:')) continue;
      const points = simplify(other);
      for (let j = 1; j < points.length; j += 1) {
        const [a, b] = [points[j - 1], points[j]];
        const e = sub(b, a);
        const denominator = cross(d, e);
        if (Math.abs(denominator) < 1e-15) continue;
        const t = cross(sub(a, p), e) / denominator;
        const u = cross(sub(a, p), d) / denominator;
        if (t < 0 || t > 1 || u < -1e-9 || u > 1 + 1e-9) continue;
        const distance = along + t * len(d);
        if (distance <= 1e-6 * scale) continue;
        if (!best || distance < best.length) {
          best = { target: other.id, at: [p[0] + t * d[0], p[1] + t * d[1]], length: distance, segment: i };
        }
      }
    }
    along += len(d);
  }
  if (!best) return null;
  return { target: best.target, at: best.at, length: best.length, path: [...path.slice(0, best.segment), best.at] };
}

/**
 * `definition` with curve `entity` extended from `end` to `target`,
 * searching `length` × 1.1 + 0.5 mm, replacing an earlier extension of the
 * same end. The input is not changed.
 */
export function withExtension(definition, entity, end, target, length) {
  const next = structuredClone(definition);
  const limit = Math.ceil((length * 1.1 + 0.5) * 1000 - 1e-6) / 1000;
  const operations = (next.curve_operations ?? []).filter((op) => {
    const body = op.extend_to;
    return !(body && body.entity === entity && body.end === end);
  });
  next.curve_operations = [...operations, {
    extend_to: { entity, end, target, limit: { literal: { value: limit, dimension: 'length', unit: 'millimeter' } }, near: null },
  }];
  return next;
}

/**
 * A signed offset distance from typed text: millimetres, or a parameter
 * name with an optional leading minus. 0 means no offset; null when the
 * text is neither.
 */
export function offsetDistance(text) {
  const trimmed = text.trim();
  const negated = /^-\s*[A-Za-z_]/.test(trimmed);
  const value = dimensionExpression(negated ? trimmed.replace(/^-\s*/, '') : trimmed, 'mm');
  if (!value) return null;
  if (value.literal) return value.literal.value === 0 ? 0 : value;
  return negated ? { negate: value } : value;
}

/** The last whole-profile offset as `{distance, join, text}`, or null. */
export function profileOffset(definition) {
  const body = (definition?.profile_operations ?? []).findLast((op) => op.offset)?.offset;
  if (!body) return null;
  const d = body.distance;
  const text = d?.literal ? String(d.literal.value) : d?.parameter ?? (d?.negate?.parameter ? `-${d.negate.parameter}` : '');
  return { distance: d, join: body.join ?? 'arc', text };
}

/**
 * `definition` with its outline offset by `distance` (positive grows a
 * closed outline; on an open one it is to the right of traversal) with
 * `join` 'arc' or 'intersection', replacing its last offset. Distance 0
 * removes that offset. The input is not changed.
 */
export function withOffset(definition, distance, join = 'arc') {
  const next = structuredClone(definition);
  const operations = [...(next.profile_operations ?? [])];
  const index = operations.findLastIndex((op) => op.offset);
  if (distance === 0) {
    if (index >= 0) operations.splice(index, 1);
  } else {
    const operation = { offset: { distance, join } };
    if (index >= 0) operations[index] = operation;
    else operations.push(operation);
  }
  next.profile_operations = operations;
  return next;
}

/**
 * Polylines joined end to end in order, each turned to continue the last:
 * `{points, closed}` in sketch-local XY. O(points).
 */
export function chainPolylines(polylines) {
  const lines = polylines.filter((l) => l?.length >= 2).map((l) => l.map((p) => [p[0], p[1]]));
  if (!lines.length) return null;
  const near = (a, b) => len(sub(a, b));
  if (lines.length > 1) {
    const [first, second] = lines;
    const gap = (p) => Math.min(near(p, second[0]), near(p, second.at(-1)));
    if (gap(first[0]) < gap(first.at(-1))) first.reverse();
  }
  const points = [...lines[0]];
  for (const line of lines.slice(1)) {
    if (near(points.at(-1), line.at(-1)) < near(points.at(-1), line[0])) line.reverse();
    points.push(...line.slice(near(points.at(-1), line[0]) < 1e-9 ? 1 : 0));
  }
  let span = 0;
  for (const p of points) span = Math.max(span, Math.abs(p[0]), Math.abs(p[1]));
  const closed = points.length > 2 && near(points[0], points.at(-1)) <= 1e-6 * Math.max(1, span);
  if (closed) points.pop();
  return { points, closed };
}

/**
 * An approximate offset of a chained outline by `distance`, signed as the
 * model's profile offset: a guide drawn while typing, before the native
 * offset previews. Arc joins round convex corners; intersection joins
 * meet edges at a capped mitre. O(points × a 48-edge window).
 */
export function offsetPolyline({ points, closed }, distance, join = 'arc') {
  const clean = points.filter((p, i) => !i || len(sub(p, points[i - 1])) > 1e-12);
  if (closed && clean.length > 1 && len(sub(clean[0], clean.at(-1))) <= 1e-12) clean.pop();
  const n = clean.length;
  if (n < 2) return [];
  let area = 0;
  if (closed) for (let i = 0; i < n; i += 1) area += cross(clean[i], clean[(i + 1) % n]);
  // Positive grows a closed outline; open ones go to the right of travel.
  const side = closed && area < 0 ? -distance : distance;
  const shift = (from, to) => {
    const d = sub(to, from);
    const l = len(d);
    return [(side * d[1]) / l, (-side * d[0]) / l];
  };
  const edges = closed ? n : n - 1;
  const offsets = Array.from({ length: edges }, (_, i) => shift(clean[i], clean[(i + 1) % n]));
  const out = []; // [point, source vertex]
  for (let i = 0; i < n; i += 1) {
    const before = closed ? offsets[(i - 1 + n) % n] : offsets[i - 1];
    const after = offsets[i] ?? null;
    const v = clean[i];
    if (!before || !after) {
      const o = before ?? after;
      out.push([[v[0] + o[0], v[1] + o[1]], i]);
      continue;
    }
    const turn = cross(before, after) * Math.sign(side);
    const angle = Math.atan2(cross(before, after), before[0] * after[0] + before[1] * after[1]);
    if (join === 'arc' && turn > 0 && Math.abs(angle) > 1e-6) {
      const steps = Math.max(1, Math.ceil(Math.abs(angle) / 0.25));
      for (let k = 0; k <= steps; k += 1) {
        const t = (angle * k) / steps;
        const [c, s] = [Math.cos(t), Math.sin(t)];
        out.push([[v[0] + before[0] * c - before[1] * s, v[1] + before[0] * s + before[1] * c], i]);
      }
      continue;
    }
    const cos = (before[0] * after[0] + before[1] * after[1]) / (side * side);
    const scale = 1 / Math.max(0.25, (1 + cos) / 2);
    out.push([[v[0] + ((before[0] + after[0]) / 2) * scale, v[1] + ((before[1] + after[1]) / 2) * scale], i]);
  }
  // Points that came closer than the distance to a nearby edge overshot
  // an inside corner; dropping them leaves the corner itself.
  const reach = Math.abs(distance) * (1 - 1e-9) - 1e-12;
  const window = 24;
  const kept = out.filter(([p, at]) => {
    for (let j = at - window; j < at + window; j += 1) {
      if (!closed && (j < 0 || j >= edges)) continue;
      const a = clean[((j % n) + n) % n];
      const b = clean[(((j + 1) % n) + n) % n];
      const e = sub(b, a);
      const t = Math.max(0, Math.min(1, ((p[0] - a[0]) * e[0] + (p[1] - a[1]) * e[1]) / (e[0] * e[0] + e[1] * e[1])));
      if (len(sub(p, [a[0] + t * e[0], a[1] + t * e[1]])) < reach) return false;
    }
    return true;
  }).map(([p]) => p);
  if (closed && kept.length) kept.push(kept[0]);
  return kept;
}

/**
 * The outline Offset works on, chained from the scene: the edited outline
 * when it has corner or curve edits but no offset yet, otherwise the
 * profile's source curves (fillets are then left out of the guide).
 */
export function offsetBase(definition, scene) {
  if (!profileOffset(definition) && (scene?.edited_profile ?? []).length) return chainPolylines(scene.edited_profile);
  const curves = new Map((scene?.entities ?? []).map((e) => [e.id, e.points]));
  return chainPolylines(profileCurves(definition).map((id) => curves.get(id)));
}

/** The modify tools as palette entries (see sketch-studio-tools.mjs). */
export const MODIFY_TOOLS = [
  { id: 'fillet', label: 'Fillet', key: 'I', hint: 'Click a corner of the outline, then type a radius or parameter and press Enter.' },
  { id: 'chamfer', label: 'Chamfer', key: 'H', hint: 'Click a corner of the outline, then type one distance, or two (incoming, outgoing), and press Enter.' },
  { id: 'extend', label: 'Extend', key: 'W', hint: 'Click near the end of a curve to run it on to the next curve it meets.' },
  { id: 'trim', label: 'Trim', key: 'M', hint: 'Click the piece of a line, arc or circle to cut away, between the curves that cross it.' },
  { id: 'split', label: 'Split', key: 'Y', hint: 'Click a line or arc where it should be cut in two.' },
  { id: 'offset', label: 'Offset', key: 'Z', hint: 'Click the outline, then type a distance (positive grows it, negative shrinks it) and press Enter.' },
];

export const MODIFY_ICONS = {
  fillet: '<path d="M4 4v9a7 7 0 0 0 7 7h9"/><path d="M4 13V20h7" stroke-dasharray="2 2" opacity=".6"/>',
  chamfer: '<path d="M4 4v10l6 6h10"/><path d="M4 14v6h6" stroke-dasharray="2 2" opacity=".6"/>',
  trim: '<path d="M4 12h5" /><path d="M9 12h6" stroke-dasharray="2 2" opacity=".6"/><path d="M15 12h5M9 4v16M15 4v16"/>',
  split: '<path d="M3 16L21 8"/><circle cx="12" cy="12" r="2.5" fill="currentColor"/><path d="M10 5l4 0M10 19l4 0" opacity=".6"/>',
  offset: '<rect x="8" y="8" width="8" height="8" rx="1"/><rect x="3" y="3" width="18" height="18" rx="5" stroke-dasharray="3 2"/>',
  extend: '<path d="M3 12h9"/><path d="M12 12h6" stroke-dasharray="2 2"/><path d="M20 4v16"/><path d="M15 9l3 3-3 3"/>',
};

/**
 * Installs Fillet, Chamfer, Extend and Offset: hover highlights the corner,
 * the curve end and its continuation, or the outline; a click records the
 * edit in the draft (corners and offsets first ask for their value in a box
 * by the click, and an offset draws a guide while it is typed).
 */
export function installModify(studio, document) {
  const { html } = studio;
  const view = document.getElementById('view');
  const box = html('div', { id: 'modify-box', hidden: true });
  view.append(box);
  document.head.append(html('style', {}, `
    #modify-box { position: absolute; z-index: 2; display: flex; gap: 4px; align-items: center; padding: 5px;
      background: var(--panel); border: 1px solid var(--line); border-radius: 8px; box-shadow: 0 4px 14px rgb(0 0 0 / 0.18); }
    #modify-box input { width: 110px; }
    #modify-box span { color: var(--muted); white-space: nowrap; }`));

  const last = { fillet: '2', chamfer: '2', offset: '2', join: 'arc' };
  let hover = null; // { corner, at } | { entity, end, reach } | { outline }
  let open = null; // { kind, point } | { kind: 'offset', base, guide }

  const layer = () => studio.activeLayer();
  const scene = () => (layer() ? studio.sceneOf(layer().feature) : null);
  const definition = () => (studio.draft?.feature === layer()?.feature ? studio.draft.definition : studio.definitions[layer()?.feature] ?? null);

  function screenAt(event) {
    const rect = view.getBoundingClientRect();
    return [event.clientX - rect.left, event.clientY - rect.top];
  }

  function pick(event) {
    const s = scene();
    const l = layer();
    if (!s || !l?.frame) return null;
    const { matrix, width, height } = studio.viewport();
    const at = screenAt(event);
    if (studio.tool === 'offset') {
      const d = definition();
      const wanted = new Set(profileCurves(d));
      const curves = { entities: (s.entities ?? []).filter((e) => wanted.has(e.id)) };
      const target = hitTest({ scene: curves, frame: l.frame, matrix, width, height, at, radius: 8 });
      if (!target) return null;
      if (hover?.outline) return hover;
      const base = offsetBase(d, s);
      return base ? { outline: base } : null;
    }
    if (studio.tool === 'extend') {
      const curves = { entities: (s.entities ?? []).filter((e) => !e.id.startsWith('edited:')) };
      const target = hitTest({ scene: curves, frame: l.frame, matrix, width, height, at, radius: 8 });
      const local = studio.localAt(event.clientX, event.clientY);
      if (!target || !local) return null;
      const entity = curves.entities.find((e) => e.id === target.id);
      const end = nearerEnd(entity, local);
      if (hover?.entity === entity.id && hover.end === end) return hover;
      return { entity: entity.id, end, reach: extension(definition(), s, entity.id, end) };
    }
    const target = hitTest({ scene: { points: s.points }, frame: l.frame, matrix, width, height, at, radius: 9 });
    if (!target) return null;
    const corners = profileCorners(definition(), s);
    return corners.has(target.id) ? { corner: target.id, existing: corners.get(target.id), at: s.points[target.id] } : { notCorner: target.id };
  }

  function close() {
    open = null;
    box.hidden = true;
  }

  function commit(next, note) {
    const draft = studio.ensureDraft();
    if (!draft) {
      studio.say(`Apply or discard the draft of ${studio.draft?.feature} first.`, true);
      return;
    }
    studio.changeDraft(next(draft.definition));
    studio.say(`${note} Previewing… Undo (Ctrl+Z) removes it.`);
  }

  function ask(kind, point, at) {
    open = { kind, point };
    const unit = kind === 'fillet' ? 'Radius' : 'Distances';
    const input = html('input', {
      'aria-label': `${kind} ${unit.toLowerCase()}`, value: last[kind], placeholder: kind === 'fillet' ? 'radius or parameter' : 'a or a, b',
      onkeydown: (event) => {
        event.stopPropagation();
        if (event.key === 'Enter') finish(input.value);
        else if (event.key === 'Escape') close();
      },
    });
    studio.fill(box, html('span', {}, `${kind === 'fillet' ? 'Fillet' : 'Chamfer'} ${point} · ${unit}`), input, html('span', {}, 'mm'),
      html('button', { onclick: () => finish(input.value) }, 'Add'));
    box.hidden = false;
    const screen = studio.toScreen(at) ?? [20, 20];
    const rect = view.getBoundingClientRect();
    box.style.left = `${Math.max(8, Math.min(rect.width - box.offsetWidth - 8, screen[0] + 14))}px`;
    box.style.top = `${Math.max(8, Math.min(rect.height - box.offsetHeight - 8, screen[1] + 14))}px`;
    setTimeout(() => { if (open?.point === point) { input.focus(); input.select(); } }, 0);
  }

  function askOffset(base, screen) {
    const current = profileOffset(definition());
    open = { kind: 'offset', base, guide: null };
    const join = html('select', { 'aria-label': 'Offset corners', onchange: () => guide(input.value, join.value) },
      html('option', { value: 'arc' }, 'Round corners'), html('option', { value: 'intersection' }, 'Sharp corners'));
    join.value = current?.join ?? last.join;
    const input = html('input', {
      'aria-label': 'offset distance', value: current?.text || last.offset, placeholder: 'distance or parameter',
      oninput: () => guide(input.value, join.value),
      onkeydown: (event) => {
        event.stopPropagation();
        if (event.key === 'Enter') finishOffset(input.value, join.value);
        else if (event.key === 'Escape') close();
      },
    });
    const buttons = [html('button', { onclick: () => finishOffset(input.value, join.value) }, current ? 'Change' : 'Add')];
    if (current) buttons.push(html('button', { onclick: () => finishOffset('0', join.value) }, 'Remove'));
    studio.fill(box, html('span', {}, `Offset outline${base.closed ? '' : ' (open: + is right of travel)'}`), input, html('span', {}, 'mm'), join, ...buttons);
    box.hidden = false;
    const rect = view.getBoundingClientRect();
    box.style.left = `${Math.max(8, Math.min(rect.width - box.offsetWidth - 8, screen[0] + 14))}px`;
    box.style.top = `${Math.max(8, Math.min(rect.height - box.offsetHeight - 8, screen[1] + 14))}px`;
    guide(input.value, join.value);
    setTimeout(() => { if (open?.kind === 'offset') { input.focus(); input.select(); } }, 0);
  }

  // The typed distance drawn as an approximate guide; parameters have none.
  function guide(text, join) {
    if (open?.kind !== 'offset') return;
    const distance = offsetDistance(text);
    open.guide = distance?.literal ? offsetPolyline(open.base, distance.literal.value, join) : null;
    studio.redraw();
  }

  function finishOffset(text, join) {
    const distance = offsetDistance(text);
    if (distance === null) {
      studio.say('Enter a distance in mm (negative shrinks the outline), a parameter name, or 0 to remove the offset.', true);
      return;
    }
    const current = profileOffset(definition());
    close();
    if (distance === 0 && !current) {
      studio.say('The outline has no offset to remove.');
      return;
    }
    if (distance !== 0) last.offset = text.trim();
    last.join = join;
    commit((d) => withOffset(d, distance, join), distance === 0 ? 'Offset removed.' : `Outline offset by ${text.trim()} mm.`);
  }

  function finish(text) {
    if (!open) return;
    const { kind, point } = open;
    const values = cornerValues(text, kind === 'fillet' ? 1 : 2);
    if (!values) {
      studio.say(kind === 'fillet' ? 'Enter a positive radius or a parameter name.' : 'Enter one or two positive distances or parameter names.', true);
      return;
    }
    last[kind] = text.trim();
    close();
    commit((d) => withCornerEdit(d, kind, point, values), `${kind === 'fillet' ? 'Fillet' : 'Chamfer'} at ${point} added.`);
  }

  for (const kind of ['fillet', 'chamfer']) {
    studio.tools.set(kind, {
      down(event) {
        if (event.button !== 0 || event.shiftKey) return false;
        const target = pick(event);
        if (target?.corner) {
          ask(kind, target.corner, target.at);
          if (target.existing) studio.say(`${target.corner} already has a ${target.existing}; the new value replaces it.`);
        } else {
          close();
          studio.say(target?.notCorner
            ? `${target.notCorner} is not a corner of the outer profile. Holes and regions take fillets in Advanced → region edits.`
            : 'Click a corner where two curves of the outline meet.', true);
        }
        return true;
      },
      move(event) {
        hover = pick(event);
        studio.redraw();
      },
    });
  }

  studio.tools.set('extend', {
    down(event) {
      if (event.button !== 0 || event.shiftKey) return false;
      const target = pick(event);
      if (!target) {
        studio.say('Click near the end of a curve.', true);
      } else if (!target.reach) {
        studio.say(`${target.entity} meets no other curve beyond its ${target.end}.`, true);
      } else {
        const { entity, end, reach } = target;
        commit((d) => withExtension(d, entity, end, reach.target, reach.length), `${entity} extended from its ${end} to ${reach.target}.`);
      }
      hover = null;
      return true;
    },
    move(event) {
      hover = pick(event);
      studio.redraw();
    },
  });

  studio.tools.set('offset', {
    down(event) {
      if (event.button !== 0 || event.shiftKey) return false;
      const target = pick(event);
      if (target?.outline) {
        askOffset(target.outline, screenAt(event));
      } else {
        close();
        studio.say('Click a curve of the outer profile.', true);
      }
      hover = null;
      return true;
    },
    move(event) {
      if (open) return;
      hover = pick(event);
      studio.redraw();
    },
  });

  // Hover marks: the corner ring, or the continuation up to where it meets.
  const decorate = studio.decorate;
  studio.decorate = (context) => {
    const nodes = decorate(context);
    const l = layer();
    if (!l?.frame || !['fillet', 'chamfer', 'extend', 'offset'].includes(studio.tool)) return nodes;
    const { element } = context;
    const polyline = (points, attributes) => {
      const screen = points.map((p) => studio.toScreen(p));
      if (screen.length < 2 || !screen.every(Boolean)) return;
      const d = screen.map((p, i) => `${i ? 'L' : 'M'}${p[0].toFixed(1)} ${p[1].toFixed(1)}`).join('');
      nodes.push(element('path', { d, class: 'ghost', fill: 'none', stroke: 'var(--ghost)', ...attributes }));
    };
    if (open?.kind === 'offset') {
      const { points, closed } = open.base;
      polyline(closed ? [...points, points[0]] : points, { 'stroke-width': 3, opacity: 0.35 });
      if (open.guide) polyline(open.guide, { 'stroke-width': 2, 'stroke-dasharray': '6 4' });
      return nodes;
    }
    if (!hover) return nodes;
    if (hover.outline) {
      const { points, closed } = hover.outline;
      polyline(closed ? [...points, points[0]] : points, { 'stroke-width': 3, opacity: 0.5 });
    } else if (hover.corner) {
      const at = studio.toScreen(hover.at);
      if (at) nodes.push(element('circle', { cx: at[0], cy: at[1], r: 9, fill: 'none', stroke: 'var(--ghost)', 'stroke-width': 2.5 }));
    } else if (hover.reach) {
      const points = hover.reach.path.map((p) => studio.toScreen(p));
      if (points.every(Boolean)) {
        const d = points.map((p, i) => `${i ? 'L' : 'M'}${p[0].toFixed(1)} ${p[1].toFixed(1)}`).join('');
        nodes.push(element('path', { d, class: 'ghost', fill: 'none', stroke: 'var(--ghost)', 'stroke-width': 2, 'stroke-dasharray': '6 4' }));
        const hit = points.at(-1);
        nodes.push(element('circle', { cx: hit[0], cy: hit[1], r: 5, fill: 'var(--ghost)' }));
      }
    }
    return nodes;
  };

  const setTool = studio.setTool;
  studio.setTool = (id) => {
    close();
    hover = null;
    setTool(id);
  };
  const activate = studio.onActivate;
  studio.onActivate = (l) => {
    close();
    hover = null;
    activate?.(l);
  };
  const key = studio.onKey;
  studio.onKey = (event) => {
    if (event.key === 'Escape' && open) {
      close();
      return true;
    }
    return key?.(event) ?? false;
  };
}
