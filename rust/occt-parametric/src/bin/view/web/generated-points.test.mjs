import {test} from 'node:test';
import assert from 'node:assert/strict';
import {captureGeneratedPoints,bindGeneratedPoints,generatedPointIds} from './generated-points.mjs';
import {geometryIds} from './constraint-controls.mjs';
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
