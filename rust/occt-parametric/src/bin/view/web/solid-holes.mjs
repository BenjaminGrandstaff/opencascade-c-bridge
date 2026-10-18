// Hole controls keep existing finishing/thread metadata and opaque formulas intact.
import {createThreadControls} from './solid-hole-threads.mjs';
const scalar=value=>({literal:{value,dimension:'scalar',unit:null}});
const length=value=>({literal:{value,dimension:'length',unit:'millimeter'}});
const mm=q=>q?.dimension==='length'?q.value*({millimeter:1,centimeter:10,meter:1000,inch:25.4}[q.unit]??NaN):NaN;
const vector=(values,dimension)=>{
 if(values.length!==3||!values.every(Number.isFinite)||dimension==='scalar'&&Math.hypot(...values)===0)throw Error('Enter a finite hole position and nonzero axis.');
 return {components:Object.fromEntries(['x','y','z'].map((k,i)=>[k,{literal:{value:values[i],dimension,unit:dimension==='length'?'millimeter':null}}]))};
};
const read=(v,dimension)=>['x','y','z'].map(k=>{const q=v?.literal?.[k]??v?.components?.[k]?.literal;return dimension==='length'?mm(q):q?.dimension==='scalar'?q.value:NaN;});
export function createHoleControls(saved=null,target='',threadSizes={metric:[],inch:[]}) {
 const thread=createThreadControls(saved?.thread??null,threadSizes);
 const original=structuredClone(saved),position=read(saved?.position,'length'),axis=read(saved?.axis,'scalar');
 const state={input:saved?.input??target,position:saved?position:[0,0,0],axis:saved?axis:[0,0,-1],placement:saved?'preserve':'literal',extent:saved?.extent?.blind?'blind':saved?.extent?.up_to_face?'up_to_face':saved?.extent??'blind',face:saved?.extent?.up_to_face?.face??null};
 const cb=saved?.finish?.counterbore,cs=saved?.finish?.countersink,tip=saved?.bottom?.drill_point;
 state.finish=cb?'counterbore':cs?'countersink':'plain';state.bottom=tip?'drill_point':'flat';state.relief=Boolean(cb?.relief);
 const authored={diameter:saved?.diameter,depth:saved?.extent?.blind?.depth,'counterbore diameter':cb?.diameter,'counterbore depth':cb?.depth,'relief diameter':cb?.relief,'countersink diameter':cs?.diameter,'countersink angle':cs?.angle_radians,'drill point angle':tip?.angle_radians};
 const fields={diameter:length(5),depth:length(5),'counterbore diameter':length(8),'counterbore depth':length(2),'relief diameter':length(6),'countersink diameter':length(8),'countersink angle':scalar(Math.PI/2),'drill point angle':scalar(118*Math.PI/180)};
 for(const key of Object.keys(fields))if(authored[key])fields[key]=authored[key];
 const angular=key=>key.endsWith('angle');
 const values=Object.fromEntries(Object.entries(fields).map(([key,value])=>[key,{value:value.literal?(angular(key)?value.literal.value*180/Math.PI:mm(value.literal)):NaN,source:authored[key]?'preserve':'literal'}]));
 function expression(key){
  const f=values[key],angle=angular(key),value=f.source==='preserve'?fields[key]:f.source.startsWith('parameter:')?{parameter:f.source.slice(10)}:angle?scalar(f.value*Math.PI/180):length(f.value);
  if(value?.literal){const q=value.literal,n=angle?q.value:mm(q);if(angle?(q.dimension!=='scalar'||!Number.isFinite(n)||n<=0||n>=Math.PI):(!Number.isFinite(n)||n<=0))throw Error(angle?'Included angles must be between 0 and 180 degrees.':'Hole dimensions must be positive lengths.');}
  return structuredClone(value);
 }
 return {
  state,
  baseDefinition(){
   if(!state.input)throw Error('Choose a solid to drill.');
   if(!['blind','through_all','up_to_next','up_to_face'].includes(state.extent))throw Error('Choose a hole extent.');
   if(state.extent==='up_to_face'&&!state.face)throw Error('Pick a limiting face.');
   if(state.bottom==='drill_point'&&state.extent!=='blind')throw Error('A drill point requires a blind hole. Choose a flat bottom for this extent.');
   const bottom=state.bottom==='flat'?'flat':{drill_point:{angle_radians:expression('drill point angle')}};
   const finish=state.finish==='plain'?'plain':state.finish==='counterbore'?{counterbore:{diameter:expression('counterbore diameter'),depth:expression('counterbore depth'),...(state.relief?{relief:expression('relief diameter')}:{})}}:{countersink:{diameter:expression('countersink diameter'),angle_radians:expression('countersink angle')}};
   return {...structuredClone(original),input:state.input,position:state.placement==='preserve'?structuredClone(original.position):vector(state.position,'length'),axis:state.placement==='preserve'?structuredClone(original.axis):vector(state.axis,'scalar'),diameter:expression('diameter'),extent:state.extent==='blind'?{blind:{depth:expression('depth')}}:state.extent==='up_to_face'?{up_to_face:{face:structuredClone(state.face)}}:state.extent,bottom,finish};
  },
  definition(){
   const hole=this.baseDefinition(),threaded=thread.definition();
   if(threaded.thread===null)delete hole.thread;
   else if(threaded.thread)hole.thread=threaded.thread;
   if(threaded.diameter)hole.diameter=threaded.diameter;
   return hole;
  },
  thread,
  render({select,number,content,button,features,parameters,editing,invalidate,pickPosition,pickFace,cancelPick,message,current}){
   select('Drill solid',[['','Choose solid'],...features.filter(f=>f.id!==editing?.id).map(f=>[f.id,f.id])],state.input,value=>{state.input=value;state.face=null;current.render();});
   if(pickPosition)content.append(button('Place hole on face in canvas',async()=>{
    if(!state.input)throw Error('Choose a solid to drill first.');
    invalidate();const valid=current.checkpoint(),input=state.input;message('Click the hole entry on a planar source face. Escape cancels.');
    const picked=await pickPosition(input);if(!valid())return;
    if(!picked){message('Hole placement canceled. Draft retained.');return;}
    if(picked.target!==input||!Array.isArray(picked.position_mm)||picked.position_mm.length!==3||!picked.position_mm.every(Number.isFinite)||!Array.isArray(picked.axis)||picked.axis.length!==3||!picked.axis.every(Number.isFinite)||Math.hypot(...picked.axis)===0)throw Error('The picked hole placement does not match the drilled solid.');
    state.position=[...picked.position_mm];state.axis=[...picked.axis];state.placement='literal';current.render();message('Hole entry selected at a fixed family-coordinate position. The axis points into the face. Preview checks every instance; moving the face requires picking again.');
   }),button('Cancel hole placement',()=>{invalidate();cancelPick();message('Hole placement canceled. Draft retained.');}));
   if(original)select('Hole placement source',[['preserve','Keep existing position and axis formulas'],['literal','Entered position and axis']],state.placement,value=>{state.placement=value;current.render();});
   for(const [i,key] of ['X','Y','Z'].entries())number(`Hole position ${key} (mm)`,Number.isFinite(state.position[i])?state.position[i]:'',value=>state.position[i]=value).disabled=state.placement==='preserve';
   for(const [i,key] of ['X','Y','Z'].entries())number(`Hole axis ${key}`,Number.isFinite(state.axis[i])?state.axis[i]:'',value=>state.axis[i]=value).disabled=state.placement==='preserve';
   select('Hole extent',[['blind','Blind'],['through_all','Through all (both directions)'],['up_to_next','Up to next face'],['up_to_face','Up to selected face']],state.extent,value=>{state.extent=value;current.render();});
   function field(key){
    const entry=values[key],angle=angular(key),choices=[['literal',angle?'Entered degrees':'Entered length'],...parameters.filter(p=>p.parameter_type?.scalar===(angle?'scalar':'length')).map(p=>[`parameter:${p.id}`,`Parameter${angle?' (radians)':''}: ${p.id}`])];
    if(authored[key])choices.push(['preserve','Keep existing formula']);
    select(`Hole ${key} source`,choices,entry.source,value=>{entry.source=value;current.render();});
    number(`Hole ${key} (${angle?'degrees':'mm'})`,entry.source==='literal'&&Number.isFinite(entry.value)?entry.value:'',value=>entry.value=value).disabled=entry.source!=='literal';
   }
   const drill=thread.drillNote();
   if(drill){const follows=document.createElement('p');follows.textContent=drill;content.append(follows);}else field('diameter');
   if(state.extent==='blind')field('depth');
   select('Hole bottom',[['flat','Flat'],['drill_point','Drill point (blind only)']],state.bottom,value=>{state.bottom=value;current.render();});
   if(state.bottom==='drill_point')field('drill point angle');
   select('Hole entry finish',[['plain','Plain bore'],['counterbore','Counterbore'],['countersink','Countersink']],state.finish,value=>{state.finish=value;current.render();});
   if(state.finish==='counterbore'){
    field('counterbore diameter');field('counterbore depth');
    select('Counterbore floor relief',[['none','No relief'],['relief','60° floor relief']],state.relief?'relief':'none',value=>{state.relief=value==='relief';current.render();});
    if(state.relief)field('relief diameter');
   }else if(state.finish==='countersink'){field('countersink diameter');field('countersink angle');}
   if(state.extent==='up_to_face'){
    const note=document.createElement('p');note.textContent=state.face?'Limiting face selector retained.':'Pick a planar limiting face on the drilled solid.';content.append(note);
    if(pickFace)content.append(button('Pick hole limiting face in canvas',async()=>{
     if(!state.input)throw Error('Choose a solid to drill first.');
     invalidate();const valid=current.checkpoint(),input=state.input;message('Click a planar limiting face. Escape cancels the pick.');
     const picked=await pickFace(input);if(!valid())return;
     if(!picked){message('Face picking canceled. Hole draft retained.');return;}
     if(picked.target!==input||!picked.face)throw Error('The picked face does not match the drilled solid.');
     state.face=structuredClone(picked.face);current.render();message(picked.binding==='anchored_center'?'Face selected with a fixed center anchor. Moving it requires picking again; Preview checks every instance.':'Hole limiting face selected. Preview checks every instance.');
    }),button('Cancel hole face pick',()=>{invalidate();cancelPick();message('Face picking canceled. Hole draft retained.');}));
   }
   thread.render({select,number,content,current});
   const note=document.createElement('p');note.textContent='Position is the bore entry in family coordinates; the axis points into material. Through all spans both directions. Included angles use degrees; scalar parameters use radians. Drill points add a tip below the blind bore depth. Preview checks recess sizes, thread class limits, relief, tip containment and every instance.';content.append(note);
  }
 };
}
