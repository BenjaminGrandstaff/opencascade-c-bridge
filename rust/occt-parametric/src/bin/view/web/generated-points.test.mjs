import {pickedCrossing} from './crossing-pick.mjs';
import {makeCurveOperation,profileIds} from './profile-controls.mjs';
import {test} from 'node:test';
import assert from 'node:assert/strict';
import {captureGeneratedPoints,bindGeneratedPoints,generatedPointIds} from './generated-points.mjs';
import {geometryIds,makeConstraint} from './constraint-controls.mjs';
import {snapPosition,drawPrimitive} from './canvas-tools.mjs';
const source=()=>({points:[{id:'a'},{id:'c'}],lines:[{id:'axis',start:'a',end:'c'}],shapes:[{rectangle:{id:'rect',first:'a',opposite:'c'}}],patterns:[{linear:{id:'row',entities:['rect:side0'],direction:'axis',count:3}}]});
const ids=['rect:corner1','rect:corner3','row:1:rect:corner1'];
test('only core-declared generated names snap and drawing saves references without metadata',()=>{
 const d=source(),solved={'rect:corner1':[10,0,0],'unknown:pole:1':[20,0,0]};
 assert.equal(snapPosition([10,0],d,solved,0,.1).point,null);
 const snapshot=captureGeneratedPoints(d,ids);assert(bindGeneratedPoints(d,snapshot));
 assert.deepEqual(geometryIds(d,'points'),['a','c',...ids]);
 const snap=snapPosition([10.01,.01],d,solved,0,.1);assert.equal(snap.point,'rect:corner1');
 const drawn=drawPrimitive(d,'line',[snap,{xy:[12,3],point:null}],{fixed:true}).definition;
 assert.equal(drawn.lines.at(-1).start,'rect:corner1');assert.equal(drawn.points.length,3);
 assert(!JSON.stringify(drawn).includes('generated_points'));
 assert(bindGeneratedPoints(drawn,snapshot));assert.deepEqual(generatedPointIds(drawn),ids);
 assert.equal(snapPosition([20,0],d,solved,0,.1).point,null);
 assert.equal(snapPosition([10,0],d,{'rect:corner1':[NaN,0]},0,.1).point,null);
});
test('generator and source role edits invalidate catalogues until native refresh',()=>{
 for(const edit of [d=>d.patterns[0].linear.count--,d=>d.shapes[0].rectangle.opposite='a',d=>d.lines[0].end='other',d=>d.lines=[]]){
  const d=source(),snapshot=captureGeneratedPoints(d,ids);bindGeneratedPoints(d,snapshot);edit(d);
  assert.deepEqual(generatedPointIds(d),[]);assert.equal(bindGeneratedPoints(d,snapshot),false);
  const refreshed=captureGeneratedPoints(d,['new:point']);assert(bindGeneratedPoints(d,refreshed));assert.deepEqual(generatedPointIds(d),['new:point']);
 }
 const d=source(),snapshot=captureGeneratedPoints(d,ids),copy=structuredClone(d);
 copy.points[0].x={parameter:'new_driver'};assert(bindGeneratedPoints(copy,snapshot));
 assert.deepEqual(generatedPointIds(d),[],'capture alone does not bind');
});
test('catalogues reject malformed or oversized metadata and deduplicate references',()=>{
 for(const value of [null,{},[''],[4],Array(40001).fill('x')])assert.equal(captureGeneratedPoints(source(),value),null);
 const d=source();assert(bindGeneratedPoints(d,captureGeneratedPoints(d,[...ids,...ids])));assert.deepEqual(generatedPointIds(d),ids);
});

test('generated curves retain native kinds in typed dimensions and reject stale or mismatched choices',()=>{
 const d=source(),curves={lines:['rect:side0','row:1:rect:side0'],arcs:['slot:end_cap'],circles:['poly:circle'],ellipses:['row:1:ellipse'],splines:['conic']};
 const snapshot=captureGeneratedPoints(d,ids,curves);assert(bindGeneratedPoints(d,snapshot));
 assert.deepEqual(geometryIds(d,'lines'),['axis',...curves.lines]);
 assert.deepEqual(geometryIds(d,'round'),['poly:circle','slot:end_cap']);
 assert.deepEqual(geometryIds(d,'splines'),['conic']);
 const mm=value=>({literal:{value,dimension:'length',unit:'millimeter'}});
 assert.deepEqual(makeConstraint(d,'curve_length',{curve:'row:1:rect:side0'},mm(10)),{curve_length:{curve:'row:1:rect:side0',value:mm(10)}});
 assert.doesNotThrow(()=>makeConstraint(d,'radius',{curve:'slot:end_cap'},mm(2)));
 assert.throws(()=>makeConstraint(d,'radius',{curve:'rect:side0'},mm(2)),/known/);
 assert.throws(()=>makeConstraint(d,'horizontal',{line:'slot:end_cap'}),/known/);
 d.patterns[0].linear.count=2;
 assert(!geometryIds(d,'curves').includes('row:1:rect:side0'));
 assert.throws(()=>makeConstraint(d,'curve_length',{curve:'row:1:rect:side0'},mm(10)),/known/);
});
test('curve metadata validates kind arrays and stays outside cloned definitions',()=>{
 for(const curves of [null,[],{lines:['']},{arcs:'bad'},{splines:[7]},{lines:Array(40001).fill('x')}])assert.equal(captureGeneratedPoints(source(),ids,curves),null);
 const d=source();bindGeneratedPoints(d,captureGeneratedPoints(d,ids,{lines:['rect:side0']}));
 assert(!geometryIds(structuredClone(d),'curves').includes('rect:side0'));
});

test('crossing clones retain verified generated sources and split names cannot shadow them',()=>{
 const d=source();bindGeneratedPoints(d,captureGeneratedPoints(d,ids,{lines:['rect:side0','row:1:rect:side0']}));
 const result=pickedCrossing(d,{name:'joint',first:'rect:side0',second:'row:1:rect:side0',position:[1,2],radius:1});
 assert.equal(result.definition.constraints[0].crossing_point.first,'rect:side0');
 assert.equal(d.points.length,2);
 assert.throws(()=>pickedCrossing(d,{name:'rect:side0',first:'rect:side0',second:'row:1:rect:side0',position:[1,2],radius:1}),/already exists/);
 assert.throws(()=>makeCurveOperation('split',{entity:'axis',at:{point:{point:'a'}},before:'rect:side0',after:'piece'},d),/unused/);
 assert(profileIds(d,'curves').includes('row:1:rect:side0'));
 d.patterns[0].linear.count=2;assert(!profileIds(d,'curves').includes('row:1:rect:side0'));
});
