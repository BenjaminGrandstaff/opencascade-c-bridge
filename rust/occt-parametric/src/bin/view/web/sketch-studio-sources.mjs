// Read-only world-space source edges, shared by drawing and canvas picking.
import {project, toGltf} from './sketch-studio-plane.mjs';
const cache = new WeakMap();
export function sourceEdgePaths(scene, {matrix, width, height}) {
  const old = cache.get(scene);
  if (old && old.width === width && old.height === height && old.matrix.every((v, i) => v === matrix[i])) return old.paths;
  const paths = (scene.edges ?? []).map(({edge, line}) => {
    const points = (scene.lines?.[line] ?? []).map(point => {
      const p = toGltf(point), at = project(matrix, p, width, height);
      const depth = matrix[3] * p[0] + matrix[7] * p[1] + matrix[11] * p[2] + matrix[15];
      return at && {at, depth};
    });
    let pen = false;
    const d = points.map(point => {
      if (!point) { pen = false; return ''; }
      const next = `${pen ? 'L' : 'M'}${point.at[0]} ${point.at[1]}`; pen = true; return next;
    }).join(' ');
    return {edge, points, d};
  });
  cache.set(scene, {matrix: [...matrix], width, height, paths});
  return paths;
}
export function pickSourceEdge(scene, viewport, at, radius = 7) {
  let best = null;
  for (const {edge, points} of sourceEdgePaths(scene, viewport)) for (let i = 1; i < points.length; i++) {
    const a = points[i - 1], b = points[i];
    if (!a || !b) continue;
    const dx = b.at[0] - a.at[0], dy = b.at[1] - a.at[1], length = dx * dx + dy * dy;
    const t = length ? Math.max(0, Math.min(1, ((at[0] - a.at[0]) * dx + (at[1] - a.at[1]) * dy) / length)) : 0;
    const distance = Math.hypot(at[0] - a.at[0] - t * dx, at[1] - a.at[1] - t * dy), depth = a.depth + t * (b.depth - a.depth);
    if (distance <= radius && (!best || distance < best.distance - 1e-6 || Math.abs(distance - best.distance) <= 1e-6 && depth < best.depth)) best = {edge, distance, depth};
  }
  return best?.edge ?? null;
}
