import test from 'node:test';
import assert from 'node:assert/strict';
import { basePlane } from './sketch-studio-plane.mjs';
import { layersFrom, nextFeatureName, planeLabel } from './sketch-studio-layers.mjs';

test('plane labels name the base plane and its offset, faces and datums', () => {
  assert.equal(planeLabel(basePlane('xy')), 'XY');
  assert.equal(planeLabel(basePlane('xz', 4)), 'XZ @ y = -4');
  assert.equal(planeLabel(basePlane('yz', 2.5)), 'YZ @ x = 2.5');
  const s = Math.SQRT1_2;
  assert.equal(planeLabel({ origin_mm: [0, 0, 0], x_axis: [s, s, 0], y_axis: [0, 0, 1] }), 'Inclined');
  assert.equal(planeLabel(basePlane('xy'), { face_support: {} }), 'Face');
  assert.equal(planeLabel(basePlane('xy'), { datum_plane: 'side' }), 'Datum side');
  assert.equal(planeLabel(null), 'Unresolved plane');
});

test('layers keep visibility, color and local drafts across reloads', () => {
  const scenes = [
    { kind: 'solid', feature: 'body' },
    { kind: 'sketch', feature: 'a', frame: basePlane('xy'), solver: { free_degrees: 2 } },
    { kind: 'sketch', feature: 'b', frame: basePlane('yz'), error: 'broken' },
  ];
  const first = layersFrom(scenes);
  assert.deepEqual(first.map((l) => [l.feature, l.plane, l.visible, l.freeDegrees, l.error]), [
    ['a', 'XY', true, 2, null],
    ['b', 'YZ', true, null, 'broken'],
  ]);
  first[0].visible = false;
  first[1].color = '#000000';
  const draft = { feature: 'sketch-1', frame: basePlane('xz'), plane: 'XZ', visible: true, color: '#111111', local: true };
  const second = layersFrom(scenes, {}, [...first, draft]);
  assert.equal(second[0].visible, false);
  assert.equal(second[1].color, '#000000');
  assert.equal(second.at(-1), draft);
  // Once the model has the feature, the local draft gives way to it.
  const created = layersFrom([...scenes, { kind: 'sketch', feature: 'sketch-1', frame: basePlane('xz') }], {}, second);
  assert.equal(created.filter((l) => l.feature === 'sketch-1').length, 1);
  assert.equal(created.at(-1).local, false);
});

test('new feature names skip taken ones', () => {
  assert.equal(nextFeatureName(['sketch-1', 'sketch-2', 'body']), 'sketch-3');
  assert.equal(nextFeatureName([]), 'sketch-1');
});
