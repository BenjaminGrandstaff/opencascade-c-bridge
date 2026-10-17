// Run: node --test rust/occt-parametric/src/bin/view/web/viewer.test.mjs
import assert from "node:assert/strict";
import test from "node:test";
import { createRenderer, encodeGlb, eye, fitCamera, instanceGroups, mergeSingles, packNormals, pan, parseGlb, parseGltf, viewProjection } from "./viewer.mjs";

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
  const lines = gltf();
  lines.meshes[0].primitives[0].mode = 1;
  assert.throws(() => parseGltf(lines), /triangle primitive/);
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

test("picking finds the nearest hit node through its matrix, or none", async () => {
  const { ray, pick } = await import("./viewer.mjs");
  const parsed = parseGltf(gltf());
  // Straight down onto node b (translated +2 in X): its triangle spans x 2..3, y 0..1 at z=0.
  const down = { origin: [2.25, 0.25, 5], direction: [0, 0, -1] };
  assert.equal(pick(parsed, down), 1);
  // Node c's triangle is turned onto the YZ plane at x = 5: hit it along -X.
  assert.equal(pick(parsed, { origin: [9, 0.2, -0.2], direction: [-1, 0, 0] }), 2);
  assert.equal(pick(parsed, { origin: [20, 20, 5], direction: [0, 0, -1] }), -1);
  // A camera ray through the view center points at the target.
  const camera = fitCamera(parsed.bounds, 1.5);
  const center = ray(camera, 1.5, 0, 0);
  const toTarget = camera.target.map((t, axis) => t - center.origin[axis]);
  const length = Math.hypot(...toTarget);
  center.direction.forEach((d, axis) => assert.ok(Math.abs(d - toTarget[axis] / length) < 1e-12));
});

test("witness points convert from model millimeters (Z up) to glTF meters (Y up)", async () => {
  const { modelToGltf } = await import("./viewer.mjs");
  assert.deepEqual([...modelToGltf([[1000, 2000, 3000]])], [1, 3, -2]);
});

/** The same triangle as binary glTF, indexed, with its three edges as lines. */
function glb() {
  const positions = new Float32Array([0, 0, 0, 1, 0, 0, 0, 1, 0]);
  const normals = new Float32Array([0, 0, 1, 0, 0, 1, 0, 0, 1]);
  const triangles = new Uint16Array([0, 1, 2, 0]); // padded to 4 bytes
  const edges = new Uint16Array([0, 1, 1, 2, 2, 0]);
  const parts = [positions, normals, triangles, edges].map((a) => new Uint8Array(a.buffer));
  const binary = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let offset = 0;
  const views = parts.map((p) => {
    binary.set(p, offset);
    const view = { buffer: 0, byteOffset: offset, byteLength: p.length };
    offset += p.length;
    return view;
  });
  return encodeGlb({
    asset: { version: "2.0" },
    buffers: [{ byteLength: binary.length }],
    bufferViews: views,
    accessors: [
      { bufferView: 0, componentType: 5126, count: 3, type: "VEC3", min: [0, 0, 0], max: [1, 1, 0] },
      { bufferView: 1, componentType: 5126, count: 3, type: "VEC3" },
      { bufferView: 2, componentType: 5123, count: 3, type: "SCALAR" },
      { bufferView: 3, componentType: 5123, count: 6, type: "SCALAR" },
    ],
    materials: [{ pbrMetallicRoughness: { baseColorFactor: [0, 0.5, 1, 1] } }],
    meshes: [{ primitives: [
      { attributes: { POSITION: 0, NORMAL: 1 }, indices: 2, material: 0, mode: 4 },
      { attributes: { POSITION: 0 }, indices: 3, mode: 1 },
    ] }],
    nodes: [
      { name: "a", mesh: 0 },
      { name: "b", mesh: 0, translation: [2, 0, 0] },
    ],
  }, binary);
}

test("binary glTF parses indexed triangles, edges, and bounds from mesh corners", async () => {
  const { pick } = await import("./viewer.mjs");
  const parsed = parseGlb(glb());
  const [mesh] = parsed.meshes;
  assert.deepEqual([...mesh.indices], [0, 1, 2]);
  assert.deepEqual([...mesh.edges], [0, 1, 1, 2, 2, 0]);
  assert.deepEqual(mesh.color, [0, 0.5, 1, 1]);
  assert.deepEqual(parsed.bounds, { min: [0, 0, 0], max: [3, 1, 0] });
  // Picking walks the indexed triangles.
  assert.equal(pick(parsed, { origin: [2.25, 0.25, 5], direction: [0, 0, -1] }), 1);
  assert.throws(() => parseGlb(new ArrayBuffer(24)), /binary glTF/);
});

test("nodes group by mesh with packed matrices, and normals pack to 16 bits", () => {
  const parsed = parseGlb(glb());
  const [group] = instanceGroups(parsed);
  assert.deepEqual(group.nodes, [0, 1]);
  assert.equal(group.matrices.length, 32);
  assert.equal(group.matrices[16 + 12], 2);
  assert.deepEqual([...packNormals(new Float32Array([0, -1, 1, 0.5]))], [0, -32767, 32767, 16384]);
});

test("ten thousand instances parse and group in linear time", () => {
  const base = parseGlb(glb());
  const started = performance.now();
  const nodes = Array.from({ length: 10000 }, (_, i) => ({ name: `n${i}`, mesh: 0, translation: [i, 0, 0] }));
  const big = encodeGlb({ ...JSON.parse(new TextDecoder().decode(new Uint8Array(glb(), 20, new DataView(glb()).getUint32(12, true)))), nodes },
    new Uint8Array(glb(), 28 + new DataView(glb()).getUint32(12, true)));
  const parsed = parseGlb(big);
  const [group] = instanceGroups(parsed);
  assert.equal(group.nodes.length, 10000);
  assert.deepEqual(parsed.bounds.max, [10000, 1, 0]);
  assert.equal(base.meshes.length, 1);
  assert.ok(performance.now() - started < 500, `${performance.now() - started} ms`);
});

test("parts drawn once merge into world-space batches with offset indices", () => {
  const parsed = parseGlb(glb());
  const batches = mergeSingles(parsed, [0, 1]);
  assert.equal(batches.length, 1);
  const [batch] = batches;
  assert.equal(batch.vertices, 6);
  // Node b's copy is moved 2 along X; its indices follow node a's three.
  assert.deepEqual([...batch.positions.slice(9, 12)], [2, 0, 0]);
  assert.deepEqual([...batch.triangles], [0, 1, 2, 3, 4, 5]);
  assert.deepEqual([...batch.edges.slice(6)], [3, 4, 4, 5, 5, 3]);
  assert.deepEqual([...batch.items], [0, 0, 0, 1, 1, 1]);
  assert.deepEqual([...batch.normals.slice(0, 3)], [0, 0, 32767]);
  // A cap splits batches without splitting a part.
  assert.equal(mergeSingles(parsed, [0, 1], 4).length, 2);
});


test("retained GPU models restore after previews without reuploading discarded CPU normals", () => {
  let serial=0; const uploaded=[],deleted=[];
  const gl=new Proxy({
    createBuffer:()=>({id:++serial}),createShader:()=>({}),createProgram:()=>({}),
    getShaderParameter:()=>true,getProgramParameter:()=>true,getAttribLocation:()=>0,getUniformLocation:()=>0,
    bufferData:(_target,data)=>uploaded.push(data),deleteBuffer:buffer=>deleted.push(buffer),
  },{get:(target,key)=>key in target?target[key]:()=>{}});
  const renderer=createRenderer({getContext:()=>gl});
  const accepted=parseGlb(glb()); renderer.setModel(accepted,{retain:true});
  assert.equal(accepted.meshes[0].normals,null);assert.equal(accepted.meshes[0].edges,null);
  const acceptedUploads=uploaded.length;
  renderer.setModel(parseGlb(glb())); assert.equal(deleted.length,0,'accepted buffers are parked');
  const beforeRestore=uploaded.length;
  renderer.setModel(accepted,{retain:true});assert.equal(uploaded.length,beforeRestore,'restoring needs no CPU payload');assert.equal(deleted.length,acceptedUploads,'preview buffers released');
  renderer.setModel(parseGlb(glb()));renderer.setModel(parseGlb(glb()));
  const beforeReload=deleted.length;
  renderer.setModel(parseGlb(glb()),{retain:true});assert.equal(deleted.length-beforeReload,2*acceptedUploads,'reload releases active preview and parked accepted model');
});
