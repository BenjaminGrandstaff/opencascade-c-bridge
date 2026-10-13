import {geometryIds,dimensionNumber} from './constraint-controls.mjs';

// These controls author native operations, never sampled browser geometry.
export function profileIds(definition, group) {
  const ids=geometryIds(definition,group);
  for(const shape of definition.shapes??[]){
    const [kind,data]=Object.entries(shape)[0];
    const suffixes=kind==='rectangle'?(group==='points'?['corner1','corner3']:['side0','side1','side2','side3']):
      kind==='slot'?(group==='points'?['start_right','end_right','end_left','start_left']:['right','end_cap','left','start_cap']):
      kind==='polygon'?(group==='points'?Array.from({length:data.sides-1},(_,i)=>`vertex${i+1}`):[...Array.from({length:data.sides},(_,i)=>`side${i}`),'circle']):[];
    ids.push(...suffixes.map(s=>`${data.id}:${s}`));
    if(kind==='conic'&&group==='curves')ids.push(data.id);
  }
  if(group==='points')for(const p of definition.projections??[]){
    ids.push(...(p.intersections??[]).map(i=>`${p.id}:intersection:${i.id}`),...(p.junctions??[]).map(j=>`${p.id}:junction:${j.id}`));
  }
  if(group==='curves'){
    for(const op of definition.curve_operations??[])if(op.split)ids.push(op.split.before,op.split.after);
    ids.push(...(definition.profile??[]),...(definition.holes??[]).flat(),...(definition.region_discovery?.curves??[]));
  }
  return [...new Set(ids)];
}
function expression(value, dimension) {
  if(!value||typeof value!=='object'||Array.isArray(value))throw new Error('Enter a driving value.');
  if(value.literal){
    const number=dimension==='length'?dimensionNumber(value,'length'):value.literal.dimension==='scalar'?value.literal.value:null;
    if(number===null||!Number.isFinite(number))throw new Error('Enter a finite value in the correct units.');
    return number;
  }
  return null; // Native expression validation is authoritative.
}
export function makeProfileOperation(type, fields) {
  if(type==='offset'){
    expression(fields.distance,'length');
    if(!['arc','intersection'].includes(fields.join))throw new Error('Choose an offset join.');
    return {offset:{distance:structuredClone(fields.distance),join:fields.join}};
  }
  if(!fields.entity?.trim())throw new Error('Choose a curve to trim.');
  if(type==='trim'){
    const first=expression(fields.first,'scalar'),last=expression(fields.last,'scalar');
    if(first!==null&&(first<0||first>1)||last!==null&&(last<0||last>1)||first!==null&&last!==null&&first>=last)throw new Error('Trim fractions must satisfy 0 ≤ first < last ≤ 1.');
    return {trim:structuredClone({entity:fields.entity,first:fields.first,last:fields.last})};
  }
  if(type!=='trim_between')throw new Error('Choose a supported profile operation.');
  const bound=b=>{
    if(b==='start'||b==='end')return b;
    if(b?.point?.point?.trim())return {point:{point:b.point.point}};
    if(b?.crossing?.curve?.trim()&&b.crossing.near?.trim()){
      if(b.crossing.curve===fields.entity)throw new Error('A crossing needs a different curve.');
      return {crossing:{curve:b.crossing.curve,near:b.crossing.near}};
    }
    throw new Error('Choose a point, or a crossing curve and nearby hint point.');
  };
  const from=bound(fields.from),to=bound(fields.to);
  if(JSON.stringify(from)===JSON.stringify(to))throw new Error('Choose distinct trim bounds.');
  return {trim_between:{entity:fields.entity,from,to}};
}

export function createProfileControls({host,definition,parameters=[],changed,message,highlight=()=>{},initialType='trim_between'}){
  let type=['trim_between','trim','offset'].includes(initialType)?initialType:'trim_between',editing=null,values={};
  const section=document.createElement('section');section.className='profile-tools';
  const heading=document.createElement('h3');heading.textContent='Trim & offset';
  const note=document.createElement('p');note.textContent='Edits derive the outer profile after solving; source geometry and constraints stay authored. Trim keeps the directed span. Positive offset expands a closed profile. Additional regions and holes use the advanced definition.';
  const select=document.createElement('select');select.setAttribute('aria-label','Profile operation');
  for(const [id,label] of [['trim_between','Trim to points / crossings'],['trim','Trim by parameter fractions'],['offset','Offset outer profile']])select.append(new Option(label,id));
  const form=document.createElement('div'),list=document.createElement('div');section.append(heading,note,select,form,list);host.append(section);
  function options(label,choices,value,change){
    const row=document.createElement('label');row.textContent=label;
    const input=document.createElement('select');input.setAttribute('aria-label',label);
    if(value&&!choices.some(([id])=>id===value))choices=[...choices,[value,`Existing: ${value}`]];
    for(const [id,caption] of choices)input.append(new Option(caption,id));input.value=value;
    input.onchange=()=>change(input.value);row.append(input);form.append(row);return input;
  }
  const ids=(group)=>[['','Choose geometry'],...profileIds(definition,group).map(id=>[id,id])];
  function valueControl(label,key,dimension){
    const preserved=values[key],number=dimension==='length'?dimensionNumber(preserved,'length'):preserved?.literal?.value;
    let source=preserved?.parameter?`parameter:${preserved.parameter}`:number!==null&&number!==undefined?'literal':'preserve';
    const choices=[['literal','Entered value'],...parameters.filter(p=>p.parameter_type?.scalar===dimension).map(p=>[`parameter:${p.id}`,`Parameter: ${p.id}`])];
    if(source==='preserve')choices.push(['preserve','Keep existing formula']);
    const row=document.createElement('label');row.textContent=label;
    const input=document.createElement('input');input.type='number';input.step='any';input.value=number??0;input.setAttribute('aria-label',label);input.disabled=source!=='literal';
    const literal=()=>({literal:{value:input.value.trim()?Number(input.value):NaN,dimension,unit:dimension==='length'?'millimeter':null}});
    options(`${label} source`,choices,source,next=>{source=next;input.disabled=source!=='literal';values[key]=source==='preserve'?structuredClone(preserved):source==='literal'?literal():{parameter:source.slice(10)};});
    input.oninput=()=>{if(source==='literal')values[key]=literal();};row.append(input);form.append(row);
  }
  function boundControl(label,key){
    const b=values[key],kind=typeof b==='string'?b:b?.point?'point':'crossing';
    options(`${label} bound`,[['start','Curve start'],['end','Curve end'],['point','Named point'],['crossing','Exact crossing']],kind,next=>{
      values[key]=next==='point'?{point:{point:''}}:next==='crossing'?{crossing:{curve:'',near:''}}:next;renderForm();
    });
    if(kind==='point')options(`${label} point`,ids('points'),b.point.point,next=>{b.point.point=next;highlight([values.entity,next]);});
    if(kind==='crossing'){
      options(`${label} crossing curve`,ids('curves').filter(([id])=>id!==values.entity),b.crossing.curve,next=>{b.crossing.curve=next;highlight([values.entity,next]);});
      options(`${label} hint point`,ids('points'),b.crossing.near,next=>b.crossing.near=next);
    }
  }
  function reset(next,index=null){
    type=next;editing=index;select.value=type;
    values=index!==null?structuredClone(definition.profile_operations[index][type]):type==='offset'?{distance:{literal:{value:1,dimension:'length',unit:'millimeter'}},join:'arc'}:
      type==='trim'?{entity:profileIds(definition,'curves')[0]??'',first:{literal:{value:0,dimension:'scalar',unit:null}},last:{literal:{value:1,dimension:'scalar',unit:null}}}:
      {entity:profileIds(definition,'curves')[0]??'',from:'start',to:'end'};
    if(type==='offset')values.join??='arc';renderForm();
  }
  function renderForm(){
    form.replaceChildren();
    if(type!=='offset')options('Trim curve',ids('curves'),values.entity,next=>{values.entity=next;highlight([next]);renderForm();});
    if(type==='trim_between'){boundControl('From','from');boundControl('To','to');}
    if(type==='trim'){valueControl('First fraction','first','scalar');valueControl('Last fraction','last','scalar');}
    if(type==='offset'){
      valueControl('Offset distance (mm)','distance','length');
      options('Offset join',[['arc','Round joins'],['intersection','Intersect adjacent curves']],values.join,next=>values.join=next);
    }
    const submit=document.createElement('button');submit.textContent=editing===null?'Add profile operation':'Update profile operation';
    submit.onclick=()=>{try{
      const op=makeProfileOperation(type,values),ops=definition.profile_operations??=[];
      if(editing===null){
        // Source trims must precede offsets and corner edits.
        const boundary=ops.findIndex(op=>op.offset||op.fillet||op.chamfer);
        ops.splice(type!=='offset'&&boundary>=0?boundary:ops.length,0,op);
      }else ops[editing]=op;
      changed();message('Profile operation updated in the draft. Preview checks closure and dependent solids.');
    }catch(error){message(error.message);}};form.append(submit);
    if(editing!==null){const cancel=document.createElement('button');cancel.textContent='New profile operation';cancel.onclick=()=>reset(type);form.append(cancel);}
  }
  for(const [index,op] of (definition.profile_operations??[]).entries()){
    const key=Object.keys(op)[0],data=op[key],row=document.createElement('div');
    const edit=document.createElement('button');edit.textContent=`${index+1}. ${key} · ${data.entity??data.point??'outer profile'}`;
    edit.disabled=!['trim','trim_between','offset'].includes(key);edit.onclick=()=>{reset(key,index);highlight(data.entity?[data.entity]:[]);};
    const remove=document.createElement('button');remove.textContent=`Remove profile operation ${index+1}`;remove.onclick=()=>{definition.profile_operations.splice(index,1);changed();message('Profile operation removed from the draft. Preview before applying.');};
    row.append(edit,remove);list.append(row);
  }
  select.onchange=()=>reset(select.value);reset(type);
  return {get type(){return type;}};
}
