import {edgeFinishDefinition} from './solid-finishes.mjs';
import {extrusionExtent,extrusionExtentMode,checkedExtrusionExtent} from './solid-extrusion.mjs';
import {loftDefinition,editLoftStations} from './solid-loft.mjs';
import {test} from 'node:test';
import assert from 'node:assert/strict';
import {sweepOrientation,revolutionDefinition,extrusionDirection,readDirection,controlledDirection,solidChanges,solidEditOutput,createSolidEditor} from './solid-editor.mjs';
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


test('geometric extrusion extents retain semantic face selectors and reject missing/self limits',()=>{
 const features=[...base,{id:'stop',operation:{box:{}}}],selector={persistent:{feature:'stop',selector:{largest_area:{planar_only:true,allow_ties:false,relative_tolerance:{literal:{value:1e-6,dimension:'scalar',unit:null}}}}}};
 const next=extrusionExtent('up_to_next','stop'),face=extrusionExtent('up_to_face','stop','z:maximum');assert.deepEqual(next,{up_to_next:{target:'stop'}});assert.equal(face.up_to_face.face.at_extreme.axis,'z');assert.equal(extrusionExtentMode(face),'up_to_face');
 assert.deepEqual(extrusionExtent('up_to_face','stop','preserve',selector).up_to_face.face,selector);
 for(const extent of ['up_to_next',{up_to_next:{target:'missing'}},{up_to_next:{target:'profile'}},{up_to_face:{target:'stop'}},{}])assert.throws(()=>checkedExtrusionExtent(features,extent,'profile','made'));
 for(const mode of ['new','cut','fuse']){const graph=solidChanges(features,{id:'made',mode,input:'profile',direction:extrusionDirection([0,0,1],depth(1)),extent:face,target:'body'});assert.deepEqual(graph.features[0].operation.extrude.extent,face);}
 assert.throws(()=>extrusionExtent('up_to_face','stop','invalid'),/face/);assert.throws(()=>extrusionExtent('up_to_face','stop','preserve'),/face/);
});
test('geometric extrusion forms hide depth and preserve existing face and direction formulas',async()=>{
 globalThis.document={createElement:tag=>new Element(tag)};globalThis.Option=class extends Element{constructor(text,value){super('option');this.textContent=text;this.value=value;}};
 try{
 const host=new Element('div'),calls=[],selector={normal_aligned:{direction:{parameter:'face-normal'},minimum_dot:{literal:{value:0.9999,dimension:'scalar',unit:null}}}};
 const saved={id:'limited',operation:{extrude:{input:'profile',direction:{parameter:'travel'},extent:{up_to_face:{target:'stop',face:selector}}}}};
 const features=[...base,{id:'stop',operation:{box:{}}},saved];
 createSolidEditor({host,instance:()=> 'part',preview:async()=>{},applied:async()=>{},restore:async()=>{},request:async(path,options)=>{if(!options)return {version:1,features,output:'limited',parameters:[]};calls.push(JSON.parse(options.body));return {can_apply:true};}});
 await button(host,'Sketch to solid').onclick();await button(host,'Edit extrusion limited').onclick();assert.equal(control(host,'Extrusion extent').value,'up_to_face');assert.equal(control(host,'Extrusion depth (mm)'),undefined);assert.equal(control(host,'Limiting face').value,'preserve');assert.equal(control(host,'Direction Z').disabled,true);
 await button(host,'Preview solid').onclick();assert.deepEqual(calls[0].features,[saved]);
 control(host,'Extrusion extent').value='up_to_next';control(host,'Extrusion extent').onchange();assert.equal(control(host,'Limiting face'),undefined);await button(host,'Preview solid').onclick();assert.deepEqual(calls[1].features[0].operation.extrude.extent,{up_to_next:{target:'stop'}});assert.deepEqual(calls[1].features[0].operation.extrude.direction,saved.operation.extrude.direction);
 control(host,'Extrusion direction source').value='literal';control(host,'Extrusion direction source').onchange();control(host,'Direction Z').value='-5';control(host,'Direction Z').oninput();await button(host,'Preview solid').onclick();assert.equal(calls[2].features[0].operation.extrude.direction.components.z.multiply[1].literal.value,-1);
 assert.deepEqual(saved.operation.extrude.extent.up_to_face.face,selector);
 }finally{delete globalThis.document;delete globalThis.Option;}
});
test('new geometric extrusions accept entered directions and switching to depth restores distance controls',async()=>{
 globalThis.document={createElement:tag=>new Element(tag)};globalThis.Option=class extends Element{constructor(text,value){super('option');this.textContent=text;this.value=value;}};
 try{
 const host=new Element('div'),calls=[];createSolidEditor({host,instance:()=> 'part',preview:async()=>{},applied:async()=>{},restore:async()=>{},request:async(path,options)=>{if(!options)return {version:1,features:base,output:'body',parameters:[]};calls.push(JSON.parse(options.body));return {can_apply:true};}});
 await button(host,'Sketch to solid').onclick();control(host,'Result feature name').value='to-body';control(host,'Result feature name').oninput();control(host,'Extrusion extent').value='up_to_face';control(host,'Extrusion extent').onchange();assert.equal(control(host,'Limiting feature').value,'body');control(host,'Limiting face').value='z:minimum';control(host,'Limiting face').onchange();await button(host,'Preview solid').onclick();assert.equal(calls[0].features[0].operation.extrude.extent.up_to_face.face.at_extreme.extremum,'minimum');
 control(host,'Extrusion extent').value='distance';control(host,'Extrusion extent').onchange();assert(control(host,'Extrusion depth (mm)'));assert.equal(control(host,'Limiting feature'),undefined);
 }finally{delete globalThis.document;delete globalThis.Option;}
});


test('canvas limiting picks store semantic selectors and late results cannot overwrite edited controls',async()=>{
 globalThis.document={createElement:tag=>new Element(tag)};globalThis.Option=class extends Element{constructor(text,value){super('option');this.textContent=text;this.value=value;}};
 try{
 const host=new Element('div'),calls=[];let resolve,canceled=0;
 createSolidEditor({host,instance:()=> 'part',preview:async()=>{},applied:async()=>{},restore:async()=>{},pickLimitingFace:()=>new Promise(r=>resolve=r),cancelLimitingFacePick:()=>canceled++,request:async(path,options)=>{if(!options)return {version:1,features:base,output:'body',parameters:[]};calls.push(JSON.parse(options.body));return {can_apply:true};}});
 await button(host,'Sketch to solid').onclick();control(host,'Result feature name').value='limited';control(host,'Result feature name').oninput();control(host,'Extrusion extent').value='up_to_face';control(host,'Extrusion extent').onchange();
 const face={normal_aligned:{direction:{parameter:'normal'}}};let pending=button(host,'Pick limiting face in canvas').onclick();resolve({target:'body',face,binding:'normal',origin_mm:[0,0,10]});await pending;assert.equal(control(host,'Limiting face').value,'preserve');await button(host,'Preview solid').onclick();assert.deepEqual(calls[0].features[0].operation.extrude.extent.up_to_face.face,face);assert.equal(JSON.stringify(calls[0].features).includes('face_index'),false);
 pending=button(host,'Pick limiting face in canvas').onclick();control(host,'Extrusion extent').value='distance';control(host,'Extrusion extent').onchange();resolve({target:'body',face:{nearest_center:{}},origin_mm:[0,0,7]});await pending;assert(control(host,'Extrusion depth (mm)'));assert.equal(control(host,'Limiting face'),undefined);assert(canceled>0);
 }finally{delete globalThis.document;delete globalThis.Option;}
});


test('edge finishes create one feature, validate sizes and retain semantic selector expressions',()=>{
 const edges=[{intersection:[{at_extreme:{axis:'y',extremum:'minimum'}},{at_extreme:{axis:'z',extremum:'maximum'}}]}];
 for(const kind of ['fillet','chamfer']){
  const operation=edgeFinishDefinition(kind,'body',edges,{parameter:'size'});assert.deepEqual(operation.edges,edges);assert.deepEqual(operation[kind==='fillet'?'radius':'distance'],{parameter:'size'});
  const graph=solidChanges(base,{id:'finished',input:'body',kind,finish:{edges,size:depth(1)}});assert.equal(graph.features.length,1);assert.equal(graph.action,'add');assert.deepEqual(graph.features[0].operation[kind].edges,edges);
  assert.throws(()=>edgeFinishDefinition(kind,'body',[],depth(1)),/edge/);assert.throws(()=>edgeFinishDefinition(kind,'body',edges,depth(-2)),/positive/);
 }
});
test('finishing canvas picks append unique selectors; changing the source clears them and late picks are ignored',async()=>{
 globalThis.document={createElement:tag=>new Element(tag)};globalThis.Option=class extends Element{constructor(text,value){super('option');this.textContent=text;this.value=value;}};
 try{
 const host=new Element('div'),calls=[];let resolve;createSolidEditor({host,instance:()=> 'part',pickSolidEdge:()=>new Promise(r=>resolve=r),preview:async()=>{},applied:async()=>{},restore:async()=>{},request:async(path,options)=>{if(!options)return {version:1,features:[...base,{id:'other',operation:{box:{}}}],output:'body',parameters:[]};calls.push(JSON.parse(options.body));return {can_apply:true};}});
 await button(host,'Sketch to solid').onclick();control(host,'Solid feature').value='fillet';control(host,'Solid feature').onchange();assert.equal(control(host,'Sketch / planar profile'),undefined);assert.equal(control(host,'Solid operation'),undefined);assert.equal(control(host,'Finish solid').value,'body');control(host,'Result feature name').value='rounded';control(host,'Result feature name').oninput();
 const edge={at_extreme:{axis:'z',extremum:'maximum'}};let pending=button(host,'Pick finishing edge in canvas').onclick();resolve({target:'body',edge});await pending;assert(button(host,'Remove edge set 1'));
 pending=button(host,'Pick finishing edge in canvas').onclick();resolve({target:'body',edge});await pending;assert.equal(button(host,'Remove edge set 2'),undefined);await button(host,'Preview solid').onclick();assert.deepEqual(calls[0].features[0].operation.fillet.edges,[edge]);
 pending=button(host,'Pick finishing edge in canvas').onclick();control(host,'Finish solid').value='other';control(host,'Finish solid').onchange();resolve({target:'body',edge});await pending;assert.equal(button(host,'Remove edge set 1'),undefined);
 }finally{delete globalThis.document;delete globalThis.Option;}
});
test('editing fillets and chamfers preserves compound size formulas and existing edge sets',async()=>{
 globalThis.document={createElement:tag=>new Element(tag)};globalThis.Option=class extends Element{constructor(text,value){super('option');this.textContent=text;this.value=value;}};
 try{
 const host=new Element('div'),calls=[],edges=[{reference:'top-rim'}],size={add:[{parameter:'size'},depth(1)]};
 const finishes=['fillet','chamfer'].map(kind=>({id:kind,operation:{[kind]:{input:'body',edges,[kind==='fillet'?'radius':'distance']:size}}}));
 createSolidEditor({host,instance:()=> 'part',preview:async()=>{},applied:async()=>{},restore:async()=>{},request:async(path,options)=>{if(!options)return {version:1,features:[...base,...finishes],output:'body',parameters:[]};calls.push(JSON.parse(options.body));return {can_apply:true};}});
 await button(host,'Sketch to solid').onclick();for(const kind of ['fillet','chamfer']){await button(host,`Edit ${kind} ${kind}`).onclick();assert.equal(control(host,'Finishing size source').value,'preserve');await button(host,'Preview solid').onclick();assert.deepEqual(calls.at(-1).features,[finishes.find(f=>f.id===kind)]);assert.equal(calls.at(-1).action,'replace');}
 }finally{delete globalThis.document;delete globalThis.Option;}
});


test('editing an unused solid previews that feature; downstream outputs retain the complete part view',()=>{
 const features=[...base,{id:'rounded',operation:{fillet:{input:'body'}}},{id:'finished',operation:{translate:{input:'rounded'}}}];
 const dependencies={profile:[],body:['profile'],rounded:['body'],finished:['rounded']};
 assert.equal(solidEditOutput({features,dependencies,output:'body'},{id:'rounded'}),'rounded');
 assert.equal(solidEditOutput({features,dependencies,output:'finished'},{id:'rounded'}),'finished');
 assert.equal(solidEditOutput({features,dependencies,output:'profile'},{id:'body'}),'body');
});

test('hole form creates a drilling operation and preserves finish, thread and placement formulas on edit',async()=>{
 globalThis.document={createElement:tag=>new Element(tag)};globalThis.Option=class extends Element{constructor(text,value){super('option');this.textContent=text;this.value=value;}};
 try{
 const saved={id:'tapped',operation:{hole:{input:'body',position:{parameter:'entry'},axis:{parameter:'axis'},diameter:{parameter:'bore'},extent:{blind:{depth:{add:[{parameter:'depth'},depth(1)]}}},bottom:{drill_point:{angle_radians:{parameter:'tip'}}},finish:{counterbore:{diameter:depth(8),depth:depth(2),relief:depth(6)}},thread:{designation:'retained-metadata'}}}};
 const host=new Element('div'),calls=[];
 createSolidEditor({host,instance:()=> 'part',preview:async()=>{},applied:async()=>{},restore:async()=>{},request:async(path,options)=>{if(!options)return {version:1,features:[...base,saved],output:'body',parameters:[]};calls.push(JSON.parse(options.body));return {can_apply:true};}});
 await button(host,'Sketch to solid').onclick();control(host,'Solid feature').value='hole';control(host,'Solid feature').onchange();
 assert.equal(control(host,'Sketch / planar profile'),undefined);assert.equal(control(host,'Solid operation'),undefined);
 control(host,'Result feature name').value='drilled';control(host,'Result feature name').oninput();
 for(const [label,value] of [['Hole position X (mm)','10'],['Hole position Y (mm)','10'],['Hole position Z (mm)','10']]){control(host,label).value=value;control(host,label).oninput();}
 await button(host,'Preview solid').onclick();assert.equal(calls.length,1);assert.equal(calls[0].features.length,1);assert.equal(calls[0].features[0].operation.hole.input,'body');
 assert.equal(calls[0].features[0].operation.hole.extent.blind.depth.literal.value,5);
 control(host,'Hole extent').value='through_all';control(host,'Hole extent').onchange();assert.equal(control(host,'Hole depth (mm)'),undefined);
 await button(host,'Preview solid').onclick();assert.equal(calls[1].features[0].operation.hole.extent,'through_all');
 await button(host,'Edit hole tapped').onclick();await button(host,'Preview solid').onclick();assert.deepEqual(calls[2].features,[saved]);
 control(host,'Hole diameter source').value='literal';control(host,'Hole diameter source').onchange();control(host,'Hole diameter (mm)').value='4';control(host,'Hole diameter (mm)').oninput();
 await button(host,'Preview solid').onclick();assert.deepEqual(calls[3].features[0].operation.hole,{...saved.operation.hole,diameter:depth(4)});
 }finally{delete globalThis.document;delete globalThis.Option;}
});

test('hole face picks retain semantic selectors and ignore replies after a field edit',async()=>{
 globalThis.document={createElement:tag=>new Element(tag)};globalThis.Option=class extends Element{constructor(text,value){super('option');this.textContent=text;this.value=value;}};
 try{
 const host=new Element('div'),calls=[];let resolve;
 createSolidEditor({host,instance:()=> 'part',preview:async()=>{},applied:async()=>{},restore:async()=>{},pickLimitingFace:()=>new Promise(r=>resolve=r),request:async(path,options)=>{if(!options)return {version:1,features:base,output:'body',parameters:[]};calls.push(JSON.parse(options.body));return {can_apply:true};}});
 await button(host,'Sketch to solid').onclick();control(host,'Solid feature').value='hole';control(host,'Solid feature').onchange();control(host,'Result feature name').value='hole';control(host,'Result feature name').oninput();control(host,'Hole extent').value='up_to_face';control(host,'Hole extent').onchange();
 let pending=button(host,'Pick hole limiting face in canvas').onclick();control(host,'Hole position X (mm)').value='10';control(host,'Hole position X (mm)').oninput();resolve({target:'body',face:{stale:true}});await pending;
 await button(host,'Preview solid').onclick();assert.equal(calls.length,0);
 pending=button(host,'Pick hole limiting face in canvas').onclick();const face={at_extreme:{axis:'z',extremum:'minimum',tolerance:depth(1e-6)}};resolve({target:'body',face});await pending;
 await button(host,'Preview solid').onclick();assert.deepEqual(calls[0].features[0].operation.hole.extent,{up_to_face:{face}});
 }finally{delete globalThis.document;delete globalThis.Option;}
});

test('hole recess and drill-point controls convert degrees, link parameters and retain hidden finish drafts',async()=>{
 globalThis.document={createElement:tag=>new Element(tag)};globalThis.Option=class extends Element{constructor(text,value){super('option');this.textContent=text;this.value=value;}};
 try{
 const host=new Element('div'),calls=[],parameters=[{id:'tip',parameter_type:{scalar:'scalar'}},{id:'recess',parameter_type:{scalar:'length'}}];
 createSolidEditor({host,instance:()=> 'part',preview:async()=>{},applied:async()=>{},restore:async()=>{},request:async(path,options)=>{if(!options)return {version:1,features:base,output:'body',parameters};calls.push(JSON.parse(options.body));return {can_apply:true};}});
 const choose=(label,value)=>{control(host,label).value=value;control(host,label).onchange();},enter=(label,value)=>{control(host,label).value=String(value);control(host,label).oninput();},last=()=>calls.at(-1).features[0].operation.hole;
 await button(host,'Sketch to solid').onclick();choose('Solid feature','hole');enter('Result feature name','drilled');
 choose('Hole bottom','drill_point');enter('Hole drill point angle (degrees)',120);choose('Hole entry finish','counterbore');enter('Hole counterbore diameter (mm)',9);enter('Hole counterbore depth (mm)',1);choose('Counterbore floor relief','relief');choose('Hole relief diameter source','parameter:recess');
 await button(host,'Preview solid').onclick();assert(Math.abs(last().bottom.drill_point.angle_radians.literal.value-2*Math.PI/3)<1e-12);assert.deepEqual(last().finish.counterbore,{diameter:depth(9),depth:depth(1),relief:{parameter:'recess'}});
 choose('Hole entry finish','countersink');enter('Hole countersink angle (degrees)',90);choose('Hole countersink diameter source','parameter:recess');choose('Hole drill point angle source','parameter:tip');await button(host,'Preview solid').onclick();assert.equal(last().finish.countersink.angle_radians.literal.value,Math.PI/2);assert.deepEqual(last().bottom.drill_point.angle_radians,{parameter:'tip'});
 choose('Hole entry finish','plain');await button(host,'Preview solid').onclick();assert.equal(last().finish,'plain');choose('Hole entry finish','counterbore');await button(host,'Preview solid').onclick();assert.equal(last().finish.counterbore.diameter.literal.value,9);assert.equal(last().finish.counterbore.relief.parameter,'recess');
 choose('Counterbore floor relief','none');await button(host,'Preview solid').onclick();assert.equal(last().finish.counterbore.relief,undefined);
 choose('Hole extent','through_all');const count=calls.length;await button(host,'Preview solid').onclick();assert.equal(calls.length,count);choose('Hole bottom','flat');await button(host,'Preview solid').onclick();assert.equal(last().bottom,'flat');
 choose('Hole entry finish','countersink');choose('Hole countersink angle source','literal');for(const v of [0,180,-1,NaN]){enter('Hole countersink angle (degrees)',v);const count=calls.length;await button(host,'Preview solid').onclick();assert.equal(calls.length,count);}
 enter('Hole countersink angle (degrees)',90);choose('Hole countersink diameter source','literal');enter('Hole countersink diameter (mm)',-2);const rejected=calls.length;await button(host,'Preview solid').onclick();assert.equal(calls.length,rejected);
 }finally{delete globalThis.document;delete globalThis.Option;}
});

test('canvas hole placement replaces placement formulas explicitly and ignores stale or canceled picks',async()=>{
 globalThis.document={createElement:tag=>new Element(tag)};globalThis.Option=class extends Element{constructor(text,value){super('option');this.textContent=text;this.value=value;}};
 try{
 const saved={id:'drilled',operation:{hole:{input:'body',position:{parameter:'entry'},axis:{parameter:'axis'},diameter:depth(4),extent:{blind:{depth:depth(2)}},bottom:'flat',finish:'plain'}}};
 const host=new Element('div'),calls=[];let resolve;
 createSolidEditor({host,instance:()=> 'part',pickHolePosition:()=>new Promise(r=>resolve=r),preview:async()=>{},applied:async()=>{},restore:async()=>{},request:async(path,options)=>{if(!options)return {version:1,features:[...base,saved],output:'drilled',parameters:[]};calls.push(JSON.parse(options.body));return {can_apply:true};}});
 await button(host,'Sketch to solid').onclick();await button(host,'Edit hole drilled').onclick();
 let picking=button(host,'Place hole on face in canvas').onclick();resolve(null);await picking;await button(host,'Preview solid').onclick();assert.deepEqual(calls.at(-1).features,[saved]);
 picking=button(host,'Place hole on face in canvas').onclick();control(host,'Hole bottom').value='drill_point';control(host,'Hole bottom').onchange();resolve({target:'body',position_mm:[99,99,99],axis:[0,0,-1]});await picking;assert.equal(control(host,'Hole placement source').value,'preserve');
 picking=button(host,'Place hole on face in canvas').onclick();resolve({target:'body',position_mm:[8,9,10],axis:[0,0,-1]});await picking;
 assert.equal(control(host,'Hole placement source').value,'literal');assert.equal(control(host,'Hole position X (mm)').value,8);assert.equal(control(host,'Hole axis Z').value,-1);
 await button(host,'Preview solid').onclick();const hole=calls.at(-1).features[0].operation.hole;assert.deepEqual(hole.position.components,{x:depth(8),y:depth(9),z:depth(10)});assert.deepEqual(hole.diameter,saved.operation.hole.diameter);assert.equal(hole.face_index,undefined);
 }finally{delete globalThis.document;delete globalThis.Option;}
});

test('loft end directions and strengths reach the native graph and preserve saved formulas',async()=>{
 globalThis.document={createElement:tag=>new Element(tag)};globalThis.Option=class extends Element{constructor(text,value){super('option');this.textContent=text;this.value=value;}};
 try{
 const profiles=[...base,{id:'upper',operation:{sketch_wire:{sketch:{}}}}],constraint={direction:{parameter:'axis'},magnitude:{multiply:[{parameter:'height'},depth(1)]}},saved={id:'transition',operation:{profile_loft:{profiles:['profile','upper'],holes:[],ruled:false,start_constraint:constraint,end_constraint:constraint}}};
 const host=new Element('div'),calls=[];createSolidEditor({host,instance:()=> 'part',preview:async()=>{},applied:async()=>{},restore:async()=>{},request:async(path,options)=>{if(!options)return {version:1,features:[...profiles,saved],output:'transition',parameters:[{id:'height',parameter_type:{scalar:'length'}}]};calls.push(JSON.parse(options.body));return {can_apply:true};}});
 await button(host,'Sketch to solid').onclick();await button(host,'Edit loft transition').onclick();await button(host,'Preview solid').onclick();assert.deepEqual(calls[0].features,[saved]);
 const choose=(label,value)=>{control(host,label).value=value;control(host,label).onchange();};
 choose('Start loft strength source','parameter:height');choose('End loft strength source','literal');control(host,'End loft strength (mm)').value='25';control(host,'End loft strength (mm)').oninput();await button(host,'Preview solid').onclick();let op=calls.at(-1).features[0].operation.profile_loft;assert.deepEqual(op.start_constraint.magnitude,{parameter:'height'});assert.deepEqual(op.end_constraint.magnitude,depth(25));assert.deepEqual(op.start_constraint.direction,constraint.direction);
 choose('Loft surfaces','ruled');const count=calls.length;await button(host,'Preview solid').onclick();assert.equal(calls.length,count);choose('Loft surfaces','smooth');choose('Start loft condition','free');await button(host,'Preview solid').onclick();op=calls.at(-1).features[0].operation.profile_loft;assert.equal(op.start_constraint,undefined);assert(op.end_constraint);
 choose('Start loft condition','tangent');choose('Start loft direction source','literal');for(const key of ['X','Y','Z']){control(host,`Start loft direction ${key}`).value='0';control(host,`Start loft direction ${key}`).oninput();}const n=calls.length;await button(host,'Preview solid').onclick();assert.equal(calls.length,n);
 }finally{delete globalThis.document;delete globalThis.Option;}
});

test('normal-to-profile loft controls preserve saved strength formulas and explicitly switch direction modes',async()=>{
 globalThis.document={createElement:tag=>new Element(tag)};globalThis.Option=class extends Element{constructor(text,value){super('option');this.textContent=text;this.value=value;}};
 try{
 const constraint={normal_to_profile:true,magnitude:{parameter:'height'}},saved={id:'transition',operation:{profile_loft:{profiles:['profile','upper'],holes:[],ruled:false,start_constraint:constraint,end_constraint:constraint}}};
 const host=new Element('div'),calls=[];createSolidEditor({host,instance:()=> 'part',preview:async()=>{},applied:async()=>{},restore:async()=>{},request:async(path,options)=>{if(!options)return {version:1,features:[...base,{id:'upper',operation:{sketch_wire:{sketch:{}}}},saved],output:'transition',parameters:[{id:'height',parameter_type:{scalar:'length'}}]};calls.push(JSON.parse(options.body));return {can_apply:true};}});
 await button(host,'Sketch to solid').onclick();await button(host,'Edit loft transition').onclick();assert.equal(control(host,'Start loft condition').value,'normal');assert.equal(control(host,'Start loft direction Z'),undefined);await button(host,'Preview solid').onclick();assert.deepEqual(calls[0].features,[saved]);
 const choose=(label,value)=>{control(host,label).value=value;control(host,label).onchange();};
 choose('Start loft condition','tangent');control(host,'Start loft direction X').value='0.2';control(host,'Start loft direction X').oninput();await button(host,'Preview solid').onclick();let op=calls.at(-1).features[0].operation.profile_loft;assert.equal(op.start_constraint.normal_to_profile,undefined);assert.equal(op.start_constraint.direction.components.x.literal.value,0.2);assert.deepEqual(op.start_constraint.magnitude,constraint.magnitude);
 choose('Start loft condition','normal');await button(host,'Preview solid').onclick();assert.deepEqual(calls.at(-1).features,[saved]);choose('Start loft condition','tangent');assert.equal(control(host,'Start loft direction X').value,0.2);choose('Start loft condition','free');await button(host,'Preview solid').onclick();assert.equal(calls.at(-1).features[0].operation.profile_loft.start_constraint,undefined);
 }finally{delete globalThis.document;delete globalThis.Option;}
});

test('loft correspondence follows profile identities through station ordering and removal',()=>{
 const ids=['a','b','c','ha','hb','hc'],features=ids.map(id=>({id})),definition={profiles:['a','b','c'],holes:[['ha','hb','hc']],ruled:false,start_constraint:{normal_to_profile:true,magnitude:depth(10)},correspondence:{b:{reverse:true},hb:{start_point:{parameter:'anchor'}},c:{reverse:true}}};
 assert.deepEqual(loftDefinition(features,definition),definition);
 const moved=editLoftStations(definition,'earlier',2);assert.deepEqual(moved.correspondence,definition.correspondence);
 const removed=editLoftStations(definition,'remove',1);assert.deepEqual(removed.correspondence,{c:{reverse:true}});
 assert.deepEqual(definition.correspondence.b,{reverse:true});
 for(const bad of [{...definition,ruled:true},{...definition,start_constraint:undefined},{...definition,correspondence:{absent:{reverse:true}}},{...definition,correspondence:{b:{reverse:'yes'}}},{...definition,correspondence:{b:{raw_edge_index:0}}}])assert.throws(()=>loftDefinition(features,bad));
});

test('loft correspondence editor preserves formulas, edits anchors and keeps hole settings on reorder',async()=>{
 globalThis.document={createElement:tag=>new Element(tag)};globalThis.Option=class extends Element{constructor(text,value){super('option');this.textContent=text;this.value=value;}};
 try{
 const names=['profile','middle','upper','ha','hb','hc'],profiles=names.map(id=>({id,operation:{sketch_wire:{sketch:{}}}}));
 const saved={id:'transition',operation:{profile_loft:{profiles:['profile','middle','upper'],holes:[['ha','hb','hc']],ruled:false,start_constraint:{normal_to_profile:true,magnitude:depth(20)},correspondence:{middle:{start_point:{parameter:'anchor'}},hb:{reverse:true}}}}};
 const host=new Element('div'),calls=[];createSolidEditor({host,instance:()=> 'part',preview:async()=>{},applied:async()=>{},restore:async()=>{},request:async(path,options)=>{if(!options)return {version:1,features:[...profiles,saved],output:'transition',parameters:[]};calls.push(JSON.parse(options.body));return {can_apply:true};}});
 await button(host,'Sketch to solid').onclick();await button(host,'Edit loft transition').onclick();await button(host,'Preview solid').onclick();assert.deepEqual(calls.at(-1).features,[saved]);
 await button(host,'Move station 2 later').onclick();await button(host,'Preview solid').onclick();let op=calls.at(-1).features[0].operation.profile_loft;assert.deepEqual(op.profiles,['profile','upper','middle']);assert.deepEqual(op.correspondence,saved.operation.profile_loft.correspondence);assert.equal(control(host,'Hole 1 station 3 traversal').value,'reverse');
 const choose=(label,value)=>{control(host,label).value=value;control(host,label).onchange();};
 choose('Outer station 3 start anchor','literal');for(const [key,value] of [['X',0],['Y',4],['Z',10]]){control(host,`Outer station 3 start ${key} (mm)`).value=String(value);control(host,`Outer station 3 start ${key} (mm)`).oninput();}
 await button(host,'Preview solid').onclick();op=calls.at(-1).features[0].operation.profile_loft;assert.deepEqual(op.correspondence.middle.start_point.components,{x:depth(0),y:depth(4),z:depth(10)});
 await button(host,'Remove loft station 3').onclick();await button(host,'Preview solid').onclick();op=calls.at(-1).features[0].operation.profile_loft;assert.equal(op.correspondence,undefined);
 }finally{delete globalThis.document;delete globalThis.Option;}
});

test('canvas loft starts preserve formulas on cancel and reject stale replies before replacing an anchor',async()=>{
 globalThis.document={createElement:tag=>new Element(tag)};globalThis.Option=class extends Element{constructor(text,value){super('option');this.textContent=text;this.value=value;}};
 try{
 const saved={id:'transition',operation:{profile_loft:{profiles:['profile','upper'],holes:[],ruled:false,start_constraint:{normal_to_profile:true,magnitude:depth(20)},correspondence:{upper:{start_point:{parameter:'anchor'}}}}}};
 const host=new Element('div'),calls=[];let resolve,invalidations=0;
 createSolidEditor({host,instance:()=> 'part',pickLoftAnchor:()=>new Promise(r=>resolve=r),invalidatePreview:()=>invalidations++,preview:async()=>{},applied:async()=>{},restore:async()=>{},request:async(path,options)=>{if(!options)return {version:1,features:[...base,{id:'upper',operation:{sketch_wire:{sketch:{}}}},saved],output:'transition',parameters:[]};calls.push(JSON.parse(options.body));return {can_apply:true};}});
 await button(host,'Sketch to solid').onclick();await button(host,'Edit loft transition').onclick();
 let pending=button(host,'Pick outer station 2 start in canvas').onclick();resolve(null);await pending;await button(host,'Preview solid').onclick();assert.deepEqual(calls.at(-1).features,[saved]);
 pending=button(host,'Pick outer station 2 start in canvas').onclick();control(host,'Outer station 2 traversal').value='reverse';control(host,'Outer station 2 traversal').onchange();resolve({profile:'upper',position_mm:[9,9,9]});await pending;assert.equal(control(host,'Outer station 2 start anchor').value,'preserve');
 pending=button(host,'Pick outer station 2 start in canvas').onclick();const before=invalidations;resolve({profile:'upper',position_mm:[0,4,20]});await pending;assert(invalidations>before);assert.equal(control(host,'Outer station 2 start anchor').value,'literal');
 await button(host,'Preview solid').onclick();const alignment=calls.at(-1).features[0].operation.profile_loft.correspondence.upper;assert.equal(alignment.reverse,true);assert.deepEqual(alignment.start_point.components,{x:depth(0),y:depth(4),z:depth(20)});
 }finally{delete globalThis.document;delete globalThis.Option;}
});

test('loft split controls preserve formulas, add exact points, choose starts and follow station identities',async()=>{
 globalThis.document={createElement:tag=>new Element(tag)};globalThis.Option=class extends Element{constructor(text,value){super('option');this.textContent=text;this.value=value;}};
 try{
 const saved={id:'transition',operation:{profile_loft:{profiles:['profile','middle','upper'],holes:[],ruled:false,start_constraint:{normal_to_profile:true,magnitude:depth(20)},correspondence:{middle:{split_points:[{parameter:'cut'}]}}}}};
 const host=new Element('div'),calls=[],picks=[];createSolidEditor({host,instance:()=> 'part',pickLoftAnchor:async(profile,splits)=>{picks.push({profile,splits});return null;},preview:async()=>{},applied:async()=>{},restore:async()=>{},request:async(path,options)=>{if(!options)return {version:1,features:[...base,...['middle','upper'].map(id=>({id,operation:{sketch_wire:{sketch:{}}}})),saved],output:'transition',parameters:[]};calls.push(JSON.parse(options.body));return {can_apply:true};}});
 await button(host,'Sketch to solid').onclick();await button(host,'Edit loft transition').onclick();await button(host,'Preview solid').onclick();assert.deepEqual(calls.at(-1).features,[saved]);
 await button(host,'Add outer station 2 split point').onclick();for(const [key,value] of [['X','1'],['Y','2'],['Z','3']]){control(host,`Outer station 2 split 2 ${key} (mm)`).value=value;control(host,`Outer station 2 split 2 ${key} (mm)`).oninput();}
 await button(host,'Preview solid').onclick();let c=calls.at(-1).features[0].operation.profile_loft.correspondence.middle;assert.deepEqual(c.split_points,[{parameter:'cut'},{components:{x:depth(1),y:depth(2),z:depth(3)}}]);
 await button(host,'Use outer station 2 split 1 as start').onclick();await button(host,'Preview solid').onclick();c=calls.at(-1).features[0].operation.profile_loft.correspondence.middle;assert.deepEqual(c.start_point,{parameter:'cut'});
 await button(host,'Remove outer station 2 split 2').onclick();await button(host,'Move station 2 later').onclick();await button(host,'Pick outer station 3 start in canvas').onclick();assert.deepEqual(picks.at(-1),{profile:'middle',splits:[{parameter:'cut'}]});
 await button(host,'Preview solid').onclick();const op=calls.at(-1).features[0].operation.profile_loft;assert.deepEqual(op.profiles,['profile','upper','middle']);assert.deepEqual(op.correspondence.middle.split_points,[{parameter:'cut'}]);
 }finally{delete globalThis.document;delete globalThis.Option;}
});
