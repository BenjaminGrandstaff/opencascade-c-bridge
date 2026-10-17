// Constraint glyphs and dimension labels for the active sketch, laid out
// so none overlap: each label tries spots around its geometry, nearest
// first; labels with no free spot fold into a badge listing them.

/** Glyphs for constraint kinds; dimensions show their value instead. */
export const GLYPHS = {
  coincident: '●', horizontal: 'H', vertical: 'V', parallel: '∥', perpendicular: '⊥', tangent: 'T',
  equal_length: '=', equal_radius: '=', equal_curve_length: '=', concentric: '◎', midpoint: 'M',
  point_on_curve: '∈', collinear: '⋯', symmetric: '⇋', coradial: '◎', curvature_continuous: 'G2',
  line_circle_tangent: 'T', circle_circle_tangent: 'T', line_spline_tangent: 'T', spline_spline_tangent: 'T',
  spline_circle_tangent: 'T', ellipse_circle_tangent: 'T', ellipse_ellipse_tangent: 'T', spline_ellipse_tangent: 'T',
  line_ellipse_tangent: 'T', crossing_point: '×', spline_point: '∈', weighted_point: 'W',
};
const DIMENSIONS = new Set(['distance', 'horizontal_distance', 'vertical_distance', 'point_line_distance', 'radius',
  'diameter', 'curve_length', 'angle', 'line_angle', 'arc_angle']);

/** Approximate label size in pixels for 12 px text. */
export const labelSize = (text) => [Math.max(16, 6 + 7 * [...text].length), 16];

/**
 * The labels to draw for a solved sketch scene: one per constraint
 * annotation, with the draft constraint's kind (by index) choosing a glyph,
 * or the solver's label for dimensions. Fixed-point, free-motion, solver,
 * parameter and group annotations are left to the inspector.
 */
export function sketchLabels(scene, definition) {
  const constraints = definition?.constraints ?? [];
  // Match authored values once, including property-order differences. Generated
  // annotations must not scan every authored constraint on every redraw.
  const signature = value => JSON.stringify(value, (_, v) => v && typeof v === 'object' && !Array.isArray(v)
    ? Object.fromEntries(Object.keys(v).sort().map(key => [key, v[key]])) : v);
  const signatures = constraints.map(signature), byValue = new Map();
  signatures.forEach((key, i) => byValue.set(key, byValue.has(key) ? -1 : i));
  const authoredIndex = annotation => {
    const expected = annotation.detail?.constraint, index = annotation.detail?.constraint_index;
    if (!expected) return -1;
    const key = signature(expected);
    return Number.isInteger(index) && signatures[index] === key ? index : byValue.get(key) ?? -1;
  };
  return (scene?.annotations ?? []).flatMap((annotation) => {
    const match = /^constraint-(\d+)$/.exec(annotation.id);
    const reference = annotation.detail?.driving === false && Boolean(annotation.detail?.measurement);
    if ((!match && !reference) || !annotation.anchors?.length) return [];
    const type = Object.keys(annotation.detail?.constraint ?? constraints[Number(match?.[1])] ?? {})[0];
    const dimension = reference || DIMENSIONS.has(type) || (!type && /\d/.test(annotation.label ?? ''));
    const text = dimension ? annotation.label : GLYPHS[type] ?? annotation.label ?? '?';
    return [{
      id: annotation.id,
      reference,
      constraintIndex: reference ? -1 : authoredIndex(annotation),
      text,
      title: `${annotation.label}${type ? ` · ${type.replaceAll('_', ' ')}` : ''} · ${annotation.status}${reference ? ' · reference (read only)' : ''}`,
      dimension,
      status: annotation.status,
      anchors: annotation.anchors,
      targets: annotation.targets ?? [],
    }];
  });
}

/**
 * Candidate offsets around an anchor for a label w × h: a table of spots
 * one label apart (7 across, 9 down), nearest first, so neighbours pack
 * without gaps.
 */
function candidates(w, h) {
  const spots = [];
  for (let i = -3; i <= 3; i += 1) for (let j = -4; j <= 4; j += 1) spots.push([i * (w + 4), j * (h + 3)]);
  return spots.sort((a, b) => Math.hypot(a[0], a[1] * 2) - Math.hypot(b[0], b[1] * 2));
}
const CELL = 32;

/**
 * Places labels (with `at`, the anchor's screen position, and `text`)
 * without overlaps inside width × height. Dimensions go first. Each label
 * takes the nearest free candidate spot; labels with none are folded into
 * badges, merged when their anchors are within 24 px. Returns
 * `{placed: [{...label, box: [x, y, w, h], leader}], badges: [{at, labels}]}`.
 * O(labels × candidates) with a spatial hash of placed boxes.
 */
export function layoutLabels(labels, width, height, margin = 2) {
  const grid = new Map();
  const cells = ([x, y, w, h]) => {
    const out = [];
    for (let i = Math.floor(x / CELL); i <= Math.floor((x + w) / CELL); i += 1) {
      for (let j = Math.floor(y / CELL); j <= Math.floor((y + h) / CELL); j += 1) out.push(`${i},${j}`);
    }
    return out;
  };
  const overlaps = (box) => cells(box).some((key) => (grid.get(key) ?? []).some((other) =>
    box[0] < other[0] + other[2] + margin && other[0] < box[0] + box[2] + margin &&
    box[1] < other[1] + other[3] + margin && other[1] < box[1] + box[3] + margin));
  const take = (box) => {
    for (const key of cells(box)) {
      if (!grid.has(key)) grid.set(key, []);
      grid.get(key).push(box);
    }
  };
  const placed = [];
  const folded = [];
  const order = [...labels].sort((a, b) => Number(b.dimension) - Number(a.dimension));
  for (const label of order) {
    const [w, h] = labelSize(label.text);
    let spot = null;
    for (const [dx, dy] of candidates(w, h)) {
      const box = [label.at[0] + dx - w / 2, label.at[1] + dy - h / 2 - (label.dimension ? 10 : 0), w, h];
      if (box[0] < 0 || box[1] < 0 || box[0] + w > width || box[1] + h > height) continue;
      if (!overlaps(box)) {
        spot = { box, leader: Math.hypot(dx, dy) > 16 };
        break;
      }
    }
    if (spot) {
      take(spot.box);
      placed.push({ ...label, ...spot });
    } else {
      folded.push(label);
    }
  }
  const badges = [];
  for (const label of folded) {
    const near = badges.find((badge) => Math.hypot(badge.at[0] - label.at[0], badge.at[1] - label.at[1]) < 24);
    if (near) near.labels.push(label);
    else badges.push({ at: label.at, labels: [label] });
  }
  // Badges take free spots too where there are any.
  for (const badge of badges) {
    const [w, h] = labelSize(`+${badge.labels.length}`);
    badge.box = [badge.at[0] - w / 2, badge.at[1] - h / 2, w, h];
    for (const [dx, dy] of candidates(w, h)) {
      const box = [badge.at[0] + dx - w / 2, badge.at[1] + dy - h / 2, w, h];
      if (box[0] >= 0 && box[1] >= 0 && box[0] + w <= width && box[1] + h <= height && !overlaps(box)) {
        badge.box = box;
        break;
      }
    }
    take(badge.box);
  }
  return { placed, badges };
}
