import {project,toGltf} from './sketch-studio-plane.mjs';
const point=p=>Array.isArray(p)&&p.length===3&&p.every(Number.isFinite);
export function loftSeamGuides(annotations,instance,output){
 const scene=annotations?.scenes?.find(s=>s.kind==='solid'&&s.instance===instance&&s.feature===output),seen=new Set(),result=[];
 for(const a of scene?.annotations??[]){
  const d=a.detail;
  if(d?.measurement!=='loft_profile_seam'||typeof d.profile!=='string'||seen.has(d.profile)||!Number.isInteger(d.station)||d.station<0||d.station>=64||d.hole!==null&&(!Number.isInteger(d.hole)||d.hole<0||d.hole>=100)||typeof d.reverse!=='boolean'||d.coordinate_system!=='family-local mm'||!Array.isArray(a.anchors)||a.anchors.length!==5||!a.anchors.every(point))continue;
  seen.add(d.profile);result.push({profile:d.profile,station:d.station,hole:d.hole,reverse:d.reverse,points:structuredClone(a.anchors)});
 }
 return result;
}
export function drawLoftSeamGuides(guides,{matrix,width,height,element}){
 const nodes=[];
 for(const g of guides){
  const points=g.points.map(p=>project(matrix,toGltf(p),width,height));if(!points.every(Boolean))continue;
  const color=g.hole===null?'#087f83':'#b06a12',attrs={'pointer-events':'none','data-loft-seam':g.profile},start=points[0];
  nodes.push(element('path',{...attrs,d:points.map(([x,y],i)=>`${i?'L':'M'}${x} ${y}`).join(' '),fill:'none',stroke:color,'stroke-width':3}));
  const b=points.at(-1),a=points.at(-2),length=Math.hypot(b[0]-a[0],b[1]-a[1]);
  if(length>=1){const dx=(b[0]-a[0])/length,dy=(b[1]-a[1])/length,size=Math.min(8,length);nodes.push(element('path',{...attrs,d:`M${b[0]} ${b[1]} L${b[0]-size*dx+size*.5*dy} ${b[1]-size*dy-size*.5*dx} L${b[0]-size*dx-size*.5*dy} ${b[1]-size*dy+size*.5*dx} Z`,fill:color}));}
  nodes.push(element('circle',{...attrs,cx:start[0],cy:start[1],r:6,fill:'white',stroke:color,'stroke-width':3}));
  const label=element('text',{...attrs,x:start[0]+10,y:start[1]-10,fill:color,'font-size':12,'paint-order':'stroke',stroke:'white','stroke-width':3,'data-loft-seam-label':g.profile});label.textContent=`${g.hole===null?'Outer':`Hole ${g.hole+1}`} station ${g.station+1} start`;nodes.push(label);
 }
 return nodes;
}
