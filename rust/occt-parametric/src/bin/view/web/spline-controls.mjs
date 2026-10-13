import {profileIds} from './profile-controls.mjs';
import {refineSpline} from './spline-refinement.mjs';
const scalar=value=>({literal:{value,dimension:'scalar',unit:null}});
export function editSplineBasis(spline,{degree,knots,multiplicities,mode,endpoints},knownPoints){
  if(!spline.basis)throw new Error('Basis controls require an explicit B-spline.');
  const count=spline.points.length,periodic=mode==='periodic',nonClamped=mode==='non_clamped';
  if(!['clamped','periodic','non_clamped'].includes(mode))throw new Error('Choose a spline basis mode.');
  if(!Number.isInteger(degree)||degree<1||degree>25||count<degree+1)throw new Error('Degree must be an integer from 1 to 25, with at least degree + 1 poles.');
  if(!Array.isArray(knots)||knots.length<2||knots.length>10002||knots.some((k,i)=>!Number.isFinite(k)||i>0&&k<=knots[i-1]))throw new Error('Use at least two finite, strictly increasing knots.');
  if(!Array.isArray(multiplicities)||multiplicities.length!==knots.length)throw new Error('Each knot needs one multiplicity.');
  for(const [i,m]of multiplicities.entries()){
    const boundary=!periodic&&(i===0||i===knots.length-1);
    if(!Number.isInteger(m)||m<1||m>degree+(boundary?1:0)||boundary&&!nonClamped&&m!==degree+1)throw new Error('Invalid knot multiplicities: clamped ends need degree + 1; internal/periodic knots allow 1 through degree.');
  }
  if(periodic&&multiplicities[0]!==multiplicities.at(-1))throw new Error('Periodic endpoint multiplicities must match.');
  if(multiplicities.reduce((sum,m)=>sum+m,0)!==count+(periodic?multiplicities.at(-1):degree+1))throw new Error('Multiplicity sum does not match the pole count and degree.');
  if(nonClamped&&(!endpoints||[endpoints.start,endpoints.end].some(id=>!knownPoints.includes(id)||spline.points.includes(id))))throw new Error('Choose known native endpoint points distinct from control poles.');
  const next=structuredClone(spline);Object.assign(next.basis,{degree,knots:[...knots],multiplicities:[...multiplicities]});
  const oldMode=spline.basis.periodic?'periodic':spline.basis.non_clamped?'non_clamped':'clamped';
  if(oldMode!==mode){next.basis.periodic=periodic;next.basis.non_clamped=nonClamped;}
  if(nonClamped)next.endpoints=structuredClone(endpoints);else delete next.endpoints;
  return next;
}
export function editSpline(spline,{points,closed=false,weights},knownPoints){
  const next=structuredClone(spline);
  if(!Array.isArray(points)||points.some(id=>!id||!knownPoints.includes(id)))throw new Error('Choose known named points for the spline.');
  if(spline.basis){
    if(points.length!==spline.points.length)throw new Error('Keep the explicit pole count. Use Insert control pole or Remove control pole to change the count.');
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

export function resizeSplinePoles(spline,{action,index,point},knownPoints){
  if(!spline.basis)throw new Error('Pole count controls require an explicit B-spline.');
  const mode=spline.basis.periodic?'periodic':spline.basis.non_clamped?'non_clamped':'clamped';
  editSplineBasis(spline,{...spline.basis,mode,endpoints:spline.endpoints},knownPoints);
  if(!['insert','remove'].includes(action)||!Number.isInteger(index)||index<0||index>spline.points.length-(action==='remove'?1:0))throw new Error('Choose a valid pole position.');
  const next=structuredClone(spline),degree=spline.basis.degree;
  if(action==='insert'){
    if(!knownPoints.includes(point)||spline.endpoints&&[spline.endpoints.start,spline.endpoints.end].includes(point))throw new Error('Choose a known control point separate from native endpoints.');
    next.points.splice(index,0,point);if(next.basis.weights?.length)next.basis.weights.splice(index,0,scalar(1));
  }else{
    next.points.splice(index,1);if(next.basis.weights?.length)next.basis.weights.splice(index,1);
  }
  const count=next.points.length;
  if(count<degree+1)throw new Error('Keep at least degree + 1 control poles.');
  let start=spline.basis.knots[0],end=spline.basis.knots.at(-1),knots,multiplicities;
  if(mode==='non_clamped'){
    const expanded=spline.basis.knots.flatMap((k,i)=>Array(spline.basis.multiplicities[i]).fill(k));
    start=expanded[degree];end=expanded[spline.points.length];
    const step=(end-start)/(count-degree);
    knots=Array.from({length:count+degree+1},(_,i)=>start+(i-degree)*step);multiplicities=knots.map(()=>1);
  }else{
    const spans=mode==='periodic'?count:count-degree;
    knots=Array.from({length:spans+1},(_,i)=>start+(end-start)*i/spans);
    multiplicities=knots.map((_,i)=>mode==='clamped'&&(i===0||i===spans)?degree+1:1);
  }
  return editSplineBasis(next,{degree,knots,multiplicities,mode,endpoints:next.endpoints},knownPoints);
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
    const note=document.createElement('p');note.textContent=explicit?'Explicit B-spline. Keep the pole count when editing this list. Degree, knots and multiplicities must agree; these edits redefine the curve. Coordinate changes use the point controls. Weights retain parameters and formulas.':'Interpolation points set the native curve. Closed splines repeat the first point reference automatically. Coordinate changes use the point controls.';form.append(note);
    const row=document.createElement('label');row.textContent=explicit?'Control pole names (ordered, comma separated)':'Interpolation point names (ordered, comma separated)';
    const points=document.createElement('input');points.value=ids.join(', ');points.setAttribute('aria-label',row.textContent);row.append(points);form.append(row);
    if(!explicit){const label=document.createElement('label');label.textContent='Closed interpolated spline';const checkbox=document.createElement('input');checkbox.type='checkbox';checkbox.checked=closed;checkbox.setAttribute('aria-label',label.textContent);checkbox.onchange=()=>closed=checkbox.checked;label.append(checkbox);form.append(label);}
    let basisFields=null;
    if(explicit){
      const group=document.createElement('fieldset'),legend=document.createElement('legend');legend.textContent='Native B-spline basis';group.append(legend);form.append(group);
      function input(label,value,type='text'){const row=document.createElement('label');row.textContent=label;const e=document.createElement('input');e.type=type;e.value=value;if(type==='number')e.step='1';e.setAttribute('aria-label',label);row.append(e);group.append(row);return e;}
      const degree=input('Spline degree',spline.basis.degree,'number'),knots=input('Unique knots (comma separated)',spline.basis.knots.join(', ')),multiplicities=input('Knot multiplicities (comma separated)',spline.basis.multiplicities.join(', '));
      const label=document.createElement('label');label.textContent='Spline basis mode';const mode=document.createElement('select');mode.setAttribute('aria-label',label.textContent);
      for(const [id,text]of [['clamped','Clamped'],['periodic','Periodic'],['non_clamped','Non-clamped']])mode.append(new Option(text,id));mode.value=spline.basis.periodic?'periodic':spline.basis.non_clamped?'non_clamped':'clamped';label.append(mode);group.append(label);
      const endpointRows=[];function endpoint(key){const row=document.createElement('label');row.textContent=`Native ${key} point`;const e=document.createElement('select');e.setAttribute('aria-label',row.textContent);for(const id of ['',...profileIds(definition,'points').filter(id=>!spline.points.includes(id))])e.append(new Option(id||'Choose endpoint',id));e.value=spline.endpoints?.[key]??'';row.append(e);group.append(row);endpointRows.push(row);return e;}
      const start=endpoint('start'),end=endpoint('end');const show=()=>endpointRows.forEach(row=>row.hidden=mode.value!=='non_clamped');mode.onchange=show;show();
      const parse=e=>String(e.value).split(',').map(value=>value.trim()?Number(value):NaN);
      basisFields=()=>({degree:String(degree.value).trim()?Number(degree.value):NaN,knots:parse(knots),multiplicities:parse(multiplicities),mode:mode.value,endpoints:{start:start.value,end:end.value}});
    }
    const withBasis=next=>basisFields?editSplineBasis(next,basisFields(),profileIds(definition,'points')):next;
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
    if(explicit){
      const group=document.createElement('fieldset'),legend=document.createElement('legend');legend.textContent='Add or remove control poles';group.append(legend);form.append(group);
      const note=document.createElement('p');note.textContent='Count changes rebuild uniform knots over the current curve parameter domain and reshape the curve. Inserted poles use unit weight. Existing point definitions and constraints remain in the sketch.';group.append(note);
      const label=document.createElement('label');label.textContent='Control point to insert';const point=document.createElement('select');point.setAttribute('aria-label',label.textContent);
      for(const id of profileIds(definition,'points').filter(id=>![spline.endpoints?.start,spline.endpoints?.end].includes(id)))point.append(new Option(id,id));label.append(point);group.append(label);
      const row=document.createElement('label');row.textContent='Control pole position';const position=document.createElement('input');position.type='number';position.min='1';position.max=String(ids.length+1);position.step='1';position.value=String(ids.length+1);position.setAttribute('aria-label',row.textContent);row.append(position);group.append(row);
      const hint=document.createElement('p');hint.textContent='Positions start at 1. Insert before this position, or remove the pole at this position.';group.append(hint);
      for(const [action,text]of [['insert','Insert control pole'],['remove','Remove control pole']]){
        const button=document.createElement('button');button.textContent=text;button.onclick=()=>{try{
          const current=withBasis(editSpline(spline,{points:points.value.split(',').map(id=>id.trim()),weights:weightsEdited?weights:undefined},profileIds(definition,'points')));
          const next=resizeSplinePoles(current,{action,index:String(position.value).trim()?Number(position.value)-1:NaN,point:point.value},profileIds(definition,'points'));
          const index=definition.splines.indexOf(spline);if(index<0)throw new Error('Spline changed. Reload its controls.');definition.splines[index]=next;changed();message('Pole count and uniform basis updated in the draft. Preview checks the reshaped curve and dependent solids.');
        }catch(error){message(error.message);}};group.append(button);
      }
    }
    if(explicit){
      const group=document.createElement('fieldset'),legend=document.createElement('legend');legend.textContent='Preserve curve shape';group.append(legend);form.append(group);
      const note=document.createElement('p');note.textContent='Exact rational knot insertion and degree elevation support clamped splines with fixed authored poles. New poles retain coordinate and weight parameter expressions. Original points and constraints remain. Elevation uses adjacent Bezier spans.';group.append(note);
      function field(label,value){const row=document.createElement('label');row.textContent=label;const e=document.createElement('input');e.type='number';e.step='any';e.value=String(value);e.setAttribute('aria-label',label);row.append(e);group.append(row);return e;}
      const knot=field('Knot to insert',(spline.basis.knots[0]+spline.basis.knots.at(-1))/2),degree=field('Elevated spline degree',spline.basis.degree+1);degree.step='1';
      for(const [action,text]of [['insert_knot','Insert knot preserving shape'],['elevate_degree','Elevate degree preserving shape']]){
        const button=document.createElement('button');button.textContent=text;button.onclick=()=>{try{
          const current=withBasis(editSpline(spline,{points:points.value.split(',').map(id=>id.trim()),weights:weightsEdited?weights:undefined},profileIds(definition,'points')));
          const staged=structuredClone(definition),index=definition.splines.indexOf(spline);if(index<0)throw new Error('Spline changed. Reload its controls.');staged.splines[index]=current;
          const next=refineSpline(staged,spline.id,{action,knot:String(knot.value).trim()?Number(knot.value):NaN,degree:String(degree.value).trim()?Number(degree.value):NaN});
          Object.assign(definition,next);changed();message('Exact spline refinement added to the draft. Preview checks the curve, constraints and dependent solids.');
        }catch(error){message(error.message);}};group.append(button);
      }
    }
    if(explicit){const reset=document.createElement('button');reset.textContent='Use unit weights';reset.onclick=()=>{
      try{const next=withBasis(editSpline(spline,{points:points.value.split(',').map(id=>id.trim()),weights:spline.points.map(()=>scalar(1))},profileIds(definition,'points')));next.basis.weights=[];definition.splines[definition.splines.indexOf(spline)]=next;changed();message('Unit weights set in the draft. Preview checks the changed curve.');}catch(error){message(error.message);}};form.append(reset);}
    const update=document.createElement('button');update.textContent='Update spline definition';update.onclick=()=>{try{
      const next=withBasis(editSpline(spline,{points:points.value.split(',').map(id=>id.trim()),closed,weights:explicit&&weightsEdited?weights:undefined},profileIds(definition,'points')));
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
