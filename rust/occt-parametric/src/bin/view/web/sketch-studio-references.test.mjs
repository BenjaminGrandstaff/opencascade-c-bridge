import test from 'node:test';
import assert from 'node:assert/strict';
import {captureGeneratedPoints, bindGeneratedPoints, generatedPointIds} from './generated-points.mjs';
import {geometryIds, makeConstraint} from './constraint-controls.mjs';
import {geometryReferences} from './sketch-studio-geometry.mjs';
import {dimensionFor, suggestions} from './sketch-studio-select.mjs';
import {infer, tangentLineAt, pointPositions} from './sketch-studio-infer.mjs';
import {commitShape} from './sketch-studio-tools.mjs';
import {fixture} from './sketch-studio-test-fixture.mjs';

const mm = value => ({literal: {value, dimension: 'length', unit: 'millimeter'}});
function sample() {
  const d = {points: [{id: 'A', x: mm(0), y: mm(0)}, {id: 'B', x: mm(10), y: mm(5)}],
    lines: [], shapes: [{rectangle: {id: 'box', first: 'A', opposite: 'B'}}],
    patterns: [{linear: {id: 'row', entities: ['box:side0'], direction: 'box:side1', count: 2}}]};
  const scene = {points: {A: [0,0,0], B: [10,5,0], 'box:corner1': [10,0,0], 'box:corner3': [0,5,0],
    'row:1:box:corner1': [30,0,0], O: [20,10,0], S: [23,10,0], E: [20,13,0], unknown: [50,50,0]},
    entities: [
      {id: 'box:side0', point_ids: ['A','box:corner1'], points: [[0,0,0],[10,0,0]]},
      {id: 'row:1:box:side0', point_ids: ['box:corner1','row:1:box:corner1'], points: [[10,0,0],[30,0,0]]},
      {id: 'slot:cap', point_ids: ['O','S','E'], points: [[23,10,0],[22.12,12.12,0],[20,13,0]]},
      {id: 'unknown-curve', point_ids: ['unknown','B'], points: [[40,40,0],[60,60,0]]},
    ]};
  const ids = ['box:corner1','box:corner3','row:1:box:corner1','O','S','E'];
  const curves = {lines: ['box:side0','row:1:box:side0'], arcs: ['slot:cap']};
  const bind = () => bindGeneratedPoints(d, captureGeneratedPoints(d, ids, curves));
  return {d, scene, ids, curves, bind};
}

test('generated line, pattern copy and arc dimensions use verified native point roles', () => {
  const {d, scene, bind} = sample(); bind();
  assert.equal(dimensionFor(d, scene, ['box:side0']).value, 10);
  assert.equal(dimensionFor(d, scene, ['row:1:box:side0']).value, 20);
  assert.equal(dimensionFor(d, scene, ['slot:cap']).value, 3);
  assert.equal(dimensionFor(d, scene, ['slot:cap']).type, 'radius');
  assert.equal(dimensionFor(d, scene, ['box:corner1','row:1:box:corner1']).value, 20);
  assert.equal(dimensionFor(d, scene, ['unknown-curve']), null);
  assert.equal(dimensionFor(d, scene, ['unknown','A']), null);
});

test('suggestion membership refreshes when Preview binds a catalogue to the same definition', () => {
  const {d, scene, bind} = sample();
  assert.deepEqual(suggestions(d, ['box:side0'], scene), []); bind();
  assert(suggestions(d, ['box:side0'], scene).some(s => s.type === 'horizontal'));
  const tangent = suggestions(d, ['box:side0','row:1:box:side0'], scene).find(s => s.type === 'tangent');
  assert.deepEqual(tangent.fields, {first: 'box:side0', second: 'row:1:box:side0', point: 'box:corner1'});
  d.patterns[0].linear.count = 3;
  assert.deepEqual(suggestions(d, ['box:side0'], scene), []);
  assert.equal(dimensionFor(d, scene, ['box:side0']), null);
});

test('generated lines supply midpoint, hover and endpoint tangent inference', () => {
  const {d, scene, bind} = sample(); bind();
  const midpoint = infer({xy: [5,.01], tool: 'point', definition: d, scene, tolerance: .1});
  assert.equal(midpoint.hovered.id, 'box:side0');
  assert(midpoint.rules.some(r => r.kind === 'midpoint' && r.line === 'box:side0'));
  assert.equal(tangentLineAt(d, scene, 'box:corner1', [10,3]), 'box:side0');
  const end = infer({xy: [30.01,.01], tool: 'line', definition: d, scene, tolerance: .1});
  assert.equal(end.point, 'row:1:box:corner1');
});

test('generated arcs supply equal-radius hover and typed radius constraints', () => {
  const {d, scene, bind} = sample(); bind();
  const hit = infer({xy: [23,10], tool: 'circle', definition: d, scene, tolerance: .1});
  assert.deepEqual(hit.hovered, {kind: 'round', id: 'slot:cap', radius: 3});
  assert.doesNotThrow(() => makeConstraint(d, 'radius', {curve: 'slot:cap'}, mm(3)));
  assert.throws(() => makeConstraint(d, 'radius', {curve: 'box:side0'}, mm(3)), /known/);
});

test('unverified scene points, glyphs and edited curve samples never create native references', () => {
  const {d, scene} = sample();
  assert.equal(pointPositions(d, scene.points).has('box:corner1'), false);
  const point = infer({xy: [50,50], tool: 'point', definition: d, scene, tolerance: .1});
  assert.equal(point.point, null); assert.deepEqual(point.rules, []);
  const side = infer({xy: [5,.01], tool: 'point', definition: d, scene, tolerance: .1});
  assert.equal(side.hovered, null); assert.deepEqual(side.rules, []);
});

test('drawing from a generated corner saves its reference and keeps the catalogue for the next edit', () => {
  const {d, scene, bind} = sample(); bind();
  const snap = infer({xy: [30,0], tool: 'line', definition: d, scene, tolerance: .1});
  const next = commitShape(d, 'line', [snap, {xy: [40,3], point: null, rules: []}], {auto: false}).definition;
  assert.equal(next.lines.at(-1).start, 'row:1:box:corner1');
  assert(geometryIds(next, 'lines').includes('box:side0'));
  assert(!JSON.stringify(next).includes('generated_points'));
  assert(!JSON.stringify(next).includes('generated_curves'));
});

test('draft creation, constraint clones and Undo/Redo retain generated references', t => {
  const f = fixture(t), {d, ids, curves} = sample(); Object.assign(f.baseline, d);
  bindGeneratedPoints(f.baseline, captureGeneratedPoints(f.baseline, ids, curves));
  f.studio.ensureDraft(); assert(geometryIds(f.studio.draft.definition, 'lines').includes('box:side0'));
  const next = structuredClone(f.studio.draft.definition);
  next.constraints = [makeConstraint(f.studio.draft.definition, 'horizontal', {line: 'box:side0'})];
  f.studio.changeDraft(next); assert(geometryIds(next, 'lines').includes('box:side0'));
  f.drawing.undo(); assert(geometryIds(f.studio.draft.definition, 'lines').includes('box:side0'));
  f.drawing.redo(); assert(geometryIds(f.studio.draft.definition, 'lines').includes('box:side0'));
  const edited = structuredClone(next); edited.patterns[0].linear.count = 3;
  f.studio.changeDraft(edited); assert.deepEqual(generatedPointIds(edited), []);
  f.drawing.discard(true);
});

test('only a current, successful Preview can refresh the draft generated catalogue', async t => {
  let resolve;
  const f = fixture(t, {api: () => new Promise(r => {resolve = r;})});
  const {d, ids, curves} = sample(); Object.assign(f.baseline, d);
  f.studio.ensureDraft(); f.studio.changeDraft(structuredClone(f.studio.draft.definition));
  const target = f.studio.draft.definition;
  assert.deepEqual(suggestions(target, ['box:side0']), []);
  t.mock.timers.tick(150); await f.flush(); resolve({can_apply: true, generated_points: ids, generated_curves: curves}); await f.flush();
  assert(suggestions(target, ['box:side0']).some(s => s.type === 'horizontal'));
  f.studio.changeDraft({...target, constraints: []}); t.mock.timers.tick(150); await f.flush(); f.drawing.undo();
  resolve({can_apply: true, generated_points: ['stale'], generated_curves: {lines: ['stale-line']}}); await f.flush();
  assert(!geometryIds(f.studio.draft.definition, 'lines').includes('stale-line'));
  f.drawing.discard(true);
});

test('missing solved defining points omit generated roles rather than guessing sample endpoints', () => {
  const {d, scene, bind} = sample(); bind();
  scene.entities[0].point_ids = [];
  assert.equal(geometryReferences(d, scene).lines.some(l => l.id === 'box:side0'), false);
  assert.equal(dimensionFor(d, scene, ['box:side0']), null);
});

test('projected lines and circles use native roles and explicit splines use their actual endpoints', () => {
  const d = {points: [], lines: [], projections: [{id: 'edge', kind: 'line'}, {id: 'round', kind: 'circle'}]};
  const scene = {points: {'edge:start': [0,0,0], 'edge:end': [4,0,0], 'round:center': [0,5,0], 'round:rim': [2,5,0]},
    entities: [{id: 'edge', point_ids: ['edge:start','edge:end']}, {id: 'round', point_ids: ['round:center','round:rim']},
      {id: 'spline', point_ids: ['pole0','pole1','edge:end'], bspline: {endpoints: {start: 'edge:end', end: 'tip'}}}]};
  bindGeneratedPoints(d, captureGeneratedPoints(d, ['tip'], {splines: ['spline']}));
  assert.equal(dimensionFor(d, scene, ['edge']).value, 4);
  assert.equal(dimensionFor(d, scene, ['round']).value, 4);
  assert.equal(dimensionFor(d, scene, ['round']).type, 'diameter');
  const tangent = suggestions(d, ['edge','spline'], scene).find(s => s.type === 'tangent');
  assert.equal(tangent.fields.point, 'edge:end');
});

test('rejected Preview metadata cannot make new generated names selectable', async t => {
  const f = fixture(t, {api: () => ({can_apply: false, error: 'invalid shape', generated_points: ['unverified'], generated_curves: {lines: ['unverified-line']}})});
  f.edit(1); t.mock.timers.tick(150); await f.flush();
  assert(!geometryIds(f.studio.draft.definition, 'points').includes('unverified'));
  assert(!geometryIds(f.studio.draft.definition, 'lines').includes('unverified-line'));
  f.drawing.discard(true);
});
