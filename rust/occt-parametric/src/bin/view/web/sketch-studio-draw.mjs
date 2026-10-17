// Drawing in the sketch studio: the toolbar, gestures with live previews,
// snapping and automatic constraints, typed values, and the draft that is
// previewed natively and applied to the model.
import {studioGhost as primitiveGhost} from './sketch-studio-tools.mjs';
import { newSketchDefinition } from './sketch-create.mjs';
import { infer } from './sketch-studio-infer.mjs';
import { pathData } from './sketch-studio-overlay.mjs';
import { CLICKS, ICONS, TOOLS, commitShape, typedClick } from './sketch-studio-tools.mjs';
import {captureGeneratedPoints, bindGeneratedPoints, inheritGeneratedReferences} from './generated-points.mjs';
import { continueOutline } from './sketch-studio-delete.mjs';

/** Snap radius in screen pixels. */
const SNAP_PIXELS = 8;
/** Clicks closer together than this, in time and pixels, are a double-click. */
const DOUBLE_CLICK_MS = 350;

/** An empty sketch definition on a local layer's plane, for its first draft. */
export function localDefinition(layer) {
  const create = layer.create;
  if (create.plane === 'datum') {
    // On a datum plane the kernel takes the origin and normal from the
    // plane and keeps the x axis projected into it.
    const vector = (v, dimension) => Object.fromEntries(['x', 'y', 'z'].map((key, i) => [key, {
      value: v[i], dimension, unit: dimension === 'length' ? 'millimeter' : null,
    }]));
    return {
      id: layer.feature, acceptance: 'solved', datum_plane: create.datum,
      origin: { literal: vector(create.frame.origin_mm, 'length') },
      x_axis: { literal: vector(create.frame.x_axis, 'scalar') },
      y_axis: { literal: vector(create.frame.y_axis, 'scalar') },
      points: [], lines: [], circles: [], profile: [], constraints: [],
    };
  }
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

// Native SketchDefinition defaults omit these empty collections. Compare
// meaning rather than JSON property order when deciding whether Preview is needed.
const DEFAULT_COLLECTIONS = new Set(['circles', 'arcs', 'profile', 'constraints', 'projections', 'ellipses',
  'profile_operations', 'splines', 'holes', 'shapes', 'measurements', 'regions', 'curve_operations', 'region_edits', 'texts', 'patterns']);
export function sameSketchDefinition(first, second) {
  const canonical = sketch => Object.fromEntries(Object.entries(sketch ?? {}).filter(([key, value]) =>
    !(DEFAULT_COLLECTIONS.has(key) && Array.isArray(value) && !value.length) &&
    !(key === 'acceptance' && value === 'solved') &&
    !(['datum_plane', 'face_support', 'region_discovery'].includes(key) && value === null)));
  const same = (a, b) => {
    if (a === b) return true;
    if (!a || !b || typeof a !== 'object' || typeof b !== 'object' || Array.isArray(a) !== Array.isArray(b)) return false;
    const keys = Object.keys(a);
    return keys.length === Object.keys(b).length && keys.every(key => Object.hasOwn(b, key) && same(a[key], b[key]));
  };
  return same(canonical(first), canonical(second));
}

export function installDrawing(studio, document) {
  const { html } = studio;
  let gesture = null; // { tool, clicks: [{xy, point, rules}] }
  let cursor = null; // the latest inference under the cursor
  let reference = null; // the last line or circle hovered, for parallel and equal snaps
  let typed = '';
  let lastClick = { time: 0, screen: [NaN, NaN] };
  const options = {sides:6,closed:false,flip:false,degree:3,weights:'',rho:.5,text:'Text',font:'Liberation Sans',style:'regular',height:5,angle:0,align:'start'};
  let previewTimer = 0;
  let previewTurn = 0;
  let draftRevision = 0;

  // A clean baseline can still own redo entries. Keep that local session
  // separate from accepted model history until Apply or explicit Discard.
  const hasDraftHistory = () => Boolean(studio.draft &&
    (studio.draft.dirty || studio.draft.undo.length || studio.draft.redo.length));
  studio.hasDraftHistory = hasDraftHistory;

  // A revision also detects Undo/Redo back to the same immutable definition.
  // Read-only solver requests may finish only in the context they started in.
  studio.draftCheckpoint = () => {
    const d = studio.draft, definition = d?.definition, revision = draftRevision;
    const version = studio.version, instance = studio.instance, feature = studio.activeLayer()?.feature;
    return () => !studio.modelPending && studio.draft === d && d?.definition === definition &&
      draftRevision === revision && studio.version === version && studio.instance === instance &&
      studio.activeLayer()?.feature === feature;
  };

  function beginModelOperation(label) {
    if (studio.modelPending) return false;
    studio.modelPending = label;
    studio.modelEpoch = (studio.modelEpoch ?? 0) + 1;
    studio.busy = true;
    draftRevision++;
    clearTimeout(previewTimer);
    previewTurn++;
    if (studio.draft) studio.draft.previewing = false;
    cancelGesture();
    studio.refresh();
    return true;
  }

  function endModelOperation() {
    studio.modelPending = null;
    studio.busy = hasDraftHistory();
    studio.refresh();
  }

  // Serialize external feature mutations with sketch Apply/history/Save.
  studio.runModelOperation = async (label, operation) => {
    if (!beginModelOperation(label)) throw Error('Wait for the current model operation.');
    try { return await operation(); }
    finally { endModelOperation(); }
  };

  // ── Draft ────────────────────────────────────────────────────────────
  /** The draft of the active layer's sketch, created on first edit. */
  function draft() {
    if (studio.modelPending) return null;
    const layer = studio.activeLayer();
    if (!layer) return null;
    if (studio.draft?.feature !== layer.feature) {
      if (hasDraftHistory()) return null;
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
      if (baseline) inheritGeneratedReferences(baseline, studio.draft.definition);
    }
    return studio.draft;
  }

  function change(next, ghostPaths = []) {
    const d = studio.draft;
    if (studio.modelPending || !d) return false;
    draftRevision++;
    inheritGeneratedReferences(d.definition, next);
    d.undo.push(d.definition);
    d.redo = [];
    d.definition = next;
    d.dirty = true;
    d.pending.push(...ghostPaths);
    studio.busy = true;
    schedulePreview();
    studio.refresh();
    return true;
  }

  function schedulePreview() {
    previewTurn++; // Invalidate the previous revision before the debounce expires.
    if (studio.draft) { studio.draft.error = null; studio.draft.previewing = true; }
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
    if (!d?.dirty || studio.modelPending) return;
    if (!d.local && sameSketchDefinition(d.definition, d.baseline)) {
      clearDraftPreview(d);
      d.dirty = false;
      studio.say('The draft matches the starting sketch. Undo keeps your local edit history; Discard returns to model history.');
      studio.refresh();
      return;
    }
    const turn = ++previewTurn;
    const definition = d.definition;
    d.previewing = true;
    studio.refresh();
    const current = () => turn === previewTurn && studio.draft === d && d.definition === definition && !studio.modelPending;
    try {
      const response = await send(false);
      if (!current()) return;
      if (response.can_apply) {
        bindGeneratedPoints(definition, captureGeneratedPoints(definition, response.generated_points ?? [], response.generated_curves ?? {}));
      }
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
      if (!current()) return;
      d.error = error.message;
      studio.say(`Preview failed: ${error.message}`, true);
    } finally {
      if (current()) { d.previewing = false; studio.refresh(); }
    }
  }

  async function apply() {
    const d = studio.draft;
    if (!d?.dirty || !beginModelOperation('Applying')) return;
    let applied = false;
    try {
      studio.say('Applying: regenerating every variant and checking requirements…');
      const response = await send(true);
      if (!response.applied) throw new Error(response.error ?? 'The draft was not applied.');
      applied = true;
      releaseDraft();
      await studio.reload();
      studio.activate(d.feature);
      studio.say(`${d.feature} applied to the model.`);
    } catch (error) {
      studio.say(applied ? `${d.feature} applied, but refreshing the view failed: ${error.message}. Reload the page.` : `Apply rejected: ${error.message}`, true);
    } finally {
      endModelOperation();
    }
  }

  // Invalidate outstanding previews before restoring a history snapshot. A
  // response from an undone edit must not restore its geometry or error.
  function clearDraftPreview(d) {
    clearTimeout(previewTimer);
    previewTurn++;
    d.previewing = false;
    d.error = null;
    d.pending = [];
    delete studio.previewScenes[d.feature];
    const accepted = studio.scenes?.find(s => s.kind === 'sketch' && s.feature === d.feature);
    const layer = studio.layers?.find(l => l.feature === d.feature);
    if (layer) { layer.freeDegrees = accepted?.solver?.free_degrees ?? null; layer.error = null; }
  }

  // O(1) snapshot navigation; restoring presentation also scans sketch layers.
  function restoreDraftHistory(d, action) {
    draftRevision++;
    clearDraftPreview(d);
    cancelGesture();
    d.dirty = d.undo.length > 0;
    studio.busy = true; // Preserve the baseline and redo stack across polling.
    if (studio.activeLayer()?.feature !== d.feature) studio.activate(d.feature);
    if (d.dirty) {
      schedulePreview();
      studio.say(`${action} draft change. Previewing…`);
    } else {
      studio.say('At the starting sketch. Redo restores your changes; Discard returns to model history.');
    }
    studio.refresh();
  }

  function releaseDraft() {
    draftRevision++;
    const d = studio.draft;
    if (!d) return;
    clearDraftPreview(d);
    studio.draft = null;
    studio.busy = Boolean(studio.modelPending);
    cancelGesture();
  }

  function discard(quiet = false) {
    if (studio.modelPending) return;
    releaseDraft();
    if (!quiet) {
      studio.say('Draft discarded.');
      studio.reload();
    }
    studio.refresh();
  }

  /** Undo or redo a model change through the server's history. */
  async function modelHistory(direction) {
    if (!beginModelOperation(direction === 'undo' ? 'Undoing' : 'Redoing')) return;
    try {
      await studio.api(`/api/${direction}`, { method: 'POST', body: JSON.stringify({version:studio.version}) });
      if (studio.draft) releaseDraft(); // Drop an idle baseline from before model navigation.
      await studio.reload();
      studio.say(direction === 'undo' ? 'Model change undone.' : 'Model change redone.');
    } catch (error) {
      studio.say(error.message, true);
    } finally {
      endModelOperation();
    }
  }

  async function save() {
    if (studio.modelPending) return;
    if (hasDraftHistory()) { studio.say('Apply or discard the studio draft before saving the model.'); return; }
    if (!beginModelOperation('Saving')) return;
    try {
      await studio.api('/api/save', { method: 'POST', body: '{}' });
      await studio.reload();
      studio.say('Model saved to its file.');
    } catch (error) {
      studio.say(`Save failed: ${error.message}`, true);
    } finally {
      endModelOperation();
    }
  }

  function undo() {
    if (studio.modelPending) return;
    const d = studio.draft;
    if (!hasDraftHistory()) {
      if (studio.modelState?.undo_count) modelHistory('undo');
      return;
    }
    if (!d?.undo.length) return;
    d.redo.push(d.definition);
    d.definition = d.undo.pop();
    restoreDraftHistory(d, 'Undid');
  }

  function redo() {
    if (studio.modelPending) return;
    const d = studio.draft;
    if (!hasDraftHistory()) {
      if (studio.modelState?.redo_count) modelHistory('redo');
      return;
    }
    if (!d?.redo.length) return;
    d.undo.push(d.definition);
    d.definition = d.redo.pop();
    restoreDraftHistory(d, 'Redid');
  }

  // ── Gestures ─────────────────────────────────────────────────────────
  function cancelGesture() {
    studio.cancelSelectionGesture?.();
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
      else if (hasDraftHistory() && studio.draft.feature !== layer.feature) studio.say(`Apply or discard the draft of ${studio.draft.feature} first.`, true);
      else studio.say(`${tool.label}: ${tool.hint}${CLICKS[id] ? ' Hold Alt to draw without automatic constraints.' : ''}`);
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
    if (!['spline','control_spline'].includes(gesture?.tool) || gesture.clicks.length < (gesture.tool==='control_spline'?Math.max(options.degree+1,options.closed?3:2):options.closed?3:2)) return;
    commit(gesture.clicks);
  }

  function commit(clicks, auto = true) {
    const d = draft();
    if (!d || !gesture) return;
    const tool = gesture.tool;
    try {
      const profile = d.local && !(d.definition.profile ?? []).length;
      const result = commitShape(d.definition, tool, clicks, {
        ...options,profile,auto,scene: studio.sceneOf(d.feature) ?? {},
      });
      const ghost = tool === 'point' ? [] : [primitiveGhost(tool, clicks.slice(0, -1), clicks.at(-1), {...options,clockwise:options.flip})];
      change(continueOutline(d.definition, result.definition), ghost);
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
      if (['spline','control_spline'].includes(tool)) finishSpline();
      else if (tool === 'line') cancelGesture();
      return true;
    }
    gesture ??= { tool, clicks: [] };
    const next = { xy: result.xy, point: result.point, rules: result.rules };
    if (['spline','control_spline'].includes(tool) && gesture.clicks.length >= 2 && result.point && result.point === gesture.clicks[0].point) {
      options.closed = true;
      finishSpline();
      return true;
    }
    if (['spline','control_spline'].includes(tool) && !result.point && gesture.clicks.length >= 2) {
      const first = studio.toScreen(gesture.clicks[0].xy);
      if (first && Math.hypot(first[0] - (event.clientX - studio.viewBox().left), first[1] - (event.clientY - studio.viewBox().top)) < SNAP_PIXELS) {
        options.closed = true;
        finishSpline();
        return true;
      }
    }
    if(tool==='control_spline'&&gesture.clicks.length>=(options.closed?255:256)){studio.say('Finish the control spline: at most 256 poles including its closing pole.',true);return true;}
    gesture.clicks.push(next);
    if (gesture.clicks.length >= CLICKS[tool]) commit(gesture.clicks, !event.altKey);
    else studio.say(result.hint ? `Snapped ${result.hint}. ${nextPrompt(tool, gesture.clicks.length)}` : nextPrompt(tool, gesture.clicks.length));
    studio.redraw();
    return true;
  }

  function nextPrompt(tool, count) {
    if(tool==='control_spline')return `${count} control poles; degree ${options.degree} needs ${options.degree+1} or more. Enter finishes.`;
    if(tool==='elliptical_arc')return ['','Click the major-axis end.','Click minor width.','Click the arc start direction.','Click the arc end direction.'][count];
    if(tool==='conic')return count===1?'Click the intersection of end tangents.':'Click the conic end.';
    if(tool==='arc_three')return count===1?'Click a point the arc passes through.':'Click the arc end.';
    if (tool === 'spline') return `${count} spline point${count === 1 ? '' : 's'}; double-click or Enter finishes.`;
    if (tool === 'arc') return count === 1 ? 'Click the arc start.' : 'Click the arc end (X flips the direction).';
    if (tool === 'ellipse') return count === 1 ? 'Click a major-axis end.' : 'Click the minor width.';
    if (tool === 'slot') return count === 1 ? 'Click the other cap centre.' : 'Click the width.';
    if (['line', 'circle', 'rectangle','center_rectangle', 'polygon'].includes(tool)) return 'Click the next point, or type a value and press Enter.';
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
      const ghost = primitiveGhost(studio.tool, clicks, cursor, {...options,clockwise:options.flip});
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
          if(tool==='control_spline'&&gesture.clicks.length>=(options.closed?255:256)){studio.say('Finish the control spline: at most 256 poles including its closing pole.',true);return true;}
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
    if ((event.key === 'x' || event.key === 'X') && ['arc','elliptical_arc'].includes(studio.tool)) {
      options.flip = !options.flip;
      studio.renderToolbar();
      studio.redraw();
      return true;
    }
    const tool = TOOLS.find((t) => t.key && t.key.toLowerCase() === event.key.toLowerCase());
    if (tool) {
      studio.setTool(tool.id);
      return true;
    }
    return false;
  };

  // ── Toolbar ──────────────────────────────────────────────────────────
  studio.renderToolbar = () => {
    // A side palette: each tool as its shape's icon over its name, in
    // Edit (select, dimension), Draw, Modify and Pattern groups.
    const button = (tool) => {
      const icon = html('span', { 'aria-hidden': 'true' });
      icon.innerHTML = `<svg viewBox="0 0 24 24">${ICONS[tool.id]}</svg>`;
      return html('button', {
        'aria-pressed': studio.tool === tool.id ? 'true' : 'false',
        title: `${tool.label}${tool.key ? ` (${tool.key})` : ''} — ${tool.hint}`,
        onclick: () => studio.setTool(tool.id),
      }, icon, tool.label, tool.key ? html('kbd', {}, tool.key) : '');
    };
    const edit = TOOLS.filter((tool) => !CLICKS[tool.id] && !tool.group);
    const draw = TOOLS.filter((tool) => CLICKS[tool.id]);
    const modify = TOOLS.filter((tool) => tool.group === 'modify');
    const pattern = TOOLS.filter((tool) => tool.group === 'pattern');
    studio.fill(document.getElementById('tools'),
      html('h2', {}, 'Edit'), html('div', { class: 'palette' }, edit.map(button)),
      html('h2', {}, 'Draw'), html('div', { class: 'palette' }, draw.map(button)),
      html('h2', {}, 'Modify'), html('div', { class: 'palette' }, modify.map(button)),
      html('h2', {}, 'Pattern'), html('div', { class: 'palette' }, pattern.map(button)));
    const extra = [];
    if (studio.tool === 'polygon') {
      extra.push(html('label', {}, 'Sides',
        html('input', { type: 'number', min: 3, max: 1000, step: 1, value: options.sides, style: 'width:64px',
          onchange: (e) => { options.sides = Number(e.target.value); studio.redraw(); } })));
    }
    if (['spline','control_spline'].includes(studio.tool)) {
      extra.push(html('label', {},
        html('input', { type: 'checkbox', checked: options.closed, style: 'width:auto', onchange: (e) => { options.closed = e.target.checked; studio.redraw(); } }), 'Closed'),
      html('button', { onclick: finishSpline }, 'Finish'));
    }
    if (['arc','elliptical_arc'].includes(studio.tool)) {
      extra.push(html('button', { 'aria-pressed': options.flip ? 'true' : 'false', title: 'Take the longer way round (X)', onclick: () => { options.flip = !options.flip; studio.renderToolbar(); studio.redraw(); } }, 'Flip', html('kbd', {}, 'X')));
    }
    const field=(label,key,type='number')=>html('label',{},label,html('input',{type,value:options[key],step:'any','aria-label':label,oninput:e=>{options[key]=type==='number'?(e.target.value.trim()?Number(e.target.value):NaN):e.target.value;studio.redraw();}}));
    const choice=(label,key,values)=>html('label',{},label,html('select',{'aria-label':label,onchange:e=>{options[key]=e.target.value;studio.redraw();}},values.map(([value,text])=>html('option',{value,selected:options[key]===value},text))));
    if(studio.tool==='control_spline')extra.push(field('Spline degree','degree'),field('Pole weights (optional)','weights','text'),html('p',{},'Degree 1–5. One positive weight per pole; empty uses 1. Closed joins at a C0 seam.'));
    if(studio.tool==='conic')extra.push(field('Conic rho (0–1)','rho'),html('p',{},'Below 0.5: elliptic; 0.5: parabolic; above 0.5: hyperbolic.'));
    if(studio.tool==='text')extra.push(field('Text content','text','text'),field('Font family','font','text'),choice('Text style','style',[['regular','Regular'],['bold','Bold'],['italic','Italic'],['bold_italic','Bold italic']]),field('Text height (mm)','height'),field('Text angle (degrees)','angle'),choice('Text alignment','align',[['start','Start'],['center','Center'],['end','End']]),html('p',{},'Fonts must be installed. Preview verifies the family and style; glyphs are native outlines.'));
    studio.fill(document.getElementById('tool-options'), extra);

    const d = studio.draft;
    const actions = document.getElementById('draft-actions');
    studio.fill(actions,
      html('button', {
        title: hasDraftHistory() ? 'Undo in the draft (Ctrl+Z)' : 'Undo the last change to the model (Ctrl+Z)',
        disabled: Boolean(studio.modelPending) || (hasDraftHistory() ? !d.undo.length : !studio.modelState?.undo_count), onclick: undo,
      }, 'Undo'),
      html('button', {
        title: hasDraftHistory() ? 'Redo in the draft (Ctrl+Shift+Z)' : 'Redo a model change (Ctrl+Shift+Z)',
        disabled: Boolean(studio.modelPending) || (hasDraftHistory() ? !d.redo.length : !studio.modelState?.redo_count), onclick: redo,
      }, 'Redo'),
      html('button', { disabled: Boolean(studio.modelPending) || !hasDraftHistory(), title: 'Clear the local draft and its Undo/Redo history; keep the accepted model', onclick: () => discard() }, 'Discard'),
      html('button', { 'aria-pressed': d?.dirty && !d.error ? 'true' : 'false', disabled: Boolean(studio.modelPending) || !d?.dirty || d.previewing || Boolean(d.error), title: d?.error ?? 'Apply the draft to the model', onclick: apply }, 'Apply'),
      html('button', {
        title: studio.modelState?.dirty ? 'Write the model to its file' : 'The model file is up to date',
        disabled: Boolean(studio.modelPending) || !studio.modelState?.dirty || hasDraftHistory(), onclick: save,
      }, 'Save'));
  };

  studio.statusExtra = () => {
    const d = studio.draft;
    if (studio.modelPending) return html('span', {}, `${studio.modelPending}…`);
    if (!hasDraftHistory()) return null;
    if (!d.dirty) return html('span', {}, `Draft · at starting sketch · ${d.redo.length ? 'Redo' : 'Undo'} available`);
    return html('span', {}, d.previewing ? 'Draft · previewing…' : d.error ? 'Draft · cannot apply' : `Draft · ${d.undo.length} change${d.undo.length === 1 ? '' : 's'}, not applied`);
  };

  studio.onActivate = (layer) => {
    draftRevision++;
    cancelGesture();
    if (layer && hasDraftHistory() && studio.draft.feature !== layer.feature) {
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
