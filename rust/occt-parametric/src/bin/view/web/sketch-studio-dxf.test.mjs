import test from 'node:test';
import assert from 'node:assert/strict';
import { localDefinition } from './sketch-studio-draw.mjs';

const mm = (value) => ({ literal: { value, dimension: 'length', unit: 'millimeter' } });

test('an imported DXF seeds a new layer on its chosen plane, empty layers stay empty', () => {
  const imported = {
    id: 'dxf', points: [{ id: 'p1', x: mm(0), y: mm(0) }, { id: 'p2', x: mm(10), y: mm(0) }, { id: 'p3', x: mm(5), y: mm(0) }],
    lines: [{ id: 'l1', start: 'p1', end: 'p2' }], arcs: [{ id: 'a1', center: 'p3', start: 'p2', end: 'p1' }],
    circles: [], splines: [], ellipses: [], profile: ['l1', 'a1'], holes: [], regions: [], constraints: [],
    origin: { literal: {} }, x_axis: { literal: {} }, y_axis: { literal: {} },
  };
  const layer = { feature: 'outline', create: { plane: 'xz', offset: 4, picked: null, imported } };
  const definition = localDefinition(layer);
  // The plane comes from the layer, the geometry from the file.
  assert.equal(definition.id, 'outline');
  assert.equal(definition.y_axis.literal.z.value, 1);
  assert.equal(definition.origin.literal.y.value, -4);
  assert.deepEqual(definition.profile, ['l1', 'a1']);
  assert.deepEqual(definition.arcs, imported.arcs);
  assert.equal(definition.points.length, 3);
  // Empty collections are not copied, and the file's own sketch is untouched.
  assert.equal(definition.holes, undefined);
  definition.points.pop();
  assert.equal(imported.points.length, 3);
  const empty = localDefinition({ feature: 'blank', create: { plane: 'xy', offset: 0, picked: null } });
  assert.deepEqual([empty.points, empty.lines, empty.profile], [[], [], []]);
});
