// Constraining in the sketch studio: selecting geometry, the contextual
// constraint bar, the Dimension tool with its inline value, solved drags of
// free points, and the inspector for a selection.
import { constraintTools, makeConstraint } from './constraint-controls.mjs';
import { pathData } from './sketch-studio-overlay.mjs';
import {
  constraintsTouching, dimensionExpression, dimensionFor, hitTest, solveStates, suggestions,
} from './sketch-studio-select.mjs';

/** Suggestions shown as buttons; the rest are in the menu. */
const BAR_BUTTONS = 6;
const SYMBOLS = {
  coincident: '●', horizontal: '—', vertical: '|', parallel: '∥', perpendicular: '⊥', tangent: '⌒',
  equal_length: '=', equal_radius: '=', concentric: '◎', midpoint: '◇', point_on_curve: '∈', collinear: '⋯',
  symmetric: '⇋', distance: '↔', radius: 'R', diameter: 'Ø', angle: '∠', point_line_distance: '↔',
};

export function installConstraints(studio, document) {
  const { html } = studio;
  const view = document.getElementById('view');
  const bar = html('div', { id: 'constraint-bar', role: 'toolbar', 'aria-label': 'Constraints for the selection', hidden: true });
  const valueBox = html('div', { id: 'value-box', hidden: true });
  view.append(bar, valueBox);
  const style = html('style', {}, `
    #constraint-bar, #value-box { position: absolute; z-index: 2; display: flex; gap: 4px; align-items: center; flex-wrap: wrap;
      max-width: min(420px, calc(100% - 24px)); padding: 5px; background: var(--panel); border: 1px solid var(--line);
      border-radius: 8px; box-shadow: 0 4px 14px rgb(0 0 0 / 0.18); }
    #constraint-bar button { padding: 3px 7px; }
    #constraint-bar select { width: auto; }
    #value-box input { width: 110px; }
    #value-box span { color: var(--muted); }
    .inspector-list { display: grid; gap: 4px; margin: 6px 0 12px; padding: 0; list-style: none; }
    .inspector-list li { display: flex; justify-content: space-between; gap: 6px; align-items: center; }
    .state-free { color: var(--free); } .state-fixed { color: var(--fixed); } .state-conflict { color: var(--conflict); }`);
  document.head.append(style);

  studio.selection = [];
  let dimension = null; // { ids, spec, at: [sx, sy] }
  let valueInput = null;
  /** True until the first key replaces the measured value shown. */
  let prefilled = false;
  let drag = null; // { point, start, current }

  const layer = () => studio.activeLayer();
  const scene = () => (layer() ? studio.sceneOf(layer().feature) : null);
  /** The definition constraints are added to: the open draft, or the layer's sketch. */
  const definition = () => studio.draft?.feature === layer()?.feature ? studio.draft.definition : studio.definitions[layer()?.feature] ?? null;

  function screenAt(event) {
    const box = view.getBoundingClientRect();
    return [event.clientX - box.left, event.clientY - box.top];
  }
  function hit(event) {
    const l = layer();
    const s = scene();
    if (!l?.frame || !s) return null;
    const { matrix, width, height } = studio.viewport();
    return hitTest({ scene: s, frame: l.frame, matrix, width, height, at: screenAt(event), radius: 7 });
  }

  function select(ids) {
    studio.selection = ids;
    studio.refresh();
  }
  studio.clearSelection = () => select([]);

  /** Adds a constraint to the draft, opening one if needed. */
  function addConstraint(constraint, note) {
    const draft = studio.ensureDraft();
    if (!draft) {
      studio.say(`Apply or discard the draft of ${studio.draft?.feature} first.`, true);
      return false;
    }
    const key = JSON.stringify(constraint);
    if ((draft.definition.constraints ?? []).some((c) => JSON.stringify(c) === key)) {
      studio.say('That constraint is already there.', true);
      return false;
    }
    const next = structuredClone(draft.definition);
    next.constraints = [...(next.constraints ?? []), constraint];
    studio.changeDraft(next);
    studio.say(`${note} added. Previewing…`);
    return true;
  }

  function applySuggestion(suggestion, valueText = null) {
    const def = definition();
    if (!def) return;
    try {
      let expression;
      if (suggestion.dimension) {
        if (valueText === null) {
          openValueBox(suggestion, studio.selection);
          return;
        }
        expression = dimensionExpression(valueText, suggestion.dimension === 'angle' ? '°' : 'mm');
        if (!expression) throw new Error('Enter a number or a parameter name.');
      }
      const constraint = makeConstraint(def, suggestion.type, { ...suggestion.fields, ...suggestion.options }, expression);
      if (addConstraint(constraint, constraintTools[suggestion.type].label)) {
        closeValueBox();
        select([]);
      }
    } catch (error) {
      studio.say(error.message, true);
    }
  }

  // ── Inline value box (dimensions) ────────────────────────────────────
  function openValueBox(suggestion, ids, at = null) {
    const def = definition();
    const measured = dimensionFor(def, scene(), ids);
    const sameKind = measured && measured.type === suggestion.type;
    const unit = suggestion.dimension === 'angle' ? '°' : 'mm';
    dimension = { suggestion, ids, at: at ?? anchorOf(ids) };
    const input = valueInput = html('input', {
      'aria-label': `${suggestion.label} value`,
      value: sameKind ? Number(measured.value.toFixed(4)).toString() : '',
      placeholder: 'value or parameter',
      onkeydown: (event) => {
        event.stopPropagation();
        if (event.key === 'Enter') applySuggestion(suggestion, input.value);
        else if (event.key === 'Escape') closeValueBox();
      },
    });
    studio.fill(valueBox, html('span', {}, suggestion.label), input, html('span', {}, unit),
      html('button', { onclick: () => applySuggestion(suggestion, input.value) }, 'Add'));
    valueBox.hidden = false;
    place(valueBox, dimension.at);
    prefilled = true;
    // After the click that opened it has finished.
    setTimeout(() => {
      if (valueInput !== input) return;
      input.focus();
      if (prefilled) input.select();
    }, 0);
    input.addEventListener('input', () => { prefilled = false; });
  }
  function closeValueBox() {
    dimension = null;
    valueInput = null;
    valueBox.hidden = true;
  }

  function place(node, at) {
    if (!at) return;
    const box = view.getBoundingClientRect();
    node.style.left = `${Math.max(8, Math.min(box.width - node.offsetWidth - 8, at[0] + 14))}px`;
    node.style.top = `${Math.max(8, Math.min(box.height - node.offsetHeight - 8, at[1] + 14))}px`;
  }

  /** Screen position near the selection: its points' or curves' centroid. */
  function anchorOf(ids) {
    const s = scene();
    const points = [];
    for (const id of ids) {
      const p = s?.points?.[id];
      if (p) points.push(p);
      const entity = s?.entities?.find((e) => e.id === id);
      if (entity?.points?.length) points.push(entity.points[Math.floor(entity.points.length / 2)]);
    }
    const screens = points.map((p) => studio.toScreen(p)).filter(Boolean);
    if (!screens.length) return null;
    return [screens.reduce((t, p) => t + p[0], 0) / screens.length, screens.reduce((t, p) => t + p[1], 0) / screens.length];
  }

  // ── Constraint bar ───────────────────────────────────────────────────
  function renderBar() {
    const def = definition();
    const options = def && studio.tool === 'select' ? suggestions(def, studio.selection) : [];
    if (!options.length) {
      bar.hidden = true;
      return;
    }
    const buttons = options.slice(0, BAR_BUTTONS).map((s, i) => html('button', {
      title: `${s.label}${i === 0 ? ' (Enter)' : ''}`, onclick: () => applySuggestion(s),
    }, `${SYMBOLS[s.type] ?? ''} ${s.label}`.trim()));
    const rest = options.slice(BAR_BUTTONS);
    const menu = rest.length ? html('select', {
      'aria-label': 'More constraints',
      onchange: (event) => { const s = rest[Number(event.target.value)]; event.target.value = ''; if (s) applySuggestion(s); },
    }, html('option', { value: '' }, `More (${rest.length})…`), ...rest.map((s, i) => html('option', { value: i }, s.label))) : null;
    studio.fill(bar, buttons, menu);
    bar.hidden = false;
    place(bar, anchorOf(studio.selection));
  }

  // ── Select tool: selection and drags ────────────────────────────────
  studio.tools.set('select', {
    down(event) {
      if (event.button !== 0) return false;
      const target = hit(event);
      if (!target) return false;
      const additive = event.shiftKey || event.ctrlKey || event.metaKey;
      if (target.kind === 'point' && !additive && draggable(target.id)) {
        drag = { point: target.id, start: screenAt(event), current: null, moved: false };
      }
      const ids = additive
        ? studio.selection.includes(target.id) ? studio.selection.filter((id) => id !== target.id) : [...studio.selection, target.id]
        : [target.id];
      select(ids);
      return true;
    },
    move(event, captured) {
      if (!captured || !drag) return;
      const [x, y] = screenAt(event);
      drag.moved ||= Math.hypot(x - drag.start[0], y - drag.start[1]) > 3;
      drag.current = studio.localAt(event.clientX, event.clientY);
      studio.redraw();
    },
    async up() {
      const pending = drag;
      drag = null;
      studio.redraw();
      if (!pending?.moved || !pending.current) return;
      await dragPoint(pending.point, pending.current);
    },
    click(event) {
      // A plain click on empty space clears the selection; Shift keeps it.
      if (studio.selection.length && !event.shiftKey) select([]);
    },
  });

  /** A free point with literal coordinates can be dragged. */
  function draggable(id) {
    const point = (definition()?.points ?? []).find((p) => p.id === id);
    return Boolean(point && !point.fixed && point.x?.literal && point.y?.literal);
  }

  async function dragPoint(point, target) {
    const draft = studio.ensureDraft();
    if (!draft || draft.local) {
      studio.say(draft?.local ? 'Apply the new sketch before dragging its points.' : `Apply or discard the draft of ${studio.draft?.feature} first.`, true);
      return;
    }
    studio.say('Solving the drag with the existing constraints…');
    try {
      const response = await studio.api('/api/sketch-drag', {
        method: 'POST',
        body: JSON.stringify({
          version: studio.version, instance: studio.instance, feature: draft.feature,
          expected: draft.baseline, definition: draft.definition, point, target,
        }),
      });
      studio.changeDraft(response.definition);
      studio.say(`${point} moved; the solver kept every constraint. Apply to keep it.`);
    } catch (error) {
      studio.say(`Drag rejected: ${error.message}`, true);
    }
  }

  // ── Dimension tool ───────────────────────────────────────────────────
  let dimensionIds = [];
  studio.tools.set('dimension', {
    down(event) {
      if (event.button !== 0) return false;
      const target = hit(event);
      const def = definition();
      if (!target || !def) {
        studio.say('Click a line, circle or arc, or two points or lines.', true);
        return true;
      }
      dimensionIds = dimensionIds.length >= 2 ? [target.id] : [...dimensionIds, target.id];
      let spec = dimensionFor(def, scene(), dimensionIds);
      if (!spec && dimensionIds.length === 2) {
        dimensionIds = [target.id];
        spec = dimensionFor(def, scene(), dimensionIds);
      }
      studio.selection = [...dimensionIds];
      if (!spec) {
        studio.say(`${target.id} needs a second point or line to dimension.`);
        closeValueBox();
      } else {
        const tool = constraintTools[spec.type];
        openValueBox({ type: spec.type, label: spec.label, fields: spec.fields, options: {}, dimension: tool.dimension }, dimensionIds, screenAt(event));
        studio.say(`${spec.label}: type a value or parameter and press Enter, or click another point or line.`);
      }
      studio.refresh();
      return true;
    },
  });

  // ── Keys ─────────────────────────────────────────────────────────────
  const drawKey = studio.onKey;
  studio.onKey = (event) => {
    // Keys typed while a value box is open go into it, even before it has
    // focus (the click that opened it may still be finishing).
    if (dimension && valueInput) {
      if (event.key === 'Enter') {
        applySuggestion(dimension.suggestion, valueInput.value);
        return true;
      }
      if (event.key.length === 1 && /[\w.\-]/.test(event.key)) {
        event.preventDefault();
        if (prefilled) valueInput.value = '';
        prefilled = false;
        valueInput.value += event.key;
        valueInput.focus();
        return true;
      }
    }
    if (event.key === 'Enter' && studio.tool === 'select' && studio.selection.length) {
      const first = suggestions(definition() ?? {}, studio.selection)[0];
      if (first) applySuggestion(first);
      return true;
    }
    if (event.key === 'Escape' && (studio.selection.length || dimension)) {
      closeValueBox();
      dimensionIds = [];
      select([]);
      return true;
    }
    return drawKey?.(event) ?? false;
  };

  // ── Overlay and inspector ───────────────────────────────────────────
  const drawDecorate = studio.decorate;
  studio.decorate = (context) => {
    const nodes = drawDecorate(context);
    const l = layer();
    if (drag?.current && l?.frame) {
      const from = scene()?.points?.[drag.point];
      const d = from && pathData([from, drag.current], l.frame, context.matrix, context.width, context.height);
      if (d) nodes.push(context.element('path', { d, class: 'ghost' }));
    }
    return nodes;
  };

  studio.entityStyle = (sceneData) => {
    const states = solveStates(sceneData);
    const selected = new Set(studio.selection);
    return {
      curve: (entity) => ({ state: states.curve(entity), selected: selected.has(entity.id) }),
      point: (id) => ({ state: states.point(id), selected: selected.has(id) }),
    };
  };

  studio.renderSelection = (host) => {
    renderBar();
    if (!studio.selection.length) return false;
    const def = definition();
    const s = scene();
    const states = s ? solveStates(s) : null;
    const entities = new Map((s?.entities ?? []).map((e) => [e.id, e]));
    const rows = studio.selection.map((id) => {
      const state = !states ? '' : entities.has(id) ? states.curve(entities.get(id)) : states.point(id);
      return html('li', {}, html('span', {}, id), html('span', { class: `state-${state}` }, state));
    });
    const touching = def ? constraintsTouching(def, studio.selection) : [];
    const constraints = touching.map((index) => {
      const constraint = def.constraints[index];
      const [type, body] = Object.entries(constraint)[0];
      const parts = Object.entries(body).filter(([, v]) => typeof v === 'string').map(([, v]) => v).join(', ');
      return html('li', {},
        html('span', {}, `${SYMBOLS[type] ?? ''} ${constraintTools[type]?.label ?? type} · ${parts}`),
        html('button', { title: 'Remove this constraint from the draft', onclick: () => removeConstraint(index) }, 'Remove'));
    });
    const options = def ? suggestions(def, studio.selection) : [];
    studio.fill(host,
      html('h2', {}, `Selection (${studio.selection.length})`),
      html('ul', { class: 'inspector-list' }, rows),
      pointEditor(def),
      html('h2', {}, `Constraints on it (${touching.length})`),
      touching.length ? html('ul', { class: 'inspector-list' }, constraints) : html('p', { style: 'color:var(--muted)' }, 'None.'),
      html('h2', {}, 'Add'),
      options.length
        ? html('div', { style: 'display:flex;flex-wrap:wrap;gap:4px' }, options.map((o) => html('button', { onclick: () => applySuggestion(o) }, o.label)))
        : html('p', { style: 'color:var(--muted)' }, studio.selection.length > 3
          ? 'Constraints take one to three items; select fewer.'
          : 'Nothing applies to this selection on its own. Shift-click more geometry to relate it.'),
      html('p', { style: 'color:var(--muted)' }, 'Shift-click adds to the selection. Enter applies the first suggestion; drag a free point to move it.'));
    return true;
  };

  /**
   * For one selected authored point with literal coordinates: its x and y
   * (sketch-local millimetres, solver starting values unless fixed) and
   * whether it is fixed.
   */
  function pointEditor(def) {
    if (studio.selection.length !== 1 || !def) return null;
    const point = (def.points ?? []).find((p) => p.id === studio.selection[0]);
    if (!point?.x?.literal || !point?.y?.literal) return null;
    const update = (change) => {
      const draft = studio.ensureDraft();
      if (!draft) {
        studio.say(`Apply or discard the draft of ${studio.draft?.feature} first.`, true);
        return;
      }
      const next = structuredClone(draft.definition);
      change(next.points.find((p) => p.id === point.id));
      studio.changeDraft(next);
      studio.say(`${point.id} changed in the draft. Previewing…`);
    };
    const coordinate = (axis) => html('label', { style: 'display:grid;gap:3px;color:var(--muted);font-size:12px' }, `${axis.toUpperCase()} (mm)`,
      html('input', {
        type: 'number', step: 'any', value: point[axis].literal.value,
        onchange: (event) => {
          const value = Number(event.target.value);
          if (!event.target.value.trim() || !Number.isFinite(value)) {
            studio.say('Enter a finite coordinate.', true);
            return;
          }
          update((p) => { p[axis] = { literal: { value, dimension: 'length', unit: 'millimeter' } }; });
        },
      }));
    return html('div', { style: 'display:grid;grid-template-columns:1fr 1fr;gap:6px;margin-bottom:10px' },
      coordinate('x'), coordinate('y'),
      html('label', { style: 'grid-column:span 2;display:flex;gap:6px;align-items:center' },
        html('input', { type: 'checkbox', checked: Boolean(point.fixed), style: 'width:auto', onchange: (event) => update((p) => { p.fixed = event.target.checked; }) }),
        'Fixed in place'));
  }

  function removeConstraint(index) {
    const draft = studio.ensureDraft();
    if (!draft) return;
    const next = structuredClone(draft.definition);
    next.constraints.splice(index, 1);
    studio.changeDraft(next);
    studio.say('Constraint removed from the draft. Previewing…');
  }

  const activate = studio.onActivate;
  studio.onActivate = (l) => {
    closeValueBox();
    dimensionIds = [];
    studio.selection = [];
    activate?.(l);
  };
  const setTool = studio.setTool;
  studio.setTool = (id) => {
    closeValueBox();
    dimensionIds = [];
    if (id !== 'select') studio.selection = [];
    setTool(id);
  };
}
