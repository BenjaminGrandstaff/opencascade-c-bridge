import {project,toGltf} from './sketch-studio-plane.mjs';
const point=p=>Array.isArray(p)&&p.length===3&&p.every(Number.isFinite);
/** Only verified native annotations for this exact displayed output are eligible. */
export function loftTangentGuides(annotations,instance,output){
 const scene=annotations?.scenes?.find(s=>s.kind==='solid'&&s.instance===instance&&s.feature===output);
 const result=[],ends=new Set();
 for(const a of scene?.annotations??[]){
  const d=a.detail,end=d?.end;
  if(d?.measurement!=='loft_end_tangent'||!['start','end'].includes(end)||ends.has(end)||!point(d.origin_mm)||!point(d.derivative_mm)||!Number.isFinite(d.strength_mm)||d.strength_mm<=0||!Array.isArray(a.anchors)||a.anchors.length!==2||!a.anchors.every(point)||d.handle_scale!==1/3)continue;
  const n=Math.hypot(...d.derivative_mm),origin=a.anchors[end==='start'?0:1];
  if(!Number.isFinite(n)||n<=0||Math.abs(n-d.strength_mm)>1e-7*Math.max(1,n)||origin.some((v,i)=>Math.abs(v-d.origin_mm[i])>1e-7)||a.anchors[1].some((v,i)=>Math.abs(v-a.anchors[0][i]-d.derivative_mm[i]/3)>1e-7*Math.max(1,n)))continue;
  result.push({end,origin:[...origin],anchors:structuredClone(a.anchors),strength:d.strength_mm,mode:d.mode,profile:d.profile});ends.add(end);if(ends.size===2)break;
 }
 return result;
}
/** Read-only Hermite handles: arrows follow station order at both ends. */
export function drawLoftTangentGuides(guides,context){
 const nodes=[],{matrix,width,height,element}=context;
 for(const g of guides){
  const a=project(matrix,toGltf(g.anchors[0]),width,height),b=project(matrix,toGltf(g.anchors[1]),width,height),o=project(matrix,toGltf(g.origin),width,height);if(!a||!b||!o)continue;
  const color=g.end==='start'?'#176bd1':'#8544bb',common={'pointer-events':'none','data-loft-tangent':g.end};
  nodes.push(element('path',{...common,d:`M${a[0]} ${a[1]} L${b[0]} ${b[1]}`,stroke:color,'stroke-width':3,fill:'none'}));
  const length=Math.hypot(b[0]-a[0],b[1]-a[1]);
  if(length>=2){const dx=(b[0]-a[0])/length,dy=(b[1]-a[1])/length,size=Math.min(9,length*.5);nodes.push(element('path',{...common,d:`M${b[0]} ${b[1]} L${b[0]-size*dx+size*.5*dy} ${b[1]-size*dy-size*.5*dx} L${b[0]-size*dx-size*.5*dy} ${b[1]-size*dy+size*.5*dx} Z`,fill:color}));}
  nodes.push(element('circle',{...common,cx:o[0],cy:o[1],r:5,fill:color,stroke:'white','stroke-width':2}));
  const text=element('text',{...common,x:o[0]+10,y:o[1]+(g.end==='start'?20:-12),fill:color,'font-size':13,'paint-order':'stroke',stroke:'white','stroke-width':3,'data-loft-tangent-label':g.end});text.textContent=`${g.end==='start'?'Start':'End'} tangent · strength ${Number(g.strength.toPrecision(6))} mm`;nodes.push(text);
 }
 return nodes;
}
