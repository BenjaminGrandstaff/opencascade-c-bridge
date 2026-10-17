// Pattern tools of the sketch studio: Mirror, Linear pattern and Circular
// pattern record the model's linked sketch patterns. Choose source curves
// (the selection, or clicks), press Enter, then click the mirror line, the
// direction line or the center point; a dashed guide shows the copies.
// Mirror shares source points on its axis and closes a half outline; any
// hole loop copied whole stays a hole.
import { freshName } from './geometry-names.mjs';
import { makePattern, patternGeometry } from './pattern-controls.mjs';
import { openEnds } from './sketch-studio-delete.mjs';
import { dimensionExpression, hitTest } from './sketch-studio-select.mjs';

const KINDS = { mirror: 'Mirror', linear: 'Linear', circular: 'Circular' };
const sub = (a, b) => [a[0] - b[0], a[1] - b[1]];
const len = (v) => Math.hypot(v[0], v[1]);

/** Curve ends `[start, end]` by id: definition lines, arcs and splines, else two-point scene curves. */
function curveEnds(definition, scene) {
  const ends = new Map();
  for (const r of [...(definition?.lines ?? []), ...(definition?.arcs ?? [])]) ends.set(r.id, [r.start, r.end]);
  for (const s of definition?.splines ?? []) if (!s.basis && s.points?.length) ends.set(s.id, [s.points[0], s.points.at(-1)]);
  for (const entity of scene?.entities ?? []) if (!ends.has(entity.id) && entity.point_ids?.length === 2) ends.set(entity.id, entity.point_ids);
  return ends;
}

/**
 * Points of the source curves lying on the axis line (its ends included),
 * from solved scene positions; a mirror shares them instead of copying.
 * O(source points).
 */
export function onAxisPoints(definition, scene, sources, axis) {
  const entities = new Map((scene?.entities ?? []).map((e) => [e.id, e]));
  const line = entities.get(axis)?.points;
  if (!line || line.length < 2) return [];
  const [a, b] = [line[0], line.at(-1)];
  const d = sub(b, a);
  const length = len(d);
  if (!length) return [];
  let span = 1;
  for (const p of Object.values(scene.points ?? {})) span = Math.max(span, Math.abs(p[0]), Math.abs(p[1]));
  const tolerance = 1e-6 * span;
  const on = new Set();
  for (const id of sources) {
    for (const point of entities.get(id)?.point_ids ?? []) {
      const p = scene.points?.[point];
      if (p && Math.abs(d[0] * (p[1] - a[1]) - d[1] * (p[0] - a[0])) / length <= tolerance) on.add(point);
    }
  }
  return [...on];
}

/**
 * `definition` with pattern `kind` ('mirror', 'linear' or 'circular') of
 * `sources` about `reference` (axis line, direction line or center point),
 * as `{definition, id, summary}`. `options`: `count` and `spacing` (linear)
 * or `angle` (circular, per step; omitted divides a full turn). Hole loops
 * whose curves are all sources get copied holes; a mirror of a whole open
 * profile whose ends lie on the axis closes it with the copies in reverse.
 * Throws with a message when the pattern is not valid. The input is not
 * changed.
 */
export function withPattern(definition, scene, kind, sources, reference, options = {}) {
  const id = freshName(definition, KINDS[kind]);
  const role = { mirror: 'axis', linear: 'direction', circular: 'center' }[kind];
  const fields = { id, entities: [...sources], [role]: reference };
  if (kind === 'mirror') fields.on_axis = onAxisPoints(definition, scene, sources, reference);
  else fields.count = options.count;
  if (kind === 'linear') fields.spacing = options.spacing;
  if (kind === 'circular' && options.angle) fields.angle = options.angle;
  const pattern = makePattern(definition, kind, fields);
  const next = structuredClone(definition);
  next.patterns = [...(next.patterns ?? []), pattern];
  const copies = kind === 'mirror' ? 1 : options.count - 1;
  const copy = (k, loop) => (kind === 'mirror' ? [...loop].reverse() : loop).map((curve) => `${id}:${k}:${curve}`);
  const notes = [];
  const wanted = new Set(sources);
  const holes = (next.holes ?? []).filter((loop) => loop.every((curve) => wanted.has(curve)));
  if (holes.length) {
    for (let k = 1; k <= copies; k += 1) for (const loop of holes) next.holes.push(copy(k, loop));
    notes.push(`${holes.length * copies} copied hole${holes.length * copies === 1 ? '' : 's'}`);
  }
  if (kind === 'mirror') {
    const profile = next.profile ?? [];
    const ends = openEnds(profile, curveEnds(definition, scene));
    const shared = new Set(fields.on_axis);
    if (ends && profile.every((curve) => wanted.has(curve)) && ends.every((point) => shared.has(point))) {
      next.profile = [...profile, ...copy(1, profile)];
      notes.push('the outline closed through the axis');
    }
  }
  const what = { mirror: 'Mirrored', linear: `${options.count}× linear pattern of`, circular: `${options.count}× circular pattern of` }[kind];
  const summary = `${id}: ${what} ${sources.length} curve${sources.length === 1 ? '' : 's'}${notes.length ? `, ${notes.join(' and ')}` : ''}.`;
  return { definition: next, id, summary };
}

/**
 * Where the copies would be, as sketch-local polylines from the scene's
 * sampled curves: reflected across the axis line, stepped `spacing` mm
 * along the direction line, or turned `angle` radians (default a full turn
 * over `count`) about the center point. O(source samples × count).
 */
export function patternGuide(scene, kind, sources, reference, { count = 2, spacing = 0, angle = null } = {}) {
  const entities = new Map((scene?.entities ?? []).map((e) => [e.id, e]));
  const curves = sources.map((id) => entities.get(id)?.points).filter((p) => p?.length);
  if (kind === 'circular') {
    const c = scene?.points?.[reference];
    if (!c) return [];
    const step = angle ?? (2 * Math.PI) / count;
    const out = [];
    for (let k = 1; k < count; k += 1) {
      const [cos, sin] = [Math.cos(k * step), Math.sin(k * step)];
      for (const points of curves) out.push(points.map((p) => [c[0] + (p[0] - c[0]) * cos - (p[1] - c[1]) * sin, c[1] + (p[0] - c[0]) * sin + (p[1] - c[1]) * cos]));
    }
    return out;
  }
  const line = entities.get(reference)?.points;
  if (!line || line.length < 2) return [];
  const [a, b] = [line[0], line.at(-1)];
  const d = sub(b, a);
  const length = len(d);
  if (!length) return [];
  const u = [d[0] / length, d[1] / length];
  if (kind === 'mirror') {
    return curves.map((points) => points.map((p) => {
      const t = (p[0] - a[0]) * u[0] + (p[1] - a[1]) * u[1];
      const foot = [a[0] + t * u[0], a[1] + t * u[1]];
      return [2 * foot[0] - p[0], 2 * foot[1] - p[1]];
    }));
  }
  const out = [];
  for (let k = 1; k < count; k += 1) for (const points of curves) out.push(points.map((p) => [p[0] + k * spacing * u[0], p[1] + k * spacing * u[1]]));
  return out;
}

/**
 * A spacing that clears the sources along the direction line: their extent
 * along it plus a quarter (at least 1 mm), rounded to 0.1 mm.
 */
export function suggestedSpacing(scene, sources, direction) {
  const entities = new Map((scene?.entities ?? []).map((e) => [e.id, e]));
  const line = entities.get(direction)?.points;
  if (!line || line.length < 2) return 10;
  const d = sub(line.at(-1), line[0]);
  const u = [d[0] / (len(d) || 1), d[1] / (len(d) || 1)];
  let [low, high] = [Infinity, -Infinity];
  for (const id of sources) for (const p of entities.get(id)?.points ?? []) {
    const t = p[0] * u[0] + p[1] * u[1];
    [low, high] = [Math.min(low, t), Math.max(high, t)];
  }
  const extent = Number.isFinite(low) ? high - low : 0;
  return Math.round((extent + Math.max(1, extent / 4)) * 10) / 10;
}

/** The pattern tools as palette entries (see sketch-studio-tools.mjs). */
export const PATTERN_TOOLS = [
  { id: 'mirror', label: 'Mirror', key: '', hint: 'Select or click the curves to mirror and press Enter, then click the mirror line.' },
  { id: 'linear_pattern', label: 'Linear', key: '', hint: 'Select or click the curves to copy and press Enter, click the direction line, then type a count and spacing.' },
  { id: 'circular_pattern', label: 'Circular', key: '', hint: 'Select or click the curves to copy and press Enter, click the center point, then type a count (and a step angle, or none for a full turn).' },
];

export const PATTERN_ICONS = {
  mirror: '<path d="M12 3v18" stroke-dasharray="2 2"/><path d="M9 6L4 12l5 6z"/><path d="M15 6l5 6-5 6z" opacity=".6"/>',
  linear_pattern: '<rect x="3" y="9" width="5" height="6"/><rect x="10" y="9" width="5" height="6" opacity=".6"/><rect x="17" y="9" width="4" height="6" opacity=".35"/><path d="M3 19h18"/>',
  circular_pattern: '<circle cx="12" cy="12" r="1.5" fill="currentColor"/><circle cx="12" cy="4.5" r="2"/><circle cx="18.5" cy="15.5" r="2" opacity=".6"/><circle cx="5.5" cy="15.5" r="2" opacity=".6"/>',
};

const KIND_OF = { mirror: 'mirror', linear_pattern: 'linear', circular_pattern: 'circular' };

/**
 * Installs the pattern tools: source curves come from the selection when
 * the tool starts or from clicks; Enter moves on to the reference; Mirror
 * records on its click, the others ask for count and spacing or angle in a
 * box with a live guide.
 */
export function installPatterns(studio, document) {
  const { html } = studio;
  const view = document.getElementById('view');
  const box = html('div', { id: 'pattern-box', hidden: true });
  view.append(box);
  document.head.append(html('style', {}, `
    #pattern-box { position: absolute; z-index: 2; display: flex; gap: 4px; align-items: center; padding: 5px;
      background: var(--panel); border: 1px solid var(--line); border-radius: 8px; box-shadow: 0 4px 14px rgb(0 0 0 / 0.18); }
    #pattern-box input { width: 70px; }
    #pattern-box span { color: var(--muted); white-space: nowrap; }`));

  const last = { count: '3', spacing: '', angle: '', ring: '6' };
  let sources = [];
  let phase = 'sources'; // 'sources' | 'reference'
  let hover = null; // curve id (sources) or reference id
  let open = null; // { kind, reference, guide }

  const kind = () => KIND_OF[studio.tool];
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
    const d = definition();
    if (!s || !l?.frame || !d) return null;
    const { matrix, width, height } = studio.viewport();
    const at = screenAt(event);
    if (phase === 'reference' && kind() === 'circular') {
      const points = new Set(patternGeometry(d, 'points'));
      const target = hitTest({ scene: { points: Object.fromEntries(Object.entries(s.points ?? {}).filter(([id]) => points.has(id))) }, frame: l.frame, matrix, width, height, at, radius: 9 });
      return target?.id ?? null;
    }
    const allowed = new Set(patternGeometry(d, phase === 'reference' ? 'lines' : 'curves'));
    const curves = { entities: (s.entities ?? []).filter((e) => allowed.has(e.id)) };
    return hitTest({ scene: curves, frame: l.frame, matrix, width, height, at, radius: 8 })?.id ?? null;
  }

  function prompt() {
    const tool = studio.tool;
    if (!kind()) return;
    if (phase === 'sources') {
      studio.say(sources.length
        ? `${sources.length} source curve${sources.length === 1 ? '' : 's'}: click more to add or remove, then press Enter.`
        : `${PATTERN_TOOLS.find((t) => t.id === tool).label}: click the curves to copy, then press Enter.`);
    } else {
      studio.say({ mirror: 'Click the mirror line.', linear: 'Click the line giving the direction (start to end).', circular: 'Click the center point.' }[kind()]
        + ' Escape goes back to the sources.');
    }
  }

  function reset(keep = []) {
    close();
    sources = keep;
    phase = 'sources';
    hover = null;
  }

  function close() {
    open = null;
    box.hidden = true;
  }

  function commit(reference, options = {}) {
    const draft = studio.ensureDraft();
    if (!draft) {
      studio.say(`Apply or discard the draft of ${studio.draft?.feature} first.`, true);
      return false;
    }
    try {
      const result = withPattern(draft.definition, studio.sceneOf(draft.feature), kind(), sources, reference, options);
      if (studio.changeDraft(result.definition) === false) return false;
      reset();
      studio.say(`${result.summary} Previewing… Undo (Ctrl+Z) removes it.`);
      return true;
    } catch (error) {
      studio.say(error.message, true);
      return false;
    }
  }

  function values(count, value) {
    const n = Number(count.trim());
    if (!Number.isInteger(n) || n < 2 || n > 1000) return { error: 'Enter a count from 2 to 1000, including the source.' };
    if (kind() === 'linear') {
      const spacing = dimensionExpression(value, 'mm');
      if (!spacing || (spacing.literal && !spacing.literal.value)) return { error: 'Enter a nonzero spacing in mm (negative reverses) or a parameter name.' };
      return { count: n, spacing };
    }
    if (!value.trim()) return { count: n };
    const angle = dimensionExpression(value, '°');
    if (!angle || (angle.literal && !angle.literal.value)) return { error: 'Enter a nonzero step angle in degrees, a parameter name, or nothing for a full turn.' };
    return { count: n, angle };
  }

  function guideOptions(parsed) {
    if (!parsed || parsed.error) return null;
    if (kind() === 'linear') return parsed.spacing.literal ? { count: parsed.count, spacing: parsed.spacing.literal.value } : null;
    if (parsed.angle && !parsed.angle.literal) return null;
    return { count: parsed.count, angle: parsed.angle?.literal.value ?? null };
  }

  function ask(reference, screen) {
    const linear = kind() === 'linear';
    if (linear && !last.spacing) last.spacing = String(suggestedSpacing(scene(), sources, reference));
    open = { kind: kind(), reference, guide: null };
    const update = () => {
      const options = guideOptions(values(count.value, value.value));
      open.guide = options ? patternGuide(scene(), kind(), sources, reference, options) : null;
      studio.redraw();
    };
    const finish = () => {
      const parsed = values(count.value, value.value);
      if (parsed.error) {
        studio.say(parsed.error, true);
        return;
      }
      last[linear ? 'count' : 'ring'] = count.value.trim();
      last[linear ? 'spacing' : 'angle'] = value.value.trim();
      commit(reference, parsed);
    };
    const keys = (event) => {
      event.stopPropagation();
      if (event.key === 'Enter') finish();
      else if (event.key === 'Escape') { close(); studio.redraw(); }
    };
    const count = html('input', { 'aria-label': 'pattern count', value: linear ? last.count : last.ring, oninput: update, onkeydown: keys });
    const value = html('input', { 'aria-label': linear ? 'pattern spacing' : 'pattern step angle', value: linear ? last.spacing : last.angle,
      placeholder: linear ? 'mm or parameter' : 'full turn', oninput: update, onkeydown: keys });
    studio.fill(box, html('span', {}, `${linear ? 'Linear' : 'Circular'} · Count`), count,
      html('span', {}, linear ? 'Spacing' : 'Step'), value, html('span', {}, linear ? 'mm' : '°'),
      html('button', { onclick: finish }, 'Add'));
    box.hidden = false;
    const rect = view.getBoundingClientRect();
    box.style.left = `${Math.max(8, Math.min(rect.width - box.offsetWidth - 8, screen[0] + 14))}px`;
    box.style.top = `${Math.max(8, Math.min(rect.height - box.offsetHeight - 8, screen[1] + 14))}px`;
    update();
    setTimeout(() => { if (open) { count.focus(); count.select(); } }, 0);
  }

  for (const tool of PATTERN_TOOLS) {
    studio.tools.set(tool.id, {
      down(event) {
        if (event.button !== 0 || event.shiftKey) return false;
        const target = pick(event);
        if (phase === 'sources') {
          if (!target) studio.say('Click a curve to add it to the sources.', true);
          else {
            sources = sources.includes(target) ? sources.filter((id) => id !== target) : [...sources, target];
            prompt();
          }
        } else if (!target) {
          studio.say(kind() === 'circular' ? 'Click a point for the center.' : 'Click a line.', true);
        } else if (kind() === 'mirror') {
          commit(target);
        } else {
          ask(target, screenAt(event));
        }
        studio.redraw();
        return true;
      },
      move(event) {
        if (open) return;
        hover = pick(event);
        studio.redraw();
      },
    });
  }

  const decorate = studio.decorate;
  studio.decorate = (context) => {
    const nodes = decorate(context);
    const s = scene();
    if (!kind() || !s) return nodes;
    const { element } = context;
    const polyline = (points, attributes) => {
      const screen = points.map((p) => studio.toScreen(p));
      if (screen.length < 2 || !screen.every(Boolean)) return;
      const d = screen.map((p, i) => `${i ? 'L' : 'M'}${p[0].toFixed(1)} ${p[1].toFixed(1)}`).join('');
      nodes.push(element('path', { d, fill: 'none', ...attributes }));
    };
    const entities = new Map((s.entities ?? []).map((e) => [e.id, e]));
    for (const id of sources) {
      const points = entities.get(id)?.points;
      if (points) polyline(points, { stroke: 'var(--accent)', 'stroke-width': 4, opacity: 0.7 });
    }
    let guide = open?.guide ?? null;
    const reference = open?.reference ?? (phase === 'reference' ? hover : null);
    if (!open && reference && sources.length) {
      const k = kind();
      const options = k === 'linear'
        ? { count: Number(last.count) || 3, spacing: Number(last.spacing) || suggestedSpacing(s, sources, reference) }
        : { count: Number(last.ring) || 6, angle: last.angle && Number.isFinite(Number(last.angle)) ? (Number(last.angle) * Math.PI) / 180 : null };
      guide = patternGuide(s, k, sources, reference, options);
    }
    if (reference) {
      if (kind() === 'circular') {
        const at = s.points?.[reference] && studio.toScreen(s.points[reference]);
        if (at) nodes.push(element('circle', { cx: at[0], cy: at[1], r: 8, fill: 'none', stroke: 'var(--ghost)', 'stroke-width': 2.5 }));
      } else if (entities.get(reference)?.points) {
        polyline(entities.get(reference).points, { stroke: 'var(--ghost)', 'stroke-width': 3 });
      }
    }
    for (const points of guide ?? []) polyline(points, { class: 'ghost', stroke: 'var(--ghost)', 'stroke-width': 2, 'stroke-dasharray': '6 4' });
    if (phase === 'sources' && hover && !sources.includes(hover) && entities.get(hover)?.points) {
      polyline(entities.get(hover).points, { stroke: 'var(--accent)', 'stroke-width': 3, opacity: 0.4 });
    }
    return nodes;
  };

  // The selection's curves become the sources when a pattern tool starts
  // (switching tools clears the selection further in).
  const setTool = studio.setTool;
  studio.setTool = (id) => {
    const s = scene();
    const curves = new Set((s?.entities ?? []).map((e) => e.id));
    const chosen = KIND_OF[id] ? (studio.selection ?? []).filter((x) => curves.has(x) && !x.startsWith('edited:')) : [];
    const keep = KIND_OF[id] && KIND_OF[studio.tool] ? sources : chosen;
    reset(keep);
    if (KIND_OF[id] && keep.length) phase = 'reference';
    setTool(id);
    if (layer()) prompt();
  };
  const activate = studio.onActivate;
  studio.onActivate = (l) => {
    reset();
    activate?.(l);
  };
  const key = studio.onKey;
  studio.onKey = (event) => {
    if (kind() && !event.ctrlKey && !event.metaKey) {
      if (event.key === 'Enter' && phase === 'sources') {
        if (!sources.length) studio.say('Click at least one curve first.', true);
        else { phase = 'reference'; hover = null; prompt(); studio.redraw(); }
        return true;
      }
      if (event.key === 'Escape' && (open || phase === 'reference' || sources.length)) {
        if (open) close();
        else if (phase === 'reference') phase = 'sources';
        else sources = [];
        hover = null;
        prompt();
        studio.redraw();
        return true;
      }
    }
    return key?.(event) ?? false;
  };
}
