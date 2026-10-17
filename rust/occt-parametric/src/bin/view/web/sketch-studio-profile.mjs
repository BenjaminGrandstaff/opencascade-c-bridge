// The existing native curve/region operation controls, isolated from the live
// draft until the user submits an operation. Canvas clicks only fill fields.
import {createProfileControls} from './profile-controls.mjs';
import {inheritGeneratedReferences} from './generated-points.mjs';

export function createStudioProfileEditor({host, definition, parameters = [], current = () => true,
  beginPicking = () => {}, modeChanged = () => {}, highlight = () => {}, changed, message}) {
  const working = structuredClone(definition);
  inheritGeneratedReferences(definition, working);
  let controls, gesture = null, armed = false;
  function valid() {
    if (current()) return true;
    cancel(); message('The sketch changed. Reopen the curve editor.'); return false;
  }
  function cancel() {
    gesture = null;
    controls?.cancelPick();
    if (armed) { armed = false; modeChanged('select'); }
  }
  controls = createProfileControls({host, definition: working, parameters,
    initialType: 'trim_between',
    highlight: ids => { if (current()) highlight(ids); },
    message: text => { if (current()) message(text); },
    beginPicking: () => {
      if (!valid()) throw Error('The sketch changed. Reopen the curve editor.');
      beginPicking(); gesture = null; armed = true; modeChanged('profile');
    },
    changed: () => {
      if (!valid()) return;
      const next = structuredClone(working);
      inheritGeneratedReferences(working, next);
      cancel(); changed(next);
      message('Curve or region edit staged. Preview checks references, boundaries and dependent solids.');
    },
  });
  host.addEventListener('change', () => {
    if (armed && !controls.pickGroup) { armed = false; modeChanged('select'); }
  });
  return {cancel, get active() { return Boolean(controls.pickGroup); }, get pickGroup() { return controls.pickGroup; },
    down(event) {
      if (!controls.pickGroup) return false;
      if (!valid()) return true;
      if (!event.position) return true;
      gesture = {entity: event.entity, position: event.position, travel: 0, tolerance: Math.max((event.tolerance ?? 0) * .15, 1e-9)};
      return true;
    },
    move(event) {
      if (!controls.pickGroup || !valid() || !gesture || !event.position) return;
      gesture.travel = Math.max(gesture.travel, Math.hypot(event.position[0] - gesture.position[0], event.position[1] - gesture.position[1]));
    },
    up(event) {
      const click = gesture; gesture = null;
      if (!controls.pickGroup || !click || !valid() || !event.position) return;
      if (Math.max(click.travel, Math.hypot(event.position[0] - click.position[0], event.position[1] - click.position[1])) > click.tolerance) {
        message('Use a click to choose operation geometry; the drag left the form unchanged.'); return;
      }
      controls.pick(click.entity);
      if (!controls.pickGroup) { armed = false; modeChanged('select'); }
    },
  };
}
