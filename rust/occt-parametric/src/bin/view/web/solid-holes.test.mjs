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
test('existing recess, relief, angle and thread formulas survive default editing without aliasing',()=>{
 const q=v=>({literal:{value:v,dimension:'length',unit:'millimeter'}}),axis={parameter:'axis'},position={parameter:'entry'},diameter={parameter:'bore'},angle={multiply:[{parameter:'angle'},{literal:{value:2,dimension:'scalar',unit:null}}]};
 for(const finish of [{counterbore:{diameter:q(8),depth:q(1),relief:{parameter:'relief'}}},{countersink:{diameter:q(8),angle_radians:angle}}]){
  const saved={input:'body',axis,position,diameter,extent:{blind:{depth:q(4)}},bottom:{drill_point:{angle_radians:angle}},finish,thread:{designation:'metadata'}};
  const controls=createHoleControls(saved),edited=controls.definition();assert.deepEqual(edited,saved);edited.thread.designation='changed';edited.bottom.drill_point.angle_radians=null;assert.deepEqual(controls.definition(),saved);
  controls.state.finish='plain';assert.equal(controls.definition().finish,'plain');assert.deepEqual(controls.definition().thread,saved.thread);
 }
});
