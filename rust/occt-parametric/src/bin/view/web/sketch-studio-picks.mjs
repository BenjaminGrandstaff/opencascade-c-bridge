// Named points and curve/region operation picking on the studio sketch plane.
// Existing picker controls own the native candidate/window semantics; this
// adapter supplies studio events, markers and guarded draft transactions.
import {createStudioProfileEditor} from './sketch-studio-profile.mjs';
import {profileIds} from './profile-controls.mjs';
import {createWeightedPointPicker} from './sketch-studio-weighted.mjs';
import {createCrossingPicker} from './crossing-pick.mjs';
import {createSplineSamplePicker} from './spline-sample-pick.mjs';
import {geometryIds} from './constraint-controls.mjs';
import {hitTest} from './sketch-studio-select.mjs';
import {pathData} from './sketch-studio-overlay.mjs';

export function installPointPicking(studio, document) {
  const {html} = studio;
  const panel = html('section', {class: 'sheet', role: 'dialog', 'aria-label': 'Named point picking', hidden: true});
  document.body.append(panel);
  let session = null, internalMode = false, paths = [];
  const setTool = studio.setTool;
  function modeChanged(mode) {
    internalMode = true;
    try { setTool('select'); studio.tool = mode; }
    finally { internalMode = false; }
    studio.refresh();
  }
  function close() {
    const old = session;
    session = null;
    old?.picker?.cancel();
    paths = [];
    panel.hidden = true;
    if (old) modeChanged('select');
    studio.redraw();
  }
  panel.addEventListener('keydown', event => {
    if (event.key !== 'Escape') return;
    event.preventDefault(); event.stopPropagation(); close();
    studio.say('Point pick canceled. Draft unchanged.');
  }, true);
  function open(kind) {
    if (studio.modelPending) return false;
    if (studio.picking || studio.isPlanePicking?.()) { studio.say('Finish or cancel the face or plane pick before picking sketch points.', true); return false; }
    close();
    setTool('select');
    const draft = studio.ensureDraft(), layer = studio.activeLayer();
    if (!draft || !layer) { studio.say('Choose a sketch and apply or discard any other open draft first.', true); return false; }
    if (draft.local && kind !== 'profile') { studio.say('Apply the new sketch before picking named points from its native curves.', true); return false; }
    if (draft.previewing || draft.error && kind !== 'profile') { studio.say('Wait for a successful Preview of the current draft before picking named points.', true); return false; }
    const scene = studio.sceneOf(draft.feature);
    if (!scene) return false;
    const source = draft.definition, checkpoint = studio.draftCheckpoint();
    const known = new Set(kind === 'profile' ? profileIds(source, 'curves') : geometryIds(source, 'curves'));
    const knownPoints = new Set(geometryIds(source, 'points'));
    const points = Object.fromEntries(Object.entries(scene.points ?? {}).filter(([id]) => knownPoints.has(id)));
    const curves = kind === 'profile'
      ? [...(scene.entities ?? []).filter(e => e.curve_id && known.has(e.curve_id)).map(e => ({...e, id: e.curve_id})), ...(scene.entities ?? []).filter(e => known.has(e.id))]
      : (scene.entities ?? []).filter(e => known.has(e.id));
    const snapshot = {kind, scene, points, curves, picker: null};
    session = snapshot;
    const current = () => session === snapshot && checkpoint() && !draft.previewing && (!draft.error || kind === 'profile') && studio.sceneOf(draft.feature) === scene;
    const query = async (path, request) => {
      if (!current()) throw Error('The sketch changed. Restart the point pick.');
      const result = await studio.api(path, {method: 'POST', body: JSON.stringify({
        version: studio.version, instance: studio.instance, feature: draft.feature,
        expected: draft.baseline, definition: source, ...request,
      })});
      if (!current() || result.version !== studio.version || result.instance !== studio.instance || result.feature !== draft.feature || request.spline && result.spline !== request.spline) {
        throw Error('The sketch or native view changed. Restart the point pick.');
      }
      return result;
    };
    panel.ariaLabel = kind === 'profile' ? 'Curve and region editing' : 'Named point picking';
    const host = html('div');
    studio.fill(panel, html('header', {}, html('h2', {}, ({crossing: 'Intersection point', sample: 'Spline sample', weighted: 'Weighted point', profile: 'Curve and region edits'})[kind]),
      html('button', {onclick: close}, 'Close')), host);
    panel.hidden = false;
    const options = {
      host, definition: source, positions: points, parameters: studio.parameters ?? [], initialOpen: true, current,
      beginPicking: () => { if (!current()) throw Error('The sketch changed. Close this panel and restart the point pick.'); },
      modeChanged,
      message: message => studio.say(message),
      highlight: ids => { studio.selection = ids; studio.refresh(); },
      ghost: next => { paths = next; studio.redraw(); },
      changed: (next, point) => {
        if (!current()) return;
        close();
        studio.changeDraft(next);
        studio.selection = point ? [point] : [];
        studio.refresh();
      },
    };
    snapshot.picker = kind === 'crossing'
      ? createCrossingPicker({...options, queryCandidates: request => query('/api/sketch-crossing-candidates', request)})
      : kind === 'sample' ? createSplineSamplePicker({...options, query: request => query('/api/sketch-spline-sample', request)})
      : kind === 'weighted' ? createWeightedPointPicker(options)
      : createStudioProfileEditor(options);
    return true;
  }

  function eventFor(event, pickEntity = false) {
    const layer = studio.activeLayer(), {matrix, width, height} = studio.viewport();
    const position = studio.localAt(event.clientX, event.clientY);
    const box = document.getElementById('view').getBoundingClientRect();
    const at = [event.clientX - box.left, event.clientY - box.top];
    let tolerance = 0;
    if (position) {
      const a = studio.toScreen(position), b = studio.toScreen([position[0] + 1, position[1]]);
      const pixels = a && b ? Math.hypot(b[0] - a[0], b[1] - a[1]) : 0;
      tolerance = pixels > 0 ? 7 / pixels : 0;
    }
    // Curve picks ignore endpoint hits. Operation picks may use saved edited
    // piece names; unrelated display glyphs cannot become source references.
    const hit = pickEntity && layer?.frame && session
      ? hitTest({scene: (session.kind === 'weighted' || session.picker?.pickGroup === 'points') ? {points: session.points} : {entities: session.curves}, frame: layer.frame, matrix, width, height, at, radius: 7}) : null;
    return {position, tolerance, entity: hit?.id ?? null, scene: session?.scene, version: studio.version};
  }
  for (const kind of ['crossing', 'sample', 'weighted', 'profile']) studio.tools.set(kind, {
    down(event) { return event.button === 0 && !event.shiftKey && session?.picker?.down(eventFor(event, true)); },
    move(event) { session?.picker?.move(eventFor(event)); },
    up(event) { return session?.picker?.up(eventFor(event)); },
    cancel: close,
  });
  studio.setTool = id => { close(); setTool(id); };
  const cancelGesture = studio.cancelSelectionGesture;
  studio.cancelSelectionGesture = () => { cancelGesture?.(); if (!internalMode) close(); };
  const change = studio.changeDraft;
  studio.changeDraft = (...args) => { close(); return change(...args); };
  const activate = studio.onActivate;
  studio.onActivate = layer => { close(); activate?.(layer); };
  const load = studio.onLoad;
  studio.onLoad = () => { close(); load?.(); };
  const onKey = studio.onKey;
  studio.onKey = event => {
    if (event.key === 'Escape' && session) { close(); studio.say('Point pick canceled. Draft unchanged.'); return true; }
    return onKey?.(event) ?? false;
  };
  const decorate = studio.decorate;
  studio.decorate = context => {
    const nodes = decorate(context), layer = studio.activeLayer();
    if (!session || !layer?.frame) return nodes;
    for (const points of paths) {
      if (points.length === 1) {
        const xy = studio.toScreen(points[0]);
        if (xy) nodes.push(context.element('circle', {cx: xy[0], cy: xy[1], r: 5, class: 'ghost'}));
      } else {
        const d = pathData(points, layer.frame, context.matrix, context.width, context.height);
        if (d) nodes.push(context.element('path', {d, class: 'ghost'}));
      }
    }
    return nodes;
  };
  studio.fill(document.getElementById('named-points'), html('h2', {}, 'Named points'),
    html('button', {onclick: () => open('crossing')}, 'Intersection point'),
    html('button', {onclick: () => open('sample')}, 'Spline sample'),
    html('button', {onclick: () => open('weighted')}, 'Weighted point'));
  studio.fill(document.getElementById('curve-edits'), html('h2', {}, 'Curve and region edits'),
    html('button', {onclick: () => open('profile')}, 'Edit curves and regions'));
  return {open, close, get active() { return session?.picker?.active ?? false; }};
}
