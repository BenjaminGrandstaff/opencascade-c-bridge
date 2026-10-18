// Thread callouts on studio holes: a catalog tap size (the frozen Carr Lane
// V1 sizes the server lists, metric or Unified inch), its class and hand,
// the bore following the catalog's tap drill, and optional modeled thread
// geometry. Saved threads that are not catalog sizes are kept as they are.
const length=value=>({literal:{value,dimension:'length',unit:'millimeter'}});
const mm=q=>q?.dimension==='length'?q.value*({millimeter:1,centimeter:10,meter:1000,inch:25.4}[q.unit]??NaN):NaN;
const same=(a,b)=>Math.abs(a-b)<=1e-9*(1+Math.abs(b));

/** The catalog size a saved thread names: [system, index], or null. */
function catalogSize(thread,sizes) {
  const nominal=mm(thread?.nominal_diameter?.literal),pitch=mm(thread?.pitch?.literal);
  for(const system of ['metric','inch']){
    const index=(sizes?.[system]??[]).findIndex(size=>same(mm(size.nominal),nominal)&&same(mm(size.pitch),pitch));
    if(index>=0)return [system,index];
  }
  return null;
}

/** Thread state for a saved hole thread (or none) against the server's sizes. */
export function createThreadControls(saved=null,sizes={metric:[],inch:[]}) {
  const found=saved?catalogSize(saved,sizes):null;
  const state={
    mode:!saved?'none':found?found[0]:'preserve',
    index:found?found[1]:0,
    class:saved?.unified?.class??'2B',
    hand:saved?.handedness??'right',
    modeled:Boolean(saved?.modeled),
    length:mm(saved?.modeled?.length?.literal),
    // A new catalog thread drills the catalog's tap drill.
    drill:saved?'keep':'catalog',
  };
  const size=()=>sizes?.[state.mode]?.[state.index];
  return {
    state,
    /** The hole's `thread` (null removes it, undefined keeps the saved one)
     * and, when the bore follows the catalog, its diameter expression. */
    definition(){
      if(state.mode==='preserve')return {thread:undefined};
      if(state.mode==='none')return {thread:null};
      const chosen=size();
      if(!chosen)throw Error('Choose a thread size.');
      const left=state.hand==='left';
      let designation,unified;
      if(state.mode==='metric')designation=`${chosen.label}${left?'-LH':''}`;
      else if(chosen.designation){
        if(!['1B','2B'].includes(state.class))throw Error('Choose an internal thread class (1B or 2B).');
        designation=`${chosen.designation.replace(/-2B$/,`-${state.class}`)}${left?'-LH':''}`;
        unified={series:chosen.series,class:state.class};
      }else designation=`${chosen.label}${left?'-LH':''}`;
      const thread={designation,nominal_diameter:{literal:structuredClone(chosen.nominal)},pitch:{literal:structuredClone(chosen.pitch)},handedness:state.hand};
      if(unified)thread.unified=unified;
      if(state.modeled){
        if(Number.isFinite(state.length)&&state.length<=0)throw Error('A modeled thread length must be positive, or left blank to thread the whole bore.');
        thread.modeled=Number.isFinite(state.length)?{length:length(state.length)}:{};
      }
      const diameter=state.drill==='catalog'?{carr_lane_tap_drill_v1:{nominal_diameter:{literal:structuredClone(chosen.nominal)},pitch:{literal:structuredClone(chosen.pitch)},system:state.mode}}:undefined;
      return {thread,diameter};
    },
    /** When the bore follows the catalog's tap drill, a sentence saying so. */
    drillNote(){
      const chosen=['metric','inch'].includes(state.mode)&&state.drill==='catalog'?size():null;
      return chosen?`Hole diameter follows the ${chosen.designation??chosen.label} tap drill (${Number(chosen.drill_mm.toFixed(4))} mm).`:null;
    },
    render({select,number,content,current}){
      const choices=[['none','No thread'],['metric','Metric (ISO) tap size'],['inch','Unified inch tap size']];
      if(saved&&!found)choices.push(['preserve',`Keep existing thread ${saved.designation}`]);
      select('Thread',choices,state.mode,value=>{if(value!==state.mode)state.index=0;state.mode=value;current.render();});
      if(!['metric','inch'].includes(state.mode))return;
      const list=sizes?.[state.mode]??[];
      select('Thread size',list.map((size,index)=>[String(index),`${size.designation??size.label} · tap drill ${Number(size.drill_mm.toFixed(3))} mm`]),String(state.index),value=>{state.index=Number(value);current.render();});
      if(state.mode==='inch'&&size()?.designation)select('Thread class',[['2B','2B (general)'],['1B','1B (loose)']],state.class,value=>state.class=value);
      select('Thread hand',[['right','Right hand'],['left','Left hand (LH)']],state.hand,value=>state.hand=value);
      select('Bore diameter',[['catalog','Follow the catalog tap drill'],['keep','Keep the hole diameter']],state.drill,value=>{state.drill=value;current.render();});
      select('Thread geometry',[['callout','Callout only'],['modeled','Cut the thread form']],state.modeled?'modeled':'callout',value=>{state.modeled=value==='modeled';current.render();});
      if(state.modeled)number('Thread length (mm, blank for the whole bore)',Number.isFinite(state.length)?state.length:'',value=>state.length=value);
      const note=document.createElement('p');
      note.textContent='Sizes are the frozen Carr Lane V1 tap catalog. Unified callouts are checked against ASME B1.1 class limits on Preview; a cut thread uses the ISO 68-1 basic profile.';
      content.append(note);
    },
  };
}
