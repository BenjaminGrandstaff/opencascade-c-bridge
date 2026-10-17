import test from 'node:test';
import assert from 'node:assert/strict';
import { fitCamera, ray, viewProjection } from './viewer.mjs';
import {
  basePlane, gridSpacing, lookAt, pickFace, planeNormal, project, projectLocal, rayToLocal,
  toGltf, toLocal, toWorld,
} from './sketch-studio-plane.mjs';

const close = (a, b, tolerance = 1e-9) =>
  assert.ok(a.every((v, i) => Math.abs(v - b[i]) <= tolerance), `${a} vs ${b}`);

test('base planes match the New sketch conventions and offsets move along the normal', () => {
  close(planeNormal(basePlane('xy')), [0, 0, 1]);
  close(planeNormal(basePlane('xz')), [0, -1, 0]);
  close(planeNormal(basePlane('yz')), [1, 0, 0]);
  close(basePlane('xz', 4).origin_mm, [0, -4, 0]);
  assert.throws(() => basePlane('zz'));
  assert.throws(() => basePlane('xy', NaN));
});

test('local and world coordinates round-trip on a tilted plane', () => {
  const s = Math.SQRT1_2;
  const frame = { origin_mm: [5, -2, 7], x_axis: [s, s, 0], y_axis: [0, 0, 1] };
  const world = toWorld(frame, [3, 4]);
  close(world, [5 + 3 * s, -2 + 3 * s, 11]);
  close(toLocal(frame, world), [3, 4]);
});

test('a ray through a projected plane point lands back on it', () => {
  const frame = { origin_mm: [10, 20, 5], x_axis: [0, 1, 0], y_axis: [0, 0, 1] };
  const camera = { target: toGltf([10, 20, 5]), distance: 0.3, yaw: 0.7, pitch: 0.4 };
  const [width, height] = [800, 600];
  const matrix = viewProjection(camera, width / height);
  for (const local of [[0, 0], [12.5, -3], [-40, 25]]) {
    const [sx, sy] = projectLocal(matrix, frame, local, width, height);
    const hit = rayToLocal(frame, ray(camera, width / height, (sx / width) * 2 - 1, 1 - (sy / height) * 2));
    // The renderer's matrix is float32: about 1e-7 relative.
    close(hit, local, 1e-4);
  }
});

test('rays parallel to or behind the plane miss it', () => {
  const frame = basePlane('xy');
  assert.equal(rayToLocal(frame, { origin: toGltf([0, 0, 10]), direction: [1, 0, 0] }), null);
  assert.equal(rayToLocal(frame, { origin: toGltf([0, 0, 10]), direction: [0, 1, 0] }), null);
  assert.equal(project(viewProjection(fitCamera(null), 1), [0, 0, 100], 100, 100), null);
});

test('look at faces each plane square with its x axis to the right', () => {
  for (const name of ['xy', 'xz', 'yz']) {
    const frame = basePlane(name, 3);
    const camera = lookAt({ target: [0, 0, 0], distance: 0.2, yaw: 2, pitch: -0.3 }, frame, [1, 2]);
    const matrix = viewProjection(camera, 1);
    const center = projectLocal(matrix, frame, [1, 2], 500, 500);
    close(center, [250, 250], 1e-6);
    const right = projectLocal(matrix, frame, [11, 2], 500, 500);
    const up = projectLocal(matrix, frame, [1, 12], 500, 500);
    assert.ok(right[0] > center[0] + 1 && Math.abs(right[1] - center[1]) < 0.5, `${name} x`);
    assert.ok(up[1] < center[1] - 1 && Math.abs(up[0] - center[0]) < 0.5, `${name} y`);
  }
});

test('grid spacing picks 1, 2 or 5 times a power of ten', () => {
  assert.equal(gridSpacing(24), 1);
  assert.equal(gridSpacing(10), 5);
  assert.equal(gridSpacing(0.5), 50);
  assert.equal(gridSpacing(100), 0.5);
  assert.equal(gridSpacing(0), 1);
});

test('face picking returns the nearest triangle face along the ray', () => {
  // Two squares at z = 0 (face 3) and z = 5 (face 7), seen from above.
  const square = (z, face) => [
    { face, points: [[0, 0, z], [10, 0, z], [10, 10, z]] },
    { face, points: [[0, 0, z], [10, 10, z], [0, 10, z]] },
  ];
  const triangles = [...square(0, 3), ...square(5, 7)];
  const down = { origin: toGltf([4, 6, 50]), direction: [0, -1, 0] };
  assert.equal(pickFace(triangles, down), 7);
  const up = { origin: toGltf([4, 6, -50]), direction: [0, 1, 0] };
  assert.equal(pickFace(triangles, up), 3);
  assert.equal(pickFace(triangles, { origin: toGltf([40, 6, 50]), direction: [0, -1, 0] }), -1);
});

test('face hits return the nearest exact family-coordinate point with unit conversion',async()=>{
 const {pickFaceHit,pickFace,toGltf}=await import('./sketch-studio-plane.mjs');
 const mesh=[{face:4,points:[[0,0,1],[10,0,1],[0,10,1]]},{face:8,points:[[0,0,3],[10,0,3],[0,10,3]]}],ray={origin:toGltf([2,3,10]),direction:[0,-1,0]};
 assert.deepEqual(pickFaceHit(mesh,ray),{face:8,point_mm:[2,3,3]});assert.equal(pickFace(mesh,ray),8);
 assert.equal(pickFaceHit(mesh,{origin:toGltf([20,20,10]),direction:[0,-1,0]}),null);assert.equal(pickFaceHit(mesh,{origin:toGltf([2,3,10]),direction:[0,1,0]}),null);
});
