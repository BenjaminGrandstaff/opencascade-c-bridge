// The studio's keyboard help and its Advanced drawer, which hosts the
// classic sketch editor's panels (point links, patterns, projections,
// curve and region edits, dependencies, profiles) as they are.
import { createSketchEditor } from './sketch-editor.mjs';
import { createSolidEditor } from './solid-editor.mjs';
import { TOOLS } from './sketch-studio-tools.mjs';

export const SHORTCUTS = [
  ...TOOLS.map((tool) => [tool.key, tool.label]),
  ['N', 'Look square at the active plane'],
  ['F', 'Fit the model in view'],
  ['Enter', 'Finish a spline, commit a typed value, or apply the first suggested constraint'],
  ['Escape', 'Cancel the gesture, clear the selection, then return to Select'],
  ['X', 'Flip an arc the longer way round'],
  ['Alt', 'Hold while drawing to add no automatic constraints'],
  ['Shift', 'Click to add to the selection; drag to pan'],
  ['Delete / Backspace', 'Delete the selection and what depends on it'],
  ['Ctrl+Z / Ctrl+Shift+Z', 'Undo and redo in the draft, or the model when no draft is open'],
  ['?', 'Show or hide this help'],
];

export function installExtras(studio, document) {
  const { html } = studio;
  const style = html('style', {}, `
    .sheet { position: fixed; z-index: 5; top: 52px; right: 12px; width: min(380px, calc(100vw - 24px));
      max-height: calc(100vh - 100px); overflow: auto; background: var(--panel); border: 1px solid var(--line);
      border-radius: 10px; box-shadow: 0 10px 30px rgb(0 0 0 / 0.25); padding: 12px; }
    .sheet header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 8px; }
    .sheet table { border-collapse: collapse; width: 100%; }
    .sheet td { padding: 3px 6px; vertical-align: top; }
    .sheet td:first-child { white-space: nowrap; font-family: ui-monospace, monospace; color: var(--accent); }
    .sheet .linked-edit { display: grid; gap: 8px; }
    .sheet .linked-edit label { display: grid; gap: 3px; color: var(--muted); font-size: 12px; }
    .sheet details { border: 1px solid var(--line); border-radius: 6px; padding: 4px 8px; margin: 4px 0; }
    .sheet summary { cursor: pointer; }`);
  document.head.append(style);

  const help = html('section', { class: 'sheet', role: 'dialog', 'aria-label': 'Keyboard shortcuts', hidden: true },
    html('header', {}, html('h2', { style: 'margin:0' }, 'Shortcuts'), html('button', { onclick: () => { help.hidden = true; } }, 'Close')),
    html('table', {}, SHORTCUTS.map(([key, text]) => html('tr', {}, html('td', {}, key), html('td', {}, text)))));
  const drawer = html('section', { class: 'sheet', role: 'dialog', 'aria-label': 'Advanced sketch editing', hidden: true });
  document.body.append(help, drawer);

  let editor = null;
  function openDrawer() {
    if (studio.modelPending) return;
    help.hidden = true;
    if (!drawer.hidden) {
      drawer.hidden = true;
      return;
    }
    const intro = html('p', { style: 'color:var(--muted);margin-top:0' },
      'The classic editor\'s panels for this model: point links, patterns, projections, curve and region edits, profiles and dependencies. ' +
      'Its previews show here; picking from the canvas (crossings, spline samples, faces and edges) needs the classic viewer.');
    const header = html('header', {}, html('h2', { style: 'margin:0' }, 'Advanced'),
      html('div', { style: 'display:flex;gap:6px' },
        html('a', { href: '/', target: '_blank', rel: 'noopener', style: 'color:var(--muted)' }, 'Classic viewer'),
        html('button', { onclick: () => { drawer.hidden = true; } }, 'Close')));
    if (studio.hasDraftHistory?.() || studio.draft?.dirty) {
      studio.fill(drawer, header, html('p', {}, studio.draft.dirty ? `Apply or discard the studio draft of ${studio.draft.feature} first; the drawer edits the applied sketch.` : `Redo and apply your changes, or discard the local history of ${studio.draft.feature} first; the drawer edits the applied sketch.`));
      drawer.hidden = false;
      return;
    }
    const host = html('div');
    const solids = html('div');
    studio.fill(drawer, header, intro, solids, host);
    const reloadAfter = async () => {
      studio.previewScenes = {};
      await studio.reload();
    };
    // Sketch to solid: extrude, revolve or cut from a sketch, as in the classic viewer.
    createSolidEditor({
      host: solids,
      request: studio.api,
      instance: () => studio.instance,
      preview: async () => studio.say('Solid draft previewed; Apply updates the model and the 3D view.'),
      applied: async () => {
        await reloadAfter();
        studio.say('Solid feature applied to the model.');
      },
      restore: async () => studio.refresh(),
      showSketch: (feature) => { if (feature && feature !== studio.active) studio.activate(feature); },
    });
    // The classic editor drives its own draft, previews and apply through
    // the same API; these adapters route its view requests to the studio.
    editor = createSketchEditor({
      host,
      request: studio.api,
      instance: () => studio.instance,
      currentVersion: () => studio.version,
      preview: async (annotations) => {
        for (const scene of annotations?.scenes ?? []) {
          if (scene.kind === 'sketch') studio.previewScenes[scene.feature] = scene;
        }
        studio.refresh();
      },
      applied: async () => {
        studio.previewScenes = {};
        await studio.reload();
        studio.say('Advanced edit applied to the model.');
      },
      restore: async () => {
        studio.previewScenes = {};
        studio.refresh();
      },
      showSketch: (feature) => { if (feature && feature !== studio.active) studio.activate(feature); },
      highlight: (ids) => {
        studio.selection = [...(ids ?? [])];
        studio.refresh();
      },
    });
    drawer.hidden = false;
  }

  const actions = document.getElementById('draft-actions');
  const bar = document.getElementById('bar');
  const extra = html('div', { class: 'group' },
    html('button', { title: 'Point links, patterns, projections, curve and region edits (classic panels)', onclick: openDrawer }, 'Advanced'),
    html('button', { title: 'Keyboard shortcuts (?)', onclick: () => { drawer.hidden = true; help.hidden = !help.hidden; } }, '?'));
  bar.insertBefore(extra, actions);

  const previous = studio.onKey;
  studio.onKey = (event) => {
    if (event.key === '?') {
      help.hidden = !help.hidden;
      return true;
    }
    if (event.key === 'Escape' && (!help.hidden || !drawer.hidden)) {
      help.hidden = true;
      drawer.hidden = true;
      return true;
    }
    return previous?.(event) ?? false;
  };
  return { openDrawer, editor: () => editor };
}
