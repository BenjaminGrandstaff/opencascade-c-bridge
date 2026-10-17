// Advanced sweep options in the solid editor (schema 175): a twist about the
// path, a scale law, a guide rail and further sections. Pure state helpers
// plus one renderer that uses the editor's own select/number/button rows.
const scalar = value => ({literal:{value,dimension:'scalar',unit:null}});
const DEGREE = Math.PI/180;
const isPath = f => ['sketch_open_wire','sketch_wire','helix'].some(key=>Object.hasOwn(f.operation,key));
const isProfile = f => ['sketch_face','sketch_wire','planar_region'].some(key=>Object.hasOwn(f.operation,key));

/** An expression's editor source: entered, a parameter, or a kept formula. */
function source(expression) {
  if(!expression)return {source:'none'};
  if(expression.parameter)return {source:`parameter:${expression.parameter}`};
  if(expression.literal?.dimension==='scalar')return {source:'literal',value:expression.literal.value};
  return {source:'preserve',saved:expression};
}

/** Editor state for a sweep definition (or a new sweep). */
export function readSweepOptions(sweep={}) {
  const twist=source(sweep.twist?.angle_radians),scale=source(sweep.scale?.end);
  return {
    twist:{...twist,degrees:twist.source==='literal'?twist.value/DEGREE:90,law:sweep.twist?.law??'linear'},
    scale:{...scale,end:scale.source==='literal'?scale.value:2,law:sweep.scale?.law??'linear'},
    guide:{input:sweep.guide?.input??'',scales:Boolean(sweep.guide?.scales)},
    sections:(sweep.sections??[]).map(section=>{const at=source(section.position);return {profile:section.profile,position:at.source==='literal'?at.value:NaN,saved:at.source==='literal'?null:section.position};}),
  };
}

function expression(option,entered,what) {
  if(option.source==='preserve')return structuredClone(option.saved);
  if(option.source.startsWith('parameter:'))return {parameter:option.source.slice(10)};
  if(!Number.isFinite(entered))throw new Error(`Enter a finite ${what}.`);
  return scalar(entered);
}

/**
 * The sweep fields for the editor state: `twist`, `scale`, `guide` and
 * `sections` when set. Mirrors the native rules so conflicts read clearly
 * before preview: a twist or guide needs least-twist orientation; a twist
 * excludes a guide; sections exclude a scale and a scaling guide; section
 * positions increase within (0, 1] and the last is 1.
 */
export function sweepOptionsDefinition(state,features,{input,path,orientation}) {
  const fields={};
  if(state.twist.source!=='none'){
    if(state.twist.source==='literal'&&(!Number.isFinite(state.twist.degrees)||state.twist.degrees===0))throw new Error('Enter a nonzero twist angle.');
    fields.twist={angle_radians:expression(state.twist,state.twist.degrees*DEGREE,'twist angle'),law:state.twist.law};
  }
  if(state.scale.source!=='none'){
    if(state.scale.source==='literal'&&!(state.scale.end>0))throw new Error('The end scale must be positive.');
    fields.scale={end:expression(state.scale,state.scale.end,'end scale'),law:state.scale.law};
  }
  if(state.guide.input){
    if(!features.some(f=>f.id===state.guide.input&&isPath(f))||[input,path].includes(state.guide.input))throw new Error('Choose a guide rail separate from the profile and path.');
    fields.guide={input:state.guide.input,scales:state.guide.scales};
  }
  if(state.sections.length){
    let previous=0;
    fields.sections=state.sections.map(section=>{
      if(!features.some(f=>f.id===section.profile&&isProfile(f))||section.profile===input)throw new Error('Choose a separate profile for every section.');
      if(!section.saved&&!(section.position>previous&&section.position<=1))throw new Error('Section positions must increase along the path, from above 0 up to 1.');
      previous=section.saved?previous:section.position;
      return {profile:section.profile,position:section.saved?structuredClone(section.saved):scalar(section.position)};
    });
    const last=state.sections.at(-1);
    if(!last.saved&&last.position!==1)throw new Error('The last section must sit at the path end (position 1).');
  }
  if((fields.twist||fields.guide)&&orientation!=='corrected_frenet')throw new Error('A twist or guide rail uses the least-twist orientation.');
  if(fields.twist&&fields.guide)throw new Error('A guide rail sets the turning; remove the twist or the guide.');
  if(fields.sections&&(fields.scale||fields.guide?.scales))throw new Error('Sections set the size along the path; remove the scale or the guide’s scaling.');
  if(fields.scale&&fields.guide?.scales)throw new Error('A scaling guide sets the size; remove the scale law.');
  return fields;
}

/** The option rows, after the editor's orientation row. */
export function renderSweepOptions({state,features,parameters,input,path,select,number,button,content,render}) {
  const scalars=parameters.filter(p=>p.parameter_type?.scalar==='scalar');
  const sources=(option,entered,unit)=>{
    const choices=[['none','None'],['literal',entered],...scalars.map(p=>[`parameter:${p.id}`,`Parameter${unit}: ${p.id}`])];
    if(option.source.startsWith('parameter:')&&!choices.some(([key])=>key===option.source))choices.push([option.source,`Existing parameter: ${option.source.slice(10)}`]);
    if(option.source==='preserve')choices.push(['preserve','Keep existing formula']);
    return choices;
  };
  const laws=[['linear','Linear along the path'],['smooth','Smooth (level at both ends)']];
  const note=document.createElement('p');
  note.textContent='Twist turns the profile about the path; scale grows or shrinks it about the path; a guide rail turns it to face a second curve and can resize it to keep touching. Sections are further profiles blended along the path, the last at its end; sketch them on planes normal to the path so they follow its constraints.';
  content.append(note);
  select('Twist',sources(state.twist,'Entered angle',' (radians)'),state.twist.source,value=>{state.twist.source=value;render();});
  if(state.twist.source!=='none'){
    number('Twist angle (degrees)',state.twist.degrees,value=>state.twist.degrees=value).disabled=state.twist.source!=='literal';
    select('Twist law',laws,state.twist.law,value=>state.twist.law=value);
  }
  select('Scale along path',sources(state.scale,'Entered end scale',''),state.scale.source,value=>{state.scale.source=value;render();});
  if(state.scale.source!=='none'){
    number('End scale (× the profile)',state.scale.end,value=>state.scale.end=value).disabled=state.scale.source!=='literal';
    select('Scale law',laws,state.scale.law,value=>state.scale.law=value);
  }
  const rails=features.filter(f=>isPath(f)&&f.id!==input&&f.id!==path);
  if(state.guide.input&&!rails.some(f=>f.id===state.guide.input))rails.push({id:state.guide.input});
  select('Guide rail',[['','None'],...rails.map(f=>[f.id,f.id])],state.guide.input,value=>{state.guide.input=value;render();});
  if(state.guide.input)select('Guide scaling',[['turn','Turn to face the rail'],['scale','Turn and resize to keep touching the rail']],state.guide.scales?'scale':'turn',value=>state.guide.scales=value==='scale');
  const profiles=features.filter(f=>isProfile(f)&&f.id!==input);
  state.sections.forEach((section,index)=>{
    const choices=profiles.map(f=>[f.id,f.id]);
    if(section.profile&&!choices.some(([key])=>key===section.profile))choices.push([section.profile,section.profile]);
    select(`Section ${index+1} profile`,[['','Choose profile'],...choices],section.profile,value=>section.profile=value);
    if(section.saved){const kept=document.createElement('p');kept.textContent=`Section ${index+1} keeps its position formula.`;content.append(kept);}
    else number(`Section ${index+1} position (0–1 along the path)`,section.position,value=>section.position=value);
    content.append(button(`Remove section ${index+1}`,()=>{state.sections.splice(index,1);render();}));
  });
  content.append(button('Add section',()=>{state.sections.push({profile:'',position:1,saved:null});render();}));
}
