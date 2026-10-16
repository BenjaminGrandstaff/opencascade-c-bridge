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
export function createSplineSamplePicker({host,definition,changed,message,query,current=()=>true,beginPicking=()=>{},modeChanged=()=>{},highlight=()=>{},ghost=()=>{},initialOpen=false,initialOptions={}}){
 const section=document.createElement('details'),heading=document.createElement('summary');heading.textContent='Pick spline sample from canvas';section.open=initialOpen;section.append(heading);host.append(section);
 const note=document.createElement('p');note.textContent='Click a source spline near the desired sample. Review its native point and fraction, or enter an exact fraction and update the marker before staging. Start, midpoint and end use parameter fractions 0, 0.5 and 1, not arc length. Preview the current draft before picking, then Preview the new link before Apply.';section.append(note);
 function field(label,tag='input'){const row=document.createElement('label');row.textContent=label;const e=document.createElement(tag);e.setAttribute('aria-label',label);row.append(e);section.append(row);return e;}
 const target=field('Spline sample target','select');target.append(new Option('New named point',''));for(const p of definition.points??[])target.append(new Option(p.id,p.id));target.value=definition.points?.some(p=>p.id===initialOptions.point)?initialOptions.point:'';
 const name=field('New spline sample name');let suggested=initialOptions.name??freshName(definition,'Sample');try{validateName(definition,suggested);}catch{suggested=freshName(definition,'Sample');}name.value=suggested;
 const fraction=field('Review sample fraction');fraction.type='number';fraction.min='0';fraction.max='1';fraction.step='any';fraction.value='';
 const updateFraction=document.createElement('button');updateFraction.textContent='Update sample fraction';section.append(updateFraction);
 const shortcuts=[['Sample start (0)',0],['Sample midpoint (0.5)',.5],['Sample end (1)',1]].map(([label,value])=>{const button=document.createElement('button');button.textContent=label;button.onclick=()=>refine(value);section.append(button);return button;});
 const status=document.createElement('p');status.setAttribute('aria-live','polite');status.setAttribute('data-spline-sample-status','');status.setAttribute('style','overflow-wrap:anywhere');section.append(status);
 let pending=null,gesture=null,epoch=0;
 const start=document.createElement('button'),stage=document.createElement('button'),stop=document.createElement('button');start.textContent='Start spline sample pick';stage.textContent='Stage spline sample';stop.textContent='Cancel spline sample pick';section.append(start,stage,stop);
 const readFraction=()=>{const value=String(fraction.value).trim()?Number(fraction.value):NaN;return Number.isFinite(value)&&value>=0&&value<=1?value:NaN;};
 function update(){const ready=Boolean(pending?.source)&&!pending.loading;fraction.disabled=!ready;updateFraction.disabled=!ready||!Number.isFinite(readFraction());for(const button of shortcuts)button.disabled=!ready;stage.disabled=!pending?.candidate||pending.loading;start.disabled=Boolean(pending);stop.disabled=!pending;target.disabled=Boolean(pending);name.disabled=Boolean(pending);if(name.parentElement)name.parentElement.hidden=Boolean(target.value);}
 function clearCandidate(){if(pending)pending.candidate=null;ghost([]);update();}
 function cancel(){const active=Boolean(pending);epoch++;pending=gesture=null;if(active){fraction.value='';ghost([]);modeChanged('select');highlight([]);status.textContent='Sample pick canceled.';}update();return active;}
 function valid(event){if(!current(event)){cancel();message('Preview the current draft before picking a spline sample.');return false;}return true;}
 async function evaluate(request,event){
  if(!pending||!valid(event))return;
  const token=++epoch,selection=pending;clearCandidate();pending.loading=true;update();status.textContent=request.fraction===undefined?'Finding the closest native spline parameter…':'Evaluating the native spline fraction…';highlight([request.spline]);
  try{const result=await query(request);if(token!==epoch||selection!==pending)return;if(!valid(event))return;
   if(request.fraction!==undefined&&result.fraction!==request.fraction)throw new Error('Native sample did not match the requested fraction.');
   pickedSplineSample(definition,{...selection,spline:request.spline,fraction:result.fraction,position:result.point});
   pending.loading=false;pending.candidate={spline:request.spline,fraction:result.fraction,position:[...result.point],event};fraction.value=String(result.fraction);
   const number=value=>String(Number(value.toPrecision(8)));
   status.textContent=`Candidate on ${request.spline}: fraction ${number(result.fraction)}, point (${result.point.map(number).join(', ')}) mm. Stage the sample or update the fraction.`;
   const paths=[[[...result.point,0]]];if(request.position)paths.push([[...request.position,0],[...result.point,0]]);ghost(paths);update();message('Native sample candidate ready. Review the marker and fraction, then Stage spline sample.');
  }catch(e){if(token===epoch&&selection===pending){pending.loading=false;clearCandidate();status.textContent=`Sample query failed: ${e.message}`;message(status.textContent);}}
 }
 async function refine(value){
  try{if(!pending?.source||pending.loading)throw new Error('Pick a source spline before updating the fraction.');
   if(value!==undefined)fraction.value=String(value);const requested=readFraction();
   if(!Number.isFinite(requested)){clearCandidate();throw new Error('Use a sample fraction from 0 to 1.');}
   const source=pending.source;await evaluate({spline:source.spline,fraction:requested},source.event);
  }catch(e){message(e.message);}
 }
 fraction.oninput=()=>{if(!pending||pending.loading)return;clearCandidate();status.textContent='Update the fraction to review its native point before staging.';};
 updateFraction.onclick=()=>refine();
 start.onclick=()=>{try{if(!geometryIds(definition,'splines').length)throw new Error('This sketch needs a source spline.');if(!target.value)validateName(definition,name.value);beginPicking();pending={point:target.value,name:name.value,loading:false};modeChanged('sample');status.textContent='Click a source spline near the sample location.';update();}catch(e){message(e.message);}};
 stage.onclick=()=>{try{
  if(!pending?.candidate||pending.loading)throw new Error('Pick a native sample candidate first.');
  const candidate=pending.candidate;if(!valid(candidate.event))return;
  if(readFraction()!==candidate.fraction)throw new Error('Update the fraction to review its native point before staging.');
  const picked=pickedSplineSample(definition,{...pending,spline:candidate.spline,fraction:candidate.fraction,position:candidate.position});
  cancel();changed(picked.definition,picked.point,candidate.spline);message(`Sample '${picked.point}' staged at fraction ${candidate.fraction}. Preview verifies the link before Apply.`);
 }catch(e){if(pending)clearCandidate();message(e.message);}};
 section.onkeydown=event=>{if(event.key==='Escape'&&pending){event.preventDefault();event.stopPropagation();cancel();message('Sample pick canceled. Draft unchanged.');}};
 stop.onclick=()=>{cancel();message('Sample pick canceled. Draft unchanged.');};target.onchange=update;update();
 return {get active(){return Boolean(pending);},get open(){return section.open;},get options(){return {point:target.value,name:name.value};},cancel,
 down(event){if(!pending)return false;if(!valid(event))return true;if(pending.loading)return true;if(!Array.isArray(event.position)||!event.position.every(Number.isFinite))return true;pending.source=null;fraction.value='';clearCandidate();status.textContent='Click a source spline near the sample location.';gesture={position:[...event.position],entity:event.entity,travel:0,tolerance:Math.max((event.tolerance??0)*.15,1e-9)};return true;},
 move(event){if(!pending||!valid(event))return;if(gesture&&event.position)gesture.travel=Math.max(gesture.travel,Math.hypot(event.position[0]-gesture.position[0],event.position[1]-gesture.position[1]));},
 async up(event){if(!pending||!gesture)return;const click=gesture;gesture=null;if(!valid(event))return;if(!Array.isArray(event.position)||!event.position.every(Number.isFinite))return;if(Math.max(click.travel,Math.hypot(event.position[0]-click.position[0],event.position[1]-click.position[1]))>click.tolerance){message('Use a click to pick a sample; the drag did not change the draft.');return;}
  if(!geometryIds(definition,'splines').includes(click.entity)){message('Click a source spline in the current sketch.');return;}
  if(!Number.isFinite(event.tolerance)||event.tolerance<=0){message('A sample pick needs a positive screen tolerance.');return;}
  pending.source={spline:click.entity,event};await evaluate({spline:click.entity,position:event.position,maximum_distance:event.tolerance},event);
 },
 };
}
