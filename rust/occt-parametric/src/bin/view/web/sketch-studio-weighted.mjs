// Weighted named-point authoring. Positions are review seeds from the solved
// scene; native Preview remains authoritative for expressions and constraints.
import {createWeightedTermControls, validateWeightedTerms} from './weighted-point-controls.mjs';
import {linkNamedPoint, pointLinks} from './point-links.mjs';
import {freshName, validateName} from './geometry-names.mjs';
import {inheritGeneratedReferences} from './generated-points.mjs';

const length = value => ({literal: {value, dimension: 'length', unit: 'millimeter'}});
const scalar = value => ({literal: {value, dimension: 'scalar', unit: null}});

/** Stable positive-weight average. null means expression weights need Preview. */
export function weightedPosition(terms, positions) {
  if (!terms.length) return null;
  const values = terms.map(term => term.weight?.literal?.value);
  if (values.some(value => !Number.isFinite(value) || value <= 0)) return null;
  const maximum = Math.max(...values);
  let total = 0, mean = [0, 0];
  for (let i = 0; i < terms.length; i++) {
    const point = positions[terms[i].point];
    if (!Array.isArray(point) || !Number.isFinite(point[0]) || !Number.isFinite(point[1])) throw Error('A source point has no finite solved position.');
    const weight = values[i] / maximum, next = total + weight;
    if (weight) {
      const fraction = weight / next;
      mean = mean.map((value, axis) => value * (1 - fraction) + point[axis] * fraction);
    }
    total = next;
  }
  return mean;
}

export function pickedWeightedPoint(definition, {point, name, terms, positions}) {
  const checked = validateWeightedTerms(definition, point, terms);
  let next = definition, target = point;
  if (!target) {
    validateName(definition, name);
    // Expressions are evaluated by native Preview; an equal-source average is
    // only the starting coordinate for the new point in that case.
    const seed = weightedPosition(checked, positions) ?? weightedPosition(checked.map(term => ({...term, weight: scalar(1)})), positions);
    next = structuredClone(definition);
    inheritGeneratedReferences(definition, next);
    target = name;
    (next.points ??= []).push({id: target, x: length(seed[0]), y: length(seed[1]), fixed: false});
  }
  return {point: target, definition: linkNamedPoint(next, {point: target, mode: 'weighted', terms: checked})};
}

export function createWeightedPointPicker({host, definition, positions, parameters = [], current = () => true,
  beginPicking = () => {}, modeChanged = () => {}, highlight = () => {}, ghost = () => {}, changed, message}) {
  const section = document.createElement('div'); host.append(section);
  const note = document.createElement('p');
  note.textContent = 'Pick source points on the canvas and review their positive weights. Equal weights make an average or midpoint. Stage creates or relinks one named point; Preview evaluates parameter/formula weights, dependent sources and every model variant. Relinking unfixes the target and replaces its saved point links; other dimensions remain active.';
  section.append(note);
  function field(label, tag = 'input') {
    const row = document.createElement('label'); row.textContent = label;
    const input = document.createElement(tag); input.setAttribute('aria-label', label); row.append(input); section.append(row); return input;
  }
  const target = field('Weighted point target', 'select'); target.append(new Option('New named point', ''));
  for (const point of definition.points ?? []) target.append(new Option(point.id, point.id));
  const name = field('New weighted point name'); name.value = freshName(definition, 'Weighted');
  const weights = createWeightedTermControls({host: section, definition, parameters, message});
  const status = document.createElement('p'); status.setAttribute('aria-live', 'polite'); status.setAttribute('data-weighted-status', ''); section.append(status);
  const start = document.createElement('button'), stage = document.createElement('button'), stop = document.createElement('button');
  start.textContent = 'Start weighted point pick'; stage.textContent = 'Stage weighted point'; stop.textContent = 'Cancel weighted point pick'; section.append(start, stage, stop);
  let pending = null, gesture = null;
  function valid() {
    if (current()) return true;
    cancel(); message('The sketch changed. Restart the weighted point pick.'); return false;
  }
  function update() {
    target.disabled = name.disabled = Boolean(pending); name.parentElement.hidden = Boolean(target.value);
    start.disabled = Boolean(pending); stop.disabled = !pending; stage.disabled = true;
    if (!pending) { status.textContent = 'Choose a target, then start picking source points.'; return; }
    try {
      const terms = validateWeightedTerms(definition, pending.point, weights.terms);
      const position = weightedPosition(terms, positions);
      const markers = terms.map(term => {
        const xy = positions[term.point];
        if (!xy || !Number.isFinite(xy[0]) || !Number.isFinite(xy[1])) throw Error('A source point has no finite solved position.');
        return [[xy[0], xy[1], 0]];
      });
      if (position) markers.push([[position[0], position[1], 0]]);
      ghost(markers); highlight(terms.map(term => term.point)); stage.disabled = false;
      status.textContent = position
        ? `Current-source weighted position: (${position.map(v => Number(v.toPrecision(8))).join(', ')}) mm. Preview re-solves the link before Apply.`
        : 'Weights include parameters or formulas. Preview computes the weighted point after staging; markers show the source points.';
    } catch (error) { ghost([]); status.textContent = error.message; }
  }
  function cancel() {
    const active = Boolean(pending); pending = gesture = null;
    if (active) { ghost([]); highlight([]); modeChanged('select'); }
    update(); return active;
  }
  target.onchange = () => {
    const saved = pointLinks(definition, target.value).find(link => link.weighted_point)?.weighted_point.terms;
    weights.load(target.value, saved ?? []); update();
  };
  start.onclick = () => {
    try {
      if (!valid()) return;
      if (!target.value) validateName(definition, name.value);
      beginPicking(); pending = {point: target.value, name: name.value}; modeChanged('weighted'); update();
      message('Click source points, review weights, then Stage weighted point.');
    } catch (error) { message(error.message); }
  };
  stage.onclick = () => {
    try {
      if (!pending || !valid()) return;
      const result = pickedWeightedPoint(definition, {...pending, terms: weights.terms, positions});
      cancel(); changed(result.definition, result.point); message(`Weighted point '${result.point}' staged. Preview verifies the link before Apply.`);
    } catch (error) { message(error.message); }
  };
  stop.onclick = () => { cancel(); message('Weighted point pick canceled. Draft unchanged.'); };
  for (const event of ['input', 'change', 'click']) section.addEventListener(event, () => { if (pending && valid()) update(); });
  weights.load('', []); update();
  return {cancel, get active() { return Boolean(pending); },
    down(event) {
      if (!pending) return false;
      if (!valid()) return true;
      if (!event.position) return true;
      gesture = {entity: event.entity, position: event.position, travel: 0, tolerance: Math.max((event.tolerance ?? 0) * .15, 1e-9)}; return true;
    },
    move(event) {
      if (!pending || !valid() || !gesture || !event.position) return;
      gesture.travel = Math.max(gesture.travel, Math.hypot(event.position[0] - gesture.position[0], event.position[1] - gesture.position[1]));
    },
    up(event) {
      const click = gesture; gesture = null;
      if (!pending || !click || !valid() || !event.position) return;
      if (Math.max(click.travel, Math.hypot(event.position[0] - click.position[0], event.position[1] - click.position[1])) > click.tolerance) { message('Use a click to add a source point; the drag did not change the weights.'); return; }
      try {
        if (weights.terms.some(term => term.point === click.entity)) throw Error('That source point is already included. Edit its weight instead.');
        weights.appendSource(click.entity); update();
      } catch (error) { message(error.message); }
    },
  };
}
