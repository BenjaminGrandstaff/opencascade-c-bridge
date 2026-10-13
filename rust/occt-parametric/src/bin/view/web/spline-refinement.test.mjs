import {test} from 'node:test';
import assert from 'node:assert/strict';
import {refineSpline} from './spline-refinement.mjs';
const literal=(value,dimension='scalar')=>({literal:{value,dimension,unit:dimension==='length'?'millimeter':null}});
const expr=(e,parameters)=>e.literal?.value??(e.parameter?parameters[e.parameter]:e.add?expr(e.add[0],parameters)+expr(e.add[1],parameters):e.multiply?expr(e.multiply[0],parameters)*expr(e.multiply[1],parameters):expr(e.divide[0],parameters)/expr(e.divide[1],parameters));
const fixture=()=>({points:[{id:'a',x:{parameter:'radius'},y:literal(0,'length'),fixed:true},{id:'b',x:{parameter:'radius'},y:{parameter:'radius'},fixed:true},{id:'c',x:literal(0,'length'),y:{parameter:'radius'},fixed:true}],splines:[{id:'curve',points:['a','b','c'],basis:{degree:2,knots:[0,1],multiplicities:[3,3],weights:[literal(1),{parameter:'weight'},literal(1)]}}],constraints:[{distance:{first:'a',second:'c',value:{parameter:'distance'}}}]});
// Independent de Boor evaluation in homogeneous coordinates.
function sample(def,u,parameters){const s=def.splines[0],b=s.basis,p=b.degree,U=b.knots.flatMap((v,i)=>Array(b.multiplicities[i]).fill(v)),k=u===U[s.points.length]?s.points.length-1:U.findLastIndex(v=>v<=u),d=[];
 for(let j=0;j<=p;j++){const i=k-p+j,point=def.points.find(t=>t.id===s.points[i]),w=b.weights?.length?expr(b.weights[i],parameters):1;d.push([expr(point.x,parameters)*w,expr(point.y,parameters)*w,w]);}
 for(let r=1;r<=p;r++)for(let j=p;j>=r;j--){const i=k-p+j,a=(u-U[i])/(U[i+p-r+1]-U[i]);d[j]=d[j].map((v,c)=>(1-a)*d[j-1][c]+a*v);}
 return d[p].slice(0,2).map(v=>v/d[p][2]);}
function sameCurve(before,after){for(const parameters of [{radius:10,weight:Math.SQRT1_2},{radius:15,weight:1.2},{radius:7,weight:.35}])for(let i=0;i<=100;i++){const s=before.splines[0],U=s.basis.knots.flatMap((k,j)=>Array(s.basis.multiplicities[j]).fill(k)),u=U[s.basis.degree]+(U[s.points.length]-U[s.basis.degree])*i/100,a=sample(before,u,parameters),b=sample(after,u,parameters);a.forEach((v,j)=>assert(Math.abs(v-b[j])<1e-10,`${u}: ${a} != ${b}`));}}
test('rational knot insertion and repeated degree elevation preserve curves and parameter expressions',()=>{
 const before=fixture(),snapshot=structuredClone(before);let next=refineSpline(before,'curve',{action:'insert_knot',knot:.5});assert.equal(next.splines[0].points.length,4);sameCurve(before,next);
 next=refineSpline(next,'curve',{action:'insert_knot',knot:.5});assert.deepEqual(next.splines[0].basis.multiplicities,[3,2,3]);sameCurve(before,next);
 next=refineSpline(next,'curve',{action:'elevate_degree',degree:5});assert.equal(next.splines[0].basis.degree,5);assert.deepEqual(next.splines[0].basis.multiplicities,[6,5,6]);sameCurve(before,next);
 assert.deepEqual(next.constraints,before.constraints);assert.deepEqual(next.points.slice(0,3),before.points);assert.equal(next.splines[0].points[0],'a');assert.equal(next.splines[0].points.at(-1),'c');assert.deepEqual(before,snapshot);
});
test('multi-span unit-weight refinement retains parameter domain and collision-free names',()=>{
 const before=fixture();before.splines[0].basis.weights=[];let next=refineSpline(before,'curve',{action:'insert_knot',knot:.2});next=refineSpline(next,'curve',{action:'insert_knot',knot:.7});sameCurve(before,next);const elevated=refineSpline(next,'curve',{action:'elevate_degree',degree:3});sameCurve(before,elevated);assert.deepEqual(elevated.splines[0].basis.weights,[]);assert.equal(new Set(elevated.points.map(p=>p.id)).size,elevated.points.length);
 before.splines[0].basis.knots=[-3,7];const shifted=refineSpline(before,'curve',{action:'insert_knot',knot:2});sameCurve(before,shifted);assert.deepEqual(shifted.splines[0].basis.knots,[-3,2,7]);
});
test('unsupported or invalid refinements leave the definition unchanged',()=>{
 const before=fixture(),snapshot=structuredClone(before);
 for(const knot of [NaN,0,1,2])assert.throws(()=>refineSpline(before,'curve',{action:'insert_knot',knot}),/inside/);
 for(const degree of [2,2.5,26,NaN])assert.throws(()=>refineSpline(before,'curve',{action:'elevate_degree',degree}),/Elevated degree/);
 const inserted=refineSpline(refineSpline(before,'curve',{action:'insert_knot',knot:.5}),'curve',{action:'insert_knot',knot:.5});assert.throws(()=>refineSpline(inserted,'curve',{action:'insert_knot',knot:.5}),/multiplicity/);
 const free=fixture();free.points[1].fixed=false;assert.throws(()=>refineSpline(free,'curve',{action:'insert_knot',knot:.5}),/fixed authored/);
 for(const mode of ['periodic','non_clamped']){const d=fixture();d.splines[0].basis[mode]=true;assert.throws(()=>refineSpline(d,'curve',{action:'insert_knot',knot:.5}),/clamped/);}
 assert.deepEqual(before,snapshot);
});
test('generic degrees and multi-span rational bases retain their exact sampled geometry',()=>{
 for(let degree=1;degree<=5;degree++){
  const count=degree+4,points=Array.from({length:count},(_,i)=>({id:`p${i}`,x:literal(i,'length'),y:literal(Math.sin(i),'length'),fixed:true})),before={points,splines:[{id:'generic',points:points.map(p=>p.id),basis:{degree,knots:[-2,-1,0,1,2],multiplicities:[degree+1,1,1,1,degree+1],weights:points.map((_,i)=>literal(1+i/10))}}]};
  let next=refineSpline(before,'generic',{action:'insert_knot',knot:.4});next=refineSpline(next,'generic',{action:'elevate_degree',degree:degree+2});sameCurve(before,next);
 }
});

test('native non-clamped transformation preserves active domain, rational drivers and endpoint references',()=>{
 const before=fixture();Object.assign(before.splines[0].basis,{non_clamped:true,knots:[0,1,2,3,4,5],multiplicities:[1,1,1,1,1,1]});before.splines[0].endpoints={start:'start',end:'end'};before.points.push({id:'start',x:literal(0,'length'),y:literal(0,'length'),fixed:false},{id:'end',x:literal(0,'length'),y:literal(0,'length'),fixed:false});
 const transformation={degree:2,knots:[0,1,2,2.5,3,4,5],multiplicities:[1,1,1,1,1,1,1],coefficients:[[1,0,0],[.25,.75,0],[0,.75,.25],[0,0,1]]};
 const next=refineSpline(before,'curve',{action:'insert_knot',knot:2.5,transformation});sameCurve(before,next);assert.deepEqual(next.splines[0].endpoints,before.splines[0].endpoints);assert.equal(next.splines[0].basis.non_clamped,true);
 const bad=structuredClone(transformation);bad.coefficients[1][0]=-.25;assert.throws(()=>refineSpline(before,'curve',{action:'insert_knot',knot:2.5,transformation:bad}),/inconsistent/);
});
