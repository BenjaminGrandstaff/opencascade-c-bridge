import test from 'node:test';
import assert from 'node:assert/strict';
import {fixture} from './sketch-studio-test-fixture.mjs';
import {installPointPicking} from './sketch-studio-picks.mjs';
import {basePlane, lookAt, projectLocal} from './sketch-studio-plane.mjs';
import {viewProjection} from './viewer.mjs';

class Element {
 constructor(tag) {this.tag=tag;this.children=[];this.value='';this.style={};}
 append(...nodes) {for(const n of nodes)n.parentElement=this;this.children.push(...nodes);}
 setAttribute(key,value) {this[key]=value;}
}
const all = root => [root,...(root.children??[]).filter(n=>n&&typeof n==='object').flatMap(all)];
const mm = value => ({literal:{value,dimension:'length',unit:'millimeter'}});
function setup(t,{sample=false,api}={}) {
 const f=fixture(t,{api});
 Object.assign(f.baseline,{points:[['A',-10,0],['B',10,0],['C',0,-10],['D',0,10]].map(([id,x,y])=>({id,x:mm(x),y:mm(y),fixed:true})),
  lines:[{id:'horizontal',start:'A',end:'B'},{id:'vertical',start:'C',end:'D'}],splines:sample?[{id:'spline',points:['A','C','B']}]:[]});
 const scene=f.studio.scenes[0];scene.points={A:[-10,0,0],B:[10,0,0],C:[0,-10,0],D:[0,10,0]};
 scene.entities=sample?[{id:'spline',points:[[-10,0,0],[0,-10,0],[10,0,0]]}]:[
  {id:'horizontal',points:[[-10,0,0],[10,0,0]]},{id:'vertical',points:[[0,-10,0],[0,10,0]]}];
 f.layer.frame=basePlane('xy');
 const matrix=viewProjection(lookAt({target:[0,0,0],distance:.06,yaw:0,pitch:0},f.layer.frame,[0,0]),1);
 f.studio.viewport=()=>({matrix,width:400,height:400});f.studio.toScreen=xy=>projectLocal(matrix,f.layer.frame,xy,400,400);
 f.studio.sceneOf=()=>scene;let local;
 f.studio.localAt=()=>local;
 f.document.body=new Element('body');f.document.createElement=tag=>new Element(tag);
 const previousDocument=globalThis.document,previousOption=globalThis.Option;
 globalThis.document=f.document;globalThis.Option=class extends Element{constructor(text,value){super('option');this.textContent=text;this.value=value;}};
 t.after(()=>{globalThis.document=previousDocument;globalThis.Option=previousOption;});
 const picks=installPointPicking(f.studio,f.document),panel=()=>f.document.body.children[0];
 const button=text=>all(panel()).find(n=>n.textContent===text||n.children?.includes(text));
 const input=label=>all(panel()).find(n=>n['aria-label']===label);
 async function click(xy) {local=xy;const s=f.studio.toScreen(xy),e={button:0,clientX:s[0],clientY:s[1]};const tool=f.studio.tools.get(f.studio.tool);assert(tool.down(e));await tool.up(e);await f.flush();}
 return {...f,picks,panel,pickButton:button,input,click};
}
const response = value => ({version:1,instance:'part',feature:'profile',spline:'spline',...value});
const crossing = points => response({complete:true,first:'horizontal',second:'vertical',coordinate_system:'sketch-local millimeters',points});

test('studio intersection picking queries the draft and stages one undoable named point',async t=>{
 const f=setup(t,{api:()=>crossing([[0,0]])});assert(f.picks.open('crossing'));
 f.input('New crossing point name').value='joint';f.pickButton('Start crossing pick').onclick();
 await f.click([-5,0]);await f.click([0,5]);assert.equal(f.calls.length,1);
 assert.equal(f.calls[0].path,'/api/sketch-crossing-candidates');assert.equal(JSON.parse(f.calls[0].body).version,1);
 await f.click([.1,.1]);assert.equal(f.studio.draft.undo.length,1);
 assert.equal(f.studio.draft.definition.constraints.at(-1).crossing_point.point,'joint');assert.equal(f.panel().hidden,true);
 f.drawing.undo();assert.equal(f.studio.draft.definition.points.length,4);f.drawing.redo();assert.equal(f.studio.draft.definition.points.at(-1).id,'joint');f.drawing.discard(true);
});

test('ambiguous native crossings do not stage until the window isolates one',async t=>{
 const f=setup(t,{api:()=>crossing([[-1,0],[1,0]])});f.picks.open('crossing');f.pickButton('Start crossing pick').onclick();
 await f.click([-5,0]);await f.click([0,5]);await f.click([0,0]);assert.equal(f.studio.draft.undo.length,0);
 f.input('Picked crossing search radius (mm)').value='.2';f.input('Picked crossing search radius (mm)').oninput();
 await f.click([1,0]);assert.equal(f.studio.draft.undo.length,1);assert.equal(f.calls.length,1);f.drawing.discard(true);
});

test('spline candidates require review and exact fraction updates before staging',async t=>{
 const f=setup(t,{sample:true,api:(path,options)=>{const q=JSON.parse(options.body);return response({fraction:q.fraction??.3,point:[2,-5]});}});
 f.picks.open('sample');f.input('New spline sample name').value='sample';f.pickButton('Start spline sample pick').onclick();
 await f.click([-5,-5]);assert.equal(f.calls[0].path,'/api/sketch-spline-sample');assert.equal(f.studio.draft.undo.length,0);
 f.pickButton('Sample midpoint (0.5)').onclick();await f.flush();f.pickButton('Stage spline sample').onclick();
 assert.equal(f.studio.draft.definition.constraints.at(-1).spline_point.fraction.literal.value,.5);assert.equal(f.studio.draft.undo.length,1);f.drawing.discard(true);
});

for(const action of ['close','edit','history','layer','apply']) {
 test(`a delayed sample reply cannot stage after ${action}`,async t=>{
  let resolve;const f=setup(t,{sample:true,api:()=>new Promise(r=>resolve=r)});
  if(action==='history'||action==='apply') {f.studio.ensureDraft();f.studio.changeDraft({...f.studio.draft.definition,revision:1});f.studio.draft.previewing=false;}
  f.picks.open('sample');f.pickButton('Start spline sample pick').onclick();const pending=f.click([-5,-5]);await f.flush();
  if(action==='close')f.picks.close();
  if(action==='edit')f.studio.changeDraft({...f.studio.draft.definition,revision:2});
  if(action==='history')f.drawing.undo();
  if(action==='layer')f.studio.onActivate({feature:'other'});
  if(action==='apply'){f.studio.api=async()=>({applied:true});await f.drawing.apply();}
  const d=f.studio.draft,snapshot=d?.definition;resolve(response({fraction:.3,point:[2,-5]}));await pending;
  assert.equal(f.studio.draft,d);assert.equal(d?.definition,snapshot);assert.equal(f.picks.active,false);f.drawing.discard(true);
 });
}

test('Escape, tool changes and pointer cancellation close picking without editing',t=>{
 const f=setup(t);for(const cancel of [()=>f.studio.onKey({key:'Escape'}),()=>f.studio.setTool('line'),()=>f.studio.tools.get('crossing').cancel()]){
  f.picks.open('crossing');f.pickButton('Start crossing pick').onclick();cancel();assert.equal(f.picks.active,false);assert.equal(f.panel().hidden,true);assert.equal(f.studio.draft.undo.length,0);
 }f.drawing.discard(true);
});

test('new, previewing and invalid sketches refuse native picking',t=>{
 const f=setup(t);f.layer.local=true;assert.equal(f.picks.open('crossing'),false);f.layer.local=false;f.drawing.discard(true);
 f.studio.ensureDraft();f.studio.draft.previewing=true;assert.equal(f.picks.open('sample'),false);
 f.studio.draft.previewing=false;f.studio.draft.error='invalid';assert.equal(f.picks.open('sample'),false);f.drawing.discard(true);
});

test('mismatched native response identities cannot make a candidate stageable',async t=>{
 const f=setup(t,{sample:true,api:()=>({...response({fraction:.3,point:[2,-5]}),version:2})});
 f.picks.open('sample');f.pickButton('Start spline sample pick').onclick();await f.click([-5,-5]);
 assert.equal(f.pickButton('Stage spline sample').disabled,true);assert.equal(f.studio.draft.undo.length,0);f.picks.close();f.drawing.discard(true);
});

test('other face and plane picking modes must finish before opening a sketch pick',t=>{
 const f=setup(t);f.studio.picking=true;assert.equal(f.picks.open('crossing'),false);
 f.studio.picking=false;f.studio.isPlanePicking=()=>true;assert.equal(f.picks.open('sample'),false);
 assert.equal(f.studio.draft,undefined);
});
