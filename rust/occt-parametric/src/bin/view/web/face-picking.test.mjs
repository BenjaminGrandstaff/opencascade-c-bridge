import {test} from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
const context={window:{},document:{getElementById:()=>null}};
vm.createContext(context);
vm.runInContext(fs.readFileSync(new URL('../../../../../../tools/model/viewer/viewer.js',import.meta.url),'utf8'),context);
const triangle=(face,z)=>({face,points:[[0,0,z],[10,0,z],[0,10,z]]});
test('face picking chooses the nearest visible triangle regardless of mesh order',()=>{
 for(const mesh of [[triangle(1,2),triangle(2,5)],[triangle(2,5),triangle(1,2)]]){
  assert.equal(context.pickOcctFace(mesh,p=>p.slice(0,2),p=>p[2],2,2).face,2);
 }
 assert.equal(context.pickOcctFace([triangle(1,2)],p=>p.slice(0,2),p=>p[2],9,9),null);
 assert.equal(context.pickOcctFace([{face:1,points:[[0,0,0],[0,0,0],[0,0,0]]}],p=>p.slice(0,2),p=>p[2],0,0),null);
});
