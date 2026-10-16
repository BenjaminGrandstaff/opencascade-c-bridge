import test from 'node:test';
import assert from 'node:assert/strict';
import {commitShape,studioGhost,typedClick} from './sketch-studio-tools.mjs';
import {drawAdditional,splineRecipe,threePointCircle} from './sketch-studio-primitives.mjs';
const empty=()=>({id:'profile',points:[],lines:[],constraints:[],profile:[]});
const c=(x,y,extra={})=>({xy:[x,y],point:null,rules:[],...extra});
const at=(d,id)=>{const p=d.points.find(p=>p.id===id);return [p.x.literal.value,p.y.literal.value];};
const close=(a,b)=>a.every((x,i)=>Math.abs(x-b[i])<1e-7);
test('center rectangles retain a real center relationship and unique names',()=>{
 const original=empty(),r=commitShape(original,'center_rectangle',[c(2,3),c(6,5)],{profile:true});
 assert.deepEqual(original,empty());assert.equal(r.definition.profile.length,4);assert.equal(r.definition.lines.length,5);
 assert.equal(new Set(r.definition.points.map(p=>p.id)).size,5);
 assert(close(at(r.definition,r.definition.constraints.at(-1).midpoint.point),[2,3]));
 const xs=r.definition.points.map(p=>p.x.literal.value);assert(xs.includes(-2)&&xs.includes(6));
 assert.deepEqual(typedClick('center_rectangle',c(2,3),[6,5],'8, 4').xy,[6,5]);
});
test('three-point arcs preserve through-point membership and choose the span through the middle click',()=>{
 for(const clicks of [[c(1,0),c(0,1),c(-1,0)],[c(1,0),c(-1,0),c(0,1)]]){
  const d=commitShape(empty(),'arc_three',clicks).definition,a=d.arcs[0];assert(close(at(d,a.center),[0,0]));
  assert.equal(a.clockwise,clicks[1].xy[0]===-1);assert.equal(d.constraints[0].point_on_curve.curve,a.id);
  const ghost=studioGhost('arc_three',clicks.slice(0,2),clicks[2]);assert.equal(ghost.length,65);assert(close(ghost[0].slice(0,2),clicks[0].xy));assert(close(ghost.at(-1).slice(0,2),clicks[2].xy));
 }
 assert.throws(()=>threePointCircle([c(0,0),c(1,0),c(2,0)]),/non-collinear/);
 assert(close(threePointCircle([c(1e9+1,1e9),c(1e9,1e9+1),c(1e9-1,1e9)]).center,[1e9,1e9]));
});
test('elliptical arcs project endpoint directions and preserve valid snapped endpoint names',()=>{
 const clicks=[c(0,0),c(5,0),c(0,3),c(9,0),c(0,8)],d=commitShape(empty(),'elliptical_arc',clicks).definition,e=d.ellipses[0];
 assert(close(at(d,e.start),[5,0]));assert(close(at(d,e.end),[0,3]));assert.equal(e.clockwise,false);assert.equal(d.profile.length,0);
 assert.equal(commitShape(empty(),'elliptical_arc',clicks,{flip:true}).definition.ellipses[0].clockwise,true);
 const base=commitShape(empty(),'point',[c(5,0)]).definition;const snap=commitShape(base,'elliptical_arc',[...clicks.slice(0,3),c(5,0,{point:base.points[0].id}),clicks[4]]).definition;
 assert.equal(snap.ellipses[0].start,base.points[0].id);
 assert.throws(()=>commitShape(base,'elliptical_arc',[...clicks.slice(0,3),c(8,0,{point:base.points[0].id}),clicks[4]]),/snapped/);
 assert.throws(()=>commitShape(empty(),'elliptical_arc',[...clicks.slice(0,4),c(10,0)]),/endpoints/);
});
test('conic rho selects existing rational quadratic recipes with bounded ghosts',()=>{
 for(const rho of [.25,.5,.75]){
  const clicks=[c(0,0),c(5,8),c(10,0)],d=commitShape(empty(),'conic',clicks,{rho}).definition;assert.equal(d.shapes[0].conic.rho.literal.value,rho);
  const ghost=studioGhost('conic',clicks.slice(0,2),clicks[2],{rho});assert.equal(ghost.length,65);assert(ghost.flat().every(Number.isFinite));assert.equal(ghost[32][1],8*rho);
 }
 for(const rho of [0,1,NaN])assert.throws(()=>commitShape(empty(),'conic',[c(0,0),c(1,2),c(2,0)],{rho}),/rho/);
 assert.throws(()=>commitShape(empty(),'conic',[c(0,0),c(1,0),c(2,0)]),/non-collinear/);
});
test('control splines author clamped rational bases and evaluate their preview rather than the pole polygon',()=>{
 const clicks=[c(0,0),c(0,10),c(10,10),c(10,0)],d=commitShape(empty(),'control_spline',clicks,{degree:3,weights:'1,2,2,1'}).definition,b=d.splines[0].basis;
 assert.deepEqual(b,{degree:3,knots:[0,1],multiplicities:[4,4],weights:[1,2,2,1].map(value=>({literal:{value,dimension:'scalar',unit:null}}))});
 const ghost=studioGhost('control_spline',clicks.slice(0,3),clicks[3],{degree:3,weights:'1,2,2,1'});assert.equal(ghost.length,65);assert(close(ghost[32],[5,60/7,0]));
 const closed=commitShape(empty(),'control_spline',clicks,{degree:3,closed:true,profile:true}).definition;assert.equal(closed.splines[0].points[0],closed.splines[0].points.at(-1));assert.deepEqual(closed.profile,[closed.splines[0].id]);
 for(const opts of [{degree:6},{degree:3,weights:'1,-1,1,1'},{degree:3,weights:'1,1'}])assert.throws(()=>splineRecipe(clicks.map(c=>c.xy),opts));
 assert.throws(()=>splineRecipe([[0,0],[1,1]],{degree:3}),/at least/);
});
test('text authors exact font, style, alignment, angle and anchor without replacing a profile',()=>{
 const base=empty();base.profile=['existing'];const d=commitShape(base,'text',[c(10,20)],{text:'CAD Ω',font:'Liberation Sans',style:'bold',height:6,angle:90,align:'center',profile:true}).definition,t=d.texts[0];
 assert.equal(t.text,'CAD Ω');assert.equal(t.font,'Liberation Sans');assert.equal(t.style,'bold');assert.equal(t.angle.literal.value,Math.PI/2);assert.equal(t.align,'center');assert(close(at(d,t.anchor),[10,20]));assert.deepEqual(d.profile,['existing']);
 for(const options of [{text:''},{text:'a\nb'},{height:0},{font:''},{style:'fake'},{angle:Infinity}])assert.throws(()=>drawAdditional(base,'text',[c(0,0)],options));
});
