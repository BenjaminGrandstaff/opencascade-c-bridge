import {geometryIds} from './constraint-controls.mjs';

export function boundaryCurves(definition) {
  return [...new Set([...geometryIds(definition, 'curves'),
    ...(definition.shapes ?? []).flatMap(shape => Object.values(shape).map(value => value.id)),
    ...(definition.curve_operations ?? []).flatMap(op => op.split ? [op.split.before, op.split.after] : [])])];
}

// Only material selection changes. Native Preview decides closure, orientation,
// containment, intersections and the validity of all dependent solids.
export function materialBoundaries(definition, fields) {
  const empty = {profile: [], holes: [], regions: [], region_discovery: null};
  if (fields.mode === 'automatic') return empty;
  const curves = new Set(boundaryCurves(definition));
  const ids = (values, known, label, allowEmpty = false) => {
    if (!Array.isArray(values) || values.length > 1000 || !allowEmpty && !values.length || new Set(values).size !== values.length || values.some(id => !known.has(id))) throw Error(`${label} needs distinct known names (1–1000, or empty only where allowed).`);
    return [...values];
  };
  if (fields.mode === 'discovery') {
    if (definition.profile_operations?.length) throw Error('Remove outer-profile operations before switching to region discovery.');
    const d = fields.discovery;
    if (!d || !['all', 'selected'].includes(d.selection)) throw Error('Choose all bounded cells or selected seeds.');
    const entities = ids(d.entities, curves, 'Discovery curves', true);
    const seeds = d.selection === 'all' ? [] : ids(d.seeds, new Set(geometryIds(definition, 'points')), 'Discovery seeds');
    return {...empty, region_discovery: {...(entities.length ? {entities} : {}), ...(seeds.length ? {seeds} : {}), ...(d.merge_adjacent ? {merge_adjacent: true} : {})}};
  }
  if (fields.mode !== 'explicit') throw Error('Choose a material selection mode.');
  const used = new Set();
  const loop = (values, label) => {
    const names = ids(values, curves, label);
    for (const id of names) { if (used.has(id)) throw Error(`Boundary curve ${id} is used in more than one loop.`); used.add(id); }
    return names;
  };
  const holes = (values, label) => {
    if (!Array.isArray(values) || values.length > 1000) throw Error('Use at most 1000 holes per region.');
    return values.map((value, i) => loop(value, `${label} ${i + 1}`));
  };
  if (!Array.isArray(fields.regions) || fields.regions.length > 1000) throw Error('Use at most 1000 additional regions.');
  return {...empty, profile: loop(fields.profile, 'Outer boundary'), holes: holes(fields.holes, 'Hole'),
    regions: fields.regions.map((region, i) => {
      const profile = loop(region.profile, `Region ${i + 1}`), inner = holes(region.holes ?? [], `Region ${i + 1} hole`);
      return {profile, ...(inner.length ? {holes: inner} : {})};
    })};
}

export function createBoundaryControls({host, definition, changed, message, highlight = () => {}, beginPicking = () => {}}) {
  const parse = value => value.split(',').map(id => id.trim()).filter(Boolean);
  const fields = {mode: definition.region_discovery ? 'discovery' : definition.profile?.length ? 'explicit' : 'automatic',
    profile: [...(definition.profile ?? [])], holes: structuredClone(definition.holes ?? []), regions: structuredClone(definition.regions ?? []),
    discovery: {...structuredClone(definition.region_discovery ?? {entities: [], seeds: [], merge_adjacent: false}), selection: definition.region_discovery?.seeds?.length ? 'selected' : 'all'}};
  fields.discovery.entities ??= []; fields.discovery.seeds ??= [];
  const knownCurves = new Set(boundaryCurves(definition)), knownPoints = new Set(geometryIds(definition, 'points'));
  let picking = null;
  const cancelPick = () => { picking = null; };
  const section = document.createElement('section'); host.append(section);
  const button = (label, action, parent = section) => { const b = document.createElement('button'); b.textContent = label; b.onclick = () => { try { action(); } catch (error) { message(error.message); } }; parent.append(b); return b; };
  function choice(label, options, value, change) {
    const row = document.createElement('label'); row.textContent = label;
    const control = document.createElement('select'); control.setAttribute('aria-label', label);
    for (const [id, text] of options) control.append(new Option(text, id));
    control.value = value; control.onchange = () => change(control.value); row.append(control); section.append(row);
  }
  function names(label, get, set, group = 'curves') {
    const row = document.createElement('label'); row.textContent = `${label} (ordered names, comma separated)`;
    const control = document.createElement('textarea'); control.setAttribute('aria-label', label); control.rows = 2; control.value = get().join(', '); control.style.width = '100%';
    control.oninput = () => set(parse(control.value)); row.append(control); section.append(row);
    const show = () => { control.value = get().join(', '); highlight(get()); };
    button(`Pick ${label} on canvas`, () => { beginPicking(); picking = {group, get, set, show, known: group === 'points' ? knownPoints : knownCurves}; message(`Click geometry in ${label} order, then finish picking. Repeated names are ignored.`); });
    button(`Remove last from ${label}`, () => { set(get().slice(0, -1)); show(); });
    button(`Clear ${label}`, () => { set([]); show(); });
  }
  function render() {
    cancelPick(); section.replaceChildren();
    const note = document.createElement('p'); note.textContent = 'Choose material boundaries for the sketch and its dependent solids. Picks only fill this form. Stage replaces the material selection in one Undo entry; geometry and operations remain. Native Preview verifies closed loops, holes and regions.'; section.append(note);
    choice('Material selection', [['automatic', 'Automatic single boundary'], ['explicit', 'Explicit boundaries and holes'], ['discovery', 'Discover bounded cells']], fields.mode, value => { fields.mode = value; render(); });
    if (fields.mode === 'explicit') {
      names('Outer boundary', () => fields.profile, value => fields.profile = value);
      function holeRows(region, prefix) {
        region.holes ??= [];
        region.holes.forEach((_, i) => { names(`${prefix}hole ${i + 1}`, () => region.holes[i], value => region.holes[i] = value); button(`Remove ${prefix}hole ${i + 1}`, () => { region.holes.splice(i, 1); render(); }); });
        button(`Add ${prefix}hole`, () => { if (region.holes.length >= 1000) throw Error('Use at most 1000 holes per region.'); region.holes.push([]); render(); });
      }
      holeRows(fields, '');
      fields.regions.forEach((region, i) => {
        names(`Region ${i + 1} outer boundary`, () => region.profile, value => region.profile = value);
        holeRows(region, `Region ${i + 1} `);
        button(`Remove region ${i + 1}`, () => { fields.regions.splice(i, 1); render(); });
      });
      button('Add material region', () => { if (fields.regions.length >= 1000) throw Error('Use at most 1000 additional regions.'); fields.regions.push({profile: [], holes: []}); render(); });
    } else if (fields.mode === 'discovery') {
      const note = document.createElement('p'); note.textContent = 'Empty discovery curves uses all sketch curves. All bounded cells fills inner cells too. Seeds must lie strictly inside the selected material. Outer-profile operations must be removed before using discovery.'; section.append(note);
      names('Discovery curves', () => fields.discovery.entities, value => fields.discovery.entities = value);
      choice('Discovered cell selection', [['all', 'All bounded cells'], ['selected', 'Cells containing selected seeds']], fields.discovery.selection, value => { fields.discovery.selection = value; render(); });
      if (fields.discovery.selection === 'selected') names('Discovery seeds', () => fields.discovery.seeds, value => fields.discovery.seeds = value, 'points');
      const row = document.createElement('label'); row.textContent = 'Merge adjacent selected cells';
      const merge = document.createElement('input'); merge.type = 'checkbox'; merge.checked = fields.discovery.merge_adjacent; merge.setAttribute('aria-label', row.textContent); merge.onchange = () => fields.discovery.merge_adjacent = merge.checked; row.append(merge); section.append(row);
    } else {
      const note = document.createElement('p'); note.textContent = 'Automatic uses all sketch curves as one boundary. Staging clears explicit holes, extra regions and discovery selection. Use explicit boundaries to exclude construction geometry.'; section.append(note);
    }
    button('Finish boundary pick', () => { cancelPick(); message('Review the ordered names, then stage material boundaries.'); });
    button('Stage material boundaries', () => { const patch = materialBoundaries(definition, fields); Object.assign(definition, patch); changed(); });
  }
  render();
  return {cancelPick, get pickGroup() { return picking?.group ?? null; }, pick(id) {
    if (!picking) return false;
    if (!picking.known.has(id)) { message('Choose a known source curve or point for this boundary selection.'); return false; }
    const names = picking.get();
    if (names.includes(id)) { message(`${id} is already in this selection.`); return true; }
    if (names.length >= 1000) { message('Use at most 1000 names per boundary selection.'); return false; }
    picking.set([...names, id]); picking.show(); return true;
  }};
}
