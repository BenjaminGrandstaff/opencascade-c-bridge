// Typed sketch relationship authoring at 10,000 authored and generated curves.
// This measures validation/control construction; DOM stubs exclude browser layout.
import assert from 'node:assert/strict';
import {performance} from 'node:perf_hooks';
import {createConstraintControls,makeConstraint} from '../../rust/occt-parametric/src/bin/view/web/constraint-controls.mjs';
import {createPatternControls,makePattern} from '../../rust/occt-parametric/src/bin/view/web/pattern-controls.mjs';
import {createWeightedTermControls,validateWeightedTerms} from '../../rust/occt-parametric/src/bin/view/web/weighted-point-controls.mjs';
import {captureGeneratedPoints,bindGeneratedPoints} from '../../rust/occt-parametric/src/bin/view/web/generated-points.mjs';
class Element {
  constructor(){this.children=[];this.style={};}
  append(...children){for(const child of children)child.parentElement=this;this.children.push(...children);}
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

// Maximum weighted term count shares one source datalist instead of 256 copies.
start=performance.now();
const host=new Element(),terms=Array.from({length:256},(_,i)=>({point:`p${i+1}`,weight:{literal:{value:1,dimension:'scalar',unit:null}}}));
const weighted=createWeightedTermControls({host,definition,message:()=>{}});
weighted.load('p0',terms);
assert.deepEqual(validateWeightedTerms(definition,'p0',weighted.terms),terms);
assert.equal(host.children[0].children[1].children.length,10000);
const weightedTime=performance.now()-start;
console.log(`Weighted links: 256 rows / 10,000 source choices ${weightedTime.toFixed(1)} ms (budget 1,000 ms; DOM stubs)`);
assert(weightedTime<1000,'Weighted source controls exceeded their scale budget.');

start=performance.now();
createPatternControls({host:new Element(),definition,changed:()=>{},message:()=>{}});
const patternMenu=performance.now()-start;
start=performance.now();
for(let i=0;i<25;i++)assert.equal(makePattern(definition,'linear',{id:'new',entities:['line9999',generated[9999]],direction:'line0',count:1000,spacing:{literal:{value:10,dimension:'length',unit:'millimeter'}}}).linear.count,1000);
const patternValidation=performance.now()-start;
console.log(`Sketch patterns: 20,000-curve menu ${patternMenu.toFixed(1)} ms; 25 validations ${patternValidation.toFixed(1)} ms (budget 5,000 ms each)`);
assert(patternMenu<5000&&patternValidation<5000,'Pattern authoring exceeded its scale budget.');
