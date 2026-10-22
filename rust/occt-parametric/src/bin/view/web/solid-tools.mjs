// Shell, draft, direct face edit, mirror and pattern features in the solid
// editor: the engine's hollow, draft, remove_faces, offset_faces,
// move_faces, mirror, linear_pattern, circular_pattern, curve_pattern,
// fill_pattern and table_pattern operations on an accepted solid. Pure state helpers plus one renderer
// that uses the editor's own select/number/button rows.
const quantity = (value,dimension='length',unit='millimeter') => ({literal:{value,dimension,unit}});
const vector = (values,dimension) => ({components:Object.fromEntries(['x','y','z'].map((key,i)=>[key,quantity(values[i],dimension,dimension==='length'?'millimeter':null)]))});
const mm = q => q?.dimension==='length' ? q.value*({millimeter:1,centimeter:10,meter:1000,inch:25.4}[q.unit]??NaN) : NaN;
const DEGREE = Math.PI/180;

/** Tool kinds, their names, and whether they change their solid in place
 * (shell, draft, face edits) rather than making a new body that can join a
 * target. */
export const SOLID_TOOLS = {
  hollow:{name:'shell',inPlace:true},
  draft:{name:'draft',inPlace:true},
  remove_faces:{name:'delete faces',inPlace:true},
  offset_faces:{name:'offset faces',inPlace:true},
  move_faces:{name:'move faces',inPlace:true},
  mirror:{name:'mirror',inPlace:false},
  linear_pattern:{name:'linear pattern',inPlace:false},
  circular_pattern:{name:'circular pattern',inPlace:false},
  curve_pattern:{name:'curve pattern',inPlace:false},
  fill_pattern:{name:'fill pattern',inPlace:false},
  table_pattern:{name:'table pattern',inPlace:false},
};
export const isSolidTool = kind => Object.hasOwn(SOLID_TOOLS,kind);
export const inPlaceTool = kind => SOLID_TOOLS[kind]?.inPlace===true;
/** Direct face edits pick curved faces too and work without history. */
const FACE_EDITS = ['remove_faces','offset_faces','move_faces'];
const picksFaces = kind => kind==='hollow'||kind==='draft'||FACE_EDITS.includes(kind);
export const solidToolKind = operation => Object.keys(SOLID_TOOLS).find(kind=>Object.hasOwn(operation??{},kind))??null;

function readVector(value,dimension) {
  const values=['x','y','z'].map(key=>value?.literal?.[key]??value?.components?.[key]?.literal);
  return values.every(v=>v?.dimension===dimension)?values.map(v=>dimension==='length'?mm(v):v.value):null;
}
/** An entered vector, or a kept formula when the saved one is not plain. */
function vectorState(saved,dimension,fallback) {
  const values=saved?readVector(saved,dimension):null;
  // An unchanged entered vector writes back exactly as it was saved.
  return {values:values??[...fallback],saved:saved&&!values?saved:null,original:values?{expression:saved,values:[...values]}:null};
}
/** An entered value, a parameter, or a kept formula. */
function scalarState(saved,read,fallback) {
  if(!saved)return {source:'literal',value:fallback,saved:null};
  if(saved.parameter)return {source:`parameter:${saved.parameter}`,value:fallback,saved:null};
  const value=saved.literal?read(saved.literal):NaN;
  return Number.isFinite(value)?{source:'literal',value,saved:null,original:{expression:saved,value}}:{source:'preserve',value:fallback,saved};
}

/** Editor state for a tool, from its saved operation body or for a new one
 * on `source`. */
export function readSolidTool(kind,body,source='') {
  const state={source:body?.input??source};
  if(kind==='hollow'){
    const thickness=body?.thickness;
    const inside=thickness?.negate!==undefined||(thickness?.literal&&thickness.literal.value<0);
    const magnitude=thickness?.negate??thickness;
    Object.assign(state,{faces:structuredClone(body?.faces??[]),picked:[],side:inside||!body?'inside':'outside',
      thickness:scalarState(magnitude?.literal?{literal:{...magnitude.literal,value:Math.abs(magnitude.literal.value)}}:magnitude,mm,2),
      tolerance:structuredClone(body?.tolerance??quantity(0.001))});
  }else if(kind==='draft'){
    Object.assign(state,{faces:structuredClone(body?.faces??[]),picked:[],
      origin:vectorState(body?.neutral_origin,'length',[0,0,0]),normal:vectorState(body?.neutral_normal,'scalar',[0,0,1]),
      pull:vectorState(body?.pull_direction,'scalar',[0,0,1]),
      angle:scalarState(body?.angle_radians,q=>q.dimension==='scalar'?q.value/DEGREE:NaN,3)});
  }else if(FACE_EDITS.includes(kind)){
    state.faces=structuredClone(body?.faces??[]);state.picked=[];
    if(kind==='offset_faces')Object.assign(state,{distance:scalarState(body?.distance,mm,1),tolerance:structuredClone(body?.tolerance??quantity(0.000001))});
    if(kind==='move_faces')state.offset=vectorState(body?.offset,'length',[5,0,0]);
  }else if(kind==='mirror'){
    Object.assign(state,{origin:vectorState(body?.origin,'length',[0,0,0]),normal:vectorState(body?.normal,'scalar',[1,0,0])});
  }else if(kind==='curve_pattern'){
    Object.assign(state,{path:body?.path??'',count:scalarState(body?.count,q=>q.value,5),orientation:body?.orientation??'fixed'});
  }else if(kind==='fill_pattern'){
    Object.assign(state,{region:body?.region??'',face:structuredClone(body?.face??null),anchor:vectorState(body?.anchor,'length',[0,0,0]),
      direction:vectorState(body?.direction,'scalar',[1,0,0]),spacing:scalarState(body?.spacing,mm,10),row:scalarState(body?.row_spacing,mm,10),
      layout:body?.layout??'grid',margin:scalarState(body?.margin,mm,2)});
  }else if(kind==='table_pattern'){
    const rows=(body?.offsets??[[0,0,0],[10,0,0]].map(v=>vector(v,'length'))).map(o=>readVector(o,'length'));
    // Offsets that are not plain entered vectors are kept as saved.
    state.offsets=rows.every(Boolean)?{text:rows.map(r=>r.join(', ')).join('\n'),saved:null}:{text:'',saved:structuredClone(body.offsets)};
  }else if(kind==='linear_pattern'){
    const step=readVector(body?.step,'length');const spacing=step?Math.hypot(...step):10;
    Object.assign(state,{direction:step&&spacing>0?step.map(v=>v/spacing):[1,0,0],spacing:{source:body&&!step?'preserve':'literal',value:spacing,saved:body&&!step?body.step:null},
      count:scalarState(body?.count,q=>q.value,3)});
  }else if(kind==='circular_pattern'){
    const step=body?.angle_step_radians;
    Object.assign(state,{origin:vectorState(body?.origin,'length',[0,0,0]),axis:vectorState(body?.axis,'scalar',[0,0,1]),
      count:scalarState(body?.count,q=>q.value,6),spacing:body&&!step?.literal?'step':body?'step':'full',
      angle:scalarState(step,q=>q.dimension==='scalar'?q.value/DEGREE:NaN,60)});
  }
  if(['linear_pattern','circular_pattern','curve_pattern'].includes(kind))state.skip=(body?.skip??[]).join(', ');
  return state;
}

/** Copy indices to skip from "1, 3, 7"; 0 is the original. */
function skipList(text,count){
  const items=(text??'').split(/[\s,]+/).filter(Boolean);
  const skip=items.map(Number);
  if(skip.some(v=>!Number.isInteger(v)||v<0))throw Error('Skip copies takes whole numbers from 0 (the original), separated by commas.');
  if(Number.isInteger(count)&&skip.some(v=>v>=count))throw Error(`Skip copies must be below the copy count (${count}).`);
  return [...new Set(skip)].sort((a,b)=>a-b);
}
const withSkip=(operation,state,count)=>{const skip=skipList(state.skip,count?.literal?.value);if(skip.length)operation.skip=skip;return operation;};

const finite = (values,what) => {if(values.length!==3||!values.every(Number.isFinite))throw Error(`Enter a finite ${what}.`);return values;};
const direction = (values,what) => {finite(values,what);const m=Math.hypot(...values);if(!(m>0))throw Error(`Enter a nonzero ${what}.`);return values.map(v=>v/m);};
const keptVector = (state,dimension,what,unit=false) => {
  if(state.saved)return structuredClone(state.saved);
  if(state.original&&state.original.values.every((v,i)=>v===state.values[i]))return structuredClone(state.original.expression);
  return vector(unit?direction(state.values,what):finite(state.values,what),dimension);
};
function scalar(state,what,entered,check) {
  if(state.source==='preserve')return structuredClone(state.saved);
  if(state.source.startsWith('parameter:'))return {parameter:state.source.slice(10)};
  if(!Number.isFinite(state.value)||!check(state.value))throw Error(`Enter a valid ${what}.`);
  if(state.original&&state.original.value===state.value)return structuredClone(state.original.expression);
  return entered(state.value);
}

/** The operation for a tool's state, checked as the engine checks it so
 * mistakes read clearly before Preview. */
export function solidToolOperation(kind,state,features) {
  if(!features.some(f=>f.id===state.source))throw Error('Choose an existing source solid.');
  const input=state.source;
  if(kind==='hollow'){
    if(!state.faces.length)throw Error('Pick at least one face to open the shell.');
    const magnitude=scalar(state.thickness,'wall thickness',v=>quantity(v),v=>v>0);
    // Negative thickness keeps the walls inside the original surface.
    const thickness=state.side==='inside'?(magnitude.literal?quantity(-magnitude.literal.value,magnitude.literal.dimension,magnitude.literal.unit):{negate:magnitude}):magnitude;
    return {hollow:{input,faces:structuredClone(state.faces),thickness,tolerance:structuredClone(state.tolerance)}};
  }
  if(kind==='draft'){
    if(!state.faces.length)throw Error('Pick at least one face to draft.');
    return {draft:{input,faces:structuredClone(state.faces),neutral_origin:keptVector(state.origin,'length','neutral plane point'),
      neutral_normal:keptVector(state.normal,'scalar','neutral plane normal',true),pull_direction:keptVector(state.pull,'scalar','pull direction',true),
      angle_radians:scalar(state.angle,'draft angle (between −90° and 90°, not 0)',v=>quantity(v*DEGREE,'scalar',null),v=>v!==0&&Math.abs(v)<90)}};
  }
  if(FACE_EDITS.includes(kind)){
    if(!state.faces.length)throw Error('Pick at least one face to edit.');
    const faces=structuredClone(state.faces);
    if(kind==='remove_faces')return {remove_faces:{input,faces}};
    if(kind==='offset_faces')return {offset_faces:{input,faces,distance:scalar(state.distance,'offset distance (nonzero; positive adds material)',v=>quantity(v),v=>v!==0),tolerance:structuredClone(state.tolerance)}};
    const offset=keptVector(state.offset,'length','move offset');
    if(offset.components&&['x','y','z'].every(key=>offset.components[key].literal.value===0))throw Error('Enter a nonzero move offset.');
    return {move_faces:{input,faces,offset}};
  }
  if(kind==='mirror')return {mirror:{input,origin:keptVector(state.origin,'length','mirror plane point'),normal:keptVector(state.normal,'scalar','mirror plane normal',true)}};
  if(kind==='table_pattern'){
    if(state.offsets.saved)return {table_pattern:{input,offsets:structuredClone(state.offsets.saved)}};
    const rows=state.offsets.text.split('\n').map(l=>l.trim()).filter(Boolean);
    if(!rows.length||rows.length>10000)throw Error('Enter 1 to 10000 offsets, one "x, y, z" (mm) per line.');
    const offsets=rows.map((row,i)=>{const v=row.split(/[\s,]+/).filter(Boolean).map(Number);if(v.length!==3||!v.every(Number.isFinite))throw Error(`Offset line ${i+1} needs three numbers: x, y, z in mm.`);return vector(v,'length');});
    return {table_pattern:{input,offsets}};
  }
  if(kind==='fill_pattern'){
    if(!features.some(f=>f.id===state.region))throw Error('Choose the region to fill: a sketch face, or a solid with a picked face.');
    const operation={input,region:state.region,anchor:keptVector(state.anchor,'length','site point on the source'),
      direction:keptVector(state.direction,'scalar','row direction',true),
      spacing:scalar(state.spacing,'spacing along rows',v=>quantity(v),v=>v>0),row_spacing:scalar(state.row,'row spacing',v=>quantity(v),v=>v>0),
      layout:state.layout,margin:scalar(state.margin,'margin from the edge (zero or more)',v=>quantity(v),v=>v>=0)};
    if(state.face)operation.face=structuredClone(state.face);
    return {fill_pattern:operation};
  }
  const count=scalar(state.count,'copy count (a whole number from 2 to 10000, including the original)',v=>quantity(v,'scalar',null),v=>Number.isInteger(v)&&v>=2&&v<=10000);
  if(kind==='linear_pattern'){
    let step;
    if(state.spacing.source==='preserve')step=structuredClone(state.spacing.saved);
    else{
      const unit=direction(state.direction,'pattern direction');
      if(!(state.spacing.value>0))throw Error('Enter a positive spacing.');
      step=vector(unit.map(v=>v*state.spacing.value),'length');
    }
    return {linear_pattern:withSkip({input,step,count},state,count)};
  }
  if(kind==='curve_pattern'){
    if(!features.some(f=>f.id===state.path))throw Error('Choose the path: a sketch wire or open wire to copy along.');
    return {curve_pattern:withSkip({input,path:state.path,count,orientation:state.orientation},state,count)};
  }
  let angle;
  if(state.spacing==='full'){
    if(!count.literal)throw Error('An even spread round a full turn needs an entered count.');
    angle=quantity(2*Math.PI/count.literal.value,'scalar',null);
  }else angle=scalar(state.angle,'angle between copies',v=>quantity(v*DEGREE,'scalar',null),v=>v!==0&&Math.abs(v)<=360);
  return {circular_pattern:withSkip({input,origin:keptVector(state.origin,'length','axis point'),axis:keptVector(state.axis,'scalar','axis direction',true),count,angle_step_radians:angle},state,count)};
}

/** The tool's rows after the editor's name row. */
export function renderSolidTool({kind,state,features,allFeatures=features,parameters,select,number,button,content,render,pickFace,message,invalidate}) {
  const note=document.createElement('p');
  note.textContent={
    hollow:'Removes the picked faces and leaves walls of the given thickness, inside or outside the original surface. Pick planar faces of the source; Preview checks every instance.',
    draft:'Tapers the picked faces about the neutral plane. Positive angles remove material on the pull-direction side. Vectors use family coordinates.',
    remove_faces:'Deletes the picked faces and closes the gap from the faces around them, so a hole, pocket, boss, fillet or chamfer disappears without editing the feature that made it. Pick every face of the detail.',
    offset_faces:'Moves the picked faces along their outward normals; positive adds material, and on a hole wall a negative distance widens the hole. The faces around them extend or trim to meet.',
    move_faces:'Moves the detail the picked faces bound, such as a hole, pocket or boss, by the offset in family coordinates. Pick every face of the detail.',
    mirror:'Reflects the source across the plane. Choose Add material with the source as target to make a symmetric part.',
    linear_pattern:'Copies the source along the direction at the spacing; the count includes the original. Add material with the source as target to join the copies.',
    circular_pattern:'Copies the source about the axis; the count includes the original. Add material with the source as target to join the copies.',
    curve_pattern:'Copies the source evenly along a sketch wire, from its start to its end (or evenly round a closed one). Model the source at the start of the path. Turning copies follow the path; fixed ones only move.',
    fill_pattern:'Copies the source to the sites of a grid that lie inside a planar region, at least the margin from its edges and holes. The grid passes through the site point; rows run along the direction.',
    table_pattern:'Copies the source by each listed offset, one "x, y, z" in mm per line. Include 0, 0, 0 to keep the original place. Cut with the copies to make uneven hole layouts.',
  }[kind];
  content.append(note);
  const solids=features.map(f=>[f.id,f.id]);
  select('Source solid',[['','Choose solid'],...solids],state.source,value=>{if(state.source!==value&&state.faces){state.faces=[];state.picked=[];}state.source=value;render();});
  const scalars=parameters.filter(p=>p.parameter_type?.scalar==='scalar'),lengths=parameters.filter(p=>p.parameter_type?.scalar==='length');
  const sources=(option,entered,list,unit)=>{
    const choices=[['literal',entered],...list.map(p=>[`parameter:${p.id}`,`Parameter${unit}: ${p.id}`])];
    if(option.source.startsWith('parameter:')&&!choices.some(([key])=>key===option.source))choices.push([option.source,`Existing parameter: ${option.source.slice(10)}`]);
    if(option.source==='preserve')choices.push(['preserve','Keep existing formula']);
    return choices;
  };
  const vectorRows=(label,v,unit)=>{
    if(v.saved){const kept=document.createElement('p');kept.textContent=`${label} keeps its formula.`;content.append(kept);content.append(button(`Enter ${label.toLowerCase()} instead`,()=>{v.saved=null;invalidate();render();}));return;}
    for(const [i,key] of ['X','Y','Z'].entries())number(`${label} ${key}${unit}`,v.values[i],value=>v.values[i]=value);
  };
  if(picksFaces(kind)){
    state.faces.forEach((_,index)=>{
      const row=document.createElement('p');
      const picked=state.picked[index];
      row.textContent=`Face ${index+1}: `+(picked?`picked ${picked.curved?'curved face centred':'planar face'} at ${(picked.origin_mm??[]).map(v=>Number(v.toPrecision(6))).join(', ')} mm`:'saved selector');
      content.append(row,button(`Remove face ${index+1}`,()=>{state.faces.splice(index,1);state.picked.splice(index,1);invalidate();render();}));
    });
    const curved=FACE_EDITS.includes(kind);
    if(pickFace)content.append(button(kind==='hollow'?'Pick a face to open':kind==='draft'?'Pick a face to draft':'Pick a face to edit',async()=>{
      if(!state.source)throw Error('Choose a source solid first.');
      message(curved?'Click a face of the amber source solid; curved faces work too. Escape cancels the pick.':'Click a planar face of the amber source solid. Escape cancels the pick.');
      const picked=curved?await pickFace(state.source,{curved:true}):await pickFace(state.source);
      if(!picked)return;
      state.faces.push(structuredClone(picked.face));state.picked[state.faces.length-1]=picked;invalidate();render();
      message(`${state.faces.length} face(s) selected. Preview checks every instance.`);
    }));
  }
  if(kind==='hollow'){
    select('Wall thickness source',sources(state.thickness,'Entered thickness',lengths,''),state.thickness.source,value=>{state.thickness.source=value;render();});
    number('Wall thickness (mm)',state.thickness.value,value=>state.thickness.value=value).disabled=state.thickness.source!=='literal';
    select('Walls',[['inside','Inside the original surface'],['outside','Outside the original surface']],state.side,value=>state.side=value);
  }else if(kind==='draft'){
    vectorRows('Neutral plane point',state.origin,' (mm)');
    vectorRows('Neutral plane normal',state.normal,'');
    vectorRows('Pull direction',state.pull,'');
    select('Draft angle source',sources(state.angle,'Entered angle',scalars,' (radians)'),state.angle.source,value=>{state.angle.source=value;render();});
    number('Draft angle (degrees)',state.angle.value,value=>state.angle.value=value).disabled=state.angle.source!=='literal';
  }else if(kind==='offset_faces'){
    select('Distance source',sources(state.distance,'Entered distance',lengths,''),state.distance.source,value=>{state.distance.source=value;render();});
    number('Distance (mm, positive adds material)',state.distance.value,value=>state.distance.value=value).disabled=state.distance.source!=='literal';
  }else if(kind==='move_faces'){
    vectorRows('Move offset',state.offset,' (mm)');
  }else if(kind==='remove_faces'){
    // Faces only.
  }else if(kind==='mirror'){
    const principal={yz:[1,0,0],xz:[0,1,0],xy:[0,0,1]};
    const current=!state.normal.saved&&Object.entries(principal).find(([,n])=>n.every((v,i)=>v===state.normal.values[i]))?.[0];
    select('Mirror plane',[['yz','YZ (normal X)'],['xz','XZ (normal Y)'],['xy','XY (normal Z)'],['custom','Custom normal']],current||'custom',value=>{if(principal[value]){state.normal={values:[...principal[value]],saved:null};}else if(state.normal.saved===null&&current)state.normal.values=[1,1,0];render();});
    vectorRows('Plane point',state.origin,' (mm)');
    if(!current)vectorRows('Plane normal',state.normal,'');
  }else if(kind==='table_pattern'){
    if(state.offsets.saved){const kept=document.createElement('p');kept.textContent='Offsets keep their formulas.';content.append(kept,button('Enter offsets instead',()=>{state.offsets={text:'0, 0, 0',saved:null};invalidate();render();}));}
    else{
      const row=document.createElement('label');row.textContent='Offsets (x, y, z in mm, one per line)';
      const area=document.createElement('textarea');area.rows=6;area.value=state.offsets.text;area.setAttribute('aria-label','Offsets (x, y, z in mm, one per line)');
      area.oninput=()=>{state.offsets.text=area.value;invalidate();};row.append(area);content.append(row);
    }
  }else if(kind==='fill_pattern'){
    select('Region to fill',[['','Choose region'],...allFeatures.map(f=>[f.id,f.id])],state.region,value=>{if(state.region!==value)state.face=null;state.region=value;render();});
    if(state.face){const kept=document.createElement('p');kept.textContent='Fills the picked face of the region.';content.append(kept,button('Use the region\'s only face instead',()=>{state.face=null;invalidate();render();}));}
    if(pickFace)content.append(button('Pick the face to fill',async()=>{
      if(!state.region)throw Error('Choose the region first.');
      message('Click a planar face of the amber region. Escape cancels the pick.');
      const picked=await pickFace(state.region);
      if(!picked)return;
      state.face=structuredClone(picked.face);invalidate();render();
    }));
    vectorRows('Site point on the source',state.anchor,' (mm)');
    vectorRows('Row direction',state.direction,'');
    for(const [label,option] of [['Spacing along rows (mm)',state.spacing],['Row spacing (mm)',state.row],['Margin from the edge (mm)',state.margin]]){
      select(`${label.replace(' (mm)','')} source`,sources(option,'Entered length',lengths,''),option.source,value=>{option.source=value;render();});
      number(label,option.value,value=>option.value=value).disabled=option.source!=='literal';
    }
    select('Layout',[['grid','Grid'],['staggered','Staggered rows']],state.layout,value=>state.layout=value);
  }else{
    select('Copy count source',sources(state.count,'Entered count',scalars,''),state.count.source,value=>{state.count.source=value;render();});
    number('Copies (including the original)',state.count.value,value=>state.count.value=value).disabled=state.count.source!=='literal';
    const skip=document.createElement('label');skip.textContent='Skip copies (0 is the original)';
    const skipInput=document.createElement('input');skipInput.value=state.skip??'';skipInput.placeholder='for example 2, 5';skipInput.setAttribute('aria-label','Skip copies (0 is the original)');
    skipInput.oninput=()=>{state.skip=skipInput.value;invalidate();};skip.append(skipInput);content.append(skip);
    if(kind==='curve_pattern'){
      select('Path',[['','Choose path'],...allFeatures.map(f=>[f.id,f.id])],state.path,value=>state.path=value);
      select('Copies',[['fixed','Keep their orientation'],['tangent','Turn with the path']],state.orientation,value=>state.orientation=value);
    }else if(kind==='linear_pattern'){
      if(state.spacing.source==='preserve'){const kept=document.createElement('p');kept.textContent='Step keeps its formula.';content.append(kept,button('Enter direction and spacing instead',()=>{state.spacing.source='literal';invalidate();render();}));}
      else{
        for(const [i,key] of ['X','Y','Z'].entries())number(`Direction ${key}`,state.direction[i],value=>state.direction[i]=value);
        number('Spacing (mm)',state.spacing.value,value=>state.spacing.value=value);
      }
    }else{
      vectorRows('Axis point',state.origin,' (mm)');
      vectorRows('Axis direction',state.axis,'');
      select('Angular spacing',[['full','Even round a full turn'],['step','Angle between copies']],state.spacing,value=>{state.spacing=value;render();});
      if(state.spacing==='step'){
        select('Angle source',sources(state.angle,'Entered angle',scalars,' (radians)'),state.angle.source,value=>{state.angle.source=value;render();});
        number('Angle between copies (degrees)',state.angle.value,value=>state.angle.value=value).disabled=state.angle.source!=='literal';
      }
    }
  }
}
