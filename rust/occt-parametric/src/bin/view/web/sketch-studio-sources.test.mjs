import test from 'node:test';
import assert from 'node:assert/strict';
import {sourceEdgePaths, pickSourceEdge} from './sketch-studio-sources.mjs';
import {viewProjection} from './viewer.mjs';
import {basePlane,lookAt,project,toGltf} from './sketch-studio-plane.mjs';
const viewport = () => ({matrix:viewProjection(lookAt({target:[0,0,0],distance:.1,yaw:0,pitch:0},basePlane('xy'),[0,0]),1),width:400,height:400});
test('source edge picks use world coordinates and native edge IDs with camera/viewport cache invalidation',()=>{
 const scene={edges:[{edge:17,line:0}],lines:[[[-10,0,5],[10,0,5]]]},v=viewport();
 const paths=sourceEdgePaths(scene,v);assert.match(paths[0].d,/^M/);assert.equal(sourceEdgePaths(scene,{...v,matrix:[...v.matrix]}),paths);
 const at=project(v.matrix,toGltf([0,0,5]),v.width,v.height);assert.equal(pickSourceEdge(scene,v,at),17);assert.equal(pickSourceEdge(scene,v,[0,0]),null);
 assert.notEqual(sourceEdgePaths(scene,{...v,width:800}),paths);const changed=[...v.matrix];changed[12]+=.1;assert.notEqual(sourceEdgePaths(scene,{...v,matrix:changed}),paths);
});
test('overlapping source edges prefer the front edge and offscreen/behind-eye segments cannot bridge',()=>{
 const v=viewport();v.matrix=viewProjection({target:[0,0,0],distance:.1,yaw:0,pitch:Math.PI/2},1);const scene={edges:[{edge:3,line:0},{edge:9,line:1}],lines:[[[-10,0,-5],[10,0,-5]],[[-10,0,5],[10,0,5]]]};
 assert.equal(pickSourceEdge(scene,v,[200,200]),9);
 const behind={edges:[{edge:1,line:0}],lines:[[[-10,0,200],[10,0,200]]]};assert.equal(pickSourceEdge(behind,v,[200,200]),null);
 assert.equal(pickSourceEdge({edges:[{edge:1,line:99}]},v,[200,200]),null);
});
