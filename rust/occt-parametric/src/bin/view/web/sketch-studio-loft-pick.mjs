import {project,toGltf,fromGltf,directionFromGltf,dot,sub,add,scale} from './sketch-studio-plane.mjs';
const point=p=>Array.isArray(p)&&p.length===3&&p.every(Number.isFinite);
const distance=(a,b)=>Math.hypot(...a.map((v,i)=>v-b[i]));
/** One bounded accepted boundary snapshot, with native validation only on click. */
export function createLoftAnchorPicker(studio,mode,viewRect=()=>({left:0,top:0})){
 let session=null;
 const valid=s=>session===s&&s.current()&&s.version===studio.version&&s.instance===studio.instance&&!studio.modelPending;
 const cancel=()=>{const old=session;session=null;if(old){old.resolve(null);mode(false);studio.redraw();}};
 const matches=(s,d)=>d?.version===s.version&&d.instance===s.instance&&d.profile===s.profile&&d.accepted===true&&d.coordinate_system==='family-local mm'&&(d.split_count??0)===s.splits.length;
 async function load(s){try{
  const d=await studio.api('/api/loft-profile-pick',{method:'POST',body:JSON.stringify({version:s.version,instance:s.instance,profile:s.profile,...(s.splits.length?{split_points:s.splits}:{})})});
  if(!valid(s)){if(session===s)cancel();return;}
  if(!matches(s,d)||typeof d.continuous!=='boolean'||!point(d.origin_mm)||!point(d.normal)||Math.hypot(...d.normal)<1e-9||!Array.isArray(d.vertices)||d.vertices.length<1||d.vertices.length>256||!d.vertices.every(point)||!Array.isArray(d.lines)||d.lines.length<1||d.lines.length>128||d.lines.some(line=>!Array.isArray(line)||line.length<2||line.length>257||!line.every(point)))throw Error('The loft boundary does not match this model and profile.');
  s.source=d;
  const all=d.lines.flat(),min=[Infinity,Infinity,Infinity],max=[-Infinity,-Infinity,-Infinity];
  for(const p of all)for(const [i,v] of toGltf(p).entries()){min[i]=Math.min(min[i],v);max[i]=Math.max(max[i],v);}
  s.bounds={min,max};s.diagonal=Math.hypot(...max.map((v,i)=>v-min[i]))*1000;
  studio.redraw();studio.say(d.continuous?'Click the amber profile boundary to place its seam. Drag to orbit; Fit frames it; Escape cancels.':'Click an amber profile corner to choose its start. Drag to orbit; Fit frames it; Escape cancels.');
 }catch(error){if(session!==s)return;session=null;mode(false);studio.redraw();s.reject(error);}}
 function pick(profile,current,splits=[]){cancel();return new Promise((resolve,reject)=>{const s={profile,current,splits:structuredClone(splits),version:studio.version,instance:studio.instance,resolve,reject,source:null,pending:false,picking:true};session=s;mode(true);studio.say('Loading the accepted loft profile…');load(s);});}
 function hit(s,event){
  const context=studio.viewport(),box=viewRect(),at=[event.clientX-box.left,event.clientY-box.top];let best=null;
  if(!s.source.continuous){
   for(const p of s.source.vertices){const screen=project(context.matrix,toGltf(p),context.width,context.height);if(!screen)continue;const d=Math.hypot(screen[0]-at[0],screen[1]-at[1]);if(d<=12&&(!best||d<best.distance))best={position:p,distance:d};}
  }else for(const line of s.source.lines)for(let i=1;i<line.length;i++){
   const a=project(context.matrix,toGltf(line[i-1]),context.width,context.height),b=project(context.matrix,toGltf(line[i]),context.width,context.height);if(!a||!b)continue;
   const dx=b[0]-a[0],dy=b[1]-a[1],length=dx*dx+dy*dy,t=length?Math.max(0,Math.min(1,((at[0]-a[0])*dx+(at[1]-a[1])*dy)/length)):0,d=Math.hypot(at[0]-a[0]-t*dx,at[1]-a[1]-t*dy);
   if(d<=12&&(!best||d<best.distance))best={position:line[i-1].map((v,k)=>v+t*(line[i][k]-v)),distance:d};
  }
  return best;
 }
 async function click(event){
  const s=session;if(!s?.picking||s.pending||event.button!==0||event.shiftKey)return;if(!valid(s)){cancel();return;}if(!s.source)return;
  const found=hit(s,event);if(!found){studio.say(s.source.continuous?'Pick closer to the amber boundary.':'Pick an existing amber corner.',true);return;}
  let hint=found.position;
  if(s.source.continuous){const ray=studio.rayAt(event.clientX,event.clientY),origin=fromGltf(ray.origin),direction=directionFromGltf(ray.direction),denominator=dot(direction,s.source.normal);if(Math.abs(denominator)>1e-9){const t=dot(sub(s.source.origin_mm,origin),s.source.normal)/denominator;if(t>0)hint=add(origin,scale(direction,t));}}
  const tolerance=distance(hint,found.position)+s.diagonal*.005+1e-6;s.pending=true;
  try{
   const d=await studio.api('/api/loft-profile-pick',{method:'POST',body:JSON.stringify({version:s.version,instance:s.instance,profile:s.profile,...(s.splits.length?{split_points:s.splits}:{}),position_mm:hint,maximum_distance_mm:tolerance})});
   if(!valid(s)){if(session===s)cancel();return;}
   if(!matches(s,d)||!point(d.position_mm)||!Number.isFinite(d.distance_mm)||d.distance_mm<0||d.distance_mm>tolerance||distance(hint,d.position_mm)>tolerance+1e-7)throw Error('The picked anchor does not match this profile.');
   s.selected=[...d.position_mm];s.picking=false;mode(false);studio.redraw();s.resolve({profile:s.profile,position_mm:[...d.position_mm]});studio.say('Loft start selected at a fixed family-coordinate position. Preview checks every instance.');
  }catch(error){if(valid(s))studio.say(`Loft start rejected: ${error.message}`,true);}finally{s.pending=false;}
 }
 const previousBounds=studio.limitFaceBounds;studio.limitFaceBounds=()=>session&&valid(session)?session.bounds??null:previousBounds?.()??null;
 studio.tools.set('solid-loft-anchor',{click,move:event=>{const s=session;if(s?.picking&&s.source&&valid(s)){s.hover=hit(s,event)?.position??null;studio.redraw();}},cancel});
 const decorate=studio.decorate;
 studio.decorate=context=>{
  const nodes=decorate?.(context)??[],s=session;if(!s?.source||!valid(s))return nodes;
  for(const line of s.source.lines){const points=line.map(p=>project(context.matrix,toGltf(p),context.width,context.height));if(points.every(Boolean))nodes.push(context.element('path',{d:points.map(([x,y],i)=>`${i?'L':'M'}${x} ${y}`).join(' '),fill:'none',stroke:'#c77912','stroke-width':3,'pointer-events':'none','data-loft-pick-profile':s.profile}));}
  for(const p of s.source.continuous?[]:s.source.vertices){const at=project(context.matrix,toGltf(p),context.width,context.height);if(at)nodes.push(context.element('circle',{cx:at[0],cy:at[1],r:5,fill:'white',stroke:'#c77912','stroke-width':2,'pointer-events':'none','data-loft-pick-corner':true}));}
  const selected=s.selected??s.hover;if(selected){const at=project(context.matrix,toGltf(selected),context.width,context.height);if(at)nodes.push(context.element('circle',{cx:at[0],cy:at[1],r:7,fill:'#087f83',stroke:'white','stroke-width':2,'pointer-events':'none','data-loft-picked-anchor':s.profile}));}
  return nodes;
 };
 return {pick,cancel,click,get active(){return Boolean(session?.picking);},get source(){return session?.source??null;}};
}
