import test from 'node:test';
import assert from 'node:assert/strict';
import {validateLoftConstraint} from './solid-loft-constraints.mjs';
test('loft direction modes must be exclusive and normal constraints preserve strength expressions',()=>{
 const magnitude={parameter:'height'},direction={parameter:'axis'};
 assert.deepEqual(validateLoftConstraint({normal_to_profile:true,magnitude}),{normal_to_profile:true,magnitude});
 for(const c of [{magnitude},{direction,normal_to_profile:true,magnitude},{normal_to_profile:false,magnitude},{normal_to_profile:'true',magnitude}])assert.throws(()=>validateLoftConstraint(c),/exactly one/);
 assert.deepEqual(validateLoftConstraint({direction,magnitude}),{direction,magnitude});
});
