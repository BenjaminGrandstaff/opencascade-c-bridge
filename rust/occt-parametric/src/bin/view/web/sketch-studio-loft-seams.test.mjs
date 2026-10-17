import test from 'node:test';
import assert from 'node:assert/strict';
import {loftSeamGuides,drawLoftSeamGuides} from './sketch-studio-loft-seams.mjs';
const seam=(hole=null)=>({anchors:[[0,0,0],[1,0,0],[2,0,0],[3,0,0],[4,0,0]],detail:{measurement:'loft_profile_seam',profile:hole===null?'outer':'hole',station:0,hole,reverse:false,coordinate_system:'family-local mm'}});
const data=()=>({scenes:[{kind:'solid',instance:'part',feature:'body',annotations:[seam(),seam(0)]}]});
test('seam guides require the verified instance/output and finite native traversal samples',()=>{
 const source=data(),guides=loftSeamGuides(source,'part','body');assert.equal(guides.length,2);guides[0].points[0][0]=9;assert.equal(source.scenes[0].annotations[0].anchors[0][0],0);
 assert.deepEqual(loftSeamGuides(source,'other','body'),[]);assert.deepEqual(loftSeamGuides(source,'part','other'),[]);
 source.scenes[0].annotations[0].anchors[0][0]=NaN;assert.equal(loftSeamGuides(source,'part','body').length,1);source.scenes[0].annotations[1].detail.station=-1;assert.deepEqual(loftSeamGuides(source,'part','body'),[]);
});
test('outer and hole starts draw labelled read-only markers and finite traversal paths',()=>{
 const nodes=drawLoftSeamGuides(loftSeamGuides(data(),'part','body'),{matrix:[1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1],width:1000,height:1000,element:(tag,attrs)=>({tag,attrs})});
 assert(nodes.every(n=>n.attrs['pointer-events']==='none'));assert(nodes.some(n=>n.textContent==='Outer station 1 start'));assert(nodes.some(n=>n.textContent==='Hole 1 station 1 start'));assert(!JSON.stringify(nodes).includes('NaN'));
});
