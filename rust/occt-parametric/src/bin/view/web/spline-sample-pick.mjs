import {geometryIds} from './constraint-controls.mjs';
import {freshName,validateName} from './geometry-names.mjs';
import {inheritGeneratedReferences} from './generated-points.mjs';
import {linkNamedPoint} from './point-links.mjs';
const length=value=>({literal:{value,dimension:'length',unit:'millimeter'}});
export function pickedSplineSample(definition,{point,name,spline,fraction,position}){
 if(!geometryIds(definition,'splines').includes(spline))throw new Error('Choose a known source spline.');
 if(!Number.isFinite(fraction)||fraction<0||fraction>1||!Array.isArray(position)||position.length!==2||!position.every(Number.isFinite))throw new Error('Native sample must have a finite point and fraction from 0 to 1.');
 let candidate=definition,target=point;
 if(!target){validateName(definition,name);candidate=structuredClone(definition);inheritGeneratedReferences(definition,candidate);target=name;(candidate.points??=[]).push({id:target,x:length(position[0]),y:length(position[1]),fixed:false});}
 return {point:target,definition:linkNamedPoint(candidate,{point:target,mode:'spline',spline,fraction:{literal:{value:fraction,dimension:'scalar',unit:null}}})};
}
export function createSplineSamplePicker({host,definition,changed,message,query,current=()=>true,beginPicking=()=>{},modeChanged=()=>{},highlight=()=>{},initialOpen=false,initialOptions={}}){
 const section=document.createElement('details'),heading=document.createElement('summary');heading.textContent='Pick spline sample from canvas';section.open=initialOpen;section.append(heading);host.append(section);
 const note=document.createElement('p');note.textContent='Click a source spline near the desired sample. Native projection finds a unique closest parameter and stages a linked point with a literal fraction. Ambiguous branches are rejected. Preview the current draft before picking, then Preview the new link before Apply. Fractions follow the curve parameter, not arc length.';section.append(note);
 function field(label,tag='input'){const row=document.createElement('label');row.textContent=label;const e=document.createElement(tag);e.setAttribute('aria-label',label);row.append(e);section.append(row);return e;}
 const target=field('Spline sample target','select');target.append(new Option('New named point',''));for(const p of definition.points??[])target.append(new Option(p.id,p.id));target.value=definition.points?.some(p=>p.id===initialOptions.point)?initialOptions.point:'';
 const name=field('New spline sample name');let suggested=initialOptions.name??freshName(definition,'Sample');try{validateName(definition,suggested);}catch{suggested=freshName(definition,'Sample');}name.value=suggested;
 const status=document.createElement('p');status.setAttribute('aria-live','polite');status.setAttribute('data-spline-sample-status','');section.append(status);
 let pending=null,gesture=null,epoch=0;
 const start=document.createElement('button'),stop=document.createElement('button');start.textContent='Start spline sample pick';stop.textContent='Cancel spline sample pick';section.append(start,stop);
 function update(){start.disabled=Boolean(pending);stop.disabled=!pending;target.disabled=Boolean(pending);name.disabled=Boolean(pending);if(name.parentElement)name.parentElement.hidden=Boolean(target.value);}
 function cancel(){const active=Boolean(pending);epoch++;pending=gesture=null;if(active){modeChanged('select');highlight([]);status.textContent='Sample pick canceled.';}update();return active;}
 start.onclick=()=>{try{if(!geometryIds(definition,'splines').length)throw new Error('This sketch needs a source spline.');if(!target.value)validateName(definition,name.value);beginPicking();pending={point:target.value,name:name.value,loading:false};modeChanged('sample');status.textContent='Click a source spline near the sample location.';update();}catch(e){message(e.message);}};
 stop.onclick=()=>{cancel();message('Sample pick canceled. Draft unchanged.');};target.onchange=update;update();
 function valid(event){if(!current(event)){cancel();message('Preview the current draft before picking a spline sample.');return false;}return true;}
 return {get active(){return Boolean(pending);},get open(){return section.open;},get options(){return {point:target.value,name:name.value};},cancel,
 down(event){if(!pending)return false;if(!valid(event))return true;if(pending.loading)return true;if(!Array.isArray(event.position)||!event.position.every(Number.isFinite))return true;gesture={position:[...event.position],entity:event.entity,travel:0,tolerance:Math.max((event.tolerance??0)*.15,1e-9)};return true;},
 move(event){if(!pending||!valid(event))return;if(gesture&&event.position)gesture.travel=Math.max(gesture.travel,Math.hypot(event.position[0]-gesture.position[0],event.position[1]-gesture.position[1]));},
 async up(event){if(!pending||!gesture)return;const click=gesture;gesture=null;if(!valid(event))return;if(!Array.isArray(event.position)||!event.position.every(Number.isFinite))return;if(Math.max(click.travel,Math.hypot(event.position[0]-click.position[0],event.position[1]-click.position[1]))>click.tolerance){message('Use a click to pick a sample; the drag did not change the draft.');return;}
  if(!geometryIds(definition,'splines').includes(click.entity)){message('Click a source spline in the current sketch.');return;}
  if(!Number.isFinite(event.tolerance)||event.tolerance<=0){message('A sample pick needs a positive screen tolerance.');return;}
  const token=++epoch,selection=pending;pending.loading=true;status.textContent='Finding the closest native spline parameter…';highlight([click.entity]);
  try{const result=await query({spline:click.entity,position:event.position,maximum_distance:event.tolerance});if(token!==epoch||selection!==pending)return;if(!valid(event))return;
   const picked=pickedSplineSample(definition,{...selection,spline:click.entity,fraction:result.fraction,position:result.point});cancel();changed(picked.definition,picked.point,click.entity);message(`Sample '${picked.point}' staged at fraction ${result.fraction}. Preview verifies the link before Apply.`);
  }catch(e){if(token===epoch&&selection===pending){pending.loading=false;status.textContent=`Sample query failed: ${e.message}`;message(status.textContent);}}
 },
 };
}
