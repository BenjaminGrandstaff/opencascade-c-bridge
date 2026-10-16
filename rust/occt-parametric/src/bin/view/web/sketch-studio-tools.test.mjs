import test from 'node:test';
import assert from 'node:assert/strict';
import { CLICKS, TOOLS, commitShape, typedClick } from './sketch-studio-tools.mjs';

const empty = () => ({ id: 's', points: [], lines: [], circles: [], arcs: [], profile: [], constraints: [] });
const click = (x, y, extra = {}) => ({ xy: [x, y], point: null, rules: [], ...extra });

test('every drawing tool has a key and a click count', () => {
  const keys = TOOLS.map((t) => t.key);
  assert.equal(new Set(keys).size, keys.length);
  for (const tool of TOOLS.filter((t) => t.id !== 'select')) assert.ok(CLICKS[tool.id], tool.id);
});

test('lines get their inferred constraints and continue the chain from their end', () => {
  const first = commitShape(empty(), 'line', [click(0, 0), click(10, 0, { rules: [{ kind: 'horizontal' }] })]);
  assert.deepEqual(first.constraints, [{ horizontal: { line: 'L1' } }]);
  assert.equal(first.definition.points.every((p) => !p.fixed), true);
  assert.deepEqual(first.chain, { xy: [10, 0], point: 'P2' });
  const second = commitShape(first.definition, 'line', [first.chain, click(10, 5, { rules: [{ kind: 'vertical' }] })]);
  assert.deepEqual(second.definition.lines.at(-1), { id: 'L2', start: 'P2', end: 'P3' });
  assert.deepEqual(second.constraints, [{ vertical: { line: 'L2' } }]);
  // Turning inference off adds no constraints.
  const plain = commitShape(empty(), 'line', [click(0, 0), click(10, 0, { rules: [{ kind: 'horizontal' }] })], { auto: false });
  assert.deepEqual(plain.constraints, []);
});

test('points inferred onto curves or midpoints are constrained there', () => {
  const base = commitShape(empty(), 'line', [click(0, 0), click(10, 0)]).definition;
  const result = commitShape(base, 'line', [
    click(5, 0, { rules: [{ kind: 'midpoint', line: 'L1' }] }),
    click(5, 8, { rules: [{ kind: 'perpendicular', line: 'L1' }] }),
  ]);
  assert.deepEqual(result.constraints, [
    { midpoint: { point: 'P3', line: 'L1' } },
    { perpendicular: { first: 'L1', second: 'L2' } },
  ]);
});

test('every shape tool draws, and closed shapes can become the profile', () => {
  const cases = {
    rectangle: [click(0, 0), click(4, 3)],
    circle: [click(0, 0), click(2, 0)],
    arc: [click(0, 0), click(2, 0), click(0, 2)],
    ellipse: [click(0, 0), click(4, 0), click(0, 2)],
    slot: [click(0, 0), click(6, 0), click(3, 1)],
    polygon: [click(0, 0), click(3, 0)],
    spline: [click(0, 0), click(2, 3), click(5, 1)],
    point: [click(1, 1)],
  };
  for (const [tool, clicks] of Object.entries(cases)) {
    const result = commitShape(empty(), tool, clicks, { profile: true, sides: 5 });
    assert.ok(result.created.length > 0, tool);
    if (['rectangle', 'circle', 'ellipse', 'slot', 'polygon'].includes(tool)) {
      assert.ok(result.definition.profile.length > 0, `${tool} profile`);
    }
  }
});

test('arcs take the shorter way unless flipped, and start tangent to a line they continue', () => {
  const ccw = commitShape(empty(), 'arc', [click(0, 0), click(2, 0), click(0, 2)]).definition.arcs[0];
  assert.equal(ccw.clockwise, false);
  const cw = commitShape(empty(), 'arc', [click(0, 0), click(2, 0), click(0, -2)]).definition.arcs[0];
  assert.equal(cw.clockwise, true);
  assert.equal(commitShape(empty(), 'arc', [click(0, 0), click(2, 0), click(0, 2)], { flip: true }).definition.arcs[0].clockwise, true);
  // A line to (10, 0), then an arc centred at (10, 3) starting at its end.
  const line = commitShape(empty(), 'line', [click(0, 0), click(10, 0)]).definition;
  const arc = commitShape(line, 'arc', [click(10, 3), click(10, 0, { point: 'P2' }), click(13, 3)]);
  assert.deepEqual(arc.constraints, [{ tangent: { first: 'L1', second: 'A1', point: 'P2' } }]);
});

test('typed values place the next click exactly', () => {
  const anchor = { xy: [1, 1] };
  const line = typedClick('line', anchor, [5, 1], '10');
  assert.deepEqual(line.xy, [11, 1]);
  const angled = typedClick('line', anchor, [5, 1], '2, 90');
  assert.ok(Math.abs(angled.xy[0] - 1) < 1e-12 && Math.abs(angled.xy[1] - 3) < 1e-12);
  assert.deepEqual(typedClick('rectangle', anchor, [-3, 4], '4,2').xy, [-3, 3]);
  assert.deepEqual(typedClick('circle', anchor, [1, 1], '2').xy, [3, 1]);
  for (const [tool, text] of [['line', '-1'], ['line', 'x'], ['rectangle', '3'], ['arc', '2'], ['circle', '1 2']]) {
    assert.equal(typedClick(tool, anchor, [5, 5], text), null, `${tool} ${text}`);
  }
  assert.equal(typedClick('line', null, [5, 5], '3'), null);
});
