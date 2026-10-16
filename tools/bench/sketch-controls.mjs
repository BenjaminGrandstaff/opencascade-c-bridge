// Typed sketch relationship authoring at 10,000 authored and generated curves.
// This measures validation/control construction; DOM stubs exclude browser layout.
import assert from 'node:assert/strict';
import {performance} from 'node:perf_hooks';
import {createConstraintControls,makeConstraint} from '../../rust/occt-parametric/src/bin/view/web/constraint-controls.mjs';
import {captureGeneratedPoints,bindGeneratedPoints} from '../../rust/occt-parametric/src/bin/view/web/generated-points.mjs';
class Element {
  constructor(){this.children=[];this.style={};}
  append(...children){this.children.push(...children);}
  replaceChildren(...children){this.children=children;}
  setAttribute(key,value){this[key]=value;}
}
globalThis.document={createElement:()=>new Element()};
globalThis.Option=class extends Element{constructor(text,value){super();this.textContent=text;this.value=value;}};
const definition={
  points:Array.from({length:10001},(_,i)=>({id:`p${i}`})),
  lines:Array.from({length:10000},(_,i)=>({id:`line${i}`,start:`p${i}`,end:`p${i+1}`})),
  splines:[{id:'blend',points:['p0','p1','p2','p3']}],
  constraints:Array.from({length:1000},()=>({equal_curve_length:{first:'line0',second:'line9999'}})),
};
const generated=Array.from({length:10000},(_,i)=>`row:${i}:side0`);
assert(bindGeneratedPoints(definition,captureGeneratedPoints(definition,[],{lines:generated})));
let start=performance.now();
for(let i=0;i<25;i++){
  const constraint=makeConstraint(definition,'equal_curve_length',{first:'line9999',second:generated[i]});
  assert.equal(constraint.equal_curve_length.second,generated[i]);
  assert.deepEqual(makeConstraint(definition,'curvature_continuous',{first:generated[i],second:'blend',point:'p0'}),{curvature_continuous:{first:generated[i],second:'blend',point:'p0'}});
}
const validation=performance.now()-start;
start=performance.now();
createConstraintControls({host:new Element(),definition,initialType:'curvature_continuous',changed:()=>{},message:()=>{}});
const controls=performance.now()-start;
console.log(`Sketch controls: 50 relationship validations ${validation.toFixed(1)} ms; 1,000-row editor ${controls.toFixed(1)} ms (10,000 authored + 10,000 generated curves; budget 5,000 ms each)`);
assert(validation<5000&&controls<5000,'Sketch relationship controls exceeded their scale budget.');
