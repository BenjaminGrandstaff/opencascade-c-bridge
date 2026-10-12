import {test} from 'node:test';
import assert from 'node:assert/strict';
import {extrusionDirection,readDirection,controlledDirection,solidChanges,createSolidEditor} from './solid-editor.mjs';
const depth=value=>({literal:{value,dimension:'length',unit:'millimeter'}});
const base=[{id:'profile',operation:{sketch_face:{sketch:{}}}},{id:'body',operation:{extrude:{input:'profile',direction:{components:{x:depth(0),y:depth(0),z:{parameter:'depth'}}},extent:'distance'}}}];

test('extrusion controls preserve parameter depth, convert units and reject invalid directions',()=>{
 const direction=extrusionDirection([0,0,-2],depth(3));assert.deepEqual(readDirection(direction),{axis:[0,0,-1],depth:depth(3)});
 assert.deepEqual(readDirection(base[1].operation.extrude.direction),{axis:[0,0,1],depth:{parameter:'depth'}});
 assert.deepEqual(readDirection({literal:{x:{value:0,dimension:'length',unit:'inch'},y:{value:0,dimension:'length',unit:'inch'},z:{value:1,dimension:'length',unit:'inch'}}}),{axis:[0,0,1],depth:depth(25.4)});
 assert.throws(()=>extrusionDirection([0,0,0],depth(3)),/nonzero/);assert.throws(()=>extrusionDirection([0,0,1],depth(-1)),/positive/);
 assert.deepEqual(controlledDirection([0,0,1],{parameter:'depth'},base[1].operation.extrude.direction),base[1].operation.extrude.direction);
 assert.equal(readDirection({parameter:'complex-direction'}),null);
});
test('new cuts/unions reference their extrusion and existing target; replacements retain identity',()=>{
 const direction=extrusionDirection([0,0,1],depth(3));
 const cut=solidChanges(base,{mode:'cut',id:'slot',input:'profile',target:'body',direction});
 assert.equal(cut.action,'add');assert.deepEqual(cut.features[1],{id:'slot',operation:{cut:{object:'body',tool:'slot-extrusion'}}});
 const fuse=solidChanges(base,{mode:'fuse',id:'boss',input:'profile',target:'body',direction});assert.deepEqual(fuse.features[1].operation,{fuse:{left:'body',right:'boss-extrusion'}});
 assert.throws(()=>solidChanges(base,{mode:'new',id:'body',input:'profile',direction}),/already exists/);
 const replace=solidChanges(base,{id:'body',input:'profile',direction,editing:base[1]});assert.equal(replace.action,'replace');assert.equal(replace.features.length,1);
});
class Element{constructor(tag){this.tagName=tag;this.children=[];this.value='';}append(...e){this.children.push(...e);}prepend(...e){this.children.unshift(...e);}replaceChildren(...e){this.children=e;}setAttribute(k,v){this[k]=v;}}
const all=e=>[e,...e.children.flatMap(all)];
const button=(host,text)=>all(host).find(e=>e.tagName==='button'&&e.textContent===text);
const control=(host,label)=>all(host).find(e=>e['aria-label']===label);

test('feature controls preview a guarded graph and retain the form after rejection',async()=>{
 globalThis.document={createElement:tag=>new Element(tag)};globalThis.Option=class extends Element{constructor(text,value){super('option');this.textContent=text;this.value=value;}};
 const host=new Element('div'),calls=[],views=[];let accepted=null,reject=true;
 createSolidEditor({host,instance:()=> 'part',preview:async d=>views.push(d),applied:async d=>accepted=d,restore:async()=>{},request:async(path,options)=>{
  if(path==='/api/features'&&!options)return {version:3,features:structuredClone(base),output:'body',parameters:[]};
  const body=JSON.parse(options.body);calls.push(body);if(body.apply&&reject)throw new Error('Required check failed.');
  return body.apply?{state:{version:4},output:'slot'}:{can_apply:true,output:'slot',annotations:{scenes:[]},geometry:{part:{solids:1,volume_mm3:7000}}};
 }});
 await button(host,'Sketch to solid').onclick();control(host,'Solid operation').value='cut';control(host,'Solid operation').onchange();
 control(host,'Result feature name').value='slot';control(host,'Result feature name').oninput();control(host,'Extrusion depth (mm)').value='3';control(host,'Extrusion depth (mm)').oninput();
 await button(host,'Preview solid').onclick();assert.equal(calls[0].version,3);assert.deepEqual(calls[0].expected,base);assert.equal(calls[0].features[1].operation.cut.object,'body');assert.equal(views.length,1);assert.equal(accepted,null);
 await button(host,'Apply verified solid').onclick();assert.equal(accepted,null);assert(button(host,'Apply verified solid'));reject=false;await button(host,'Apply verified solid').onclick();assert.deepEqual(accepted,{version:4});
 delete globalThis.document;delete globalThis.Option;
});
