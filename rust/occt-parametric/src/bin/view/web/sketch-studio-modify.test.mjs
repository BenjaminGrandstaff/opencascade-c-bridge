import test from 'node:test';
import assert from 'node:assert/strict';
import {
  chainPolylines, cornerValues, extension, nearerEnd, offsetBase, offsetDistance, offsetPolyline, profileCorners, profileOffset,
  withCornerEdit, withExtension, withOffset,
} from './sketch-studio-modify.mjs';

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

test('arc ends count as corners though scenes list arc centers too', () => {
  const { definition, scene } = fixture();
  definition.arcs = [{ id: 'bump', center: 'S', start: 'A', end: 'B', clockwise: true }];
  definition.profile = ['bump', 'BC', 'CD', 'DA'];
  scene.entities.push({ id: 'bump', point_ids: ['S', 'A', 'B'], points: samples([0, 0], [20, 0]) });
  assert.deepEqual([...profileCorners(definition, scene).keys()].sort(), ['A', 'B', 'C', 'D']);
});

test('offsets read signed distances and parameters, replace the last offset and follow corner edits', () => {
  assert.deepEqual(offsetDistance('-1.5'), mm(-1.5));
  assert.deepEqual(offsetDistance('wall'), { parameter: 'wall' });
  assert.deepEqual(offsetDistance('- wall'), { negate: { parameter: 'wall' } });
  assert.equal(offsetDistance('0'), 0);
  assert.equal(offsetDistance(''), null);
  assert.equal(offsetDistance('1,2'), null);
  const { definition } = fixture();
  const grown = withOffset(definition, mm(2), 'intersection');
  assert.deepEqual(grown.profile_operations, [{ offset: { distance: mm(2), join: 'intersection' } }]);
  assert.equal(definition.profile_operations, undefined, 'the input is unchanged');
  assert.deepEqual(profileOffset(grown), { distance: mm(2), join: 'intersection', text: '2' });
  assert.equal(profileOffset(withOffset(definition, { negate: { parameter: 'wall' } })).text, '-wall');
  // A fillet added afterwards goes before the offset, as the model requires.
  const filleted = withCornerEdit(grown, 'fillet', 'B', [mm(1)]);
  assert.deepEqual(filleted.profile_operations.map((op) => Object.keys(op)[0]), ['fillet', 'offset']);
  const changed = withOffset(filleted, mm(-1));
  assert.deepEqual(changed.profile_operations, [{ fillet: { point: 'B', radius: mm(1) } }, { offset: { distance: mm(-1), join: 'arc' } }]);
  assert.deepEqual(withOffset(changed, 0).profile_operations, [{ fillet: { point: 'B', radius: mm(1) } }]);
});

test('the offset guide chains the outline and grows it whichever way it runs', () => {
  const { definition, scene } = fixture();
  const base = offsetBase(definition, scene);
  assert.equal(base.closed, true);
  assert.equal(base.points.length, 4 * 31);
  const box = (points) => points.reduce((b, p) => ({
    min: [Math.min(b.min[0], p[0]), Math.min(b.min[1], p[1])], max: [Math.max(b.max[0], p[0]), Math.max(b.max[1], p[1])],
  }), { min: [Infinity, Infinity], max: [-Infinity, -Infinity] });
  const near = (a, b) => a.every((v, i) => Math.abs(v - b[i]) < 1e-9);
  const sharp = box(offsetPolyline(base, 2, 'intersection'));
  assert.ok(near(sharp.min, [-2, -2]) && near(sharp.max, [22, 12]), JSON.stringify(sharp));
  const reversed = { points: [...base.points].reverse(), closed: true };
  const shrunk = box(offsetPolyline(reversed, -1, 'intersection'));
  assert.ok(near(shrunk.min, [1, 1]) && near(shrunk.max, [19, 9]), JSON.stringify(shrunk));
  // Round corners stay 2 from the corner.
  const round = offsetPolyline(base, 2, 'arc');
  const corner = Math.min(...round.map((p) => Math.hypot(p[0] - 20, p[1])));
  assert.ok(Math.abs(corner - 2) < 1e-9 && round.every((p) => Math.hypot(p[0] - 20, p[1]) >= 2 - 1e-9 || p[0] < 18 || p[1] > 2));
  // Open outlines go to the right of travel; curves drawn either way chain.
  const open = chainPolylines([[[0, 0], [10, 0]], [[10, 5], [10, 0]]]);
  assert.deepEqual(open, { points: [[0, 0], [10, 0], [10, 5]], closed: false });
  assert.deepEqual(offsetPolyline(open, 1, 'intersection'), [[0, -1], [11, -1], [11, 5]]);
  // An edited outline without an offset is the base; with one, the sources are.
  const edited = { ...scene, edited_profile: [[[0, 0, 0], [5, 0, 0]], [[5, 0, 0], [0, 5, 0]], [[0, 5, 0], [0, 0, 0]]] };
  assert.equal(offsetBase(definition, edited).points.length, 3);
  assert.equal(offsetBase(withOffset(definition, mm(1)), edited).points.length, 4 * 31);
});

test('the offset guide stays linear on a 100,000-point outline', () => {
  const n = 100000;
  const points = Array.from({ length: n }, (_, i) => [50 * Math.cos((2 * Math.PI * i) / n), 50 * Math.sin((2 * Math.PI * i) / n)]);
  const started = performance.now();
  const chained = chainPolylines([points.slice(0, n / 2 + 1), points.slice(n / 2).concat([points[0]])]);
  const out = offsetPolyline(chained, 5, 'arc');
  assert.ok(Math.abs(Math.hypot(...out[123]) - 55) < 1e-6);
  assert.ok(performance.now() - started < 500, `${performance.now() - started} ms`);
});
