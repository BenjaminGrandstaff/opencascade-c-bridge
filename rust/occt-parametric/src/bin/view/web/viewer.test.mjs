// Run: node --test rust/occt-parametric/src/bin/view/web/viewer.test.mjs
import assert from "node:assert/strict";
import test from "node:test";
import { eye, fitCamera, pan, parseGltf, viewProjection } from "./viewer.mjs";

function gltf() {
  // One triangle, drawn by two nodes translated apart along X.
  const positions = new Float32Array([0, 0, 0, 1, 0, 0, 0, 1, 0]);
  const normals = new Float32Array([0, 0, 1, 0, 0, 1, 0, 0, 1]);
  const bytes = Buffer.concat([Buffer.from(positions.buffer), Buffer.from(normals.buffer)]);
  return {
    asset: { version: "2.0" },
    buffers: [{ byteLength: bytes.length, uri: `data:application/octet-stream;base64,${bytes.toString("base64")}` }],
    bufferViews: [
      { buffer: 0, byteOffset: 0, byteLength: 36 },
      { buffer: 0, byteOffset: 36, byteLength: 36 },
    ],
    accessors: [
      { bufferView: 0, componentType: 5126, count: 3, type: "VEC3" },
      { bufferView: 1, componentType: 5126, count: 3, type: "VEC3" },
    ],
    materials: [{ pbrMetallicRoughness: { baseColorFactor: [1, 0, 0, 1] } }],
    meshes: [{ primitives: [{ attributes: { POSITION: 0, NORMAL: 1 }, material: 0, mode: 4 }] }],
    nodes: [
      { name: "a", mesh: 0, translation: [0, 0, 0] },
      { name: "b[1]", mesh: 0, translation: [2, 0, 0] },
      // A quarter turn about Y, then 5 along X: (1,0,0) -> (5,0,-1).
      { name: "c", mesh: 0, matrix: [0, 0, -1, 0, 0, 1, 0, 0, 1, 0, 0, 0, 5, 0, 0, 1] },
    ],
  };
}

function clip(matrix, point) {
  const out = [0, 0, 0, 0];
  for (let row = 0; row < 4; row += 1) {
    out[row] = matrix[row] * point[0] + matrix[4 + row] * point[1] + matrix[8 + row] * point[2] + matrix[12 + row];
  }
  return out.map((value) => value / out[3]);
}

test("parses shared meshes, node offsets, colors and bounds", () => {
  const parsed = parseGltf(gltf());
  assert.equal(parsed.meshes.length, 1);
  assert.deepEqual([...parsed.meshes[0].positions.slice(3, 6)], [1, 0, 0]);
  assert.deepEqual(parsed.meshes[0].color, [1, 0, 0, 1]);
  assert.deepEqual(parsed.nodes.map((n) => n.name), ["a", "b[1]", "c"]);
  assert.deepEqual([...parsed.nodes[1].matrix.slice(12, 15)], [2, 0, 0]);
  assert.deepEqual(parsed.bounds, { min: [0, 0, -1], max: [5, 1, 0] });
});

test("rejects glTF outside the exported subset", () => {
  const external = gltf();
  external.buffers[0].uri = "model.bin";
  assert.throws(() => parseGltf(external), /data URI/);
  const indexed = gltf();
  indexed.meshes[0].primitives[0].indices = 0;
  assert.throws(() => parseGltf(indexed), /unindexed/);
  assert.equal(parseGltf({ ...gltf(), nodes: [] }).bounds, null);
});

test("fitted cameras keep the whole model inside narrow and wide views", () => {
  const { bounds } = parseGltf(gltf());
  for (const aspect of [0.4, 1, 2.5]) {
    const matrix = viewProjection(fitCamera(bounds, aspect), aspect);
    assert.deepEqual(clip(matrix, fitCamera(bounds, aspect).target).slice(0, 2).map((v) => Math.abs(v) < 1e-6), [true, true]);
    for (const x of [0, 5]) {
      for (const y of [0, 1]) {
        const [cx, cy, cz] = clip(matrix, [x, y, 0]);
        assert.ok(Math.abs(cx) <= 1 && Math.abs(cy) <= 1 && Math.abs(cz) <= 1, `${aspect}: ${cx}, ${cy}`);
      }
    }
  }
});

test("panning moves the target across the view, not along it", () => {
  const camera = fitCamera({ min: [0, 0, 0], max: [1, 1, 1] });
  const moved = pan(camera, 10, 0, 500);
  const shift = moved.target.map((t, axis) => t - camera.target[axis]);
  const forward = eye(camera).map((e, axis) => camera.target[axis] - e);
  assert.ok(Math.hypot(...shift) > 0);
  assert.ok(Math.abs(shift.reduce((sum, v, axis) => sum + v * forward[axis], 0)) < 1e-12);
});
