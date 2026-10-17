// Accepted source overlays and edge picking in model coordinates. Excludes DOM.
import assert from 'node:assert/strict';
import {performance} from 'node:perf_hooks';
import {sourceEdgePaths,pickSourceEdge} from '../../rust/occt-parametric/src/bin/view/web/sketch-studio-sources.mjs';
import {viewProjection} from '../../rust/occt-parametric/src/bin/view/web/viewer.mjs';
import {basePlane,lookAt,project,toGltf} from '../../rust/occt-parametric/src/bin/view/web/sketch-studio-plane.mjs';
const scene={edges:[],lines:[]};
for(let i=0;i<10000;i++){
 const x=(i%100)*2,y=Math.floor(i/100)*2;
 scene.edges.push({edge:i,line:i});scene.lines.push(Array.from({length:33},(_,k)=>[x+k/32,y,0]));
}
const viewport={matrix:viewProjection(lookAt({target:[0,0,0],distance:1,yaw:0,pitch:0},basePlane('xy'),[100,100]),1),width:1000,height:1000};
let start=performance.now();const paths=sourceEdgePaths(scene,viewport),build=performance.now()-start;
start=performance.now();for(let i=0;i<100;i++)assert.equal(sourceEdgePaths(scene,{...viewport,matrix:[...viewport.matrix]}),paths);const cached=(performance.now()-start)/100;
const at=project(viewport.matrix,toGltf([100.5,100,0]),1000,1000);
start=performance.now();assert.equal(pickSourceEdge(scene,viewport,at),5050);const pick=performance.now()-start;
console.log(`Studio source edges: 10,000 edges / 330,000 samples ${build.toFixed(1)} ms build, ${cached.toFixed(3)} ms cached overlay, ${pick.toFixed(1)} ms click (budgets 1,000/1/100 ms; no hover query)`);
assert(build<1000&&cached<1&&pick<100,'Studio source edge projection/picking exceeded its scale budget.');
