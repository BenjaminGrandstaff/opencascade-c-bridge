// Sketch layers: one per sketch feature, each on its own plane.
import { dot, planeNormal } from './sketch-studio-plane.mjs';

export const PALETTE = ['#2f6fdb', '#c36d12', '#1a7f37', '#8250df', '#cf222e', '#0a7f8f', '#9a6700'];

const format = (value) => Number(value.toPrecision(6)).toString();

/**
 * A short name for a sketch's plane: "Face", "Datum <id>", or the base
 * plane it lies on with its offset ("XY", "XZ @ y = -4"), else "Inclined".
 */
export function planeLabel(frame, definition = null) {
  if (definition?.face_support) return 'Face';
  if (definition?.datum_plane) return `Datum ${definition.datum_plane}`;
  if (!frame) return 'Unresolved plane';
  const n = planeNormal(frame);
  for (const [axis, name, index] of [[[0, 0, 1], 'XY', 2], [[0, 1, 0], 'XZ', 1], [[1, 0, 0], 'YZ', 0]]) {
    if (Math.abs(dot(n, axis)) > 1 - 1e-9) {
      const offset = frame.origin_mm[index];
      return Math.abs(offset) < 1e-9 ? name : `${name} @ ${'xyz'[index]} = ${format(offset)}`;
    }
  }
  return 'Inclined';
}

/**
 * Layers for the sketch scenes, keeping each feature's visibility and color
 * from `previous`, and any local layers (new sketches not yet created in
 * the model). O(scenes + previous).
 */
export function layersFrom(scenes, definitions = {}, previous = []) {
  const prior = new Map(previous.map((layer) => [layer.feature, layer]));
  const used = new Set();
  const layers = scenes
    .filter((scene) => scene.kind === 'sketch')
    .map((scene, index) => {
      const old = prior.get(scene.feature);
      used.add(scene.feature);
      return {
        feature: scene.feature,
        frame: scene.frame ?? null,
        plane: planeLabel(scene.frame, definitions[scene.feature]),
        visible: old?.visible ?? true,
        color: old?.color ?? PALETTE[index % PALETTE.length],
        error: scene.error ?? null,
        freeDegrees: scene.solver?.free_degrees ?? null,
        local: false,
      };
    });
  for (const layer of previous) {
    if (layer.local && !used.has(layer.feature)) layers.push(layer);
  }
  return layers;
}

/** A unique feature name with the given prefix among taken names. */
export function nextFeatureName(taken, prefix = 'sketch') {
  const names = new Set(taken);
  for (let i = 1; ; i += 1) if (!names.has(`${prefix}-${i}`)) return `${prefix}-${i}`;
}
