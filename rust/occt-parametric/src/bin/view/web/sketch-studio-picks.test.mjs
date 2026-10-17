import test from 'node:test';
import assert from 'node:assert/strict';
import {captureGeneratedPoints,bindGeneratedPoints} from './generated-points.mjs';
import {fixture} from './sketch-studio-test-fixture.mjs';
import {installPointPicking} from './sketch-studio-picks.mjs';
import {basePlane, lookAt, projectLocal} from './sketch-studio-plane.mjs';
import {viewProjection} from './viewer.mjs';

class Element {
 constructor(tag) {this.tag=tag;this.children=[];this.value='';this.style={};this.listeners={};}
 append(...nodes) {for(const n of nodes)n.parentElement=this;this.children.push(...nodes);}
 get selectedOptions() {return this.children.filter(c=>c.selected);}
 setAttribute(key,value) {this[key]=value;}
 addEventListener(type,handler) {this.listeners[type]=handler;}
 replaceChildren(...nodes) {this.children=[];this.append(...nodes);}
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

test('weighted canvas sources stage one link and support draft Undo/Redo',async t=>{
 const f=setup(t);f.picks.open('weighted');f.input('New weighted point name').value='average';
 f.pickButton('Start weighted point pick').onclick();await f.click([-10,0]);await f.click([10,0]);
 assert.equal(f.pickButton('Stage weighted point').disabled,false);assert.equal(f.studio.draft.undo.length,0);
 f.pickButton('Stage weighted point').onclick();assert.equal(f.studio.draft.undo.length,1);
 const link=f.studio.draft.definition.constraints.at(-1).weighted_point;
 assert.equal(link.point,'average');assert.deepEqual(link.terms.map(t=>t.point),['A','B']);
 f.drawing.undo();assert.equal(f.studio.draft.definition.points.length,4);f.drawing.redo();assert.equal(f.studio.draft.definition.points.at(-1).id,'average');
 assert.equal(f.calls.length,0);f.drawing.discard(true);
});

test('weighted picks reject duplicate and empty canvas hits without changing the draft',async t=>{
 const f=setup(t);f.picks.open('weighted');f.pickButton('Start weighted point pick').onclick();
 await f.click([-10,0]);await f.click([-10,0]);assert.match(f.studio.message,/already included/);
 await f.click([7,7]);assert.match(f.studio.message,/known source point/);
 f.pickButton('Stage weighted point').onclick();assert.equal(f.studio.draft.definition.constraints.at(-1).weighted_point.terms.length,1);f.drawing.discard(true);
});

test('relinking an existing weighted target preserves its parameter and formula drivers',async t=>{
 const f=setup(t);f.baseline.constraints=[{weighted_point:{point:'D',terms:[{point:'A',weight:{parameter:'blend'}},{point:'B',weight:{add:[{literal:{value:1,dimension:'scalar',unit:null}},{literal:{value:2,dimension:'scalar',unit:null}}]}}]}}];
 f.picks.open('weighted');f.input('Weighted point target').value='D';f.input('Weighted point target').onchange();
 f.pickButton('Start weighted point pick').onclick();f.pickButton('Stage weighted point').onclick();
 const next=f.studio.draft.definition;
 assert.deepEqual(next.constraints[0].weighted_point.terms,f.baseline.constraints[0].weighted_point.terms);
 assert.equal(next.points.find(p=>p.id==='D').fixed,false);assert.equal(next.constraints.length,1);f.drawing.discard(true);
});

test('weighted point staging is canceled by an intervening draft edit',async t=>{
 const f=setup(t);f.picks.open('weighted');f.pickButton('Start weighted point pick').onclick();await f.click([-10,0]);
 const stage=f.pickButton('Stage weighted point').onclick;
 f.studio.changeDraft({...f.studio.draft.definition,revision:1});const snapshot=f.studio.draft.definition;stage();
 assert.equal(f.studio.draft.definition,snapshot);assert.equal(f.studio.draft.undo.length,1);f.drawing.discard(true);
});

function choose(f,label,value) { const input=f.input(label);input.value=value;input.onchange(); }
function enter(f,label,value) { const input=f.input(label);input.value=value;input.oninput?.();input.onchange?.(); }

test('profile canvas picks fill fields without editing; submit creates one isolated Undo entry',async t=>{
 const f=setup(t);const original=structuredClone(f.baseline);f.picks.open('profile');choose(f,'Profile operation','fillet');
 f.pickButton('Pick corner point on canvas').onclick();await f.click([-10,0]);
 assert.equal(f.input('Corner point').value,'A');assert.equal(f.studio.draft.undo.length,0);
 enter(f,'Fillet radius (mm)','2.5');f.pickButton('Add profile operation').onclick();
 assert.deepEqual(f.baseline,original);assert.deepEqual(f.studio.draft.definition.profile_operations,[{fillet:{point:'A',radius:mm(2.5)}}]);assert.equal(f.studio.draft.undo.length,1);
 f.drawing.undo();assert.equal(f.studio.draft.definition.profile_operations,undefined);f.drawing.redo();assert.equal(f.studio.draft.definition.profile_operations.length,1);f.drawing.discard(true);
});

test('trim bounds can be picked as named points on the current plane',async t=>{
 const f=setup(t);f.picks.open('profile');choose(f,'From bound','point');f.pickButton('Pick from point on canvas').onclick();await f.click([-10,0]);
 choose(f,'To bound','point');f.pickButton('Pick to point on canvas').onclick();await f.click([10,0]);f.pickButton('Add profile operation').onclick();
 assert.deepEqual(f.studio.draft.definition.profile_operations[0],{trim_between:{entity:'horizontal',from:{point:{point:'A'}},to:{point:{point:'B'}}}});f.drawing.discard(true);
});

test('splitting replaces explicit boundary references atomically with named pieces',async t=>{
 const f=setup(t);f.baseline.profile=['horizontal'];f.baseline.points.push({id:'P',x:mm(0),y:mm(0),fixed:true});f.studio.scenes[0].points.P=[0,0,0];
 f.picks.open('profile');choose(f,'Profile operation','split');choose(f,'Split bound','point');f.pickButton('Pick split point on canvas').onclick();await f.click([0,0]);
 enter(f,'Before piece name','first');enter(f,'After piece name','last');f.pickButton('Add profile operation').onclick();
 assert.deepEqual(f.studio.draft.definition.profile,['first','last']);assert.deepEqual(f.baseline.profile,['horizontal']);
 assert.equal(f.studio.draft.definition.curve_operations[0].split.at.point.point,'P');assert.equal(f.studio.draft.undo.length,1);f.drawing.discard(true);
});

test('extension target picking uses curves rather than endpoint point hits',async t=>{
 const f=setup(t);f.picks.open('profile');choose(f,'Profile operation','extend_to');
 f.pickButton('Pick extension target on canvas').onclick();await f.click([0,10]);f.pickButton('Add profile operation').onclick();
 assert.deepEqual(f.studio.draft.definition.curve_operations[0],{extend_to:{entity:'horizontal',end:'end',target:'vertical',limit:mm(10)}});f.drawing.discard(true);
});

for(const action of ['edit','history','layer','apply']) {
 test(`old curve form callbacks cannot commit after ${action}`,async t=>{
  const f=setup(t);f.studio.ensureDraft();f.studio.changeDraft({...f.studio.draft.definition,revision:1});f.studio.draft.previewing=false;
  f.picks.open('profile');choose(f,'Profile operation','offset');const submit=f.pickButton('Add profile operation').onclick;
  if(action==='edit')f.studio.changeDraft({...f.studio.draft.definition,revision:2});
  if(action==='history')f.drawing.undo();
  if(action==='layer')f.studio.onActivate({feature:'other'});
  if(action==='apply'){f.studio.api=async()=>({applied:true});await f.drawing.apply();}
  const d=f.studio.draft,snapshot=d?.definition;submit();assert.equal(f.studio.draft,d);assert.equal(d?.definition,snapshot);assert.match(f.studio.message,/sketch changed/);f.drawing.discard(true);
 });
}

test('rejected profile operations can be removed through the studio editor',t=>{
 const f=setup(t);f.studio.ensureDraft();f.studio.changeDraft({...f.studio.draft.definition,profile_operations:[{offset:{distance:mm(999),join:'arc'}}]});
 f.studio.draft.previewing=false;f.studio.draft.error='invalid boundary';assert(f.picks.open('profile'));
 f.pickButton('Remove profile operation 1').onclick();assert.deepEqual(f.studio.draft.definition.profile_operations,[]);assert.equal(f.studio.draft.error,null);f.drawing.discard(true);
});

test('profile operations can be staged on an already previewed new local sketch',t=>{
 const f=setup(t);f.studio.ensureDraft();f.studio.draft.local=true;f.layer.local=true;assert(f.picks.open('profile'));
 choose(f,'Profile operation','offset');f.pickButton('Add profile operation').onclick();assert.equal(f.studio.draft.definition.profile_operations.length,1);f.drawing.discard(true);
});

test('split-piece display aliases resolve to native piece names for operation picking',async t=>{
 const f=setup(t);f.baseline.curve_operations=[{split:{entity:'horizontal',at:{point:{point:'C'}},before:'piece',after:'other'}}];
 f.studio.scenes[0].entities.push({id:'edited:piece',curve_id:'piece',points:[[-10,0,0],[0,0,0]]});
 f.picks.open('profile');choose(f,'Profile operation','extend_to');f.pickButton('Pick source curve on canvas').onclick();await f.click([-5,0]);
 assert.equal(f.input('Source curve').value,'piece');assert.equal(f.studio.draft.undo.length,0);f.picks.close();f.drawing.discard(true);
});

test('material region seed picking stages an isolated operation with Undo',async t=>{
 const f=setup(t);f.picks.open('profile');choose(f,'Profile operation','offset');choose(f,'Boundary target','region');
 f.pickButton('Pick region seed on canvas').onclick();await f.click([0,10]);
 assert.equal(f.input('Region seed').value,'D');assert.equal(f.studio.draft.undo.length,0);
 enter(f,'Offset distance (mm)','-2');f.pickButton('Add profile operation').onclick();
 assert.deepEqual(f.studio.draft.definition.region_edits,[{seed:'D',operations:[{offset:{distance:mm(-2),join:'arc'}}]}]);
 assert.equal(f.baseline.region_edits,undefined);assert.equal(f.studio.draft.undo.length,1);
 f.drawing.undo();assert.equal(f.studio.draft.definition.region_edits,undefined);f.drawing.discard(true);
});

test('discovery seed picks fill selection without editing until explicitly updated',async t=>{
 const f=setup(t);f.baseline.region_discovery={entities:['horizontal','vertical'],seeds:[],merge_adjacent:false};
 f.picks.open('profile');f.pickButton('Pick discovery seed on canvas').onclick();await f.click([0,-10]);
 assert.equal(f.input('Selected region seeds (comma separated)').value,'C');assert.equal(f.studio.draft.undo.length,0);
 f.pickButton('Update region selection').onclick();
 assert.deepEqual(f.studio.draft.definition.region_discovery.seeds,['C']);assert.deepEqual(f.baseline.region_discovery.seeds,[]);
 assert.equal(f.studio.draft.undo.length,1);f.drawing.discard(true);
});

test('linear pattern canvas sources and direction stage one immutable draft with Undo/Redo',async t=>{
 const f=setup(t);const original=structuredClone(f.baseline);f.picks.open('pattern');enter(f,'Pattern name','row');
 f.pickButton('Pick pattern sources on canvas').onclick();await f.click([-5,0]);await f.click([-5,0]);
 assert.deepEqual(f.input('Pattern source curves').selectedOptions.map(o=>o.value),['horizontal']);assert.equal(f.studio.draft.undo.length,0);
 f.pickButton('Pick pattern direction on canvas').onclick();await f.click([0,10]);assert.equal(f.input('Pattern direction').value,'vertical');
 enter(f,'Pattern count (including source)','3');enter(f,'Pattern spacing (mm)','-5');f.pickButton('Add sketch pattern').onclick();
 assert.deepEqual(f.studio.draft.definition.patterns,[{linear:{id:'row',entities:['horizontal'],direction:'vertical',count:3,spacing:mm(-5)}}]);
 assert.deepEqual(f.baseline,original);assert.equal(f.studio.draft.undo.length,1);
 f.drawing.undo();assert.equal(f.studio.draft.definition.patterns,undefined);f.drawing.redo();assert.equal(f.studio.draft.definition.patterns[0].linear.id,'row');f.drawing.discard(true);
});

test('circular centers and mirror shared axis points use point picking',async t=>{
 const f=setup(t);f.picks.open('pattern');choose(f,'Pattern type','circular');enter(f,'Pattern name','ring');
 f.pickButton('Pick pattern sources on canvas').onclick();await f.click([-5,0]);f.pickButton('Pick pattern center on canvas').onclick();await f.click([0,10]);
 assert.equal(f.input('Pattern center').value,'D');f.pickButton('Add sketch pattern').onclick();
 assert.deepEqual(f.studio.draft.definition.patterns[0],{circular:{id:'ring',entities:['horizontal'],center:'D',count:2}});f.drawing.discard(true);
 f.picks.open('pattern');choose(f,'Pattern type','mirror');enter(f,'Pattern name','mirror');
 f.pickButton('Pick pattern sources on canvas').onclick();await f.click([-5,0]);f.pickButton('Pick pattern axis on canvas').onclick();await f.click([0,5]);
 f.pickButton('Pick shared axis points on canvas').onclick();await f.click([0,-10]);await f.click([0,10]);f.pickButton('Add sketch pattern').onclick();
 assert.deepEqual(f.studio.draft.definition.patterns[0],{mirror:{id:'mirror',entities:['horizontal'],axis:'vertical',on_axis:['C','D']}});f.drawing.discard(true);
});

test('pattern form rejects missing sources and self references without staging history',async t=>{
 const f=setup(t);f.picks.open('pattern');enter(f,'Pattern name','row');f.pickButton('Add sketch pattern').onclick();assert.match(f.studio.message,/known source/);
 f.pickButton('Pick pattern sources on canvas').onclick();await f.click([7,7]);assert.match(f.studio.message,/known pattern sources/);
 await f.click([-5,0]);f.pickButton('Add sketch pattern').onclick();assert.match(f.studio.message,/own reference/);
 assert.equal(f.studio.draft.undo.length,0);assert.equal(f.studio.draft.definition.patterns,undefined);f.drawing.discard(true);
});

test('editing a pattern excludes itself and later copies but accepts earlier generated curves',async t=>{
 const f=setup(t);f.baseline.patterns=[{linear:{id:'earlier',entities:['horizontal'],direction:'vertical',count:2,spacing:mm(2)}},{linear:{id:'later',entities:['horizontal'],direction:'vertical',count:2,spacing:mm(4)}}];
 bindGeneratedPoints(f.baseline,captureGeneratedPoints(f.baseline,[],{lines:['earlier:1:horizontal','later:1:horizontal']}));
 f.studio.scenes[0].entities.push({id:'earlier:1:horizontal',points:[[-10,2,0],[10,2,0]]},{id:'later:1:horizontal',points:[[-10,4,0],[10,4,0]]});
 f.picks.open('pattern');choose(f,'Pattern to edit','later');f.pickButton('Pick pattern sources on canvas').onclick();await f.click([-5,4]);
 assert.match(f.studio.message,/before this pattern/);await f.click([-5,2]);f.pickButton('Update sketch pattern').onclick();
 assert.deepEqual(f.studio.draft.definition.patterns[1].linear.entities,['horizontal','earlier:1:horizontal']);assert.equal(f.studio.draft.undo.length,1);f.drawing.discard(true);
});

test('existing pattern updates retain formulas and removal is one repairable draft edit',t=>{
 const f=setup(t),formula={multiply:[{parameter:'pitch'},{literal:{value:2,dimension:'scalar',unit:null}}]};
 f.baseline.patterns=[{linear:{id:'row',entities:['horizontal'],direction:'vertical',count:2,spacing:formula}}];
 f.picks.open('pattern');choose(f,'Pattern to edit','row');enter(f,'Pattern count (including source)','4');f.pickButton('Update sketch pattern').onclick();
 assert.deepEqual(f.studio.draft.definition.patterns[0].linear.spacing,formula);assert.equal(f.baseline.patterns[0].linear.count,2);
 f.studio.draft.previewing=false;f.studio.draft.error='invalid dependent boundary';f.picks.open('pattern');choose(f,'Pattern to edit','row');f.pickButton('Remove sketch pattern').onclick();
 assert.deepEqual(f.studio.draft.definition.patterns,[]);assert.equal(f.studio.draft.undo.length,2);assert.equal(f.studio.draft.error,null);f.drawing.discard(true);
});

for(const action of ['close','edit','history','layer','apply'])test(`old pattern callbacks cannot stage after ${action}`,async t=>{
 const f=setup(t);f.studio.ensureDraft();f.studio.changeDraft({...f.studio.draft.definition,revision:1});f.studio.draft.previewing=false;
 f.picks.open('pattern');enter(f,'Pattern name','row');f.pickButton('Pick pattern sources on canvas').onclick();await f.click([-5,0]);
 f.pickButton('Pick pattern direction on canvas').onclick();await f.click([0,5]);const submit=f.pickButton('Add sketch pattern').onclick;
 if(action==='close')f.picks.close();if(action==='edit')f.studio.changeDraft({...f.studio.draft.definition,revision:2});
 if(action==='history')f.drawing.undo();if(action==='layer')f.studio.onActivate({feature:'other'});
 if(action==='apply'){f.studio.api=async()=>({applied:true});await f.drawing.apply();}
 const d=f.studio.draft,snapshot=d?.definition;submit();assert.equal(f.studio.draft,d);assert.equal(d?.definition,snapshot);assert.match(f.studio.message,/sketch changed/);f.drawing.discard(true);
});

test('pattern editing supports previewed local sketches and is locked while Preview or Apply is pending',t=>{
 const f=setup(t);f.studio.ensureDraft();f.studio.draft.local=true;f.layer.local=true;assert(f.picks.open('pattern'));f.picks.close();
 f.studio.draft.previewing=true;assert.equal(f.picks.open('pattern'),false);f.studio.draft.previewing=false;
 f.studio.modelPending=true;assert.equal(f.picks.open('pattern'),false);f.studio.modelPending=false;f.drawing.discard(true);
});

test('boundary picks retain click order and ignore repeats without editing until one atomic stage',async t=>{
 const f=setup(t);f.baseline.profile=['horizontal'];f.picks.open('boundary');
 f.pickButton('Clear Outer boundary').onclick();f.pickButton('Pick Outer boundary on canvas').onclick();await f.click([0,5]);await f.click([-5,0]);await f.click([0,5]);
 assert.equal(f.input('Outer boundary').value,'vertical, horizontal');assert.equal(f.studio.draft.undo.length,0);
 f.pickButton('Remove last from Outer boundary').onclick();f.pickButton('Finish boundary pick').onclick();f.pickButton('Stage material boundaries').onclick();
 assert.deepEqual(f.studio.draft.definition.profile,['vertical']);assert.deepEqual(f.baseline.profile,['horizontal']);assert.equal(f.studio.draft.undo.length,1);
 f.drawing.undo();assert.deepEqual(f.studio.draft.definition.profile,['horizontal']);f.drawing.redo();assert.deepEqual(f.studio.draft.definition.profile,['vertical']);f.drawing.discard(true);
});
test('primary holes and additional regions with holes stage together and never mutate the baseline',t=>{
 const f=setup(t);f.baseline.profile=['horizontal'];f.baseline.circles=['inner','extra','extra-hole'].map(id=>({id}));
 f.picks.open('boundary');f.pickButton('Add hole').onclick();enter(f,'hole 1','inner');f.pickButton('Add material region').onclick();enter(f,'Region 1 outer boundary','extra');f.pickButton('Add Region 1 hole').onclick();enter(f,'Region 1 hole 1','extra-hole');
 f.pickButton('Stage material boundaries').onclick();assert.deepEqual(f.studio.draft.definition.holes,[['inner']]);assert.deepEqual(f.studio.draft.definition.regions,[{profile:['extra'],holes:[['extra-hole']]}]);assert.equal(f.baseline.holes,undefined);assert.equal(f.studio.draft.undo.length,1);f.drawing.discard(true);
});
test('discovery initialization supports canvas curve and seed picks with explicit mode switching',async t=>{
 const f=setup(t);f.baseline.profile=['horizontal'];f.picks.open('boundary');choose(f,'Material selection','discovery');
 f.pickButton('Pick Discovery curves on canvas').onclick();await f.click([-5,0]);choose(f,'Discovered cell selection','selected');f.pickButton('Pick Discovery seeds on canvas').onclick();await f.click([0,10]);
 f.pickButton('Stage material boundaries').onclick();assert.deepEqual(f.studio.draft.definition.region_discovery,{entities:['horizontal'],seeds:['D']});assert.deepEqual(f.studio.draft.definition.profile,[]);assert.deepEqual(f.baseline.profile,['horizontal']);assert.equal(f.studio.draft.undo.length,1);f.drawing.discard(true);
});
test('automatic mode explicitly clears material selections and rejected boundaries remain repairable',t=>{
 const f=setup(t);f.baseline.profile=['horizontal'];f.baseline.holes=[['vertical']];f.studio.ensureDraft();f.studio.draft.error='open boundary';assert(f.picks.open('boundary'));
 choose(f,'Material selection','automatic');f.pickButton('Stage material boundaries').onclick();assert.deepEqual(f.studio.draft.definition.profile,[]);assert.deepEqual(f.studio.draft.definition.holes,[]);assert.equal(f.studio.draft.error,null);f.drawing.discard(true);
});
test('split-piece display names are pickable as material boundary members',async t=>{
 const f=setup(t);f.baseline.profile=['horizontal'];f.baseline.curve_operations=[{split:{entity:'horizontal',before:'first',after:'last'}}];
 f.studio.scenes[0].entities.push({id:'edited:first',curve_id:'first',points:[[-10,0,0],[0,0,0]]});f.picks.open('boundary');f.pickButton('Clear Outer boundary').onclick();f.pickButton('Pick Outer boundary on canvas').onclick();await f.click([-5,0]);f.pickButton('Stage material boundaries').onclick();
 assert.deepEqual(f.studio.draft.definition.profile,['first']);assert.equal(f.studio.draft.undo.length,1);f.drawing.discard(true);
});
for(const action of ['close','edit','history','layer','apply'])test(`old boundary forms cannot overwrite a draft after ${action}`,async t=>{
 const f=setup(t);f.baseline.profile=['horizontal'];f.studio.ensureDraft();f.studio.changeDraft({...f.studio.draft.definition,revision:1});f.studio.draft.previewing=false;
 f.picks.open('boundary');enter(f,'Outer boundary','vertical');const submit=f.pickButton('Stage material boundaries').onclick;
 if(action==='close')f.picks.close();if(action==='edit')f.studio.changeDraft({...f.studio.draft.definition,revision:2});if(action==='history')f.drawing.undo();if(action==='layer')f.studio.onActivate({feature:'other'});
 if(action==='apply'){f.studio.api=async()=>({applied:true});await f.drawing.apply();}
 const d=f.studio.draft,definition=d?.definition;submit();assert.equal(f.studio.draft,d);assert.equal(d?.definition,definition);assert.match(f.studio.message,/sketch changed/);f.drawing.discard(true);
});
test('boundary validation retains the form and local sketches can stage material selection',t=>{
 const f=setup(t);f.studio.ensureDraft();f.studio.draft.local=true;f.layer.local=true;f.picks.open('boundary');choose(f,'Material selection','explicit');
 f.pickButton('Stage material boundaries').onclick();assert.match(f.studio.message,/Outer boundary/);assert.equal(f.studio.draft.undo.length,0);
 enter(f,'Outer boundary','horizontal');f.pickButton('Add hole').onclick();enter(f,'hole 1','horizontal');f.pickButton('Stage material boundaries').onclick();assert.match(f.studio.message,/more than one loop/);assert.equal(f.studio.draft.undo.length,0);
 f.pickButton('Remove hole 1').onclick();f.pickButton('Stage material boundaries').onclick();assert.equal(f.studio.draft.undo.length,1);f.drawing.discard(true);
});
