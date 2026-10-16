import test from 'node:test';
import assert from 'node:assert/strict';
import { viewProjection } from './viewer.mjs';
import { basePlane, lookAt } from './sketch-studio-plane.mjs';
import {
  MAX_GRID_LINES, localBounds, pathData, pixelsPerMm, planeGrid, sketchPaths, sketchPoints,
} from './sketch-studio-overlay.mjs';

const frame = basePlane('xy');
const camera = lookAt({ target: [0, 0, 0], distance: 0.1, yaw: 0, pitch: 0 }, frame);
const matrix = viewProjection(camera, 1);

test('paths and points project onto the screen in order', () => {
  const scene = {
    entities: [{ id: 'L1', points: [[0, 0, 0], [10, 0, 0]] }],
    points: { a: [0, 0, 0], b: [10, 0, 0] },
  };
  const [path] = sketchPaths(scene, frame, matrix, 400, 400);
  assert.equal(path.id, 'L1');
  assert.match(path.d, /^M200\.0 200\.0L\d+\.\d 200\.0$/);
  const points = sketchPoints(scene, frame, matrix, 400, 400);
  assert.deepEqual(points.map((p) => p.id), ['a', 'b']);
  assert.ok(points[1].xy[0] > points[0].xy[0]);
});

test('a path is broken where it passes behind the eye', () => {
  const side = { target: [0, 0, 0], distance: 0.05, yaw: 0, pitch: 0 };
  const m = viewProjection(side, 1);
  // From the front (glTF +z is model -y), a line running toward and past
  // the eye along model -y loses its far end.
  const d = pathData([[0, 0], [0, -20], [0, -80], [0, -100]], basePlane('xy'), m, 400, 400);
  assert.equal((d.match(/M/g) ?? []).length, 1);
  assert.equal((d.match(/L/g) ?? []).length, 1);
});

test('bounds cover points and curve samples', () => {
  assert.deepEqual(localBounds({ points: { a: [1, 2, 0] }, entities: [{ points: [[-3, 5, 0]] }] }), {
    min: [-3, 2],
    max: [1, 5],
  });
  assert.equal(localBounds({}), null);
});

test('the grid uses a round spacing and stays within its line budget', () => {
  const scale = pixelsPerMm(frame, matrix, 400, 400);
  assert.ok(scale > 0);
  const grid = planeGrid(frame, matrix, 400, 400, { min: [-20, -10], max: [20, 10] });
  assert.ok([1, 2, 5].includes(grid.spacing / 10 ** Math.floor(Math.log10(grid.spacing))));
  assert.equal(grid.axes.length, 2);
  const huge = planeGrid(frame, matrix, 400, 400, { min: [-1e6, -1e6], max: [1e6, 1e6] });
  assert.ok(huge.lines.length <= 2 * (MAX_GRID_LINES + 1), `${huge.lines.length}`);
});
