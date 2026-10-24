import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readJoint,jointOperation,jointChanges,jointSummary} from './solid-joints.mjs';
import {readPart,partOperation} from './solid-parts.mjs';
import {createSolidEditor} from './solid-editor.mjs';
const mm=value=>({literal:{value,dimension:'length',unit:'millimeter'}});
const at=(x,y,z)=>({components:{x:mm(x),y:mm(y),z:mm(z)}});
const down={components:{x:{literal:{value:0,dimension:'scalar',unit:null}},y:{literal:{value:0,dimension:'scalar',unit:null}},z:{literal:{value:-1,dimension:'scalar',unit:null}}}};
const solids=[{id:'base',operation:{box:{}}},{id:'lid',operation:{box:{}}}];
const catalog={parts:[{part:'socket_head_cap_screw_iso_4762',sizes:[3,4,5,6,8],takes_length:true},{part:'hex_nut_iso_4032',sizes:[5,6],takes_length:false}],screw_lengths:{'5':[8,10,12,16],'6':[10,12,16,20]}};

test('a new joint adds its hardware, one holes feature per solid and the group as the result',()=>{
 const state=readJoint(null,'lid');state.stack.push('base');state.fastening='tapped';state.head='counterbored';state.size='5';state.positions=[[10,10,30],[70,10,30]];
 const body=jointOperation(state);
 assert.deepEqual(body,{stack:['lid','base'],positions:[at(10,10,30),at(70,10,30)],axis:down,nominal_diameter:mm(5),fastening:'tapped',head:'counterbored',washers:true});
 const changes=jointChanges(solids,'fixing',body);
 assert.equal(changes.action,'add');
 assert.deepEqual(changes.features.map(f=>[f.id,Object.keys(f.operation)[0]]),[['fixing-hardware','fastened_joint'],['fixing-lid','joint_holes'],['fixing-base','joint_holes'],['fixing','compound']]);
 assert.deepEqual(changes.features[2].operation.joint_holes,{joint:'fixing-hardware',input:'base'});
 assert.deepEqual(changes.features[3].operation.compound.inputs,['fixing-lid','fixing-base','fixing-hardware']);
 // Lengths, engagement and clearance appear only when set.
 Object.assign(state,{length:'16',engagement:9,clearance:'fine'});
 assert.deepEqual([jointOperation(state).length,jointOperation(state).engagement,jointOperation(state).clearance],[mm(16),mm(9),'fine']);
 assert.throws(()=>jointChanges([...solids,{id:'fixing-lid',operation:{}}],'fixing',body),/'fixing-lid' already exists/);
});

test('joint drafts name what to fix, and saved formulas survive an edit',()=>{
 const state=readJoint(null,'lid');
 assert.throws(()=>jointOperation(state),/at least one screw/);
 state.positions=[[0,0,0]];state.stack=['lid','lid'];assert.throws(()=>jointOperation(state),/once/);
 state.stack=['lid'];state.fastening='tapped';assert.throws(()=>jointOperation(state),/clamped solid and a tapped solid/);
 state.fastening='nut';state.axis=[0,0,0];assert.throws(()=>jointOperation(state),/nonzero screw axis/);
 const saved={stack:['lid','base'],positions:[{parameter:'spot'}],axis:{parameter:'down'},nominal_diameter:{parameter:'bolt'},fastening:'nut',head:'surface',washers:false,length:{parameter:'reach'}};
 const kept=readJoint(saved);
 assert.deepEqual(jointOperation(kept),saved);
 const editing={id:'fixing-hardware',operation:{fastened_joint:saved}};
 assert.deepEqual(jointChanges(solids,'',jointOperation(kept),editing),{action:'replace',features:[{id:'fixing-hardware',operation:{fastened_joint:saved}}]});
 kept.stack=['base'];assert.throws(()=>jointChanges(solids,'',jointOperation(kept),editing),/Keep the joint/);
});

test('joint reports read as one line each',()=>{
 const report={feature:'fixing-hardware',positions:4,screw:{nominal_diameter_mm:5,length_mm:12,chosen:true,minimum_length_mm:10.5},thread_engagement_mm:{min:9,max:9},
   hardware:[{designation:'ISO 4762 M5 × 12',quantity:4}]};
 assert.deepEqual(jointSummary([report]),['fixing-hardware: 4 × M5 × 12, shortest standard length (needs 10.5 mm); thread engaged 9 mm. Parts: 4 × ISO 4762 M5 × 12.']);
 assert.deepEqual(jointSummary(undefined),[]);
});

test('catalog parts check their sizes and lengths and keep saved formulas',()=>{
 const state=readPart(null);state.size='5';state.length=16;state.origin=[1,2,3];
 assert.deepEqual(partOperation(state,catalog),{part:'socket_head_cap_screw_iso_4762',nominal_diameter:mm(5),length:mm(16),origin:at(1,2,3),axis:down});
 state.part='hex_nut_iso_4032';assert.equal(partOperation(state,catalog).length,undefined);
 state.size='7';assert.throws(()=>partOperation(state,catalog),/M5, M6/);
 const saved={part:'socket_head_cap_screw_iso_4762',nominal_diameter:{parameter:'bolt'},length:{parameter:'reach'},origin:{parameter:'seat'},axis:down};
 assert.deepEqual(partOperation(readPart(saved),catalog),saved);
});

class Element{constructor(tag){this.tagName=tag;this.children=[];this.value='';}append(...e){this.children.push(...e);}prepend(...e){this.children.unshift(...e);}replaceChildren(...e){this.children=e;}setAttribute(k,v){this[k]=v;}}
const all=e=>[e,...e.children.flatMap(all)];
const button=(host,text)=>all(host).find(e=>e.tagName==='button'&&e.textContent===text);
const control=(host,label)=>all(host).find(e=>e['aria-label']===label);
const choose=(host,label,value)=>{control(host,label).value=value;control(host,label).onchange();};

test('the solid editor places screws by picking and shows the chosen screw',async()=>{
 globalThis.document={createElement:tag=>new Element(tag)};globalThis.Option=class extends Element{constructor(text,value){super('option');this.textContent=text;this.value=value;}};
 try{
  const host=new Element('div'),calls=[];let resolve;
  const report={feature:'fixing-hardware',positions:2,screw:{nominal_diameter_mm:6,length_mm:30,chosen:true,minimum_length_mm:28.2},thread_engagement_mm:{min:3.4,max:3.4},hardware:[{designation:'ISO 4762 M6 × 30',quantity:2},{designation:'ISO 4032 M6',quantity:2}]};
  createSolidEditor({host,instance:()=> 'part',pickHolePosition:target=>{calls.push({picked:target});return new Promise(r=>resolve=r);},preview:async()=>{},applied:async()=>{},restore:async()=>{},
   request:async(path,options)=>{if(!options)return {version:1,features:solids,output:'lid',parameters:[],hardware:catalog};calls.push(JSON.parse(options.body));return {can_apply:true,geometry:{},fastened_joints:[report]};}});
  await button(host,'Sketch to solid').onclick();
  choose(host,'Solid feature','fastened_joint');
  assert.equal(control(host,'Head-side solid').value,'lid');
  await button(host,'Add stacked solid').onclick();choose(host,'Stacked solid 2','base');
  // Two picks on the head-side solid; a face facing another way is refused.
  for(const position of [[10,10,18],[50,10,18]]){const picking=button(host,'Add screw position on face in canvas').onclick();resolve({target:'lid',position_mm:position,axis:[0,0,-2]});await picking;}
  let picking=button(host,'Add screw position on face in canvas').onclick();resolve({target:'lid',position_mm:[0,5,5],axis:[1,0,0]});await picking;
  assert.equal(calls.filter(c=>c.picked).length,3);
  assert.equal(control(host,'Screw 2 X (mm)').value,50);assert.equal(control(host,'Screw 3 X (mm)'),undefined);
  const name=control(host,'Result feature name');name.value='fixing';name.oninput();
  await button(host,'Preview solid').onclick();
  const edit=calls.at(-1);
  assert.equal(edit.output,'fixing');assert.equal(edit.action,'add');
  assert.deepEqual(edit.features[0].operation.fastened_joint.positions,[at(10,10,18),at(50,10,18)]);
  assert.deepEqual(edit.features[0].operation.fastened_joint.nominal_diameter,mm(6));
  const shown=all(host).find(e=>e['data-joint-report']==='');
  assert.match(shown.children[0].textContent,/2 × M6 × 30, shortest standard length \(needs 28.2 mm\).*2 × ISO 4032 M6/);
 }finally{delete globalThis.document;delete globalThis.Option;}
});

test('a catalog part seats on a picked face and can be grouped with its solid',async()=>{
 globalThis.document={createElement:tag=>new Element(tag)};globalThis.Option=class extends Element{constructor(text,value){super('option');this.textContent=text;this.value=value;}};
 try{
  const host=new Element('div'),calls=[];let resolve;
  createSolidEditor({host,instance:()=> 'part',pickHolePosition:()=>new Promise(r=>resolve=r),preview:async()=>{},applied:async()=>{},restore:async()=>{},
   request:async(path,options)=>{if(!options)return {version:1,features:solids,output:'lid',parameters:[],hardware:catalog};calls.push(JSON.parse(options.body));return {can_apply:true,geometry:{}};}});
  await button(host,'Sketch to solid').onclick();
  choose(host,'Solid feature','standard_part');choose(host,'Solid operation','group');
  choose(host,'Target solid','lid');choose(host,'Catalog part','hex_nut_iso_4032');
  const picking=button(host,'Seat part on face in canvas').onclick();resolve({target:'lid',position_mm:[5,5,18],axis:[0,0,-1]});await picking;
  // A nut sits on the face, so it extends away from it.
  assert.equal(control(host,'Part axis Z').value,1);
  const name=control(host,'Result feature name');name.value='nutted';name.oninput();
  await button(host,'Preview solid').onclick();
  const features=calls.at(-1).features;
  assert.deepEqual(features.map(f=>f.id),['nutted-catalog-part','nutted']);
  assert.equal(features[0].operation.standard_part.part,'hex_nut_iso_4032');
  assert.deepEqual(features[1].operation,{compound:{inputs:['lid','nutted-catalog-part']}});
 }finally{delete globalThis.document;delete globalThis.Option;}
});
