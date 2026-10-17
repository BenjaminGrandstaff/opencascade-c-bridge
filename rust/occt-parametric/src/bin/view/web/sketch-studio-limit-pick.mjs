import {pickFace,pickFaceHit,project,toGltf} from './sketch-studio-plane.mjs';
import {pickSourceEdge,sourceEdgePaths} from './sketch-studio-sources.mjs';

export function createSolidSourcePicker(studio,mode,viewRect=()=>({left:0,top:0})) {
  let session=null;
  const valid=s=>session===s&&s.current()&&s.version===studio.version&&s.instance===studio.instance&&!studio.modelPending;
  function cancel(){const old=session;session=null;if(old){old.resolve(null);mode(false,old.kind);studio.redraw();}}
  function fail(s,error){if(session!==s)return;session=null;mode(false,s.kind);studio.redraw();s.reject(error);}
  async function load(s){
    try{
      const result=await studio.api('/api/sketch-source-view',{method:'POST',body:JSON.stringify({version:s.version,instance:s.instance,output:s.target})});
      if(!valid(s)){if(session===s)cancel();return;}
      const source=(result.scenes??[]).find(scene=>scene.kind==='solid'&&scene.instance===s.instance&&scene.feature===s.target);
      if(result.version!==s.version||!result.accepted||!result.source_view||!source||!Array.isArray(source.mesh))throw Error('The limiting feature view does not match this model and instance.');
      const min=[Infinity,Infinity,Infinity],max=[-Infinity,-Infinity,-Infinity];
      for(const triangle of source.mesh)for(const point of triangle.points)for(const [i,v] of toGltf(point).entries()){min[i]=Math.min(min[i],v);max[i]=Math.max(max[i],v);}
      s.bounds=min.every(Number.isFinite)&&max.every(Number.isFinite)?{min,max}:null;
      s.source=source;studio.redraw();studio.say(s.kind==='hole'?'Click the hole entry on a planar face of the amber source. Fit frames it; Escape cancels.':s.kind==='edge'?'Click an edge of the amber source solid. Fit frames it; drag to orbit; Escape cancels the pick.':'Click a planar face of the amber limiting feature. Fit frames the target; drag to orbit; Escape cancels the pick.');
    }catch(error){fail(s,error);}
  }
  function pick(target,current,kind='face'){
    cancel();
    return new Promise((resolve,reject)=>{
      const s={target,current,kind,resolve,reject,version:studio.version,instance:studio.instance,source:null,picking:true,pending:false,face:-1};session=s;
      mode(true,kind);studio.say('Loading the accepted source feature…');load(s);
    });
  }
  async function click(event){
    const s=session;
    if(!s?.picking||s.pending||event.button!==0||event.shiftKey)return;
    if(!valid(s)){cancel();return;}
    if(!s.source)return;
    const entry=s.kind==='hole'?pickFaceHit(s.source.mesh,studio.rayAt(event.clientX,event.clientY)):null;
    const face=s.kind==='hole'?entry?.face??-1:hit(s,event);
    if(face===null||face<0){studio.say(s.kind==='edge'?'That click missed the source edges.':'That click missed the limiting feature. Pick a planar face.',true);return;}
    s.pending=true;
    try{
      const edge=s.kind==='edge',id='solid-finish-edge';
      const result=await studio.api(edge?'/api/sketch-edge-pick':'/api/sketch-face-pick',{method:'POST',body:JSON.stringify({version:s.version,instance:s.instance,output:s.target,...(edge?{edge:face,id}:{face})})});
      if(!valid(s)){if(session===s)cancel();return;}
      if(result.version!==s.version||result.instance!==s.instance)throw Error('The picked source does not match this model and instance.');
      if(edge?(result.projection?.input!==s.target||result.projection.id!==id||!result.projection.edge):(result.face_support?.input!==s.target||!result.face_support.face))throw Error('The pick does not match the requested source feature.');
      let placement=null;
      if(s.kind==='hole'){
        const n=result.normal,o=result.origin_mm,p=entry.point_mm;
        if(!Array.isArray(n)||n.length!==3||!n.every(Number.isFinite)||!Array.isArray(o)||o.length!==3||!o.every(Number.isFinite)||!p.every(Number.isFinite))throw Error('The picked face has no finite position or normal.');
        const magnitude=Math.hypot(...n);if(!Number.isFinite(magnitude)||magnitude===0)throw Error('The picked face normal is invalid.');
        const normal=n.map(v=>v/magnitude),distance=p.reduce((sum,v,i)=>sum+(v-o[i])*normal[i],0);
        if(Math.abs(distance)>1e-5)throw Error('The clicked point does not lie on the resolved face. Pick again.');
        placement={position_mm:p.map((v,i)=>v-distance*normal[i]),axis:normal.map(v=>-v)};s.placement=placement;
      }
      s.face=face;s.picking=false;mode(false,s.kind);studio.redraw();
      s.resolve(edge?{target:s.target,edge:result.projection.edge,binding:result.binding,center_mm:result.center_mm,length_mm:result.length_mm}:{...placement,target:s.target,face:result.face_support.face,binding:result.binding,normal:result.normal,origin_mm:result.origin_mm});
      studio.say(s.kind==='hole'?'Hole entry selected at a fixed family-coordinate position, directed into the face. Preview checks every instance.':edge?'Edge selected. Preview checks the finishing operation in every instance.':'Limiting face selected. Preview verifies full forward coverage in every instance.');
    }catch(error){if(valid(s))studio.say(`Source pick rejected: ${error.message}`,true);}
    finally{s.pending=false;}
  }
  function hit(s,event){
    if(s.kind!=='edge')return pickFace(s.source.mesh,studio.rayAt(event.clientX,event.clientY));
    const box=viewRect();return pickSourceEdge(s.source,studio.viewport(),[event.clientX-box.left,event.clientY-box.top]);
  }
  function move(event){const s=session;if(!s?.picking||s.pending||!s.source||!valid(s))return;const face=hit(s,event);if(face!==s.face){s.face=face;studio.redraw();}}
  studio.limitFaceBounds=()=>session&&valid(session)?session.bounds??null:null;
  for(const name of ['solid-limit-face','solid-finish-edge'])studio.tools.set(name,{click,move,cancel});
  const decorate=studio.decorate;
  studio.decorate=context=>{
    const nodes=decorate?.(context)??[],s=session;
    if(!s?.source||!valid(s))return nodes;
    for(const {d,edge} of sourceEdgePaths(s.source,context))if(d)nodes.push(context.element('path',{d,fill:'none',stroke:s.kind==='edge'&&edge===s.face?'#176bd1':'#c77912','stroke-width':s.kind==='edge'&&edge===s.face?4:2,'stroke-dasharray':s.kind==='edge'&&edge===s.face?'':'5 3','pointer-events':'none',...(s.kind==='edge'?{'data-finish-edge':edge,'data-edge-selected':edge===s.face}:{'data-limit-edge':edge})}));
    if(s.kind!=='edge')for(const triangle of s.source.mesh)if(triangle.face===s.face){const points=triangle.points.map(p=>project(context.matrix,toGltf(p),context.width,context.height));if(points.every(Boolean))nodes.push(context.element('path',{d:points.map(([x,y],i)=>`${i?'L':'M'}${x} ${y}`).join(' ')+' Z',fill:'#c77912','fill-opacity':0.25,stroke:'none','pointer-events':'none','data-limit-face':s.face}));}
    if(s.placement){
      const p=s.placement.position_mm,diagonal=s.bounds?Math.hypot(...s.bounds.max.map((v,i)=>v-s.bounds.min[i]))*1000:10;
      const end=p.map((v,i)=>v+s.placement.axis[i]*diagonal*0.15),a=project(context.matrix,toGltf(p),context.width,context.height),b=project(context.matrix,toGltf(end),context.width,context.height);
      if(a){nodes.push(context.element('circle',{cx:a[0],cy:a[1],r:6,fill:'#176bd1',stroke:'white','stroke-width':2,'pointer-events':'none','data-hole-position':true}));if(b)nodes.push(context.element('path',{d:`M${a[0]} ${a[1]} L${b[0]} ${b[1]}`,stroke:'#176bd1','stroke-width':3,'pointer-events':'none','data-hole-axis':true}));}
    }
    return nodes;
  };
  return {pick,cancel,click,get active(){return Boolean(session?.picking);},get source(){return session?.source??null;}};
}

export const createLimitFacePicker=createSolidSourcePicker;
