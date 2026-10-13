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
    ids.push(...(definition.profile??[]),...(definition.holes??[]).flat(),...(definition.regions??[]).flatMap(r=>[...r.profile,...(r.holes??[]).flat()]),...(definition.region_discovery?.entities??[]));
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
  if(type==='fillet'||type==='chamfer'){
    if(!fields.point?.trim())throw new Error('Choose a profile corner point.');
    const data={point:fields.point};
    for(const key of type==='fillet'?['radius']:['incoming','outgoing']){
      const value=expression(fields[key],'length');
      if(value!==null&&value<=0)throw new Error('Corner distances must be positive.');
      data[key]=structuredClone(fields[key]);
    }
    return {[type]:data};
  }
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

export function makeCurveOperation(type,fields,definition,editing=null){
  if(!fields.entity?.trim())throw new Error('Choose a source curve.');
  if(type==='extend_to'){
    const limit=expression(fields.limit,'length');
    if(limit!==null&&limit<=1e-6)throw new Error('Extension limit must exceed 0.000001 mm.');
    if(!['start','end'].includes(fields.end)||!fields.target||fields.target===fields.entity)throw new Error('Choose an end and a different target curve.');
    const data={entity:fields.entity,end:fields.end,target:fields.target,limit:structuredClone(fields.limit)};
    if(fields.near)data.near=fields.near;return {extend_to:data};
  }
  if(type!=='split')throw new Error('Choose a curve operation.');
  const at=fields.at;
  if(!at?.point&&!at?.crossing)throw new Error('Split at an interior point or crossing.');
  // Reuse the native-bound contract and self-crossing guard.
  const bound=makeProfileOperation('trim_between',{entity:fields.entity,from:'start',to:at}).trim_between.to;
  const reserved=profileIds({...definition,curve_operations:(definition.curve_operations??[]).filter((_,i)=>i!==editing),profile:[],holes:[],regions:[],region_discovery:null},'curves');
  if(!fields.before?.trim()||!fields.after?.trim()||fields.before===fields.after||[fields.before,fields.after].some(id=>reserved.includes(id)))throw new Error('Split pieces need two distinct unused names.');
  return {split:{entity:fields.entity,at:bound,before:fields.before,after:fields.after}};
}
export function replaceSplitBoundaryReferences(definition,split){
  const replace=ids=>ids.flatMap(id=>id===split.entity?[split.before,split.after]:[id]);
  definition.profile=replace(definition.profile??[]);
  if(definition.holes)definition.holes=definition.holes.map(replace);
  for(const region of definition.regions??[]){region.profile=replace(region.profile);if(region.holes)region.holes=region.holes.map(replace);}
  if(definition.region_discovery?.entities)definition.region_discovery.entities=replace(definition.region_discovery.entities);
}
export function updateDiscoverySelection(definition,{mode,seeds,entities,merge_adjacent}){
  if(!definition.region_discovery)throw new Error('This sketch uses explicit boundaries. Configure discovery in the full definition first.');
  if(!['all','selected'].includes(mode)||mode==='selected'&&!seeds.length)throw new Error('Choose at least one seed, or explicitly choose all bounded cells.');
  if(mode==='selected'&&seeds.some(id=>!profileIds(definition,'points').includes(id))||entities.some(id=>!profileIds(definition,'curves').includes(id)))throw new Error('Choose known seed points and arrangement curves.');
  definition.region_discovery={...definition.region_discovery,seeds:mode==='all'?[]:[...new Set(seeds)],entities:[...new Set(entities)],merge_adjacent};
}

export function createProfileControls({host,definition,parameters=[],changed,message,highlight=()=>{},beginPicking=()=>{},initialType='trim_between',initialScope=null,initialSeed=''}){
  const supported=['trim_between','trim','fillet','chamfer','offset','split','extend_to'];
  let type=supported.includes(initialType)?initialType:'trim_between',editing=null,editCollection='profile',values={},picking=null,scope='profile',replacePieces=true;
  const section=document.createElement('section');section.className='profile-tools';
  const heading=document.createElement('h3');heading.textContent='Curve & region edits';
  const note=document.createElement('p');note.textContent='Source edits run before boundaries are built. Corner and offset edits can target the outer profile or the material region containing a named seed. Preview checks all dependent geometry.';
  const select=document.createElement('select');select.setAttribute('aria-label','Profile operation');
  for(const [id,label] of [['trim_between','Trim to points / crossings'],['trim','Trim by parameter fractions'],['split','Split source curve'],['extend_to','Extend to curve crossing'],['fillet','Fillet profile corner'],['chamfer','Chamfer profile corner'],['offset','Offset profile / material region']])select.append(new Option(label,id));
  const form=document.createElement('div'),list=document.createElement('div');section.append(heading,note,select,form,list);host.append(section);
  function options(label,choices,value,change,mount=form){
    const row=document.createElement('label');row.textContent=label;
    const input=document.createElement('select');input.setAttribute('aria-label',label);
    if(value&&!choices.some(([id])=>id===value))choices=[...choices,[value,`Existing: ${value}`]];
    for(const [id,caption] of choices)input.append(new Option(caption,id));input.value=value;
    input.onchange=()=>change(input.value);row.append(input);mount.append(row);return input;
  }
  const ids=(group)=>[['','Choose geometry'],...profileIds(definition,group).map(id=>[id,id])];
  function pickButton(label,group,set,exclude=null,mount=form){
    const button=document.createElement('button');button.textContent=`Pick ${label} on canvas`;
    button.onclick=()=>{beginPicking();picking={label,group,set,exclude};message(`Click ${group==='points'?'a named point':'a curve'} for ${label}. Escape cancels.`);};mount.append(button);
  }
  function text(label,value,change,mount=form,type='text'){
    const row=document.createElement('label');row.textContent=label;const input=document.createElement('input');input.type=type;input.setAttribute('aria-label',label);
    if(type==='checkbox')input.checked=value;else input.value=value;
    input.onchange=()=>change(type==='checkbox'?input.checked:input.value);row.append(input);mount.append(row);return input;
  }
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
  function boundControl(label,key,endpoints=true){
    const b=values[key],kind=typeof b==='string'?b:b?.point?'point':'crossing';
    options(`${label} bound`,[...(endpoints?[['start','Curve start'],['end','Curve end']]:[]),['point','Named point'],['crossing','Exact crossing']],kind,next=>{
      values[key]=next==='point'?{point:{point:''}}:next==='crossing'?{crossing:{curve:'',near:''}}:next;renderForm();
    });
    if(kind==='point'){
      options(`${label} point`,ids('points'),b.point.point,next=>{b.point.point=next;highlight([values.entity,next]);});
      pickButton(`${label.toLowerCase()} point`,'points',id=>b.point.point=id);
    }
    if(kind==='crossing'){
      options(`${label} crossing curve`,ids('curves').filter(([id])=>id!==values.entity),b.crossing.curve,next=>{b.crossing.curve=next;highlight([values.entity,next]);});
      options(`${label} hint point`,ids('points'),b.crossing.near,next=>b.crossing.near=next);
      pickButton(`${label.toLowerCase()} crossing curve`,'curves',id=>b.crossing.curve=id,values.entity);
      pickButton(`${label.toLowerCase()} hint point`,'points',id=>b.crossing.near=id);
    }
  }
  function reset(next,index=null,collection='profile'){
    picking=null;type=next;editing=index;editCollection=collection;select.value=type;
    scope=collection==='region'?'region':index===null?(initialScope??(definition.region_discovery?'region':'profile')):'profile';
    const operation=collection==='curve'?definition.curve_operations?.[index]:collection==='region'?definition.region_edits?.[index[0]]?.operations[index[1]]:definition.profile_operations?.[index];
    values=index!==null?structuredClone(operation[type]):type==='offset'?{distance:{literal:{value:1,dimension:'length',unit:'millimeter'}},join:'arc'}:
      type==='split'?{entity:profileIds(definition,'curves')[0]??'',at:{point:{point:''}},before:'',after:''}:
      type==='extend_to'?{entity:profileIds(definition,'curves')[0]??'',end:'end',target:'',limit:{literal:{value:10,dimension:'length',unit:'millimeter'}}}:
      type==='fillet'?{point:'',radius:{literal:{value:1,dimension:'length',unit:'millimeter'}}}:
      type==='chamfer'?{point:'',incoming:{literal:{value:1,dimension:'length',unit:'millimeter'}},outgoing:{literal:{value:1,dimension:'length',unit:'millimeter'}}}:
      type==='trim'?{entity:profileIds(definition,'curves')[0]??'',first:{literal:{value:0,dimension:'scalar',unit:null}},last:{literal:{value:1,dimension:'scalar',unit:null}}}:
      {entity:profileIds(definition,'curves')[0]??'',from:'start',to:'end'};
    if(collection==='region')values.seed=definition.region_edits[index[0]].seed;
    else if(index===null)values.seed=initialSeed;
    if(type==='offset')values.join??='arc';renderForm();
  }
  function renderForm(){
    picking=null;
    form.replaceChildren();
    if(['trim','trim_between','split','extend_to'].includes(type)){
      const caption=type==='split'||type==='extend_to'?'Source curve':'Trim curve';
      options(caption,ids('curves'),values.entity,next=>{values.entity=next;highlight([next]);renderForm();});
      pickButton(caption.toLowerCase(),'curves',id=>values.entity=id);
    }
    if(['fillet','chamfer','offset'].includes(type)){
      options('Boundary target',[['profile','Outer profile'],['region','Material region at seed']],scope,next=>{scope=next;renderForm();});
      if(scope==='region'){
        options('Region seed',ids('points'),values.seed??'',next=>values.seed=next);
        pickButton('region seed','points',id=>values.seed=id);
      }
    }
    if(type==='split'){
      boundControl('Split','at',false);text('Before piece name',values.before,next=>values.before=next);text('After piece name',values.after,next=>values.after=next);
      text('Use split pieces in explicit boundaries',replacePieces,next=>replacePieces=next,form,'checkbox');
    }
    if(type==='extend_to'){
      options('Curve end',[['start','Start'],['end','End']],values.end,next=>values.end=next);
      options('Extension target',ids('curves').filter(([id])=>id!==values.entity),values.target,next=>values.target=next);
      pickButton('extension target','curves',id=>values.target=id,values.entity);valueControl('Extension limit (mm)','limit','length');
      options('Extension hint (optional)',ids('points'),values.near??'',next=>values.near=next);
      pickButton('extension hint','points',id=>values.near=id);
    }
    if(type==='fillet'||type==='chamfer'){
      options('Corner point',ids('points'),values.point,next=>{values.point=next;highlight([next]);});
      pickButton('corner point','points',id=>values.point=id);
      if(type==='fillet')valueControl('Fillet radius (mm)','radius','length');
      else{
        const hint=document.createElement('p');hint.textContent='Incoming and outgoing distances follow the directed profile traversal.';form.append(hint);
        valueControl('Incoming distance (mm)','incoming','length');valueControl('Outgoing distance (mm)','outgoing','length');
      }
    }
    if(type==='trim_between'){boundControl('From','from');boundControl('To','to');}
    if(type==='trim'){valueControl('First fraction','first','scalar');valueControl('Last fraction','last','scalar');}
    if(type==='offset'){
      valueControl('Offset distance (mm)','distance','length');
      options('Offset join',[['arc','Round joins'],['intersection','Intersect adjacent curves']],values.join,next=>values.join=next);
    }
    const submit=document.createElement('button');submit.textContent=editing===null?'Add profile operation':'Update profile operation';
    submit.onclick=()=>{try{
      if(type==='split'||type==='extend_to'){
        const index=editCollection==='curve'?editing:null,op=makeCurveOperation(type,values,definition,index),ops=definition.curve_operations??=[];
        if(index===null)ops.push(op);else ops[index]=op;
        if(type==='split'&&replacePieces)replaceSplitBoundaryReferences(definition,op.split);
        changed();message('Source curve edit updated. Preview checks references and closure. Removing or renaming pieces requires updating their references.');return;
      }
      const op=makeProfileOperation(type,values);
      if(scope==='region'){
        if(!values.seed||!profileIds(definition,'points').includes(values.seed))throw new Error('Choose a named point strictly inside the material region.');
        const entries=definition.region_edits??=[];
        if(editCollection==='region'&&editing!==null){
          const entry=entries[editing[0]];
          if(entry.seed!==values.seed&&entry.operations.length>1)throw new Error('A shared region entry keeps its seed. Add a new region operation to target a different region.');
          entry.seed=values.seed;entry.operations[editing[1]]=op;
        }else entries.push({seed:values.seed,operations:[op]});
        if(editing!==null&&editCollection==='profile')definition.profile_operations.splice(editing,1);
        changed();message('Region operation updated. Preview checks the seed, material boundaries and dependent solids.');return;
      }
      const ops=definition.profile_operations??=[];
      if(editCollection==='region'&&editing!==null)throw new Error('Remove the region operation before adding it to the outer profile.');
      if(editing===null){
        // Source trims must precede offsets and corner edits.
        const boundary=ops.findIndex(op=>type==='fillet'||type==='chamfer'?op.offset:op.offset||op.fillet||op.chamfer);
        ops.splice(type!=='offset'&&boundary>=0?boundary:ops.length,0,op);
      }else ops[editing]=op;
      changed();message('Profile operation updated in the draft. Preview checks closure and dependent solids.');
    }catch(error){message(error.message);}};form.append(submit);
    if(editing!==null){const cancel=document.createElement('button');cancel.textContent='New profile operation';cancel.onclick=()=>reset(type);form.append(cancel);}
  }
  for(const [index,op] of (definition.profile_operations??[]).entries()){
    const key=Object.keys(op)[0],data=op[key],row=document.createElement('div');
    const edit=document.createElement('button');edit.textContent=`${index+1}. ${key} · ${data.entity??data.point??'outer profile'}`;
    edit.disabled=!supported.includes(key);edit.onclick=()=>{reset(key,index);highlight(data.entity?[data.entity]:data.point?[data.point]:[]);};
    const remove=document.createElement('button');remove.textContent=`Remove profile operation ${index+1}`;remove.onclick=()=>{definition.profile_operations.splice(index,1);changed();message('Profile operation removed from the draft. Preview before applying.');};
    row.append(edit,remove);list.append(row);
  }
  for(const [index,op] of (definition.curve_operations??[]).entries()){
    const key=Object.keys(op)[0],row=document.createElement('div'),edit=document.createElement('button'),remove=document.createElement('button');
    edit.textContent=`Source ${index+1}. ${key} · ${op[key].entity}`;edit.onclick=()=>reset(key,index,'curve');
    remove.textContent=`Remove source operation ${index+1}`;remove.onclick=()=>{definition.curve_operations.splice(index,1);changed();message('Source operation removed. Update references to its pieces before applying.');};row.append(edit,remove);list.append(row);
  }
  for(const [entryIndex,entry] of (definition.region_edits??[]).entries())for(const [opIndex,op] of entry.operations.entries()){
    const key=Object.keys(op)[0],row=document.createElement('div'),edit=document.createElement('button'),remove=document.createElement('button');
    edit.textContent=`Region ${entryIndex+1}.${opIndex+1}. ${key} · ${entry.seed}`;edit.onclick=()=>reset(key,[entryIndex,opIndex],'region');
    remove.textContent=`Remove region operation ${entryIndex+1}.${opIndex+1}`;remove.onclick=()=>{entry.operations.splice(opIndex,1);if(!entry.operations.length)definition.region_edits.splice(entryIndex,1);changed();message('Region operation removed from the draft.');};row.append(edit,remove);list.append(row);
  }
  if(definition.region_discovery){
    const discovery=document.createElement('details'),title=document.createElement('summary');title.textContent='Discovered region selection';discovery.append(title);section.append(discovery);
    let mode=definition.region_discovery.seeds?.length?'selected':'all',seeds=[...(definition.region_discovery.seeds??[])],entities=[...(definition.region_discovery.entities??[])],merge=definition.region_discovery.merge_adjacent??false;
    const parse=text=>text.split(',').map(id=>id.trim()).filter(Boolean);
    const modeControl=options('Region selection mode',[['all','All bounded cells (fills inner cells)'],['selected','Only cells containing selected seeds']],mode,next=>mode=next,discovery);
    const seedControl=text('Selected region seeds (comma separated)',seeds.join(', '),next=>seeds=parse(next),discovery);
    text('Arrangement curves (empty uses all)',entities.join(', '),next=>entities=parse(next),discovery);
    text('Merge adjacent selected cells',merge,next=>merge=next,discovery,'checkbox');
    pickButton('discovery seed','points',id=>{seeds=[...new Set([...seeds,id])];seedControl.value=seeds.join(', ');mode='selected';modeControl.value=mode;},null,discovery);
    const apply=document.createElement('button');apply.textContent='Update region selection';apply.onclick=()=>{try{updateDiscoverySelection(definition,{mode,seeds,entities,merge_adjacent:merge});changed();message('Region selection changed in the draft. Preview checks seeds and material.');}catch(error){message(error.message);}};discovery.append(apply);
  }
  select.onchange=()=>reset(select.value);reset(type);
  return {
    get type(){return type;},
    get scope(){return scope;},get seed(){return values.seed??'';},
    cancelPick(){if(picking){picking=null;message('Profile pick canceled. Form values retained.');}},
    pick(id){
      if(!picking)return false;
      if(id===picking.exclude||!profileIds(definition,picking.group).includes(id)){
        message(`Choose ${picking.group==='points'?'a named point':'a different named curve'} for ${picking.label}.`);return true;
      }
      const label=picking.label;picking.set(id);renderForm();highlight([values.entity??values.point,id].filter(Boolean));
      message(`Selected ${id} for ${label}. ${label==='discovery seed'?'Update region selection':'Add or update the operation'}, then Preview.`);return true;
    },
  };
}
