import {profileIds} from './profile-controls.mjs';
const scalar=value=>({literal:{value,dimension:'scalar',unit:null}});
export function editSpline(spline,{points,closed=false,weights},knownPoints){
  const next=structuredClone(spline);
  if(!Array.isArray(points)||points.some(id=>!id||!knownPoints.includes(id)))throw new Error('Choose known named points for the spline.');
  if(spline.basis){
    if(points.length!==spline.points.length)throw new Error('Keep the explicit pole count. Changes to the knot vector belong in the full definition.');
    if(spline.endpoints&&points.some(id=>id===spline.endpoints.start||id===spline.endpoints.end))throw new Error('Native endpoint references must remain separate from control poles.');
    next.points=[...points];
    if(weights!==undefined){
      if(!Array.isArray(weights)||weights.length!==points.length)throw new Error('Each pole needs one weight.');
      for(const w of weights){
        if(!w||typeof w!=='object'||Array.isArray(w))throw new Error('Enter a scalar weight expression.');
        if(w.literal&&(w.literal.dimension!=='scalar'||w.literal.unit!=null||!Number.isFinite(w.literal.value)||w.literal.value<=0))throw new Error('Spline weights must be finite positive scalars.');
      }
      next.basis.weights=structuredClone(weights);
    }
  }else{
    if(weights!==undefined)throw new Error('Rational weights require an explicit B-spline basis.');
    const ids=[...points];if(ids.length>1&&ids[0]===ids.at(-1))ids.pop();
    if(ids.length<(closed?3:2)||new Set(ids).size!==ids.length)throw new Error(`Use at least ${closed?3:2} distinct interpolation points.`);
    next.points=closed?[...ids,ids[0]]:ids;
  }
  return next;
}

export function createSplineControls({host,definition,parameters=[],changed,message,highlight=()=>{},focusPoint=()=>{},beginPicking=()=>{},initialSelection=null}){
  const splines=definition.splines??[];let selected=splines.some(s=>s.id===initialSelection)?initialSelection:splines[0]?.id,picking=false;
  if(!splines.length)return {get selected(){return null;},pick:()=>false,cancelPick(){}};
  const section=document.createElement('section'),title=document.createElement('h3');title.textContent='Spline points & weights';
  const choice=document.createElement('select');choice.setAttribute('aria-label','Spline to edit');for(const s of splines)choice.append(new Option(s.id,s.id));
  const pick=document.createElement('button');pick.textContent='Select spline from canvas';pick.onclick=()=>{beginPicking();picking=true;message('Click an authored spline curve or its control polygon. Escape cancels.');};
  const form=document.createElement('div');section.append(title,choice,pick,form);host.append(section);
  function render(){
    picking=false;choice.value=selected;form.replaceChildren();const spline=splines.find(s=>s.id===selected),explicit=Boolean(spline.basis);
    let closed=!explicit&&spline.points[0]===spline.points.at(-1),weightsEdited=false;
    const ids=closed?spline.points.slice(0,-1):[...spline.points],weights=spline.basis?.weights?.length?structuredClone(spline.basis.weights):spline.points.map(()=>scalar(1));
    const note=document.createElement('p');note.textContent=explicit?`Explicit B-spline · degree ${spline.basis.degree}. Replace or reorder poles here; keep their count and native basis. Coordinate changes use the point controls. Weights retain parameters and formulas.`:'Interpolation points set the native curve. Closed splines repeat the first point reference automatically. Coordinate changes use the point controls.';form.append(note);
    const row=document.createElement('label');row.textContent=explicit?'Control pole names (ordered, comma separated)':'Interpolation point names (ordered, comma separated)';
    const points=document.createElement('input');points.value=ids.join(', ');points.setAttribute('aria-label',row.textContent);row.append(points);form.append(row);
    if(!explicit){const label=document.createElement('label');label.textContent='Closed interpolated spline';const checkbox=document.createElement('input');checkbox.type='checkbox';checkbox.checked=closed;checkbox.setAttribute('aria-label',label.textContent);checkbox.onchange=()=>closed=checkbox.checked;label.append(checkbox);form.append(label);}
    for(const [i,id] of ids.entries()){
      const edit=document.createElement('button');edit.textContent=`Edit point ${i+1}: ${id}`;edit.onclick=()=>{highlight([spline.id,id]);focusPoint(id);};form.append(edit);
      if(!explicit)continue;
      const original=structuredClone(weights[i]),number=original.literal?.dimension==='scalar'?original.literal.value:1;
      let source=original.parameter?`parameter:${original.parameter}`:original.literal?'literal':'preserve';
      const label=document.createElement('label');label.textContent=`Weight ${i+1} source`;
      const driver=document.createElement('select');driver.setAttribute('aria-label',label.textContent);
      const choices=[['literal','Entered value'],...parameters.filter(p=>p.parameter_type?.scalar==='scalar').map(p=>[`parameter:${p.id}`,`Parameter: ${p.id}`])];
      if(source==='preserve')choices.push(['preserve','Keep existing formula']);if(source.startsWith('parameter:')&&!choices.some(([id])=>id===source))choices.push([source,`Existing parameter: ${original.parameter}`]);
      for(const [id,text]of choices)driver.append(new Option(text,id));driver.value=source;label.append(driver);form.append(label);
      const valueRow=document.createElement('label');valueRow.textContent=`Weight ${i+1}`;const input=document.createElement('input');input.type='number';input.step='any';input.value=number;input.setAttribute('aria-label',valueRow.textContent);input.disabled=valueRow.hidden=source!=='literal';valueRow.append(input);form.append(valueRow);
      const literal=()=>scalar(String(input.value).trim()?Number(input.value):NaN);
      input.oninput=()=>{weights[i]=literal();weightsEdited=true;};driver.onchange=()=>{source=driver.value;input.disabled=valueRow.hidden=source!=='literal';weights[i]=source==='preserve'?structuredClone(original):source.startsWith('parameter:')?{parameter:source.slice(10)}:literal();weightsEdited=true;};
    }
    if(explicit){const reset=document.createElement('button');reset.textContent='Use unit weights';reset.onclick=()=>{
      try{const next=editSpline(spline,{points:points.value.split(',').map(id=>id.trim()),weights:spline.points.map(()=>scalar(1))},profileIds(definition,'points'));next.basis.weights=[];definition.splines[definition.splines.indexOf(spline)]=next;changed();message('Unit weights set in the draft. Preview checks the changed curve.');}catch(error){message(error.message);}};form.append(reset);}
    const update=document.createElement('button');update.textContent='Update spline definition';update.onclick=()=>{try{
      const next=editSpline(spline,{points:points.value.split(',').map(id=>id.trim()),closed,weights:explicit&&weightsEdited?weights:undefined},profileIds(definition,'points'));
      const index=definition.splines.indexOf(spline);if(index<0)throw new Error('Spline changed. Reload its controls.');definition.splines[index]=next;changed();message('Spline definition changed in the draft. Preview checks curve geometry, constraints and dependent solids.');
    }catch(error){message(error.message);}};form.append(update);
  }
  choice.onchange=()=>{selected=choice.value;render();highlight([selected]);};render();
  return {
    get selected(){return selected;},
    cancelPick(){if(picking){picking=false;message('Spline pick canceled. Form retained.');}},
    pick(id){if(!picking)return false;if(!splines.some(s=>s.id===id)){message('Choose an authored spline curve.');return true;}selected=id;render();highlight([id]);title.scrollIntoView?.({block:'nearest'});message(`Selected spline ${id}.`);return true;},
  };
}
