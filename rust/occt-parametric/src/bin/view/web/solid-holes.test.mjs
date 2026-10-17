import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createHoleControls} from './solid-holes.mjs';
test('holes reject invalid axes, positions, missing sources and unset limiting faces',()=>{
 const h=createHoleControls(null,'body');assert.equal(h.definition().bottom,'flat');
 h.state.axis=[0,0,0];assert.throws(()=>h.definition(),/nonzero/);
 h.state.axis=[0,0,-1];h.state.position=[0,NaN,0];assert.throws(()=>h.definition(),/finite/);
 h.state.position=[0,0,0];h.state.extent='up_to_face';assert.throws(()=>h.definition(),/limiting face/);
 h.state.face={normal:{direction:{parameter:'axis'}}};const d=h.definition();assert.deepEqual(d.extent.up_to_face.face,h.state.face);d.extent.up_to_face.face.normal=null;assert(h.state.face.normal);
 h.state.input='';assert.throws(()=>h.definition(),/solid/);
});
