import {test} from 'node:test';import assert from 'node:assert/strict';
import {pickedSplineSample,createSplineSamplePicker} from './spline-sample-pick.mjs';
import {captureGeneratedPoints,bindGeneratedPoints} from './generated-points.mjs';
const definition=()=>({points:[{id:'a',fixed:true},{id:'b',fixed:true},{id:'p',fixed:true}],splines:[{id:'curve',points:['a','b']}],patterns:[{linear:{id:'row',count:2}}],constraints:[]});
const options={name:'Sample1',spline:'row:1:curve',fraction:.3,position:[3,4]};
const bound=()=>{const d=definition();bindGeneratedPoints(d,captureGeneratedPoints(d,[],{splines:['row:1:curve']}));return d;};
test('native picked samples save a literal fraction and free named target without freezing geometry',()=>{
 const d=bound(),before=structuredClone(d),result=pickedSplineSample(d,options),p=result.definition.points.at(-1);
 assert.deepEqual(d,before);assert.equal(p.id,'Sample1');assert.equal(p.fixed,false);assert.equal(p.x.literal.value,3);
 assert.deepEqual(result.definition.constraints,[{spline_point:{point:'Sample1',spline:'row:1:curve',fraction:{literal:{value:.3,dimension:'scalar',unit:null}}}}]);
 const relink=pickedSplineSample(d,{...options,point:'p'});assert.equal(relink.definition.points.length,d.points.length);assert.equal(relink.definition.points[2].fixed,false);
 assert(!('generated_curves' in result.definition));
});
test('sample staging rejects invalid native results, collisions, unknown/stale sources and defining targets',()=>{
 const d=bound();for(const changes of [{fraction:NaN},{fraction:1.1},{position:[Infinity,0]},{position:[0]},{name:'a'},{name:'row:1:curve'},{spline:'curve:pole:1'},{point:'a',spline:'curve'}])assert.throws(()=>pickedSplineSample(d,{...options,...changes}));
 d.patterns[0].linear.count=3;assert.throws(()=>pickedSplineSample(d,options),/known/);
});
class Element{constructor(tag){this.tagName=tag;this.children=[];this.value='';}append(...items){for(const item of items)item.parentElement=this;this.children.push(...items);}setAttribute(k,v){this[k]=v;}}
const all=e=>[e,...e.children.flatMap(all)];
function setup(config={}){
 globalThis.document={createElement:tag=>new Element(tag)};globalThis.Option=class extends Element{constructor(text,value){super('option');this.textContent=text;this.value=value;}};
 const host=new Element('div'),changes=[],messages=[],queries=[],modes=[];
 const picker=createSplineSamplePicker({host,definition:bound(),changed:(...v)=>changes.push(v),message:v=>messages.push(v),modeChanged:v=>modes.push(v),query:async q=>{queries.push(q);return {fraction:.3,point:[3,4]};},...config});
 const button=text=>all(host).find(e=>e.textContent===text),event=(entity='row:1:curve',x=3,y=4)=>({entity,position:[x,y],tolerance:1,scene:{}});
 return {picker,host,button,event,changes,messages,queries,modes};
}
const cleanup=()=>{delete globalThis.document;delete globalThis.Option;};
test('one native query per sample click stages a linked point and returns to selection',async()=>{try{
 const s=setup();s.button('Start spline sample pick').onclick();assert(s.picker.active);
 s.picker.down(s.event());s.picker.move(s.event());assert.equal(s.queries.length,0);await s.picker.up(s.event());
 assert.equal(s.queries.length,1);assert.deepEqual(s.queries[0],{spline:'row:1:curve',position:[3,4],maximum_distance:1});assert.equal(s.changes.length,1);assert.equal(s.changes[0][1],'Sample1');assert(!s.picker.active);assert.deepEqual(s.modes,['sample','select']);
 }finally{cleanup();}});
test('drags, wrong curve kinds and stale native views cannot query or stage a point',async()=>{try{
 let valid=true;const s=setup({current:()=>valid});s.button('Start spline sample pick').onclick();
 s.picker.down(s.event());s.picker.move(s.event('row:1:curve',8,4));await s.picker.up(s.event('row:1:curve',8,4));
 s.picker.down(s.event('line'));await s.picker.up(s.event('line'));assert.equal(s.queries.length,0);
 valid=false;s.picker.down(s.event());assert(!s.picker.active);assert.equal(s.changes.length,0);assert(s.messages.at(-1).includes('Preview'));
 }finally{cleanup();}});
test('canceling or changing native scenes discards a pending sample response',async()=>{try{
 for(const cancel of [true,false]){
  let release,valid=true;const s=setup({current:()=>valid,query:()=>new Promise(r=>release=r)});s.button('Start spline sample pick').onclick();s.picker.down(s.event());const pending=s.picker.up(s.event());
  if(cancel)s.picker.cancel();else valid=false;release({fraction:.3,point:[3,4]});await pending;assert.equal(s.changes.length,0);assert(!s.picker.active);
 }
 }finally{cleanup();}});
test('failed or malformed native samples leave the draft unchanged and permit another click',async()=>{try{
 let attempts=0;const s=setup({query:async()=>{attempts++;if(attempts===1)throw new Error('ambiguous parameter');if(attempts===2)return {fraction:2,point:[3,4]};return {fraction:.3,point:[3,4]};}});s.button('Start spline sample pick').onclick();
 for(let i=0;i<3;i++){s.picker.down(s.event());await s.picker.up(s.event());if(i<2){assert.equal(s.changes.length,0);assert(s.picker.active);}}
 assert.equal(s.changes.length,1);assert.equal(attempts,3);
 }finally{cleanup();}});
test('reopening the picker after a sample was added suggests the next available name',()=>{try{
 const d=bound();d.points.push({id:'Sample1'});const s=setup({definition:d,initialOptions:{name:'Sample1'}});
 assert.equal(all(s.host).find(e=>e['aria-label']==='New spline sample name').value,'Sample2');
 s.button('Start spline sample pick').onclick();assert(s.picker.active);
 }finally{cleanup();}});
