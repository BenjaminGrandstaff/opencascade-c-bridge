import {test} from 'node:test';import assert from 'node:assert/strict';
import {pickedCrossing,createCrossingPicker,crossingWindowFeedback} from './crossing-pick.mjs';
const length=value=>({literal:{value,dimension:'length',unit:'millimeter'}});
function definition(){return {points:[['a',-10,0],['b',10,0],['c',0,-10],['d',0,10],['existing',4,4]].map(([id,x,y])=>({id,x:length(x),y:length(y),fixed:true})),lines:[{id:'horizontal',start:'a',end:'b'},{id:'vertical',start:'c',end:'d'}],constraints:[{distance:{first:'a',second:'b',value:length(20)}}],profile:[]};}
class Element{constructor(tag){this.tagName=tag;this.children=[];this.value='';}append(...nodes){for(const node of nodes)node.parentElement=this;this.children.push(...nodes);}setAttribute(k,v){this[k]=v;}}
const all=e=>[e,...e.children.flatMap(all)],find=(e,t)=>all(e).find(x=>x.textContent===t),input=(e,label)=>all(e).find(x=>x['aria-label']===label);
function mock(action){globalThis.document={createElement:tag=>new Element(tag)};globalThis.Option=class extends Element{constructor(text,value){super('option');this.textContent=text;this.value=value;}};try{action();}finally{delete globalThis.document;delete globalThis.Option;}}
test('picked crossings create a non-fixed named point and literal native window without mutation',()=>{
 const d=definition(),before=structuredClone(d),result=pickedCrossing(d,{name:'hit',first:'horizontal',second:'vertical',position:[0.1,-0.2],radius:2});assert.deepEqual(d,before);assert.equal(result.point,'hit');assert.equal(result.definition.points.at(-1).fixed,false);assert.deepEqual(result.definition.constraints[0],d.constraints[0]);const c=result.definition.constraints.at(-1).crossing_point;assert.deepEqual(c.near.literal,{x:length(.1).literal,y:length(-.2).literal,z:length(0).literal});assert.deepEqual(c.maximum_distance,length(2));
 const old=structuredClone(d);old.constraints.push({spline_point:{point:'existing',spline:'old',fraction:{parameter:'location'}}});const relink=pickedCrossing(old,{point:'existing',first:'horizontal',second:'vertical',position:[0,0],radius:1});assert.equal(relink.definition.points.length,d.points.length);assert.equal(relink.definition.points.at(-1).fixed,false);assert(!relink.definition.constraints.some(c=>c.spline_point));assert(old.constraints.some(c=>c.spline_point));
});
test('invalid picks, duplicate namespaces and source-defining targets reject atomically',()=>{
 const d=definition();d.shapes=[{rectangle:{id:'Box',first:'a',second:'b'}}];const before=structuredClone(d),base={name:'hit',first:'horizontal',second:'vertical',position:[0,0],radius:2};
 for(const patch of [{name:'a'},{name:'Box'},{name:' horizontal '},{radius:0},{radius:NaN},{position:[Infinity,0]},{position:[0,0,0]},{second:'horizontal'},{first:'missing'},{point:'a'},{point:'missing'}])assert.throws(()=>pickedCrossing(d,{...base,...patch}));assert.deepEqual(d,before);
});
test('canvas picker requires two distinct curves and a click hint, preserving drafts until completion',()=>mock(()=>{
 const d=definition(),before=structuredClone(d),host=new Element('div'),scene={},shown=[],messages=[],modes=[];let result;
 const picker=createCrossingPicker({host,definition:d,changed:(v,id)=>result={v,id},message:m=>messages.push(m),ghost:p=>shown.push(p),modeChanged:m=>modes.push(m)}),event=(entity,position=[0,0])=>({scene,entity,position,tolerance:1});
 const click=(entity,position)=>{picker.down(event(entity,position));picker.up(event(entity,position));};find(host,'Start crossing pick').onclick();assert(picker.active);assert.equal(modes.at(-1),'crossing');click(undefined);assert.equal(result,undefined);click('horizontal');click('horizontal');assert(messages.at(-1).includes('different second'));click('vertical');picker.move(event(undefined,[.1,.2]));assert.equal(shown.at(-1)[0].length,65);assert.equal(result,undefined);click(undefined,[.1,.2]);assert(!picker.active);assert.equal(result.id,'Crossing1');assert.equal(result.v.constraints.at(-1).crossing_point.first,'horizontal');assert.deepEqual(shown.at(-1),[]);assert.equal(modes.at(-1),'select');assert.deepEqual(d,before);
}));
test('dragging, cancellation, changed native views and stale contexts never stage a link',()=>mock(()=>{
 const d=definition(),host=new Element('div'),scene={},messages=[];let result,current=true;
 const p=createCrossingPicker({host,definition:d,changed:v=>result=v,message:m=>messages.push(m),current:()=>current}),event=(entity,position=[0,0],s=scene)=>({scene:s,entity,position,tolerance:1});const start=()=>find(host,'Start crossing pick').onclick();
 start();p.down(event('horizontal'));p.move(event('horizontal',[2,0]));p.up(event('horizontal',[0,0]));assert(messages.at(-1).includes('drag'));assert(!all(host).some(e=>e.textContent?.startsWith('First curve:')));assert.equal(result,undefined);
 p.down(event('horizontal'));p.up(event('horizontal'));p.down(event('vertical',[0,0],{}));assert(!p.active);assert.equal(result,undefined);
 start();current=false;p.down(event('horizontal'));assert(!p.active);assert.equal(result,undefined);current=true;start();find(host,'Cancel crossing pick').onclick();assert(!p.active);assert.equal(result,undefined);
}));
test('existing targets and radius are captured at start and invalid options do not arm picking',()=>mock(()=>{
 const host=new Element('div');let result;const picker=createCrossingPicker({host,definition:definition(),changed:v=>result=v,message(){}});input(host,'Picked crossing search radius (mm)').value='';find(host,'Start crossing pick').onclick();assert(!picker.active);input(host,'Picked crossing search radius (mm)').value='3';input(host,'Picked crossing target').value='existing';find(host,'Start crossing pick').onclick();assert(input(host,'Picked crossing target').disabled);const scene={};for(const entity of ['horizontal','vertical',undefined]){const e={entity,scene,position:[0,0],tolerance:1};picker.down(e);picker.up(e);}assert.equal(result.points.length,5);assert.equal(result.constraints.at(-1).crossing_point.maximum_distance.literal.value,3);
}));

test('native numeric window feedback matches inclusive native distance semantics with bounded hover work',()=>{
 assert.equal(crossingWindowFeedback([[3,4]],[0,0],5).kind,'unique');assert.equal(crossingWindowFeedback([[3,4]],[0,0],4.99).kind,'missing');assert.equal(crossingWindowFeedback([[-8,5],[8,5]],[0,5],9).kind,'ambiguous');assert.throws(()=>crossingWindowFeedback([[NaN,0]],[0,0],1));assert.throws(()=>crossingWindowFeedback(Array(1001).fill([0,0]),[0,0],1));
 const points=Array.from({length:1000},(_,i)=>[i,0]),now=performance.now();for(let i=0;i<1000;i++)assert.equal(crossingWindowFeedback(points,[i,0],.2).kind,'unique');const elapsed=performance.now()-now;console.log(`1000 hovers over 1000 native candidates: ${elapsed.toFixed(1)} ms`);assert(elapsed<2000);
});
async function asyncMock(action){globalThis.document={createElement:tag=>new Element(tag)};globalThis.Option=class extends Element{constructor(text,value){super('option');this.textContent=text;this.value=value;}};try{await action();}finally{delete globalThis.document;delete globalThis.Option;}}
const flush=()=>new Promise(resolve=>setImmediate(resolve));
const snapshot=points=>({complete:true,first:'horizontal',second:'vertical',coordinate_system:'sketch-local millimeters',points});
test('one native query serves every hover and blocks staging until a unique result is ready',async()=>asyncMock(async()=>{
 const host=new Element('div'),scene={};let resolveQuery,queries=0,result;const picker=createCrossingPicker({host,definition:definition(),message(){},changed:v=>result=v,queryCandidates:()=>{queries++;return new Promise(resolve=>resolveQuery=resolve);}}),event=(entity,position=[0,0])=>({scene,entity,position,tolerance:1}),click=(entity,position)=>{picker.down(event(entity,position));picker.up(event(entity,position));};
 find(host,'Start crossing pick').onclick();click('horizontal');click('vertical');assert.equal(queries,1);for(let i=0;i<100;i++)picker.move(event(undefined,[8.5,5]));click(undefined,[8.5,5]);assert.equal(result,undefined);resolveQuery(snapshot([[-8,5],[8,5]]));await flush();assert(all(host).some(e=>e.textContent?.startsWith('Native window: one finite')));click(undefined,[8.5,5]);assert(result);assert.equal(queries,1);
}));
test('missing and ambiguous windows remain unmodified, and incomplete or failed native responses never stage',async()=>asyncMock(async()=>{
 const host=new Element('div'),scene={};let result,reply=snapshot([[-8,0],[8,0]]);const picker=createCrossingPicker({host,definition:definition(),message(){},changed:v=>result=v,queryCandidates:async()=>reply}),event=(entity,position=[0,0])=>({scene,entity,position,tolerance:1}),click=(entity,position)=>{picker.down(event(entity,position));picker.up(event(entity,position));},sources=async()=>{find(host,'Start crossing pick').onclick();click('horizontal');click('vertical');await flush();};
 input(host,'Picked crossing search radius (mm)').value='10';await sources();click(undefined,[0,0]);assert.equal(result,undefined);assert(all(host).some(e=>e.textContent?.includes('2 finite crossings')));picker.move(event(undefined,[0,30]));click(undefined,[0,30]);assert.equal(result,undefined);assert(picker.active);picker.cancel();
 reply={...snapshot([[0,0]]),complete:false};await sources();click(undefined,[0,0]);assert.equal(result,undefined);assert(all(host).some(e=>e.textContent?.includes('incomplete')));picker.cancel();
 reply=null;await sources();assert(all(host).some(e=>e.textContent?.includes('Native crossing query failed')));assert.equal(result,undefined);
}));
test('late native responses cannot revive canceled picks or contaminate a new pair snapshot',async()=>asyncMock(async()=>{
 const host=new Element('div'),scene={},requests=[];let result;const picker=createCrossingPicker({host,definition:definition(),message(){},changed:v=>result=v,queryCandidates:()=>new Promise(resolve=>requests.push(resolve))}),event=(entity)=>({scene,entity,position:[0,0],tolerance:1}),click=entity=>{picker.down(event(entity));picker.up(event(entity));},sources=()=>{find(host,'Start crossing pick').onclick();click('horizontal');click('vertical');};
 sources();picker.cancel();sources();requests[0](snapshot([[0,0]]));await flush();click(undefined);assert.equal(result,undefined);assert(picker.active);requests[1](snapshot([[0,0]]));await flush();click(undefined);assert(result);assert(!picker.active);
}));

test('live radius edits resolve ambiguity and stage only the latest valid radius from one native snapshot',async()=>asyncMock(async()=>{
 const host=new Element('div'),scene={},definitionBefore=definition(),before=structuredClone(definitionBefore),shown=[];let result,queries=0;
 const picker=createCrossingPicker({host,definition:definitionBefore,message(){},ghost:paths=>shown.push(paths),changed:v=>result=v,queryCandidates:async()=>{queries++;return snapshot([[-8,0],[8,0]]);}}),event=(entity,position=[-8,0])=>({scene,entity,position,tolerance:1}),click=entity=>{picker.down(event(entity));picker.up(event(entity));},radius=input(host,'Picked crossing search radius (mm)'),set=value=>{radius.value=value;radius.oninput();};
 radius.value='20';find(host,'Start crossing pick').onclick();assert(!radius.disabled);click('horizontal');click('vertical');await flush();picker.move(event());assert(all(host).some(e=>e.textContent?.includes('2 finite crossings')));
 set('1');assert(all(host).some(e=>e.textContent?.includes('one finite crossing')));assert.equal(queries,1);
 for(const invalid of ['','0','-1','Infinity']){set(invalid);assert.deepEqual(shown.at(-1),[]);click();assert.equal(result,undefined);assert(picker.active);}
 set('0.5');assert(all(host).some(e=>e.textContent?.includes('one finite crossing')));assert.deepEqual(definitionBefore,before);click();assert.equal(result.constraints.at(-1).crossing_point.maximum_distance.literal.value,.5);assert.equal(queries,1);
}));
test('radius edits made while candidates load survive late responses and remain blocked while invalid',async()=>asyncMock(async()=>{
 const host=new Element('div'),scene={};let resolveQuery,result,queries=0;
 const picker=createCrossingPicker({host,definition:definition(),message(){},changed:v=>result=v,queryCandidates:()=>{queries++;return new Promise(resolve=>resolveQuery=resolve);}}),event=entity=>({scene,entity,position:[8,0],tolerance:1}),click=entity=>{picker.down(event(entity));picker.up(event(entity));},radius=input(host,'Picked crossing search radius (mm)');
 find(host,'Start crossing pick').onclick();click('horizontal');click('vertical');picker.move(event());radius.value='';radius.oninput();resolveQuery(snapshot([[-8,0],[8,0]]));await flush();assert(all(host).some(e=>e.textContent?.includes('Enter a positive finite crossing search radius')));click();assert.equal(result,undefined);
 radius.value='0.25';radius.oninput();assert(all(host).some(e=>e.textContent?.includes('one finite crossing')));click();assert.equal(result.constraints.at(-1).crossing_point.maximum_distance.literal.value,.25);assert.equal(queries,1);
}));
test('radius edits invalidate pending click gestures and cannot reuse a stale native view',async()=>asyncMock(async()=>{
 const host=new Element('div'),scene={};let result,current=true;const picker=createCrossingPicker({host,definition:definition(),message(){},changed:v=>result=v,current:()=>current,queryCandidates:async()=>snapshot([[0,0]])}),event=entity=>({scene,entity,position:[0,0],tolerance:1}),click=entity=>{picker.down(event(entity));picker.up(event(entity));},radius=input(host,'Picked crossing search radius (mm)');
 find(host,'Start crossing pick').onclick();click('horizontal');click('vertical');await flush();picker.down(event());radius.value='1';radius.oninput();picker.up(event());assert.equal(result,undefined);assert(picker.active);current=false;radius.value='2';radius.oninput();assert(!picker.active);click();assert.equal(result,undefined);
}));
