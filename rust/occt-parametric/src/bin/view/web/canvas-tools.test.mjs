import {test} from 'node:test';
import assert from 'node:assert/strict';
import {snapPosition,drawPrimitive,primitiveGhost,nextId,createCanvasTools} from './canvas-tools.mjs';
const empty=()=>({points:[],lines:[],circles:[],constraints:[],profile:[]});
const click=(x,y,point=null)=>({xy:[x,y],point});

test('snapping prefers authored native point positions, then an optional grid',()=>{
  const d=drawPrimitive(empty(),'point',[click(2,3)]).definition;
  assert.deepEqual(snapPosition([10.2,8.1],d,{P1:[10,8,0]},1,.5),{point:'P1',xy:[10,8]});
  assert.deepEqual(snapPosition([4.4,7.6],d,{},1,.1),{point:null,xy:[4,8]});
  assert.deepEqual(snapPosition([4.4,7.6],d,{},0,.1),{point:null,xy:[4.4,7.6]});
  assert.throws(()=>snapPosition([NaN,1],d),/finite/);
});
test('drawing names stay stable, reuse picked points, and create closed constrained rectangles',()=>{
  const source=empty(),rect=drawPrimitive(source,'rectangle',[click(0,0),click(10,5)],{profile:true}).definition;
  assert.equal(source.points.length,0);assert.equal(rect.points.length,4);assert.equal(rect.lines.length,4);assert.equal(rect.constraints.length,4);assert.deepEqual(rect.profile,rect.lines.map(l=>l.id));
  for(let i=0;i<4;i++)assert.equal(rect.lines[i].end,rect.lines[(i+1)%4].start);
  const line=drawPrimitive(rect,'line',[click(0,0,'P1'),click(3,4)]).definition;assert.equal(line.lines.at(-1).start,'P1');assert.equal(line.points.length,5);assert.equal(nextId(line,'P'),'P6');
  const circle=drawPrimitive(source,'circle',[click(1,1),click(4,1)],{profile:true}).definition;assert.deepEqual(circle.profile,['C1']);assert.equal(circle.circles[0].center,'P1');
  assert.throws(()=>drawPrimitive(source,'rectangle',[click(0,0),click(0,2)]),/width/);
  assert.throws(()=>drawPrimitive({...source,holes:[['hole']]},'circle',[click(0,0),click(2,0)],{profile:true}),/profile controls/);
  assert.equal(primitiveGhost('circle',[click(0,0)],click(2,0)).length,65);
});
class Element{constructor(tag){this.tagName=tag;this.children=[];this.value='';}append(...items){this.children.push(...items);}setAttribute(k,v){this[k]=v;}}
const all=e=>[e,...e.children.flatMap(all)];
test('canvas gestures cancel unfinished drawings and retain completed local ghosts until native preview',async()=>{
 globalThis.document={createElement:tag=>new Element(tag)};globalThis.Option=class extends Element{constructor(text,value){super('option');this.value=value;}};
 let d=empty(),renders=0,paths=[],drags=[];const host=new Element('div');
 const tools=createCanvasTools({host,getDraft:()=>d,setDraft:value=>d=value,changed:()=>renders++,message(){},ghost:p=>paths=p,dragPoint:async(id,target)=>drags.push([id,target])});
 const select=all(host).find(e=>e['aria-label']==='Canvas tool');select.value='line';select.onchange();
 const event=(x,y,point)=>({position:[x,y],point,tolerance:.01,scene:{points:{}},shiftKey:false});
 tools.down(event(0,0));tools.move(event(10,5));assert.equal(paths.length,1);tools.cancel();assert.equal(paths.length,0);assert.equal(d.points.length,0);
 tools.down(event(0,0));tools.down(event(10,5));assert.equal(d.lines.length,1);assert.equal(renders,1);assert.equal(paths.length,1);tools.reviewed();assert.equal(paths.length,0);
 select.value='drag';select.onchange();assert.equal(tools.down(event(0,0,'P1')),true); // Fixed point: no drag.
 await tools.up(event(4,4));assert.equal(drags.length,0);
 d.points[0].fixed=false;tools.down(event(0,0,'P1'));await tools.up(event(4,4));assert.deepEqual(drags,[['P1',[4,4]]]);
 delete globalThis.document;delete globalThis.Option;
});
