import {test} from 'node:test';
import assert from 'node:assert/strict';
import {newSketchDefinition,createSketchCreator} from './sketch-create.mjs';
const coords=v=>['x','y','z'].map(k=>v.literal[k].value);
const dot=(a,b)=>a.reduce((s,v,i)=>s+v*b[i],0);
test('new rectangle and circle profiles have closed authored boundaries and oriented plane offsets',()=>{
 const r=newSketchDefinition({id:'p',width:20,height:12,plane:'xz',offset:4});
 assert.deepEqual(coords(r.origin),[0,-4,0]);assert.deepEqual(coords(r.x_axis),[1,0,0]);assert.deepEqual(coords(r.y_axis),[0,0,1]);
 assert.deepEqual(r.profile,['AB','BC','CD','DA']);assert.equal(r.points[0].x.literal.value,-10);assert.equal(r.points[0].y.literal.value,-6);
 assert.deepEqual(coords(newSketchDefinition({id:'p',plane:'yz',offset:-3}).origin),[-3,0,0]);
 const c=newSketchDefinition({id:'p',shape:'circle',width:10});
 assert.equal(c.points[1].x.literal.value,5);assert.deepEqual(c.profile,['circle']);assert.equal(c.lines.length,0);
 for(const input of [{width:0},{width:NaN},{height:-1},{offset:Infinity},{plane:'bad'},{shape:'bad'}])assert.throws(()=>newSketchDefinition({id:'p',...input}));
});
test('six face directions use semantic support and a perpendicular right-handed sketch frame',()=>{
 for(const face of ['+x','-x','+y','-y','+z','-z']){
  const s=newSketchDefinition({id:'p',plane:'face',support:'body',face,offset:-2});
  const n=coords(s.face_support.face.normal_aligned.direction),x=coords(s.x_axis),y=coords(s.y_axis);
  assert.equal(dot(n,x),0);assert.equal(dot(n,y),0);assert.equal(dot(x,y),0);assert.equal(dot(x,x),1);assert.equal(dot(y,y),1);
  assert.deepEqual([x[1]*y[2]-x[2]*y[1],x[2]*y[0]-x[0]*y[2],x[0]*y[1]-x[1]*y[0]].map(v=>v||0),n);
  assert.equal(s.face_support.input,'body');assert.equal(s.face_support.offset.literal.value,-2);
 }
 assert.throws(()=>newSketchDefinition({id:'p',plane:'face'}),/support/);
});
class Element{constructor(tag){this.tagName=tag;this.children=[];this.value='';}append(...e){this.children.push(...e);}prepend(...e){this.children.unshift(...e);}replaceChildren(...e){this.children=e;}setAttribute(k,v){this[k]=v;}}
const all=e=>[e,...e.children.flatMap(all)];
const button=(h,t)=>all(h).find(e=>e.tagName==='button'&&e.textContent===t);
const control=(h,l)=>all(h).find(e=>e['aria-label']===l);
test('new sketch preview is guarded and read-only; rejection retains fields for correction',async()=>{
 globalThis.document={createElement:tag=>new Element(tag)};globalThis.Option=class extends Element{constructor(text,value){super('option');this.textContent=text;this.value=value;}};
 const host=new Element('div'),calls=[],views=[],features=[{id:'body',operation:{box:{}}},{id:'new-profile',operation:{sketch_face:{}}}];
 let accepted=null,reject=true,shown;
 createSketchCreator({host,instance:()=> 'part',preview:async d=>views.push(d),applied:async d=>accepted=d,restore:async()=>{},showSketch:id=>shown=id,request:async(path,options)=>{
  if(path==='/api/features')return {version:3,features,output:'body'};
  assert.equal(path,'/api/sketch-create');const body=JSON.parse(options.body);calls.push(body);
  if(body.apply&&reject)throw Error('Support is ambiguous.');
  return body.apply?{state:{version:4},feature:body.feature}:{can_apply:true,annotations:{scenes:[]}};
 }});
 await button(host,'New sketch').onclick();assert.equal(control(host,'New sketch feature name').value,'new-profile-2');
 control(host,'Sketch plane').value='face';control(host,'Sketch plane').onchange();
 control(host,'Face normal').value='-x';control(host,'Face normal').onchange();
 await button(host,'Preview new sketch').onclick();
 assert.equal(accepted,null);assert.equal(views.length,1);assert.equal(calls[0].version,3);assert.deepEqual(calls[0].expected,features);assert.equal(calls[0].definition.face_support.input,'body');assert.equal(shown,'new-profile-2');
 await button(host,'Apply verified new sketch').onclick();assert.equal(accepted,null);assert(button(host,'Apply verified new sketch'));
 reject=false;await button(host,'Apply verified new sketch').onclick();assert.deepEqual(accepted,{version:4});
 delete globalThis.document;delete globalThis.Option;
});
