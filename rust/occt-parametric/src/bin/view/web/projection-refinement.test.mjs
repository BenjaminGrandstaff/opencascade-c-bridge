import {test} from 'node:test';import assert from 'node:assert/strict';
import {projectionRefinement,withProjectionRefinements,createProjectionRefinementControls} from './projection-refinement.mjs';
const source={id:'reference',input:'source',kind:'chain',edge:{all:true},junctions:[{id:'joint',near:{parameter:'near'}}],intersections:[{id:'cross',other:'guide'}]};
test('projected refinement recipes preserve source links and named references while validating targets',()=>{
 const ops=[projectionRefinement('elevate_degree',4),projectionRefinement('insert_knot',.37,2)],next=withProjectionRefinements(source,ops);assert.deepEqual(next.refinements,ops);const restored=withProjectionRefinements(next,[]);assert.deepEqual(restored,source);assert.equal(source.refinements,undefined);assert.deepEqual(next.junctions,source.junctions);assert.deepEqual(next.edge,source.edge);
 for(const value of [0,1,-.1,NaN])assert.throws(()=>projectionRefinement('insert_knot',value),/fraction/);
 for(const value of [0,26,2.5])assert.throws(()=>projectionRefinement('elevate_degree',value),/degree/);
 assert.throws(()=>projectionRefinement('insert_knot',.5,0),/multiplicity/);assert.throws(()=>withProjectionRefinements({...source,kind:'circle'},ops),/exact/);assert.throws(()=>withProjectionRefinements(source,Array(33).fill(ops[0])),/32/);
});
class Element{constructor(tag){this.tagName=tag;this.children=[];this.value='';}append(...e){this.children.push(...e);}replaceChildren(...e){this.children=e;}setAttribute(k,v){this[k]=v;}}
const all=e=>[e,...e.children.flatMap(all)],field=(h,t)=>all(h).find(e=>e['aria-label']===t),click=(h,t)=>all(h).find(e=>e.tagName==='button'&&e.textContent===t).onclick();
test('projection controls add, remove and clear operations atomically with source highlight',()=>{
 globalThis.document={createElement:tag=>new Element(tag)};let p=structuredClone(source),changes=0;const host=new Element('div'),messages=[],highlights=[];
 const render=()=>{host.replaceChildren();createProjectionRefinementControls({host,projection:p,changed:next=>{p=next;changes++;render();},message:m=>messages.push(m),highlight:ids=>highlights.push(ids)});};render();
 field(host,'Projected knot fraction · reference').value='0';click(host,'Insert projected knot · reference');assert.equal(changes,0);
 field(host,'Projected minimum degree · reference').value='4';click(host,'Elevate projected degree · reference');assert.equal(p.refinements[0].elevate_degree.degree,4);
 field(host,'Projected knot fraction · reference').value='.37';field(host,'Projected knot multiplicity · reference').value='2';click(host,'Insert projected knot · reference');assert.equal(p.refinements.length,2);assert.equal(p.refinements[1].insert_knot.multiplicity,2);
 click(host,'Remove projection refinement reference 1');assert.equal(p.refinements.length,1);click(host,'Highlight projected curve · reference');assert.deepEqual(highlights,[['reference']]);click(host,'Clear projected refinement · reference');assert.deepEqual(p,source);assert.equal(changes,4);
 delete globalThis.document;
});
