// Authored reference graph. Solver coupling is intentionally not inferred.
const geometryGroups=['points','lines','circles','arcs','ellipses','splines','projections','texts'];
const referenceKeys=new Set(['start','end','center','rim','major','minor','first','second','opposite','vertex','control','point','line','circle','arc','ellipse','spline','curve','entity','axis','direction','anchor','target','near','other','seed']);
const referenceArrays=new Set(['points','entities','profile','holes','on_axis','seeds']);
function references(value){
  const result=new Set(),stack=[value];
  while(stack.length){const v=stack.pop();if(!v||typeof v!=='object')continue;
    for(const [key,item]of Object.entries(v)){
      if(referenceKeys.has(key)&&typeof item==='string')result.add(item);
      else if(referenceArrays.has(key)&&Array.isArray(item)){
        const arrays=[item];while(arrays.length){for(const id of arrays.pop())if(typeof id==='string')result.add(id);else if(Array.isArray(id))arrays.push(id);}
      }else if(item&&typeof item==='object'&&!['literal','parameter','formula','basis','edge','face','near'].includes(key))stack.push(item);
    }
  }return result;
}
const unwrap=value=>Object.entries(value??{})[0]??['unknown',{}];
export function sketchDependencyGraph(definition,{features=[],feature='',featureDependencies={}}={}){
  const nodes=new Map(),geometry=new Map(),owners=[];
  const add=(key,label,kind,id=null)=>{if(!nodes.has(key))nodes.set(key,{key,label,kind,id,dependencies:new Set(),consumers:new Set()});return key;};
  const link=(consumer,source)=>{if(consumer!==source){nodes.get(consumer).dependencies.add(source);nodes.get(source).consumers.add(consumer);}};
  const geo=(id,label,kind)=>{const key=add(`geometry:${id}`,label,kind,id);geometry.set(id,key);return key;};
  for(const group of geometryGroups)for(const item of definition[group]??[]){
    geo(item.id,`${item.id} · ${group==='points'?'point':group==='projections'?'live projection':group.slice(0,-1)}`,group);
    if(group==='projections')owners.push({id:item.id,key:geometry.get(item.id),kind:'projection'});
  }
  for(const shape of definition.shapes??[]){const [kind,data]=unwrap(shape);owners.push({id:data.id,key:geo(data.id,`${data.id} · ${kind}`, 'shapes'),kind:'shape'});}
  for(const pattern of definition.patterns??[]){const [kind,data]=unwrap(pattern);owners.push({id:data.id,key:add(`pattern:${data.id}`,`${data.id} · ${kind} pattern`,'pattern'),kind:'pattern'});}
  owners.sort((a,b)=>b.id.length-a.id.length);
  function resolve(id){
    if(geometry.has(id))return geometry.get(id);
    const owner=owners.find(o=>id.startsWith(`${o.id}:`));
    const key=geo(id,`${id} · ${owner?'generated reference':'unresolved reference'}`,owner?'generated':'unresolved');
    if(owner){link(key,owner.key);if(owner.kind==='pattern'){
      const rest=id.slice(owner.id.length+1),match=rest.match(/^\d+:(.+)$/);if(match)link(key,resolve(match[1]));
    }}return key;
  }
  const consume=(key,value)=>{for(const id of references(value))link(key,resolve(id));};
  for(const group of geometryGroups)for(const item of definition[group]??[]){
    const key=geometry.get(item.id);if(group!=='projections')consume(key,item);
    else{
      link(key,add(`feature:${item.input}`,`${item.input} · source feature`,'feature'));
      const suffixes={line:['start','end'],circle:['center','rim'],arc:['center','start','end'],ellipse:['center','major','minor'],spline:['start','end'],chain:['start','end']};
      for(const suffix of suffixes[item.kind]??[])resolve(`${item.id}:${suffix}`);
      for(const junction of item.junctions??[])resolve(`${item.id}:junction:${junction.id}`);
      for(const crossing of item.intersections??[]){const point=resolve(`${item.id}:intersection:${crossing.id}`);link(point,resolve(crossing.other));}
    }
  }
  for(const shape of definition.shapes??[]){const [,data]=unwrap(shape);consume(geometry.get(data.id),data);}
  for(const pattern of definition.patterns??[]){const [,data]=unwrap(pattern);consume(`pattern:${data.id}`,data);}
  for(const [index,constraint]of (definition.constraints??[]).entries()){
    const [kind,data]=unwrap(constraint),key=add(`constraint:${index}`,`Constraint ${index+1} · ${kind.replaceAll('_',' ')}`,'constraint');consume(key,constraint);
    if(kind==='spline_point')link(resolve(data.point),resolve(data.spline));
    if(kind==='weighted_point')for(const term of data.terms??[])link(resolve(data.point),resolve(term.point));
  }
  for(const item of definition.measurements??[])consume(add(`measurement:${item.id}`,`${item.id} · reference dimension`,'measurement'),item.measure);
  const curves=()=>[...geometry.entries()].filter(([,key])=>['lines','circles','arcs','ellipses','splines','projections','shapes'].includes(nodes.get(key).kind)).map(([id])=>id);
  const profile=add('profile:outer','Outer profile','profile');
  consume(profile,{profile:definition.profile?.length?definition.profile:definition.region_discovery?[]:curves()});
  if(!definition.profile?.length&&!definition.region_discovery)for(const owner of owners)if(owner.kind==='pattern')link(profile,owner.key);
  for(const [index,hole]of (definition.holes??[]).entries())consume(add(`profile:hole:${index}`,`Hole ${index+1}`,'profile'),{profile:hole});
  for(const [index,region]of (definition.regions??[]).entries())consume(add(`profile:region:${index}`,`Region ${index+1}`,'profile'),region);
  if(definition.region_discovery){const key=add('profile:discovery','Discovered regions','profile');consume(key,{...definition.region_discovery,entities:definition.region_discovery.entities?.length?definition.region_discovery.entities:curves()});}
  for(const [index,operation]of (definition.curve_operations??[]).entries()){
    const [kind,data]=unwrap(operation),key=add(`curve-operation:${index}`,`Curve edit ${index+1} · ${kind.replaceAll('_',' ')}`,'edit');consume(key,operation);
    if(kind==='split')for(const id of [data.before,data.after])if(id)link(resolve(id),key);
  }
  for(const [index,operation]of (definition.profile_operations??[]).entries()){
    const [kind]=unwrap(operation),key=add(`profile-operation:${index}`,`Profile edit ${index+1} · ${kind.replaceAll('_',' ')}`,'edit');consume(key,operation);link(key,profile);
  }
  for(const [index,edit]of (definition.region_edits??[]).entries())consume(add(`region-edit:${index}`,`Region edit ${index+1}`,'edit'),edit);
  const sketch=add(`feature:${feature}`,`${feature||definition.id} · sketch feature`,'feature');
  // Any authored sketch change requires its feature and downstream features to regenerate.
  for(const node of nodes.values())if(node.key!==sketch&&node.kind!=='feature')link(sketch,node.key);
  if(definition.face_support)link(sketch,add(`feature:${definition.face_support.input}`,`${definition.face_support.input} · support feature`,'feature'));
  if(definition.datum_plane)link(sketch,add(`datum:${definition.datum_plane}`,`${definition.datum_plane} · support datum`,'datum'));
  const featureIds=new Set(features.map(f=>f.id)),featureKeys=new Set(['input','object','tool','left','right','feature','source_feature','source']);
  for(const f of features){
    const key=add(`feature:${f.id}`,`${f.id} · ${f.id===feature?'sketch':'model'} feature`,'feature');if(f.id===feature)continue;
    if(Object.hasOwn(featureDependencies,f.id)){for(const id of featureDependencies[f.id])link(key,add(`feature:${id}`,`${id} · model feature`,'feature'));continue;}
    const stack=[f.operation];while(stack.length){const v=stack.pop();if(!v||typeof v!=='object')continue;for(const [field,item]of Object.entries(v)){
      if(featureKeys.has(field)&&typeof item==='string'&&featureIds.has(item))link(key,add(`feature:${item}`,`${item} · model feature`,'feature'));
      else if(item&&typeof item==='object')stack.push(item);
    }}
  }
  return {nodes,geometry,resolveGenerated(id){
    if(geometry.has(id))return geometry.get(id);
    if(!owners.some(o=>id.startsWith(`${o.id}:`)))return null;
    const key=resolve(id);link(sketch,key);return key;
  }};
}
export function dependencyReach(graph,key,direction='consumers'){
  const seen=new Set([key]),queue=[key],result=[];
  for(let i=0;i<queue.length;i++)for(const next of graph.nodes.get(queue[i])?.[direction]??[]){if(!seen.has(next)){seen.add(next);queue.push(next);result.push(next);}}
  return result;
}
export function createDependencyControls({host,definition,features,feature,featureDependencies,initialSelection=null,initialOpen=false,highlight=()=>{},beginPicking=()=>{},message=()=>{}}){
  const graph=sketchDependencyGraph(definition,{features,feature,featureDependencies}),section=document.createElement('details'),heading=document.createElement('summary');heading.textContent='Geometry dependencies';section.open=initialOpen;section.append(heading);host.append(section);
  const label=document.createElement('label');label.textContent='Inspect geometry dependencies';const choice=document.createElement('select');choice.setAttribute('aria-label',label.textContent);
  if(initialSelection)graph.resolveGenerated(initialSelection);
  for(const [id,key]of graph.geometry)choice.append(new Option(graph.nodes.get(key).label,id));label.append(choice);section.append(label);
  let selected=graph.geometry.has(initialSelection)?initialSelection:graph.geometry.keys().next().value,picking=false;
  const results=document.createElement('div');results.setAttribute('aria-live','polite');
  const button=(text,action,parent=section)=>{const b=document.createElement('button');b.textContent=text;b.onclick=action;parent.append(b);return b;};
  button('Inspect dependencies from canvas',()=>{beginPicking();picking=true;message('Click a sketch point or curve to inspect its authored references.');});
  const note=document.createElement('p');note.textContent='References in the current draft. Constraints can move other geometry during solving; this view does not predict that motion. Every sketch edit also regenerates downstream model features.';section.append(note,results);
  function highlightNode(key){const node=graph.nodes.get(key);const ids=node.id?[node.id]:dependencyReach(graph,key,'dependencies').map(k=>graph.nodes.get(k).id).filter(Boolean).slice(0,100);highlight(ids);}
  function render(){
    choice.value=selected??'';results.replaceChildren();const key=graph.geometry.get(selected);if(!key){results.textContent='No geometry to inspect.';return;}
    const node=graph.nodes.get(key),uses=[...node.dependencies],used=[...node.consumers],direct=new Set(used),further=dependencyReach(graph,key).filter(k=>!direct.has(k));
    for(const [title,keys]of [['Uses',uses],['Further sources',dependencyReach(graph,key,'dependencies').filter(k=>!node.dependencies.has(k))],['Used by',used],['Further dependents',further]]){
      const h=document.createElement('h4');h.textContent=`${title} (${keys.length})`;results.append(h);
      if(!keys.length){const p=document.createElement('p');p.textContent='None.';results.append(p);continue;}
      const list=document.createElement('ul');results.append(list);
      for(const k of keys.slice(0,100)){const row=document.createElement('li'),item=graph.nodes.get(k);row.textContent=item.label+' ';if(!['feature','datum','unresolved'].includes(item.kind))button('Highlight',()=>highlightNode(k),row).setAttribute('aria-label',`Highlight ${item.label}`);list.append(row);}
      if(keys.length>100){const p=document.createElement('p');p.textContent=`Showing 100 of ${keys.length} references.`;results.append(p);}
    }
    if(node.kind==='projections'){const p=document.createElement('p');p.textContent='Detach freezes this curve and its named points from the selected instance. Existing authored references keep their IDs; live intersection consumers must be removed first. Sketch support remains linked.';results.prepend(p);}
  }
  function inspect(id){if(!graph.geometry.has(id)){const key=graph.resolveGenerated(id);if(!key)return false;choice.append(new Option(graph.nodes.get(key).label,id));}selected=id;section.open=true;render();highlightNode(graph.geometry.get(id));return true;}
  choice.onchange=()=>{beginPicking();inspect(choice.value);};render();
  return {inspect,pick(id){if(!picking)return false;if(!inspect(id)){message('Choose geometry represented in the current draft.');return true;}picking=false;message(`Inspecting dependencies of ${id}.`);return true;},cancelPick(){picking=false;},get selected(){return selected;},get open(){return section.open;}};
}
