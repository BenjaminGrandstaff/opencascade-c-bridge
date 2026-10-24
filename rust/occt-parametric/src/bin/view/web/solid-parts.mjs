// Catalog standard parts in the solid editor: an ISO screw, nut, washer or
// dowel pin from the engine's frozen catalog, seated on a picked face. Pure
// state helpers plus one renderer that uses the editor's row helpers.
const length = value => ({literal:{value,dimension:'length',unit:'millimeter'}});
const mm = q => q?.dimension==='length' ? q.value*({millimeter:1,centimeter:10,meter:1000,inch:25.4}[q.unit]??NaN) : NaN;
const vector = (values,dimension) => ({components:Object.fromEntries(['x','y','z'].map((key,i)=>[key,{literal:{value:values[i],dimension,unit:dimension==='length'?'millimeter':null}}]))});
const readVector = (value,dimension) => {
  const values=['x','y','z'].map(key=>value?.literal?.[key]??value?.components?.[key]?.literal);
  return values.every(v=>v?.dimension===dimension)?values.map(v=>dimension==='length'?mm(v):v.value):null;
};
/** Display names of the catalog parts. */
export const PART_NAMES = {
  socket_head_cap_screw_iso_4762:'Socket head cap screw (ISO 4762)',
  hex_nut_iso_4032:'Hex nut (ISO 4032)',
  plain_washer_iso_7089:'Plain washer (ISO 7089)',
  dowel_pin_iso_8734:'Dowel pin (ISO 8734)',
};
const prefix = part => part.startsWith('dowel')?'Ø':'M';

/** Editor state for a saved standard_part body, or a new screw. */
export function readPart(body) {
  const origin=body?readVector(body.origin,'length'):[0,0,0],axis=body?readVector(body.axis,'scalar'):[0,0,-1];
  const size=body?.nominal_diameter,partLength=body?.length;
  return {
    part:body?.part??'socket_head_cap_screw_iso_4762',
    size:size?.parameter?`parameter:${size.parameter}`:size?.literal?String(mm(size.literal)):'6',savedSize:size&&!size.parameter&&!size.literal?structuredClone(size):null,
    length:partLength?.literal?mm(partLength.literal):20,lengthSource:partLength?.parameter?`parameter:${partLength.parameter}`:partLength&&!partLength.literal?'preserve':'literal',savedLength:partLength&&!partLength.literal?structuredClone(partLength):null,
    origin:origin??[0,0,0],axis:axis??[0,0,-1],savedPlacement:body&&(!origin||!axis)?{origin:structuredClone(body.origin),axis:structuredClone(body.axis)}:null,
  };
}

/** The standard_part body for `state` against the catalog. */
export function partOperation(state,catalog) {
  const entry=catalog?.parts?.find(p=>p.part===state.part);
  if(!entry)throw Error('Choose a catalog part.');
  const size=state.savedSize??(state.size.startsWith('parameter:')?{parameter:state.size.slice(10)}:length(Number(state.size)));
  if(size.literal&&!entry.sizes.includes(size.literal.value))throw Error(`Choose one of the catalog sizes: ${entry.sizes.map(s=>prefix(state.part)+s).join(', ')}.`);
  const body={part:state.part,nominal_diameter:structuredClone(size)};
  if(entry.takes_length){
    if(state.lengthSource==='preserve')body.length=structuredClone(state.savedLength);
    else if(state.lengthSource.startsWith('parameter:'))body.length={parameter:state.lengthSource.slice(10)};
    else{if(!(state.length>0))throw Error('Enter a positive part length.');body.length=length(state.length);}
  }
  if(state.savedPlacement)Object.assign(body,structuredClone(state.savedPlacement));
  else{
    if(!state.origin.every(Number.isFinite))throw Error('Enter a finite seat position.');
    if(!state.axis.every(Number.isFinite)||Math.hypot(...state.axis)===0)throw Error('Enter a finite, nonzero part axis.');
    body.origin=vector(state.origin,'length');body.axis=vector(state.axis,'scalar');
  }
  return body;
}

/** Renders the part controls with the editor's row helpers. */
export function renderPart({state,catalog,parameters,select,number,button,content,render,invalidate,message,pickPosition,cancelPick,checkpoint,target}) {
  const note=document.createElement('p');note.textContent='The seat is the underside of a screw head, or the bearing face of a nut, washer or pin end. The part extends along its axis, a screw head behind the seat. Threads are not modeled. Group with a solid to see both together.';content.append(note);
  const parts=catalog?.parts??[];
  select('Catalog part',parts.map(p=>[p.part,PART_NAMES[p.part]??p.part]),state.part,value=>{state.part=value;const sizes=parts.find(p=>p.part===value)?.sizes??[];if(!state.size.startsWith('parameter:')&&!sizes.includes(Number(state.size)))state.size=String(sizes[Math.floor(sizes.length/2)]??'');render();});
  const entry=parts.find(p=>p.part===state.part);
  const sizes=[...(entry?.sizes??[]).map(s=>[String(s),`${prefix(state.part)}${s}`]),...parameters.filter(p=>p.parameter_type?.scalar==='length').map(p=>[`parameter:${p.id}`,`Parameter: ${p.id}`])];
  if(state.savedSize)sizes.push(['preserve','Keep existing size formula']);
  select('Part size',sizes,state.savedSize?'preserve':state.size,value=>{if(value!=='preserve'){state.savedSize=null;state.size=value;}render();});
  if(entry?.takes_length){
    const choices=[['literal','Entered length'],...parameters.filter(p=>p.parameter_type?.scalar==='length').map(p=>[`parameter:${p.id}`,`Parameter: ${p.id}`])];
    if(state.savedLength)choices.push(['preserve','Keep existing length formula']);
    select('Part length source',choices,state.lengthSource,value=>{state.lengthSource=value;render();});
    number(state.part.startsWith('socket')?'Length under the head (mm)':'Overall length (mm)',state.length,value=>state.length=value).disabled=state.lengthSource!=='literal';
  }
  if(pickPosition)content.append(button('Seat part on face in canvas',async()=>{
    if(!target())throw Error('Choose a solid to seat the part on first.');
    invalidate();const valid=checkpoint(),solid=target();message('Click the seat on a planar face. Escape cancels.');
    const picked=await pickPosition(solid);if(!valid())return;
    if(!picked){message('Seat placement canceled. Draft retained.');return;}
    if(picked.target!==solid||!picked.position_mm?.every?.(Number.isFinite)||!picked.axis?.every?.(Number.isFinite))throw Error('The picked seat is not on the chosen solid.');
    state.origin=[...picked.position_mm];
    // Screws and pins go into the face; nuts and washers sit on it.
    const into=state.part.startsWith('socket')||state.part.startsWith('dowel');
    state.axis=picked.axis.map(v=>into?v:-v);state.savedPlacement=null;render();
    message(into?'Seated on the face, pointing into it.':'Seated on the face, extending away from it.');
  }),button('Cancel seat placement',()=>{invalidate();cancelPick();message('Seat placement canceled. Draft retained.');}));
  if(state.savedPlacement)content.append(button('Replace kept seat and axis',()=>{invalidate();state.savedPlacement=null;render();}));
  else{
    for(const [k,key] of ['X','Y','Z'].entries())number(`Seat ${key} (mm)`,state.origin[k],value=>state.origin[k]=value);
    for(const [k,key] of ['X','Y','Z'].entries())number(`Part axis ${key}`,state.axis[k],value=>state.axis[k]=value);
  }
}
