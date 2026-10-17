import {sameSketchDefinition} from './sketch-studio-draw.mjs';
import test from 'node:test';
import assert from 'node:assert/strict';
import {dimensionValueText, editDrivingDimension, constraintsTouching} from './sketch-studio-select.mjs';
import {sketchLabels} from './sketch-studio-labels.mjs';
import {fixture} from './sketch-studio-test-fixture.mjs';
import {installConstraints} from './sketch-studio-constrain.mjs';
const mm = value => ({literal: {value, dimension: 'length', unit: 'millimeter'}});
const definition = () => ({points: [{id:'A',x:mm(0),y:mm(0)},{id:'B',x:mm(10),y:mm(0)}], lines:[{id:'L',start:'A',end:'B'}],
 constraints: [{horizontal:{line:'L'}},{distance:{first:'A',second:'B',value:mm(10)}}]});

test('driving dimension text converts length units, radians, parameters and compound formulas', () => {
 assert.equal(dimensionValueText({literal:{value:2,dimension:'length',unit:'inch'}},'length'),'50.8');
 assert.equal(dimensionValueText({literal:{value:Math.PI/2,dimension:'scalar',unit:null}},'angle'),'90');
 assert.equal(dimensionValueText({parameter:'width'},'length'),'width');
 assert.equal(dimensionValueText({multiply:[mm(2),mm(3)]},'length'),null);
});

test('editing replaces only the driving value, preserves roles and leaves the source immutable', () => {
 const d=definition(), original=structuredClone(d), next=editDrivingDimension(d,1,d.constraints[1],'24');
 assert.deepEqual(d,original); assert.deepEqual(next.constraints[0],d.constraints[0]);
 assert.deepEqual(next.constraints[1],{distance:{first:'A',second:'B',value:mm(24)}});
 assert.deepEqual(editDrivingDimension(d,1,d.constraints[1],'width').constraints[1].distance.value,{parameter:'width'});
 assert.equal(editDrivingDimension(d,1,d.constraints[1],'10'),d);
});

test('driving edits validate dimensions and reject stale or non-dimensional constraints', () => {
 const d=definition();
 assert.throws(()=>editDrivingDimension(d,1,{distance:{}},'12'),/changed/);
 assert.throws(()=>editDrivingDimension(d,0,d.constraints[0],'12'),/driving/);
 for(const value of ['','-1','1+']) assert.throws(()=>editDrivingDimension(d,1,d.constraints[1],value));
 d.constraints[1]={line_angle:{line:'L',value:{literal:{value:Math.PI/2,dimension:'scalar',unit:null}}}};
 assert.throws(()=>editDrivingDimension(d,1,d.constraints[1],'190'),/signed angle/);
 const value=editDrivingDimension(d,1,d.constraints[1],'-45').constraints[1].line_angle.value.literal.value;
 assert(Math.abs(value+Math.PI/4)<1e-12);
});

test('untouched unit expressions and compound formulas preserve the exact saved expression', () => {
 const d=definition();d.constraints[1].distance.value={literal:{value:1,dimension:'length',unit:'inch'}};
 assert.equal(editDrivingDimension(d,1,d.constraints[1],'25.4'),d);
 d.constraints[1].distance.value={add:[mm(3),mm(7)]};
 assert.equal(editDrivingDimension(d,1,d.constraints[1],''),d);
 assert.deepEqual(editDrivingDimension(d,1,d.constraints[1],'12').constraints[1].distance.value,mm(12));
});

function editorFixture(t) {
 const f=fixture(t);Object.assign(f.baseline,definition());
 f.studio.sceneOf=()=>f.studio.scenes[0];
 installConstraints(f.studio,f.document);
 const box=()=>f.document.getElementById('view').children.find(n=>n.attrs.id==='value-box');
 const input=()=>box().children.find(n=>n.tag==='input');
 const update=value=>{input().value=value;box().children.find(n=>n.tag==='button').attrs.onclick();};
 return {...f,box,input,update};
}

test('inline editing makes one Undo entry and retains constraint indexes through Undo/Redo', t => {
 const f=editorFixture(t);assert(f.studio.editDimension(1,[10,10]));assert.equal(f.input().attrs.value,'10');
 f.update('20');assert.equal(f.studio.draft.definition.constraints.length,2);assert.equal(f.studio.draft.undo.length,1);
 assert.equal(f.studio.draft.definition.constraints[1].distance.value.literal.value,20);
 f.drawing.undo();assert.equal(f.studio.draft.definition.constraints[1].distance.value.literal.value,10);
 f.drawing.redo();assert.equal(f.studio.draft.definition.constraints[1].distance.value.literal.value,20);
 assert(f.box().hidden);f.drawing.discard(true);
});

test('cancel and unchanged Update do not add local history', t => {
 const f=editorFixture(t);f.studio.editDimension(1,[10,10]);
 f.input().attrs.onkeydown({key:'Escape',stopPropagation(){}});assert.equal(f.studio.hasDraftHistory(),false);
 f.studio.editDimension(1,[10,10]);f.update('10');assert.equal(f.studio.hasDraftHistory(),false);
 f.drawing.discard(true);
});

test('invalid inline values leave the editor open for correction', t => {
 const f=editorFixture(t);f.studio.editDimension(1,[10,10]);f.update('-1');
 assert.equal(f.studio.draft.undo.length,0);assert.equal(f.box().hidden,false);assert.equal(f.studio.error,true);
 f.update('width');assert.deepEqual(f.studio.draft.definition.constraints[1].distance.value,{parameter:'width'});
 f.drawing.discard(true);
});

for(const action of ['edit','history','discard','version']) {
 test(`inline editor cannot overwrite a dimension after ${action}`, t => {
  const f=editorFixture(t);f.studio.editDimension(1,[10,10]);
  const old=f.input();old.value='30';const update=f.box().children.find(n=>n.tag==='button').attrs.onclick;
  if(action==='edit') f.studio.changeDraft({...f.studio.draft.definition,changed:true});
  if(action==='history') {f.studio.changeDraft({...f.studio.draft.definition,changed:true});f.drawing.undo();}
  if(action==='discard') f.drawing.discard(true);
  if(action==='version') f.studio.version++;
  const d=f.studio.draft, snapshot=d?.definition;update();
  assert.equal(f.studio.draft,d);assert.equal(d?.definition,snapshot);assert.match(f.studio.message,/sketch changed/);
  f.drawing.discard(true);
 });
}

test('labels distinguish authored driving dimensions, native-only constraints and measured references', () => {
 const d=definition(),constraint=d.constraints[1];
 const scene={annotations:[
  {id:'constraint-1',label:'10 mm',status:'passed',anchors:[[0,0,0]],detail:{constraint,constraint_index:1}},
  {id:'constraint-2',label:'5 mm',status:'passed',anchors:[[0,0,0]],detail:{constraint:{distance:{first:'X',second:'Y',value:mm(5)}}}},
  {id:'measurement-width',label:'(10 mm)',status:'measured',anchors:[[0,0,0]],detail:{driving:false,measurement:{id:'width'}}},
 ]};
 const labels=sketchLabels(scene,d);assert.equal(labels.length,3);
 assert.equal(labels[0].constraintIndex,1);assert.equal(labels[1].constraintIndex,-1);
 assert.equal(labels[2].reference,true);assert.equal(labels[2].constraintIndex,-1);assert.match(labels[2].title,/read only/);
 assert.equal(labels[2].text,'(10 mm)');
});

test('selecting a line includes its endpoint driving dimensions in the inspector', () => {
 assert.deepEqual(constraintsTouching(definition(),['L']),[0,1]);
});

test('label matching handles reordered keys and keeps duplicate authored values unambiguous', () => {
 const d=definition();d.constraints.push(structuredClone(d.constraints[1]));
 const a={id:'constraint-99',label:'10 mm',anchors:[[0,0,0]],detail:{constraint:{distance:{value:mm(10),second:'B',first:'A'}}}};
 assert.equal(sketchLabels({annotations:[a]},d)[0].constraintIndex,-1);
 a.detail.constraint_index=1;
 assert.equal(sketchLabels({annotations:[a]},d)[0].constraintIndex,1);
});

test('matching 5,000 native-only dimension labels avoids quadratic authored scans', () => {
 const d={constraints:Array.from({length:5000},(_,i)=>({distance:{first:`A${i}`,second:`B${i}`,value:mm(i+1)}}))};
 const scene={annotations:Array.from({length:5000},(_,i)=>({id:`constraint-${i+5000}`,label:'10 mm',anchors:[[0,0,0]],detail:{constraint:{distance:{first:`X${i}`,second:`Y${i}`,value:mm(10)}}}}))};
 const start=performance.now(),labels=sketchLabels(scene,d),elapsed=performance.now()-start;
 assert.equal(labels.length,5000);assert(labels.every(l=>l.constraintIndex===-1));assert(elapsed<500,`${elapsed} ms`);
});

test('editing back to the accepted expression skips unchanged Preview but keeps Undo history', async t => {
 const f=editorFixture(t);f.studio.editDimension(1,[10,10]);f.update('20');
 f.studio.editDimension(1,[10,10]);f.update('10');
 t.mock.timers.tick(150);await f.flush();
 assert.equal(f.calls.length,0);assert.equal(f.studio.draft.dirty,false);assert.equal(f.studio.draft.undo.length,2);
 assert.equal(f.button('Apply').attrs.disabled,true);assert.equal(f.button('Undo').attrs.disabled,false);
 f.drawing.undo();assert.equal(f.studio.draft.definition.constraints[1].distance.value.literal.value,20);
 f.drawing.redo();t.mock.timers.tick(150);await f.flush();assert.equal(f.studio.draft.dirty,false);assert.equal(f.calls.length,0);
 f.drawing.discard(true);
});

test('unchanged sketch comparison ignores key order and native empty collection defaults',()=>{
 const baseline=definition(),next=structuredClone(baseline);
 next.profile_operations=[];next.curve_operations=[];next.region_edits=[];
 next.constraints[1].distance.value={literal:{unit:'millimeter',dimension:'length',value:10}};
 assert(sameSketchDefinition(baseline,next));
 next.constraints[1].distance.value.literal.value=11;assert.equal(sameSketchDefinition(baseline,next),false);
 assert.equal(sameSketchDefinition({points:[],lines:[]},{lines:[]}),false,'required points are not optional');
 assert.equal(sameSketchDefinition({points:[1,2]},{points:[2,1]}),false,'array order remains meaningful');
});
