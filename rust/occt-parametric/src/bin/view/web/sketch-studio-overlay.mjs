// What the studio draws over the 3D view: sketches on their planes and the
// active plane's grid, as screen-space SVG path data.
import { gridSpacing, projectLocal } from './sketch-studio-plane.mjs';

/** Most grid lines drawn along either axis. */
export const MAX_GRID_LINES = 201;

const pair = ([x, y]) => `${x.toFixed(1)} ${y.toFixed(1)}`;

/**
 * SVG path data through sketch-local points, broken wherever a point falls
 * behind the eye. O(points).
 */
export function pathData(points, frame, matrix, width, height) {
  let d = '';
  let pen = false;
  for (const point of points) {
    const screen = projectLocal(matrix, frame, point, width, height);
    if (!screen) {
      pen = false;
      continue;
    }
    d += `${pen ? 'L' : 'M'}${pair(screen)}`;
    pen = true;
  }
  return d;
}

/** Each entity of a sketch scene as screen path data. O(sampled points). */
export function sketchPaths(scene, frame, matrix, width, height) {
  return (scene.entities ?? []).map((entity) => ({
    id: entity.id,
    d: pathData(entity.points, frame, matrix, width, height),
  }));
}

/** The sketch's named points on screen, skipping any behind the eye. */
export function sketchPoints(scene, frame, matrix, width, height) {
  return Object.entries(scene.points ?? {}).flatMap(([id, point]) => {
    const screen = projectLocal(matrix, frame, point, width, height);
    return screen ? [{ id, xy: screen }] : [];
  });
}

/** Sketch-local bounds of a scene's points and curves, or null when empty. */
export function localBounds(scene) {
  const min = [Infinity, Infinity];
  const max = [-Infinity, -Infinity];
  const visit = ([x, y]) => {
    min[0] = Math.min(min[0], x); min[1] = Math.min(min[1], y);
    max[0] = Math.max(max[0], x); max[1] = Math.max(max[1], y);
  };
  Object.values(scene?.points ?? {}).forEach(visit);
  (scene?.entities ?? []).forEach((entity) => entity.points.forEach(visit));
  return Number.isFinite(min[0]) ? { min, max } : null;
}

/** How many pixels one sketch millimetre covers near `center`. */
export function pixelsPerMm(frame, matrix, width, height, center = [0, 0]) {
  const at = projectLocal(matrix, frame, center, width, height);
  const right = projectLocal(matrix, frame, [center[0] + 1, center[1]], width, height);
  const up = projectLocal(matrix, frame, [center[0], center[1] + 1], width, height);
  if (!at || !right || !up) return 0;
  return Math.max(Math.hypot(right[0] - at[0], right[1] - at[1]), Math.hypot(up[0] - at[0], up[1] - at[1]));
}

/**
 * Grid lines over `bounds` (sketch-local, grown to whole spacings) with a
 * round spacing for the current zoom, at most MAX_GRID_LINES per axis, and
 * the plane's two axes through its origin.
 */
export function planeGrid(frame, matrix, width, height, bounds) {
  const center = [(bounds.min[0] + bounds.max[0]) / 2, (bounds.min[1] + bounds.max[1]) / 2];
  let spacing = gridSpacing(pixelsPerMm(frame, matrix, width, height, center));
  const span = Math.max(bounds.max[0] - bounds.min[0], bounds.max[1] - bounds.min[1]);
  while (span / spacing + 1 > MAX_GRID_LINES) spacing *= 2;
  const from = bounds.min.map((v) => Math.floor(v / spacing) * spacing);
  const to = bounds.max.map((v) => Math.ceil(v / spacing) * spacing);
  const lines = [];
  for (let u = from[0]; u <= to[0] + spacing / 2; u += spacing) {
    lines.push(pathData([[u, from[1]], [u, to[1]]], frame, matrix, width, height));
  }
  for (let v = from[1]; v <= to[1] + spacing / 2; v += spacing) {
    lines.push(pathData([[from[0], v], [to[0], v]], frame, matrix, width, height));
  }
  const axes = [
    pathData([[from[0], 0], [to[0], 0]], frame, matrix, width, height),
    pathData([[0, from[1]], [0, to[1]]], frame, matrix, width, height),
  ];
  return { spacing, lines, axes };
}
