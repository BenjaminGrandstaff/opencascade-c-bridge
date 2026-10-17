import test from 'node:test';
import assert from 'node:assert/strict';
import {createLoftAnchorPicker} from './sketch-studio-loft-pick.mjs';
const matrix=[1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1];
const snapshot=()=>({version:3,instance:'part',profile:'profile',accepted:true,coordinate_system:'family-local mm',continuous:false,origin_mm:[500,500,0],normal:[0,0,1],vertices:[[0,0,0],[1000,0,0],[1000,1000,0],[0,1000,0]],lines:[[[0,0,0],[1000,0,0],[1000,1000,0],[0,1000,0],[0,0,0]]]});
const picked=()=>({version:3,instance:'part',profile:'profile',accepted:true,coordinate_system:'family-local mm',position_mm:[0,0,0],distance_mm:0});
const flush=async()=>{for(let i=0;i<8;i++)await Promise.resolve();};
function fixture(api){let current=true;const calls=[],modes=[],messages=[];const studio={version:3,instance:'part',tools:new Map(),redraw(){},say:m=>messages.push(m),api:async(path,options)=>{const body=JSON.parse(options.body);calls.push({path,body});return api?api(body):body.position_mm?picked():snapshot();},viewport:()=>({matrix,width:100,height:100}),rayAt:()=>({origin:[0,1,1],direction:[0,-1,0]}),decorate:()=>[]};const picker=createLoftAnchorPicker(studio,a=>modes.push(a));return {studio,picker,calls,modes,messages,current:()=>current,expire:()=>current=false};}
const click={button:0,clientX:50,clientY:50};
test('corner picking snaps to accepted vertices and stores coordinates without topology indices',async()=>{
 const f=fixture(),pending=f.picker.pick('profile',f.current);await flush();assert(f.picker.active);assert.equal(f.studio.limitFaceBounds().max[0],1);
 await f.picker.click({button:0,clientX:75,clientY:50});assert.equal(f.calls.length,1);assert.match(f.messages.at(-1),/corner/);
 await f.picker.click(click);assert.deepEqual(await pending,{profile:'profile',position_mm:[0,0,0]});assert.deepEqual(f.calls[1].body.position_mm,[0,0,0]);assert.equal(f.calls[1].body.vertex,undefined);assert.equal(f.picker.active,false);assert.deepEqual(f.modes,[true,false]);
 f.picker.cancel();assert.equal(f.picker.source,null);assert.equal(f.studio.limitFaceBounds(),null);
});
test('continuous picks use the profile plane and accept only a verified native snap',async()=>{
 const f=fixture(body=>body.position_mm?{...picked(),position_mm:[500,0,0]}:{...snapshot(),continuous:true});
 f.studio.rayAt=()=>({origin:[.5,1,0],direction:[0,-1,0]});
 const pending=f.picker.pick('profile',f.current);await flush();await f.picker.click({button:0,clientX:75,clientY:50});assert.deepEqual((await pending).position_mm,[500,0,0]);assert.deepEqual(f.calls[1].body.position_mm,[500,0,0]);
});
test('wrong boundary snapshots and nonfinite data reject before picking',async()=>{
 for(const delta of [{instance:'other'},{profile:'other'},{normal:[0,0,0]},{vertices:[[NaN,0,0]]},{lines:[[[0,0,0]]]}]){
  const f=fixture(()=>({...snapshot(),...delta}));await assert.rejects(f.picker.pick('profile',f.current),/does not match/);assert.equal(f.picker.active,false);
 }
});
test('late loads and snaps are discarded after cancel, model, instance or draft changes',async()=>{
 for(const action of ['load','cancel','model','instance','draft']){
  let resolve;const f=fixture(body=>action==='load'||body.position_mm?new Promise(r=>resolve=r):snapshot());const pending=f.picker.pick('profile',f.current);await flush();let response;
  if(action!=='load')response=f.picker.click(click);
  if(action==='model')f.studio.version++;else if(action==='instance')f.studio.instance='other';else if(action==='draft')f.expire();else f.picker.cancel();
  resolve(action==='load'?snapshot():picked());await response;await flush();assert.equal(await pending,null);assert.equal(f.picker.source,null);
 }
});
test('native snap rejections keep picking available and all candidate overlays are read-only',async()=>{
 let reject=true;const f=fixture(body=>!body.position_mm?snapshot():reject?{...picked(),position_mm:[999,999,999]}:picked());const pending=f.picker.pick('profile',f.current);await flush();await f.picker.click(click);assert(f.picker.active);assert.match(f.messages.at(-1),/rejected/);
 const context={matrix,width:100,height:100,element:(tag,attrs)=>({tag,attrs})};assert(f.studio.decorate(context).length>0);assert(f.studio.decorate(context).every(n=>n.attrs['pointer-events']==='none'));
 reject=false;await f.picker.click(click);await pending;f.picker.cancel();assert.deepEqual(f.studio.decorate(context),[]);
});
