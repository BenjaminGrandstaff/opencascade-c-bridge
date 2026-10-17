import test from 'node:test';
import assert from 'node:assert/strict';
import {sameSketchDefinition} from './sketch-studio-draw.mjs';
import {boundaryCurves,materialBoundaries} from './sketch-boundaries.mjs';
import {bindGeneratedPoints,captureGeneratedPoints} from './generated-points.mjs';
const definition=()=>({points:[{id:'seed'}],lines:[{id:'AB'},{id:'BC'},{id:'CD'},{id:'DA'}],circles:[{id:'hole'},{id:'island'}],constraints:[]});
const fields=()=>({mode:'explicit',profile:['AB','BC','CD','DA'],holes:[['hole']],regions:[{profile:['island'],holes:[]}]});
test('material selection changes only its exclusive fields and preserves ordered profiles and regions',()=>{
 const d=definition(),original=structuredClone(d),f=fields(),p=materialBoundaries(d,f);
 assert.deepEqual(d,original);assert.deepEqual(p,{profile:f.profile,holes:f.holes,regions:[{profile:['island']}],region_discovery:null});
 p.profile.reverse();assert.deepEqual(f.profile,['AB','BC','CD','DA']);
 assert.deepEqual(materialBoundaries({...d,region_discovery:{}},{mode:'automatic'}),{profile:[],holes:[],regions:[],region_discovery:null});
});
test('explicit loops reject missing, unknown and repeated curves across all material regions',()=>{
 const d=definition();
 for(const mutate of [f=>f.profile=[],f=>f.profile.push('missing'),f=>f.holes.push([]),f=>f.holes.push(['AB']),f=>f.regions[0].profile=['hole'],f=>f.regions[0].holes=[['AB']],f=>f.regions[0].profile=[],f=>f.regions=Array(1001).fill({profile:['island']})]){
  const f=fields();mutate(f);assert.throws(()=>materialBoundaries(d,f));
 }
});
test('boundary names include native shape containers, verified generated curves and split results',()=>{
 const d=definition();d.shapes=[{rectangle:{id:'box'}}];d.curve_operations=[{split:{entity:'AB',before:'first',after:'last'}}];
 bindGeneratedPoints(d,captureGeneratedPoints(d,[],{lines:['box:side0','copies:1:AB']}));
 assert(boundaryCurves(d).includes('box'));assert(boundaryCurves(d).includes('first'));assert(boundaryCurves(d).includes('copies:1:AB'));
 assert.doesNotThrow(()=>materialBoundaries(d,{mode:'explicit',profile:['box'],holes:[],regions:[]}));
 d.shapes[0].rectangle.id='changed';assert(!boundaryCurves(d).includes('copies:1:AB'),'stale generated names cannot become boundaries');
});
test('discovery replaces explicit loops with reviewed curves and seed selection and retains operation definitions',()=>{
 const d=definition();d.region_edits=[{seed:'seed',operations:[]}];
 const f={mode:'discovery',discovery:{entities:['AB','BC','CD','DA','hole'],seeds:['seed'],selection:'selected',merge_adjacent:true}};
 const p=materialBoundaries(d,f);assert.deepEqual(p.profile,[]);assert.deepEqual(p.holes,[]);assert.deepEqual(p.regions,[]);assert.deepEqual(p.region_discovery,{entities:f.discovery.entities,seeds:['seed'],merge_adjacent:true});assert.equal(d.region_edits.length,1);
 assert.equal(materialBoundaries(d,{...f,discovery:{...f.discovery,selection:'all'}}).region_discovery.seeds,undefined);
 assert.throws(()=>materialBoundaries({...d,profile_operations:[{}]},f),/Remove outer-profile operations/);
 for(const discovery of [{...f.discovery,seeds:[]},{...f.discovery,seeds:['missing']},{...f.discovery,entities:['missing']},{...f.discovery,selection:'invalid'}])assert.throws(()=>materialBoundaries(d,{...f,discovery}));
});

test('unchanged saved regions/discovery keep native omitted defaults and avoid redundant Preview',()=>{
 const d={...definition(),profile:['AB'],regions:[{profile:['island']}]};
 const patch=materialBoundaries(d,{mode:'explicit',profile:['AB'],holes:[],regions:[{profile:['island'],holes:[]}]});
 assert(sameSketchDefinition(d,{...d,...patch}));
 for(const discovery of [{},{entities:['AB']},{seeds:['seed'],merge_adjacent:true}]){
  const accepted={...definition(),region_discovery:discovery};
  const patch=materialBoundaries(accepted,{mode:'discovery',discovery:{entities:discovery.entities??[],seeds:discovery.seeds??[],selection:discovery.seeds?.length?'selected':'all',merge_adjacent:discovery.merge_adjacent??false}});
  assert(sameSketchDefinition(accepted,{...accepted,...patch}));
 }
});
