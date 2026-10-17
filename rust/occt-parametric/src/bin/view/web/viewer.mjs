// Dependency-free WebGL renderer for the glTF that InstanceGraph::export_gltf
// writes, as binary glTF (GLB, served at /api/model.glb) or JSON with an
// embedded base64 buffer: float VEC3 POSITION and NORMAL, triangles indexed
// (unsigned short or int) or not, an optional line primitive of feature
// edges per mesh, nodes with a translation or a rigid column-major matrix,
// and base colors.
//
// Meshes shared by several nodes are drawn with instancing (WebGL 2, or
// WebGL 1's ANGLE_instanced_arrays); meshes drawn once are merged into
// world-space batches by color. So thousands of parts cost one draw call per
// shared mesh plus one per batch. Only positions and indices stay in
// JavaScript, for picking; normals and edges live on the GPU, normals packed
// as 16-bit integers.

const FLOAT = 5126;
const UNSIGNED_SHORT = 5123;
const UNSIGNED_INT = 5125;
const TRIANGLES = 4;
const LINES = 1;
const GLB_MAGIC = 0x46546c67; // "glTF"
const JSON_CHUNK = 0x4e4f534a;
const BIN_CHUNK = 0x004e4942;

function decode(uri) {
  const prefix = "data:application/octet-stream;base64,";
  if (!uri?.startsWith(prefix)) throw new Error("glTF buffer must be an embedded base64 data URI");
  const text = atob(uri.slice(prefix.length));
  const bytes = new Uint8Array(text.length);
  for (let i = 0; i < text.length; i += 1) bytes[i] = text.charCodeAt(i);
  return bytes.buffer;
}

/** The bytes of accessor `index`, as a typed array view (no copy). */
function view(gltf, buffers, index) {
  const accessor = gltf.accessors[index];
  const bufferView = gltf.bufferViews[accessor.bufferView];
  const offset = (bufferView.byteOffset ?? 0) + (accessor.byteOffset ?? 0);
  const buffer = buffers[bufferView.buffer];
  if (accessor.type === "VEC3" && accessor.componentType === FLOAT) {
    return offset % 4 === 0
      ? new Float32Array(buffer, offset, accessor.count * 3)
      : new Float32Array(buffer.slice(offset, offset + accessor.count * 12));
  }
  if (accessor.type === "SCALAR" && accessor.componentType === UNSIGNED_SHORT) {
    return offset % 2 === 0
      ? new Uint16Array(buffer, offset, accessor.count)
      : new Uint16Array(buffer.slice(offset, offset + accessor.count * 2));
  }
  if (accessor.type === "SCALAR" && accessor.componentType === UNSIGNED_INT) {
    return offset % 4 === 0
      ? new Uint32Array(buffer, offset, accessor.count)
      : new Uint32Array(buffer.slice(offset, offset + accessor.count * 4));
  }
  throw new Error("only float VEC3 and unsigned short or int SCALAR accessors are supported");
}

/** Corners of `bounds` moved by column-major `matrix`, folded into `min`/`max`. */
function grow(min, max, matrix, bounds) {
  for (let corner = 0; corner < 8; corner += 1) {
    const p = [0, 1, 2].map((axis) => ((corner >> axis) & 1 ? bounds.max[axis] : bounds.min[axis]));
    for (let axis = 0; axis < 3; axis += 1) {
      const value = matrix[axis] * p[0] + matrix[4 + axis] * p[1] + matrix[8 + axis] * p[2] + matrix[12 + axis];
      if (value < min[axis]) min[axis] = value;
      if (value > max[axis]) max[axis] = value;
    }
  }
}

function parse(gltf, buffers) {
  const meshes = (gltf.meshes ?? []).map((mesh) => {
    const triangles = mesh.primitives.find((p) => (p.mode ?? TRIANGLES) === TRIANGLES);
    if (!triangles) throw new Error("each glTF mesh needs a triangle primitive");
    const edges = mesh.primitives.find((p) => p.mode === LINES);
    const material = gltf.materials?.[triangles.material];
    // Positions and indices are copied so picking keeps them without the
    // whole downloaded buffer; normals and edges stay views until upload.
    const positions = new Float32Array(view(gltf, buffers, triangles.attributes.POSITION));
    const indices = triangles.indices === undefined ? null : view(gltf, buffers, triangles.indices).slice();
    const accessor = gltf.accessors[triangles.attributes.POSITION];
    let bounds;
    if (accessor.min && accessor.max) {
      bounds = { min: [...accessor.min], max: [...accessor.max] };
    } else {
      const min = [Infinity, Infinity, Infinity];
      const max = [-Infinity, -Infinity, -Infinity];
      for (let i = 0; i < positions.length; i += 1) {
        const axis = i % 3;
        if (positions[i] < min[axis]) min[axis] = positions[i];
        if (positions[i] > max[axis]) max[axis] = positions[i];
      }
      bounds = { min, max };
    }
    return {
      positions,
      indices,
      bounds,
      normals: view(gltf, buffers, triangles.attributes.NORMAL),
      edges: edges ? view(gltf, buffers, edges.indices) : null,
      color: material?.pbrMetallicRoughness?.baseColorFactor ?? [0.8, 0.8, 0.8, 1],
    };
  });
  const min = [Infinity, Infinity, Infinity];
  const max = [-Infinity, -Infinity, -Infinity];
  const nodes = (gltf.nodes ?? [])
    .filter((node) => node.mesh !== undefined)
    .map((node) => {
      const [tx, ty, tz] = node.translation ?? [0, 0, 0];
      const matrix = new Float32Array(node.matrix ?? [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, tx, ty, tz, 1]);
      // The mesh's bounding box corners, moved: O(nodes), and never smaller
      // than the moved mesh.
      grow(min, max, matrix, meshes[node.mesh].bounds);
      return {
        name: node.name ?? "",
        mesh: node.mesh,
        matrix,
        annotationMatrix: node.extras?.familyLocalMatrix ? new Float64Array(node.extras.familyLocalMatrix) : null,
      };
    });
  return { meshes, nodes, bounds: nodes.length ? { min, max } : null };
}

/** Meshes, nodes and scene bounds (glTF meters, Y up) of a glTF JSON object. */
export function parseGltf(gltf) {
  return parse(gltf, (gltf.buffers ?? []).map((buffer) => decode(buffer.uri)));
}

/** `parseGltf` for binary glTF (an ArrayBuffer): no base64, no copies of normals or edges. */
export function parseGlb(bytes) {
  const data = new DataView(bytes);
  if (bytes.byteLength < 20 || data.getUint32(0, true) !== GLB_MAGIC || data.getUint32(4, true) !== 2) {
    throw new Error("not a binary glTF 2.0 file");
  }
  let offset = 12;
  let json = null;
  let bin = null;
  while (offset + 8 <= bytes.byteLength) {
    const length = data.getUint32(offset, true);
    const type = data.getUint32(offset + 4, true);
    const start = offset + 8;
    if (type === JSON_CHUNK) json = JSON.parse(new TextDecoder().decode(new Uint8Array(bytes, start, length)));
    if (type === BIN_CHUNK) bin = bytes.slice(start, start + length);
    offset = start + length;
  }
  if (!json) throw new Error("binary glTF has no JSON chunk");
  return parse(json, bin ? [bin] : []);
}

/** Binary glTF of a glTF JSON object and its one binary buffer (for tests and tools). */
export function encodeGlb(gltf, binary = new Uint8Array(0)) {
  const pad = (bytes, fill) => {
    const padded = new Uint8Array(Math.ceil(bytes.length / 4) * 4).fill(fill);
    padded.set(bytes);
    return padded;
  };
  const json = pad(new TextEncoder().encode(JSON.stringify(gltf)), 0x20);
  const bin = pad(new Uint8Array(binary.buffer ?? binary, binary.byteOffset ?? 0, binary.byteLength), 0);
  const total = 12 + 8 + json.length + (bin.length ? 8 + bin.length : 0);
  const out = new Uint8Array(total);
  const data = new DataView(out.buffer);
  data.setUint32(0, GLB_MAGIC, true);
  data.setUint32(4, 2, true);
  data.setUint32(8, total, true);
  data.setUint32(12, json.length, true);
  data.setUint32(16, JSON_CHUNK, true);
  out.set(json, 20);
  if (bin.length) {
    data.setUint32(20 + json.length, bin.length, true);
    data.setUint32(24 + json.length, BIN_CHUNK, true);
    out.set(bin, 28 + json.length);
  }
  return out.buffer;
}

/**
 * A camera that frames `bounds` in a view of width/height `aspect`, looking
 * from the front-right and above.
 */
export function fitCamera(bounds, aspect = 1) {
  if (!bounds) return { target: [0, 0, 0], distance: 1, yaw: 0.8, pitch: 0.5 };
  const target = bounds.min.map((low, axis) => (low + bounds.max[axis]) / 2);
  const radius = Math.max(
    1e-6,
    Math.hypot(...bounds.max.map((high, axis) => high - bounds.min[axis])) / 2,
  );
  // The narrower of the vertical and horizontal fields of view decides.
  const half = Math.atan(Math.tan(Math.PI / 8) * Math.min(1, aspect));
  return { target, distance: (radius / Math.sin(half)) * 1.05, yaw: 0.8, pitch: 0.5 };
}

/** Eye position of an orbit camera (yaw about +Y, pitch above the XZ plane). */
export function eye(camera) {
  const { target, distance, yaw, pitch } = camera;
  return [
    target[0] + distance * Math.cos(pitch) * Math.sin(yaw),
    target[1] + distance * Math.sin(pitch),
    target[2] + distance * Math.cos(pitch) * Math.cos(yaw),
  ];
}

const subtract = (a, b) => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const cross = (a, b) => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
const dot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const unit = (a) => {
  const length = Math.hypot(...a);
  return a.map((v) => v / length);
};

/** Column-major projection × view matrix for an orbit camera. */
export function viewProjection(camera, aspect) {
  const from = eye(camera);
  const forward = unit(subtract(camera.target, from));
  const right = unit(cross(forward, [0, 1, 0]));
  const up = cross(right, forward);
  const near = camera.distance / 1000;
  const far = camera.distance * 1000;
  const f = 1 / Math.tan(Math.PI / 8);
  const view = [
    right[0], up[0], -forward[0], 0,
    right[1], up[1], -forward[1], 0,
    right[2], up[2], -forward[2], 0,
    -dot(right, from), -dot(up, from), dot(forward, from), 1,
  ];
  const projection = [
    f / aspect, 0, 0, 0,
    0, f, 0, 0,
    0, 0, (far + near) / (near - far), -1,
    0, 0, (2 * far * near) / (near - far), 0,
  ];
  const result = new Float32Array(16);
  for (let column = 0; column < 4; column += 1) {
    for (let row = 0; row < 4; row += 1) {
      let sum = 0;
      for (let k = 0; k < 4; k += 1) sum += projection[k * 4 + row] * view[column * 4 + k];
      result[column * 4 + row] = sum;
    }
  }
  return result;
}

/** Moves the orbit target in the view plane by a drag of (dx, dy) pixels. */
export function pan(camera, dx, dy, height) {
  const from = eye(camera);
  const forward = unit(subtract(camera.target, from));
  const right = unit(cross(forward, [0, 1, 0]));
  const up = cross(right, forward);
  const scale = (2 * camera.distance * Math.tan(Math.PI / 8)) / height;
  return {
    ...camera,
    target: camera.target.map((t, axis) => t - right[axis] * dx * scale + up[axis] * dy * scale),
  };
}

/** World-space ray through normalized device coordinates (x, y) in [-1, 1]. */
export function ray(camera, aspect, x, y) {
  const origin = eye(camera);
  const forward = unit(subtract(camera.target, origin));
  const right = unit(cross(forward, [0, 1, 0]));
  const up = cross(right, forward);
  const t = Math.tan(Math.PI / 8);
  const direction = unit(
    forward.map((f, axis) => f + right[axis] * x * aspect * t + up[axis] * y * t),
  );
  return { origin, direction };
}

/**
 * Index of the nearest node a ray hits, or -1. Each node's rigid matrix is
 * inverted to test its mesh in local space: a bounds check, then every
 * triangle of the meshes it passes (Möller–Trumbore). O(nodes + triangles
 * of the nodes whose bounds the ray crosses).
 */
export function pick(parsed, { origin, direction }) {
  let best = Infinity;
  let hit = -1;
  parsed.nodes.forEach((node, index) => {
    const m = node.matrix;
    const shifted = [0, 1, 2].map((axis) => origin[axis] - m[12 + axis]);
    // Rigid inverse: the transpose of the rotation, applied to the shift.
    const local = (v) => [0, 1, 2].map((c) => m[4 * c] * v[0] + m[4 * c + 1] * v[1] + m[4 * c + 2] * v[2]);
    const o = local(shifted);
    const d = local(direction);
    const mesh = parsed.meshes[node.mesh];
    let near = 0;
    let far = best;
    for (let axis = 0; axis < 3; axis += 1) {
      const inverse = 1 / d[axis];
      let t0 = (mesh.bounds.min[axis] - o[axis]) * inverse;
      let t1 = (mesh.bounds.max[axis] - o[axis]) * inverse;
      if (t0 > t1) [t0, t1] = [t1, t0];
      near = Math.max(near, t0);
      far = Math.min(far, t1);
      if (!(near <= far)) return;
    }
    const p = mesh.positions;
    const indices = mesh.indices;
    const count = indices ? indices.length : p.length / 3;
    for (let k = 0; k < count; k += 3) {
      const a = 3 * (indices ? indices[k] : k);
      const b = 3 * (indices ? indices[k + 1] : k + 1);
      const c = 3 * (indices ? indices[k + 2] : k + 2);
      const e1 = [p[b] - p[a], p[b + 1] - p[a + 1], p[b + 2] - p[a + 2]];
      const e2 = [p[c] - p[a], p[c + 1] - p[a + 1], p[c + 2] - p[a + 2]];
      const q = cross(d, e2);
      const determinant = dot(e1, q);
      if (Math.abs(determinant) < 1e-20) continue;
      const s = [o[0] - p[a], o[1] - p[a + 1], o[2] - p[a + 2]];
      const u = dot(s, q) / determinant;
      if (u < 0 || u > 1) continue;
      const r = cross(s, e1);
      const v = dot(d, r) / determinant;
      if (v < 0 || u + v > 1) continue;
      const t = dot(e2, r) / determinant;
      if (t > 0 && t < best) {
        best = t;
        hit = index;
      }
    }
  });
  return hit;
}

// One vertex shader serves faces and edges: the instance matrix arrives as
// four attribute columns (per instance when instanced, constant identity for
// merged batches, whose vertices are already in world space), and `item` is
// the node index, so selection is a single uniform.
const VERTEX = `
attribute vec3 position;
attribute vec3 normal;
attribute vec4 column0;
attribute vec4 column1;
attribute vec4 column2;
attribute vec4 column3;
attribute float item;
uniform mat4 viewProjection;
uniform float selected;
varying vec3 surfaceNormal;
varying vec3 world;
varying float faded;
void main() {
  mat4 model = mat4(column0, column1, column2, column3);
  world = (model * vec4(position, 1.0)).xyz;
  // Node transforms are rigid, so their rotation also turns normals.
  surfaceNormal = mat3(model) * normal;
  faded = selected >= 0.0 && abs(item - selected) > 0.5 ? 1.0 : 0.0;
  gl_Position = viewProjection * vec4(world, 1.0);
}`;

// Two lights fixed to the camera (key from above-left, fill from the right),
// a sky-to-ground ambient and a small highlight: crisp, even CAD shading.
const FRAGMENT = `
precision mediump float;
uniform vec4 color;
uniform vec3 eye;
uniform vec3 keyLight;
uniform vec3 fillLight;
uniform float edges;
varying vec3 surfaceNormal;
varying vec3 world;
varying float faded;
void main() {
  vec3 base = color.rgb;
  if (edges > 0.5) {
    vec3 line = mix(base, vec3(0.62), 0.75 * faded);
    gl_FragColor = vec4(line, color.a);
    return;
  }
  vec3 n = normalize(surfaceNormal);
  if (!gl_FrontFacing) n = -n;
  vec3 toEye = normalize(eye - world);
  float sky = 0.5 + 0.5 * n.y;
  vec3 ambient = mix(vec3(0.24, 0.23, 0.22), vec3(0.42, 0.44, 0.47), sky);
  float key = max(dot(n, keyLight), 0.0);
  float fill = max(dot(n, fillLight), 0.0);
  vec3 halfway = normalize(keyLight + toEye);
  float highlight = pow(max(dot(n, halfway), 0.0), 48.0);
  vec3 lit = base * (ambient + 0.62 * key + 0.22 * fill) + vec3(0.16 * highlight);
  // Unselected parts fade toward gray while one part is selected.
  lit = mix(lit, vec3(0.36) * (ambient + 0.62 * key + 0.22 * fill), 0.7 * faded);
  gl_FragColor = vec4(pow(lit, vec3(1.0 / 2.2)), color.a);
}`;

const MARKER_VERTEX = `
attribute vec3 position;
uniform mat4 viewProjection;
void main() {
  gl_Position = viewProjection * vec4(position, 1.0);
  gl_PointSize = 12.0;
}`;

const MARKER_FRAGMENT = `
precision mediump float;
void main() {
  vec2 offset = gl_PointCoord - vec2(0.5);
  float distance = length(offset);
  if (distance > 0.5) discard;
  // A red dot with a white rim, visible on light and dark parts.
  gl_FragColor = distance > 0.36 ? vec4(1.0) : vec4(0.86, 0.15, 0.15, 1.0);
}`;

/** Model-space millimeter points (Z up) as glTF meters (Y up). */
export function modelToGltf(points) {
  const out = new Float32Array(points.length * 3);
  points.forEach(([x, y, z], i) => out.set([x * 0.001, z * 0.001, -y * 0.001], i * 3));
  return out;
}

function compile(gl, type, source) {
  const shader = gl.createShader(type);
  gl.shaderSource(shader, source);
  gl.compileShader(shader);
  if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(shader));
  return shader;
}

function link(gl, vertex, fragment) {
  const program = gl.createProgram();
  gl.attachShader(program, compile(gl, gl.VERTEX_SHADER, vertex));
  gl.attachShader(program, compile(gl, gl.FRAGMENT_SHADER, fragment));
  gl.linkProgram(program);
  if (!gl.getProgramParameter(program, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(program));
  return program;
}

/** Unit normals packed as normalized 16-bit integers (6 bytes, not 12). */
export function packNormals(normals) {
  const packed = new Int16Array(normals.length);
  for (let i = 0; i < normals.length; i += 1) {
    packed[i] = Math.max(-32767, Math.min(32767, Math.round(normals[i] * 32767)));
  }
  return packed;
}

/**
 * Nodes grouped by mesh, each group's rigid matrices packed as consecutive
 * 16-float columns for instanced drawing. O(nodes).
 */
export function instanceGroups(parsed) {
  const groups = parsed.meshes.map(() => []);
  parsed.nodes.forEach((node, index) => groups[node.mesh].push(index));
  return groups.map((nodes) => {
    const matrices = new Float32Array(nodes.length * 16);
    nodes.forEach((index, k) => matrices.set(parsed.nodes[index].matrix, k * 16));
    return { nodes, matrices };
  });
}

/**
 * Meshes drawn by one node each, merged into world-space batches of one
 * color: positions moved by the node's matrix, normals turned and packed,
 * each vertex tagged with its node index, and triangle and edge indices
 * offset. A batch holds at most `cap` vertices (65,535 for 16-bit indices).
 * One draw call then covers thousands of distinct parts. O(vertices).
 */
export function mergeSingles(parsed, singles, cap = 1 << 22) {
  const byColor = new Map();
  for (const index of singles) {
    const node = parsed.nodes[index];
    const mesh = parsed.meshes[node.mesh];
    const key = mesh.color.join(",");
    if (!byColor.has(key)) byColor.set(key, { color: mesh.color, parts: [] });
    byColor.get(key).parts.push(index);
  }
  const batches = [];
  for (const { color, parts } of byColor.values()) {
    let pending = [];
    let vertices = 0;
    const flush = () => {
      if (pending.length) batches.push(build(color, pending));
      pending = [];
      vertices = 0;
    };
    for (const index of parts) {
      const count = parsed.meshes[parsed.nodes[index].mesh].positions.length / 3;
      if (vertices + count > cap) flush();
      pending.push(index);
      vertices += count;
    }
    flush();
  }
  return batches;

  function build(color, parts) {
    const meshes = parts.map((index) => parsed.meshes[parsed.nodes[index].mesh]);
    const vertexCount = meshes.reduce((n, m) => n + m.positions.length / 3, 0);
    const triangleCount = meshes.reduce((n, m) => n + (m.indices ? m.indices.length : m.positions.length / 3), 0);
    const edgeCount = meshes.reduce((n, m) => n + (m.edges ? m.edges.length : 0), 0);
    const Index = vertexCount > 65535 ? Uint32Array : Uint16Array;
    const positions = new Float32Array(vertexCount * 3);
    const normals = new Int16Array(vertexCount * 3);
    const items = new Float32Array(vertexCount);
    const triangles = new Index(triangleCount);
    const edges = new Index(edgeCount);
    let [v, t, e] = [0, 0, 0];
    parts.forEach((index, k) => {
      const m = parsed.nodes[index].matrix;
      const mesh = meshes[k];
      const p = mesh.positions;
      const n = mesh.normals;
      for (let i = 0; i < p.length; i += 3) {
        const o = 3 * v + i;
        for (let axis = 0; axis < 3; axis += 1) {
          positions[o + axis] = m[axis] * p[i] + m[4 + axis] * p[i + 1] + m[8 + axis] * p[i + 2] + m[12 + axis];
          const turned = m[axis] * n[i] + m[4 + axis] * n[i + 1] + m[8 + axis] * n[i + 2];
          normals[o + axis] = Math.max(-32767, Math.min(32767, Math.round(turned * 32767)));
        }
      }
      items.fill(index, v, v + p.length / 3);
      if (mesh.indices) {
        for (let i = 0; i < mesh.indices.length; i += 1) triangles[t + i] = mesh.indices[i] + v;
        t += mesh.indices.length;
      } else {
        for (let i = 0; i < p.length / 3; i += 1) triangles[t + i] = i + v;
        t += p.length / 3;
      }
      if (mesh.edges) {
        for (let i = 0; i < mesh.edges.length; i += 1) edges[e + i] = mesh.edges[i] + v;
        e += mesh.edges.length;
      }
      v += p.length / 3;
    });
    return { color, positions, normals, items, triangles, edges, vertices: vertexCount };
  }
}

/**
 * A renderer drawing parsed models on `canvas`, with feature edges. Meshes
 * shared by several nodes are instanced; meshes drawn once are merged into
 * world-space batches, so a frame costs one draw call per shared mesh plus
 * one per batch, not one per part.
 */
export function createRenderer(canvas) {
  const gl2 = canvas.getContext("webgl2", { antialias: true });
  const gl = gl2 ?? canvas.getContext("webgl", { antialias: true });
  if (!gl) throw new Error("WebGL is not available in this browser");
  const instancing = gl2
    ? { divisor: (i, d) => gl2.vertexAttribDivisor(i, d), elements: (...a) => gl2.drawElementsInstanced(...a) }
    : (() => {
        const ext = gl.getExtension("ANGLE_instanced_arrays");
        return ext && { divisor: (i, d) => ext.vertexAttribDivisorANGLE(i, d), elements: (...a) => ext.drawElementsInstancedANGLE(...a) };
      })();
  const wideIndices = Boolean(gl2 || gl.getExtension("OES_element_index_uint"));
  const program = link(gl, VERTEX, FRAGMENT);
  const location = {
    position: gl.getAttribLocation(program, "position"),
    normal: gl.getAttribLocation(program, "normal"),
    columns: [0, 1, 2, 3].map((i) => gl.getAttribLocation(program, `column${i}`)),
    item: gl.getAttribLocation(program, "item"),
    viewProjection: gl.getUniformLocation(program, "viewProjection"),
    selected: gl.getUniformLocation(program, "selected"),
    color: gl.getUniformLocation(program, "color"),
    eye: gl.getUniformLocation(program, "eye"),
    keyLight: gl.getUniformLocation(program, "keyLight"),
    fillLight: gl.getUniformLocation(program, "fillLight"),
    edges: gl.getUniformLocation(program, "edges"),
  };
  const markerProgram = link(gl, MARKER_VERTEX, MARKER_FRAGMENT);
  const markerLocation = {
    position: gl.getAttribLocation(markerProgram, "position"),
    viewProjection: gl.getUniformLocation(markerProgram, "viewProjection"),
  };
  const markerBuffer = gl.createBuffer();
  const IDENTITY = new Float32Array([1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]);
  let markerCount = 0;
  let selected = -1;
  let model = { meshes: [], nodes: [] };
  // Draw items: { color, position, normal, faces, edges, instances? }.
  let items = [];
  let owned = [];
  let retained = false, parked = null;

  function upload(target, data) {
    const buffer = gl.createBuffer();
    gl.bindBuffer(target, buffer);
    gl.bufferData(target, data, gl.STATIC_DRAW);
    owned.push(buffer);
    return buffer;
  }

  function release() {
    for (const buffer of owned) gl.deleteBuffer(buffer);
    owned = [];
    items = [];
  }

  function elements(indices) {
    if (!indices || !indices.length) return null;
    if (indices instanceof Uint32Array && !wideIndices) {
      throw new Error("this browser's WebGL cannot draw meshes over 65,535 vertices");
    }
    return {
      buffer: upload(gl.ELEMENT_ARRAY_BUFFER, indices),
      type: indices instanceof Uint32Array ? gl.UNSIGNED_INT : gl.UNSIGNED_SHORT,
      count: indices.length,
    };
  }

  function bind(item, primitive) {
    gl.bindBuffer(gl.ARRAY_BUFFER, item.position);
    gl.enableVertexAttribArray(location.position);
    gl.vertexAttribPointer(location.position, 3, gl.FLOAT, false, 0, 0);
    gl.bindBuffer(gl.ARRAY_BUFFER, item.normal);
    gl.enableVertexAttribArray(location.normal);
    gl.vertexAttribPointer(location.normal, 3, gl.SHORT, true, 0, 0);
    gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, primitive.buffer);
    if (item.instances) {
      gl.bindBuffer(gl.ARRAY_BUFFER, item.instances.matrices);
      location.columns.forEach((attribute, column) => {
        gl.enableVertexAttribArray(attribute);
        gl.vertexAttribPointer(attribute, 4, gl.FLOAT, false, 64, column * 16);
        instancing.divisor(attribute, 1);
      });
      gl.bindBuffer(gl.ARRAY_BUFFER, item.instances.items);
      gl.enableVertexAttribArray(location.item);
      gl.vertexAttribPointer(location.item, 1, gl.FLOAT, false, 0, 0);
      instancing.divisor(location.item, 1);
    } else {
      // Merged batches are in world space: identity matrix columns as
      // constant attributes, and a per-vertex node index.
      location.columns.forEach((attribute, column) => {
        if (instancing) instancing.divisor(attribute, 0);
        gl.disableVertexAttribArray(attribute);
        gl.vertexAttrib4fv(attribute, IDENTITY.subarray(column * 4, column * 4 + 4));
      });
      gl.bindBuffer(gl.ARRAY_BUFFER, item.items);
      gl.enableVertexAttribArray(location.item);
      gl.vertexAttribPointer(location.item, 1, gl.FLOAT, false, 0, 0);
      if (instancing) instancing.divisor(location.item, 0);
    }
  }

  function drawItem(item, edges) {
    const primitive = edges ? item.edges : item.faces;
    if (!primitive) return;
    bind(item, primitive);
    const mode = edges ? gl.LINES : gl.TRIANGLES;
    if (item.instances) instancing.elements(mode, primitive.count, primitive.type, 0, item.instances.count);
    else gl.drawElements(mode, primitive.count, primitive.type, 0);
  }

  function reset() {
    for (const attribute of [...location.columns, location.item]) {
      if (instancing) instancing.divisor(attribute, 0);
      gl.disableVertexAttribArray(attribute);
    }
    gl.disableVertexAttribArray(location.normal);
  }

  return {
    /** Highlights node `index` by fading the others; -1 clears it. */
    setSelection(index) {
      selected = index;
    },
    /** Draws dots at glTF-space points (see `modelToGltf`) over the model. */
    setMarkers(points) {
      gl.bindBuffer(gl.ARRAY_BUFFER, markerBuffer);
      gl.bufferData(gl.ARRAY_BUFFER, points, gl.STATIC_DRAW);
      markerCount = points.length / 3;
    },
    /**
     * Uploads a parsed model. Its normals and edge indices then live only on
     * the GPU (the parsed meshes drop them); positions and indices stay for
     * picking. With `retain`, keep these GPU buffers available while a temporary
     * model is shown; replacing the retained model releases the previous one.
     */
    setModel(parsed, { retain = false } = {}) {
      // Studio previews borrow the view while the accepted model stays on the
      // GPU. Keep at most one such model; no CPU normal/edge copies are needed.
      const dropParked = () => {
        if (parked) for (const buffer of parked.owned) gl.deleteBuffer(buffer);
        parked = null;
      };
      if (parsed === model && items.length) {
        retained = retain;
        if (retain) dropParked();
        return;
      }
      if (parked?.model === parsed) {
        release();
        ({model, items, owned} = parked);
        parked = null; retained = retain;
        return;
      }
      if (retained && !retain) {
        dropParked();
        parked = {model, items, owned};
        items = []; owned = [];
      } else release();
      if (retain) dropParked();
      model = parsed; retained = retain;
      const groups = instanceGroups(parsed);
      const singles = [];
      groups.forEach((group, mesh) => {
        if (group.nodes.length === 1 || (!instancing && group.nodes.length)) singles.push(...group.nodes);
      });
      const cap = wideIndices ? 1 << 22 : 65535;
      for (const batch of mergeSingles(parsed, singles, cap)) {
        items.push({
          color: batch.color,
          position: upload(gl.ARRAY_BUFFER, batch.positions),
          normal: upload(gl.ARRAY_BUFFER, batch.normals),
          items: upload(gl.ARRAY_BUFFER, batch.items),
          faces: elements(batch.triangles),
          edges: elements(batch.edges),
        });
      }
      groups.forEach((group, index) => {
        if (group.nodes.length < 2 || !instancing) return;
        const mesh = parsed.meshes[index];
        const triangles = mesh.indices ?? Uint32Array.from({ length: mesh.positions.length / 3 }, (_, i) => i);
        items.push({
          color: mesh.color,
          position: upload(gl.ARRAY_BUFFER, mesh.positions),
          normal: upload(gl.ARRAY_BUFFER, packNormals(mesh.normals)),
          faces: elements(triangles.length > 65535 || mesh.positions.length / 3 > 65535 ? Uint32Array.from(triangles) : triangles),
          edges: elements(mesh.edges),
          instances: {
            count: group.nodes.length,
            matrices: upload(gl.ARRAY_BUFFER, group.matrices),
            items: upload(gl.ARRAY_BUFFER, new Float32Array(group.nodes)),
          },
        });
      });
      for (const mesh of parsed.meshes) {
        mesh.normals = null;
        mesh.edges = null;
      }
    },
    draw(camera) {
      const width = Math.max(1, Math.round(canvas.clientWidth * devicePixelRatio));
      const height = Math.max(1, Math.round(canvas.clientHeight * devicePixelRatio));
      if (canvas.width !== width || canvas.height !== height) {
        canvas.width = width;
        canvas.height = height;
      }
      gl.viewport(0, 0, width, height);
      gl.clearColor(0, 0, 0, 0);
      gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);
      gl.enable(gl.DEPTH_TEST);
      gl.useProgram(program);
      const projection = viewProjection(camera, width / height);
      const from = eye(camera);
      const forward = unit(subtract(camera.target, from));
      const right = unit(cross(forward, [0, 1, 0]));
      const up = cross(right, forward);
      // Lights in camera space: key above-left and behind the eye, fill right.
      const toward = (x, y, z) => unit([0, 1, 2].map((a) => right[a] * x + up[a] * y - forward[a] * z));
      gl.uniformMatrix4fv(location.viewProjection, false, projection);
      gl.uniform3fv(location.eye, from);
      gl.uniform3fv(location.keyLight, toward(-0.45, 0.6, 0.65));
      gl.uniform3fv(location.fillLight, toward(0.7, -0.1, 0.5));
      gl.uniform1f(location.selected, selected);
      const opaque = items.filter((item) => item.color[3] >= 1);
      const clear = items.filter((item) => item.color[3] < 1);
      // Faces pushed back a little so edges drawn on them stay visible.
      gl.enable(gl.POLYGON_OFFSET_FILL);
      gl.polygonOffset(1, 1);
      gl.uniform1f(location.edges, 0);
      for (const item of opaque) {
        gl.uniform4fv(location.color, item.color);
        drawItem(item, false);
      }
      gl.disable(gl.POLYGON_OFFSET_FILL);
      gl.uniform1f(location.edges, 1);
      gl.uniform4fv(location.color, [0.11, 0.12, 0.13, 1]);
      for (const item of opaque) drawItem(item, true);
      if (clear.length) {
        gl.enable(gl.BLEND);
        gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
        gl.depthMask(false);
        gl.uniform1f(location.edges, 0);
        for (const item of clear) {
          gl.uniform4fv(location.color, item.color);
          drawItem(item, false);
        }
        gl.depthMask(true);
        gl.disable(gl.BLEND);
      }
      reset();
      if (markerCount) {
        // Markers stay visible through the parts that would hide them.
        gl.disable(gl.DEPTH_TEST);
        gl.useProgram(markerProgram);
        gl.uniformMatrix4fv(markerLocation.viewProjection, false, projection);
        gl.bindBuffer(gl.ARRAY_BUFFER, markerBuffer);
        gl.enableVertexAttribArray(markerLocation.position);
        gl.vertexAttribPointer(markerLocation.position, 3, gl.FLOAT, false, 0, 0);
        gl.drawArrays(gl.POINTS, 0, markerCount);
        gl.disableVertexAttribArray(markerLocation.position);
      }
    },
  };
}
