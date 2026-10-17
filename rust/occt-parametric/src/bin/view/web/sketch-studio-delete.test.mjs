import test from 'node:test';
import assert from 'node:assert/strict';
import { continueOutline, deletion, describeRemoval, openEnds } from './sketch-studio-delete.mjs';

const ids = (records) => (records ?? []).map((r) => r.id);
const mm = (value) => ({ literal: { value, dimension: 'length', unit: 'millimeter' } });
const point = (id) => ({ id, x: mm(0), y: mm(0), fixed: false });

/** Two lines sharing B, a circle on C, constraints, a profile and a hole. */
function corner() {
  return {
    id: 'sketch',
    points: ['A', 'B', 'C', 'R', 'F'].map(point),
    lines: [{ id: 'L1', start: 'A', end: 'B' }, { id: 'L2', start: 'B', end: 'C' }],
    circles: [{ id: 'hole', center: 'C', rim: 'R' }],
    constraints: [
      { horizontal: { line: 'L1' } },
      { distance: { first: 'A', second: 'F', value: { parameter: 'B' } } },
      { coincident: { first: 'C', second: 'F' } },
    ],
    profile: ['L1', 'L2'],
    holes: [['hole']],
  };
}

test('a line takes its unshared endpoint and the constraints on it, and leaves its loop', () => {
  const before = corner();
  const copy = structuredClone(before);
  const result = deletion(before, ['L1']);
  assert.deepEqual(before, copy, 'the input is unchanged');
  assert.deepEqual(ids(result.definition.lines), ['L2']);
  assert.deepEqual(ids(result.definition.points), ['B', 'C', 'R', 'F'], 'B is shared with L2; F is a free point');
  assert.deepEqual(result.definition.constraints, [{ coincident: { first: 'C', second: 'F' } }],
    'the distance from A goes; its parameter named B is not a reference');
  assert.deepEqual(result.definition.profile, ['L2']);
  assert.deepEqual(result.removed, { points: ['A'], lines: ['L1'], constraints: 2 });
  assert.equal(result.summary, '1 point, 1 line and 2 constraints');
});

test('a point takes the curves built on it, their other unused points, and an emptied hole', () => {
  const result = deletion(corner(), ['C']);
  assert.deepEqual(ids(result.definition.lines), ['L1']);
  assert.deepEqual(ids(result.definition.circles), []);
  assert.deepEqual(ids(result.definition.points), ['A', 'B', 'F'], 'R only served the circle; B still ends L1');
  assert.deepEqual(result.definition.holes, []);
  assert.deepEqual(result.definition.profile, ['L1']);
  assert.equal(result.definition.constraints.length, 2, 'only the coincidence with C goes');
});

test('a generated name stands for its shape, and patterns lose members or fall', () => {
  const definition = {
    id: 'sketch',
    points: ['A', 'B', 'P', 'Q', 'M'].map(point),
    lines: [{ id: 'axis', start: 'P', end: 'Q' }, { id: 'spoke', start: 'M', end: 'P' }],
    shapes: [{ rectangle: { id: 'box', first: 'A', opposite: 'B' } }],
    patterns: [
      { mirror: { id: 'copy', entities: ['box:side0'], axis: 'axis' } },
      { linear: { id: 'row', entities: ['spoke', 'box:side1'], direction: 'axis', spacing: mm(5), count: 2 } },
    ],
    constraints: [
      { coincident: { first: 'box:corner1', second: 'M' } },
      { horizontal: { line: 'copy:1:box:side0' } },
      { vertical: { line: 'row:1:spoke' } },
    ],
    profile: ['box:side0', 'box:side1', 'box:side2', 'box:side3'],
  };
  const result = deletion(definition, ['box:side2']);
  assert.deepEqual(result.owners, [{ id: 'box:side2', owner: 'box', kind: 'shapes' }]);
  assert.deepEqual(result.definition.shapes, []);
  assert.deepEqual(ids(result.definition.points), ['P', 'Q', 'M'], 'the rectangle corners go with it');
  assert.deepEqual(result.definition.patterns.map((p) => Object.values(p)[0].id), ['row'], 'the mirror copied only the box');
  assert.deepEqual(result.definition.patterns[0].linear.entities, ['spoke']);
  assert.deepEqual(result.definition.constraints, [{ vertical: { line: 'row:1:spoke' } }]);
  assert.deepEqual(result.definition.profile, []);
  assert.equal(result.summary, '2 points, 1 shape, 1 pattern and 2 constraints');

  const axis = deletion(definition, ['Q']);
  assert.deepEqual(axis.definition.patterns, [], 'both patterns read the axis line');
  assert.deepEqual(ids(axis.definition.lines), ['spoke']);
  assert.deepEqual(axis.definition.constraints, [{ coincident: { first: 'box:corner1', second: 'M' } }]);
});

test('edits naming deleted curves go, with the split pieces they made and regions they emptied', () => {
  const definition = {
    id: 'sketch',
    points: ['A', 'B', 'C', 'D', 'H'].map(point),
    lines: [{ id: 'cut', start: 'A', end: 'B' }, { id: 'keep', start: 'C', end: 'D' }],
    curve_operations: [{ split: { entity: 'cut', at: { crossing: { curve: 'keep', near: 'H' } }, before: 'piece1', after: 'piece2' } }],
    region_edits: [{ seed: 'H', entities: ['keep'] }],
    regions: [{ profile: ['piece1'], holes: [] }, { profile: ['keep'], holes: [['piece2']] }],
    measurements: [{ id: 'm', measure: { curve_length: { curve: 'piece1' } } }],
    constraints: [{ point_on_curve: { point: 'H', curve: 'piece2' } }],
    profile: [],
  };
  const result = deletion(definition, ['cut']);
  assert.deepEqual(result.definition.curve_operations, []);
  assert.equal(result.definition.region_edits.length, 1, 'the region edit names only kept geometry');
  assert.deepEqual(result.definition.regions, [{ profile: ['keep'], holes: [] }]);
  assert.deepEqual(result.definition.measurements, []);
  assert.deepEqual(result.definition.constraints, []);
  assert.deepEqual(ids(result.definition.points), ['C', 'D', 'H']);
  assert.equal(result.summary, '2 points, 1 line, 1 constraint, 1 reference dimension and 1 curve or region edit');
});

test('unknown names are refused and an empty selection deletes nothing', () => {
  assert.throws(() => deletion(corner(), ['nope']), /not geometry of this sketch/);
  assert.throws(() => deletion(corner(), []), /Select geometry/);
  assert.equal(describeRemoval({}), 'nothing');
});

test('deleting from a 10,000-line chain stays linear', () => {
  const n = 10000;
  const definition = { id: 'chain', points: [], lines: [], constraints: [], profile: [] };
  for (let i = 0; i <= n; i += 1) definition.points.push(point(`P${i}`));
  for (let i = 0; i < n; i += 1) {
    definition.lines.push({ id: `L${i}`, start: `P${i}`, end: `P${i + 1}` });
    definition.constraints.push({ horizontal: { line: `L${i}` } });
    definition.profile.push(`L${i}`);
  }
  let started = performance.now();
  const one = deletion(definition, ['L0']);
  assert.equal(one.definition.lines.length, n - 1);
  assert.deepEqual(one.removed.points, ['P0']);
  const all = deletion(definition, definition.lines.map((l) => l.id));
  assert.equal(all.definition.points.length, 0);
  assert.equal(all.definition.constraints.length, 0);
  assert.ok(performance.now() - started < 2000, `${performance.now() - started} ms`);
  started = performance.now();
  const points = deletion(definition, definition.points.map((p) => p.id));
  assert.equal(points.definition.lines.length, 0);
  assert.ok(performance.now() - started < 2000);
});

test('a loop that loses one edge opens at the gap and closes again when the edge is redrawn', () => {
  const square = {
    id: 'square',
    points: ['A', 'B', 'C', 'D'].map(point),
    lines: [{ id: 'AB', start: 'A', end: 'B' }, { id: 'BC', start: 'B', end: 'C' }, { id: 'CD', start: 'C', end: 'D' }, { id: 'DA', start: 'D', end: 'A' }],
    profile: ['AB', 'BC', 'CD', 'DA'],
  };
  const ends = (d) => new Map(d.lines.map((l) => [l.id, [l.start, l.end]]));
  assert.equal(openEnds(square.profile, ends(square)), null, 'closed');
  const cut = deletion(square, ['BC']).definition;
  assert.deepEqual(cut.profile, ['CD', 'DA', 'AB']);
  assert.deepEqual(openEnds(cut.profile, ends(cut)), ['C', 'B']);
  // An arc from B to C (its own center) closes it; a stray line does not join.
  const drawn = structuredClone(cut);
  drawn.points.push(point('M'), point('Z'));
  drawn.arcs = [{ id: 'arc', center: 'M', start: 'B', end: 'C' }];
  drawn.lines.push({ id: 'stray', start: 'M', end: 'Z' });
  const closed = continueOutline(cut, drawn);
  assert.deepEqual(closed.profile, ['CD', 'DA', 'AB', 'arc']);
  assert.deepEqual(drawn.profile, ['CD', 'DA', 'AB'], 'the input is unchanged');
  // Two lines drawn the other way round close it too, turned to run with it.
  const chain = structuredClone(cut);
  chain.points.push(point('E'));
  chain.lines.push({ id: 'CE', start: 'C', end: 'E' }, { id: 'EB', start: 'E', end: 'B' });
  const turned = continueOutline(cut, chain);
  assert.deepEqual(turned.profile, ['CE', 'CD', 'DA', 'AB', 'EB']);
  assert.deepEqual(turned.lines.slice(-2), [{ id: 'CE', start: 'E', end: 'C' }, { id: 'EB', start: 'B', end: 'E' }]);
  assert.equal(openEnds(turned.profile, new Map(turned.lines.map((l) => [l.id, [l.start, l.end]]))), null);
  // A reversed arc swaps its ends and its sense.
  const back = structuredClone(cut);
  back.points.push(point('M'));
  back.arcs = [{ id: 'arc', center: 'M', start: 'C', end: 'B', clockwise: false }];
  assert.deepEqual(continueOutline(cut, back).arcs, [{ id: 'arc', center: 'M', start: 'B', end: 'C', clockwise: true }]);
  // Closed or automatic profiles are left alone.
  const extra = structuredClone(square);
  extra.lines.push({ id: 'AC', start: 'A', end: 'C' });
  assert.deepEqual(continueOutline(square, extra).profile, square.profile);
  // Two separate gaps leave the kept order.
  assert.deepEqual(deletion(square, ['AB', 'CD']).definition.profile, ['BC', 'DA']);
});
