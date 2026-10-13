import {test} from 'node:test';
import assert from 'node:assert/strict';
import {editSpline,createSplineControls} from './spline-controls.mjs';
const scalar=value=>({literal:{value,dimension:'scalar',unit:null}}),basis={degree:2,knots:[0,1],multiplicities:[3,3]},explicit={id:'curve',points:['a','b','c'],basis:{...basis,weights:[scalar(1),{parameter:'weight'},scalar(1)]}},known=['a','b','c','d','start','end'];
test('spline edits preserve basis, identity and endpoints while validating weights and closure',()=>{
 const weights=[scalar(1),{multiply:[{parameter:'weight'},scalar(2)]},scalar(1)],next=editSpline(explicit,{points:['a','d','c'],weights},known);
 assert.equal(next.id,'curve');assert.deepEqual(next.basis.knots,[0,1]);assert.deepEqual(next.basis.weights,weights);assert.equal(explicit.points[1],'b');
 assert.throws(()=>editSpline(explicit,{points:['a','b'],weights},known),/pole count/);
 assert.throws(()=>editSpline(explicit,{points:['a','b','c'],weights:[scalar(1),scalar(0),scalar(1)]},known),/positive/);
 assert.throws(()=>editSpline({...explicit,endpoints:{start:'start',end:'end'}},{points:['a','start','c']},known),/separate/);
 assert.deepEqual(editSpline({id:'s',points:['a','b','c','a']},{points:['a','b','c'],closed:false},known).points,['a','b','c']);
 assert.deepEqual(editSpline({id:'s',points:['a','b','c']},{points:['a','d','c'],closed:true},known).points,['a','d','c','a']);
 assert.throws(()=>editSpline({id:'s',points:['a','b']},{points:['a','a']},known),/distinct/);
});
class Element{constructor(tag){this.tagName=tag;this.children=[];this.value='';}append(...children){this.children.push(...children);}replaceChildren(...children){this.children=children;}setAttribute(k,v){this[k]=v;}}
const all=e=>[e,...e.children.flatMap(all)],field=(h,name)=>all(h).find(e=>e['aria-label']===name),click=(h,text)=>all(h).find(e=>e.tagName==='button'&&e.textContent===text).onclick();
test('weight forms preserve formulas and implicit unit weights; canvas picks and point focus remain explicit',()=>{
 globalThis.document={createElement:tag=>new Element(tag)};globalThis.Option=class extends Element{constructor(text,value){super('option');this.textContent=text;this.value=value;}};
 const formula={multiply:[{parameter:'weight'},scalar(2)]},d={points:known.map(id=>({id})),splines:[structuredClone(explicit)]};d.splines[0].basis.weights[1]=formula;
 const host=new Element('div'),focused=[];let controls,changes=0;const render=()=>{const selected=controls?.selected;host.replaceChildren();controls=createSplineControls({host,definition:d,parameters:[{id:'weight',parameter_type:{scalar:'scalar'}}],initialSelection:selected,changed:()=>{changes++;render();},message(){},focusPoint:id=>focused.push(id)});};render();
 click(host,'Update spline definition');assert.deepEqual(d.splines[0].basis.weights[1],formula);
 const driver=field(host,'Weight 2 source');driver.value='literal';driver.onchange();const value=field(host,'Weight 2');value.value='2';value.oninput();click(host,'Update spline definition');assert.equal(d.splines[0].basis.weights[1].literal.value,2);
 click(host,'Edit point 2: b');assert.deepEqual(focused,['b']);assert.equal(controls.pick('curve'),false);click(host,'Select spline from canvas');controls.pick('curve');assert.equal(controls.selected,'curve');
 click(host,'Use unit weights');assert.deepEqual(d.splines[0].basis.weights,[]);click(host,'Update spline definition');assert.deepEqual(d.splines[0].basis.weights,[]);assert.equal(changes,4);
 delete globalThis.document;delete globalThis.Option;
});
