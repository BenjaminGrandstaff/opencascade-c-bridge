import test from 'node:test';
import assert from 'node:assert/strict';
import { layoutLabels, sketchLabels } from './sketch-studio-labels.mjs';

const overlap = (a, b) => a[0] < b[0] + b[2] && b[0] < a[0] + a[2] && a[1] < b[1] + b[3] && b[1] < a[1] + a[3];

test('labels piled on one spot spread out without overlapping, and the rest fold into a badge', () => {
  const labels = Array.from({ length: 30 }, (_, i) => ({ id: `c${i}`, text: i % 3 ? 'H' : '12.000 mm', dimension: i % 3 === 0, at: [200, 200] }));
  const { placed, badges } = layoutLabels(labels, 400, 400);
  for (let i = 0; i < placed.length; i += 1) {
    for (let j = 0; j < i; j += 1) assert.ok(!overlap(placed[i].box, placed[j].box), `${placed[i].id} / ${placed[j].id}`);
  }
  const folded = badges.flatMap((b) => b.labels);
  assert.equal(placed.length + folded.length, 30);
  assert.ok(placed.length >= 20, `${placed.length}`);
  assert.ok(badges.length <= 1);
  // Dimensions are placed before glyphs.
  assert.ok(folded.every((l) => !l.dimension) || placed.every((l) => l.dimension));
  for (const badge of badges) for (const label of placed) assert.ok(!overlap(badge.box, label.box));
});

test('labels stay inside the view and get leaders when moved away', () => {
  const { placed } = layoutLabels([
    { id: 'a', text: 'H', dimension: false, at: [5, 5] },
    { id: 'b', text: 'V', dimension: false, at: [5, 5] },
  ], 300, 300);
  for (const label of placed) {
    assert.ok(label.box[0] >= 0 && label.box[1] >= 0);
  }
  assert.equal(placed.filter((l) => l.leader).length >= 1, true);
});

test('scene annotations become glyphs or dimension values', () => {
  const scene = {
    annotations: [
      { id: 'constraint-0', label: 'H', status: 'passed', anchors: [[0, 0, 0]], targets: ['L1'] },
      { id: 'constraint-1', label: '12.000 mm', status: 'passed', anchors: [[0, 0, 0], [12, 0, 0]], targets: ['A', 'B'] },
      { id: 'constraint-2', label: 'ON', status: 'failed', anchors: [[1, 1, 0]] },
      { id: 'fixed-A', label: 'fixed A', status: 'fixed', anchors: [[0, 0, 0]] },
      { id: 'solver-report', label: 'Constraints solved', status: 'passed', anchors: [[0, 0, 0]] },
    ],
  };
  const definition = { constraints: [{ horizontal: { line: 'L1' } }, { distance: {} }, { point_on_curve: {} }] };
  const labels = sketchLabels(scene, definition);
  assert.deepEqual(labels.map((l) => [l.text, l.dimension, l.status]), [
    ['H', false, 'passed'], ['12.000 mm', true, 'passed'], ['∈', false, 'failed'],
  ]);
});

test('layout of 5,000 labels stays fast', () => {
  const labels = Array.from({ length: 5000 }, (_, i) => ({ id: `${i}`, text: 'H', dimension: false, at: [(i * 37) % 1600, (i * 53) % 900] }));
  const start = performance.now();
  const { placed, badges } = layoutLabels(labels, 1600, 900);
  const elapsed = performance.now() - start;
  assert.equal(placed.length + badges.reduce((t, b) => t + b.labels.length, 0), 5000);
  assert.ok(elapsed < 500, `${elapsed} ms`);
});
