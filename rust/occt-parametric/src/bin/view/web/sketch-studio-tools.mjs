import {inheritGeneratedReferences} from './generated-points.mjs';
// Drawing tools of the sketch studio: the toolbar's tools, how many clicks
// each takes, and turning a finished gesture into draft geometry plus the
// constraints inferred while drawing. Geometry itself comes from the
// classic editor's `drawPrimitive`, unchanged.
import {ADDITIONAL,drawAdditional,additionalGhost} from './sketch-studio-primitives.mjs';
import { drawPrimitive,primitiveGhost } from './canvas-tools.mjs';
import { rulesToConstraints, tangentLineAt } from './sketch-studio-infer.mjs';
import { MODIFY_ICONS, MODIFY_TOOLS } from './sketch-studio-modify.mjs';

export const TOOLS = [
  { id: 'select', label: 'Select', key: 'V', hint: 'Click geometry to select it. Drag to orbit.' },
  { id: 'line', label: 'Line', key: 'L', hint: 'Click points to chain lines; double-click or Escape ends the chain.' },
  { id: 'rectangle', label: 'Rectangle', key: 'R', hint: 'Click two opposite corners.' },
  {id:'center_rectangle',label:'Center rect',key:'B',hint:'Click the center, then a corner. Typed width and height are full sizes.'},
  { id: 'circle', label: 'Circle', key: 'C', hint: 'Click the centre, then a point on the circle.' },
  { id: 'arc', label: 'Arc', key: 'A', hint: 'Click the centre, the start, then the end.' },
  {id:'arc_three',label:'3-point arc',key:'J',hint:'Click start, a point the arc passes through, then end.'},
  { id: 'spline', label: 'Spline', key: 'S', hint: 'Click points; double-click or Enter finishes; clicking the first point closes it.' },
  {id:'control_spline',label:'Control spline',key:'K',hint:'Click control poles; Enter or Finish commits. Set degree and optional positive weights.'},
  { id: 'ellipse', label: 'Ellipse', key: 'E', hint: 'Click the centre, a major-axis end, then the minor width.' },
  {id:'elliptical_arc',label:'Ellipse arc',key:'U',hint:'Click center, major-axis end, minor width, then start and end directions. X flips the span.'},
  {id:'conic',label:'Conic',key:'Q',hint:'Click start, tangent-line intersection, then end. Rho controls the conic family.'},
  { id: 'slot', label: 'Slot', key: 'O', hint: 'Click the two cap centres, then the width.' },
  { id: 'polygon', label: 'Polygon', key: 'G', hint: 'Click the centre, then a vertex.' },
  { id: 'point', label: 'Point', key: 'P', hint: 'Click to place a point.' },
  {id:'text',label:'Text',key:'T',hint:'Set content and installed font, then click its baseline anchor. Native Preview builds glyph outlines.'},
  { id: 'dimension', label: 'Dimension', key: 'D', hint: 'Click a line, circle or arc, or two points or lines, then type the value and press Enter.' },
  ...MODIFY_TOOLS.map((tool) => ({ ...tool, group: 'modify' })),
];

/**
 * Toolbar icons: SVG markup for a 24 × 24 view box drawn with the current
 * text colour, one per tool, each picturing the shape it draws.
 */
export const ICONS = {
  ...MODIFY_ICONS,
  center_rectangle:'<rect x="3" y="6" width="18" height="12"/><path d="M9 12h6M12 9v6"/>',
  arc_three:'<path d="M3 17Q12 -5 21 17"/><circle cx="3" cy="17" r="1.5"/><circle cx="12" cy="6" r="1.5"/><circle cx="21" cy="17" r="1.5"/>',
  elliptical_arc:'<path d="M3 12a9 6 0 1 1 9 6"/><circle cx="3" cy="12" r="1.5"/>',
  conic:'<path d="M3 18Q12 -6 21 18M3 18L12 3l9 15"/>',
  text:'<path d="M4 5h16M12 5v15M8 20h8"/>',
  control_spline:'<path d="M3 18L8 4l8 0 5 14M3 18C8 4 16 4 21 18"/><circle cx="8" cy="4" r="1.5"/><circle cx="16" cy="4" r="1.5"/>',
  select: '<path d="M6 3l12 9-5.5 1L16 20l-2.5 1-3-7L6 18z" fill="currentColor" stroke="none"/>',
  line: '<path d="M4 19L20 5"/><circle cx="4" cy="19" r="2" fill="currentColor"/><circle cx="20" cy="5" r="2" fill="currentColor"/>',
  rectangle: '<rect x="3.5" y="6" width="17" height="12"/>',
  circle: '<circle cx="12" cy="12" r="8"/><circle cx="12" cy="12" r="1.3" fill="currentColor"/>',
  arc: '<path d="M4 18A9 9 0 0 1 20 9"/><circle cx="12" cy="18" r="1.3" fill="currentColor"/>',
  spline: '<path d="M3 17C7 3 11 21 15 9s4-4 6-3"/>',
  ellipse: '<ellipse cx="12" cy="12" rx="9" ry="5.5"/>',
  slot: '<rect x="3" y="8" width="18" height="8" rx="4"/>',
  polygon: '<path d="M12 3.5l7.4 4.25v8.5L12 20.5l-7.4-4.25v-8.5z"/>',
  point: '<circle cx="12" cy="12" r="3" fill="currentColor"/><path d="M12 4v3M12 17v3M4 12h3M17 12h3"/>',
  dimension: '<path d="M4 6v12M20 6v12M4 12h16M7 9.5L4 12l3 2.5M17 9.5l3 2.5-3 2.5"/>',
};

/** Clicks that finish each shape; splines finish on request. */
export const CLICKS = { point: 1, line: 2, rectangle: 2, circle: 2, polygon: 2, arc: 3, ellipse: 3, slot: 3, spline: Infinity,center_rectangle:2,arc_three:3,elliptical_arc:5,conic:3,text:1,control_spline:Infinity };

const distance = (a, b) => Math.hypot(a[0] - b[0], a[1] - b[1]);

/** The point ids each click became, from the shape drawPrimitive added. */
function clickIds(tool, before, after, created, clicks) {
  const added = (collection) => (after[collection] ?? []).slice((before[collection] ?? []).length);
  if (tool === 'point') return [clicks[0].point ?? created[0]];
  if (tool === 'line') {
    const [line] = added('lines');
    return [line.start, line.end];
  }
  if (tool === 'rectangle') {
    const lines = added('lines');
    return [lines[0].start, lines[1].end];
  }
  if (tool === 'circle') {
    const [circle] = added('circles');
    return [circle.center, circle.rim];
  }
  if (tool === 'arc') {
    const [arc] = added('arcs');
    return [arc.center, arc.start, arc.end];
  }
  if (tool === 'ellipse') {
    const [ellipse] = added('ellipses');
    return [ellipse.center, ellipse.major, ellipse.minor];
  }
  if (tool === 'slot' || tool === 'polygon') {
    const [shape] = added('shapes');
    const body = shape.slot ?? shape.polygon;
    return tool === 'slot' ? [body.start, body.end, null] : [body.center, body.vertex];
  }
  if (tool === 'spline') {
    const [spline] = added('splines');
    return clicks.map((_, i) => spline.points[i]);
  }
  return clicks.map(() => null);
}

/**
 * Adds a finished gesture to `definition` (left unchanged): the shape, then
 * the constraints its clicks inferred. `clicks` are `{xy, point, rules}` in
 * sketch-local millimetres. Arcs take the shorter way round from start to
 * end unless `flip`; an arc starting at a line's end with its centre on the
 * line's normal there becomes tangent to it. With `auto` false no inferred
 * constraints are added. Returns the new definition, the created ids, the
 * added constraints, and for lines where the next line of a chain starts.
 */
export function commitShape(definition, tool, clicks, { profile = false, sides = 6, closed = false, flip = false, auto = true, scene = {}, ...extra } = {}) {
  if (!CLICKS[tool]) throw new Error('Choose a drawing tool.');
  let clockwise = false;
  if (tool === 'arc') {
    const [c, s, e] = clicks.map((click) => click.xy);
    const turn = (s[0] - c[0]) * (e[1] - c[1]) - (s[1] - c[1]) * (e[0] - c[0]);
    clockwise = (turn < 0) !== flip;
  }
  const additional=ADDITIONAL.has(tool)?drawAdditional(definition,tool,clicks,{profile,closed,flip,...extra}):null;
  const { definition: drawn, created } = additional??drawPrimitive(
    definition, tool, clicks.map(({ xy, point }) => ({ xy, point: point ?? null })),
    { fixed: false, profile, clockwise, closed, sides },
  );
  inheritGeneratedReferences(definition, drawn);
  const ids = additional?.clickIds??clickIds(tool, definition, drawn, created, clicks);
  const boundary = created.filter((id) => !(drawn.points ?? []).some((p) => p.id === id));
  const shape = ['line', 'circle', 'arc'].includes(tool) ? boundary[0] : null;
  const withIds = clicks.map((click, i) => ({ ...click, id: ids[i], rules: auto && !(tool==='elliptical_arc'&&i>=2&&!click.point) ? [...(click.rules ?? [])] : [] }));
  if (auto && tool === 'arc') {
    const line = tangentLineAt(drawn, scene, ids[1], clicks[0].xy);
    if (line) withIds[1].rules.push({ kind: 'tangent', line });
  }
  const constraints = rulesToConstraints(withIds.filter((click) => click.id), shape);
  const existing = new Set((drawn.constraints ?? []).map((c) => JSON.stringify(c)));
  const added = constraints.filter((c) => !existing.has(JSON.stringify(c)));
  if (added.length) drawn.constraints = [...(drawn.constraints ?? []), ...added];
  return {
    definition: drawn,
    created,
    constraints: added,
    chain: tool === 'line' ? { xy: clicks[1].xy, point: ids[1] } : null,
  };
}

/**
 * The click a typed value stands for, from the gesture's previous click
 * and the cursor, or null when the text does not fit the tool:
 * line "length" or "length, angle°"; circle and polygon "radius";
 * rectangle "width, height" (toward the cursor's quadrant).
 */
export function typedClick(tool, anchor, cursor, text) {
  if (!anchor) return null;
  const values = text.split(/[,\s]+/).filter(Boolean).map(Number);
  if (!values.length || values.some((v) => !Number.isFinite(v))) return null;
  const [a0, a1] = anchor.xy;
  const towards = Math.atan2(cursor[1] - a1, cursor[0] - a0);
  if (tool === 'line') {
    const [length, degrees] = values;
    if (!(length > 0) || values.length > 2) return null;
    const angle = degrees === undefined ? towards : (degrees * Math.PI) / 180;
    return { xy: [a0 + Math.cos(angle) * length, a1 + Math.sin(angle) * length], point: null, rules: [] };
  }
  if (tool === 'circle' || tool === 'polygon') {
    const [radius] = values;
    if (!(radius > 0) || values.length > 1) return null;
    const angle = distance(anchor.xy, cursor) > 1e-12 ? towards : 0;
    return { xy: [a0 + Math.cos(angle) * radius, a1 + Math.sin(angle) * radius], point: null, rules: [] };
  }
  if (tool === 'rectangle'||tool==='center_rectangle') {
    let [width, height] = values;
    if (!(width > 0) || !(height > 0) || values.length !== 2) return null;
    if(tool==='center_rectangle'){width/=2;height/=2;}
    const sx = cursor[0] < a0 ? -1 : 1;
    const sy = cursor[1] < a1 ? -1 : 1;
    return { xy: [a0 + sx * width, a1 + sy * height], point: null, rules: [] };
  }
  return null;
}

export function studioGhost(tool,clicks,cursor,options={}){return ADDITIONAL.has(tool)?additionalGhost(tool,clicks,cursor,options):primitiveGhost(tool,clicks,cursor,options);}
