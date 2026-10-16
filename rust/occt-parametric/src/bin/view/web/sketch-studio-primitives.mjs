// Additional studio gestures author existing sketch records. Preview paths are
// bounded display approximations; native regeneration validates exact geometry.
import {drawPrimitive,ellipseMinor} from './canvas-tools.mjs';
import {geometryIds} from './constraint-controls.mjs';
import {geometryNames} from './geometry-names.mjs';
import {inheritGeneratedReferences} from './generated-points.mjs';
export const ADDITIONAL = new Set(['elliptical_arc','conic','text','arc_three','center_rectangle','control_spline']);
const mm=value=>({literal:{value,dimension:'length',unit:'millimeter'}});
const scalar=value=>({literal:{value,dimension:'scalar',unit:null}});
const norm=v=>Math.hypot(...v),sub=(a,b)=>a.map((v,i)=>v-b[i]);
const cross=(a,b)=>a[0]*b[1]-a[1]*b[0];
const finite=click=>Array.isArray(click?.xy)&&click.xy.length===2&&click.xy.every(Number.isFinite);

// Translate near the first point for numerical stability far from the origin.
export function threePointCircle(clicks){
 if(clicks.length!==3||clicks.some(c=>!finite(c)))throw Error('Choose arc start, a point on the arc, then its end.');
 const [a,b,c]=clicks.map(c=>c.xy),u=sub(b,a),v=sub(c,a),scale=Math.max(norm(u),norm(v));
 const determinant=2*cross(u,v);
 if(!scale||Math.abs(determinant)<=1e-10*scale*scale)throw Error('Three-point arcs need three distinct, non-collinear points.');
 const offset=[(norm(u)**2*v[1]-norm(v)**2*u[1])/determinant,(u[0]*norm(v)**2-v[0]*norm(u)**2)/determinant];
 const center=a.map((x,i)=>x+offset[i]);if(!center.every(Number.isFinite))throw Error('The three-point arc is too large or ill-conditioned.');return {center,clockwise:cross(u,v)<0};
}
function ellipseData(clicks){
 const center=clicks[0].xy,major=clicks[1].xy,minor=ellipseMinor(clicks).xy;
 const u=sub(major,center),a=norm(u),b=norm(sub(minor,center)),v=[-u[1]*b/a,u[0]*b/a];
 const angle=click=>{const d=sub(click.xy,center);return Math.atan2((d[0]*v[0]+d[1]*v[1])/(b*b),(d[0]*u[0]+d[1]*u[1])/(a*a));};
 const at=t=>center.map((x,i)=>x+u[i]*Math.cos(t)+v[i]*Math.sin(t));
 const endpoint=click=>{const xy=at(angle(click));if(norm(sub(click.xy,center))<=1e-9)throw Error('Choose an ellipse endpoint direction away from its center.');if(click.point&&norm(sub(xy,click.xy))>1e-6)throw Error('A snapped ellipse endpoint must lie on the ellipse.');return {...click,xy:click.point?click.xy:xy};};
 return {minor,angle,at,endpoint};
}
export function splineRecipe(poles,{degree=3,weights='',closed=false}={}){
 if(!Number.isInteger(degree)||degree<1||degree>5)throw Error('Control spline degree must be an integer from 1 to 5.');
 if(poles.length<Math.max(degree+1,closed?3:2)||poles.length+(closed?1:0)>256)throw Error(`A degree ${degree} control spline needs at least ${degree+1} poles and at most 256 including its closing pole.`);
 if(poles.some(p=>!Array.isArray(p)||p.length!==2||!p.every(Number.isFinite)))throw Error('Control poles must be finite.');
 for(let i=1;i<poles.length;i++)if(norm(sub(poles[i],poles[i-1]))<=1e-9)throw Error('Adjacent control poles must be distinct.');
 const parsed=typeof weights==='string'?(weights.trim()?weights.split(/[\s,]+/).map(Number):[]):weights;
 if(!Array.isArray(parsed)||parsed.length&&parsed.length!==poles.length||parsed.some(w=>!Number.isFinite(w)||w<=0))throw Error('Enter one positive weight per pole, or leave weights empty.');
 const points=poles.map(p=>[...p]),w=parsed.length?[...parsed]:poles.map(()=>1);
 if(closed){if(norm(sub(poles[0],poles.at(-1)))<=1e-9)throw Error('Use Closed instead of repeating the first pole.');points.push([...poles[0]]);w.push(w[0]);}
 const spans=points.length-degree,knots=Array.from({length:spans+1},(_,i)=>i/spans),multiplicities=knots.map((_,i)=>i===0||i===spans?degree+1:1);
 return {points,weights:w,basis:{degree,knots,multiplicities,...(parsed.length?{weights:w.map(scalar)}: {})}};
}
// O(samples * degree² + poles), with 65 samples, degree <=5 and <=256 poles.
function splinePath(recipe){
 const {points,weights,basis:{degree,knots,multiplicities}}=recipe,U=knots.flatMap((k,i)=>Array(multiplicities[i]).fill(k)),n=points.length-1;
 return Array.from({length:65},(_,i)=>{
  const t=i/64,span=Math.min(n,degree+Math.floor(t*(points.length-degree)));
  const d=Array.from({length:degree+1},(_,j)=>{const k=span-degree+j;return [...points[k].map(x=>x*weights[k]),weights[k]];});
  for(let r=1;r<=degree;r++)for(let j=degree;j>=r;j--){const k=span-degree+j,den=U[k+degree-r+1]-U[k],a=den?(t-U[k])/den:0;d[j]=d[j].map((x,c)=>(1-a)*d[j-1][c]+a*x);}
  return [d[degree][0]/d[degree][2],d[degree][1]/d[degree][2],0];
 });
}
export function drawAdditional(definition,tool,clicks,options={}){
 if(!ADDITIONAL.has(tool)||clicks.some(c=>!finite(c)))throw Error('Use a supported tool and finite sketch coordinates.');
 const next=structuredClone(definition),used=geometryNames(definition),known=new Set(geometryIds(definition,'points')),created=[],ids=[];
 const counters=new Map();
 const name=prefix=>{let i=counters.get(prefix)??1;while(used.has(`${prefix}${i}`))i++;counters.set(prefix,i+1);const id=`${prefix}${i}`;used.add(id);return id;};
 const point=click=>{if(click.point){if(!known.has(click.point))throw Error('The snapped point is unavailable. Preview to refresh references.');return click.point;}const id=name('P');(next.points??=[]).push({id,x:mm(click.xy[0]),y:mm(click.xy[1]),fixed:false});known.add(id);created.push(id);return id;};
 const record=(collection,prefix,data)=>{const id=name(prefix);(next[collection]??=[]).push({id,...data});created.push(id);return id;};
 if(tool==='center_rectangle'){
  if(clicks.length!==2)throw Error('Choose rectangle center, then a corner.');
  const [c,e]=clicks,a={xy:c.xy.map((x,i)=>2*x-e.xy[i]),point:null};
  inheritGeneratedReferences(definition,next);
  const drawn=drawPrimitive(next,'rectangle',[a,e],{fixed:false,profile:options.profile});
  const centerId=c.point;if(centerId&&!known.has(centerId))throw Error('Choose a known center point.');
  // A diagonal is construction geometry (outside the saved profile).
  const d=drawn.definition;for(const id of geometryNames(d))used.add(id);const center=centerId??name('P');if(!centerId)(d.points??=[]).push({id:center,x:mm(c.xy[0]),y:mm(c.xy[1]),fixed:false});
  // Reserve names created by drawPrimitive before naming the diagonal.
  const names=geometryNames(d);let i=1;while(names.has(`L${i}`))i++;const line=`L${i}`,sides=d.lines.slice(-4);
  d.lines.push({id:line,start:sides[0].start,end:sides[1].end});(d.constraints??=[]).push({midpoint:{point:center,line}});
  inheritGeneratedReferences(definition,d);return {definition:d,created:[...drawn.created,...(!centerId?[center]:[]),line],clickIds:[center,sides[1].end]};
 }
 if(tool==='arc_three'){
  const {center,clockwise}=threePointCircle(clicks),[start,middle,end]=clicks.map(point),centerId=point({xy:center});
  const id=record('arcs','A',{center:centerId,start,end,clockwise});(next.constraints??=[]).push({point_on_curve:{point:middle,curve:id}});ids.push(start,middle,end);
 }else if(tool==='elliptical_arc'){
  if(clicks.length!==5)throw Error('Choose ellipse center, major axis, minor width, start and end directions.');
  const e=ellipseData(clicks),ends=clicks.slice(3).map(e.endpoint);if(norm(sub(ends[0].xy,ends[1].xy))<=1e-9)throw Error('Elliptical arc endpoints must differ. Use Ellipse for a full loop.');
  const center=point(clicks[0]),major=point(clicks[1]),minor=point({...clicks[2],xy:e.minor}),start=point(ends[0]),end=point(ends[1]);
  record('ellipses','E',{center,major,minor,start,end,clockwise:(Math.sin(e.angle(ends[1])-e.angle(ends[0]))<0)!==Boolean(options.flip)});ids.push(center,major,minor,start,end);
 }else if(tool==='conic'){
  if(clicks.length!==3)throw Error('Choose conic start, tangent intersection, then end.');
  const rho=options.rho??.5;if(!Number.isFinite(rho)||rho<=0||rho>=1)throw Error('Conic rho must be strictly between 0 and 1.');
  const [a,b,c]=clicks.map(c=>c.xy),scale=Math.max(norm(sub(b,a)),norm(sub(c,a)));if(!scale||Math.abs(cross(sub(b,a),sub(c,a)))<=1e-10*scale*scale)throw Error('Conic start, control and end must be non-collinear.');
  const [start,control,end]=clicks.map(point),id=name('Conic');(next.shapes??=[]).push({conic:{id,start,control,end,rho:scalar(rho)}});created.push(id);ids.push(start,control,end);
 }else if(tool==='control_spline'){
  const recipe=splineRecipe(clicks.map(c=>c.xy),options),points=clicks.map(point);if(options.closed)points.push(points[0]);
  const id=record('splines','S',{points,basis:recipe.basis});ids.push(...points.slice(0,clicks.length));if(options.profile&&options.closed){if(next.holes?.length||next.regions?.length||next.region_discovery||next.region_edits?.length||next.profile_operations?.length)throw Error('Use profile controls for sketches with regions or profile edits.');next.profile=[id];}
 }else if(tool==='text'){
  if(clicks.length!==1)throw Error('Choose a text baseline anchor.');
  const {text='Text',font='Liberation Sans',style='regular',height=5,angle=0,align='start'}=options;
  if(typeof text!=='string'||!text.trim()||[...text].length>1000||/[\p{Cc}\p{Zl}\p{Zp}]/u.test(text))throw Error('Text needs 1 to 1000 printable characters on one line.');
  if(typeof font!=='string'||!font.trim()||!['regular','bold','italic','bold_italic'].includes(style)||!['start','center','end'].includes(align))throw Error('Choose a font family, style and alignment.');
  if(!Number.isFinite(height)||height<=0||!Number.isFinite(angle))throw Error('Text height must be positive and its angle finite.');
  const anchor=point(clicks[0]);record('texts','Text',{text,font,style,height:mm(height),anchor,angle:scalar(angle*Math.PI/180),align});ids.push(anchor);
 }
 inheritGeneratedReferences(definition,next);return {definition:next,created,clickIds:ids};
}
export function additionalGhost(tool,clicks,cursor,options={}){
 if(!cursor)return [];const c=[...clicks,cursor],fallback=c.map(p=>[...p.xy,0]);
 try{
  if(tool==='center_rectangle'&&c.length>=2){const [a,b]=c.map(p=>p.xy),o=a.map((x,i)=>2*x-b[i]);return [[...o,0],[b[0],o[1],0],[...b,0],[o[0],b[1],0],[...o,0]];}
  if(tool==='arc_three'&&c.length===3){const {center,clockwise}=threePointCircle(c),radius=norm(sub(c[0].xy,center)),a=Math.atan2(c[0].xy[1]-center[1],c[0].xy[0]-center[0]);let span=Math.atan2(c[2].xy[1]-center[1],c[2].xy[0]-center[0])-a;while(clockwise?span>=0:span<=0)span+=clockwise?-2*Math.PI:2*Math.PI;return Array.from({length:65},(_,i)=>[center[0]+radius*Math.cos(a+span*i/64),center[1]+radius*Math.sin(a+span*i/64),0]);}
  if(tool==='elliptical_arc'&&c.length>=3){const e=ellipseData(c);let a=c.length>=4?e.angle(c[3]):0,span=2*Math.PI;if(c.length>=5){span=e.angle(c[4])-a;const clockwise=(Math.sin(span)<0)!==Boolean(options.flip);while(clockwise?span>=0:span<=0)span+=clockwise?-2*Math.PI:2*Math.PI;}return Array.from({length:65},(_,i)=>[...e.at(a+span*i/64),0]);}
  if(tool==='conic'&&c.length===3){const rho=options.rho??.5,w=rho/(1-rho);if(!(w>0)||!Number.isFinite(w))return fallback;return Array.from({length:65},(_,i)=>{const t=i/64,a=(1-t)**2,b=2*w*t*(1-t),d=t*t;return [0,1].map(k=>(a*c[0].xy[k]+b*c[1].xy[k]+d*c[2].xy[k])/(a+b+d)).concat(0);});}
  if(tool==='control_spline')return splinePath(splineRecipe(c.map(p=>p.xy),options));
  // Native font outlines determine the glyphs; show only the baseline anchor.
  if(tool==='text')return [[...cursor.xy,0]];
 }catch{return fallback;}
 return fallback;
}
