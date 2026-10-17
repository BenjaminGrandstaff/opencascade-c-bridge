import test from 'node:test';
import assert from 'node:assert/strict';
import {fixture} from './sketch-studio-test-fixture.mjs';

test('undoing the first edit retains redo and never navigates accepted model history',t=>{
 const f=fixture(t);f.edit(1);const snapshot=f.studio.draft.definition;f.drawing.undo();
 assert.deepEqual(f.studio.draft.definition,f.baseline);assert.equal(f.studio.draft.dirty,false);assert.equal(f.studio.draft.redo.length,1);assert(f.studio.hasDraftHistory());
 assert(f.button('Undo').attrs.disabled);assert.equal(f.button('Redo').attrs.disabled,false);assert.equal(f.button('Discard').attrs.disabled,false);assert(f.button('Apply').attrs.disabled);assert(f.button('Save').attrs.disabled);
 assert.match(f.studio.message,/starting sketch/);assert.match(f.studio.statusExtra().children[0],/Redo available/);
 f.drawing.undo();assert.equal(f.calls.length,0);f.drawing.redo();assert.equal(f.studio.draft.definition,snapshot);assert.equal(f.studio.draft.dirty,true);f.drawing.discard(true);
});

test('multi-step local history returns all snapshots and a new edit discards only the redo branch',t=>{
 const f=fixture(t);f.edit(1);const a=f.studio.draft.definition;f.edit(2);const b=f.studio.draft.definition;
 f.drawing.undo();assert.equal(f.studio.draft.definition,a);f.drawing.undo();assert.deepEqual(f.studio.draft.definition,f.baseline);
 f.drawing.redo();assert.equal(f.studio.draft.definition,a);f.drawing.redo();assert.equal(f.studio.draft.definition,b);
 f.drawing.undo();f.edit(3);assert.equal(f.studio.draft.redo.length,0);assert(f.button('Redo').attrs.disabled);f.drawing.redo();assert.equal(f.studio.draft.definition.points[0].id,'p3');assert.equal(f.calls.length,0);f.drawing.discard(true);
});

test('a new sketch can undo to an empty local definition without previewing invalid empty geometry',async t=>{
 const f=fixture(t,{local:true});f.studio.ensureDraft();const empty=f.studio.draft.definition;f.edit(1);f.drawing.undo();
 t.mock.timers.tick(200);await f.flush();assert.equal(f.calls.length,0);assert.equal(f.studio.draft.definition,empty);assert.equal(f.layer.freeDegrees,null);assert.equal(f.studio.busy,true);
 f.drawing.redo();assert.equal(f.studio.draft.definition.points[0].id,'p1');f.drawing.discard(true);
});

test('undo immediately invalidates late preview success and failure and restores accepted solver feedback',async t=>{
 let resolve,reject;const pending=new Promise(a=>resolve=a);
 const q=fixture(t,{api:()=>pending});q.edit(1);t.mock.timers.tick(150);await q.flush();assert(q.studio.draft.previewing);
 q.studio.previewScenes.profile={kind:'sketch',feature:'profile',solver:{free_degrees:99}};q.layer.freeDegrees=99;q.layer.error='old';q.studio.draft.error='old';
 q.drawing.undo();assert.equal(q.layer.freeDegrees,2);assert.equal(q.layer.error,null);assert.equal(q.studio.draft.error,null);assert.equal(q.studio.previewScenes.profile,undefined);assert.equal(q.studio.draft.previewing,false);
 resolve({can_apply:true,annotations:{scenes:[{kind:'sketch',feature:'profile',solver:{free_degrees:99}}]}});await q.flush();assert.equal(q.studio.previewScenes.profile,undefined);assert.match(q.studio.message,/starting sketch/);
 // A later preview rejection must also stay out of the baseline history state.
 q.studio.api=()=>new Promise((a,b)=>{reject=b});q.drawing.redo();t.mock.timers.tick(150);await q.flush();q.drawing.undo();reject(Error('late rejected preview'));await q.flush();assert.equal(q.studio.draft.error,null);assert.match(q.studio.message,/starting sketch/);q.drawing.discard(true);
});

test('held redo survives browsing another layer; explicit Discard releases model history',async t=>{
 const f=fixture(t);f.edit(1);f.drawing.undo();f.studio.layers.push({feature:'other',local:false});f.studio.activate('other');assert.equal(f.studio.ensureDraft(),null);assert.equal(f.studio.draft.redo.length,1);
 f.drawing.redo();assert.equal(f.studio.activeLayer().feature,'profile');f.drawing.undo();f.drawing.discard(true);assert.equal(f.studio.draft,null);assert.equal(f.studio.busy,false);assert.equal(f.studio.hasDraftHistory(),false);
 f.drawing.undo();await f.flush();f.drawing.redo();await f.flush();assert.deepEqual(f.calls.map(c=>c.path),['/api/undo','/api/redo']);assert(f.calls.every(c=>JSON.parse(c.body).version===1));assert.equal(f.button('Save').attrs.disabled,false);
});

test('Apply ends the draft history and makes accepted model history available again',async t=>{
 const f=fixture(t,{api:()=>({applied:true})});f.edit(1);f.drawing.undo();f.drawing.redo();await f.drawing.apply();assert.equal(f.studio.draft,null);assert.equal(f.studio.hasDraftHistory(),false);assert.equal(f.studio.busy,false);assert.equal(f.calls[0].path,'/api/sketch');assert.equal(JSON.parse(f.calls[0].body).apply,true);
});

test('model navigation sends its version and drops a clean idle draft from the previous model',async t=>{
 const f=fixture(t);f.studio.ensureDraft();assert.equal(f.studio.hasDraftHistory(),false);f.drawing.undo();await f.flush();assert.equal(f.calls[0].path,'/api/undo');assert.deepEqual(JSON.parse(f.calls[0].body),{version:1});assert.equal(f.studio.draft,null);assert.equal(f.studio.busy,false);
});
