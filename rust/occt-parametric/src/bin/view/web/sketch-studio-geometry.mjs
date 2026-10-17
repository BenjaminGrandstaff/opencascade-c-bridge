// Geometry roles for the studio, including core-verified generated names.
// Solved scene point_ids carry defining roles, never inferred from samples or
// name suffixes. The returned records are transient inspection data.
import {geometryIds} from './constraint-controls.mjs';

export function geometryReferences(definition, scene = {}) {
  const result = {};
  const entities = new Map((scene?.entities ?? []).map(e => [e.id, e]));
  const fields = {
    lines: ['start', 'end'], circles: ['center', 'rim'],
    arcs: ['center', 'start', 'end'], ellipses: ['center', 'major', 'minor'],
  };
  for (const group of ['lines', 'circles', 'arcs', 'ellipses', 'splines']) {
    const authored = definition[group] ?? [];
    const known = new Set(authored.map(e => e.id));
    result[group] = [...authored];
    for (const id of geometryIds(definition, group)) {
      if (known.has(id)) continue;
      const entity = entities.get(id), points = entity?.point_ids;
      if (!Array.isArray(points) || points.length < (fields[group]?.length ?? 2)) continue;
      if (group === 'splines') {
        result[group].push({id, points, endpoints: entity.bspline?.endpoints});
      } else {
        result[group].push({id, ...Object.fromEntries(fields[group].map((key, i) => [key, points[i]]))});
      }
    }
  }
  return result;
}
