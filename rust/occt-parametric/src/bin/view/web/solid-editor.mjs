const quantity = (value,dimension='length',unit='millimeter') => ({literal:{value,dimension,unit}});
const mm = q => q?.dimension==='length' ? q.value*({millimeter:1,centimeter:10,meter:1000,inch:25.4}[q.unit]??NaN) : NaN;
export function extrusionDirection(axis,depth) {
  const magnitude=Math.hypot(...axis);
  if(axis.length!==3||!axis.every(Number.isFinite)||!Number.isFinite(magnitude)||magnitude===0)throw new Error('Enter a finite, nonzero direction.');
  if(depth?.literal&&(!Number.isFinite(mm(depth.literal))||mm(depth.literal)<=0))throw new Error('Depth must be a positive length.');
  return {components:Object.fromEntries(['x','y','z'].map((key,i)=>{
    const factor=axis[i]/magnitude;
    return [key,factor===0?quantity(0):factor===1?structuredClone(depth):{multiply:[structuredClone(depth),quantity(factor,'scalar',null)]}];
  }))};
}
export function readDirection(direction) {
  const literal=direction.literal??(direction.components&&Object.values(direction.components).every(expr=>expr.literal)?Object.fromEntries(Object.entries(direction.components).map(([key,expr])=>[key,expr.literal])):null);
  if(literal){const axis=['x','y','z'].map(key=>mm(literal[key])),distance=Math.hypot(...axis);if(distance>0&&Number.isFinite(distance))return {axis:axis.map(v=>v/distance),depth:quantity(distance)};}
  const components=direction.components;
  if(components){
    const axis=[0,0,0];let depth=null;
    for(const [i,key] of ['x','y','z'].entries()){
      const value=components[key];if(value?.literal&&mm(value.literal)===0)continue;
      let expression=value,factor=1;
      if(value?.multiply?.[1]?.literal?.dimension==='scalar'){expression=value.multiply[0];factor=value.multiply[1].literal.value;}
      if(!expression||!Number.isFinite(factor)||depth&&JSON.stringify(expression)!==JSON.stringify(depth))return null;
      depth=expression;axis[i]=factor;
    }
    const magnitude=Math.hypot(...axis);
    if(depth&&magnitude>0&&Number.isFinite(magnitude))return {axis:axis.map(v=>v/magnitude),depth:magnitude===1?depth:{multiply:[depth,quantity(magnitude,'scalar',null)]}};
  }
  return null;
}
export function controlledDirection(axis,depth,preserved=null) {
  const known=preserved&&readDirection(preserved);
  const sameDepth=known&&(depth.parameter?depth.parameter===known.depth.parameter:depth.literal&&known.depth.literal&&mm(depth.literal)===mm(known.depth.literal));
  if(sameDepth&&axis.every((v,i)=>v===known.axis[i]))return structuredClone(preserved);
  return extrusionDirection(axis,depth);
}
const vector = (values,dimension) => ({components:Object.fromEntries(['x','y','z'].map((key,i)=>[key,quantity(values[i],dimension,dimension==='length'?'millimeter':null)]))});
export function revolutionDefinition(origin,axis,angle,extent='angle') {
  if(origin.length!==3||!origin.every(Number.isFinite))throw new Error('Enter a finite axis origin.');
  const magnitude=Math.hypot(...axis);
  if(axis.length!==3||!axis.every(Number.isFinite)||!Number.isFinite(magnitude)||magnitude===0)throw new Error('Enter a finite, nonzero revolution axis.');
  if(angle?.literal&&(angle.literal.dimension!=='scalar'||!Number.isFinite(angle.literal.value)||angle.literal.value===0||Math.abs(angle.literal.value)>2*Math.PI))throw new Error('Revolution angle must be nonzero and between −360 and 360 degrees.');
  if(!['angle','symmetric'].includes(extent))throw new Error('Choose an angular or symmetric extent.');
  return {origin:vector(origin,'length'),axis:vector(axis.map(v=>v/magnitude),'scalar'),angle_radians:angle,extent};
}
const readVector = (value,dimension) => {
  const values=['x','y','z'].map(key=>value?.literal?.[key]??value?.components?.[key]?.literal);
  return values.every(v=>v?.dimension===dimension)?values.map(v=>dimension==='length'?mm(v):v.value):null;
};
export function solidChanges(features,{mode,id,input,target,direction,extent='distance',editing=null,kind='extrude',revolution=null}) {
  if(!id?.trim())throw new Error('Enter a result feature name.');
  if(!features.some(f=>f.id===input))throw new Error('Choose an existing planar profile.');
  if(!['extrude','revolve'].includes(kind))throw new Error('Choose extrusion or revolution.');
  const operation=kind==='revolve'?{revolve:{input,...revolution}}:{extrude:{input,direction,extent}};
  if(kind==='revolve'&&(!revolution||!['angle','symmetric'].includes(revolution.extent)))throw new Error('Choose an angular or symmetric extent.');
  if(kind==='extrude'&&!['distance','symmetric'].includes(extent))throw new Error('Choose a distance or symmetric extent.');
  if(editing){
    if(id!==editing.id)throw new Error('Keep the existing feature name.');
    if(!editing.operation[kind])throw new Error('Keep the existing feature type.');
    return {action:'replace',features:[{id,operation}]};
  }
  if(!['new','fuse','cut'].includes(mode))throw new Error('Choose a solid operation.');
  if(mode!=='new'&&!features.some(f=>f.id===target))throw new Error('Choose an existing target solid.');
  const toolId=mode==='new'?id:`${id}-${kind==='revolve'?'revolution':'extrusion'}`;
  const changes=[{id:toolId,operation}];
  if(mode!=='new')changes.push({id,operation:mode==='cut'?{cut:{object:target,tool:toolId}}:{fuse:{left:target,right:toolId}}});
  if(changes.some(change=>features.some(f=>f.id===change.id)))throw new Error('That result or tool feature name already exists.');
  return {action:'add',features:changes};
}
const profile = f => ['sketch_face','sketch_wire','planar_region'].some(key=>Object.hasOwn(f.operation,key));
const solidTarget = f => !profile(f) && !f.operation.sketch_open_wire;
export function createSolidEditor({host,request,instance,preview,applied,restore,showSketch=()=>{},showSolid=()=>{},initialProfile=()=>null,profileDirection=()=>null,profileAxis=()=>null,invalidatePreview=()=>{},autoStart=false}) {
  let data=null,editing=null,busy=false,mode='new',id='',input='',target='',extent='distance',axis=[0,0,1],depth=10,source='literal',preserved=null;
  let kind='extrude',origin=[0,0,0],angle=360,axisSource='literal',savedRevolve=null;
  const section=document.createElement('section');section.className='linked-edit solid-editor';
  const launch=document.createElement('button');launch.textContent='Sketch to solid';
  const content=document.createElement('div'),status=document.createElement('p');status.setAttribute('role','status');section.append(launch,content,status);host.prepend(section);
  const message=text=>status.textContent=text;
  function button(text,action){const e=document.createElement('button');e.textContent=text;e.onclick=async()=>{if(busy)return;try{await action();}catch(error){message(error.message);}};return e;}
  function select(label,choices,value,change){const row=document.createElement('label');row.textContent=label;const e=document.createElement('select');e.setAttribute('aria-label',label);for(const [key,title] of choices)e.append(new Option(title,key));e.value=value;e.onchange=()=>{invalidatePreview();change(e.value);};row.append(e);content.append(row);return e;}
  function number(label,value,change){const row=document.createElement('label');row.textContent=label;const e=document.createElement('input');e.type='number';e.step='any';e.value=value;e.setAttribute('aria-label',label);e.oninput=()=>{invalidatePreview();change(e.value.trim()?Number(e.value):NaN);};row.append(e);content.append(row);return e;}
  function start(feature=null){
    invalidatePreview();
    kind=feature?.operation.revolve?'revolve':'extrude';
    const sweep=feature?.operation[kind];
    editing=feature;mode='new';id=feature?.id??'';input=sweep?.input??(data.features.some(f=>profile(f)&&f.id===initialProfile())?initialProfile():data.features.find(profile)?.id)??'';target=data.features.some(f=>f.id===data.output&&solidTarget(f))?data.output:data.features.findLast(solidTarget)?.id??'';
    extent=sweep?.extent??'distance';preserved=sweep?.direction??null;
    const known=preserved?readDirection(preserved):null;axis=known?.axis??profileDirection(input)??[0,0,1];
    const value=known?.depth;depth=value?.literal?mm(value.literal):10;
    source=!feature?'literal':!known?'preserve':value.parameter?`parameter:${value.parameter}`:value.literal?'literal':'preserve';
    savedRevolve=feature?.operation.revolve??null;axisSource=savedRevolve?'preserve':'literal';
    origin=readVector(savedRevolve?.origin,'length')??[0,0,0];
    if(savedRevolve){axis=readVector(savedRevolve.axis,'scalar')??[0,0,1];extent=savedRevolve.extent??'angle';const a=savedRevolve.angle_radians;angle=a.literal?a.literal.value*180/Math.PI:360;source=a.parameter?`parameter:${a.parameter}`:'preserve';}
    else angle=360;
    render();message(feature?`Editing ${kind==='revolve'?'revolution':'extrusion'} ${feature.id}.`:'Choose a profile and operation. Apply any pending sketch edits first.');
  }
  function render(){
    content.replaceChildren();const title=document.createElement('h3');title.textContent=editing?`Edit ${kind==='revolve'?'revolution':'extrusion'}`:'Create solid feature';content.append(title);
    const note=document.createElement('p');note.textContent='Family-wide edit. Uses the applied profile’s material regions and holes. Axes use family coordinates. Extrusion depth is travel distance; revolution angles are signed degrees (scalar parameters use radians). Preview keeps the accepted model unchanged.';content.append(note);
    if(!editing)select('Solid operation',[['new','New solid'],['fuse','Add material (union)'],['cut','Remove material (cut)']],mode,value=>{mode=value;render();});
    if(!editing)select('Solid feature',[['extrude','Extrusion'],['revolve','Revolution']],kind,value=>{kind=value;extent=kind==='revolve'?'angle':'distance';source='literal';if(kind==='revolve'){const frame=profileAxis(input);origin=[...(frame?.origin??[0,0,0])];axis=[...(frame?.axis??[0,0,1])];}else axis=profileDirection(input)??[0,0,1];render();});
    const nameLabel=document.createElement('label');nameLabel.textContent='Result feature name';const name=document.createElement('input');name.value=id;name.disabled=Boolean(editing);name.setAttribute('aria-label','Result feature name');name.oninput=()=>{invalidatePreview();id=name.value;};nameLabel.append(name);content.append(nameLabel);
    const profiles=data.features.filter(profile);if(editing&&!profiles.some(f=>f.id===input)){const original=data.features.find(f=>f.id===input);if(original)profiles.push(original);}
    select('Sketch / planar profile',[['','Choose profile'],...profiles.map(f=>[f.id,f.id])],input,value=>{input=value;if(!editing){if(kind==='revolve'){const frame=profileAxis(input);origin=[...(frame?.origin??[0,0,0])];axis=[...(frame?.axis??[0,0,1])];}else axis=profileDirection(input)??axis;render();}});
    content.append(button('Show source sketch',()=>showSketch(input)));
    if(!editing&&mode!=='new')select('Target solid',[['','Choose target'],...data.features.filter(solidTarget).map(f=>[f.id,f.id])],target,value=>target=value);
    if(kind==='extrude'){
      select('Extrusion extent',[['distance','One direction'],['symmetric','Symmetric (total depth)']],extent,value=>extent=value);
      const choices=[['literal','Entered depth'],...data.parameters.filter(p=>p.parameter_type?.scalar==='length').map(p=>[`parameter:${p.id}`,`Parameter: ${p.id}`])];
      if(source.startsWith('parameter:')&&!choices.some(([key])=>key===source))choices.push([source,`Existing parameter: ${source.slice(10)}`]);
      if(preserved)choices.push(['preserve','Keep existing direction and formula']);
      select('Extrusion depth source',choices,source,value=>{source=value;render();});
      number('Extrusion depth (mm)',depth,value=>depth=value).disabled=source!=='literal';
      for(const [index,key] of ['X','Y','Z'].entries())number(`Direction ${key}`,axis[index],value=>axis[index]=value).disabled=source==='preserve';
    }else{
      select('Revolution extent',[['angle','One direction'],['symmetric','Symmetric (total angle)']],extent,value=>extent=value);
      if(savedRevolve)select('Revolution axis source',[['preserve','Keep existing axis and origin formulas'],['literal','Entered axis and origin']],axisSource,value=>{axisSource=value;render();});
      for(const [i,key] of ['X','Y','Z'].entries())number(`Axis origin ${key} (mm)`,origin[i],value=>origin[i]=value).disabled=axisSource==='preserve';
      for(const [i,key] of ['X','Y','Z'].entries())number(`Revolution axis ${key}`,axis[i],value=>axis[i]=value).disabled=axisSource==='preserve';
      const choices=[['literal','Entered angle'],...data.parameters.filter(p=>p.parameter_type?.scalar==='scalar').map(p=>[`parameter:${p.id}`,`Parameter (radians): ${p.id}`])];
      if(source.startsWith('parameter:')&&!choices.some(([key])=>key===source))choices.push([source,`Existing parameter: ${source.slice(10)}`]);
      if(savedRevolve)choices.push(['preserve','Keep existing angle formula']);
      select('Revolution angle source',choices,source,value=>{source=value;render();});
      number('Revolution angle (degrees)',angle,value=>angle=value).disabled=source!=='literal';
    }
    content.append(button('Preview solid',()=>submit(false)),button('Apply verified solid',()=>submit(true)),button('Close solid draft',async()=>{data=null;content.replaceChildren();await restore();message('Solid draft closed.');}));
    const heading=document.createElement('h3');heading.textContent='Feature list';content.append(heading);
    for(const feature of data.features){
      const kind=Object.keys(feature.operation)[0],row=document.createElement('div'),label=document.createElement('span');label.textContent=`${feature.id} · ${kind.replaceAll('_',' ')}`;row.append(label);
      if(feature.operation.extrude&&['distance','symmetric'].includes(feature.operation.extrude.extent??'distance'))row.append(button(`Edit extrusion ${feature.id}`,()=>start(feature)));
      else if(feature.operation.revolve)row.append(button(`Edit revolution ${feature.id}`,()=>start(feature)));
      else if(profile(feature))row.append(button(`View sketch ${feature.id}`,()=>showSketch(feature.id)));
      content.append(row);
    }
  }
  async function submit(apply){
    invalidatePreview();
    let direction,revolution;
    if(kind==='extrude'){
      const expression=source.startsWith('parameter:')?{parameter:source.slice(10)}:quantity(depth);
      direction=source==='preserve'?preserved:controlledDirection(axis,expression,preserved);
    }else{
      const expression=source==='preserve'?savedRevolve.angle_radians:source.startsWith('parameter:')?{parameter:source.slice(10)}:quantity(angle*Math.PI/180,'scalar',null);
      revolution=axisSource==='preserve'?{origin:structuredClone(savedRevolve.origin),axis:structuredClone(savedRevolve.axis),angle_radians:expression,extent}:revolutionDefinition(origin,axis,expression,extent);
    }
    const changes=solidChanges(data.features,{mode,id,input,target,direction,extent,editing,kind,revolution});
    const output=editing?(data.features.some(f=>f.id===data.output&&solidTarget(f))?data.output:editing.id):id,selectedInstance=instance();busy=true;content.inert=true;launch.disabled=true;message('Regenerating solid and checking requirements…');
    try{
      const response=await request('/api/features',{method:'POST',body:JSON.stringify({version:data.version,instance:selectedInstance,expected:data.features,...changes,output,apply})});
      if(apply){data=null;content.replaceChildren();await applied(response.state);showSolid(response.output);message('Solid applied. Save writes the feature graph to the model file.');}
      else{if(response.annotations||response.gltf){await preview(response.annotations,response);showSolid(response.output);}const evidence=response.geometry?.[selectedInstance];message(response.can_apply?`Solid preview passes. ${evidence?.solids??''} solid(s), ${Number(evidence?.volume_mm3??0).toLocaleString()} mm³. Apply repeats validation.`:`Solid draft rejected: ${response.error}`);}
    }finally{busy=false;content.inert=false;launch.disabled=false;}
  }
  launch.onclick=async()=>{if(busy)return;try{message('Loading solid features…');data=await request('/api/features');start();}catch(error){message(error.message);}};
  if(autoStart){launch.hidden=true;launch.onclick();}
  return section;
}
