// Shell, draft, mirror and pattern features in the solid editor: the
// engine's hollow, draft, mirror, linear_pattern and circular_pattern
// operations on an accepted solid. Pure state helpers plus one renderer
// that uses the editor's own select/number/button rows.
const quantity = (value,dimension='length',unit='millimeter') => ({literal:{value,dimension,unit}});
const vector = (values,dimension) => ({components:Object.fromEntries(['x','y','z'].map((key,i)=>[key,quantity(values[i],dimension,dimension==='length'?'millimeter':null)]))});
const mm = q => q?.dimension==='length' ? q.value*({millimeter:1,centimeter:10,meter:1000,inch:25.4}[q.unit]??NaN) : NaN;
const DEGREE = Math.PI/180;

/** Tool kinds, their names, and whether they change their solid in place
 * (shell, draft) rather than making a new body that can join a target. */
export const SOLID_TOOLS = {
  hollow:{name:'shell',inPlace:true},
  draft:{name:'draft',inPlace:true},
  mirror:{name:'mirror',inPlace:false},
  linear_pattern:{name:'linear pattern',inPlace:false},
  circular_pattern:{name:'circular pattern',inPlace:false},
};
export const isSolidTool = kind => Object.hasOwn(SOLID_TOOLS,kind);
export const inPlaceTool = kind => SOLID_TOOLS[kind]?.inPlace===true;
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
  }else if(kind==='mirror'){
    Object.assign(state,{origin:vectorState(body?.origin,'length',[0,0,0]),normal:vectorState(body?.normal,'scalar',[1,0,0])});
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
  return state;
}

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
  if(kind==='mirror')return {mirror:{input,origin:keptVector(state.origin,'length','mirror plane point'),normal:keptVector(state.normal,'scalar','mirror plane normal',true)}};
  const count=scalar(state.count,'copy count (a whole number from 2 to 10000, including the original)',v=>quantity(v,'scalar',null),v=>Number.isInteger(v)&&v>=2&&v<=10000);
  if(kind==='linear_pattern'){
    let step;
    if(state.spacing.source==='preserve')step=structuredClone(state.spacing.saved);
    else{
      const unit=direction(state.direction,'pattern direction');
      if(!(state.spacing.value>0))throw Error('Enter a positive spacing.');
      step=vector(unit.map(v=>v*state.spacing.value),'length');
    }
    return {linear_pattern:{input,step,count}};
  }
  let angle;
  if(state.spacing==='full'){
    if(!count.literal)throw Error('An even spread round a full turn needs an entered count.');
    angle=quantity(2*Math.PI/count.literal.value,'scalar',null);
  }else angle=scalar(state.angle,'angle between copies',v=>quantity(v*DEGREE,'scalar',null),v=>v!==0&&Math.abs(v)<=360);
  return {circular_pattern:{input,origin:keptVector(state.origin,'length','axis point'),axis:keptVector(state.axis,'scalar','axis direction',true),count,angle_step_radians:angle}};
}

/** The tool's rows after the editor's name row. */
export function renderSolidTool({kind,state,features,parameters,select,number,button,content,render,pickFace,message,invalidate}) {
  const note=document.createElement('p');
  note.textContent={
    hollow:'Removes the picked faces and leaves walls of the given thickness, inside or outside the original surface. Pick planar faces of the source; Preview checks every instance.',
    draft:'Tapers the picked faces about the neutral plane. Positive angles remove material on the pull-direction side. Vectors use family coordinates.',
    mirror:'Reflects the source across the plane. Choose Add material with the source as target to make a symmetric part.',
    linear_pattern:'Copies the source along the direction at the spacing; the count includes the original. Add material with the source as target to join the copies.',
    circular_pattern:'Copies the source about the axis; the count includes the original. Add material with the source as target to join the copies.',
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
  if(kind==='hollow'||kind==='draft'){
    state.faces.forEach((_,index)=>{
      const row=document.createElement('p');
      const picked=state.picked[index];
      row.textContent=`Face ${index+1}: `+(picked?`picked planar face at ${(picked.origin_mm??[]).map(v=>Number(v.toPrecision(6))).join(', ')} mm`:'saved selector');
      content.append(row,button(`Remove face ${index+1}`,()=>{state.faces.splice(index,1);state.picked.splice(index,1);invalidate();render();}));
    });
    if(pickFace)content.append(button(kind==='hollow'?'Pick a face to open':'Pick a face to draft',async()=>{
      if(!state.source)throw Error('Choose a source solid first.');
      message('Click a planar face of the amber source solid. Escape cancels the pick.');
      const picked=await pickFace(state.source);
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
  }else if(kind==='mirror'){
    const principal={yz:[1,0,0],xz:[0,1,0],xy:[0,0,1]};
    const current=!state.normal.saved&&Object.entries(principal).find(([,n])=>n.every((v,i)=>v===state.normal.values[i]))?.[0];
    select('Mirror plane',[['yz','YZ (normal X)'],['xz','XZ (normal Y)'],['xy','XY (normal Z)'],['custom','Custom normal']],current||'custom',value=>{if(principal[value]){state.normal={values:[...principal[value]],saved:null};}else if(state.normal.saved===null&&current)state.normal.values=[1,1,0];render();});
    vectorRows('Plane point',state.origin,' (mm)');
    if(!current)vectorRows('Plane normal',state.normal,'');
  }else{
    select('Copy count source',sources(state.count,'Entered count',scalars,''),state.count.source,value=>{state.count.source=value;render();});
    number('Copies (including the original)',state.count.value,value=>state.count.value=value).disabled=state.count.source!=='literal';
    if(kind==='linear_pattern'){
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
