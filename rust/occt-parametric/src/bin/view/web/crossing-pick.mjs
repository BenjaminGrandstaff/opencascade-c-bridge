import {geometryIds} from './constraint-controls.mjs';
import {linkNamedPoint} from './point-links.mjs';
const length=value=>({literal:{value,dimension:'length',unit:'millimeter'}});
function geometryNames(definition){
 const used=new Set(['points','lines','circles','arcs','ellipses','splines','texts','patterns','projections','measurements'].flatMap(k=>(definition[k]??[]).map(e=>e.id)));
 for(const shape of definition.shapes??[])for(const value of Object.values(shape))if(value?.id)used.add(value.id);
 for(const id of geometryIds(definition,'points'))used.add(id);
 for(const p of definition.projections??[])for(const item of [...(p.intersections??[]),...(p.junctions??[])]){if(item.id)used.add(`${p.id}:${item.id}`);if(item.point)used.add(item.point);}
 for(const op of definition.curve_operations??[])if(op.split){used.add(op.split.before);used.add(op.split.after);}
 return used;
}
function freshName(definition){const used=geometryNames(definition);let i=1;while(used.has(`Crossing${i}`))i++;return `Crossing${i}`;}
function validateName(definition,name){
 if(typeof name!=='string'||!name.trim()||name!==name.trim())throw new Error('Enter a new point name without leading or trailing spaces.');
 if(geometryNames(definition).has(name))throw new Error('That geometry name already exists. Choose an existing target or a new name.');
}
export function pickedCrossing(definition,{point,name,first,second,position,radius}){
 if(!Array.isArray(position)||position.length!==2||!position.every(Number.isFinite))throw new Error('Choose a finite sketch-local crossing hint.');
 if(!Number.isFinite(radius)||radius<=0)throw new Error('Use a positive finite crossing search radius.');
 let candidate=definition,target=point;
 if(!target){validateName(definition,name);target=name;candidate=structuredClone(definition);(candidate.points??=[]).push({id:target,x:length(position[0]),y:length(position[1]),fixed:false});}
 const near={literal:{x:length(position[0]).literal,y:length(position[1]).literal,z:length(0).literal}};
 return {point:target,definition:linkNamedPoint(candidate,{point:target,mode:'selected_crossing',first,second,near,maximum_distance:length(radius)})};
}
export function crossingWindowFeedback(points,position,radius){
 if(!Array.isArray(points)||points.length>1000||points.some(p=>!Array.isArray(p)||p.length!==2||!p.every(Number.isFinite)))throw new Error('Native crossing candidates are invalid or exceed the 1000-point budget.');
 if(!Array.isArray(position)||position.length!==2||!position.every(Number.isFinite)||!Number.isFinite(radius)||radius<=0)throw new Error('Use a finite hint and positive search radius.');
 const matches=points.filter(p=>Math.hypot(p[0]-position[0],p[1]-position[1])<=radius);
 return {kind:matches.length===1?'unique':matches.length===0?'missing':'ambiguous',matches};
}
export function createCrossingPicker({host,definition,changed,message,beginPicking=()=>{},highlight=()=>{},ghost=()=>{},modeChanged=()=>{},current=()=>true,queryCandidates=null,initialOpen=false,initialOptions={}}){
 const section=document.createElement('details'),heading=document.createElement('summary');heading.textContent='Pick crossing from canvas';section.open=initialOpen;section.append(heading);host.append(section);
 const note=document.createElement('p');note.textContent='Choose a target and radius, then click two curves and a hint near the intended crossing. The orange circle previews the search area; native candidate markers and feedback describe the current solved curves. Picking writes a literal hint and radius into the draft; Preview must find exactly one native crossing. Adjust the radius while picking without choosing the curves again. Other dimensions remain active.';section.append(note);
 function field(caption,tag='input'){const row=document.createElement('label');row.textContent=caption;const input=document.createElement(tag);input.setAttribute('aria-label',caption);row.append(input);section.append(row);return input;}
 const target=field('Picked crossing target','select');target.append(new Option('New named point',''));for(const p of definition.points??[])target.append(new Option(p.id,p.id));target.value=(definition.points??[]).some(p=>p.id===initialOptions.point)?initialOptions.point:'';
 const name=field('New crossing point name');name.type='text';let suggested=initialOptions.name??freshName(definition);try{validateName(definition,suggested);}catch{suggested=freshName(definition);}name.value=suggested;
 const radius=field('Picked crossing search radius (mm)');radius.type='number';radius.step='any';radius.value=initialOptions.radius??'2';
 const status=document.createElement('p');status.setAttribute('aria-live','polite');const feedback=document.createElement('p');feedback.setAttribute('aria-live','polite');feedback.setAttribute('data-crossing-feedback','');section.append(status,feedback);
 let pending=null,gesture=null;
 const button=(text,action)=>{const b=document.createElement('button');b.textContent=text;b.onclick=()=>{try{action();}catch(e){message(e.message);}};section.append(b);return b;};
 function update(){name.parentElement&&(name.parentElement.hidden=Boolean(target.value));for(const input of [target,name])input.disabled=Boolean(pending);start.disabled=Boolean(pending);stop.disabled=!pending;status.textContent=!pending?'No crossing pick in progress.':pending.curves.length===0?'Click the first source curve.':pending.curves.length===1?`First curve: ${pending.curves[0]}. Click a different source curve.`:`Sources: ${pending.curves.join(' × ')}. Click near the intended crossing.`;}
 function cancel(){const active=Boolean(pending);pending=gesture=null;feedback.textContent='';feedback.className='';if(active){ghost([]);highlight([]);modeChanged('select');}update();return active;}
 const start=button('Start crossing pick',()=>{
  const value=String(radius.value).trim()?Number(radius.value):NaN;if(!Number.isFinite(value)||value<=0)throw new Error('Use a positive finite crossing search radius.');
  if(!target.value)validateName(definition,name.value);
  if(geometryIds(definition,'curves').length<2)throw new Error('This sketch needs two distinct source curves.');
  beginPicking();pending={point:target.value,name:name.value,radius:value,curves:[],scene:null,queryStatus:'idle',queryError:null,candidates:null,cursor:null,lastEvent:null};modeChanged('crossing');update();message(status.textContent);
 });
 const stop=button('Cancel crossing pick',()=>{cancel();message('Crossing pick canceled. Draft unchanged.');});
 function valid(event){if(!current(event)||(pending.scene&&pending.scene!==event.scene)){cancel();message('The native view changed. Restart the crossing pick in the current sketch.');return false;}pending.lastEvent=event;return true;}
 function paint(position){
  if(!pending||pending.curves.length!==2||!Array.isArray(position)||position.length!==2||!position.every(Number.isFinite))return;
  pending.cursor=[...position];if(!Number.isFinite(pending.radius)||pending.radius<=0){ghost([]);feedback.className='failed';feedback.textContent='Enter a positive finite crossing search radius before picking a hint.';return;}const [x,y]=position,r=pending.radius,paths=[Array.from({length:65},(_,i)=>{const a=2*Math.PI*i/64;return [x+r*Math.cos(a),y+r*Math.sin(a),0];}),[[x,y,0]]];
  if(pending.queryStatus==='ready'){const result=crossingWindowFeedback(pending.candidates,position,r);feedback.className=result.kind==='unique'?'passed':'failed';feedback.textContent=result.kind==='unique'?'Native window: one finite crossing. Click to stage; Preview still checks the model.':result.kind==='missing'?'Native window: no finite crossing. Move the hint or increase the radius.':`Native window: ${result.matches.length} finite crossings. Move the hint or reduce the radius.${result.matches.length>100?' Showing the first 100 candidate markers.':''}`;paths.push(...result.matches.slice(0,100).map(p=>[[...p,0]]));}
  if(pending.queryStatus!=='ready')queryFeedback();
  ghost(paths);
 }
 function queryFeedback(){
  feedback.className=pending?.queryStatus==='error'?'failed':'';
  feedback.textContent=pending?.queryStatus==='loading'?'Loading finite native crossings…':pending?.queryStatus==='error'?`Native crossing query failed: ${pending.queryError}. Cancel and restart after fixing the draft.`:pending?.queryStatus==='ready'?'Native crossings ready. Move the cursor to inspect the search window.':'';
 }
 function changeRadius(){
  if(!pending)return;if(pending.lastEvent&&!valid(pending.lastEvent))return;
  gesture=null;const value=String(radius.value).trim()?Number(radius.value):NaN;pending.radius=Number.isFinite(value)&&value>0?value:null;
  if(pending.radius===null){ghost([]);feedback.className='failed';feedback.textContent='Enter a positive finite crossing search radius before picking a hint.';return;}
  if(pending.cursor)paint(pending.cursor);else queryFeedback();
 }
 radius.oninput=changeRadius;
 async function prepare(event){
  if(!queryCandidates)return;const selected=pending;selected.queryStatus='loading';feedback.className='';feedback.textContent='Loading finite native crossings…';
  try{const result=await queryCandidates({first:selected.curves[0],second:selected.curves[1]});if(pending!==selected)return;if(!valid(event))return;
   if(result?.complete!==true||result.first!==selected.curves[0]||result.second!==selected.curves[1]||result.coordinate_system!=='sketch-local millimeters')throw new Error('Native crossing snapshot is incomplete or does not match this curve pair.');
   crossingWindowFeedback(result.points,[0,0],1);selected.candidates=result.points;selected.queryStatus='ready';changeRadius();
  }catch(error){if(pending!==selected)return;selected.queryStatus='error';selected.queryError=error.message;selected.candidates=null;changeRadius();}
 }
 target.onchange=update;update();
 return {cancel,get active(){return Boolean(pending);},get open(){return section.open;},get options(){return {point:target.value,name:name.value,radius:radius.value};},
  down(event){if(!pending)return false;if(!valid(event))return true;if(!Array.isArray(event.position)||!event.position.every(Number.isFinite))return true;gesture={position:[...event.position],entity:event.entity,travel:0,tolerance:Math.max((event.tolerance??0)*0.15,1e-9)};return true;},
  move(event){if(!pending)return;if(!valid(event))return;if(gesture&&event.position)gesture.travel=Math.max(gesture.travel,Math.hypot(event.position[0]-gesture.position[0],event.position[1]-gesture.position[1]));paint(event.position);},
  up(event){if(!pending||!gesture)return;const click=gesture;gesture=null;if(!valid(event))return;if(!Array.isArray(event.position)||!event.position.every(Number.isFinite))return;if(Math.max(click.travel,Math.hypot(event.position[0]-click.position[0],event.position[1]-click.position[1]))>click.tolerance){message('Use a click to pick a crossing; the drag did not change the draft.');return;}
   if(pending.curves.length<2){const id=click.entity;if(!geometryIds(definition,'curves').includes(id)){message('Click an authored or projected source curve.');return;}if(pending.curves.includes(id)){message('Choose a different second curve.');return;}pending.scene=event.scene;pending.curves.push(id);highlight(pending.curves);update();message(status.textContent);if(pending.curves.length===2)void prepare(event);return;}
   if(!Number.isFinite(pending.radius)||pending.radius<=0){paint(event.position);message('Use a positive finite crossing search radius before picking a hint.');return;}
   if(queryCandidates){paint(event.position);if(pending.queryStatus!=='ready'){message(pending.queryStatus==='loading'?'Wait for the native crossing query, then click again.':'Native crossings are unavailable. Cancel and restart after fixing the draft.');return;}const check=crossingWindowFeedback(pending.candidates,event.position,pending.radius);if(check.kind!=='unique'){message(feedback.textContent);return;}}
   try{const result=pickedCrossing(definition,{...pending,first:pending.curves[0],second:pending.curves[1],position:event.position});cancel();changed(result.definition,result.point);message(`Crossing '${result.point}' staged with a literal search window. Preview verifies a unique finite native crossing before Apply.`);}catch(e){message(e.message);}
  },
 };
}
