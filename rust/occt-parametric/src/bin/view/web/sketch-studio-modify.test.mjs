import test from 'node:test';
import assert from 'node:assert/strict';
import { cornerValues, extension, nearerEnd, profileCorners, withCornerEdit, withExtension } from './sketch-studio-modify.mjs';

const mm = (value) => ({ literal: { value, dimension: 'length', unit: 'millimeter' } });
const samples = (a, b, n = 32) => Array.from({ length: n }, (_, i) => [a[0] + ((b[0] - a[0]) * i) / (n - 1), a[1] + ((b[1] - a[1]) * i) / (n - 1), 0]);

/** A 20 × 10 rectangle with a spur inside and a free stop line. */
function fixture() {
  const points = { A: [0, 0, 0], B: [20, 0, 0], C: [20, 10, 0], D: [0, 10, 0], S: [12, 5, 0], T: [6, 5, 0], U: [3, -4, 0], V: [3, 14, 0] };
  const lines = [['AB', 'A', 'B'], ['BC', 'B', 'C'], ['CD', 'C', 'D'], ['DA', 'D', 'A'], ['spur', 'S', 'T'], ['stop', 'U', 'V']];
  const definition = { id: 's', lines: lines.map(([id, start, end]) => ({ id, start, end })), profile: ['AB', 'BC', 'CD', 'DA'] };
  const scene = { points, entities: lines.map(([id, a, b]) => ({ id, point_ids: [a, b], points: samples(points[a], points[b]) })) };
  return { definition, scene };
}

test('corners are where two outline curves meet, with any edit already there', () => {
  const { definition, scene } = fixture();
  assert.deepEqual([...profileCorners(definition, scene).keys()].sort(), ['A', 'B', 'C', 'D']);
  const filleted = withCornerEdit(definition, 'fillet', 'B', [mm(2)]);
  assert.equal(profileCorners(filleted, scene).get('B'), 'fillet');
  // An empty profile means every line: the spur's and stop's ends are not corners.
  assert.deepEqual([...profileCorners({ ...definition, profile: [] }, scene).keys()].sort(), ['A', 'B', 'C', 'D']);
});

test('corner edits record native fillets and chamfers and replace an earlier one', () => {
  const { definition } = fixture();
  const fillet = withCornerEdit(definition, 'fillet', 'A', cornerValues('3', 1));
  assert.deepEqual(fillet.profile_operations, [{ fillet: { point: 'A', radius: mm(3) } }]);
  assert.equal(definition.profile_operations, undefined, 'the input is unchanged');
  const chamfer = withCornerEdit(fillet, 'chamfer', 'A', cornerValues('2, gap', 2));
  assert.deepEqual(chamfer.profile_operations, [{ chamfer: { point: 'A', incoming: mm(2), outgoing: { parameter: 'gap' } } }]);
  assert.deepEqual(cornerValues('1.5', 2), [mm(1.5), mm(1.5)]);
  assert.equal(cornerValues('0', 1), null);
  assert.equal(cornerValues('-2', 1), null);
  assert.equal(cornerValues('1, 2', 1), null);
  assert.equal(cornerValues('', 1), null);
});

test('a line extends straight to the first curve it meets, not one touching its end', () => {
  const { definition, scene } = fixture();
  const spur = scene.entities.find((e) => e.id === 'spur');
  assert.equal(nearerEnd(spur, [7, 5]), 'end');
  const left = extension(definition, scene, 'spur', 'end');
  assert.equal(left.target, 'stop');
  assert.deepEqual(left.at, [3, 5]);
  assert.equal(left.length, 3);
  const right = extension(definition, scene, 'spur', 'start');
  assert.equal(right.target, 'BC');
  assert.equal(right.length, 8);
  // AB's end touches BC; it runs on past to nothing.
  assert.equal(extension(definition, scene, 'AB', 'end'), null);
  const next = withExtension(definition, 'spur', 'end', left.target, left.length);
  assert.deepEqual(next.curve_operations, [{ extend_to: { entity: 'spur', end: 'end', target: 'stop', limit: mm(3.8), near: null } }]);
  assert.equal(withExtension(next, 'spur', 'end', 'DA', 6).curve_operations.length, 1, 'one extension per end');
});

test('an arc extends round its circle', () => {
  const center = [0, 0, 0];
  const arcPoints = Array.from({ length: 33 }, (_, i) => {
    const angle = (Math.PI / 2) * (i / 32);
    return [5 * Math.cos(angle), 5 * Math.sin(angle), 0];
  });
  const scene = {
    points: { O: center, P: [5, 0, 0], Q: [0, 5, 0], L0: [-8, -1, 0], L1: [8, -1, 0] },
    entities: [
      { id: 'arc', point_ids: ['P', 'Q'], points: arcPoints },
      { id: 'floor', point_ids: ['L0', 'L1'], points: samples([-8, -1], [8, -1]) },
    ],
  };
  const definition = { arcs: [{ id: 'arc', center: 'O', start: 'P', end: 'Q', clockwise: false }], lines: [] };
  const hit = extension(definition, scene, 'arc', 'end');
  assert.equal(hit.target, 'floor');
  // Counterclockwise from (0, 5), the circle meets y = -1 at x = -√24.
  assert.ok(Math.abs(hit.at[0] + Math.sqrt(24)) < 0.05 && Math.abs(hit.at[1] + 1) < 1e-9, `${hit.at}`);
  assert.ok(hit.length > 5 * Math.PI / 2 && hit.length < 5 * Math.PI, `${hit.length}`);
  const back = extension(definition, scene, 'arc', 'start');
  assert.ok(back.at[0] > 0, 'clockwise from (5, 0) it meets the floor on the right');
});

test('extension stays fast on 10,000 curves', () => {
  const entities = [{ id: 'probe', point_ids: ['a', 'b'], points: samples([0, 0], [1, 0], 2) }];
  for (let i = 0; i < 10000; i += 1) entities.push({ id: `L${i}`, point_ids: [`p${i}`, `q${i}`], points: samples([2 + i, -1], [2 + i, 1], 2) });
  const scene = { points: { a: [0, 0, 0], b: [1, 0, 0] }, entities };
  const started = performance.now();
  const hit = extension({ lines: [] }, scene, 'probe', 'end');
  assert.equal(hit.target, 'L0');
  assert.ok(performance.now() - started < 200, `${performance.now() - started} ms`);
});
