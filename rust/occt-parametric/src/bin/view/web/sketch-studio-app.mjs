// The sketch studio page: sketches drawn on their planes in the 3D view, as
// layers, over the same server API the classic sketch editor uses.
import { createRenderer, fitCamera, pan, parseGltf, ray, viewProjection } from './viewer.mjs';
import {
  basePlane, lookAt, pickFace, planeNormal, projectLocal, rayToLocal,
} from './sketch-studio-plane.mjs';
import { PALETTE, layersFrom, nextFeatureName, planeLabel } from './sketch-studio-layers.mjs';
import { localBounds, planeGrid, sketchPaths, sketchPoints } from './sketch-studio-overlay.mjs';
import { installDrawing } from './sketch-studio-draw.mjs';
import { installConstraints } from './sketch-studio-constrain.mjs';
import { layoutLabels, sketchLabels } from './sketch-studio-labels.mjs';
import { installExtras } from './sketch-studio-extras.mjs';

const SVG = 'http://www.w3.org/2000/svg';
const format = (value) => Number(Number(value).toPrecision(6)).toString();
const vectorText = (v) => `(${v.map(format).join(', ')})`;
/** Numbers of a literal vector expression, as face picks return axes. */
const literalVector = (expression) => ['x', 'y', 'z'].map((key) => expression.literal[key].value);

export async function api(path, options = {}) {
  const response = await fetch(path, {
    ...options,
    headers: { 'Content-Type': 'application/json', 'X-OCCT-View': '1' },
  });
  const body = await response.json();
  if (!response.ok) throw new Error(body.error ?? response.statusText);
  return body;
}

/** Starts the studio on `document`; returns its controller for later phases and tests. */
export function startStudio(document) {
  const $ = (id) => document.getElementById(id);
  const canvas = $('canvas');
  const overlay = $('overlay');
  let renderer = null;
  try {
    renderer = createRenderer(canvas);
  } catch (error) {
    $('empty').hidden = false;
    $('empty').textContent = `3D view unavailable: ${error.message}`;
  }

  const studio = {
    version: null,
    instance: null,
    parsed: null,
    scenes: [],
    /** Solved scenes of unapplied drafts, by feature; they replace the accepted ones. */
    previewScenes: {},
    definitions: {},
    features: [],
    layers: [],
    active: null,
    camera: fitCamera(null),
    message: 'Loading the model…',
    error: false,
    picking: false,
    newSketch: null,
    busy: false,
    tools: new Map(),
    tool: 'select',
    /** Extra SVG drawn by the active tool (phase 2 and later). */
    decorate: () => [],
  };

  const aspect = () => canvas.clientWidth / Math.max(1, canvas.clientHeight);
  const size = () => [overlay.clientWidth || canvas.clientWidth, overlay.clientHeight || canvas.clientHeight];

  function say(message, error = false) {
    studio.message = message;
    studio.error = error;
    renderStatus();
  }

  const sceneOf = (feature) =>
    studio.previewScenes[feature] ?? studio.scenes.find((s) => s.kind === 'sketch' && s.feature === feature);
  const activeLayer = () => studio.layers.find((layer) => layer.feature === studio.active) ?? null;
  studio.activeLayer = activeLayer;
  studio.sceneOf = sceneOf;
  studio.say = say;
  studio.api = api;

  async function load() {
    const state = await api('/api/state');
    const { instances } = await api('/api/instances');
    if (!instances.length) throw new Error('The model has no editable instance.');
    if (!instances.some((i) => i.id === studio.instance)) studio.instance = instances[0].id;
    const [gltf, annotations, sketches] = await Promise.all([
      fetch('/api/model.gltf').then((r) => r.json()),
      api(`/api/annotations?id=${encodeURIComponent(studio.instance)}`),
      api('/api/sketches'),
    ]);
    const first = studio.parsed === null;
    studio.version = state.version;
    studio.modelState = state;
    studio.parsed = parseGltf(gltf);
    renderer?.setModel(studio.parsed);
    studio.scenes = annotations.scenes ?? [];
    studio.features = sketches.features ?? [];
    studio.definitions = Object.fromEntries((sketches.sketches ?? []).map((s) => [s.feature, s.definition]));
    studio.layers = layersFrom(studio.scenes, studio.definitions, studio.layers);
    if (!studio.layers.some((layer) => layer.feature === studio.active)) {
      studio.active = studio.layers[0]?.feature ?? null;
    }
    if (first) studio.camera = fitCamera(studio.parsed.bounds, aspect());
    document.title = `Sketch studio · ${state.family}`;
    studio.onLoad?.();
    refresh();
  }
  studio.reload = load;

  // ── Drawing ─────────────────────────────────────────────────────────
  function element(name, attributes, parent) {
    const node = document.createElementNS(SVG, name);
    for (const [key, value] of Object.entries(attributes)) node.setAttribute(key, value);
    parent?.append(node);
    return node;
  }
  studio.svg = element;

  function draw() {
    renderer?.draw(studio.camera);
    const [width, height] = size();
    overlay.setAttribute('viewBox', `0 0 ${width} ${height}`);
    overlay.replaceChildren();
    const matrix = viewProjection(studio.camera, width / height);
    const active = activeLayer();
    if (active?.frame) {
      const scene = sceneOf(active.feature);
      const bounds = localBounds(scene) ?? { min: [-25, -25], max: [25, 25] };
      const margin = Math.max(10, 0.5 * Math.max(bounds.max[0] - bounds.min[0], bounds.max[1] - bounds.min[1]));
      const grid = planeGrid(active.frame, matrix, width, height, {
        min: [bounds.min[0] - margin, bounds.min[1] - margin],
        max: [bounds.max[0] + margin, bounds.max[1] + margin],
      });
      const group = element('g', { 'data-grid': grid.spacing }, overlay);
      for (const d of grid.lines) if (d) element('path', { d, class: 'grid' }, group);
      for (const d of grid.axes) if (d) element('path', { d, class: 'axis' }, group);
    }
    for (const layer of studio.layers) {
      if (!layer.visible || !layer.frame) continue;
      const scene = sceneOf(layer.feature);
      if (!scene) continue;
      const isActive = layer.feature === studio.active;
      const group = element('g', { class: isActive ? 'sketch active' : 'sketch inactive', 'data-feature': layer.feature }, overlay);
      // The active sketch is colored by how solved each piece is; others
      // keep their layer color.
      const style = isActive ? studio.entityStyle?.(scene) : null;
      const color = (state) => ({ free: 'var(--free)', fixed: 'var(--fixed)', conflict: 'var(--conflict)' })[state] ?? layer.color;
      const entities = new Map((scene.entities ?? []).map((e) => [e.id, e]));
      const paths = sketchPaths(scene, layer.frame, matrix, width, height);
      if (isActive) {
        // An outline in the view color keeps light geometry visible over
        // light faces and dark geometry over dark ones.
        for (const path of paths) if (path.d) element('path', { d: path.d, class: 'halo' }, group);
      }
      for (const path of paths) {
        if (!path.d) continue;
        const look = style?.curve(entities.get(path.id));
        element('path', {
          d: path.d, class: look?.selected ? 'curve selected' : 'curve',
          stroke: look?.selected ? 'var(--accent)' : look ? color(look.state) : layer.color,
          'stroke-width': look?.selected ? 4 : 2, 'data-entity': path.id,
        }, group);
      }
      if (isActive) {
        for (const point of sketchPoints(scene, layer.frame, matrix, width, height)) {
          const look = style?.point(point.id);
          element('circle', {
            cx: point.xy[0], cy: point.xy[1], r: look?.selected ? 5.5 : 3.5, class: 'point',
            fill: look?.selected ? 'var(--accent)' : look ? color(look.state) : layer.color, 'data-point': point.id,
          }, group);
        }
        drawLabels(scene, layer, matrix, width, height);
      }
    }
    for (const node of studio.decorate({ matrix, width, height, element })) overlay.append(node);
    const empty = $('empty');
    if (renderer) {
      empty.hidden = studio.layers.length > 0;
      empty.textContent = 'No sketches yet. Choose New sketch to start one on a plane or a face.';
    }
  }

  /**
   * The active sketch's constraint glyphs and dimension values, laid out
   * without overlaps; those with no room fold into a badge that lists them
   * on hover.
   */
  function drawLabels(scene, layer, matrix, width, height) {
    const definition = studio.draft?.feature === layer.feature ? studio.draft.definition : studio.definitions[layer.feature];
    const labels = sketchLabels(scene, definition).flatMap((label) => {
      const n = label.anchors.length;
      const center = [label.anchors.reduce((t, a) => t + a[0], 0) / n, label.anchors.reduce((t, a) => t + a[1], 0) / n];
      const at = projectLocal(matrix, layer.frame, center, width, height);
      return at ? [{ ...label, at }] : [];
    });
    const { placed, badges } = layoutLabels(labels, width, height);
    const group = element('g', { class: 'labels' }, overlay);
    const selected = new Set(studio.selection ?? []);
    for (const label of placed) {
      const [x, y, w, h] = label.box;
      const failed = label.status === 'failed';
      const node = element('g', {
        class: `label${label.dimension ? ' dimension' : ' glyph'}${failed ? ' failed' : ''}${label.targets.some((t) => selected.has(t)) ? ' related' : ''}`,
      }, group);
      if (label.leader) element('path', { d: `M${label.at[0].toFixed(1)} ${label.at[1].toFixed(1)}L${(x + w / 2).toFixed(1)} ${(y + h / 2).toFixed(1)}`, class: 'leader' }, node);
      element('rect', { x, y, width: w, height: h, rx: 4 }, node);
      const text = element('text', { x: x + w / 2, y: y + h / 2 + 4, 'text-anchor': 'middle' }, node);
      text.textContent = label.text;
      element('title', {}, node).textContent = label.title;
    }
    for (const badge of badges) {
      const [x, y, w, h] = badge.box;
      const node = element('g', { class: 'badge' }, group);
      element('rect', { x, y, width: w, height: h, rx: 8 }, node);
      const text = element('text', { x: x + w / 2, y: y + h / 2 + 4, 'text-anchor': 'middle' }, node);
      text.textContent = `+${badge.labels.length}`;
      // The folded labels, shown while the badge is hovered.
      const list = element('g', { class: 'badge-list' }, node);
      const lines = badge.labels.map((label) => label.title);
      const listWidth = Math.min(360, 14 + 6.5 * Math.max(...lines.map((line) => line.length)));
      const top = Math.max(0, Math.min(height - 18 * lines.length - 8, y + h + 4));
      const left = Math.max(0, Math.min(width - listWidth, x));
      element('rect', { x: left, y: top, width: listWidth, height: 18 * lines.length + 8, rx: 6 }, list);
      lines.forEach((line, i) => {
        const t = element('text', { x: left + 7, y: top + 17 + 18 * i }, list);
        t.textContent = line;
      });
    }
  }

  let frame = 0;
  function redraw() {
    if (frame) return;
    frame = requestAnimationFrame(() => {
      frame = 0;
      draw();
    });
  }
  studio.redraw = redraw;

  // ── Panels ──────────────────────────────────────────────────────────
  /** Replaces a node's children, skipping null and undefined ones. */
  function fill(node, ...children) {
    node.replaceChildren(...children.flat().filter((child) => child !== null && child !== undefined));
  }
  studio.fill = fill;

  function html(tag, attributes = {}, ...children) {
    const node = document.createElement(tag);
    for (const [key, value] of Object.entries(attributes)) {
      if (key.startsWith('on')) node.addEventListener(key.slice(2), value);
      else if (value === true) node.setAttribute(key, '');
      else if (value !== false && value !== null && value !== undefined) node.setAttribute(key, value);
    }
    node.append(...children.flat().filter((child) => child !== null && child !== undefined));
    return node;
  }
  studio.html = html;

  function renderLayers() {
    const host = $('layers');
    const rows = studio.layers.map((layer) => {
      const meta = layer.local
        ? 'new · not yet in the model'
        : layer.error
          ? layer.error
          : layer.freeDegrees === null
            ? ''
            : layer.freeDegrees === 0
              ? 'fully constrained'
              : `${layer.freeDegrees} free degree${layer.freeDegrees === 1 ? '' : 's'}`;
      const row = html('div', {
        class: 'layer', role: 'button', tabindex: 0,
        'aria-current': layer.feature === studio.active ? 'true' : 'false',
        title: `${layer.feature} on ${layer.plane}`,
        onclick: () => activate(layer.feature),
        onkeydown: (event) => { if (event.key === 'Enter' || event.key === ' ') activate(layer.feature); },
      },
      html('input', {
        type: 'checkbox', checked: layer.visible, 'aria-label': `Show ${layer.feature}`,
        onclick: (event) => event.stopPropagation(),
        onchange: (event) => { layer.visible = event.target.checked; redraw(); },
      }),
      html('span', { class: 'swatch', style: `background:${layer.color}` }),
      html('span', { class: 'name' }, `${layer.feature} `, html('span', { style: 'font-weight:400;color:var(--muted)' }, layer.plane)),
      meta ? html('span', { class: `meta${layer.error ? ' error' : ''}` }, meta) : null);
      return row;
    });
    fill(host, html('h2', {}, 'Layers'), ...rows, newSketchPanel());
  }

  function newSketchPanel() {
    if (!studio.newSketch) {
      return html('button', { style: 'margin-top:10px;width:100%', onclick: openNewSketch }, '+ New sketch');
    }
    const values = studio.newSketch;
    const field = (label, control) => html('label', {}, label, control);
    const name = html('input', { value: values.name, 'aria-label': 'Sketch name', oninput: (e) => { values.name = e.target.value; } });
    const plane = html('select', { 'aria-label': 'Sketch plane', onchange: (e) => { values.plane = e.target.value; values.picked = null; renderLayers(); } },
      ...[['xy', 'XY (faces +Z)'], ['xz', 'XZ (faces −Y)'], ['yz', 'YZ (faces +X)'], ['face', 'A face of the model']]
        .map(([value, text]) => html('option', { value, selected: values.plane === value }, text)));
    const offset = html('input', { type: 'number', step: 'any', value: values.offset, 'aria-label': 'Offset along the normal (mm)', oninput: (e) => { values.offset = Number(e.target.value); } });
    const pickNote = values.plane !== 'face'
      ? null
      : values.picked
        ? html('p', { style: 'margin:0;color:var(--muted)' }, `Face picked · normal ${vectorText(values.picked.normal)}`)
        : html('button', { onclick: beginFacePick }, studio.picking ? 'Click a face in the view…' : 'Pick a face');
    return html('div', { class: 'form' },
      html('h2', { style: 'margin:0' }, 'New sketch'),
      field('Name', name), field('Plane', plane), pickNote, field('Offset along the normal (mm)', offset),
      html('div', { class: 'row' },
        html('button', { onclick: createLocalLayer }, 'Start sketch'),
        html('button', { onclick: () => { studio.newSketch = null; cancelFacePick(); renderLayers(); } }, 'Cancel')));
  }

  function openNewSketch() {
    const taken = [...studio.features.map((f) => f.id), ...studio.layers.map((l) => l.feature)];
    studio.newSketch = { name: nextFeatureName(taken), plane: 'xy', offset: 0, picked: null };
    renderLayers();
    say('Choose a plane, or pick a face of the model, then Start sketch.');
  }

  function createLocalLayer() {
    const values = studio.newSketch;
    try {
      const name = values.name.trim();
      if (!name || name !== values.name) throw new Error('Enter a sketch name without surrounding spaces.');
      if (studio.features.some((f) => f.id === name) || studio.layers.some((l) => l.feature === name)) {
        throw new Error(`'${name}' is already taken.`);
      }
      if (!Number.isFinite(values.offset)) throw new Error('The offset must be a finite number.');
      let frame;
      if (values.plane === 'face') {
        if (!values.picked) throw new Error('Pick a face first.');
        const n = values.picked.normal;
        frame = {
          origin_mm: values.picked.origin_mm.map((v, i) => v + n[i] * values.offset),
          x_axis: literalVector(values.picked.x_axis),
          y_axis: literalVector(values.picked.y_axis),
        };
      } else {
        frame = basePlane(values.plane, values.offset);
      }
      const layer = {
        feature: name, frame, plane: values.plane === 'face' ? 'Face' : planeLabel(frame),
        visible: true, color: PALETTE[studio.layers.length % PALETTE.length],
        error: null, freeDegrees: null, local: true,
        create: { plane: values.plane, offset: values.offset, picked: values.picked },
      };
      studio.layers.push(layer);
      studio.newSketch = null;
      cancelFacePick();
      activate(name, true);
      say(`New sketch ${name} on ${layer.plane}. Draw a closed shape on it; Apply adds it to the model.`);
    } catch (error) {
      say(error.message, true);
    }
  }

  function renderInspector() {
    const host = $('inspector');
    const layer = activeLayer();
    if (studio.renderSelection?.(host)) return;
    if (!layer) {
      fill(host, html('h2', {}, 'Inspector'), html('p', { style: 'color:var(--muted)' }, 'Select a layer to see its plane.'));
      return;
    }
    const scene = sceneOf(layer.feature);
    const rows = [['Plane', layer.plane]];
    if (layer.frame) {
      rows.push(['Origin', `${vectorText(layer.frame.origin_mm)} mm`], ['X axis', vectorText(layer.frame.x_axis)],
        ['Y axis', vectorText(layer.frame.y_axis)], ['Normal', vectorText(planeNormal(layer.frame))]);
    }
    const solver = scene?.solver;
    if (solver) {
      rows.push(['Solver', solver.accepted ? 'solved' : 'not solved'], ['Free degrees', String(solver.free_degrees)],
        ['Residual', Number(solver.max_residual).toExponential(1)]);
      const conflicts = solver.diagnosis?.conflict_groups?.length ?? 0;
      if (conflicts) rows.push(['Conflicts', String(conflicts)]);
    }
    if (layer.error) rows.push(['Error', layer.error]);
    fill(host, 
      html('h2', {}, layer.feature),
      html('dl', {}, ...rows.flatMap(([term, value]) => [html('dt', {}, term), html('dd', {}, value)])),
      layer.local ? html('p', { style: 'color:var(--muted)' }, 'This sketch exists only here until its first closed shape is applied.') : null,
    );
  }
  studio.renderInspector = renderInspector;

  function renderStatus() {
    const layer = activeLayer();
    const scene = layer && sceneOf(layer.feature);
    const solver = scene?.solver;
    fill($('status'), 
      html('span', { class: studio.error ? 'message error' : 'message' }, studio.message),
      layer ? html('span', {}, `Active: ${layer.feature} · ${layer.plane}`) : null,
      solver ? html('span', {}, `${solver.free_degrees} free · residual ${Number(solver.max_residual).toExponential(1)}`) : null,
      studio.statusExtra?.() ?? null,
    );
  }
  studio.renderStatus = renderStatus;

  function refresh() {
    renderLayers();
    renderInspector();
    renderStatus();
    studio.renderToolbar?.();
    redraw();
  }
  studio.refresh = refresh;

  function activate(feature, look = false) {
    studio.active = feature;
    const layer = activeLayer();
    if (look && layer?.frame) lookAtActive();
    studio.onActivate?.(layer);
    refresh();
  }
  studio.activate = activate;

  function lookAtActive() {
    const layer = activeLayer();
    if (!layer?.frame) return;
    const bounds = localBounds(sceneOf(layer.feature));
    const center = bounds ? [(bounds.min[0] + bounds.max[0]) / 2, (bounds.min[1] + bounds.max[1]) / 2] : [0, 0];
    studio.camera = lookAt(studio.camera, layer.frame, center);
    if (bounds) {
      // Fit the sketch: its half-diagonal (glTF metres) within the narrower
      // half field of view, with a margin.
      const radius = Math.max(1e-6, Math.hypot(bounds.max[0] - bounds.min[0], bounds.max[1] - bounds.min[1]) / 2000);
      const half = Math.atan(Math.tan(Math.PI / 8) * Math.min(1, aspect()));
      studio.camera = { ...studio.camera, distance: (radius / Math.sin(half)) * 1.15 };
    }
    redraw();
  }

  // ── Face picking ────────────────────────────────────────────────────
  function beginFacePick() {
    studio.picking = true;
    renderLayers();
    say('Click a flat face of the model. Escape cancels.');
  }
  function cancelFacePick() {
    studio.picking = false;
  }
  async function pickFaceAt(clientX, clientY) {
    const solid = studio.scenes.find((s) => s.kind === 'solid' && Array.isArray(s.mesh));
    if (!solid) throw new Error('No solid is shown to pick a face from.');
    const face = pickFace(solid.mesh, studio.rayAt(clientX, clientY));
    if (face < 0) {
      say('That click missed the model. Click a flat face.', true);
      return;
    }
    const picked = await api('/api/sketch-face-pick', {
      method: 'POST',
      body: JSON.stringify({ version: studio.version, instance: studio.instance, output: solid.feature, face }),
    });
    if (!studio.newSketch) return;
    studio.newSketch.picked = picked;
    studio.newSketch.plane = 'face';
    cancelFacePick();
    renderLayers();
    say(picked.binding === 'anchored_center'
      ? 'Face picked; it is anchored to its current center. Start sketch to draw on it.'
      : 'Face picked. Start sketch to draw on it.');
  }

  // ── View controls ───────────────────────────────────────────────────
  studio.rayAt = (clientX, clientY) => {
    const box = canvas.getBoundingClientRect();
    const x = ((clientX - box.left) / box.width) * 2 - 1;
    const y = 1 - ((clientY - box.top) / box.height) * 2;
    return ray(studio.camera, aspect(), x, y);
  };
  /** Sketch-local coordinates under the cursor on the active plane, or null. */
  studio.localAt = (clientX, clientY) => {
    const layer = activeLayer();
    return layer?.frame ? rayToLocal(layer.frame, studio.rayAt(clientX, clientY)) : null;
  };
  /** Screen pixels within the view of a sketch-local point on the active plane. */
  studio.toScreen = (local) => {
    const layer = activeLayer();
    if (!layer?.frame) return null;
    const [width, height] = size();
    return projectLocal(viewProjection(studio.camera, width / height), layer.frame, local, width, height);
  };

  let drag = null;
  canvas.addEventListener('contextmenu', (event) => event.preventDefault());
  canvas.addEventListener('pointerdown', (event) => {
    // Keep focus where it is (for example in a value box a tool opened).
    event.preventDefault();
    canvas.setPointerCapture(event.pointerId);
    // The active tool sees left clicks first; what it declines navigates
    // (drag to orbit, Shift-drag or right-drag to pan).
    if (event.button === 0 && studio.tools.get(studio.tool)?.down?.(event)) {
      drag = { tool: true };
      return;
    }
    drag = { x: event.clientX, y: event.clientY, moved: 0, pan: event.shiftKey || event.button === 2 || event.button === 1 };
    canvas.style.cursor = 'grabbing';
  });
  canvas.addEventListener('pointermove', (event) => {
    if (!drag || drag.tool) {
      studio.tools.get(studio.tool)?.move?.(event, Boolean(drag?.tool));
      return;
    }
    const dx = event.clientX - drag.x;
    const dy = event.clientY - drag.y;
    drag.x = event.clientX;
    drag.y = event.clientY;
    drag.moved += Math.abs(dx) + Math.abs(dy);
    if (drag.pan) {
      studio.camera = pan(studio.camera, dx, dy, canvas.clientHeight);
    } else {
      const limit = Math.PI / 2 - 0.01;
      studio.camera = {
        ...studio.camera,
        yaw: studio.camera.yaw - dx * 0.008,
        pitch: Math.max(-limit, Math.min(limit, studio.camera.pitch + dy * 0.008)),
      };
    }
    redraw();
  });
  canvas.addEventListener('pointerup', async (event) => {
    const was = drag;
    drag = null;
    canvas.style.cursor = 'grab';
    if (was?.tool) {
      studio.tools.get(studio.tool)?.up?.(event);
      return;
    }
    const click = was && was.moved < 4 && event.button === 0;
    if (!click) return;
    if (studio.picking) {
      try {
        await pickFaceAt(event.clientX, event.clientY);
      } catch (error) {
        say(`Face pick rejected: ${error.message}`, true);
      }
      return;
    }
    studio.tools.get(studio.tool)?.click?.(event);
  });
  canvas.addEventListener('pointercancel', () => {
    drag = null;
    canvas.style.cursor = 'grab';
  });
  canvas.addEventListener('wheel', (event) => {
    event.preventDefault();
    studio.camera = { ...studio.camera, distance: studio.camera.distance * Math.exp(event.deltaY * 0.001) };
    redraw();
  }, { passive: false });
  new ResizeObserver(redraw).observe(canvas);

  $('look-at').addEventListener('click', lookAtActive);
  $('fit').addEventListener('click', () => {
    studio.camera = fitCamera(studio.parsed?.bounds ?? null, aspect());
    redraw();
  });
  document.addEventListener('keydown', (event) => {
    if (event.target.closest?.('input, select, textarea')) return;
    if (studio.onKey?.(event)) return;
    if (event.key === 'Escape' && studio.picking) {
      cancelFacePick();
      renderLayers();
      say('Face picking canceled.');
    } else if (event.key === 'n' || event.key === 'N') {
      lookAtActive();
    } else if (event.key === 'f' || event.key === 'F') {
      $('fit').click();
    }
  });

  // Follow edits made elsewhere (the classic viewer, the CLI, the file).
  async function poll() {
    try {
      const state = await api('/api/state');
      if (state.version !== studio.version && !studio.busy) await load();
      else if (state.dirty !== studio.modelState?.dirty) {
        studio.modelState = state;
        studio.renderToolbar?.();
      }
    } catch (error) {
      say(error.message, true);
    }
    setTimeout(poll, 1000);
  }

  studio.viewport = () => {
    const [width, height] = size();
    return { matrix: viewProjection(studio.camera, width / height), width, height };
  };
  studio.drawing = installDrawing(studio, document);
  installConstraints(studio, document);
  installExtras(studio, document);
  load()
    .then(() => say(studio.layers.length ? 'Choose a layer, then a tool. Drag to orbit, Shift-drag or right-drag to pan, scroll to zoom.' : 'No sketches yet. Choose New sketch.'))
    .catch((error) => say(error.message, true))
    .finally(() => setTimeout(poll, 1000));
  document.studio = studio;
  return studio;
}

