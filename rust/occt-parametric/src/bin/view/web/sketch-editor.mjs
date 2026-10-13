import {createSketchLinks,requireConstructionProfile} from "./sketch-links.mjs";
import {createCanvasTools} from "./canvas-tools.mjs";
import {createConstraintControls} from "./constraint-controls.mjs";
import {createProfileControls} from "./profile-controls.mjs";
// A local draft is isolated from the studio until a guarded, verified Apply.
export const length = value => ({ literal: { value, dimension: 'length', unit: 'millimeter' } });
export function addElement(definition, collection, element) {
  if (!element.id?.trim()) throw new Error('Enter an entity name.');
  const collections = ['points','lines','circles','arcs','ellipses','splines','shapes','texts','patterns','projections','measurements'];
  if (collections.some(key => (definition[key] ?? []).some(item => item.id === element.id))) throw new Error('That entity name already exists.');
  (definition[collection] ??= []).push(structuredClone(element));
}
export function createSketchEditor({ host, request, instance, preview, applied, restore, showSketch = () => {}, highlight = () => {}, ghost = () => {}, canvasMode = () => {}, showSolid=()=>{}, highlightFace=()=>{}, highlightEdge=()=>{}, beginPicking=()=>{}, currentVersion=()=>null }) {
  let dragInFlight = false;
  let baseline = null, draft = null, version = null, feature = null, busy = false, generation = 0, parameters = [], features = [], constraintControls = null, profileControls = null;
  const panel = document.createElement('section');
  panel.className = 'linked-edit';
  const launch = document.createElement('button'); launch.textContent = 'Edit sketch geometry';
  const content = document.createElement('div'), status = document.createElement('p'); status.setAttribute('role', 'status');
  const canvasMount = document.createElement('div'); canvasMount.hidden = true;
  panel.append(launch, canvasMount, content, status); host.prepend(panel);
  const message = text => { status.textContent = text; };
  const links=createSketchLinks({context:()=>({draft,feature,version,features,instance:instance()}),
    request,preview,showSolid,highlightFace,highlightEdge,beginPicking:()=>{profileControls?.cancelPick();beginPicking();},currentVersion,message,
    changed:render,addProjection:(definition,projection)=>{requireConstructionProfile(definition);addElement(definition,'projections',projection);}});

  const canvasTools = createCanvasTools({host:canvasMount, getDraft:()=>draft, setDraft:next=>draft=next,
    changed:render, message, ghost:paths=>ghost(paths,feature), modeChanged:mode=>canvasMode(mode,feature),
    dragPoint:async(point,target)=>{
      const turn=++generation, selectedInstance=instance();
      dragInFlight=true;busy=true;content.inert=true;canvasMount.inert=true;launch.disabled=true;message('Solving drag with existing constraints…');
      try {
        const response=await request('/api/sketch-drag',{method:'POST',body:JSON.stringify({version,instance:selectedInstance,feature,expected:baseline,definition:draft,point,target})});
        if(turn!==generation||selectedInstance!==instance())return;
        draft=response.definition;canvasTools.reviewed();render();await preview(response.annotations);message('Drag solved and verified in the draft. Apply to update the model.');
      }catch(error){if(turn===generation)message(`Drag rejected: ${error.message}`);}
      finally{dragInFlight=false;busy=false;content.inert=false;canvasMount.inert=false;launch.disabled=false;}
    },
  });
  function resetCanvas(){links.cancel();profileControls?.cancelPick();generation++;canvasTools.reviewed();canvasMount.hidden=true;canvasMode('select');}
  function button(text, action) {
    const b = document.createElement('button'); b.textContent = text;
    b.addEventListener('click', async () => {
      if (busy) return;
      try { await action(); } catch (error) { message(error.message); }
    }); return b;
  }
  function input(label, value, change, type = 'text') {
    const row = document.createElement('label'); row.textContent = label;
    const control = document.createElement('input'); control.type = type;
    if (type === 'checkbox') control.checked = value; else control.value = value;
    if (type === 'number') control.step = 'any';
    control.addEventListener('change', () => {
      if (busy) return;
      if (type === 'number' && (!control.value.trim() || !Number.isFinite(Number(control.value)))) { message('Enter a finite coordinate.'); return; }
      change(type === 'checkbox' ? control.checked : type === 'number' ? Number(control.value) : control.value);
      message('Draft changed. Preview before applying.');
    }); row.append(control); return row;
  }
  function jsonEditor(label, value, change) {
    let current = structuredClone(value);
    const row = document.createElement('label'); row.textContent = label;
    const control = document.createElement('textarea'); control.value = JSON.stringify(value, null, 2); control.rows = 5; control.style.width = '100%';
    control.addEventListener('change', () => {
      try { const next = JSON.parse(control.value); change(next); current = structuredClone(next); message('Draft changed. Preview before applying.'); }
      catch (error) { message(`Invalid JSON: ${error.message}`); control.value = JSON.stringify(current, null, 2); }
    }); row.append(control); return row;
  }
  function render() {
    content.replaceChildren();
    const note = document.createElement('p'); note.textContent = `Family-wide edit · ${feature}. Coordinates are sketch-local. Free point coordinates are solver starting positions; constraints determine the solved shape.`; content.append(note);
    links.render(content);
    profileControls=createProfileControls({host:content,definition:draft,parameters,initialType:profileControls?.type,initialScope:profileControls?.scope,initialSeed:profileControls?.seed,changed:render,message,highlight:ids=>highlight(ids,feature),
      beginPicking:()=>{links.cancel();canvasTools.selectMode();showSketch(feature);beginPicking();}});
    constraintControls = createConstraintControls({host:content, definition:draft, parameters,
      initialSelection:constraintControls?.selection ?? [], initialType:constraintControls?.type ?? 'horizontal',
      changed:render, message, highlight:ids=>highlight(ids, feature)});
    for (const point of draft.points ?? []) {
      const row = document.createElement('fieldset'); row.dataset.editPoint = point.id;
      const title = document.createElement('legend'); title.textContent = point.id; row.append(title);
      for (const axis of ['x','y']) {
        const q = point[axis]?.literal;
        if (q?.unit === 'millimeter') row.append(input(`${axis.toUpperCase()} (mm)`, q.value, value => point[axis] = length(value), 'number'));
        else row.append(jsonEditor(`${axis.toUpperCase()} expression`, point[axis], value => point[axis] = value));
      }
      row.append(input('Fixed position', point.fixed, value => point.fixed = value, 'checkbox'));
      row.append(button('Remove point', () => { draft.points = draft.points.filter(p => p.id !== point.id); render(); message('Point removed from draft. Referencing geometry must also be updated.'); }));
      content.append(row);
    }
    const add = document.createElement('details'), title = document.createElement('summary'); title.textContent = 'Add geometry'; add.append(title);
    const kind = document.createElement('select'); for (const name of ['point','line','circle']) kind.append(new Option(name, name));
    const fields = document.createElement('div'); let values = {};
    function form() {
      fields.replaceChildren(); values = { id: '', x: 0, y: 0, fixed: true, first: draft.points?.[0]?.id ?? '', second: draft.points?.[1]?.id ?? '' };
      fields.append(input('Entity name', '', value => values.id = value));
      if (kind.value === 'point') {
        fields.append(input('X (mm)', 0, value => values.x = value, 'number'), input('Y (mm)', 0, value => values.y = value, 'number'), input('Fixed position', true, value => values.fixed = value, 'checkbox'));
      } else {
        for (const [key, caption] of [['first', kind.value === 'line' ? 'Start point' : 'Center point'], ['second', kind.value === 'line' ? 'End point' : 'Rim point']]) {
          const label = document.createElement('label'); label.textContent = caption;
          const select = document.createElement('select'); for (const p of draft.points ?? []) select.append(new Option(p.id, p.id)); select.value = values[key]; select.onchange = () => values[key] = select.value;
          label.append(select); fields.append(label);
        }
      }
    }
    kind.onchange = form; form(); add.append(kind, fields, button('Add to draft', () => {
      const k = kind.value;
      if (k !== 'point' && (!values.first || !values.second || values.first === values.second)) throw new Error('Choose two distinct points.');
      const element = k === 'point' ? {id: values.id, x: length(values.x), y: length(values.y), fixed: values.fixed} : k === 'line' ? {id: values.id, start: values.first, end: values.second} : {id: values.id, center: values.first, rim: values.second};
      addElement(draft, {point:'points',line:'lines',circle:'circles'}[k], element); render(); message('Geometry added to the draft.');
    })); content.append(add);
    content.append(input('Outer profile (curve names, comma separated)', (draft.profile ?? []).join(', '), value => draft.profile = value.split(',').map(s => s.trim()).filter(Boolean)));
    const constraints = document.createElement('details'), ct = document.createElement('summary'); ct.textContent = `Advanced constraint definitions (${draft.constraints?.length ?? 0})`; constraints.append(ct);
    for (const [index, constraint] of (draft.constraints ?? []).entries()) {
      constraints.append(jsonEditor(`Constraint ${index + 1}`, constraint, value => { draft.constraints[index] = value; render(); }), button(`Remove constraint ${index + 1}`, () => { draft.constraints.splice(index, 1); render(); message('Constraint removed from draft.'); }));
    }
    content.append(constraints);
    const advanced = document.createElement('details'), at = document.createElement('summary'); at.textContent = 'Full definition (advanced geometry, regions and constraints)'; advanced.append(at, jsonEditor('Sketch definition', draft, value => { draft = value; render(); })); content.append(advanced);
    content.append(button('Preview draft', () => submit(false)), button('Apply verified draft', () => submit(true)), button('Discard draft', async () => { generation++; baseline = draft = null; resetCanvas(); constraintControls = profileControls = null; highlight([]); content.replaceChildren(); message('Draft discarded.'); await restore(); }));
  }
  async function submit(apply) {
    if(links.busy)throw new Error("Wait for the source pick to finish.");
    links.cancel();
    profileControls?.cancelPick();
    busy = true; content.inert = true; canvasMount.inert = true; launch.disabled = true;
    message(apply ? 'Regenerating and checking requirements…' : 'Computing draft preview…');
    try {
      const response = await request('/api/sketch', {method:'POST', body:JSON.stringify({version, instance:instance(), feature, expected:baseline, definition:draft, apply})});
      if (apply) { baseline = draft = null; resetCanvas(); constraintControls = profileControls = null; highlight([]); content.replaceChildren(); await applied(response.state); message('Sketch applied. Save writes it to the model file.'); }
      else { canvasTools.reviewed();await preview(response.annotations); showSketch(feature); message(response.can_apply ? 'Preview passes regeneration and required checks. Apply repeats the checks against the current model.' : `Draft cannot be applied: ${response.error}`); }
    } finally { busy = false; content.inert = false; canvasMount.inert = false; launch.disabled = false; }
  }
  launch.addEventListener('click', async () => {
    if (busy || links.busy) return;
    links.cancel();
    profileControls?.cancelPick();
    const turn = ++generation;
    try {
      const data = await request('/api/sketches'); if (turn !== generation) return;
      if (!data.sketches.length) { message('This family has no authored sketch features.'); return; }
      // Explicit selection avoids changing a different sketch than the displayed one.
      content.replaceChildren();
      const choice = document.createElement('select'); choice.setAttribute('aria-label', 'Sketch to edit');
      for (const item of data.sketches) choice.append(new Option(item.feature, item.feature));
      content.append(choice, button('Load sketch draft', () => {
        const item = data.sketches.find(s => s.feature === choice.value);
        canvasTools.reviewed(); canvasMount.hidden=false;parameters = data.parameters ?? [];features = data.features ?? [];links.cancel();constraintControls = profileControls = null; highlight([]); version = data.version; feature = item.feature; baseline = structuredClone(item.definition); draft = structuredClone(baseline); showSketch(feature);canvasMode(canvasTools.mode,feature);render(); message('Draft loaded. Click sketch points or curves to select geometry for dimensions and constraints.');
      })); message('Choose a sketch. Loading replaces any previous local draft.');
    } catch (error) { message(error.message); }
  });
  return {
    get facePicking(){return links.facePicking;},get edgePicking(){return links.edgePicking;},
    pickFace:links.pickFace,pickEdge:links.pickEdge,cancelLinkPick(){links.cancel();profileControls?.cancelPick();},
    canvasDown(event){return !busy&&draft&&event.scene.feature===feature ? canvasTools.down(event):false;},
    canvasMove(event){return !busy&&draft&&event.scene.feature===feature ? canvasTools.move(event):false;},
    canvasUp(event){if(!busy&&draft&&event.scene.feature===feature)return canvasTools.up(event).catch(error=>message(error.message));},
    canvasCancel(){generation++;canvasTools.cancel();if(dragInFlight)message('Drag canceled. Current draft retained.');},
    selectGeometry(id, scene) {if(busy || !draft || scene.feature !== feature)return false;if(profileControls?.pick(id))return true;return constraintControls?.pick(id) ?? false;},
    selectConstraint(annotation, scene) {if(busy || !draft || scene.feature !== feature)return;if(annotation?.detail?.constraint && !constraintControls?.selectAnnotation(annotation))message('This label has no matching supported constraint in the draft. Preview your changes or use the constraint list.');},
    clearSelection() {profileControls?.cancelPick();constraintControls?.clear();},
    selectPoint(id, scene) {
    if (busy || !draft || scene.feature !== feature) return;
    const row = [...content.querySelectorAll('[data-edit-point]')].find(row => row.dataset.editPoint === id);
    row?.scrollIntoView({block:'nearest'}); row?.querySelector('input,textarea')?.focus();
  }};
}
