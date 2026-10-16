// Drawing in the sketch studio: the toolbar, gestures with live previews,
// snapping and automatic constraints, typed values, and the draft that is
// previewed natively and applied to the model.
import { primitiveGhost } from './canvas-tools.mjs';
import { newSketchDefinition } from './sketch-create.mjs';
import { infer } from './sketch-studio-infer.mjs';
import { pathData } from './sketch-studio-overlay.mjs';
import { CLICKS, TOOLS, commitShape, typedClick } from './sketch-studio-tools.mjs';

/** Snap radius in screen pixels. */
const SNAP_PIXELS = 8;
/** Clicks closer together than this, in time and pixels, are a double-click. */
const DOUBLE_CLICK_MS = 350;

/** An empty sketch definition on a local layer's plane, for its first draft. */
export function localDefinition(layer) {
  const create = layer.create;
  const definition = newSketchDefinition({
    id: layer.feature,
    shape: 'circle',
    width: 1,
    plane: create.plane,
    offset: create.offset,
    support: create.picked?.face_support?.input ?? '',
    face: create.picked ? 'picked' : '+z',
    picked: create.picked,
  });
  return { ...definition, points: [], lines: [], circles: [], profile: [], constraints: [] };
}

export function installDrawing(studio, document) {
  const { html } = studio;
  let gesture = null; // { tool, clicks: [{xy, point, rules}] }
  let cursor = null; // the latest inference under the cursor
  let reference = null; // the last line or circle hovered, for parallel and equal snaps
  let typed = '';
  let lastClick = { time: 0, screen: [NaN, NaN] };
  const options = { sides: 6, closed: false, flip: false };
  let previewTimer = 0;
  let previewTurn = 0;

  // ── Draft ────────────────────────────────────────────────────────────
  /** The draft of the active layer's sketch, created on first edit. */
  function draft() {
    const layer = studio.activeLayer();
    if (!layer) return null;
    if (studio.draft?.feature !== layer.feature) {
      if (studio.draft?.dirty) return null;
      const baseline = layer.local ? null : studio.definitions[layer.feature];
      if (!layer.local && !baseline) return null;
      studio.draft = {
        feature: layer.feature,
        local: layer.local,
        baseline,
        definition: structuredClone(baseline ?? localDefinition(layer)),
        undo: [],
        redo: [],
        dirty: false,
        pending: [],
        error: null,
        previewing: false,
      };
    }
    return studio.draft;
  }

  function change(next, ghostPaths = []) {
    const d = studio.draft;
    d.undo.push(d.definition);
    d.redo = [];
    d.definition = next;
    d.dirty = true;
    d.pending.push(...ghostPaths);
    studio.busy = true;
    schedulePreview();
    studio.refresh();
  }

  function schedulePreview() {
    clearTimeout(previewTimer);
    previewTimer = setTimeout(preview, 150);
  }

  async function send(apply) {
    const d = studio.draft;
    const features = studio.features;
    if (d.local) {
      return studio.api('/api/sketch-create', {
        method: 'POST',
        body: JSON.stringify({ version: studio.version, instance: studio.instance, expected: features, feature: d.feature, definition: d.definition, apply }),
      });
    }
    return studio.api('/api/sketch', {
      method: 'POST',
      body: JSON.stringify({ version: studio.version, instance: studio.instance, feature: d.feature, expected: d.baseline, definition: d.definition, apply }),
    });
  }

  async function preview() {
    const d = studio.draft;
    if (!d?.dirty) return;
    const turn = ++previewTurn;
    const definition = d.definition;
    d.previewing = true;
    studio.renderStatus();
    try {
      const response = await send(false);
      if (turn !== previewTurn || studio.draft !== d || d.definition !== definition) return;
      const scene = response.annotations?.scenes?.find((s) => s.kind === 'sketch' && s.feature === d.feature);
      if (scene) studio.previewScenes[d.feature] = scene;
      d.pending = [];
      d.error = response.can_apply ? null : response.error ?? 'The draft cannot be applied.';
      const layer = studio.layers.find((l) => l.feature === d.feature);
      if (layer) {
        layer.freeDegrees = scene?.solver?.free_degrees ?? layer.freeDegrees;
        layer.error = d.error;
      }
      studio.say(d.error ? `Preview: ${d.error}` : 'Draft solved and checked. Apply adds it to the model.', Boolean(d.error));
    } catch (error) {
      if (turn !== previewTurn) return;
      d.error = error.message;
      studio.say(`Preview failed: ${error.message}`, true);
    } finally {
      if (turn === previewTurn) d.previewing = false;
      studio.refresh();
    }
  }

  async function apply() {
    const d = studio.draft;
    if (!d?.dirty) return;
    clearTimeout(previewTimer);
    try {
      studio.say('Applying: regenerating every variant and checking requirements…');
      const response = await send(true);
      if (!response.applied) throw new Error(response.error ?? 'The draft was not applied.');
      discard(true);
      await studio.reload();
      studio.activate(d.feature);
      studio.say(`${d.feature} applied to the model.`);
    } catch (error) {
      studio.say(`Apply rejected: ${error.message}`, true);
    }
  }

  function discard(quiet = false) {
    const d = studio.draft;
    if (!d) return;
    delete studio.previewScenes[d.feature];
    studio.draft = null;
    studio.busy = false;
    cancelGesture();
    if (!quiet) {
      studio.say('Draft discarded.');
      studio.reload();
    }
    studio.refresh();
  }

  function undo() {
    const d = studio.draft;
    if (!d?.undo.length) return;
    d.redo.push(d.definition);
    d.definition = d.undo.pop();
    d.dirty = d.undo.length > 0;
    if (!d.dirty) {
      discard();
      return;
    }
    schedulePreview();
    studio.refresh();
  }

  function redo() {
    const d = studio.draft;
    if (!d?.redo.length) return;
    d.undo.push(d.definition);
    d.definition = d.redo.pop();
    d.dirty = true;
    schedulePreview();
    studio.refresh();
  }

  // ── Gestures ─────────────────────────────────────────────────────────
  function cancelGesture() {
    gesture = null;
    cursor = null;
    typed = '';
    studio.redraw();
  }

  function setTool(id) {
    if (!TOOLS.some((t) => t.id === id)) return;
    cancelGesture();
    studio.tool = id;
    const tool = TOOLS.find((t) => t.id === id);
    if (id !== 'select') {
      const layer = studio.activeLayer();
      if (!layer) studio.say('Choose or start a sketch layer to draw on.', true);
      else if (studio.draft?.dirty && studio.draft.feature !== layer.feature) studio.say(`Apply or discard the draft of ${studio.draft.feature} first.`, true);
      else studio.say(`${tool.label}: ${tool.hint} Hold Alt to draw without automatic constraints.`);
    } else {
      studio.say(tool.hint);
    }
    studio.refresh();
  }
  studio.setTool = setTool;

  /** Snap radius in sketch millimetres around a local point. */
  function tolerance(local) {
    const a = studio.toScreen(local);
    const b = studio.toScreen([local[0] + 1, local[1]]);
    if (!a || !b) return 0;
    const pixels = Math.hypot(b[0] - a[0], b[1] - a[1]);
    return pixels > 0 ? SNAP_PIXELS / pixels : 0;
  }

  /** The inferred click under the pointer, or null off the plane. */
  function inferAt(event) {
    const d = draft();
    const local = studio.localAt(event.clientX, event.clientY);
    if (!d || !local) return null;
    const tool = studio.tool;
    const anchor = gesture && ['line', 'circle'].includes(tool) && gesture.clicks.length === 1
      ? gesture.clicks[0]
      : gesture && tool === 'arc' && gesture.clicks.length === 1 ? gesture.clicks[0] : null;
    const result = infer({
      xy: local, tool, definition: d.definition, scene: studio.sceneOf(d.feature) ?? {},
      tolerance: tolerance(local), anchor, reference, auto: !event.altKey,
    });
    if (result.hovered) reference = result.hovered;
    return result;
  }

  function finishSpline() {
    if (gesture?.tool !== 'spline' || gesture.clicks.length < (options.closed ? 3 : 2)) return;
    commit(gesture.clicks);
  }

  function commit(clicks, auto = true) {
    const d = draft();
    const tool = gesture.tool;
    try {
      const profile = d.local && !(d.definition.profile ?? []).length;
      const result = commitShape(d.definition, tool, clicks, {
        profile, sides: options.sides, closed: options.closed, flip: options.flip, auto, scene: studio.sceneOf(d.feature) ?? {},
      });
      const ghost = tool === 'point' ? [] : [primitiveGhost(tool, clicks.slice(0, -1), clicks.at(-1), { clockwise: false, closed: options.closed, sides: options.sides })];
      change(result.definition, ghost);
      const added = result.constraints.map((c) => Object.keys(c)[0].replaceAll('_', ' '));
      studio.say(`${TOOLS.find((t) => t.id === tool).label} drawn${added.length ? `, with ${added.join(', ')}` : ''}. Previewing…`);
      gesture = result.chain ? { tool, clicks: [{ ...result.chain, rules: [] }] } : null;
    } catch (error) {
      studio.say(error.message, true);
      gesture = null;
    }
    typed = '';
    studio.redraw();
  }

  function click(event) {
    const d = draft();
    if (!d) {
      studio.say(studio.activeLayer() ? `Apply or discard the draft of ${studio.draft?.feature} first.` : 'Choose or start a sketch layer to draw on.', true);
      return true;
    }
    const result = inferAt(event);
    if (!result) {
      studio.say('That point is off the sketch plane. Turn the view toward the plane (N).', true);
      return true;
    }
    const now = performance.now();
    const double = now - lastClick.time < DOUBLE_CLICK_MS
      && Math.hypot(event.clientX - lastClick.screen[0], event.clientY - lastClick.screen[1]) < 5;
    lastClick = { time: now, screen: [event.clientX, event.clientY] };
    const tool = studio.tool;
    if (double && gesture) {
      if (tool === 'spline') finishSpline();
      else if (tool === 'line') cancelGesture();
      return true;
    }
    gesture ??= { tool, clicks: [] };
    const next = { xy: result.xy, point: result.point, rules: result.rules };
    if (tool === 'spline' && gesture.clicks.length >= 2 && result.point && result.point === gesture.clicks[0].point) {
      options.closed = true;
      finishSpline();
      return true;
    }
    if (tool === 'spline' && !result.point && gesture.clicks.length >= 2) {
      const first = studio.toScreen(gesture.clicks[0].xy);
      if (first && Math.hypot(first[0] - (event.clientX - studio.viewBox().left), first[1] - (event.clientY - studio.viewBox().top)) < SNAP_PIXELS) {
        options.closed = true;
        finishSpline();
        return true;
      }
    }
    gesture.clicks.push(next);
    if (gesture.clicks.length >= CLICKS[tool]) commit(gesture.clicks, !event.altKey);
    else studio.say(result.hint ? `Snapped ${result.hint}. ${nextPrompt(tool, gesture.clicks.length)}` : nextPrompt(tool, gesture.clicks.length));
    studio.redraw();
    return true;
  }

  function nextPrompt(tool, count) {
    if (tool === 'spline') return `${count} spline point${count === 1 ? '' : 's'}; double-click or Enter finishes.`;
    if (tool === 'arc') return count === 1 ? 'Click the arc start.' : 'Click the arc end (X flips the direction).';
    if (tool === 'ellipse') return count === 1 ? 'Click a major-axis end.' : 'Click the minor width.';
    if (tool === 'slot') return count === 1 ? 'Click the other cap centre.' : 'Click the width.';
    if (['line', 'circle', 'rectangle', 'polygon'].includes(tool)) return 'Click the next point, or type a value and press Enter.';
    return '';
  }

  // ── Overlay ──────────────────────────────────────────────────────────
  studio.decorate = ({ matrix, width, height, element }) => {
    const layer = studio.activeLayer();
    const nodes = [];
    if (!layer?.frame) return nodes;
    const d = studio.draft?.feature === layer.feature ? studio.draft : null;
    for (const path of d?.pending ?? []) {
      const data = pathData(path, layer.frame, matrix, width, height);
      if (data) nodes.push(element('path', { d: data, class: 'curve', stroke: layer.color, 'stroke-dasharray': '2 3' }));
    }
    if (studio.tool === 'select' || !cursor) return nodes;
    const clicks = gesture?.clicks ?? [];
    if (CLICKS[studio.tool] && (clicks.length || studio.tool === 'point')) {
      const ghost = primitiveGhost(studio.tool, clicks, cursor, { clockwise: options.flip, closed: options.closed, sides: options.sides });
      const data = pathData(ghost, layer.frame, matrix, width, height);
      if (data) nodes.push(element('path', { d: data, class: 'ghost' }));
    }
    const at = studio.toScreen(cursor.xy);
    if (at) {
      const snapped = cursor.point || cursor.rules.length;
      nodes.push(element('circle', { cx: at[0], cy: at[1], r: snapped ? 6 : 3, fill: 'none', stroke: 'var(--ghost)', 'stroke-width': 2 }));
      const text = [cursor.hint, typed && `= ${typed}`].filter(Boolean).join('  ');
      if (text) {
        const label = element('text', { x: at[0] + 12, y: at[1] - 10, fill: 'var(--ghost)', 'font-size': 12, 'paint-order': 'stroke', stroke: 'var(--view)', 'stroke-width': 3 });
        label.textContent = text;
        nodes.push(label);
      }
    }
    return nodes;
  };

  // ── Pointer and keys ─────────────────────────────────────────────────
  studio.viewBox = () => document.getElementById('overlay').getBoundingClientRect();
  document.getElementById('canvas').addEventListener('pointerleave', () => {
    cursor = null;
    studio.redraw();
  });
  for (const tool of TOOLS) {
    if (!CLICKS[tool.id]) continue;
    studio.tools.set(tool.id, {
      // Shift-drag pans while drawing.
      down: (event) => event.button === 0 && !event.shiftKey && click(event),
      move: (event) => {
        cursor = inferAt(event);
        studio.redraw();
        studio.renderStatus();
      },
    });
  }

  studio.onKey = (event) => {
    const mod = event.ctrlKey || event.metaKey;
    if (mod && event.key.toLowerCase() === 'z') {
      event.preventDefault();
      if (event.shiftKey) redo();
      else undo();
      return true;
    }
    if (mod && event.key.toLowerCase() === 'y') {
      event.preventDefault();
      redo();
      return true;
    }
    if (mod) return false;
    if (gesture && /^[0-9.,\- ]$/.test(event.key) && gesture.clicks.length) {
      typed += event.key;
      studio.redraw();
      return true;
    }
    if (gesture && event.key === 'Backspace' && typed) {
      typed = typed.slice(0, -1);
      studio.redraw();
      return true;
    }
    if (event.key === 'Enter') {
      if (typed && gesture && cursor) {
        const next = typedClick(gesture.tool, gesture.clicks.at(-1), cursor.xy, typed);
        if (!next) {
          studio.say(`"${typed}" does not fit the ${gesture.tool} tool.`, true);
          typed = '';
        } else {
          gesture.clicks.push(next);
          if (gesture.clicks.length >= CLICKS[gesture.tool]) commit(gesture.clicks);
        }
        studio.redraw();
        return true;
      }
      finishSpline();
      return true;
    }
    if (event.key === 'Escape') {
      if (gesture) cancelGesture();
      else studio.setTool('select');
      return true;
    }
    if ((event.key === 'x' || event.key === 'X') && studio.tool === 'arc') {
      options.flip = !options.flip;
      studio.renderToolbar();
      studio.redraw();
      return true;
    }
    const tool = TOOLS.find((t) => t.key.toLowerCase() === event.key.toLowerCase());
    if (tool) {
      studio.setTool(tool.id);
      return true;
    }
    return false;
  };

  // ── Toolbar ──────────────────────────────────────────────────────────
  studio.renderToolbar = () => {
    const bar = document.getElementById('tools');
    studio.fill(bar, TOOLS.map((tool) => html('button', {
      'aria-pressed': studio.tool === tool.id ? 'true' : 'false',
      title: `${tool.label} (${tool.key}) — ${tool.hint}`,
      onclick: () => studio.setTool(tool.id),
    }, tool.label, html('kbd', {}, tool.key))));
    const extra = [];
    if (studio.tool === 'polygon') {
      extra.push(html('label', { style: 'display:flex;gap:4px;align-items:center' }, 'Sides',
        html('input', { type: 'number', min: 3, max: 1000, step: 1, value: options.sides, style: 'width:64px',
          onchange: (e) => { options.sides = Number(e.target.value); studio.redraw(); } })));
    }
    if (studio.tool === 'spline') {
      extra.push(html('label', { style: 'display:flex;gap:4px;align-items:center' },
        html('input', { type: 'checkbox', checked: options.closed, style: 'width:auto', onchange: (e) => { options.closed = e.target.checked; studio.redraw(); } }), 'Closed'),
      html('button', { onclick: finishSpline }, 'Finish'));
    }
    if (studio.tool === 'arc') {
      extra.push(html('button', { 'aria-pressed': options.flip ? 'true' : 'false', title: 'Take the longer way round (X)', onclick: () => { options.flip = !options.flip; studio.renderToolbar(); studio.redraw(); } }, 'Flip', html('kbd', {}, 'X')));
    }
    if (extra.length) bar.append(html('span', { style: 'width:8px' }), ...extra);

    const d = studio.draft;
    const actions = document.getElementById('draft-actions');
    studio.fill(actions,
      html('button', { title: 'Undo (Ctrl+Z)', disabled: !d?.undo.length, onclick: undo }, 'Undo'),
      html('button', { title: 'Redo (Ctrl+Shift+Z)', disabled: !d?.redo.length, onclick: redo }, 'Redo'),
      html('button', { disabled: !d?.dirty, onclick: () => discard() }, 'Discard'),
      html('button', { 'aria-pressed': d?.dirty && !d.error ? 'true' : 'false', disabled: !d?.dirty || d.previewing || Boolean(d.error), title: d?.error ?? 'Apply the draft to the model', onclick: apply }, 'Apply'));
  };

  studio.statusExtra = () => {
    const d = studio.draft;
    if (!d?.dirty) return null;
    return html('span', {}, d.previewing ? 'Draft · previewing…' : d.error ? 'Draft · cannot apply' : `Draft · ${d.undo.length} change${d.undo.length === 1 ? '' : 's'}, not applied`);
  };

  studio.onActivate = (layer) => {
    cancelGesture();
    if (layer && studio.draft?.dirty && studio.draft.feature !== layer.feature) {
      studio.say(`The draft of ${studio.draft.feature} is still open. Apply or discard it before drawing on ${layer.feature}.`);
    }
  };

  // Shared with the constraint tools (phase 3).
  studio.ensureDraft = draft;
  studio.changeDraft = change;
  studio.cancelGesture = cancelGesture;

  studio.renderToolbar();
  return { setTool, apply, discard, undo, redo, commit: (clicks) => { gesture = { tool: studio.tool, clicks: [] }; commit(clicks); } };
}
