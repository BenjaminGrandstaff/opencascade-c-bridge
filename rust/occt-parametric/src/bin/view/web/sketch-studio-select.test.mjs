import test from 'node:test';
import assert from 'node:assert/strict';
import { viewProjection } from './viewer.mjs';
import { basePlane, lookAt } from './sketch-studio-plane.mjs';
import {
  constraintsTouching, dimensionExpression, dimensionFor, hitTest, solveStates, suggestions,
} from './sketch-studio-select.mjs';

const mm = (value) => ({ literal: { value, dimension: 'length', unit: 'millimeter' } });
const point = (id, x, y) => ({ id, x: mm(x), y: mm(y), fixed: false });
const definition = {
  points: [point('A', 0, 0), point('B', 10, 0), point('C', 10, 5), point('O', 20, 0), point('R', 23, 0),
    point('S', 20, 3), point('E', 17, 0), point('X', 4, 3)],
  lines: [{ id: 'L1', start: 'A', end: 'B' }, { id: 'L2', start: 'B', end: 'C' }],
  circles: [{ id: 'C1', center: 'O', rim: 'R' }],
  arcs: [{ id: 'A1', center: 'O', start: 'S', end: 'E', clockwise: false }],
  constraints: [{ horizontal: { line: 'L1' } }, { distance: { first: 'X', second: 'O', value: mm(1) } }],
};

test('selections suggest the natural constraints first, dimensions last', () => {
  const lines = suggestions(definition, ['L1', 'L2']).map((s) => s.type);
  assert.deepEqual(lines.slice(0, 3), ['parallel', 'perpendicular', 'tangent']);
  assert.ok(lines.includes('angle'));
  assert.ok(lines.indexOf('angle') > lines.indexOf('collinear'));
  // Two lines sharing B: tangency gets the shared point filled in.
  const tangent = suggestions(definition, ['L1', 'L2']).find((s) => s.type === 'tangent');
  assert.deepEqual(tangent.fields, { first: 'L1', second: 'L2', point: 'B' });
  assert.equal(suggestions(definition, ['A', 'B'])[0].type, 'coincident');
  const pointLine = suggestions(definition, ['X', 'L1']).map((s) => s.type);
  assert.deepEqual(pointLine.slice(0, 2), ['midpoint', 'point_on_curve']);
  // Options become separate suggestions.
  const sides = suggestions(definition, ['L1', 'C1']).filter((s) => s.type === 'line_circle_tangent');
  assert.deepEqual(sides.map((s) => s.options.side), ['left', 'right']);
  assert.deepEqual(suggestions(definition, ['C1', 'A1']).map((s) => s.type).slice(0, 2), ['equal_radius', 'concentric']);
  assert.deepEqual(suggestions(definition, []), []);
  assert.deepEqual(suggestions(definition, ['L1', 'L1']), []);
});

test('dimensions measure what the selection is', () => {
  assert.deepEqual(dimensionFor(definition, {}, ['L1']), { type: 'distance', fields: { first: 'A', second: 'B' }, value: 10, unit: 'mm', label: 'Length of L1' });
  assert.equal(dimensionFor(definition, {}, ['C1']).type, 'diameter');
  assert.equal(dimensionFor(definition, {}, ['C1']).value, 6);
  assert.equal(dimensionFor(definition, {}, ['A1']).value, 3);
  assert.equal(dimensionFor(definition, {}, ['A', 'C']).value, Math.hypot(10, 5));
  const pointLine = dimensionFor(definition, {}, ['X', 'L1']);
  assert.equal(pointLine.type, 'point_line_distance');
  assert.equal(pointLine.value, 3);
  const angle = dimensionFor(definition, {}, ['L1', 'L2']);
  assert.equal(angle.type, 'angle');
  assert.ok(Math.abs(angle.value - 90) < 1e-9);
  assert.equal(dimensionFor(definition, {}, ['O']), null);
});

test('typed dimension values become literals or parameter references', () => {
  assert.deepEqual(dimensionExpression('12.5', 'mm'), mm(12.5));
  assert.ok(Math.abs(dimensionExpression('90', '°').literal.value - Math.PI / 2) < 1e-12);
  assert.deepEqual(dimensionExpression('width', 'mm'), { parameter: 'width' });
  assert.equal(dimensionExpression('', 'mm'), null);
  assert.equal(dimensionExpression('1+', 'mm'), null);
});

test('constraints touching a selection are found for removal', () => {
  assert.deepEqual(constraintsTouching(definition, ['L1']), [0]);
  assert.deepEqual(constraintsTouching(definition, ['O']), [1]);
  assert.deepEqual(constraintsTouching(definition, ['C1']), []);
});

test('solve states mark free, conflicting and fixed geometry', () => {
  const scene = {
    solver: { diagnosis: {
      free_motions: [{ points: [{ point: 'C', dx: 0, dy: 1 }, { point: 'B', dx: 0, dy: 0 }] }],
      conflict_groups: [{ sources: [{ kind: 'constraint', index: 1 }, { kind: 'arc_radii', entity: 'A1' }] }],
      failed_constraints: [],
    } },
    annotations: [{ id: 'constraint-1', targets: ['X', 'O'] }],
  };
  const states = solveStates(scene);
  assert.equal(states.point('C'), 'free');
  assert.equal(states.point('B'), 'fixed');
  assert.equal(states.point('X'), 'conflict');
  assert.equal(states.curve({ id: 'L2', point_ids: ['B', 'C'] }), 'free');
  assert.equal(states.curve({ id: 'A1', point_ids: ['O', 'S', 'E'] }), 'conflict');
  assert.equal(states.curve({ id: 'L1', point_ids: ['A', 'B'] }), 'fixed');
});

test('hit testing prefers points, then the nearest curve', () => {
  const frame = basePlane('xy');
  const matrix = viewProjection(lookAt({ target: [0, 0, 0], distance: 0.05, yaw: 0, pitch: 0 }, frame, [5, 0]), 1);
  const scene = {
    points: { A: [0, 0, 0], B: [10, 0, 0] },
    entities: [{ id: 'L1', points: [[0, 0, 0], [10, 0, 0]] }],
  };
  const hit = (local) => {
    // Screen position of a local point, by projecting it.
    const s = hitTest({ scene, frame, matrix, width: 400, height: 400, at: local, radius: 6 });
    return s && [s.kind, s.id];
  };
  assert.deepEqual(hit([200, 200]), ['curve', 'L1']);
  assert.equal(hit([200, 150]), null);
});
