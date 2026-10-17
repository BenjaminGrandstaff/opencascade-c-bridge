// Modify tools of the sketch studio: Fillet and Chamfer round or cut a
// corner of the outer profile, and Extend runs a curve's end on to the next
// curve it meets. They record the model's own edits (profile fillet and
// chamfer operations, extend-to curve operations), so the source geometry
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
  const inProfile = new Set(profileCurves(definition));
  const uses = new Map();
  for (const entity of scene?.entities ?? []) {
    if (!inProfile.has(entity.id) || entity.point_ids?.length !== 2) continue;
    const [a, b] = entity.point_ids;
    if (a === b) continue;
    for (const id of [a, b]) uses.set(id, (uses.get(id) ?? 0) + 1);
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
  next.profile_operations = [...operations, operation];
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

/** The modify tools as palette entries (see sketch-studio-tools.mjs). */
export const MODIFY_TOOLS = [
  { id: 'fillet', label: 'Fillet', key: 'I', hint: 'Click a corner of the outline, then type a radius or parameter and press Enter.' },
  { id: 'chamfer', label: 'Chamfer', key: 'H', hint: 'Click a corner of the outline, then type one distance, or two (incoming, outgoing), and press Enter.' },
  { id: 'extend', label: 'Extend', key: 'W', hint: 'Click near the end of a curve to run it on to the next curve it meets.' },
];

export const MODIFY_ICONS = {
  fillet: '<path d="M4 4v9a7 7 0 0 0 7 7h9"/><path d="M4 13V20h7" stroke-dasharray="2 2" opacity=".6"/>',
  chamfer: '<path d="M4 4v10l6 6h10"/><path d="M4 14v6h6" stroke-dasharray="2 2" opacity=".6"/>',
  extend: '<path d="M3 12h9"/><path d="M12 12h6" stroke-dasharray="2 2"/><path d="M20 4v16"/><path d="M15 9l3 3-3 3"/>',
};

/**
 * Installs Fillet, Chamfer and Extend: hover highlights the corner or the
 * curve end and its continuation, a click records the edit in the draft
 * (corners first ask for their value in a box by the corner).
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
    #modify-box span { color: var(--muted); }`));

  const last = { fillet: '2', chamfer: '2' };
  let hover = null; // { corner, at } | { entity, end, reach }
  let open = null; // { kind, point }

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

  // Hover marks: the corner ring, or the continuation up to where it meets.
  const decorate = studio.decorate;
  studio.decorate = (context) => {
    const nodes = decorate(context);
    const l = layer();
    if (!hover || !l?.frame || !['fillet', 'chamfer', 'extend'].includes(studio.tool)) return nodes;
    const { element } = context;
    if (hover.corner) {
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
