import test from 'node:test';
import assert from 'node:assert/strict';
import {loftTangentGuides,drawLoftTangentGuides} from './sketch-studio-loft-guides.mjs';
const annotation=end=>({anchors:end==='start'?[[0,0,0],[0,0,2]]:[[0,0,8],[0,0,10]],detail:{measurement:'loft_end_tangent',end,origin_mm:[0,0,end==='start'?0:10],derivative_mm:[0,0,6],strength_mm:6,handle_scale:1/3,mode:'normal_to_profile',profile:end}});
const annotations=()=>({scenes:[{kind:'solid',instance:'part',feature:'body',annotations:[annotation('start'),annotation('end')]}]});
test('native tangent guides retain true end orientation and reject wrong output, instance and inconsistent vectors',()=>{
 const source=annotations(),guides=loftTangentGuides(source,'part','body');assert.equal(guides.length,2);assert.deepEqual(guides[1].anchors,[[0,0,8],[0,0,10]]);guides[0].anchors[0][0]=99;assert.equal(source.scenes[0].annotations[0].anchors[0][0],0);
 assert.deepEqual(loftTangentGuides(source,'other','body'),[]);assert.deepEqual(loftTangentGuides(source,'part','other'),[]);
 source.scenes[0].annotations[0].detail.strength_mm=999;assert.equal(loftTangentGuides(source,'part','body').length,1);source.scenes[0].annotations[1].anchors[1][0]=NaN;assert.deepEqual(loftTangentGuides(source,'part','body'),[]);
});
test('guide overlays draw distinct read-only arrows and full strength labels with finite paths',()=>{
 const context={matrix:[1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1],width:100,height:100,element:(tag,attrs)=>({tag,attrs})};const nodes=drawLoftTangentGuides(loftTangentGuides(annotations(),'part','body'),context);
 assert(nodes.every(n=>n.attrs['pointer-events']==='none'));assert(nodes.some(n=>n.attrs['data-loft-tangent']==='start'));assert(nodes.some(n=>n.attrs['data-loft-tangent']==='end'));assert(nodes.some(n=>n.textContent==='End tangent · strength 6 mm'));assert(!JSON.stringify(nodes).includes('NaN'));
});
