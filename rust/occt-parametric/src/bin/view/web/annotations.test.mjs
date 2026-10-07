import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createAnnotationController, editableControls } from './annotations.mjs';
const deferred = () => { let resolve; const promise = new Promise(r => resolve = r); return { promise, resolve }; };
const data = (id, version) => ({version, scenes:[{kind:'sketch', instance:id, feature:'profile'}]});

 test('linked controls preserve actual parameter units and edit only declared controls', () => {
  const parameters = [{id:'width',kind:'scalar',unit:'inch',value:2}, {id:'enabled',kind:'boolean',value:true}, {id:'location',kind:'vector'}, {id:'count',kind:'integer',value:3}];
  assert.deepEqual(editableControls({parameters:['derived','width','location','enabled']}, parameters), parameters.slice(0,2));
  assert.deepEqual(editableControls({parameters:[]}, parameters), []);
});

test('a late previous-instance reply cannot replace the selected part', async () => {
  const a=deferred(), b=deferred(), updates=[];
  const viewer={update:d=>updates.push(d),fit(){}};
  const controller=createAnnotationController({request:id=>id==='a'?a.promise:b.promise,mount:()=>viewer});
  const old=controller.refresh('a',1), current=controller.refresh('b',1);
  b.resolve(data('b',1)); await current;
  a.resolve(data('a',1)); await old;
  assert.deepEqual(updates.map(d=>d.scenes[0].instance),['b']);
});

test('new native geometry updates the mounted view; unchanged polls do no work', async () => {
  let calls=0, mounts=0, fits=0;
  const updates=[];
  const controller=createAnnotationController({request:async()=>data('block',++calls),mount:()=>{mounts++;return {update:d=>updates.push(d),fit:()=>fits++};}});
  await controller.refresh('block',1);
  await controller.refresh('block',1);
  await controller.refresh('block',2);
  controller.fit();
  assert.equal(calls,2);assert.equal(mounts,1);assert.equal(fits,1);
  assert.deepEqual(updates.map(d=>d.version),[1,2]);
});

test('rejected previews survive polling, then revert to accepted data on a valid edit or revert', async () => {
  const updates=[],messages=[];let calls=0;
  const controller=createAnnotationController({request:async()=>{calls++;return data('block',1);},mount:()=>({update:d=>updates.push(d)}),changed:m=>messages.push(m)});
  await controller.refresh('block',1);
  await controller.preview({...data('block',1),accepted:false});
  await controller.refresh('block',1);
  assert.equal(calls,1);assert.equal(updates.at(-1).accepted,false);
  assert(messages.some(m=>m.includes('accepted model is unchanged')));
  controller.invalidate();await controller.refresh('block',1);
  assert.equal(calls,2);assert.notEqual(updates.at(-1).accepted,false);
});

test('a stale model version is retried instead of pairing new geometry with old controls', async () => {
  let calls=0;const updates=[];
  const controller=createAnnotationController({request:async()=>{calls++;return data('block',2);},mount:()=>({update:d=>updates.push(d)})});
  await controller.refresh('block',1);assert.equal(updates.length,0);
  await controller.refresh('block',2);assert.equal(calls,2);assert.equal(updates.at(-1).version,2);
});

test('only one renderer mounts when the selected scope changes while its panel is loading', async () => {
  const mounting=deferred(),updates=[];let mounts=0;
  const controller=createAnnotationController({request:async id=>data(id,1),mount:()=>{mounts++;return mounting.promise;}});
  const a=controller.refresh('a',1);await new Promise(r=>setImmediate(r));
  const b=controller.refresh('b',1);await new Promise(r=>setImmediate(r));
  mounting.resolve({update:d=>updates.push(d)});await Promise.all([a,b]);
  assert.equal(mounts,1);assert.deepEqual(updates.map(d=>d.scenes[0].instance),['b']);
});
