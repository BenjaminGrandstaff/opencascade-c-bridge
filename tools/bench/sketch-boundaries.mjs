// Material authoring at 10,000 source curves. DOM layout and native solving
// are excluded; repeated picks must not rebuild the form or mutate the draft.
import assert from 'node:assert/strict';
import {performance} from 'node:perf_hooks';
import {createBoundaryControls} from '../../rust/occt-parametric/src/bin/view/web/sketch-boundaries.mjs';
class Element {
 constructor(){this.children=[];this.style={};}
 append(...children){this.children.push(...children);}
 replaceChildren(...children){this.children=children;}
 setAttribute(key,value){this[key]=value;}
}
globalThis.document={createElement:()=>new Element()};
globalThis.Option=class extends Element{constructor(text,value){super();this.textContent=text;this.value=value;}};
const definition={points:[],lines:Array.from({length:10000},(_,i)=>({id:`line${i}`}))},host=new Element();let changes=0;
const walk=e=>[e,...e.children.flatMap(walk)];
let start=performance.now();const controls=createBoundaryControls({host,definition,changed:()=>changes++,message:()=>{}});
const mode=walk(host).find(e=>e['aria-label']==='Material selection');mode.value='explicit';mode.onchange();const build=performance.now()-start;
start=performance.now();walk(host).find(e=>e.textContent==='Pick Outer boundary on canvas').onclick();for(let i=0;i<1000;i++)assert(controls.pick(`line${i}`));const picks=performance.now()-start;
assert.equal(changes,0);assert.equal(definition.profile,undefined);assert.equal(controls.pick('line1000'),false,'a loop cannot exceed 1000 entries');
start=performance.now();walk(host).find(e=>e.textContent==='Stage material boundaries').onclick();const stage=performance.now()-start;
assert.equal(changes,1);assert.equal(definition.profile.length,1000);
console.log(`Studio boundaries: 10,000-curve form ${build.toFixed(1)} ms, 1,000 picks ${picks.toFixed(1)} ms, stage ${stage.toFixed(1)} ms (1,000 ms budget each; DOM stubs)`);
assert(build<1000&&picks<1000&&stage<1000,'Boundary authoring exceeded its scale budget.');
