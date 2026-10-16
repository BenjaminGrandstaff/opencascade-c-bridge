import test from 'node:test';
import assert from 'node:assert/strict';
import { infer, pointPositions, rulesToConstraints, tangentLineAt } from './sketch-studio-infer.mjs';

const mm = (value) => ({ literal: { value, dimension: 'length', unit: 'millimeter' } });
const point = (id, x, y) => ({ id, x: mm(x), y: mm(y), fixed: false });
// An L: a horizontal line A→B (10 mm) and a 30° line B→C; a circle at (30, 0) r 5.
const definition = {
  points: [point('A', 0, 0), point('B', 10, 0), point('C', 10 + 8 * Math.cos(Math.PI / 6), 8 * Math.sin(Math.PI / 6)),
    point('O', 30, 0), point('R', 35, 0)],
  lines: [{ id: 'L1', start: 'A', end: 'B' }, { id: 'L2', start: 'B', end: 'C' }],
  circles: [{ id: 'C1', center: 'O', rim: 'R' }],
};
const scene = {
  entities: [{ id: 'C1', points: Array.from({ length: 65 }, (_, i) => [30 + 5 * Math.cos(i * Math.PI / 32), 5 * Math.sin(i * Math.PI / 32), 0]) }],
};
const near = (a, b, t = 1e-9) => assert.ok(Math.abs(a[0] - b[0]) <= t && Math.abs(a[1] - b[1]) <= t, `${a} vs ${b}`);

test('positions prefer solved scene points over draft literals', () => {
  const positions = pointPositions(definition, { A: [0.5, 0, 0] });
  near(positions.get('A'), [0.5, 0]);
  near(positions.get('B'), [10, 0]);
});

test('existing points win, then midpoints, then curves', () => {
  let r = infer({ xy: [10.2, 0.1], tool: 'line', definition, scene, tolerance: 0.5 });
  assert.equal(r.point, 'B');
  r = infer({ xy: [5.1, 0.2], tool: 'line', definition, scene, tolerance: 0.5 });
  near(r.xy, [5, 0]);
  assert.deepEqual(r.rules, [{ kind: 'midpoint', line: 'L1' }]);
  r = infer({ xy: [30, 5.2], tool: 'point', definition, scene, tolerance: 0.5 });
  assert.deepEqual(r.rules, [{ kind: 'on_curve', curve: 'C1' }]);
  assert.ok(Math.abs(Math.hypot(r.xy[0] - 30, r.xy[1]) - 5) < 0.02);
  // Without automatic constraints only points snap.
  r = infer({ xy: [5.1, 0.2], tool: 'line', definition, scene, tolerance: 0.5, auto: false });
  near(r.xy, [5.1, 0.2]);
  assert.deepEqual(r.rules, []);
});

test('lines near horizontal or vertical snap onto the axis and say so', () => {
  const anchor = { xy: [0, 20] };
  let r = infer({ xy: [10, 20.3], tool: 'line', definition, scene, tolerance: 0.1, anchor });
  near(r.xy, [Math.hypot(10, 0.3), 20]);
  assert.deepEqual(r.rules, [{ kind: 'horizontal' }]);
  r = infer({ xy: [-0.2, 5], tool: 'line', definition, scene, tolerance: 0.1, anchor });
  assert.deepEqual(r.rules, [{ kind: 'vertical' }]);
  assert.ok(Math.abs(r.xy[0]) < 1e-9 && r.xy[1] < 20);
  r = infer({ xy: [10, 25], tool: 'line', definition, scene, tolerance: 0.1, anchor });
  assert.deepEqual(r.rules, []);
});

test('a hovered line makes nearby directions parallel or perpendicular, and equal lengths equal', () => {
  const reference = { kind: 'line', id: 'L2', angle: Math.PI / 6, length: 8 };
  const anchor = { xy: [0, 20] };
  const at = (angle, length) => [Math.cos(angle) * length, 20 + Math.sin(angle) * length];
  let r = infer({ xy: at(Math.PI / 6 + 0.03, 15), tool: 'line', definition, scene, tolerance: 0.1, anchor, reference });
  assert.deepEqual(r.rules, [{ kind: 'parallel', line: 'L2' }]);
  near(r.xy, at(Math.PI / 6, 15));
  r = infer({ xy: at(Math.PI / 6 + Math.PI / 2 - 0.02, 8.05), tool: 'line', definition, scene, tolerance: 0.1, anchor, reference });
  assert.deepEqual(r.rules, [{ kind: 'perpendicular', line: 'L2' }, { kind: 'equal_length', line: 'L2' }]);
  near(r.xy, at(Math.PI / 6 + Math.PI / 2, 8), 1e-9);
});

test('an end snapped to a point keeps only rules that already hold', () => {
  // From (0, 0.1) to B (10, 0): 0.6° off horizontal, but B cannot move.
  const r = infer({ xy: [10, 0], tool: 'line', definition, scene, tolerance: 0.5, anchor: { xy: [0, 0.1] } });
  assert.equal(r.point, 'B');
  assert.deepEqual(r.rules, []);
});

test('circles near a hovered radius become equal to it', () => {
  const reference = { kind: 'round', id: 'C1', radius: 5 };
  const r = infer({ xy: [50, 5.03], tool: 'circle', definition, scene, tolerance: 0.01, anchor: { xy: [50, 0] }, reference });
  assert.deepEqual(r.rules, [{ kind: 'equal_radius', curve: 'C1' }]);
  near(r.xy, [50, 5]);
});

test('hovering reports the line or circle under the cursor', () => {
  assert.equal(infer({ xy: [3, 0.05], tool: 'line', definition, scene, tolerance: 0.2 }).hovered.id, 'L1');
  assert.equal(infer({ xy: [30, -5.05], tool: 'line', definition, scene, tolerance: 0.2 }).hovered.id, 'C1');
});

test('arcs whose centre lies on a line end normal are tangent to it', () => {
  // L1 runs along x to B (10, 0): a centre straight above B is tangent.
  assert.equal(tangentLineAt(definition, {}, 'B', [10, 4]), 'L1');
  assert.equal(tangentLineAt(definition, {}, 'B', [12, 4]), null);
  assert.equal(tangentLineAt(definition, {}, null, [10, 4]), null);
});

test('rules become constraints on the new point and shape', () => {
  const clicks = [
    { id: 'P1', rules: [{ kind: 'on_curve', curve: 'C1' }] },
    { id: 'P2', rules: [{ kind: 'horizontal' }, { kind: 'equal_length', line: 'L1' }, { kind: 'midpoint', line: 'L2' }] },
  ];
  assert.deepEqual(rulesToConstraints(clicks, 'L9'), [
    { point_on_curve: { point: 'P1', curve: 'C1' } },
    { horizontal: { line: 'L9' } },
    { equal_length: { first: 'L1', second: 'L9' } },
    { midpoint: { point: 'P2', line: 'L2' } },
  ]);
  assert.deepEqual(rulesToConstraints([{ id: 'S', rules: [{ kind: 'tangent', line: 'L1' }] }], 'A1'), [
    { tangent: { first: 'L1', second: 'A1', point: 'S' } },
  ]);
});
