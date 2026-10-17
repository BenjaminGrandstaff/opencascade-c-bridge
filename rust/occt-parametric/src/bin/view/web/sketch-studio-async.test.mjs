import test from 'node:test';
import assert from 'node:assert/strict';
import {fixture} from './sketch-studio-test-fixture.mjs';
import {installConstraints} from './sketch-studio-constrain.mjs';
import {basePlane, lookAt, projectLocal} from './sketch-studio-plane.mjs';
import {viewProjection} from './viewer.mjs';

function deferred() {
  let resolve, reject;
  const promise = new Promise((a, b) => { resolve = a; reject = b; });
  return {promise, resolve, reject};
}

test('Apply sends one immutable edit and locks history and edits through reload', async t => {
  const request = deferred(), reload = deferred();
  const f = fixture(t, {api: () => request.promise});
  f.edit(1);
  const d = f.studio.draft, snapshot = d.definition;
  f.studio.reload = () => reload.promise;
  const apply = f.drawing.apply();
  assert.equal(f.studio.modelPending, 'Applying');
  assert.equal(f.studio.busy, true);
  for (const label of ['Apply', 'Discard', 'Undo', 'Redo', 'Save']) assert(f.button(label).attrs.disabled, label);
  await f.drawing.apply();
  f.drawing.undo(); f.drawing.redo(); f.drawing.discard();
  assert.equal(f.studio.ensureDraft(), null);
  assert.equal(f.studio.changeDraft({...snapshot, points: []}), false);
  assert.equal(f.calls.length, 1);
  assert.deepEqual(JSON.parse(f.calls[0].body).definition, snapshot);
  assert.equal(f.studio.draft.definition, snapshot);
  request.resolve({applied: true}); await f.flush();
  assert.equal(f.studio.draft, null);
  assert.equal(f.studio.busy, true);
  assert.equal(f.studio.ensureDraft(), null);
  reload.resolve(); await apply;
  assert.equal(f.studio.modelPending, null);
  assert.equal(f.studio.busy, false);
});

test('Apply rejection preserves the exact draft and unlocks it for retry', async t => {
  const request = deferred();
  const f = fixture(t, {api: () => request.promise}); f.edit(1);
  const d = f.studio.draft;
  const apply = f.drawing.apply(); request.reject(Error('revision conflict')); await apply;
  assert.equal(f.studio.draft, d); assert.equal(d.undo.length, 1);
  assert.equal(f.studio.modelPending, null); assert.equal(f.studio.busy, true);
  assert.match(f.studio.message, /Apply rejected: revision conflict/);
  f.studio.api = async () => ({applied: true}); await f.drawing.apply();
  assert.equal(f.studio.draft, null);
});

test('a successful Apply followed by reload failure is reported as applied and cannot be retried', async t => {
  const f = fixture(t, {api: () => ({applied: true})}); f.edit(1);
  f.studio.reload = async () => { throw Error('offline'); };
  await f.drawing.apply(); await f.drawing.apply();
  assert.equal(f.calls.length, 1); assert.equal(f.studio.draft, null);
  assert.equal(f.studio.modelPending, null);
  assert.match(f.studio.message, /applied, but refreshing the view failed: offline/);
});

for (const outcome of ['success', 'failure']) {
  test(`old Preview ${outcome} cannot alter a newer edit during its debounce`, async t => {
    const request = deferred(); const f = fixture(t, {api: () => request.promise});
    f.edit(1); t.mock.timers.tick(150); await f.flush(); f.edit(2);
    const message = f.studio.message;
    if (outcome === 'success') request.resolve({can_apply: false, error: 'old invalid geometry'});
    else request.reject(Error('old failure'));
    await f.flush(); assert.equal(f.studio.draft.error, null);
    assert.equal(f.studio.draft.previewing, true); assert.equal(f.studio.message, message);
    f.drawing.discard(true);
  });
}

test('Preview cannot interrupt an in-flight Apply or reappear after Apply rejection', async t => {
  const preview = deferred(), request = deferred();
  const f = fixture(t, {api: (path, options) => JSON.parse(options.body).apply ? request.promise : preview.promise});
  f.edit(1); t.mock.timers.tick(150); await f.flush();
  const apply = f.drawing.apply();
  preview.resolve({can_apply: false, error: 'obsolete'}); await f.flush();
  assert.match(f.studio.message, /^Applying:/); assert.equal(f.studio.draft.error, null);
  request.resolve({applied: false, error: 'requirements failed'}); await apply;
  assert.match(f.studio.message, /Apply rejected: requirements failed/);
  assert.equal(f.studio.draft.error, null); assert.equal(f.studio.draft.previewing, false);
  f.drawing.discard(true);
});

test('model Undo/Redo and Save serialize requests and block draft creation until reload', async t => {
  const request = deferred(); const f = fixture(t, {api: () => request.promise});
  f.drawing.undo(); f.drawing.redo();
  f.button('Save').attrs.onclick();
  assert.equal(f.calls.length, 1); assert.equal(f.studio.ensureDraft(), null);
  request.resolve({}); await f.flush();
  assert.equal(f.studio.modelPending, null);
  assert(f.studio.ensureDraft());
});

function dragFixture(t) {
  const requests = [];
  const f = fixture(t, {api: (path, options) => {
    assert.equal(path, '/api/sketch-drag');
    const pending = deferred(); requests.push({...pending, body: JSON.parse(options.body)}); return pending.promise;
  }});
  const mm = value => ({literal: {value, dimension: 'length', unit: 'millimeter'}});
  f.baseline.points = [{id: 'P', x: mm(0), y: mm(0), fixed: false}];
  f.layer.frame = basePlane('xy');
  f.studio.scenes[0].points = {P: [0, 0, 0]};
  f.studio.sceneOf = feature => f.studio.scenes.find(s => s.feature === feature);
  const matrix = viewProjection(lookAt({target: [0, 0, 0], distance: .05, yaw: 0, pitch: 0}, f.layer.frame, [0, 0]), 1);
  f.studio.viewport = () => ({matrix, width: 400, height: 400});
  f.studio.localAt = () => [5, 3];
  installConstraints(f.studio, f.document);
  const tool = f.studio.tools.get('select');
  const xy = projectLocal(matrix, f.layer.frame, [0, 0, 0], 400, 400);
  f.start = () => {
    assert(tool.down({button: 0, clientX: xy[0], clientY: xy[1]}));
    tool.move({clientX: xy[0] + 20, clientY: xy[1] + 10}, true);
    return tool.up();
  };
  f.result = request => ({definition: {...request.body.definition, moved: true}});
  return {...f, requests};
}

test('a current solved drag becomes one undoable draft edit', async t => {
  const f = dragFixture(t), done = f.start();
  assert.equal(f.requests.length, 1); f.requests[0].resolve(f.result(f.requests[0])); await done;
  assert.equal(f.studio.draft.definition.moved, true); assert.equal(f.studio.draft.undo.length, 1);
  f.drawing.undo(); assert.equal(f.studio.draft.definition.moved, undefined); f.drawing.discard(true);
});

for (const context of ['edit', 'undo-redo', 'discard', 'layer', 'version', 'tool', 'apply']) {
  test(`a solved drag cannot overwrite the draft after ${context}`, async t => {
    const f = dragFixture(t);
    if (['undo-redo', 'apply'].includes(context)) {
      f.studio.ensureDraft(); f.studio.changeDraft({...f.studio.draft.definition, revision: 1});
    }
    const done = f.start(), request = f.requests[0];
    if (context === 'edit') f.edit(2);
    if (context === 'undo-redo') { f.drawing.undo(); f.drawing.redo(); }
    if (context === 'discard') f.drawing.discard(true);
    if (context === 'layer') {
      f.studio.layers.push({feature: 'other'}); f.studio.activate('other'); f.studio.onActivate(f.studio.activeLayer());
      f.studio.activate('profile'); f.studio.onActivate(f.layer);
    }
    if (context === 'version') f.studio.version++;
    if (context === 'tool') f.studio.setTool('line');
    let apply;
    if (context === 'apply') {
      const accepted = deferred(); f.studio.api = () => accepted.promise;
      apply = f.drawing.apply(); accepted.resolve({applied: true}); await apply;
    }
    const d = f.studio.draft, definition = d?.definition, message = f.studio.message;
    request.resolve(f.result(request)); await done;
    assert.equal(f.studio.draft, d); assert.equal(d?.definition, definition); assert.equal(f.studio.message, message);
    f.drawing.discard(true);
  });
}

test('only the newest overlapping drag may change the draft or report an error', async t => {
  const f = dragFixture(t), first = f.start(), second = f.start();
  f.requests[1].resolve(f.result(f.requests[1])); await second;
  const snapshot = f.studio.draft.definition, message = f.studio.message;
  f.requests[0].reject(Error('old solve failed')); await first;
  assert.equal(f.studio.draft.definition, snapshot); assert.equal(f.studio.draft.undo.length, 1);
  assert.equal(f.studio.message, message); f.drawing.discard(true);
});

test('an older drag success is ignored even while the newest solve is still pending', async t => {
  const f = dragFixture(t), first = f.start(), second = f.start();
  const original = f.studio.draft.definition;
  f.requests[0].resolve(f.result(f.requests[0])); await first;
  assert.equal(f.studio.draft.definition, original); assert.equal(f.studio.draft.undo.length, 0);
  f.requests[1].resolve(f.result(f.requests[1])); await second;
  assert.equal(f.studio.draft.undo.length, 1); f.drawing.discard(true);
});

test('a current drag rejection leaves geometry and history intact', async t => {
  const f = dragFixture(t), done = f.start(), original = f.studio.draft.definition;
  f.requests[0].reject(Error('point cannot move')); await done;
  assert.equal(f.studio.draft.definition, original); assert.equal(f.studio.draft.undo.length, 0);
  assert.match(f.studio.message, /Drag rejected: point cannot move/); f.drawing.discard(true);
});
