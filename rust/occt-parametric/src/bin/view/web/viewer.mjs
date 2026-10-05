// Dependency-free WebGL renderer for the glTF subset that
// InstanceGraph::export_gltf writes: base64 buffers, unindexed triangle
// meshes with POSITION and NORMAL, nodes with a translation or a rigid
// column-major matrix, and base colors.

const FLOAT = 5126;
const TRIANGLES = 4;

function decode(uri) {
  const prefix = "data:application/octet-stream;base64,";
  if (!uri.startsWith(prefix)) throw new Error("glTF buffer must be an embedded base64 data URI");
  const text = atob(uri.slice(prefix.length));
  const bytes = new Uint8Array(text.length);
  for (let i = 0; i < text.length; i += 1) bytes[i] = text.charCodeAt(i);
  return bytes.buffer;
}

function vectors(gltf, buffers, index) {
  const accessor = gltf.accessors[index];
  if (accessor.componentType !== FLOAT || accessor.type !== "VEC3") {
    throw new Error("only float VEC3 accessors are supported");
  }
  const view = gltf.bufferViews[accessor.bufferView];
  const offset = (view.byteOffset ?? 0) + (accessor.byteOffset ?? 0);
  // Copy, so unaligned offsets in the shared buffer are not a problem.
  return new Float32Array(buffers[view.buffer].slice(offset, offset + accessor.count * 12));
}

/** Meshes, nodes and scene bounds (glTF meters, Y up) of a glTF object. */
export function parseGltf(gltf) {
  const buffers = (gltf.buffers ?? []).map((buffer) => decode(buffer.uri));
  const meshes = (gltf.meshes ?? []).map((mesh) => {
    const [primitive] = mesh.primitives;
    if ((primitive.mode ?? TRIANGLES) !== TRIANGLES || primitive.indices !== undefined) {
      throw new Error("only unindexed triangle meshes are supported");
    }
    const material = gltf.materials?.[primitive.material];
    return {
      positions: vectors(gltf, buffers, primitive.attributes.POSITION),
      normals: vectors(gltf, buffers, primitive.attributes.NORMAL),
      color: material?.pbrMetallicRoughness?.baseColorFactor ?? [0.8, 0.8, 0.8, 1],
    };
  });
  const min = [Infinity, Infinity, Infinity];
  const max = [-Infinity, -Infinity, -Infinity];
  const nodes = (gltf.nodes ?? [])
    .filter((node) => node.mesh !== undefined)
    .map((node) => {
      const [tx, ty, tz] = node.translation ?? [0, 0, 0];
      const matrix = new Float32Array(
        node.matrix ?? [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, tx, ty, tz, 1],
      );
      const positions = meshes[node.mesh].positions;
      for (let i = 0; i < positions.length; i += 3) {
        for (let axis = 0; axis < 3; axis += 1) {
          const value =
            matrix[axis] * positions[i] +
            matrix[4 + axis] * positions[i + 1] +
            matrix[8 + axis] * positions[i + 2] +
            matrix[12 + axis];
          if (value < min[axis]) min[axis] = value;
          if (value > max[axis]) max[axis] = value;
        }
      }
      return { name: node.name ?? "", mesh: node.mesh, matrix };
    });
  return { meshes, nodes, bounds: nodes.length ? { min, max } : null };
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

const VERTEX = `
attribute vec3 position;
attribute vec3 normal;
uniform mat4 viewProjection;
uniform mat4 model;
varying vec3 surfaceNormal;
varying vec3 world;
void main() {
  world = (model * vec4(position, 1.0)).xyz;
  // Node transforms are rigid, so their rotation also turns normals.
  surfaceNormal = mat3(model) * normal;
  gl_Position = viewProjection * vec4(world, 1.0);
}`;

const FRAGMENT = `
precision mediump float;
uniform vec4 color;
uniform vec3 eye;
varying vec3 surfaceNormal;
varying vec3 world;
void main() {
  vec3 toEye = normalize(eye - world);
  float light = 0.3 + 0.7 * abs(dot(normalize(surfaceNormal), toEye));
  gl_FragColor = vec4(pow(color.rgb * light, vec3(1.0 / 2.2)), color.a);
}`;

function compile(gl, type, source) {
  const shader = gl.createShader(type);
  gl.shaderSource(shader, source);
  gl.compileShader(shader);
  if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(shader));
  return shader;
}

/** A renderer drawing parsed models on `canvas` with headlight shading. */
export function createRenderer(canvas) {
  const gl = canvas.getContext("webgl", { antialias: true });
  if (!gl) throw new Error("WebGL is not available in this browser");
  const program = gl.createProgram();
  gl.attachShader(program, compile(gl, gl.VERTEX_SHADER, VERTEX));
  gl.attachShader(program, compile(gl, gl.FRAGMENT_SHADER, FRAGMENT));
  gl.linkProgram(program);
  if (!gl.getProgramParameter(program, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(program));
  const location = {
    position: gl.getAttribLocation(program, "position"),
    normal: gl.getAttribLocation(program, "normal"),
    viewProjection: gl.getUniformLocation(program, "viewProjection"),
    model: gl.getUniformLocation(program, "model"),
    color: gl.getUniformLocation(program, "color"),
    eye: gl.getUniformLocation(program, "eye"),
  };
  let model = { meshes: [], nodes: [] };
  let buffers = [];

  function release() {
    for (const { position, normal } of buffers) {
      gl.deleteBuffer(position);
      gl.deleteBuffer(normal);
    }
    buffers = [];
  }

  function upload(data) {
    const buffer = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
    gl.bufferData(gl.ARRAY_BUFFER, data, gl.STATIC_DRAW);
    return buffer;
  }

  return {
    setModel(parsed) {
      release();
      model = parsed;
      buffers = parsed.meshes.map((mesh) => ({
        position: upload(mesh.positions),
        normal: upload(mesh.normals),
        count: mesh.positions.length / 3,
      }));
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
      gl.uniformMatrix4fv(location.viewProjection, false, viewProjection(camera, width / height));
      gl.uniform3fv(location.eye, eye(camera));
      for (const node of model.nodes) {
        const mesh = buffers[node.mesh];
        gl.bindBuffer(gl.ARRAY_BUFFER, mesh.position);
        gl.enableVertexAttribArray(location.position);
        gl.vertexAttribPointer(location.position, 3, gl.FLOAT, false, 0, 0);
        gl.bindBuffer(gl.ARRAY_BUFFER, mesh.normal);
        gl.enableVertexAttribArray(location.normal);
        gl.vertexAttribPointer(location.normal, 3, gl.FLOAT, false, 0, 0);
        gl.uniformMatrix4fv(location.model, false, node.matrix);
        gl.uniform4fv(location.color, model.meshes[node.mesh].color);
        gl.drawArrays(gl.TRIANGLES, 0, mesh.count);
      }
    },
  };
}
