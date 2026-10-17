import test from 'node:test';
import assert from 'node:assert/strict';
import {pickedWeightedPoint, weightedPosition} from './sketch-studio-weighted.mjs';
import {captureGeneratedPoints, bindGeneratedPoints} from './generated-points.mjs';
import {geometryIds} from './constraint-controls.mjs';
const mm=value=>({literal:{value,dimension:'length',unit:'millimeter'}});
const scalar=value=>({literal:{value,dimension:'scalar',unit:null}});
const definition=()=>({points:[{id:'A',x:mm(0),y:mm(0),fixed:true},{id:'B',x:mm(10),y:mm(6),fixed:true}],constraints:[],lines:[]});
const terms=[{point:'A',weight:scalar(1)},{point:'B',weight:scalar(3)}],positions={A:[0,0,0],B:[10,6,0]};

test('weighted positions normalize large positive weights without overflow',()=>{
 assert.deepEqual(weightedPosition(terms,positions),[7.5,4.5]);
 assert.deepEqual(weightedPosition(terms.map(t=>({...t,weight:scalar(1e308)})),{A:[-1e308,0],B:[1e308,2]}),[0,1]);
 assert.equal(weightedPosition([{point:'A',weight:{parameter:'blend'}}],positions),null);
 assert.throws(()=>weightedPosition(terms,{A:[0,0],B:[NaN,0]}),/finite solved position/);
});

test('a new weighted point records expressions and leaves the input untouched',()=>{
 const d=definition(),original=structuredClone(d),result=pickedWeightedPoint(d,{name:'average',terms,positions});
 assert.deepEqual(d,original);assert.equal(result.point,'average');assert.equal(result.definition.points.at(-1).x.literal.value,7.5);
 assert.deepEqual(result.definition.constraints[0].weighted_point,{point:'average',terms});
 assert.equal(result.definition.points.at(-1).fixed,false);
});

test('expression weights retain their expression and use finite source coordinates only as solver seeds',()=>{
 const d=definition(),drivers=[{point:'A',weight:{parameter:'blend'}},{point:'B',weight:{add:[scalar(1),scalar(2)]}}];
 const result=pickedWeightedPoint(d,{name:'average',terms:drivers,positions});
 assert.deepEqual(result.definition.constraints[0].weighted_point.terms,drivers);
 assert.equal(result.definition.points.at(-1).x.literal.value,5);
});

test('invalid sources, names, targets, weights and oversized lists reject atomically',()=>{
 const d=definition(),base={name:'average',terms,positions},original=structuredClone(d);
 for(const patch of [{name:'A'},{name:' average '},{terms:[]},{terms:[{point:'unknown',weight:scalar(1)}]},{terms:[{point:'A',weight:scalar(0)}]},
  {terms:[{point:'A',weight:scalar(-1)}]},{terms:Array(257).fill(terms[0])},{point:'A'},{point:'missing'}]) assert.throws(()=>pickedWeightedPoint(d,{...base,...patch}));
 assert.deepEqual(d,original);
});

test('verified generated points can be sources and the catalogue survives the link clone',()=>{
 const d=definition();d.shapes=[{rectangle:{id:'box',first:'A',opposite:'B'}}];
 bindGeneratedPoints(d,captureGeneratedPoints(d,['box:corner1'],{lines:['box:side0']}));
 const result=pickedWeightedPoint(d,{name:'average',terms:[{point:'box:corner1',weight:scalar(1)},terms[0]],positions:{...positions,'box:corner1':[10,0,0]}});
 assert.equal(result.definition.points.at(-1).x.literal.value,5);
 assert(geometryIds(result.definition,'lines').includes('box:side0'));
 assert(!JSON.stringify(result.definition).includes('generated_points'));
});
