import {pickFace,project,toGltf} from './sketch-studio-plane.mjs';
import {sourceEdgePaths} from './sketch-studio-sources.mjs';

export function createLimitFacePicker(studio,mode) {
  let session=null;
  const valid=s=>session===s&&s.current()&&s.version===studio.version&&s.instance===studio.instance&&!studio.modelPending;
  function cancel(){const old=session;session=null;if(old){old.resolve(null);mode(false);studio.redraw();}}
  function fail(s,error){if(session!==s)return;session=null;mode(false);studio.redraw();s.reject(error);}
  async function load(s){
    try{
      const result=await studio.api('/api/sketch-source-view',{method:'POST',body:JSON.stringify({version:s.version,instance:s.instance,output:s.target})});
      if(!valid(s)){if(session===s)cancel();return;}
      const source=(result.scenes??[]).find(scene=>scene.kind==='solid'&&scene.instance===s.instance&&scene.feature===s.target);
      if(result.version!==s.version||!result.accepted||!result.source_view||!source||!Array.isArray(source.mesh))throw Error('The limiting feature view does not match this model and instance.');
      const min=[Infinity,Infinity,Infinity],max=[-Infinity,-Infinity,-Infinity];
      for(const triangle of source.mesh)for(const point of triangle.points)for(const [i,v] of toGltf(point).entries()){min[i]=Math.min(min[i],v);max[i]=Math.max(max[i],v);}
      s.bounds=min.every(Number.isFinite)&&max.every(Number.isFinite)?{min,max}:null;
      s.source=source;studio.redraw();studio.say('Click a planar face of the amber limiting feature. Fit frames the target; drag to orbit; Escape cancels the pick.');
    }catch(error){fail(s,error);}
  }
  function pick(target,current){
    cancel();
    return new Promise((resolve,reject)=>{
      const s={target,current,resolve,reject,version:studio.version,instance:studio.instance,source:null,picking:true,pending:false,face:-1};session=s;
      mode(true);studio.say('Loading the accepted limiting feature…');load(s);
    });
  }
  async function click(event){
    const s=session;
    if(!s?.picking||s.pending||event.button!==0||event.shiftKey)return;
    if(!valid(s)){cancel();return;}
    if(!s.source)return;
    const face=pickFace(s.source.mesh,studio.rayAt(event.clientX,event.clientY));
    if(face<0){studio.say('That click missed the limiting feature. Pick a planar face.',true);return;}
    s.pending=true;
    try{
      const result=await studio.api('/api/sketch-face-pick',{method:'POST',body:JSON.stringify({version:s.version,instance:s.instance,output:s.target,face})});
      if(!valid(s)){if(session===s)cancel();return;}
      if(result.version!==s.version||result.instance!==s.instance||result.face_support?.input!==s.target||!result.face_support.face)throw Error('The picked face does not match the limiting feature and instance.');
      s.face=face;s.picking=false;mode(false);studio.redraw();
      s.resolve({target:s.target,face:result.face_support.face,binding:result.binding,normal:result.normal,origin_mm:result.origin_mm});
      studio.say('Limiting face selected. Preview verifies full forward coverage in every instance.');
    }catch(error){if(valid(s))studio.say(`Face pick rejected: ${error.message}`,true);}
    finally{s.pending=false;}
  }
  function move(event){const s=session;if(!s?.picking||s.pending||!s.source||!valid(s))return;const face=pickFace(s.source.mesh,studio.rayAt(event.clientX,event.clientY));if(face!==s.face){s.face=face;studio.redraw();}}
  studio.limitFaceBounds=()=>session&&valid(session)?session.bounds??null:null;
  studio.tools.set('solid-limit-face',{click,move,cancel});
  const decorate=studio.decorate;
  studio.decorate=context=>{
    const nodes=decorate?.(context)??[],s=session;
    if(!s?.source||!valid(s))return nodes;
    for(const {d,edge} of sourceEdgePaths(s.source,context))if(d)nodes.push(context.element('path',{d,fill:'none',stroke:'#c77912','stroke-width':2,'stroke-dasharray':'5 3','pointer-events':'none','data-limit-edge':edge}));
    for(const triangle of s.source.mesh)if(triangle.face===s.face){const points=triangle.points.map(p=>project(context.matrix,toGltf(p),context.width,context.height));if(points.every(Boolean))nodes.push(context.element('path',{d:points.map(([x,y],i)=>`${i?'L':'M'}${x} ${y}`).join(' ')+' Z',fill:'#c77912','fill-opacity':0.25,stroke:'none','pointer-events':'none','data-limit-face':s.face}));}
    return nodes;
  };
  return {pick,cancel,click,get active(){return Boolean(session?.picking);},get source(){return session?.source??null;}};
}
