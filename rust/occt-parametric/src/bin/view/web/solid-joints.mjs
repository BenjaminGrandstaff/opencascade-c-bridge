// Fastened joints in the solid editor: socket head cap screws through a
// stack of solids, held by nuts or tapped into the last one. A new joint
// adds the hardware, one joint_holes feature per stacked solid and a
// compound of them all as the result; editing changes the joint only, and
// its holes and group follow. Pure state helpers plus one renderer that uses
// the editor's own select/number/button rows.
const length = value => ({literal:{value,dimension:'length',unit:'millimeter'}});
const mm = q => q?.dimension==='length' ? q.value*({millimeter:1,centimeter:10,meter:1000,inch:25.4}[q.unit]??NaN) : NaN;
const vector = (values,dimension) => ({components:Object.fromEntries(['x','y','z'].map((key,i)=>[key,{literal:{value:values[i],dimension,unit:dimension==='length'?'millimeter':null}}]))});
const readVector = (value,dimension) => {
  const values=['x','y','z'].map(key=>value?.literal?.[key]??value?.components?.[key]?.literal);
  return values.every(v=>v?.dimension===dimension)?values.map(v=>dimension==='length'?mm(v):v.value):null;
};
/** Most solids one joint clamps, and most screws, as the engine allows. */
export const MAX_JOINT_STACK = 8, MAX_JOINT_POSITIONS = 1000;
const SCREW = 'socket_head_cap_screw_iso_4762';

/** Editor state for a saved joint body, or a new one clamping `target`. */
export function readJoint(body,target='') {
  const positions=(body?.positions??[]).map(p=>readVector(p,'length'));
  const axis=body?readVector(body.axis,'scalar'):[0,0,-1];
  const size=body?.nominal_diameter;
  const screwLength=body?.length;
  return {
    stack:body?structuredClone(body.stack):target?[target]:[''],
    // A saved formula position or axis is kept as it was unless replaced.
    positions:positions.every(Boolean)?positions:[],savedPositions:positions.every(Boolean)?null:structuredClone(body.positions),
    axis:axis??[0,0,-1],savedAxis:axis?null:structuredClone(body.axis),
    size:size?.parameter?`parameter:${size.parameter}`:size?.literal?String(mm(size.literal)):'6',
    savedSize:size&&!size.parameter&&!size.literal?structuredClone(size):null,
    fastening:body?.fastening??'nut',head:body?.head??'surface',washers:body?.washers??true,clearance:body?.clearance??'medium',
    length:screwLength?.literal?String(mm(screwLength.literal)):screwLength?'preserve':'auto',savedLength:screwLength&&!screwLength.literal?structuredClone(screwLength):null,
    engagement:body?.engagement?.literal?mm(body.engagement.literal):NaN,savedEngagement:body?.engagement&&!body.engagement.literal?structuredClone(body.engagement):null,
  };
}

/** The fastened_joint body for `state`, or an error naming what to fix. */
export function jointOperation(state) {
  const stack=state.stack;
  if(!stack.length||stack.some(id=>!id))throw Error('Choose every stacked solid.');
  if(stack.length>MAX_JOINT_STACK)throw Error(`A joint clamps at most ${MAX_JOINT_STACK} solids.`);
  if(new Set(stack).size!==stack.length)throw Error('Each solid can appear in the stack once.');
  if(state.fastening==='tapped'&&stack.length<2)throw Error('A tapped joint needs a clamped solid and a tapped solid.');
  if(!state.savedPositions){
    if(!state.positions.length)throw Error('Add at least one screw position.');
    if(state.positions.length>MAX_JOINT_POSITIONS)throw Error(`A joint has at most ${MAX_JOINT_POSITIONS} screws.`);
    if(!state.positions.every(p=>p.length===3&&p.every(Number.isFinite)))throw Error('Enter finite screw positions.');
  }
  if(!state.savedAxis&&(!state.axis.every(Number.isFinite)||Math.hypot(...state.axis)===0))throw Error('Enter a finite, nonzero screw axis.');
  const size=state.savedSize??(state.size.startsWith('parameter:')?{parameter:state.size.slice(10)}:length(Number(state.size)));
  if(size.literal&&!(size.literal.value>0))throw Error('Choose a screw size.');
  const body={stack:[...stack],positions:state.savedPositions?structuredClone(state.savedPositions):state.positions.map(p=>vector(p,'length')),
    axis:state.savedAxis?structuredClone(state.savedAxis):vector(state.axis,'scalar'),nominal_diameter:size,fastening:state.fastening,head:state.head,washers:state.washers};
  if(state.clearance!=='medium')body.clearance=state.clearance;
  if(state.length==='preserve')body.length=structuredClone(state.savedLength);
  else if(state.length!=='auto'){const value=Number(state.length);if(!(value>0))throw Error('Enter a positive screw length.');body.length=length(value);}
  if(state.savedEngagement)body.engagement=structuredClone(state.savedEngagement);
  else if(Number.isFinite(state.engagement)){if(!(state.engagement>0))throw Error('Thread engagement must be positive.');body.engagement=length(state.engagement);}
  return body;
}

/** Feature ids a new joint named `id` adds: the hardware, each solid's
 * holes, and the group that is the result. */
export function jointIds(id,stack) {
  return {hardware:`${id}-hardware`,holes:stack.map(part=>`${id}-${part}`),result:id};
}

/** The edit for a new joint, or for a changed saved one (`editing`). */
export function jointChanges(features,id,body,editing=null) {
  if(editing){
    if(!editing.operation.fastened_joint)throw Error('Keep the existing feature type.');
    if(JSON.stringify(editing.operation.fastened_joint.stack)!==JSON.stringify(body.stack))throw Error('Keep the joint’s stacked solids; make a new joint to clamp others.');
    return {action:'replace',features:[{id:editing.id,operation:{fastened_joint:body}}]};
  }
  if(!id?.trim())throw Error('Enter a result feature name.');
  for(const part of body.stack)if(!features.some(f=>f.id===part))throw Error(`Unknown stacked solid '${part}'.`);
  const ids=jointIds(id,body.stack);
  const added=[{id:ids.hardware,operation:{fastened_joint:body}},
    ...body.stack.map((part,i)=>({id:ids.holes[i],operation:{joint_holes:{joint:ids.hardware,input:part}}})),
    {id:ids.result,operation:{compound:{inputs:[...ids.holes,ids.hardware]}}}];
  const taken=added.find(change=>features.some(f=>f.id===change.id));
  if(taken)throw Error(`Feature '${taken.id}' already exists; choose another joint name.`);
  return {action:'add',features:added};
}

/** One line per reported joint: the screw chosen and the parts list. */
export function jointSummary(reports) {
  return (reports??[]).map(r=>{
    const s=r.screw,round=v=>Number(Number(v).toFixed(3));
    const fit=s.chosen?`shortest standard length (needs ${round(s.minimum_length_mm)} mm)`:`entered length (needs ${round(s.minimum_length_mm)} mm)`;
    const engaged=r.thread_engagement_mm.min===r.thread_engagement_mm.max?`${round(r.thread_engagement_mm.min)} mm`:`${round(r.thread_engagement_mm.min)}–${round(r.thread_engagement_mm.max)} mm`;
    return `${r.feature}: ${r.positions} × M${round(s.nominal_diameter_mm)} × ${round(s.length_mm)}, ${fit}; thread engaged ${engaged}. Parts: ${r.hardware.map(h=>`${h.quantity} × ${h.designation}`).join(', ')}.`;
  });
}

/** Renders the joint controls with the editor's row helpers. */
export function renderJoint({state,catalog,features,parameters,select,number,button,content,render,invalidate,message,pickPosition,cancelPick,checkpoint}) {
  const note=document.createElement('p');note.textContent='Screws go through the stacked solids in order, head side first, along one axis. The engine measures the stack at every screw, picks the screw length, and drills each solid to match. Preview lists the screw and parts.';content.append(note);
  const solids=[['','Choose solid'],...features.map(f=>[f.id,f.id])];
  for(const [i,part] of state.stack.entries()){
    select(i===0?'Head-side solid':`Stacked solid ${i+1}`,solids,part,value=>{state.stack[i]=value;render();});
    if(state.stack.length>1)content.append(button(`Remove stacked solid ${i+1}`,()=>{invalidate();state.stack.splice(i,1);render();}));
  }
  const add=button('Add stacked solid',()=>{invalidate();state.stack.push('');render();});add.disabled=state.stack.length>=MAX_JOINT_STACK;content.append(add);
  select('Held by',[['nut','Nut behind the last solid'],['tapped','Tapped hole in the last solid']],state.fastening,value=>{state.fastening=value;render();});
  select('Screw head',[['surface','On the surface'],['counterbored','Counterbored flush']],state.head,value=>{state.head=value;render();});
  select('Washers',[['yes','Plain washers'],['no','No washers']],state.washers?'yes':'no',value=>{state.washers=value==='yes';render();});
  const screw=catalog?.parts?.find(p=>p.part===SCREW);
  const sizes=[...(screw?.sizes??[]).map(s=>[String(s),`M${s}`]),...parameters.filter(p=>p.parameter_type?.scalar==='length').map(p=>[`parameter:${p.id}`,`Parameter: ${p.id}`])];
  if(state.savedSize)sizes.push(['preserve','Keep existing size formula']);
  select('Screw size',sizes,state.savedSize?'preserve':state.size,value=>{if(value!=='preserve'){state.savedSize=null;state.size=value;}render();});
  const standard=catalog?.screw_lengths?.[state.size]??[];
  const lengths=[['auto','Shortest standard length that fits'],...standard.map(l=>[String(l),`${l} mm`])];
  if(state.length!=='auto'&&state.length!=='preserve'&&!lengths.some(([key])=>key===state.length))lengths.push([state.length,`${state.length} mm`]);
  if(state.savedLength)lengths.push(['preserve','Keep existing length formula']);
  select('Screw length',lengths,state.length,value=>{state.length=value;render();});
  if(state.size.startsWith('parameter:')||!standard.length){const n=document.createElement('p');n.textContent=standard.length?'Standard lengths follow the entered size.':'No standard lengths are tabulated for this size; choose Keep or enter one below.';content.append(n);}
  number('Entered screw length (mm, blank for automatic)',state.length!=='auto'&&state.length!=='preserve'?Number(state.length):'',value=>{state.length=Number.isFinite(value)?String(value):'auto';});
  select('Clearance holes',[['fine','Fine (ISO 273)'],['medium','Medium (ISO 273)'],['coarse','Coarse (ISO 273)']],state.clearance,value=>state.clearance=value);
  if(state.fastening==='tapped')number('Least thread engagement (mm, blank for 1.5 × size)',Number.isFinite(state.engagement)?state.engagement:'',value=>state.engagement=value).disabled=Boolean(state.savedEngagement);
  const heading=document.createElement('h4');heading.textContent=state.savedPositions?'Screw positions (kept formulas)':`Screw positions (${state.positions.length})`;content.append(heading);
  if(pickPosition)content.append(button('Add screw position on face in canvas',async()=>{
    const head=state.stack[0];if(!head)throw Error('Choose the head-side solid first.');
    invalidate();const valid=checkpoint();message('Click where a screw enters the head-side solid. Escape cancels.');
    const picked=await pickPosition(head);if(!valid())return;
    if(!picked){message('Screw placement canceled. Draft retained.');return;}
    if(picked.target!==head||!picked.position_mm?.every?.(Number.isFinite)||!picked.axis?.every?.(Number.isFinite))throw Error('The picked point is not on the head-side solid.');
    const axis=picked.axis.map(v=>v/Math.hypot(...picked.axis));
    if(state.positions.length&&!state.savedAxis){
      const current=state.axis.map(v=>v/Math.hypot(...state.axis));
      if(axis.reduce((sum,v,i)=>sum+v*current[i],0)<1-1e-6)throw Error('Every screw in a joint shares one axis; pick a face facing the same way as the first.');
    }
    if(!state.positions.length){state.axis=axis;state.savedAxis=null;}
    if(state.savedPositions){state.savedPositions=null;state.positions=[];}
    state.positions.push([...picked.position_mm]);render();
    message(`Screw ${state.positions.length} placed. The axis points into the face. Preview checks the joint on every instance.`);
  }),button('Cancel screw placement',()=>{invalidate();cancelPick();message('Screw placement canceled. Draft retained.');}));
  if(!state.savedPositions){
    for(const [i,p] of state.positions.entries()){
      for(const [k,key] of ['X','Y','Z'].entries())number(`Screw ${i+1} ${key} (mm)`,Number.isFinite(p[k])?p[k]:'',value=>p[k]=value);
      content.append(button(`Remove screw ${i+1}`,()=>{invalidate();state.positions.splice(i,1);render();}));
    }
    const typed=button('Add screw position',()=>{invalidate();state.positions.push([...(state.positions.at(-1)??[0,0,0])]);render();});typed.disabled=state.positions.length>=MAX_JOINT_POSITIONS;content.append(typed);
  }else content.append(button('Replace kept positions',()=>{invalidate();state.savedPositions=null;state.positions=[];render();}));
  if(!state.savedAxis)for(const [k,key] of ['X','Y','Z'].entries())number(`Screw axis ${key}`,state.axis[k],value=>state.axis[k]=value);
  else content.append(button('Replace kept axis',()=>{invalidate();state.savedAxis=null;render();}));
}
