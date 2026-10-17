import test from 'node:test';
import assert from 'node:assert/strict';
import {fixture} from './sketch-studio-test-fixture.mjs';
import {installPointPicking} from './sketch-studio-picks.mjs';
import {installStudioLinks} from './sketch-studio-links.mjs';
import {basePlane,lookAt} from './sketch-studio-plane.mjs';
import {viewProjection} from './viewer.mjs';
class Element {
 constructor(tag){this.tagName=tag;this.children=[];this.value='';this.listeners={};}
 append(...v){for(const n of v)n.parentElement=this;this.children.push(...v);}
 get selectedOptions(){return this.children.filter(c=>c.selected);}
 replaceChildren(...v){this.children=[];this.append(...v);}
 setAttribute(k,v){this[k]=v;}
 addEventListener(k,v){this.listeners[k]=v;}
}
const all=e=>[e,...(e.children??[]).filter(v=>v&&typeof v==='object').flatMap(all)];
const source=()=>({version:1,accepted:true,source_view:true,scenes:[{kind:'solid',instance:'part',feature:'plate',edges:[{edge:4,line:0}],lines:[[[-10,0,0],[10,0,0]]],mesh:[{face:7,points:[[-10,-10,0],[10,-10,0],[0,10,0]]}]}]});
const projected=()=>({version:1,instance:'part',projection:{id:'reference',input:'plate',kind:'line',edge:{all:true}}});
function setup(t,{api}={}){
 const f=fixture(t,{api:api??(path=>path==='/api/sketch-source-view'?source():projected())});
 Object.assign(f.baseline,{points:[{id:'A'},{id:'B'}],lines:[{id:'AB',start:'A',end:'B'}],profile:['AB']});
 f.studio.features=[{id:'plate',operation:{box:{}}},{id:'profile',operation:{sketch_face:{sketch:{}}}},{id:'dependent',operation:{extrude:{input:'profile'}}}];
 f.layer.frame=basePlane('xy');f.studio.sceneOf=()=>f.studio.scenes[0];f.studio.decorate=()=>[];
 f.studio.viewport=()=>({matrix:viewProjection(lookAt({target:[0,0,0],distance:.1,yaw:0,pitch:0},f.layer.frame,[0,0]),1),width:400,height:400});
 f.studio.rayAt=()=>({origin:[0,.1,0],direction:[0,-1,0]});
 f.document.body={children:[],append(...v){this.children.push(...v)}};f.document.createElement=tag=>new Element(tag);
 const previousDocument=globalThis.document,previousOption=globalThis.Option;globalThis.document=f.document;globalThis.Option=class extends Element{constructor(text,value){super('option');this.textContent=text;this.value=value;}};
 t.after(()=>{globalThis.document=previousDocument;globalThis.Option=previousOption;});
 const picks=installPointPicking(f.studio,f.document);
 const links=installStudioLinks(f.studio,f.document),panel=f.document.body.children.at(-1);
 return {...f,picks,links,panel,input:label=>all(panel).find(e=>e['aria-label']===label),button:label=>all(panel).find(e=>e.textContent===label),click:()=>links.click({button:0,clientX:200,clientY:200})};
}
test('accepted edge picking is read-only until one isolated link is staged with Undo/Redo',async t=>{
 const f=setup(t),original=structuredClone(f.baseline);assert(f.links.open());assert.equal(f.studio.tool,'source-link');
 assert.deepEqual(f.input('Sketch link source').children.map(o=>o.value),['plate']);
 await f.button('Project picked edge').onclick();assert(f.links.source);assert.equal(f.studio.draft.undo.length,0);
 const nodes=f.studio.decorate({...f.studio.viewport(),element:(tag,attrs)=>({tag,attrs})});assert.equal(nodes[0].attrs['data-source-edge'],4);
 await f.click();assert.deepEqual(f.studio.draft.definition.projections,[projected().projection]);assert.deepEqual(f.baseline,original);assert.equal(f.studio.draft.undo.length,1);assert.equal(f.links.source,null);
 assert.equal(f.calls.at(-1).path,'/api/sketch-edge-pick');assert.equal(JSON.parse(f.calls.at(-1).body).edge,4);
 f.drawing.undo();assert.equal(f.studio.draft.definition.projections,undefined);f.drawing.redo();assert.equal(f.studio.draft.definition.projections.length,1);f.drawing.discard(true);
});
test('picked face support preserves coordinates and offset drivers in one draft edit',async t=>{
 const pick={version:1,instance:'part',face_support:{input:'plate',face:{all:true}},x_axis:{literal:'x'},y_axis:{literal:'y'}};
 const f=setup(t,{api:path=>path==='/api/sketch-source-view'?source():pick});f.baseline.face_support={input:'old',offset:{parameter:'gap'}};
 f.links.open();await f.button('Reattach to picked face').onclick();await f.click();
 assert.equal(JSON.parse(f.calls.at(-1).body).face,7);assert.deepEqual(f.studio.draft.definition.points,f.baseline.points);assert.deepEqual(f.studio.draft.definition.face_support.offset,{parameter:'gap'});assert.equal(f.studio.draft.definition.face_support.input,'plate');assert.equal(f.studio.draft.undo.length,1);f.drawing.discard(true);
});
for(const action of ['close','edit','history','layer','apply','tool','escape'])test(`late source geometry cannot reopen or edit after ${action}`,async t=>{
 let resolve;const f=setup(t,{api:()=>new Promise(r=>resolve=r)});f.edit(1);f.studio.draft.previewing=false;f.links.open();const pending=f.button('Project picked edge').onclick();
 if(action==='close')f.links.close();if(action==='edit')f.edit(2);if(action==='history')f.drawing.undo();if(action==='layer')f.studio.onActivate({feature:'other'});
 if(action==='tool')f.studio.setTool('line');if(action==='escape')f.studio.onKey({key:'Escape'});
 if(action==='apply'){f.studio.api=async()=>({applied:true});await f.drawing.apply();}
 const d=f.studio.draft,definition=d?.definition;resolve(source());await pending;
 assert.equal(f.links.source,null);assert.equal(f.studio.draft,d);assert.equal(d?.definition,definition);f.drawing.discard(true);
});
for(const action of ['close','edit','history','layer','apply'])test(`late semantic edge pick cannot stage after ${action}`,async t=>{
 let resolve;const f=setup(t,{api:path=>path==='/api/sketch-source-view'?source():new Promise(r=>resolve=r)});
 f.edit(1);f.studio.draft.previewing=false;f.links.open();await f.button('Project picked edge').onclick();const pending=f.click();
 if(action==='close')f.links.close();if(action==='edit')f.edit(2);if(action==='history')f.drawing.undo();if(action==='layer')f.studio.onActivate({feature:'other'});
 if(action==='apply'){f.studio.api=async()=>({applied:true});await f.drawing.apply();}
 const d=f.studio.draft,definition=d?.definition;resolve(projected());await pending;assert.equal(f.studio.draft,d);assert.equal(d?.definition,definition);f.drawing.discard(true);
});
test('source identity mismatches and duplicate geometry names never stage projections',async t=>{
 let wrong=true;const f=setup(t,{api:path=>path==='/api/sketch-source-view'?{...source(),accepted:!wrong}:projected()});
 f.links.open();await f.button('Project picked edge').onclick();assert.equal(f.links.source,null);assert.match(f.studio.message,/does not match/);
 wrong=false;f.drawing.discard(true);f.baseline.points.push({id:'reference'});f.links.close();f.links.open();f.input('Projection name').value='reference';f.input('Projection name').oninput();await f.button('Project picked edge').onclick();await f.click();
 assert.equal(f.studio.draft.undo.length,0);assert.match(f.studio.message,/already exists/);f.drawing.discard(true);
});
test('construction projections require explicit boundaries and support rejected-draft repair',async t=>{
 const f=setup(t);f.baseline.profile=[];f.links.open();await f.button('Project picked edge').onclick();await f.click();assert.match(f.studio.message,/explicit/);assert.equal(f.studio.draft.undo.length,0);f.drawing.discard(true);
 f.baseline.projections=[projected().projection];f.studio.ensureDraft();f.studio.draft.error='invalid projection';assert(f.links.open());await f.button('Remove projection reference').onclick();assert.deepEqual(f.studio.draft.definition.projections,[]);assert.equal(f.studio.draft.undo.length,1);f.drawing.discard(true);
});
test('projection refinement and exact detachment stage independently through shared history',async t=>{
 const f=setup(t,{api:()=>({version:1,can_apply:true,annotations:{scenes:[{kind:'sketch',instance:'part',feature:'profile'}]},definition:{...f.studio.draft.definition,projections:[],splines:[{id:'reference',points:['A','B']}]}})});
 f.baseline.projections=[{...projected().projection,kind:'spline'}];f.links.open();await f.button('Insert projected knot · reference').onclick();
 assert.equal(f.studio.draft.definition.projections[0].refinements[0].insert_knot.fraction,.5);assert.equal(f.studio.draft.undo.length,1);
 f.studio.draft.previewing=false;f.links.open();await f.button('Detach projected spline reference').onclick();assert.equal(f.studio.draft.definition.splines[0].id,'reference');assert.equal(f.studio.draft.undo.length,2);f.drawing.undo();assert.equal(f.studio.draft.definition.projections[0].kind,'spline');f.drawing.discard(true);
});
test('new sketches, model mutations and competing plane picks guard source-link sessions',t=>{
 const f=setup(t);f.layer.local=true;f.studio.ensureDraft();f.studio.draft.local=true;assert.equal(f.links.open(),false);f.studio.draft.local=false;
 f.studio.modelPending=true;assert.equal(f.links.open(),false);f.studio.modelPending=false;f.studio.picking=true;assert.equal(f.links.open(),false);f.studio.picking=false;
 f.studio.isPlanePicking=()=>true;assert.equal(f.links.open(),false);f.drawing.discard(true);
});

test('stale refinement callbacks and late exact detach responses cannot overwrite a newer draft',async t=>{
 let resolve;const f=setup(t,{api:()=>new Promise(r=>resolve=r)});f.baseline.projections=[{...projected().projection,kind:'spline'}];
 f.links.open();const refine=f.button('Insert projected knot · reference').onclick;
 const pending=f.button('Detach projected spline reference').onclick();f.edit(1);const next=f.studio.draft.definition;
 resolve({version:1,instance:'part',definition:{id:'old'}});await pending;await refine();
 assert.equal(f.studio.draft.definition,next);assert.equal(f.studio.draft.undo.length,1);assert.equal(f.studio.draft.definition.projections[0].refinements,undefined);f.drawing.discard(true);
});
test('edge responses with a mismatched source cannot stage and shift clicks leave source picks alone',async t=>{
 const f=setup(t,{api:path=>path==='/api/sketch-source-view'?source():{...projected(),projection:{...projected().projection,input:'other'}}});
 f.links.open();await f.button('Project picked edge').onclick();await f.links.click({button:0,shiftKey:true,clientX:200,clientY:200});assert.equal(f.calls.length,1);
 await f.click();assert.match(f.studio.message,/does not match/);assert.equal(f.studio.draft.undo.length,0);f.drawing.discard(true);
});

test('opening curve controls cancels a pending source view without disturbing the new panel',async t=>{
 let resolve;const f=setup(t,{api:()=>new Promise(r=>resolve=r)});f.links.open();const pending=f.button('Project picked edge').onclick();
 assert(f.picks.open('profile'));resolve(source());await pending;
 assert.equal(f.links.source,null);assert.equal(f.panel.hidden,true);assert.equal(f.document.body.children[0].hidden,false);assert.equal(f.studio.draft.undo.length,0);f.drawing.discard(true);
});
test('opening source links invalidates older pattern form callbacks',t=>{
 const f=setup(t);f.baseline.lines.push({id:'axis',start:'A',end:'B'});f.picks.open('pattern');const form=f.document.body.children[0],input=name=>all(form).find(e=>e['aria-label']===name);
 input('Pattern name').value='row';const old=all(form).find(e=>e.textContent==='Remove sketch pattern');assert.equal(old,undefined);
 const sources=input('Pattern source curves');sources.children.find(o=>o.value==='AB').selected=true;
 input('Pattern direction').value='axis';
 // An otherwise valid old form cannot mutate the newer source-link session.
 const submit=all(form).find(e=>e.textContent==='Add sketch pattern').onclick;
 f.links.open();const definition=f.studio.draft.definition;submit();assert.equal(f.studio.draft.definition,definition);assert.equal(f.studio.draft.undo.length,0);assert.equal(f.panel.hidden,false);f.drawing.discard(true);
});
