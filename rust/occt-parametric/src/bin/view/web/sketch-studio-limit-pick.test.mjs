import test from 'node:test';
import assert from 'node:assert/strict';
import {createLimitFacePicker} from './sketch-studio-limit-pick.mjs';
const mesh=[{face:7,points:[[0,0,1],[2,0,1],[0,2,1]]}];
const scene={kind:'solid',feature:'stop',instance:'part',mesh,edges:[]};
const source=()=>({version:3,accepted:true,source_view:true,scenes:[scene]});
const chosen=()=>({version:3,instance:'part',face_support:{input:'stop',face:{normal_aligned:{direction:{parameter:'normal'}}}},binding:'normal',normal:[0,0,1],origin_mm:[1,1,1]});
const flush=async()=>{for(let i=0;i<6;i++)await Promise.resolve();};
function fixture(api){const calls=[],modes=[],messages=[];let current=true;const studio={version:3,instance:'part',tools:new Map(),api:async(path,options)=>{calls.push({path,body:JSON.parse(options.body)});return api?api(path,options):path==='/api/sketch-source-view'?source():chosen();},redraw(){},say:m=>messages.push(m),rayAt:()=>({origin:[0.0005,0.003,-0.0005],direction:[0,-1,0]}),decorate:()=>[]};const picker=createLimitFacePicker(studio,v=>modes.push(v));return {studio,picker,calls,modes,messages,current:()=>current,expire:()=>current=false};}
const event={button:0,clientX:2,clientY:3};
test('native target view and face hint produce only a semantic limiting selector',async()=>{
 const f=fixture(),pending=f.picker.pick('stop',f.current);await flush();assert.equal(f.picker.source,scene);assert.deepEqual(f.studio.limitFaceBounds(),{min:[0,0.001,-0.002],max:[0.002,0.001,-0]});assert(f.picker.active);
 await f.picker.click(event);const result=await pending;assert.deepEqual(result.face,chosen().face_support.face);assert.equal(result.target,'stop');assert.equal(result.face_index,undefined);assert.equal(f.calls[1].body.face,7);assert.equal(f.picker.active,false);assert.equal(f.picker.source,scene);assert.deepEqual(f.modes,[true,false]);f.picker.cancel();assert.equal(f.picker.source,null);assert.equal(f.studio.limitFaceBounds(),null);
});
test('late target loads and face replies cannot survive cancellation or context changes',async()=>{
 for(const step of ['load','face','context']){
  let reply;const f=fixture(path=>path==='/api/sketch-source-view'&&step!=='load'?source():new Promise(r=>reply=r));const pending=f.picker.pick('stop',f.current);await flush();let click;
  if(step!=='load')click=f.picker.click(event);
  if(step==='context')f.expire();else f.picker.cancel();reply(step==='load'?source():chosen());await click;await flush();assert.equal(await pending,null);assert.equal(f.picker.source,null);assert.equal(f.picker.active,false);
 }
});
test('mismatched source views reject without starting a face pick',async()=>{
 const f=fixture(()=>({...source(),scenes:[{...scene,instance:'other'}]}));await assert.rejects(f.picker.pick('stop',f.current),/does not match/);assert.equal(f.picker.source,null);assert.equal(f.picker.active,false);
});
test('missed clicks, curved faces and mismatched face replies leave picking available for correction',async()=>{
 let mode='curved';const f=fixture(path=>{if(path==='/api/sketch-source-view')return source();if(mode==='curved')throw Error('Pick a planar face.');return mode==='wrong'?{...chosen(),instance:'other'}:chosen();});const pending=f.picker.pick('stop',f.current);await flush();
 f.studio.rayAt=()=>({origin:[1,1,1],direction:[0,1,0]});await f.picker.click(event);assert.equal(f.calls.length,1);assert(f.picker.active);
 f.studio.rayAt=()=>({origin:[0.0005,0.003,-0.0005],direction:[0,-1,0]});await f.picker.click(event);assert(f.picker.active);assert.match(f.messages.at(-1),/planar/);mode='wrong';await f.picker.click(event);assert(f.picker.active);assert.match(f.messages.at(-1),/does not match/);mode='valid';await f.picker.click(event);assert.equal((await pending).target,'stop');
});
test('hover and selected face overlays are read-only and clear on cancel',async()=>{
 const f=fixture(),pending=f.picker.pick('stop',f.current);await flush();f.studio.tools.get('solid-limit-face').move(event);
 const context={matrix:[1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1],width:100,height:100,element:(tag,attrs)=>({tag,attrs})};const overlays=f.studio.decorate(context);assert.equal(overlays[0].attrs['data-limit-face'],7);assert.equal(overlays[0].attrs['pointer-events'],'none');f.picker.cancel();assert.equal(await pending,null);assert.deepEqual(f.studio.decorate(context),[]);assert.deepEqual(scene.mesh,mesh);
});


test('finishing edge picks use screen hits and return semantic selectors without edge indices',async()=>{
 const edgeScene={...scene,lines:[[[0,0,1],[1,0,1]]],edges:[{edge:4,line:0}]};
 const selected={version:3,instance:'part',binding:'extreme',projection:{id:'solid-finish-edge',input:'stop',edge:{at_extreme:{axis:'x',extremum:'minimum'}}},center_mm:[0.5,0,1],length_mm:1};
 const f=fixture(path=>path==='/api/sketch-source-view'?{...source(),scenes:[edgeScene]}:selected);f.studio.viewport=()=>({matrix:[1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1],width:100,height:100});
 const pending=f.picker.pick('stop',f.current,'edge');await flush();await f.picker.click({button:0,clientX:50,clientY:49.95});const result=await pending;assert.deepEqual(result.edge,selected.projection.edge);assert.equal(result.edge_index,undefined);assert.equal(f.calls[1].path,'/api/sketch-edge-pick');assert.equal(f.calls[1].body.edge,4);
});
test('late finishing edge replies are discarded after cancel',async()=>{
 let resolve;const edgeScene={...scene,lines:[[[0,0,1],[1,0,1]]],edges:[{edge:4,line:0}]};const f=fixture(path=>path==='/api/sketch-source-view'?{...source(),scenes:[edgeScene]}:new Promise(r=>resolve=r));f.studio.viewport=()=>({matrix:[1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1],width:100,height:100});
 const pending=f.picker.pick('stop',f.current,'edge');await flush();const click=f.picker.click({button:0,clientX:50,clientY:49.95});f.picker.cancel();resolve({});await click;assert.equal(await pending,null);assert.equal(f.picker.source,null);
});

test('hole placement uses the clicked triangle point and inward native normal, not the face center',async()=>{
 const f=fixture(),pending=f.picker.pick('stop',f.current,'hole');await flush();await f.picker.click(event);const result=await pending;
 assert.deepEqual(result.position_mm,[0.5,0.5,1]);assert.deepEqual(result.axis,[-0,-0,-1]);assert.notDeepEqual(result.position_mm,chosen().origin_mm);assert.equal(result.face_index,undefined);
 const context={matrix:[1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1],width:100,height:100,element:(tag,attrs)=>({tag,attrs})};assert(f.studio.decorate(context).some(n=>n.attrs['data-hole-position']));assert(f.studio.decorate(context).some(n=>n.attrs['data-hole-axis']));f.picker.cancel();assert.deepEqual(f.studio.decorate(context),[]);
});
test('hole placement rejects invalid or off-plane native normals and ignores late replies',async()=>{
 for(const invalid of [{normal:[0,0,0]},{normal:[NaN,0,1]},{origin_mm:[0,0,2]}]){
  const f=fixture(path=>path==='/api/sketch-source-view'?source():({...chosen(),...invalid}));const pending=f.picker.pick('stop',f.current,'hole');await flush();await f.picker.click(event);assert(f.picker.active);assert.match(f.messages.at(-1),/rejected/);f.picker.cancel();assert.equal(await pending,null);
 }
 let resolve;const f=fixture(path=>path==='/api/sketch-source-view'?source():new Promise(r=>resolve=r));const pending=f.picker.pick('stop',f.current,'hole');await flush();const click=f.picker.click(event);f.picker.cancel();resolve(chosen());await click;assert.equal(await pending,null);assert.equal(f.picker.source,null);
});
