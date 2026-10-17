import {loftDefinition,editLoftStations} from './solid-loft.mjs';
import {test} from 'node:test';
import assert from 'node:assert/strict';
import {sweepOrientation,revolutionDefinition,extrusionDirection,readDirection,controlledDirection,solidChanges,createSolidEditor} from './solid-editor.mjs';
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


test('revolution graphs use signed radians, normalized scalar axes and atomic boolean tools',()=>{
 const revolution=revolutionDefinition([1,2,3],[0,0,4],{literal:{value:-Math.PI,dimension:'scalar',unit:null}},'symmetric');
 assert.equal(revolution.axis.components.z.literal.value,1);
 assert.equal(revolution.origin.components.y.literal.unit,'millimeter');
 for(const mode of ['new','fuse','cut']){
  const graph=solidChanges(base,{kind:'revolve',revolution,mode,id:'turned',input:'profile',target:'body'});
  assert.deepEqual(graph.features[0].operation,{revolve:{input:'profile',...revolution}});
  assert.equal(graph.features[0].id,mode==='new'?'turned':'turned-revolution');
  if(mode==='cut')assert.equal(graph.features[1].operation.cut.tool,'turned-revolution');
  if(mode==='fuse')assert.equal(graph.features[1].operation.fuse.right,'turned-revolution');
 }
 for(const value of [0,7,NaN])assert.throws(()=>revolutionDefinition([0,0,0],[0,0,1],{literal:{value,dimension:'scalar',unit:null}}),/angle/);
 assert.throws(()=>revolutionDefinition([0,0,0],[0,0,0],{parameter:'angle'}),/nonzero/);
 assert.throws(()=>revolutionDefinition([0,Infinity,0],[0,0,1],{parameter:'angle'}),/origin/);
 assert.throws(()=>solidChanges(base,{kind:'revolve',revolution,id:'body',input:'profile',editing:base[1]}),/type/);
});

test('revolution forms use profile axes and preserve authored formulas on editing',async()=>{
 globalThis.document={createElement:tag=>new Element(tag)};globalThis.Option=class extends Element{constructor(text,value){super('option');this.textContent=text;this.value=value;}};
 try{
 const host=new Element('div'),calls=[],frame={origin:[2,3,4],axis:[0,1,0]};
 const saved={id:'turned',operation:{revolve:{input:'profile',origin:{parameter:'axis-origin'},axis:{parameter:'axis-vector'},angle_radians:{multiply:[{parameter:'angle'},{literal:{value:2,dimension:'scalar',unit:null}}]},extent:'symmetric'}}};
 createSolidEditor({host,instance:()=> 'part',profileAxis:()=>frame,preview:async()=>{},applied:async()=>{},restore:async()=>{},request:async(path,options)=>{
  if(!options)return {version:1,features:[...base,saved],output:'turned',parameters:[]};
  const body=JSON.parse(options.body);calls.push(body);return {can_apply:true};
 }});
 await button(host,'Sketch to solid').onclick();
 control(host,'Solid feature').value='revolve';control(host,'Solid feature').onchange();
 assert.equal(control(host,'Axis origin X (mm)').value,2);
 control(host,'Axis origin X (mm)').value='8';control(host,'Axis origin X (mm)').oninput();assert.equal(frame.origin[0],2);
 control(host,'Result feature name').value='new-ring';control(host,'Result feature name').oninput();
 control(host,'Revolution angle (degrees)').value='-90';control(host,'Revolution angle (degrees)').oninput();
 await button(host,'Preview solid').onclick();assert.equal(calls[0].features[0].operation.revolve.angle_radians.literal.value,-Math.PI/2);
 button(host,'Edit revolution turned').onclick();await button(host,'Preview solid').onclick();assert.deepEqual(calls[1].features,[saved]);
 control(host,'Revolution extent').value='angle';control(host,'Revolution extent').onchange();
 await button(host,'Preview solid').onclick();assert.deepEqual(calls[2].features[0].operation.revolve,{...saved.operation.revolve,extent:'angle'});
 }finally{delete globalThis.document;delete globalThis.Option;}
});


test('sweep graphs retain a separate path, orientation formula and atomic boolean tool',()=>{
 const features=[...base,{id:'route',operation:{sketch_open_wire:{sketch:{}}}}];
 const saved={binormal:{direction:{parameter:'normal'}}};
 assert.deepEqual(sweepOrientation('preserve',[0,0,1],saved),saved);
 assert.deepEqual(sweepOrientation('binormal',[0,3,0]),{binormal:{direction:{components:{x:{literal:{value:0,dimension:'scalar',unit:null}},y:{literal:{value:1,dimension:'scalar',unit:null}},z:{literal:{value:0,dimension:'scalar',unit:null}}}}}});
 assert.throws(()=>sweepOrientation('binormal',[0,0,0]),/nonzero/);
 for(const mode of ['new','fuse','cut']){
  const graph=solidChanges(features,{kind:'sweep',mode,id:'pipe',input:'profile',path:'route',orientation:saved,target:'body'});
  assert.deepEqual(graph.features[0].operation,{sweep:{profile:'profile',path:'route',orientation:saved}});
  if(mode!=='new')assert.equal(graph.features[0].id,'pipe-sweep');
 }
 assert.throws(()=>solidChanges(features,{kind:'sweep',mode:'new',id:'pipe',input:'profile',path:'profile'}),/separate/);
 assert.throws(()=>solidChanges(features,{kind:'sweep',mode:'new',id:'pipe',input:'profile',path:'missing'}),/path/);
});

test('sweep form chooses an applied wire and edits preserve binormal formula drivers',async()=>{
 globalThis.document={createElement:tag=>new Element(tag)};globalThis.Option=class extends Element{constructor(text,value){super('option');this.textContent=text;this.value=value;}};
 try{
 const host=new Element('div'),calls=[],shown=[];
 const saved={id:'pipe',operation:{sweep:{profile:'profile',path:'route',orientation:{binormal:{direction:{parameter:'normal'}}}}}};
 const features=[...base,{id:'route',operation:{sketch_open_wire:{sketch:{}}}},{id:'active-route',operation:{sketch_open_wire:{sketch:{}}}},saved];
 createSolidEditor({host,initialProfile:()=> 'active-route',instance:()=> 'part',preview:async()=>{},applied:async()=>{},restore:async()=>{},showSketch:id=>shown.push(id),request:async(path,options)=>{
  if(!options)return {version:1,features,output:'pipe',parameters:[]};
  const body=JSON.parse(options.body);calls.push(body);return {can_apply:true};
 }});
 await button(host,'Sketch to solid').onclick();control(host,'Solid feature').value='sweep';control(host,'Solid feature').onchange();
 assert.equal(control(host,'Sweep path').value,'active-route');assert.equal(control(host,'Revolution extent'),undefined);assert.equal(control(host,'Extrusion extent'),undefined);
 control(host,'Result feature name').value='new-pipe';control(host,'Result feature name').oninput();
 await button(host,'Show sweep path').onclick();assert.deepEqual(shown,['active-route']);
 control(host,'Sweep orientation').value='fixed';control(host,'Sweep orientation').onchange();await button(host,'Preview solid').onclick();assert.equal(calls[0].features[0].operation.sweep.orientation,'fixed');
 await button(host,'Edit sweep pipe').onclick();assert.equal(control(host,'Sweep orientation').value,'preserve');await button(host,'Preview solid').onclick();assert.deepEqual(calls[1].features,[saved]);
 control(host,'Sweep orientation').value='binormal';control(host,'Sweep orientation').onchange();control(host,'Binormal Y').value='2';control(host,'Binormal Y').oninput();await button(host,'Preview solid').onclick();assert(calls[2].features[0].operation.sweep.orientation.binormal.direction.components.y.literal.value>0);
 }finally{delete globalThis.document;delete globalThis.Option;}
});


test('loft definitions validate ordered outer sections, distinct complete hole tracks and limits',()=>{
 const features=['a','b','c','ha','hb','hc'].map(id=>({id})),definition={profiles:['a','b','c'],holes:[['ha','hb','hc']],ruled:true};
 assert.deepEqual(loftDefinition(features,definition),definition);
 for(const invalid of [{profiles:['a']},{profiles:['a','a']},{profiles:['a','b'],holes:[['ha']]},{profiles:['a','b'],holes:[['a','hb']]},{profiles:['a','missing']},{profiles:['a','b'],ruled:'yes'},{profiles:Array(1001).fill('a')},{profiles:['a','b'],holes:Array(101).fill(['ha','hb'])}])assert.throws(()=>loftDefinition(features,invalid));
 assert.deepEqual(definition.profiles,['a','b','c']);
});
test('loft station moves, insertion and removal align every hole track without mutating sources',()=>{
 const definition={profiles:['a','b','c'],holes:[['ha','hb','hc'],['ja','jb','jc']],ruled:false};
 const moved=editLoftStations(definition,'earlier',2);assert.deepEqual(moved.profiles,['a','c','b']);assert.deepEqual(moved.holes,[['ha','hc','hb'],['ja','jc','jb']]);
 assert.deepEqual(editLoftStations(moved,'later',1),definition);
 const added=editLoftStations(definition,'add');assert.equal(added.profiles.at(-1),'');assert(added.holes.every(track=>track.at(-1)===''));
 const removed=editLoftStations(definition,'remove',1);assert.deepEqual(removed.holes,[['ha','hc'],['ja','jc']]);assert.deepEqual(definition.profiles,['a','b','c']);
 assert.throws(()=>editLoftStations(removed,'remove',0),/two/);assert.throws(()=>editLoftStations(definition,'earlier',0),/further/);assert.throws(()=>editLoftStations(definition,'remove',-1),/existing/);
});
test('loft solid graphs use native sections for new solids, boolean tools and stable replacements',()=>{
 const features=[...base,{id:'upper',operation:{sketch_wire:{sketch:{}}}}],loft={profiles:['profile','upper'],holes:[],ruled:false};
 for(const mode of ['new','fuse','cut']){
  const graph=solidChanges(features,{kind:'profile_loft',loft,id:'transition',mode,target:'body'});assert.deepEqual(graph.features[0].operation,{profile_loft:loft});if(mode!=='new')assert.equal(graph.features[0].id,'transition-loft');
 }
 const editing={id:'transition',operation:{profile_loft:loft}};
 assert.equal(solidChanges([...features,editing],{kind:'profile_loft',loft,id:'transition',editing}).action,'replace');
});
test('loft form reorders outer and hole profiles together and preserves edited native tracks',async()=>{
 globalThis.document={createElement:tag=>new Element(tag)};globalThis.Option=class extends Element{constructor(text,value){super('option');this.textContent=text;this.value=value;}};
 try{
 const host=new Element('div'),calls=[];
 const saved={id:'duct',operation:{profile_loft:{profiles:['lower','middle','upper'],holes:[['bore-lower','bore-middle','bore-upper']],ruled:true}}};
 const features=['lower','middle','upper','bore-lower','bore-middle','bore-upper'].map(id=>({id,operation:{sketch_wire:{sketch:{}}}}));features.push(saved);
 createSolidEditor({host,instance:()=> 'part',preview:async()=>{},applied:async()=>{},restore:async()=>{},request:async(path,options)=>{if(!options)return {version:1,features,output:'duct',parameters:[]};calls.push(JSON.parse(options.body));return {can_apply:true};}});
 await button(host,'Sketch to solid').onclick();control(host,'Sketch / planar profile').value='middle';control(host,'Sketch / planar profile').onchange();control(host,'Solid feature').value='profile_loft';control(host,'Solid feature').onchange();assert.equal(control(host,'Outer profile at station 1').value,'middle');await button(host,'Edit loft duct').onclick();assert.equal(control(host,'Sketch / planar profile'),undefined);await button(host,'Preview solid').onclick();assert.deepEqual(calls[0].features,[saved]);
 await button(host,'Move station 3 earlier').onclick();assert.equal(control(host,'Outer profile at station 2').value,'upper');assert.equal(control(host,'Hole 1 at station 2').value,'bore-upper');
 await button(host,'Remove loft station 2').onclick();assert.equal(control(host,'Outer profile at station 2').value,'middle');assert.equal(control(host,'Hole 1 at station 2').value,'bore-middle');
 await button(host,'Add loft station').onclick();assert.equal(control(host,'Hole 1 at station 3').value,'');control(host,'Outer profile at station 3').value='upper';control(host,'Outer profile at station 3').onchange();control(host,'Hole 1 at station 3').value='bore-upper';control(host,'Hole 1 at station 3').onchange();
 await button(host,'Preview solid').onclick();assert.deepEqual(calls[1].features,[saved]);
 await button(host,'Add loft hole track').onclick();assert(control(host,'Hole 2 at station 3'));await button(host,'Remove loft hole track 2').onclick();assert.equal(control(host,'Hole 2 at station 3'),undefined);
 assert.deepEqual(features.at(-1),saved);
 }finally{delete globalThis.document;delete globalThis.Option;}
});
