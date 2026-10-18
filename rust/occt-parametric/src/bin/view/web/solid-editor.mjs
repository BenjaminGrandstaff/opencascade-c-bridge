import {createLoftCorrespondenceControls} from './solid-loft-correspondence.mjs';
import {createLoftEndControls} from './solid-loft-constraints.mjs';
import {createHoleControls} from './solid-holes.mjs';
import {isEdgeFinish,edgeFinishDefinition} from './solid-finishes.mjs';
import {extrusionExtentMode,extrusionExtent,checkedExtrusionExtent} from './solid-extrusion.mjs';
import {loftDefinition,editLoftStations} from './solid-loft.mjs';
import {readSweepOptions,sweepOptionsDefinition,renderSweepOptions} from './solid-sweep-options.mjs';
import {SOLID_TOOLS,isSolidTool,inPlaceTool,solidToolKind,readSolidTool,renderSolidTool,solidToolOperation} from './solid-tools.mjs';
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
export function sweepOrientation(mode,axis,preserved=null) {
  if(mode==='preserve'&&preserved)return structuredClone(preserved);
  if(['corrected_frenet','frenet','fixed'].includes(mode))return mode;
  if(mode!=='binormal')throw new Error('Choose a sweep orientation.');
  const magnitude=Math.hypot(...axis);
  if(axis.length!==3||!axis.every(Number.isFinite)||!Number.isFinite(magnitude)||magnitude===0)throw new Error('Enter a finite, nonzero binormal direction.');
  return {binormal:{direction:vector(axis.map(v=>v/magnitude),'scalar')}};
}
const pathFeature = f => ['sketch_open_wire','sketch_wire','helix'].some(key=>Object.hasOwn(f.operation,key));
const solidNames={extrude:'extrusion',revolve:'revolution',sweep:'sweep',profile_loft:'loft',fillet:'fillet',chamfer:'chamfer',hole:'hole',...Object.fromEntries(Object.entries(SOLID_TOOLS).map(([kind,tool])=>[kind,tool.name]))};
export function solidChanges(features,{mode,id,input,target,direction,extent='distance',editing=null,kind='extrude',revolution=null,path='',orientation='corrected_frenet',sweepOptions={},loft=null,finish=null,hole=null,toolOperation=null}) {
  if(!id?.trim())throw new Error('Enter a result feature name.');
  if(kind!=='profile_loft'&&!features.some(f=>f.id===input))throw new Error('Choose an existing planar profile.');
  if(!Object.hasOwn(solidNames,kind))throw new Error('Choose a supported solid feature.');
  if(kind==='sweep'&&(!features.some(f=>f.id===path)||path===input))throw new Error('Choose a separate existing sweep path.');
  const operation=toolOperation?structuredClone(toolOperation):kind==='hole'?{hole:structuredClone(hole)}:isEdgeFinish(kind)?{[kind]:edgeFinishDefinition(kind,input,finish?.edges,finish?.size)}:kind==='profile_loft'?{profile_loft:loftDefinition(features,loft??{})}:kind==='sweep'?{sweep:{profile:input,path,orientation,...structuredClone(sweepOptions)}}:kind==='revolve'?{revolve:{input,...revolution}}:{extrude:{input,direction,extent}};
  if(kind==='revolve'&&(!revolution||!['angle','symmetric'].includes(revolution.extent)))throw new Error('Choose an angular or symmetric extent.');
  if(kind==='extrude')operation.extrude.extent=checkedExtrusionExtent(features,extent,input,id);
  if(editing){
    if(id!==editing.id)throw new Error('Keep the existing feature name.');
    if(!editing.operation[kind])throw new Error('Keep the existing feature type.');
    return {action:'replace',features:[{id,operation}]};
  }
  if(isEdgeFinish(kind)||kind==='hole'||inPlaceTool(kind)){
    if(features.some(f=>f.id===id))throw Error('That result feature name already exists.');
    return {action:'add',features:[{id,operation}]};
  }
  if(!['new','fuse','cut'].includes(mode))throw new Error('Choose a solid operation.');
  if(mode!=='new'&&!features.some(f=>f.id===target))throw new Error('Choose an existing target solid.');
  const toolId=mode==='new'?id:`${id}-${solidNames[kind].replaceAll(' ','-')}`;
  const changes=[{id:toolId,operation}];
  if(mode!=='new')changes.push({id,operation:mode==='cut'?{cut:{object:target,tool:toolId}}:{fuse:{left:target,right:toolId}}});
  if(changes.some(change=>features.some(f=>f.id===change.id)))throw new Error('That result or tool feature name already exists.');
  return {action:'add',features:changes};
}
const profile = f => ['sketch_face','sketch_wire','planar_region'].some(key=>Object.hasOwn(f.operation,key));
const solidTarget = f => !profile(f) && !pathFeature(f);
export function solidEditOutput(data,editing) {
  const pending=[data.output],seen=new Set();
  while(pending.length){const id=pending.pop();if(!id||seen.has(id))continue;seen.add(id);pending.push(...(data.dependencies?.[id]??[]));}
  return seen.has(editing.id)&&data.features.some(f=>f.id===data.output&&solidTarget(f))?data.output:editing.id;
}
export function createSolidEditor({host,request,instance,preview,applied,restore,showSketch=()=>{},showSolid=()=>{},initialProfile=()=>null,profileDirection=()=>null,profileAxis=()=>null,invalidatePreview=()=>{},pickSolidEdge=null,pickLoftAnchor=null,pickLoftSplit=null,pickHolePosition=null,pickLimitingFace=null,cancelLimitingFacePick=()=>{},started=()=>{},loftDiagnosed=()=>{},autoStart=false}) {
  let data=null,editing=null,busy=false,mode='new',id='',input='',target='',extent='distance',axis=[0,0,1],depth=10,source='literal',preserved=null;
  let kind='extrude',origin=[0,0,0],angle=360,axisSource='literal',savedRevolve=null,path='',orientation='corrected_frenet',binormal=[0,0,1],savedOrientation=null,sweepState=readSweepOptions(),loft={profiles:[],holes:[],ruled:false},loftStarted=false;
  let limitTarget='',limitFace='z:maximum',savedFace=null,limitDirectionSource='literal';
  let finishTarget='',finishEdges=[],finishSize=2,finishSource='literal',savedSize=null;
  let formTurn=0,pickedLimit=null,holeControls=null,loftEnds=null,loftCorrespondence=null,tool=null;
  const diagnosis=document.createElement('div');diagnosis.setAttribute('data-loft-profile-report','');
  const invalidate=()=>{diagnosis.replaceChildren();formTurn++;cancelLimitingFacePick();invalidatePreview();};
  const section=document.createElement('section');section.className='linked-edit solid-editor';
  const launch=document.createElement('button');launch.textContent='Sketch to solid';
  const content=document.createElement('div'),status=document.createElement('p');status.setAttribute('role','status');section.append(launch,content,status);host.prepend(section);
  const message=text=>status.textContent=text;
  function button(text,action){const e=document.createElement('button');e.textContent=text;e.onclick=async()=>{if(busy)return;try{await action();}catch(error){message(error.message);}};return e;}
  function select(label,choices,value,change){const row=document.createElement('label');row.textContent=label;const e=document.createElement('select');e.setAttribute('aria-label',label);for(const [key,title] of choices)e.append(new Option(title,key));e.value=value;e.onchange=()=>{invalidate();change(e.value);};row.append(e);content.append(row);return e;}
  function number(label,value,change){const row=document.createElement('label');row.textContent=label;const e=document.createElement('input');e.type='number';e.step='any';e.value=value;e.setAttribute('aria-label',label);e.oninput=()=>{invalidate();change(e.value.trim()?Number(e.value):NaN);};row.append(e);content.append(row);return e;}
  function start(feature=null){
    invalidate();
    kind=solidToolKind(feature?.operation)??(feature?.operation.hole?'hole':feature?.operation.fillet?'fillet':feature?.operation.chamfer?'chamfer':feature?.operation.profile_loft?'profile_loft':feature?.operation.sweep?'sweep':feature?.operation.revolve?'revolve':'extrude');
    const sweep=isSolidTool(kind)?null:feature?.operation[kind];
    editing=feature;mode='new';id=feature?.id??'';input=sweep?.profiles?.[0]??sweep?.profile??sweep?.input??(data.features.some(f=>profile(f)&&f.id===initialProfile())?initialProfile():data.features.find(profile)?.id)??'';target=data.features.some(f=>f.id===data.output&&solidTarget(f))?data.output:data.features.findLast(solidTarget)?.id??'';
    extent=extrusionExtentMode(sweep?.extent??'distance');
    const limit=sweep?.extent?.up_to_face??sweep?.extent?.up_to_next;
    limitTarget=limit?.target??data.features.findLast(f=>solidTarget(f)&&f.id!==feature?.id)?.id??'';savedFace=limit?.face??null;limitFace=savedFace?'preserve':'z:maximum';pickedLimit=null;limitDirectionSource=feature?'preserve':'literal';preserved=sweep?.direction??null;
    const known=preserved?readDirection(preserved):null;axis=known?.axis??profileDirection(input)??[0,0,1];
    const value=known?.depth;depth=value?.literal?mm(value.literal):10;
    source=!feature?'literal':!known?'preserve':value.parameter?`parameter:${value.parameter}`:value.literal?'literal':'preserve';
    savedRevolve=feature?.operation.revolve??null;axisSource=savedRevolve?'preserve':'literal';
    origin=readVector(savedRevolve?.origin,'length')??[0,0,0];
    if(savedRevolve){axis=readVector(savedRevolve.axis,'scalar')??[0,0,1];extent=savedRevolve.extent??'angle';const a=savedRevolve.angle_radians;angle=a.literal?a.literal.value*180/Math.PI:360;source=a.parameter?`parameter:${a.parameter}`:'preserve';}
    else angle=360;
    path=feature?.operation.sweep?.path??data.features.find(f=>pathFeature(f)&&f.id===initialProfile()&&f.id!==input)?.id??data.features.find(f=>pathFeature(f)&&!f.operation.sketch_wire&&f.id!==input)?.id??data.features.find(f=>pathFeature(f)&&f.id!==input)?.id??'';
    savedOrientation=feature?.operation.sweep?.orientation??null;
    sweepState=readSweepOptions(feature?.operation.sweep);
    orientation=savedOrientation&&typeof savedOrientation==='object'?'preserve':savedOrientation??'corrected_frenet';
    binormal=readVector(savedOrientation?.binormal?.direction,'scalar')??[0,0,1];
    loft=feature?.operation.profile_loft?structuredClone(feature.operation.profile_loft):{profiles:[input,data.features.find(f=>profile(f)&&f.id!==input)?.id??''],holes:[],ruled:false};
    loftEnds=createLoftEndControls(loft);
    loftCorrespondence=createLoftCorrespondenceControls(loft);
    loft.holes??=[];loftStarted=Boolean(feature?.operation.profile_loft);
    finishTarget=isEdgeFinish(kind)?sweep.input:target;finishEdges=isEdgeFinish(kind)?structuredClone(sweep.edges):[];savedSize=isEdgeFinish(kind)?sweep.radius??sweep.distance:null;finishSize=savedSize?(savedSize.literal?mm(savedSize.literal):NaN):2;finishSource=savedSize?.parameter?`parameter:${savedSize.parameter}`:savedSize?'preserve':'literal';
    holeControls=createHoleControls(feature?.operation.hole,target,data.thread_sizes);
    tool=isSolidTool(kind)?readSolidTool(kind,feature?.operation[kind],target):null;
    render();started(feature,data.output);message(feature?`Editing ${solidNames[kind]} ${feature.id}.`:'Choose a profile and operation. Apply any pending sketch edits first.');
  }
  function render(){
    content.replaceChildren();const title=document.createElement('h3');title.textContent=editing?`Edit ${solidNames[kind]}`:'Create solid feature';content.append(title);
    const note=document.createElement('p');note.textContent=kind==='hole'?'Family-wide drilling operation. Position and axis use family coordinates. Preview keeps the accepted model unchanged.':isEdgeFinish(kind)?'Family-wide edit. Finishes the accepted source solid using saved edge selectors. Preview keeps the accepted model unchanged.':'Family-wide edit. Uses the applied profile’s material regions and holes. Axes use family coordinates. Extrusion depth is travel distance; revolution angles are signed degrees (scalar parameters use radians). Preview keeps the accepted model unchanged.';content.append(note);
    if(!editing&&!isEdgeFinish(kind)&&kind!=='hole'&&!inPlaceTool(kind))select('Solid operation',[['new','New solid'],['fuse','Add material (union)'],['cut','Remove material (cut)']],mode,value=>{mode=value;render();});
    if(!editing)select('Solid feature',[['hole','Drill a hole'],['extrude','Extrusion'],['revolve','Revolution'],['sweep','Sweep along path'],['profile_loft','Loft through profiles'],...(pickSolidEdge?[['fillet','Fillet solid edges'],['chamfer','Chamfer solid edges']]:[]),['hollow','Shell (hollow) a solid'],['draft','Draft faces'],['mirror','Mirror a solid'],['linear_pattern','Linear pattern of a solid'],['circular_pattern','Circular pattern of a solid']],kind,value=>{kind=value;if(isSolidTool(kind))tool=readSolidTool(kind,null,tool?.source||target);if(kind==='profile_loft'&&!loftStarted){loft={profiles:[input,data.features.find(f=>profile(f)&&f.id!==input)?.id??''],holes:[],ruled:false};loftStarted=true;}extent=kind==='revolve'?'angle':'distance';source='literal';if(kind==='revolve'){const frame=profileAxis(input);origin=[...(frame?.origin??[0,0,0])];axis=[...(frame?.axis??[0,0,1])];}else axis=profileDirection(input)??[0,0,1];render();});
    const nameLabel=document.createElement('label');nameLabel.textContent='Result feature name';const name=document.createElement('input');name.value=id;name.disabled=Boolean(editing);name.setAttribute('aria-label','Result feature name');name.oninput=()=>{invalidate();id=name.value;};nameLabel.append(name);content.append(nameLabel);
    const profiles=data.features.filter(profile);if(editing&&!profiles.some(f=>f.id===input)){const original=data.features.find(f=>f.id===input);if(original)profiles.push(original);}
    if(kind==='profile_loft'&&editing)for(const id of [...loft.profiles,...loft.holes.flat()]){if(!profiles.some(f=>f.id===id)){const saved=data.features.find(f=>f.id===id);if(saved)profiles.push(saved);}}
    if(kind!=='profile_loft'&&!isEdgeFinish(kind)&&kind!=='hole'&&!isSolidTool(kind)){
      select('Sketch / planar profile',[['','Choose profile'],...profiles.map(f=>[f.id,f.id])],input,value=>{input=value;if(!editing){if(kind==='revolve'){const frame=profileAxis(input);origin=[...(frame?.origin??[0,0,0])];axis=[...(frame?.axis??[0,0,1])];}else axis=profileDirection(input)??axis;render();}});
      content.append(button('Show source sketch',()=>showSketch(input)));
    }
    if(!editing&&!isEdgeFinish(kind)&&kind!=='hole'&&!inPlaceTool(kind)&&mode!=='new')select('Target solid',[['','Choose target'],...data.features.filter(solidTarget).map(f=>[f.id,f.id])],target,value=>target=value);
    if(isSolidTool(kind))renderSolidTool({kind,state:tool,features:data.features.filter(f=>solidTarget(f)&&f.id!==editing?.id),parameters:data.parameters,select,number,button,content,render,invalidate,message,pickFace:pickLimitingFace});
    if(kind==='hole')holeControls.render({select,number,content,button,features:data.features.filter(solidTarget),parameters:data.parameters,editing,invalidate,pickPosition:pickHolePosition,pickFace:pickLimitingFace,cancelPick:cancelLimitingFacePick,message,current:{render,checkpoint:()=>{const turn=formTurn;return ()=>turn===formTurn&&Boolean(data)&&kind==='hole';}}});
    if(kind==='extrude'){
      select('Extrusion extent',[['distance','One direction'],['symmetric','Symmetric (total depth)'],['up_to_next','Up to next face'],['up_to_face','Up to selected face']],extent,value=>{extent=value;render();});
      if(['distance','symmetric'].includes(extent)){
        const choices=[['literal','Entered depth'],...data.parameters.filter(p=>p.parameter_type?.scalar==='length').map(p=>[`parameter:${p.id}`,`Parameter: ${p.id}`])];
        if(source.startsWith('parameter:')&&!choices.some(([key])=>key===source))choices.push([source,`Existing parameter: ${source.slice(10)}`]);
        if(preserved)choices.push(['preserve','Keep existing direction and formula']);
        select('Extrusion depth source',choices,source,value=>{source=value;render();});
        number('Extrusion depth (mm)',depth,value=>depth=value).disabled=source!=='literal';
        for(const [index,key] of ['X','Y','Z'].entries())number(`Direction ${key}`,axis[index],value=>axis[index]=value).disabled=source==='preserve';
      }else{
        select('Limiting feature',[['','Choose limit'],...data.features.filter(f=>f.id!==input&&f.id!==editing?.id).map(f=>[f.id,f.id])],limitTarget,value=>{limitTarget=value;if(value!==editing?.operation.extrude?.extent?.up_to_face?.target){limitFace='z:maximum';pickedLimit=null;}render();});
        if(extent==='up_to_face'){
          const choices=['x','y','z'].flatMap(axis=>[['minimum',`Minimum ${axis.toUpperCase()}`],['maximum',`Maximum ${axis.toUpperCase()}`]].map(([extremum,label])=>[`${axis}:${extremum}`,label]));
          if(savedFace)choices.push(['preserve',pickedLimit?'Picked face':'Keep existing face selector']);
          select('Limiting face',choices,limitFace,value=>{limitFace=value;if(value!=='preserve')pickedLimit=null;render();});
          if(pickLimitingFace)content.append(button('Pick limiting face in canvas',async()=>{
            if(!limitTarget)throw Error('Choose a limiting feature first.');
            invalidate();const turn=formTurn,target=limitTarget;
            message('Click a planar face of the amber limiting feature. Escape cancels the pick.');
            const picked=await pickLimitingFace(target);
            if(turn!==formTurn||!data)return;
            if(!picked){message('Face picking canceled. Solid draft retained.');return;}
            if(picked.target!==target||!picked.face)throw Error('The picked face does not match the limiting feature.');
            savedFace=structuredClone(picked.face);limitFace='preserve';pickedLimit=picked;render();
            message(picked.binding==='anchored_center'?'Face selected with a fixed center anchor. Moving it requires picking again; Preview validates the result.':'Limiting face selected. Preview validates every instance.');
          }),button('Cancel face pick',()=>{invalidate();message('Face picking canceled. Solid draft retained.');}));
          if(pickedLimit){const info=document.createElement('p');info.textContent='Picked planar face · '+(pickedLimit.origin_mm??[]).map(v=>Number(v.toPrecision(6))).join(', ')+' mm';content.append(info);}

        }
        if(preserved)select('Extrusion direction source',[['literal','Entered direction'],['preserve','Keep existing direction formula']],limitDirectionSource,value=>{limitDirectionSource=value;render();});
        for(const [index,key] of ['X','Y','Z'].entries())number(`Direction ${key}`,axis[index],value=>axis[index]=value).disabled=limitDirectionSource==='preserve';
        const note=document.createElement('p');note.textContent='The limit must cover the complete profile strictly forward. Up to next finds the first covering face of the limiting feature; selected faces use minimum/maximum center along a family axis. Direction sets orientation only; depth is determined by geometry. Native preview rejects missing or ambiguous limits.';content.append(note);
      }
    }else if(kind==='revolve'){
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
    if(kind==='sweep'){
      const paths=data.features.filter(f=>pathFeature(f)&&f.id!==input);
      if(editing&&!paths.some(f=>f.id===path)){const original=data.features.find(f=>f.id===path);if(original)paths.push(original);}
      select('Sweep path',[['','Choose path'],...paths.map(f=>[f.id,f.id])],path,value=>path=value);
      content.append(button('Show sweep path',()=>showSketch(path)));
      const note=document.createElement('p');note.textContent='Place the closed profile at the path start, usually perpendicular to it. The path is an applied open/closed sketch wire or helix. Profile holes are retained. Least twist is the usual orientation; binormal uses a family-coordinate direction.';content.append(note);
      const choices=[['corrected_frenet','Least twist'],['frenet','Frenet (curvature frame)'],['fixed','Fixed (parallel sections)'],['binormal','Binormal direction']];
      if(savedOrientation&&typeof savedOrientation==='object')choices.push(['preserve','Keep existing orientation formula']);
      select('Sweep orientation',choices,orientation,value=>{orientation=value;render();});
      if(orientation==='binormal')for(const [i,key] of ['X','Y','Z'].entries())number(`Binormal ${key}`,binormal[i],value=>binormal[i]=value);
      renderSweepOptions({state:sweepState,features:data.features,parameters:data.parameters,input,path,select,number,button,content,render});
    }
    if(kind==='profile_loft'){
      const note=document.createElement('p');note.textContent='Order distinct closed profiles along the part. Each section has one boundary. For hollow parts, add a hole track and choose one inner profile per station; reordering stations moves all tracks together.';content.append(note);
      const change=next=>{invalidate();loft=next;render();};
      select('Loft surfaces',[['smooth','Smooth'],['ruled','Ruled (straight between sections)']],loft.ruled?'ruled':'smooth',value=>loft.ruled=value==='ruled');
      const choices=[['','Choose profile'],...profiles.map(f=>[f.id,f.id])];
      const correspondenceControls={select,number,render,button,content,pickAnchor:pickLoftAnchor,pickSplit:pickLoftSplit,invalidate,message,checkpoint:()=>{const turn=formTurn;return ()=>turn===formTurn&&Boolean(data)&&kind==='profile_loft';}};
      const stationAction=(label,action,index,disabled)=>{const e=button(label,()=>change(editLoftStations(loft,action,index)));e.disabled=disabled;return e;};
      for(const [i,id] of loft.profiles.entries()){
        const heading=document.createElement('h4');heading.textContent=`Loft station ${i+1}`;content.append(heading);
        select(`Outer profile at station ${i+1}`,choices,id,value=>{loft.profiles[i]=value;render();});
        loftCorrespondence.render(id,`Outer station ${i+1}`,correspondenceControls);
        for(const [h,track] of loft.holes.entries()){
          select(`Hole ${h+1} at station ${i+1}`,choices,track[i],value=>{loft.holes[h][i]=value;render();});
          loftCorrespondence.render(track[i],`Hole ${h+1} station ${i+1}`,correspondenceControls);
        }
        content.append(button(`Show loft station ${i+1}`,()=>showSketch(loft.profiles[i])),stationAction(`Move station ${i+1} earlier`,'earlier',i,i===0),stationAction(`Move station ${i+1} later`,'later',i,i===loft.profiles.length-1),stationAction(`Remove loft station ${i+1}`,'remove',i,loft.profiles.length<=2));
      }
      content.append(stationAction('Add loft station','add',0,loft.profiles.length>=1000));
      const addHole=button('Add loft hole track',()=>{if(loft.holes.length>=100)throw Error('A loft supports at most 100 hole tracks.');change({...loft,holes:[...loft.holes,loft.profiles.map(()=> '')]});});addHole.disabled=loft.holes.length>=100;content.append(addHole);
      for(const h of loft.holes.keys())content.append(button(`Remove loft hole track ${h+1}`,()=>change({...loft,holes:loft.holes.filter((_,i)=>i!==h)})));
      loftEnds.render({select,number,content,parameters:data.parameters,render});
      content.append(button('Check loft profiles',async()=>{
        invalidate();const turn=formTurn,selectedInstance=instance();
        const operation={profile_loft:loftDefinition(data.features,{...loft,...loftEnds.definition(),...loftCorrespondence.definition(loft)})};
        message('Checking exact source profiles and correspondence…');
        try{
          const response=await request('/api/loft-profile-report',{method:'POST',body:JSON.stringify({version:data.version,instance:selectedInstance,operation})});
          if(turn!==formTurn||!data||kind!=='profile_loft'||selectedInstance!==instance())return;
          const canonical=value=>JSON.stringify(value,(_,v)=>v&&typeof v==='object'&&!Array.isArray(v)?Object.fromEntries(Object.entries(v).sort(([a],[b])=>a.localeCompare(b))):v);
          const report=response.report,stations=report?.stations??[];
          if(response.version!==data.version||response.instance!==selectedInstance||canonical(response.operation)!==canonical(operation)||typeof report?.checks_passed!=='boolean'||report.solid_checked!==false||!Array.isArray(report.profiles)||report.profiles.length>8192||!Array.isArray(report.issues)||!Array.isArray(stations)||stations.length>1000)throw Error('The loft profile report does not match this draft.');
          const note=document.createElement('p');note.textContent=report.checks_passed?'Profile checks pass. Preview still checks correspondence, twist, hole containment and the resulting solid.':'Profile checks found problems. Correct the listed profiles, then Preview the solid.';diagnosis.append(note);
          for(const item of report.issues){const row=document.createElement('p');row.textContent=item.message;diagnosis.append(row);}
          for(const p of report.profiles){
            const row=document.createElement('p');row.textContent=`${p.hole===null?'Outer':`Hole ${p.hole+1}`} station ${p.station+1} · ${p.profile}: ${p.source_edges??'?'} source → ${p.aligned_edges??'?'} aligned edges${p.reverse?' · reversed':''}. ${p.issues.map(i=>i.message).join(' ')}`;diagnosis.append(row);
          }
          for(const s of stations){
            const row=document.createElement('p');row.textContent=`Station ${s.station+1} material region · ${s.outer}: ${s.area_mm2===null?'invalid':`${Number(s.area_mm2.toFixed(6))} mm² with ${s.holes.length} hole${s.holes.length===1?'':'s'}`}. ${s.issues.map(i=>i.message).join(' ')}`;diagnosis.append(row);
          }
          loftDiagnosed(report);diagnosis.scrollIntoView?.({block:'nearest'});
          message('Profile diagnosis complete. Apply still performs full validation.');
        }catch(error){if(turn===formTurn&&data)message(error.message);}
      }),diagnosis);

    }
    if(isEdgeFinish(kind)){
      select('Finish solid',[['','Choose solid'],...data.features.filter(f=>solidTarget(f)&&f.id!==editing?.id).map(f=>[f.id,f.id])],finishTarget,value=>{if(finishTarget!==value)finishEdges=[];finishTarget=value;render();});
      const note=document.createElement('p');note.textContent='Pick one or more edge sets on the accepted source solid. Existing semantic selectors remain linked; native preview checks the size and resulting solid across every instance.';content.append(note);
      for(const [i,edge] of finishEdges.entries())content.append(button(`Remove edge set ${i+1}`,()=>{invalidate();finishEdges.splice(i,1);render();}));
      if(!finishEdges.length){const empty=document.createElement('p');empty.textContent='No edge sets selected.';content.append(empty);}
      if(pickSolidEdge)content.append(button('Pick finishing edge in canvas',async()=>{
        if(!finishTarget)throw Error('Choose a source solid first.');
        invalidate();const turn=formTurn,target=finishTarget;message('Click an edge of the amber source solid. Escape cancels the pick.');
        const picked=await pickSolidEdge(target);
        if(turn!==formTurn||!data)return;
        if(!picked){message('Edge picking canceled. Solid draft retained.');return;}
        if(picked.target!==target||!picked.edge)throw Error('The picked edge does not match the source solid.');
        if(finishEdges.some(edge=>JSON.stringify(edge)===JSON.stringify(picked.edge))){message('That edge set is already selected.');return;}
        finishEdges.push(structuredClone(picked.edge));render();message(picked.binding==='anchored_center'?'Edge added with a fixed center anchor. Moving it requires picking again; Preview validates the result.':'Edge set added. Preview checks every instance.');
      }),button('Cancel edge pick',()=>{invalidate();message('Edge picking canceled. Solid draft retained.');}));
      const choices=[['literal','Entered size'],...data.parameters.filter(p=>p.parameter_type?.scalar==='length').map(p=>[`parameter:${p.id}`,`Parameter: ${p.id}`])];
      if(finishSource.startsWith('parameter:')&&!choices.some(([key])=>key===finishSource))choices.push([finishSource,`Existing parameter: ${finishSource.slice(10)}`]);
      if(savedSize)choices.push(['preserve','Keep existing size formula']);
      select('Finishing size source',choices,finishSource,value=>{finishSource=value;render();});
      number(kind==='fillet'?'Fillet radius (mm)':'Chamfer distance (mm)',finishSource==='literal'?finishSize:finishSource==='preserve'&&savedSize?.literal?mm(savedSize.literal):'',value=>finishSize=value).disabled=finishSource!=='literal';
    }
    content.append(button('Preview solid',()=>submit(false)),button('Apply verified solid',()=>submit(true)),button('Close solid draft',async()=>{invalidate();data=null;content.replaceChildren();await restore();message('Solid draft closed.');}));
    const heading=document.createElement('h3');heading.textContent='Feature list';content.append(heading);
    for(const feature of data.features){
      const kind=Object.keys(feature.operation)[0],row=document.createElement('div'),label=document.createElement('span');label.textContent=`${feature.id} · ${kind.replaceAll('_',' ')}`;row.append(label);
      if(feature.operation.hole)row.append(button(`Edit hole ${feature.id}`,()=>start(feature)));
      else if(feature.operation.extrude)row.append(button(`Edit extrusion ${feature.id}`,()=>start(feature)));
      else if(feature.operation.fillet||feature.operation.chamfer)row.append(button(`Edit ${feature.operation.fillet?'fillet':'chamfer'} ${feature.id}`,()=>start(feature)));
      else if(feature.operation.profile_loft)row.append(button(`Edit loft ${feature.id}`,()=>start(feature)));
      else if(feature.operation.sweep)row.append(button(`Edit sweep ${feature.id}`,()=>start(feature)));
      else if(feature.operation.revolve)row.append(button(`Edit revolution ${feature.id}`,()=>start(feature)));
      else if(solidToolKind(feature.operation))row.append(button(`Edit ${SOLID_TOOLS[solidToolKind(feature.operation)].name} ${feature.id}`,()=>start(feature)));
      else if(profile(feature))row.append(button(`View sketch ${feature.id}`,()=>showSketch(feature.id)));
      content.append(row);
    }
  }
  async function submit(apply){
    invalidate();
    let direction,revolution;
    if(kind==='extrude'){
      if(['up_to_next','up_to_face'].includes(extent))direction=limitDirectionSource==='preserve'?preserved:extrusionDirection(axis,quantity(1));
      else{
        const expression=source.startsWith('parameter:')?{parameter:source.slice(10)}:quantity(depth);
        direction=source==='preserve'?preserved:controlledDirection(axis,expression,preserved);
      }
    }else if(kind==='revolve'){
      const expression=source==='preserve'?savedRevolve.angle_radians:source.startsWith('parameter:')?{parameter:source.slice(10)}:quantity(angle*Math.PI/180,'scalar',null);
      revolution=axisSource==='preserve'?{origin:structuredClone(savedRevolve.origin),axis:structuredClone(savedRevolve.axis),angle_radians:expression,extent}:revolutionDefinition(origin,axis,expression,extent);
    }
    const finish={edges:finishEdges,size:finishSource==='preserve'?savedSize:finishSource.startsWith('parameter:')?{parameter:finishSource.slice(10)}:quantity(finishSize)};
    const changes=solidChanges(data.features,{mode,id,toolOperation:isSolidTool(kind)?solidToolOperation(kind,tool,data.features):null,input:kind==='hole'?holeControls.state.input:isEdgeFinish(kind)?finishTarget:isSolidTool(kind)?tool.source:input,target,hole:kind==='hole'?holeControls.definition():null,finish,direction,extent:kind==='extrude'?extrusionExtent(extent,limitTarget,limitFace,savedFace):extent,editing,kind,revolution,path,loft:kind==='profile_loft'?{...loft,...loftEnds.definition(),...loftCorrespondence.definition(loft)}:loft,orientation:kind==='sweep'?sweepOrientation(orientation,binormal,savedOrientation):undefined,sweepOptions:kind==='sweep'?sweepOptionsDefinition(sweepState,data.features,{input,path,orientation:sweepOrientation(orientation,binormal,savedOrientation)}):{}});
    const output=editing?solidEditOutput(data,editing):id,selectedInstance=instance();busy=true;content.inert=true;launch.disabled=true;message('Regenerating solid and checking requirements…');
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
