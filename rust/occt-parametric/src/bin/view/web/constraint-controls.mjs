import {generatedPointIds,generatedCurveIds} from './generated-points.mjs';
// Typed authoring controls; native validation and solving remain authoritative.
const pair = group => [['first', group], ['second', group]];
export const constraintTools = {
  horizontal: {label:'Horizontal', fields:[['line','lines']]},
  vertical: {label:'Vertical', fields:[['line','lines']]},
  coincident: {label:'Coincident points', fields:pair('points')},
  parallel: {label:'Parallel lines', fields:pair('lines')},
  perpendicular: {label:'Perpendicular lines', fields:pair('lines')},
  collinear: {label:'Collinear lines', fields:pair('lines')},
  equal_length: {label:'Equal line lengths', fields:pair('lines')},
  equal_curve_length: {label:'Equal curve lengths', fields:pair('curves')},
  coradial: {label:'Coradial circles / arcs', fields:pair('round'), hint:'The curves share a center and radius. They may have different arc spans.'},
  symmetric: {label:'Symmetric points about line', fields:[...pair('points'),['axis','lines']], hint:'The points reflect across the infinite supporting line of the axis.'},
  equal_radius: {label:'Equal radii', fields:pair('round')},
  concentric: {label:'Concentric curves', fields:pair('centered')},
  midpoint: {label:'Point at midpoint', fields:[['point','points'],['line','lines']]},
  point_on_curve: {label:'Point on curve', fields:[['point','points'],['curve','curves']]},
  tangent: {label:'Tangent at shared point', fields:[...pair('curves'),['point','points']]},
  curvature_continuous: {label:'Curvature continuity (G2)', fields:[...pair('curves'),['point','points']], hint:'Choose the shared named endpoint of both curves. G2 includes tangency; replace the tangent constraint at this joint to avoid redundancy.'},
  line_spline_tangent: {label:'Line / spline contact tangency', fields:[['line','lines'],['spline','splines'],['point','points']], hint:'Choose a contact point seeded near the intended touch on the bounded spline and the infinite supporting line.'},
  spline_spline_tangent: {label:'Spline / spline contact tangency', fields:[...pair('splines'),['point','points']], hint:'Choose two distinct bounded splines and a contact point seeded near their intended touch.'},
  ellipse_ellipse_tangent: {label:'Ellipse / ellipse contact tangency', fields:[...pair('ellipses'),['point','points']], hint:'Choose two distinct full ellipses and a contact point seeded near their intended touch. Trimmed ellipses are unsupported for this relation.'},
  spline_ellipse_tangent: {label:'Spline / ellipse contact tangency', fields:[['spline','splines'],['ellipse','ellipses'],['point','points']], hint:'Choose a bounded spline, a full ellipse and a contact point seeded near their intended touch.'},
  spline_circle_tangent: {label:'Spline / circle or arc contact tangency', fields:[['spline','splines'],['circle','round'],['point','points']], hint:'Choose a bounded spline and a circle or arc. Seed the contact near the intended touch; arc contacts must stay inside the directed arc span.'},
  ellipse_circle_tangent: {label:'Ellipse / circle or arc contact tangency', fields:[['ellipse','ellipses'],['circle','round'],['point','points']], hint:'Choose a full ellipse and a circle or arc. Seed the contact near the intended touch; arc contacts must stay inside the directed arc span.'},
  line_ellipse_tangent: {label:'Line tangent to full ellipse', fields:[['line','lines'],['ellipse','ellipses']], options:{side:['left','right']}, hint:'Choose a full ellipse. Left/right is measured from the line’s start toward its end on its infinite supporting line.'},
  line_circle_tangent: {label:'Line tangent to circle / arc', fields:[['line','lines'],['circle','round']], options:{side:['left','right']}},
  circle_circle_tangent: {label:'Circle / arc tangency', fields:pair('round'), options:{mode:['external','internal']}},
  distance: {label:'Point distance', fields:pair('points'), dimension:'length', minimum:0},
  horizontal_distance: {label:'Horizontal distance (signed)', fields:pair('points'), dimension:'length'},
  vertical_distance: {label:'Vertical distance (signed)', fields:pair('points'), dimension:'length'},
  point_line_distance: {label:'Point to line distance (signed)', fields:[['point','points'],['line','lines']], dimension:'length'},
  radius: {label:'Radius', fields:[['curve','round']], dimension:'length', positive:true},
  diameter: {label:'Diameter', fields:[['curve','round']], dimension:'length', positive:true},
  curve_length: {label:'Curve length', fields:[['curve','curves']], dimension:'length', positive:true},
  angle: {label:'Angle between lines (signed)', fields:pair('lines'), dimension:'angle'},
  line_angle: {label:'Line angle to X axis (signed)', fields:[['line','lines']], dimension:'angle'},
  arc_angle: {label:'Arc sweep angle', fields:[['arc','arcs']], dimension:'angle', positive:true},
};
export function geometryIds(definition, group) {
  const groups = {round:['circles','arcs'],centered:['circles','arcs','ellipses'],curves:['lines','circles','arcs','ellipses','splines']};
  const keys=groups[group]??[group],ids=keys.flatMap(key=>(definition[key]??[]).map(e=>e.id));
  const kindGroup={line:'lines',circle:'circles',arc:'arcs',ellipse:'ellipses',spline:'splines',chain:'splines'};
  const suffixes={line:['start','end'],circle:['center','rim'],arc:['center','start','end'],ellipse:['center','major','minor'],spline:['start','end'],chain:['start','end']};
  for(const p of definition.projections??[]){
    if(group==='points')ids.push(...(suffixes[p.kind]??[]).map(key=>`${p.id}:${key}`),...(p.junctions??[]).map(j=>`${p.id}:junction:${j.id}`),...(p.intersections??[]).map(i=>`${p.id}:intersection:${i.id}`));
    else if(keys.includes(kindGroup[p.kind]))ids.push(p.id);
  }
  if(group==='points')ids.push(...generatedPointIds(definition));
  else ids.push(...generatedCurveIds(definition,keys));
  return [...new Set(ids)];
}
export function dimensionNumber(expression, dimension) {
  const q = expression?.literal;
  if (!q) return null;
  if (dimension === 'angle') return q.dimension === 'scalar' ? q.value * 180 / Math.PI : null;
  const factor = {millimeter:1,centimeter:10,meter:1000,inch:25.4}[q.unit];
  return q.dimension === 'length' && factor ? q.value * factor : null;
}
// O(N) time and temporary space in geometry/catalogue data for at most three roles.
export function makeConstraint(definition, type, fields, expression) {
  const tool = constraintTools[type];
  if (!tool) throw new Error('Unsupported constraint tool.');
  const data = {};
  for (const [key, group] of tool.fields) {
    if (!geometryIds(definition, group).includes(fields[key])) throw new Error(`Choose a known ${group} entity for ${key}.`);
    data[key] = fields[key];
  }
  if (data.first && data.first === data.second) throw new Error('Choose two distinct entities.');
  for (const [key, choices] of Object.entries(tool.options ?? {})) {
    if (!choices.includes(fields[key])) throw new Error(`Choose ${key}.`);
    data[key] = fields[key];
  }
  if (tool.dimension) {
    if (!expression || typeof expression !== 'object') throw new Error('Enter a driving value.');
    const number = dimensionNumber(expression, tool.dimension);
    if (expression.literal && (number === null || !Number.isFinite(number))) throw new Error('Enter a finite dimension in the correct units.');
    if (number !== null && (tool.positive && number <= 0 || tool.minimum !== undefined && number < tool.minimum)) throw new Error('This dimension must be positive' + (tool.minimum === 0 ? ' or zero.' : '.'));
    if (number !== null && tool.dimension === 'angle' && (type === 'arc_angle' ? number > 360 : Math.abs(number) > 180)) throw new Error(type === 'arc_angle' ? 'Arc sweep must be at most 360 degrees.' : 'Use a signed angle from -180 to 180 degrees.');
    data.value = structuredClone(expression);
  }
  return {[type]: data};
}
// Reference dimensions have the same entity roles, but never a driving value.
export function makeMeasurement(definition, id, type, fields, editing=null) {
  if(typeof id!=='string'||!id.trim()||id!==id.trim())throw new Error('Enter a reference name without leading or trailing spaces.');
  if(!constraintTools[type]?.dimension)throw new Error('Choose a supported reference dimension.');
  const used=new Set([...geometryIds(definition,'points'),...geometryIds(definition,'curves'),
    ...['texts','patterns','projections'].flatMap(k=>(definition[k]??[]).map(e=>e.id)),
    ...(definition.shapes??[]).flatMap(s=>Object.values(s).map(v=>v.id)),
    ...(definition.curve_operations??[]).flatMap(op=>op.split?[op.split.before,op.split.after]:[]),
    ...(definition.measurements??[]).filter((_,i)=>i!==editing).map(m=>m.id)]);
  if(used.has(id))throw new Error('That name already exists. Choose a unique reference name.');
  const data={};
  for(const [key,group] of constraintTools[type].fields){
    if(!geometryIds(definition,group).includes(fields[key]))throw new Error(`Choose a known ${group} entity for ${key}.`);
    data[key]=fields[key];
  }
  if(data.first&&data.first===data.second)throw new Error('Choose two distinct entities.');
  return {id,measure:{[type]:data}};
}
export function authoredMeasurementIndex(definition, annotation) {
  const expected=annotation?.detail?.measurement;
  if(!expected||typeof expected!=='object')return -1;
  const matches=(definition.measurements??[]).map((m,i)=>same(m,expected)?i:-1).filter(i=>i>=0);
  return matches.length===1?matches[0]:-1;
}
// Compare nested definitions without relying on serialized property order.
function same(a,b) {
  if (a === b) return true;
  if (!a || !b || typeof a !== 'object' || typeof b !== 'object') return false;
  const keys=Object.keys(a);return keys.length===Object.keys(b).length && keys.every(k=>Object.hasOwn(b,k)&&same(a[k],b[k]));
}
export function authoredConstraintIndex(definition, annotation) {
  const expected=annotation?.detail?.constraint, index=annotation?.detail?.constraint_index;
  if (!expected) return -1;
  if (Number.isInteger(index) && same(definition.constraints?.[index], expected)) return index;
  const matches=(definition.constraints ?? []).map((c,i)=>same(c,expected)?i:-1).filter(i=>i>=0);
  return matches.length===1 ? matches[0] : -1;
}

export function createConstraintControls({host, definition, parameters=[], changed, message, highlight=()=>{}, initialSelection=[], initialType='horizontal', initialReference=false}) {
  let picked=[...initialSelection], type=initialType, reference=initialReference, editing=null, fields={}, source='literal', number=0, preserved=null, name='';
  const section=document.createElement('section');section.className='constraint-tools';
  const heading=document.createElement('h3');heading.textContent='Dimensions & constraints';
  const selection=document.createElement('p'), form=document.createElement('div'), list=document.createElement('div');
  const toolSelect=document.createElement('select');toolSelect.setAttribute('aria-label','Constraint type');
  const behavior=document.createElement('select');behavior.setAttribute('aria-label','Dimension behavior');
  behavior.append(new Option('Driving dimensions & constraints','driving'),new Option('Reference dimensions (measured)','reference'));
  behavior.value=reference?'reference':'driving';
  function toolOptions(){toolSelect.replaceChildren();for(const [key,tool] of Object.entries(constraintTools))if(!reference||tool.dimension)toolSelect.append(new Option(tool.label,key));}
  toolOptions();
  const clear=document.createElement('button');clear.textContent='Clear geometry selection';clear.onclick=()=>{picked=[];highlight([]);showSelection();};
  section.append(heading,selection,clear,behavior,toolSelect,form,list);host.append(section);
  function showSelection(){selection.textContent=picked.length?`Selected: ${picked.join(', ')}`:'Click sketch points or curves to select geometry. Selection order determines first and second.';}
  function options(label, choices, value, change) {
    const row=document.createElement('label');row.textContent=label;
    const select=document.createElement('select');select.setAttribute('aria-label',label);
    for(const [id,caption] of choices)select.append(new Option(caption,id));select.value=value;
    select.onchange=()=>change(select.value);row.append(select);form.append(row);return select;
  }
  function reset(next, index=null) {
    type=reference&&!constraintTools[next]?.dimension?'distance':next;editing=index;toolSelect.value=type;
    const item=index===null?null:reference?definition.measurements[index]:definition.constraints[index];
    const data=(item?(reference?item.measure[type]:item[type]):null)??{};name=reference?item?.id??'':'';fields={};
    const used=new Set();
    for(const [key,group] of constraintTools[type].fields){
      const ids=geometryIds(definition,group), selected=picked.filter(id=>ids.includes(id)&&!used.has(id));
      fields[key]=data[key] ?? selected[0] ?? ids.find(id=>!used.has(id)) ?? '';used.add(fields[key]);
    }
    for(const [key,choices] of Object.entries(constraintTools[type].options ?? {}))fields[key]=data[key] ?? choices[0];
    if(index!==null){picked=constraintTools[type].fields.map(([key])=>data[key]);showSelection();highlight(picked);}
    preserved=data.value ?? null;number=dimensionNumber(preserved,constraintTools[type].dimension) ?? (constraintTools[type].positive?1:0);
    source=preserved?.parameter?`parameter:${preserved.parameter}`:preserved && !preserved.literal?'preserve':'literal';renderForm();
  }
  function renderForm() {
    form.replaceChildren();const tool=constraintTools[type];
    if(reference){
      const note=document.createElement('p');note.textContent='Reference dimensions measure the solved sketch without constraining it. Preview to see their values on the drawing.';form.append(note);
      const row=document.createElement('label');row.textContent='Reference name';const input=document.createElement('input');input.setAttribute('aria-label','Reference name');input.value=name;input.oninput=()=>name=input.value;row.append(input);form.append(row);
    }
    if(tool.hint){const hint=document.createElement('p');hint.textContent=tool.hint;form.append(hint);}
    if(type==='line_circle_tangent'||type==='circle_circle_tangent'||tool.dimension==='angle'){
      const hint=document.createElement('p');hint.textContent=type==='line_circle_tangent'?'Left/right is measured from the line’s start toward its end.':type==='circle_circle_tangent'?'Internal tangency places the second circle inside the first.':reference?'Reference angle values on the drawing are in radians.':'Angles are entered in degrees; positive angles turn counterclockwise.';form.append(hint);
    }
    for(const [key,group] of tool.fields)options(key[0].toUpperCase()+key.slice(1),[['','Choose geometry'],...geometryIds(definition,group).map(id=>[id,id])],fields[key],value=>{fields[key]=value;picked=Object.values(fields).filter(id=>geometryIds(definition,'points').includes(id)||geometryIds(definition,'curves').includes(id));showSelection();highlight(picked);});
    for(const [key,choices] of Object.entries(tool.options ?? {}))options(key[0].toUpperCase()+key.slice(1),choices.map(id=>[id,id]),fields[key],value=>fields[key]=value);
    if(tool.dimension&&!reference){
      const dimension=tool.dimension==='angle'?'scalar':'length';
      const choices=[['literal','Entered value'],...parameters.filter(p=>p.parameter_type?.scalar===dimension).map(p=>[`parameter:${p.id}`,`Parameter: ${p.id}`])];
      if(source.startsWith('parameter:')&&!choices.some(([id])=>id===source))choices.push([source,`Existing parameter: ${source.slice(10)}`]);
      if(preserved && source==='preserve')choices.push(['preserve','Keep existing formula']);
      options('Driving value',choices,source,value=>{source=value;renderForm();});
      const row=document.createElement('label');row.textContent=`Value (${tool.dimension==='angle'?'degrees':'mm'})`;
      const input=document.createElement('input');input.type='number';input.step='any';input.value=number;input.disabled=source!=='literal';input.setAttribute('aria-label',row.textContent);
      input.oninput=()=>number=input.value.trim()?Number(input.value):NaN;row.append(input);form.append(row);
      if(source==='preserve'){const note=document.createElement('p');note.textContent='The existing formula is retained. Choose Entered value or a parameter to replace it explicitly.';form.append(note);}
    }
    const submit=document.createElement('button');submit.textContent=reference?(editing===null?'Add reference dimension':'Update reference dimension'):(editing===null?'Add dimension / constraint':'Update selected constraint');
    submit.onclick=()=>{
      try {
        if(reference){
          const measurement=makeMeasurement(definition,name,type,fields,editing);
          if(editing===null)(definition.measurements??=[]).push(measurement);else definition.measurements[editing]=measurement;
          changed();message('Reference dimension updated in the draft. Preview to measure it.');return;
        }
        const expression=source==='preserve'?preserved:source.startsWith('parameter:')?{parameter:source.slice(10)}:{literal:{value:tool.dimension==='angle'?number*Math.PI/180:number,dimension:tool.dimension==='angle'?'scalar':'length',unit:tool.dimension==='angle'?null:'millimeter'}};
        const constraint=makeConstraint(definition,type,fields,expression);
        if(editing===null){if((definition.constraints ?? []).some(c=>same(c,constraint)))throw new Error('That constraint already exists. Edit the existing constraint instead.');(definition.constraints ??= []).push(constraint);}
        else definition.constraints[editing]=constraint;
        changed();message('Constraint updated in the draft. Preview to solve and check it.');
      } catch(error){message(error.message);}
    };form.append(submit);
    if(editing!==null){const cancel=document.createElement('button');cancel.textContent=reference?'New reference dimension':'New constraint';cancel.onclick=()=>reset(type);form.append(cancel);}
  }
  function renderList(){
    list.replaceChildren();
    for(const [index,constraint] of (definition.constraints ?? []).entries()){
      const key=Object.keys(constraint)[0],tool=constraintTools[key];
      const button=document.createElement('button');button.textContent=`${index+1}. ${tool?.label ?? key} · ${tool?tool.fields.map(([name])=>constraint[key][name]).join(', '):'advanced'}`;
      button.disabled=!tool;button.onclick=()=>{reference=false;behavior.value='driving';toolOptions();reset(key,index);highlight(tool.fields.map(([name])=>constraint[key][name]));};
      const row=document.createElement('div'),remove=document.createElement('button');remove.textContent=`Remove constraint ${index+1}`;remove.onclick=()=>{definition.constraints.splice(index,1);changed();message('Constraint removed from the draft. Preview to check the remaining constraints.');};row.append(button,remove);list.append(row);
    }
    for(const [index,item] of (definition.measurements??[]).entries()){
      const key=Object.keys(item.measure)[0],tool=constraintTools[key];
      const row=document.createElement('div'),edit=document.createElement('button'),remove=document.createElement('button');
      edit.textContent=`Reference ${item.id} · ${tool?.label??key} · ${tool?tool.fields.map(([k])=>item.measure[key][k]).join(', '):'advanced'}`;
      edit.disabled=!tool?.dimension;edit.onclick=()=>{reference=true;behavior.value='reference';toolOptions();reset(key,index);};
      remove.textContent=`Remove reference ${item.id}`;remove.onclick=()=>{definition.measurements.splice(index,1);changed();message('Reference dimension removed from the draft. Preview to check it.');};row.append(edit,remove);list.append(row);
    }
  }
  behavior.onchange=()=>{reference=behavior.value==='reference';toolOptions();reset(type);};
  toolSelect.onchange=()=>reset(toolSelect.value,reference?editing:null);showSelection();reset(type);renderList();
  return {
    get selection(){return [...picked];},
    get type(){return type;},
    get reference(){return reference;},
    pick(id){
      if(!geometryIds(definition,'points').includes(id)&&!geometryIds(definition,'curves').includes(id)){message('Select known sketch geometry. Preview generator edits to refresh generated references.');return false;}
      picked=picked.includes(id)?picked.filter(value=>value!==id):[...picked,id];
      highlight(picked);showSelection();
      const used=new Set();for(const [key,group] of constraintTools[type].fields){const match=picked.find(value=>geometryIds(definition,group).includes(value)&&!used.has(value));fields[key]=match ?? '';if(match)used.add(match);}
      renderForm();return true;
    },
    selectAnnotation(annotation){
      const measured=authoredMeasurementIndex(definition,annotation);
      if(measured>=0){const key=Object.keys(definition.measurements[measured].measure)[0];if(!constraintTools[key]?.dimension)return false;reference=true;behavior.value='reference';toolOptions();reset(key,measured);heading.scrollIntoView?.({block:'nearest'});return true;}
      const index=authoredConstraintIndex(definition,annotation);if(index<0)return false;const key=Object.keys(definition.constraints[index])[0];if(!constraintTools[key])return false;reference=false;behavior.value='driving';toolOptions();reset(key,index);heading.scrollIntoView?.({block:'nearest'});return true;},
    clear(){picked=[];highlight([]);showSelection();},
  };
}
