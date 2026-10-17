// Deleting geometry in the sketch studio. A deletion removes the chosen
// points, curves, shapes, texts, projections or patterns and everything that
// can no longer stand without them, following the classic editor's authored
// reference graph:
//
// - curves, shapes and texts built on a deleted point go with it;
// - points left unused by any remaining geometry go with the curve, shape or
//   text that used them (shared endpoints stay);
// - constraints, reference dimensions and curve, profile and region edits
//   that name anything deleted go too, as do references through them;
// - profile, hole and region loops and pattern entity lists drop the deleted
//   members; an emptied hole, region or pattern goes. A loop that loses one
//   run of curves is turned to start after the gap, so it reads as an open
//   path that `continueOutline` closes again when new curves are drawn.
//
// Generated names (a rectangle's side, a pattern copy, a projection's
// endpoint) stand for the record that makes them.
import { sketchDependencyGraph } from './sketch-dependencies.mjs';

const RECORDS = ['points', 'lines', 'circles', 'arcs', 'ellipses', 'splines', 'projections', 'texts'];
/** Graph kinds whose use of a point keeps it. */
const HOLDERS = new Set(['lines', 'circles', 'arcs', 'ellipses', 'splines', 'projections', 'texts', 'shapes', 'pattern']);
/** Graph kinds whose deletion can leave their points unused. */
const BUILT = new Set(['lines', 'circles', 'arcs', 'ellipses', 'splines', 'texts', 'shapes']);
const PATTERN_MEMBERS = ['entities', 'on_axis'];
const NOUNS = {
  points: ['point', 'points'], lines: ['line', 'lines'], circles: ['circle', 'circles'], arcs: ['arc', 'arcs'],
  ellipses: ['ellipse', 'ellipses'], splines: ['spline', 'splines'], projections: ['projection', 'projections'],
  texts: ['text', 'texts'], shapes: ['shape', 'shapes'], patterns: ['pattern', 'patterns'],
  constraints: ['constraint', 'constraints'], measurements: ['reference dimension', 'reference dimensions'],
  edits: ['curve or region edit', 'curve or region edits'],
};

const unwrap = (value) => Object.entries(value ?? {})[0] ?? ['unknown', {}];

/**
 * A lookup from authored or generated name to the record that owns it:
 * `{key, id, kind}`, where kind is a definition group ('lines', 'shapes',
 * 'patterns'…), or null. A generated name `owner:rest` belongs to the
 * longest owner prefix.
 */
function owners(definition) {
  const byId = new Map();
  for (const group of RECORDS) for (const r of definition[group] ?? []) byId.set(r.id, { key: `geometry:${r.id}`, id: r.id, kind: group });
  for (const s of definition.shapes ?? []) { const { id } = unwrap(s)[1]; byId.set(id, { key: `geometry:${id}`, id, kind: 'shapes' }); }
  for (const p of definition.patterns ?? []) { const { id } = unwrap(p)[1]; byId.set(id, { key: `pattern:${id}`, id, kind: 'patterns' }); }
  const generators = new Set(['shapes', 'patterns', 'projections', 'texts']);
  return (name) => {
    if (byId.has(name)) return byId.get(name);
    for (let end = name.lastIndexOf(':'); end > 0; end = name.lastIndexOf(':', end - 1)) {
      const owner = byId.get(name.slice(0, end));
      if (owner && generators.has(owner.kind)) return owner;
    }
    return null;
  };
}

/**
 * Deletes `ids` (authored or generated names) from sketch `definition`.
 * Returns `{definition, removed, owners, summary}`: the new definition (the
 * input is not changed), the removed names by group (constraint and edit
 * groups count entries), each generated name mapped to the record deleted
 * for it, and a sentence listing what went. Throws for an unknown name.
 * O(size of the sketch).
 */
export function deletion(definition, ids) {
  if (!ids.length) throw new Error('Select geometry to delete.');
  const graph = sketchDependencyGraph(definition);
  const ownerOf = owners(definition);
  const via = [];
  const start = [];
  for (const id of ids) {
    const owner = ownerOf(id);
    if (!owner) throw new Error(`${id} is not geometry of this sketch.`);
    if (owner.id !== id) via.push({ id, owner: owner.id, kind: owner.kind });
    start.push(owner.key);
  }

  const gone = new Set();
  const deleted = new Set();
  const queue = [];
  const idOf = (key) => graph.nodes.get(key)?.id ?? null;
  const remove = (key) => {
    if (gone.has(key) || !graph.nodes.has(key)) return;
    gone.add(key);
    if (idOf(key)) deleted.add(idOf(key));
    queue.push(key);
  };
  const patterns = new Map((definition.patterns ?? []).map((p) => [`pattern:${unwrap(p)[1].id}`, unwrap(p)[1]]));

  /** A pattern goes when it loses a non-member reference or all its entities. */
  function patternFalls(key) {
    const data = patterns.get(key);
    const members = new Set(PATTERN_MEMBERS.flatMap((field) => data[field] ?? []));
    const used = [...graph.nodes.get(key).dependencies].map(idOf).filter(Boolean);
    if (used.some((id) => deleted.has(id) && !members.has(id))) return true;
    return (data.entities ?? []).every((id) => deleted.has(id));
  }

  /** Points `key` used that no remaining holder uses. */
  function orphans(key) {
    const result = [];
    for (const dependency of graph.nodes.get(key).dependencies) {
      const point = graph.nodes.get(dependency);
      if (point?.kind !== 'points' || gone.has(dependency)) continue;
      const held = [...point.consumers].some((c) => !gone.has(c) && HOLDERS.has(graph.nodes.get(c).kind));
      if (!held) result.push(dependency);
    }
    return result;
  }

  start.forEach(remove);
  for (let i = 0; i < queue.length; i += 1) {
    const node = graph.nodes.get(queue[i]);
    for (const consumer of node.consumers) {
      const kind = graph.nodes.get(consumer).kind;
      if (gone.has(consumer) || kind === 'feature' || kind === 'datum' || kind === 'profile') continue;
      if (kind === 'pattern') {
        if (patternFalls(consumer)) remove(consumer);
      } else {
        remove(consumer);
      }
    }
    if (BUILT.has(node.kind)) orphans(queue[i]).forEach(remove);
  }

  // Apply: drop removed records and entries, prune loops and member lists.
  const indices = (prefix) => new Set([...gone].filter((k) => k.startsWith(prefix)).map((k) => Number(k.slice(prefix.length))));
  const next = structuredClone(definition);
  const removed = {};
  const note = (group, names) => { if (names.length) removed[group] = names; };
  for (const group of RECORDS) {
    if (!Array.isArray(next[group])) continue;
    note(group, next[group].filter((r) => deleted.has(r.id)).map((r) => r.id));
    next[group] = next[group].filter((r) => !deleted.has(r.id));
  }
  if (Array.isArray(next.shapes)) {
    note('shapes', next.shapes.map((s) => unwrap(s)[1].id).filter((id) => gone.has(`geometry:${id}`)));
    next.shapes = next.shapes.filter((s) => !gone.has(`geometry:${unwrap(s)[1].id}`));
  }
  if (Array.isArray(next.patterns)) {
    note('patterns', next.patterns.map((p) => unwrap(p)[1].id).filter((id) => gone.has(`pattern:${id}`)));
    next.patterns = next.patterns.filter((p) => !gone.has(`pattern:${unwrap(p)[1].id}`));
    for (const pattern of next.patterns) {
      const data = unwrap(pattern)[1];
      for (const field of PATTERN_MEMBERS) if (Array.isArray(data[field])) data[field] = data[field].filter((id) => !deleted.has(id));
    }
  }
  const constraints = indices('constraint:');
  if (Array.isArray(next.constraints)) {
    if (constraints.size) removed.constraints = constraints.size;
    next.constraints = next.constraints.filter((_, i) => !constraints.has(i));
  }
  if (Array.isArray(next.measurements)) {
    note('measurements', next.measurements.filter((m) => gone.has(`measurement:${m.id}`)).map((m) => m.id));
    next.measurements = next.measurements.filter((m) => !gone.has(`measurement:${m.id}`));
  }
  let edits = 0;
  for (const [group, prefix] of [['curve_operations', 'curve-operation:'], ['profile_operations', 'profile-operation:'], ['region_edits', 'region-edit:']]) {
    if (!Array.isArray(next[group])) continue;
    const drop = indices(prefix);
    edits += drop.size;
    next[group] = next[group].filter((_, i) => !drop.has(i));
  }
  if (edits) removed.edits = edits;
  const keep = (loop) => afterGap(loop, deleted);
  if (Array.isArray(next.profile)) next.profile = keep(next.profile);
  if (Array.isArray(next.holes)) next.holes = next.holes.map(keep).filter((loop) => loop.length);
  if (Array.isArray(next.regions)) {
    next.regions = next.regions
      .map((region) => ({ ...region, profile: keep(region.profile ?? []), holes: (region.holes ?? []).map(keep).filter((loop) => loop.length) }))
      .filter((region) => region.profile.length);
  }
  if (next.region_discovery) {
    for (const field of ['entities', 'seeds']) {
      if (Array.isArray(next.region_discovery[field])) next.region_discovery[field] = keep(next.region_discovery[field]);
    }
  }
  return { definition: next, removed, owners: via, summary: describeRemoval(removed) };
}

/**
 * `loop` without the `deleted` names, starting after the deleted run when
 * exactly one contiguous run (cyclically) was deleted, else in its order.
 */
function afterGap(loop, deleted) {
  const kept = loop.filter((id) => !deleted.has(id));
  if (!kept.length || kept.length === loop.length) return kept;
  const starts = loop.map((id, i) => !deleted.has(id) && deleted.has(loop.at(i - 1))).reduce((n, b) => n + b, 0);
  if (starts !== 1) return kept;
  const first = loop.findIndex((id, i) => !deleted.has(id) && deleted.has(loop.at(i - 1)));
  return [...loop.slice(first), ...loop.slice(0, first)].filter((id) => !deleted.has(id));
}

/** End points of open authored curves: lines, arcs and splines. */
function curveEnds(definition) {
  const ends = new Map();
  for (const l of definition.lines ?? []) ends.set(l.id, [l.start, l.end]);
  for (const a of definition.arcs ?? []) ends.set(a.id, [a.start, a.end]);
  for (const s of definition.splines ?? []) if (s.points?.length) ends.set(s.id, [s.points[0], s.points.at(-1)]);
  return ends;
}

/**
 * The free ends `[start, end]` of `loop` read as a path of curves in order,
 * each ending where the next starts (as profiles must), or null when it is
 * closed, empty or not such a path.
 */
export function openEnds(loop, ends) {
  if (!loop.length || loop.some((id) => !ends.has(id))) return null;
  for (let i = 1; i < loop.length; i += 1) {
    if (ends.get(loop[i - 1])[1] !== ends.get(loop[i])[0]) return null;
  }
  const start = ends.get(loop[0])[0];
  const end = ends.get(loop.at(-1))[1];
  return start === end ? null : [start, end];
}

/**
 * Turns new curve `id` of `definition` round, in place: a line swaps its
 * ends, an arc swaps them and its sense (the same curve). Splines are not
 * reversed. Returns whether it did.
 */
function reverse(definition, id, ends) {
  const line = (definition.lines ?? []).find((l) => l.id === id);
  const arc = !line && (definition.arcs ?? []).find((a) => a.id === id);
  const record = line || arc;
  if (!record) return false;
  [record.start, record.end] = [record.end, record.start];
  if (arc) arc.clockwise = !arc.clockwise;
  ends.set(id, [record.start, record.end]);
  return true;
}

/**
 * `next` with curves that `previous` lacked joined to any profile, hole or
 * region loop left open (by a deletion): a new line, arc or spline that
 * meets a loop's free end is appended or prepended, a line or arc turned
 * round when it runs the other way, so redrawing a deleted edge closes the
 * outline again. Other definitions are returned unchanged.
 */
export function continueOutline(previous, next) {
  const before = curveEnds(previous);
  const fresh = [...curveEnds(next).keys()].filter((id) => !before.has(id));
  const loopsOf = (d) => [
    ...(Array.isArray(d.profile) ? [d.profile] : []),
    ...(d.holes ?? []),
    ...(d.regions ?? []).flatMap((r) => [r.profile ?? [], ...(r.holes ?? [])]),
  ];
  if (!fresh.length) return next;
  const nextEnds = curveEnds(next);
  if (!loopsOf(next).some((loop) => openEnds(loop, nextEnds))) return next;
  const result = structuredClone(next);
  const ends = curveEnds(result);
  const unused = new Set(fresh);
  for (const loop of loopsOf(result)) {
    let free = openEnds(loop, ends);
    while (free && unused.size) {
      const [start, end] = free;
      const id = [...unused].find((c) => ends.get(c).includes(start) || ends.get(c).includes(end));
      if (!id) break;
      unused.delete(id);
      const [p, q] = ends.get(id);
      if (p === end) loop.push(id);
      else if (q === start) loop.unshift(id);
      else if (q === end && reverse(result, id, ends)) loop.push(id);
      else if (p === start && reverse(result, id, ends)) loop.unshift(id);
      free = openEnds(loop, ends);
    }
  }
  return result;
}

/** "2 lines, 3 points and 1 constraint", in a fixed order. */
export function describeRemoval(removed) {
  const parts = Object.keys(NOUNS)
    .filter((group) => removed[group])
    .map((group) => {
      const count = Array.isArray(removed[group]) ? removed[group].length : removed[group];
      return `${count} ${NOUNS[group][count === 1 ? 0 : 1]}`;
    });
  if (parts.length < 2) return parts[0] ?? 'nothing';
  return `${parts.slice(0, -1).join(', ')} and ${parts.at(-1)}`;
}

/**
 * Delete in the studio: the Delete or Backspace key with the Select tool,
 * and a Delete button with what it will remove in the selection inspector.
 * Deletions go into the draft like any edit, so Undo restores them.
 */
export function installDelete(studio, document) {
  const { html } = studio;
  const definition = () => {
    const layer = studio.activeLayer();
    return studio.draft?.feature === layer?.feature ? studio.draft.definition : studio.definitions[layer?.feature] ?? null;
  };

  function run() {
    const ids = [...studio.selection];
    const draft = studio.ensureDraft();
    if (!draft) {
      studio.say(`Apply or discard the draft of ${studio.draft?.feature} first.`, true);
      return;
    }
    try {
      const result = deletion(draft.definition, ids);
      studio.selection = [];
      studio.changeDraft(result.definition);
      const via = result.owners.map((o) => `${o.id} belongs to ${o.owner}`).join('; ');
      studio.say(`Deleted ${result.summary}${via ? ` (${via})` : ''}. Previewing… Undo (Ctrl+Z) restores it.`);
    } catch (error) {
      studio.say(error.message, true);
    }
  }

  const key = studio.onKey;
  studio.onKey = (event) => {
    const mod = event.ctrlKey || event.metaKey || event.altKey;
    // Keys typed into an open dimension value box belong to it.
    const typing = document.getElementById('value-box')?.hidden === false;
    if (!mod && !typing && (event.key === 'Delete' || event.key === 'Backspace') && studio.tool === 'select' && studio.selection.length) {
      event.preventDefault();
      run();
      return true;
    }
    return key?.(event) ?? false;
  };

  const inspect = studio.renderSelection;
  studio.renderSelection = (host) => {
    if (!inspect?.(host)) return false;
    const def = definition();
    if (!def || !studio.selection.length) return true;
    let plan;
    try {
      plan = deletion(def, studio.selection);
    } catch (error) {
      host.append(html('p', { style: 'color:var(--muted)' }, error.message));
      return true;
    }
    host.append(
      html('h2', {}, 'Delete'),
      html('p', { style: 'color:var(--muted)' }, `Removes ${plan.summary}.`),
      html('button', { title: 'Delete the selection and what depends on it (Delete)', onclick: run }, 'Delete selection'));
    return true;
  };
}
