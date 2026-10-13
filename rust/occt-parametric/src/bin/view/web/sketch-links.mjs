// Source links change only the local draft until native Preview/Apply accepts it.
export function availableSources(features, sketch) {
  const ids=new Set(features.map(f=>f.id)),blocked=new Set([sketch]);
  const dependencies=value=>{
    const result=new Set();
    function walk(v){if(!v||typeof v!=='object')return;for(const [key,item]of Object.entries(v)){
      if(['input','object','tool','left','right','feature','source_feature','source'].includes(key)&&typeof item==='string'&&ids.has(item))result.add(item);
      else if(typeof item==='object')walk(item);
    }}walk(value);return result;
  };
  let changed=true;while(changed){changed=false;for(const f of features)if(!blocked.has(f.id)&&[...dependencies(f.operation)].some(id=>blocked.has(id))){blocked.add(f.id);changed=true;}}
  return features.filter(f=>!blocked.has(f.id));
}
export function requireConstructionProfile(draft){
  if(!draft.profile?.length&&!draft.region_discovery?.curves?.length)
    throw Error('Set an explicit outer profile or discovery curve list before adding construction references.');
}
export function attachPickedFace(draft,pick){
  const offset=structuredClone(draft.face_support?.offset??{literal:{value:0,dimension:'length',unit:'millimeter'}});
  draft.face_support=structuredClone(pick.face_support);draft.face_support.offset=offset;
  draft.datum_plane=null;draft.x_axis=structuredClone(pick.x_axis);draft.y_axis=structuredClone(pick.y_axis);
}
export function createSketchLinks({context,request,preview,showSolid,changed,message,addProjection,highlightFace=()=>{},highlightEdge=()=>{},beginPicking=()=>{},currentVersion=()=>null}){
  let lastDraft=null,nameEdited=false,source='',name='reference',mode=null,turn=0,busy=false,ready=false;
  function cancel(){const active=mode;mode=null;ready=false;turn++;highlightFace(null);highlightEdge(null);if(active)message('Source picking canceled. Sketch draft retained.');}
  async function start(next){
    const c=context();if(!c?.draft||!source||busy)return;
    beginPicking();cancel();mode=next;const ticket=++turn;
    message('Loading accepted source geometry…');
    try{
      const data=await request('/api/sketch-source-view',{method:'POST',body:JSON.stringify({version:c.version,instance:c.instance,output:source})});
      if(ticket!==turn)return;
      if(currentVersion()!==null&&currentVersion()!==data.version){cancel();message('Model changed. Reload the sketch draft.');return;}
      await preview(data,'Accepted source geometry — click a face or edge to update the draft.');
      if(ticket!==turn)return;showSolid(source);ready=true;
      message(next==='face'?'Click a planar face to reattach. Sketch-local coordinates and offset expression are retained.':'Click a visible edge to add a linked construction reference. Escape cancels.');
    }catch(error){if(ticket===turn){cancel();message(error.message);}}
  }
  function button(text,action){const e=document.createElement('button');e.textContent=text;e.onclick=async()=>{if(busy)return;try{await action();}catch(error){message(error.message);}};return e;}
  function render(host){
    const c=context();if(!c?.draft)return;
    if(lastDraft!==c.draft){lastDraft=c.draft;name='reference';nameEdited=false;}
    if(!nameEdited){
      const ids=new Set(['points','lines','circles','arcs','ellipses','splines','shapes','texts','patterns','projections','measurements'].flatMap(key=>(c.draft[key]??[]).map(e=>e.id)));
      name='reference';for(let i=2;ids.has(name);i++)name=`reference-${i}`;
    }
    const section=document.createElement('fieldset'),legend=document.createElement('legend');legend.textContent='Support & projected references';section.append(legend);
    const choices=availableSources(c.features??[],c.feature);if(!choices.some(f=>f.id===source))source=choices[0]?.id??'';
    const label=document.createElement('label');label.textContent='Source feature';const select=document.createElement('select');select.setAttribute('aria-label','Sketch link source');
    for(const f of choices)select.append(new Option(f.id,f.id));select.value=source;select.onchange=()=>{cancel();source=select.value;};label.append(select);section.append(label);
    const face=button('Reattach to picked face',()=>start('face'));face.disabled=!source;
    section.append(face);
    const id=document.createElement('label');id.textContent='Projection name';const input=document.createElement('input');input.value=name;input.setAttribute('aria-label','Projection name');input.oninput=()=>{name=input.value;nameEdited=true;};id.append(input);section.append(id);
    const edge=button('Project picked edge',()=>start('edge'));edge.disabled=!source;section.append(edge);
    const note=document.createElement('p');note.textContent='Links are family-wide. Coordinates stay sketch-local. Projections are construction references until included in the profile. Preview checks all downstream geometry and requirements.';section.append(note);
    for(const p of c.draft.projections??[]){
      const row=document.createElement('div'),text=document.createElement('span');text.textContent=`${p.id} · ${p.input} · ${p.kind}`;row.append(text,button(`Remove projection ${p.id}`,()=>{cancel();context().draft.projections=context().draft.projections.filter(v=>v.id!==p.id);changed();message('Projection removed from draft. Repair any constraints or profile references before applying.');}));section.append(row);
    }
    host.append(section);
  }
  async function pick(hit,next){
    const c=context();if(mode!==next||!ready||busy||!c?.draft)return;
    if(hit.output!==source||!hit.accepted||hit.version!==c.version||hit.instance!==c.instance){cancel();message('View changed or is a preview. Reload the sketch draft before picking.');return;}
    const ticket=++turn;busy=true;
    try{
      const body={version:hit.version,instance:hit.instance,output:hit.output};
      if(next==='face')body.face=hit.face;else{body.edge=hit.edge;body.id=name;}
      const response=await request(next==='face'?'/api/sketch-face-pick':'/api/sketch-edge-pick',{method:'POST',body:JSON.stringify(body)});
      if(ticket!==turn||context().draft!==c.draft||context().instance!==c.instance||(currentVersion()!==null&&currentVersion()!==response.version))return;
      mode=null;
      if(next==='face'){attachPickedFace(c.draft,response);highlightFace(hit.face,hit.output);}
      else{addProjection(c.draft,response.projection);highlightEdge(hit.edge,hit.output);}
      changed();message(next==='face'?'Support changed in the draft. Preview before applying.':`Projection added to the draft (${response.projection.kind}). Preview to resolve its geometry.${response.binding==='anchored_center'?' This reference is anchored to its current center; moving the source requires picking again.':''}`);
    }catch(error){if(ticket===turn)message(`Source pick rejected: ${error.message}`);}
    finally{busy=false;}
  }
  return {render,cancel,get facePicking(){return mode==='face'&&ready;},get edgePicking(){return mode==='edge'&&ready;},get busy(){return busy;},pickFace:hit=>pick(hit,'face'),pickEdge:hit=>pick(hit,'edge')};
}
