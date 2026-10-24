import {test} from 'node:test';
import assert from 'node:assert/strict';
import {templateInputs} from './sketch-studio-templates.mjs';

const shaft={id:'shaft',inputs:[
 {id:'diameter',kind:'length',default:20,minimum:2,maximum:200},
 {id:'material',kind:'choice',default:'stainless_steel_304',choices:['aluminum_6061_t6','stainless_steel_304']},
 {id:'bolt',kind:'length',default:6,allowed:[5,6,8]}]};

test('template inputs send what was entered, leave blanks to defaults and name bad values',()=>{
 assert.deepEqual(templateInputs(shaft,{}),{});
 assert.deepEqual(templateInputs(shaft,{diameter:'25',material:'aluminum_6061_t6',bolt:''}),{diameter:25,material:'aluminum_6061_t6'});
 assert.throws(()=>templateInputs(shaft,{diameter:'1'}),/diameter must be between 2 and 200/);
 assert.throws(()=>templateInputs(shaft,{diameter:'wide'}),/diameter must be a number in millimeters/);
 assert.throws(()=>templateInputs(shaft,{material:'cheese'}),/material must be one of/);
 assert.throws(()=>templateInputs(shaft,{bolt:'7'}),/bolt must be one of 5, 6, 8/);
});
