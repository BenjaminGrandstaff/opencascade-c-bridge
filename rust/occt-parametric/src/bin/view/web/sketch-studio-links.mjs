import {createSketchLinks, requireConstructionProfile} from './sketch-links.mjs';
import {geometryNames} from './geometry-names.mjs';
import {inheritGeneratedReferences} from './generated-points.mjs';
import {sketchDependencyGraph, dependencyReach} from './sketch-dependencies.mjs';
import {pickFace} from './sketch-studio-plane.mjs';
import {hitTest} from './sketch-studio-select.mjs';
import {pickSourceEdge, sourceEdgePaths} from './sketch-studio-sources.mjs';

export function installStudioLinks(studio, document) {
  const {html} = studio;
  const panel = html('section', {class: 'sheet', role: 'dialog', 'aria-label': 'Sketch source links', hidden: true});
  document.body.append(panel);
  let session = null, internalMode = false;
  const setTool = studio.setTool;
  function mode(tool) {
    internalMode = true;
    try { setTool('select'); studio.tool = tool; }
    finally { internalMode = false; }
    studio.refresh();
  }
  function close() {
    const old = session; session = null;
    old?.links.cancel(); panel.hidden = true;
    if (old) mode('select');
    studio.redraw();
  }
  function open() {
    if (studio.modelPending) return false;
    if (studio.picking || studio.isPlanePicking?.()) { studio.say('Finish or cancel the face or plane pick first.', true); return false; }
    close(); setTool('select');
    const draft = studio.ensureDraft();
    if (!draft) return false;
    if (draft.previewing) { studio.say('Wait for Preview before editing source links.', true); return false; }
    if (draft.local) { studio.say('Apply the new sketch before linking accepted source geometry.', true); return false; }
    let working = structuredClone(draft.definition);
    inheritGeneratedReferences(draft.definition, working);
    const checkpoint = studio.draftCheckpoint(), version = studio.version, instance = studio.instance, feature = draft.feature;
    const snapshot = {links: null, source: null, pending: 0}; session = snapshot;
    const current = () => session === snapshot && checkpoint() && !draft.previewing;
    const valid = () => { if (current()) return true; studio.say('The sketch changed. Reopen source links.'); return false; };
    const message = text => { if (current()) studio.say(text); };
    const stage = next => {
      if (!valid()) return;
      const definition = structuredClone(next); inheritGeneratedReferences(working, definition);
      close(); studio.changeDraft(definition); studio.selection = []; studio.refresh();
      studio.say('Source link staged. Preview checks the sketch and dependent solids.');
    };
    const request = async (path, options) => {
      if (!current()) throw Error('The sketch changed. Reopen source links.');
      const body = JSON.parse(options.body); snapshot.pending++;
      try {
        const result = await studio.api(path, options);
        if (!current() || result.version !== version || ['/api/sketch-edge-pick', '/api/sketch-face-pick'].includes(path) && result.instance !== instance) throw Error('The model or draft changed. Reopen source links.');
        if (path === '/api/sketch-source-view' && (!result.accepted || !result.source_view || !(result.scenes ?? []).some(s => s.kind === 'solid' && s.instance === instance && s.feature === body.output))) throw Error('The source view does not match this instance and feature.');
        if (path === '/api/sketch-edge-pick' && (result.projection?.input !== body.output || result.projection?.id !== body.id)) throw Error('The picked edge does not match the requested source and name.');
        if (path === '/api/sketch-face-pick' && result.face_support?.input !== body.output) throw Error('The picked face does not match the requested source.');
        if (path === '/api/sketch-detach-projection' && (result.can_apply !== true || result.definition?.id !== working.id || !(result.annotations?.scenes ?? []).some(s => s.kind === 'sketch' && s.instance === instance && s.feature === feature))) throw Error('The detached geometry does not match the verified sketch draft.');
        return result;
      } finally { snapshot.pending--; }
    };
    const host = html('div'), dependencies = html('div', {'aria-live': 'polite'});
    studio.fill(panel, html('header', {}, html('h2', {}, 'Support and projected references'), html('button', {onclick: close}, 'Close')), host, dependencies);
    const highlight = ids => { if (current()) { studio.selection = ids; studio.refresh(); } };
    const links = createSketchLinks({
      context: () => ({draft: working, feature, version, instance, features: studio.features}),
      request, currentVersion: () => studio.version, message,
      beginPicking: () => { if (!valid()) throw Error('The sketch changed. Reopen source links.'); },
      preview: async data => { if (!current()) return; snapshot.source = data.scenes.find(s => s.kind === 'solid' && s.instance === instance); studio.redraw(); },
      showSolid: () => {},
      highlightFace: face => { if (face === null) { snapshot.source = null; studio.redraw(); } },
      highlightEdge: () => {}, highlight,
      beginProjectionPicking: () => { if (!valid()) throw Error('The sketch changed. Reopen source links.'); },
      changed: () => stage(working),
      addProjection: (definition, projection) => {
        requireConstructionProfile(definition);
        if (geometryNames(definition).has(projection.id)) throw Error('That geometry name already exists.');
        (definition.projections ??= []).push(structuredClone(projection));
      },
      detachProjection: async projection => {
        if (!valid() || snapshot.pending) return;
        links.cancel(); studio.say('Resolving exact projected spline geometry…');
        const result = await request('/api/sketch-detach-projection', {method: 'POST', body: JSON.stringify({version, instance, feature, expected: draft.baseline, definition: working, projection})});
        if (!current()) return;
        working = result.definition; stage(working);
      },
      inspectDependencies: id => {
        if (!valid()) return;
        const graph = sketchDependencyGraph(working, {features: studio.features, feature}), key = graph.geometry.get(id);
        studio.fill(dependencies, html('h3', {}, `Dependencies of ${id}`), ...[['Sources', 'dependencies'], ['Dependents', 'consumers']].map(([label, direction]) => html('div', {}, html('h4', {}, label), html('ul', {}, ...dependencyReach(graph, key, direction).slice(0, 100).map(k => html('li', {}, graph.nodes.get(k).label))))));
        highlight([id]);
      },
    });
    snapshot.links = links; links.render(host); panel.hidden = false; mode('source-link');
    studio.say('Choose an earlier feature, then project an edge or pick a planar support face. Source edges appear as a dashed overlay.');
    return true;
  }
  async function click(event) {
    const s = session;
    if (!s || event.shiftKey || event.button !== 0 || studio.modelPending || s.pending) return;
    const box = document.getElementById('view').getBoundingClientRect(), at = [event.clientX - box.left, event.clientY - box.top];
    if (!s.links.facePicking && !s.links.edgePicking) {
      const layer = studio.activeLayer(), scene = studio.sceneOf(layer?.feature);
      const ids = new Set((studio.draft?.definition.projections ?? []).map(p => p.id));
      const hit = layer?.frame && hitTest({scene: {entities: (scene?.entities ?? []).filter(e => ids.has(e.id))}, frame: layer.frame, ...studio.viewport(), at});
      if (hit) s.links.pickProjection(hit.id);
      return;
    }
    if (!s.source) return;
    const source = s.source, hit = {accepted: true, version: studio.version, instance: studio.instance, output: source.feature};
    if (s.links.facePicking) {
      const face = pickFace(source.mesh ?? [], studio.rayAt(event.clientX, event.clientY));
      if (face < 0) { studio.say('That click missed the source. Pick a planar face.', true); return; }
      await s.links.pickFace({...hit, face});
    } else {
      const edge = pickSourceEdge(source, studio.viewport(), at);
      if (edge === null) { studio.say('That click missed the source edges.', true); return; }
      await s.links.pickEdge({...hit, edge});
    }
  }
  studio.tools.set('source-link', {click, cancel: close});
  panel.addEventListener('keydown', event => { if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); close(); } });
  studio.setTool = id => { close(); setTool(id); };
  const cancel = studio.cancelSelectionGesture;
  studio.cancelSelectionGesture = () => { cancel?.(); if (!internalMode) close(); };
  const change = studio.changeDraft; studio.changeDraft = (...args) => { close(); return change(...args); };
  const activate = studio.onActivate; studio.onActivate = layer => { close(); activate?.(layer); };
  const load = studio.onLoad; studio.onLoad = () => { close(); load?.(); };
  const key = studio.onKey; studio.onKey = event => { if (event.key === 'Escape' && session) { close(); return true; } return key?.(event) ?? false; };
  const decorate = studio.decorate;
  studio.decorate = context => {
    const nodes = decorate(context), source = session?.source;
    if (source) for (const {edge, d} of sourceEdgePaths(source, context)) if (d) nodes.push(context.element('path', {d, fill: 'none', stroke: '#c77912', 'stroke-width': 2, 'stroke-dasharray': '5 3', 'data-source-edge': edge}));
    return nodes;
  };
  studio.fill(document.getElementById('source-links'), html('h2', {}, 'Source links'), html('button', {onclick: open}, 'Project geometry and support'));
  return {open, close, click, get source() { return session?.source ?? null; }};
}
