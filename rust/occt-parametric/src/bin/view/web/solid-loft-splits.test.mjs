import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createLoftSplitControls,validateSplitPoints} from './solid-loft-splits.mjs';
import {loftDefinition} from './solid-loft.mjs';
const q=n=>({literal:{value:n,dimension:'length',unit:'millimeter'}}),point={components:{x:q(0),y:q(4),z:q(20)}};
test('split point validation preserves formulas and rejects nonfinite, scalar, malformed and over-budget points',()=>{
 const saved=[point,{parameter:'cut'}];assert.deepEqual(validateSplitPoints(saved),saved);const cloned=validateSplitPoints(saved);cloned[0].components.y.literal.value=9;assert.equal(saved[0].components.y.literal.value,4);
 for(const bad of [null,{},[null],[[0,0,0]],Array(129).fill(point),[{components:{x:q(NaN),y:q(0),z:q(0)}}],[{components:{x:{literal:{value:0,dimension:'scalar',unit:null}},y:q(0),z:q(0)}}]])assert.throws(()=>validateSplitPoints(bad));
 const d={profiles:['a','b'],holes:[],ruled:false,start_constraint:{normal_to_profile:true,magnitude:q(20)},correspondence:{a:{split_points:saved}}};assert.deepEqual(loftDefinition([{id:'a'},{id:'b'}],d),d);assert.throws(()=>loftDefinition([{id:'a'},{id:'b'}],{...d,start_constraint:undefined}));
});

test('split controls disable both add paths at the saved point budget',()=>{
 const buttons=[],controls=createLoftSplitControls(Array(128).fill(point));
 controls.render('Outer station 1',{select(){},number(){},render(){},button:(name,onclick)=>{const b={name,onclick};buttons.push(b);return b;},content:{append(){}},invalidate(){},checkpoint(){},message(){},profile:'a',pickSplit(){}},()=>{});
 for(const name of ['Pick outer station 1 split in canvas','Add outer station 1 split point'])assert.equal(buttons.find(b=>b.name===name).disabled,true);
 assert.equal(controls.definition().length,128);
});
