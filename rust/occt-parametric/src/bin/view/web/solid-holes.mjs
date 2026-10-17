// Hole controls keep existing finishing/thread metadata and opaque formulas intact.
const length=value=>({literal:{value,dimension:'length',unit:'millimeter'}});
const mm=q=>q?.dimension==='length'?q.value*({millimeter:1,centimeter:10,meter:1000,inch:25.4}[q.unit]??NaN):NaN;
const vector=(values,dimension)=>{
 if(values.length!==3||!values.every(Number.isFinite)||dimension==='scalar'&&Math.hypot(...values)===0)throw Error('Enter a finite hole position and nonzero axis.');
 return {components:Object.fromEntries(['x','y','z'].map((k,i)=>[k,{literal:{value:values[i],dimension,unit:dimension==='length'?'millimeter':null}}]))};
};
const read=(v,dimension)=>['x','y','z'].map(k=>{const q=v?.literal?.[k]??v?.components?.[k]?.literal;return dimension==='length'?mm(q):q?.dimension==='scalar'?q.value:NaN;});
export function createHoleControls(saved=null,target='') {
 const original=structuredClone(saved),position=read(saved?.position,'length'),axis=read(saved?.axis,'scalar');
 const state={input:saved?.input??target,position:saved?position:[0,0,0],axis:saved?axis:[0,0,-1],placement:saved?'preserve':'literal',extent:saved?.extent?.blind?'blind':saved?.extent?.up_to_face?'up_to_face':saved?.extent??'blind',face:saved?.extent?.up_to_face?.face??null};
 const fields={diameter:saved?.diameter??length(5),depth:saved?.extent?.blind?.depth??length(5)};
 const values=Object.fromEntries(Object.entries(fields).map(([key,value])=>[key,{value:value.literal?mm(value.literal):NaN,source:saved?'preserve':'literal'}]));
 function expression(key){const f=values[key],value=f.source==='preserve'?fields[key]:f.source.startsWith('parameter:')?{parameter:f.source.slice(10)}:length(f.value);if(value?.literal&&(!Number.isFinite(mm(value.literal))||mm(value.literal)<=0))throw Error('Hole diameter and depth must be positive lengths.');return structuredClone(value);}
 return {
  state,
  definition(){
   if(!state.input)throw Error('Choose a solid to drill.');
   if(!['blind','through_all','up_to_next','up_to_face'].includes(state.extent))throw Error('Choose a hole extent.');
   if(state.extent==='up_to_face'&&!state.face)throw Error('Pick a limiting face.');
   return {...original,input:state.input,position:state.placement==='preserve'?structuredClone(original.position):vector(state.position,'length'),axis:state.placement==='preserve'?structuredClone(original.axis):vector(state.axis,'scalar'),diameter:expression('diameter'),extent:state.extent==='blind'?{blind:{depth:expression('depth')}}:state.extent==='up_to_face'?{up_to_face:{face:structuredClone(state.face)}}:state.extent,bottom:original?.bottom??'flat',finish:original?.finish??'plain'};
  },
  render({select,number,content,button,features,parameters,editing,invalidate,pickFace,cancelPick,message,current}){
   select('Drill solid',[['','Choose solid'],...features.filter(f=>f.id!==editing?.id).map(f=>[f.id,f.id])],state.input,value=>{state.input=value;state.face=null;current.render();});
   if(original)select('Hole placement source',[['preserve','Keep existing position and axis formulas'],['literal','Entered position and axis']],state.placement,value=>{state.placement=value;current.render();});
   for(const [i,key] of ['X','Y','Z'].entries())number(`Hole position ${key} (mm)`,Number.isFinite(state.position[i])?state.position[i]:'',value=>state.position[i]=value).disabled=state.placement==='preserve';
   for(const [i,key] of ['X','Y','Z'].entries())number(`Hole axis ${key}`,Number.isFinite(state.axis[i])?state.axis[i]:'',value=>state.axis[i]=value).disabled=state.placement==='preserve';
   select('Hole extent',[['blind','Blind'],['through_all','Through all (both directions)'],['up_to_next','Up to next face'],['up_to_face','Up to selected face']],state.extent,value=>{state.extent=value;current.render();});
   for(const key of ['diameter',...(state.extent==='blind'?['depth']:[])]){
    const field=values[key],choices=[['literal','Entered length'],...parameters.filter(p=>p.parameter_type?.scalar==='length').map(p=>[`parameter:${p.id}`,`Parameter: ${p.id}`])];
    if(original)choices.push(['preserve','Keep existing formula']);
    select(`Hole ${key} source`,choices,field.source,value=>{field.source=value;current.render();});
    number(`Hole ${key} (mm)`,field.source==='literal'?field.value:'',value=>field.value=value).disabled=field.source!=='literal';
   }
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
   const note=document.createElement('p');note.textContent='Position is the bore entry in family coordinates; the axis points into material. Through all spans both directions. Existing drill points, counterbores, countersinks and threads are retained. Preview validates every instance.';content.append(note);
  }
 };
}
