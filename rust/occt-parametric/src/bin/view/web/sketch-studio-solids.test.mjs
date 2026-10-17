import test from 'node:test';
import assert from 'node:assert/strict';
import {fixture} from './sketch-studio-test-fixture.mjs';
import {installStudioSolids} from './sketch-studio-solids.mjs';
import {basePlane} from './sketch-studio-plane.mjs';
const mm=value=>({literal:{value,dimension:'length',unit:'millimeter'}});
const features=()=>[{id:'profile',operation:{sketch_face:{sketch:{}}}},{id:'body',operation:{extrude:{input:'profile',direction:{components:{x:mm(0),y:mm(0),z:{parameter:'depth'}}}}}}];
const graph=()=>({version:1,features:features(),parameters:[{id:'depth',parameter_type:{scalar:'length'}}],output:'body'});
const preview=()=>({version:1,instance:'part',can_apply:true,output:'made-part',geometry:{part:{valid:true,solids:1,volume_mm3:200}},annotations:{scenes:[]},gltf:{asset:{version:'2.0'},meshes:[]}});
class Element {
 constructor(tag,attrs={},...children){this.tag=tag;this.style={};this.children=[];this.listeners={};Object.assign(this,attrs);this.append(...children.flat());}
 append(...v){for(const n of v)if(n&&typeof n==='object')n.parentElement=this;this.children.push(...v);}
 prepend(...v){this.children.unshift(...v);}
 replaceChildren(...v){this.children=[];this.append(...v);}
 setAttribute(k,v){this[k]=v;}
 addEventListener(k,v){this.listeners[k]=v;}
}
const all=e=>[e,...(e.children??[]).filter(v=>v&&typeof v==='object').flatMap(all)];
function setup(t,{api}={}){
 const f=fixture(t,{api:api??((path,options)=>!options?graph():JSON.parse(options.body).apply?{applied:true,output:'made-part',state:{version:2}}:preview())});
 f.studio.drawing=f.drawing;f.layer.frame=basePlane('xz');f.studio.parsed={accepted:true};f.studio.setSolidPreview=value=>{f.studio.solidPreview=value;};
 f.studio.html=(tag,attrs={},...children)=>new Element(tag,attrs,...children);f.document.body=new Element('body');f.document.createElement=tag=>new Element(tag);
 const oldDocument=globalThis.document,oldOption=globalThis.Option;globalThis.document=f.document;globalThis.Option=class extends Element{constructor(text,value){super('option');this.textContent=text;this.value=value;}};
 t.after(()=>{globalThis.document=oldDocument;globalThis.Option=oldOption;});
 const solids=installStudioSolids(f.studio,f.document),panel=f.document.body.children[0];
 const button=name=>all(panel).find(e=>e.textContent===name),input=name=>all(panel).find(e=>e['aria-label']===name);
 const open=async()=>{assert(solids.open());await f.flush();};
 const enter=(label,value)=>{const e=input(label);e.value=value;e.oninput();};
 return {...f,solids,panel,button,input,open,enter};
}
test('solid controls default to the active profile normal and display a read-only native preview',async t=>{
 const f=setup(t);await f.open();assert.equal(f.input('Sketch / planar profile').value,'profile');assert.equal(f.input('Direction Y').value,-1);
 f.enter('Result feature name','made-part');await f.button('Preview solid').onclick();
 const body=JSON.parse(f.calls.at(-1).body);assert.equal(body.apply,false);assert.equal(body.features[0].operation.extrude.direction.components.y.multiply[1].literal.value,-1);
 assert.deepEqual(f.studio.solidPreview,preview().gltf);assert.deepEqual(f.studio.parsed,{accepted:true});assert.equal(f.studio.version,1);assert(!f.studio.draft);
 f.enter('Extrusion depth (mm)','20');assert.equal(f.studio.solidPreview,null);f.solids.close();assert.equal(f.panel.hidden,true);
});
for(const action of ['close','tool','edit','layer','load','escape'])test(`a late solid preview cannot replace the view after ${action}`,async t=>{
 let resolve;const f=setup(t,{api:(path,options)=>!options?graph():new Promise(r=>resolve=r)});await f.open();f.enter('Result feature name','made-part');const pending=f.button('Preview solid').onclick();
 if(action==='close')f.solids.close();if(action==='tool')f.studio.setTool('line');if(action==='edit')f.edit(1);if(action==='layer')f.studio.onActivate({feature:'other'});if(action==='load')f.studio.onLoad();if(action==='escape')f.studio.onKey({key:'Escape'});
 resolve(preview());await pending;assert.equal(f.studio.solidPreview,null);assert.equal(f.panel.hidden,true);assert.deepEqual(f.studio.parsed,{accepted:true});f.drawing.discard(true);
});
test('rejected and mismatched solid previews leave the accepted view intact',async t=>{
 let reply={...preview(),can_apply:false,gltf:undefined,error:'no material removal'};const f=setup(t,{api:(path,options)=>!options?graph():reply});await f.open();f.enter('Result feature name','made-part');await f.button('Preview solid').onclick();assert.equal(f.studio.solidPreview,null);assert.match(f.studio.message,/no material removal/);
 reply={...preview(),instance:'other'};await f.button('Preview solid').onclick();assert.equal(f.studio.solidPreview,null);assert.equal(f.studio.version,1);
});
test('Apply serializes the model mutation through reload and then closes the solid form',async t=>{
 let resolve,release;const f=setup(t,{api:(path,options)=>!options?graph():new Promise(r=>resolve=r)});await f.open();f.enter('Result feature name','made-part');
 f.studio.reload=()=>new Promise(r=>release=r);const pending=f.button('Apply verified solid').onclick();assert.equal(f.studio.modelPending,'Applying solid');assert.equal(f.studio.ensureDraft(),null);assert.equal(f.solids.open(),false);
 f.solids.close();assert.equal(f.panel.hidden,false,'pending Apply owns the form');resolve({applied:true,output:'made-part',state:{version:2}});await f.flush();assert.equal(f.studio.modelPending,'Applying solid');release();await pending;
 assert.equal(f.studio.modelPending,null);assert.equal(f.panel.hidden,true);assert.equal(f.studio.solidPreview,null);
});
test('a failed Apply unlocks the form for correction and retries; an accepted refresh failure cannot reapply',async t=>{
 let fail=true;const f=setup(t,{api:(path,options)=>{if(!options)return graph();if(fail)throw Error('Variant failed');return {applied:true,output:'made-part',state:{version:2}};}});await f.open();f.enter('Result feature name','made-part');await f.button('Apply verified solid').onclick();assert.equal(f.studio.modelPending,null);assert.equal(f.panel.hidden,false);
 fail=false;f.studio.reload=async()=>{throw Error('offline')};await f.button('Apply verified solid').onclick();assert.equal(f.panel.hidden,true);assert.match(f.studio.message,/applied.*refreshing/);assert.equal(f.studio.modelPending,null);
});
test('retained sketch history, local sketches and face/plane picking block solid authoring',t=>{
 const f=setup(t);f.edit(1);f.drawing.undo();assert.equal(f.solids.open(),false);f.drawing.discard(true);
 f.layer.local=true;assert.equal(f.solids.open(),false);f.layer.local=false;f.studio.picking=true;assert.equal(f.solids.open(),false);f.studio.picking=false;f.studio.isPlanePicking=()=>true;assert.equal(f.solids.open(),false);
});
test('existing extrusion edits preserve parameter depth and formula drivers',async t=>{
 const f=setup(t,{api:(path,options)=>!options?graph():{...preview(),output:'body'}});await f.open();await f.button('Edit extrusion body').onclick();assert.equal(f.input('Extrusion depth source').value,'parameter:depth');
 await f.button('Preview solid').onclick();const body=JSON.parse(f.calls.at(-1).body);assert.equal(body.action,'replace');assert.deepEqual(body.features[0].operation.extrude.direction,features()[1].operation.extrude.direction);
});

test('a late feature graph load cannot reopen a closed solid form',async t=>{
 let resolve;const f=setup(t,{api:()=>new Promise(r=>resolve=r)});assert(f.solids.open());f.solids.close();resolve(graph());await f.flush();assert.equal(f.panel.hidden,true);assert.equal(f.input('Solid operation'),undefined);assert.equal(f.studio.solidPreview,null);
});

test('a planar current output falls back to a solid target and edits preview the extrusion itself',async t=>{
 const f=setup(t,{api:(path,options)=>!options?{...graph(),output:'profile'}:{...preview(),output:'body'}});await f.open();
 const operation=f.input('Solid operation');operation.value='fuse';operation.onchange();assert.equal(f.input('Target solid').value,'body');
 await f.button('Edit extrusion body').onclick();await f.button('Preview solid').onclick();assert.equal(JSON.parse(f.calls.at(-1).body).output,'body');
});

test('queued value changes invalidate a pending preview without closing the corrected form',async t=>{
 let resolve;const f=setup(t,{api:(path,options)=>!options?graph():new Promise(r=>resolve=r)});await f.open();f.enter('Result feature name','made-part');const pending=f.button('Preview solid').onclick();f.enter('Extrusion depth (mm)','20');resolve(preview());await pending;assert.equal(f.studio.solidPreview,null);assert.equal(f.panel.hidden,false);
});


test('solid face picking Escape retains the form; field edits and close clear target overlays',async t=>{
 const source={version:1,accepted:true,source_view:true,scenes:[{kind:'solid',instance:'part',feature:'body',mesh:[],edges:[]}]};
 const f=setup(t,{api:(path,options)=>!options?graph():path==='/api/sketch-source-view'?source:preview()});await f.open();
 f.input('Extrusion extent').value='up_to_face';f.input('Extrusion extent').onchange();let pending=f.button('Pick limiting face in canvas').onclick();await f.flush();assert(f.solids.picker.active);assert.equal(f.studio.tool,'solid-limit-face');assert.equal(f.panel.hidden,false);
 f.studio.onKey({key:'Escape'});await pending;assert.equal(f.solids.picker.active,false);assert.equal(f.solids.picker.source,null);assert.equal(f.panel.hidden,false);assert.equal(f.studio.tool,'select');assert.match(all(f.panel).find(e=>e.role==='status').textContent,/canceled/);
 pending=f.button('Pick limiting face in canvas').onclick();await f.flush();f.enter('Direction X','1');await pending;assert.equal(f.solids.picker.source,null);assert.equal(f.panel.hidden,false);
 pending=f.button('Pick limiting face in canvas').onclick();await f.flush();f.solids.close();await pending;assert.equal(f.solids.picker.source,null);assert.equal(f.panel.hidden,true);
});

test('verified loft guides appear with the preview and clear on field edits, rejection and close',async t=>{
 const response={...preview(),annotations:{scenes:[{kind:'solid',instance:'part',feature:'made-part',annotations:[{anchors:[[0,0,0],[0,0,2]],detail:{measurement:'loft_end_tangent',end:'start',origin_mm:[0,0,0],derivative_mm:[0,0,6],strength_mm:6,handle_scale:1/3}}]}]}};let reject=false;
 const f=setup(t,{api:(path,options)=>!options?graph():reject?{...response,can_apply:false,gltf:undefined,error:'bad tangent'}:response});await f.open();f.enter('Result feature name','made-part');
 const context={matrix:[1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1],width:100,height:100,element:(tag,attrs)=>({tag,attrs})},shown=()=>f.studio.decorate(context).filter(n=>n.attrs?.['data-loft-tangent']);
 await f.button('Preview solid').onclick();assert(shown().length>0);f.enter('Extrusion depth (mm)','20');assert.equal(shown().length,0);
 await f.button('Preview solid').onclick();assert(shown().length>0);reject=true;await f.button('Preview solid').onclick();assert.equal(shown().length,0);reject=false;await f.button('Preview solid').onclick();f.solids.close();assert.equal(shown().length,0);
});

test('opening an accepted loft shows native guides without requiring a no-change preview',async t=>{
 const loft={id:'body',operation:{profile_loft:{profiles:['profile','upper'],holes:[],ruled:false,start_constraint:{normal_to_profile:true,magnitude:mm(6)}}}};
 const f=setup(t,{api:()=>({...graph(),features:[features()[0],{id:'upper',operation:{sketch_wire:{sketch:{}}}},loft]})});
 f.studio.scenes=[{kind:'solid',instance:'part',feature:'body',annotations:[{anchors:[[0,0,0],[0,0,2]],detail:{measurement:'loft_end_tangent',end:'start',origin_mm:[0,0,0],derivative_mm:[0,0,6],strength_mm:6,handle_scale:1/3}}]}];await f.open();await f.button('Edit loft body').onclick();
 const context={matrix:[1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1],width:100,height:100,element:(tag,attrs)=>({tag,attrs})};assert(f.studio.decorate(context).some(n=>n.attrs?.['data-loft-tangent']));assert.equal(f.studio.solidPreview,null);
 f.input('Start loft strength source').value='literal';f.input('Start loft strength source').onchange();assert(!f.studio.decorate(context).some(n=>n.attrs?.['data-loft-tangent']));
});
