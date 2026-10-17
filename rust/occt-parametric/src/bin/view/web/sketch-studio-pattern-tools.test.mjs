import test from 'node:test';
import assert from 'node:assert/strict';
import { onAxisPoints, patternGuide, suggestedSpacing, withPattern } from './sketch-studio-pattern-tools.mjs';

const mm = (value) => ({ literal: { value, dimension: 'length', unit: 'millimeter' } });
const point = (id) => ({ id, x: mm(0), y: mm(0), fixed: false });
const samples = (a, b, n = 8) => Array.from({ length: n }, (_, i) => [a[0] + ((b[0] - a[0]) * i) / (n - 1), a[1] + ((b[1] - a[1]) * i) / (n - 1), 0]);

/** Half of a house outline right of the axis x = 0, a hole, and a free circle. */
function half() {
  const points = { A: [0, 0, 0], B: [10, 0, 0], C: [10, 6, 0], D: [0, 10, 0], Q: [0, -5, 0], R: [0, 15, 0], H: [5, 3, 0], I: [6, 3, 0], K: [3, 8, 0] };
  const lines = [['AB', 'A', 'B'], ['BC', 'B', 'C'], ['CD', 'C', 'D'], ['axis', 'Q', 'R']];
  const definition = {
    id: 's',
    points: Object.keys(points).map(point),
    lines: lines.map(([id, start, end]) => ({ id, start, end })),
    circles: [{ id: 'hole', center: 'H', rim: 'I' }],
    profile: ['AB', 'BC', 'CD'],
    holes: [['hole']],
    constraints: [],
  };
  const circle = Array.from({ length: 33 }, (_, i) => [5 + Math.cos((i / 32) * 2 * Math.PI), 3 + Math.sin((i / 32) * 2 * Math.PI), 0]);
  const scene = {
    points,
    entities: [
      ...lines.map(([id, a, b]) => ({ id, point_ids: [a, b], points: samples(points[a], points[b]) })),
      { id: 'hole', point_ids: ['H', 'I'], points: circle },
    ],
  };
  return { definition, scene };
}

test('a mirror shares the points on its axis and closes a half outline', () => {
  const { definition, scene } = half();
  assert.deepEqual(onAxisPoints(definition, scene, ['AB', 'BC', 'CD'], 'axis').sort(), ['A', 'D']);
  const result = withPattern(definition, scene, 'mirror', ['AB', 'BC', 'CD', 'hole'], 'axis');
  assert.equal(result.id, 'Mirror1');
  assert.deepEqual(result.definition.patterns, [{ mirror: { id: 'Mirror1', entities: ['AB', 'BC', 'CD', 'hole'], axis: 'axis', on_axis: ['A', 'D'] } }]);
  assert.deepEqual(result.definition.profile, ['AB', 'BC', 'CD', 'Mirror1:1:CD', 'Mirror1:1:BC', 'Mirror1:1:AB']);
  assert.deepEqual(result.definition.holes, [['hole'], ['Mirror1:1:hole']]);
  assert.match(result.summary, /1 copied hole and the outline closed/);
  assert.equal(definition.patterns, undefined, 'the input is unchanged');
  // Part of the outline mirrors without touching the profile.
  assert.deepEqual(withPattern(definition, scene, 'mirror', ['BC'], 'axis').definition.profile, definition.profile);
  // A second mirror gets a fresh name; the axis cannot mirror itself.
  assert.equal(withPattern(result.definition, scene, 'mirror', ['BC'], 'axis').id, 'Mirror2');
  assert.throws(() => withPattern(definition, scene, 'mirror', ['axis'], 'axis'), /own reference line/);
});

test('linear and circular patterns copy holes for every instance and check their values', () => {
  const { definition, scene } = half();
  const row = withPattern(definition, scene, 'linear', ['hole'], 'AB', { count: 3, spacing: mm(2.5) });
  assert.deepEqual(row.definition.patterns, [{ linear: { id: 'Linear1', entities: ['hole'], direction: 'AB', spacing: mm(2.5), count: 3 } }]);
  assert.deepEqual(row.definition.holes, [['hole'], ['Linear1:1:hole'], ['Linear1:2:hole']]);
  assert.match(row.summary, /3× linear pattern of 1 curve, 2 copied holes/);
  const ring = withPattern(definition, scene, 'circular', ['hole'], 'K', { count: 4 });
  assert.deepEqual(ring.definition.patterns[0], { circular: { id: 'Circular1', entities: ['hole'], center: 'K', count: 4 } });
  assert.equal(ring.definition.holes.length, 4);
  const stepped = withPattern(definition, scene, 'circular', ['BC'], 'K', { count: 2, angle: { parameter: 'turn' } });
  assert.deepEqual(stepped.definition.patterns[0].circular.angle, { parameter: 'turn' });
  assert.equal(stepped.definition.holes.length, 1, 'outline curves are not holes');
  assert.throws(() => withPattern(definition, scene, 'linear', ['hole'], 'AB', { count: 1, spacing: mm(2) }), /count/);
  assert.throws(() => withPattern(definition, scene, 'circular', ['hole'], 'AB', { count: 3 }), /center/);
  assert.throws(() => withPattern(definition, scene, 'linear', ['nope'], 'AB', { count: 2, spacing: mm(1) }), /source curves/);
});

test('the guide reflects, steps and turns the sampled sources', () => {
  const { scene } = half();
  const near = (a, b) => a.every((v, i) => Math.abs(v - b[i]) < 1e-9);
  const [mirrored] = patternGuide(scene, 'mirror', ['BC'], 'axis');
  assert.ok(near(mirrored[0], [-10, 0]) && near(mirrored.at(-1), [-10, 6]));
  const steps = patternGuide(scene, 'linear', ['BC'], 'AB', { count: 3, spacing: 4 });
  assert.equal(steps.length, 2);
  assert.ok(near(steps[1][0], [18, 0]));
  const turned = patternGuide(scene, 'circular', ['BC'], 'A', { count: 4 });
  assert.equal(turned.length, 3);
  assert.ok(near(turned[0][0], [0, 10]), JSON.stringify(turned[0][0]));
  assert.ok(near(patternGuide(scene, 'circular', ['BC'], 'A', { count: 2, angle: Math.PI })[0][0], [-10, 0]));
  assert.equal(suggestedSpacing(scene, ['hole'], 'AB'), 3);
  assert.equal(suggestedSpacing(scene, ['AB'], 'AB'), 12.5);
});

test('patterning 1,000 curves stays fast', () => {
  const n = 1000;
  const points = { O: [0, 0, 0], E: [1, 0, 0] };
  const definition = { id: 's', points: [point('O'), point('E')], lines: [{ id: 'dir', start: 'O', end: 'E' }], circles: [], holes: [], profile: [], constraints: [] };
  const entities = [{ id: 'dir', point_ids: ['O', 'E'], points: [[0, 0, 0], [1, 0, 0]] }];
  for (let i = 0; i < n; i += 1) {
    points[`c${i}`] = [i * 3, 5, 0];
    points[`r${i}`] = [i * 3 + 1, 5, 0];
    definition.points.push(point(`c${i}`), point(`r${i}`));
    definition.circles.push({ id: `h${i}`, center: `c${i}`, rim: `r${i}` });
    definition.holes.push([`h${i}`]);
    entities.push({ id: `h${i}`, point_ids: [`c${i}`, `r${i}`], points: samples(points[`c${i}`], points[`r${i}`], 32) });
  }
  const scene = { points, entities };
  const sources = definition.circles.map((c) => c.id);
  const started = performance.now();
  const result = withPattern(definition, scene, 'linear', sources, 'dir', { count: 3, spacing: mm(1) });
  const guide = patternGuide(scene, 'linear', sources, 'dir', { count: 3, spacing: 1 });
  const elapsed = performance.now() - started;
  assert.equal(result.definition.holes.length, 3 * n);
  assert.equal(guide.length, 2 * n);
  assert.ok(elapsed < 500, `${elapsed} ms`);
});
