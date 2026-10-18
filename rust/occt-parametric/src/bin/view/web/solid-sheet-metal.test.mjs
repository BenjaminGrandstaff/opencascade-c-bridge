import test from 'node:test';
import assert from 'node:assert/strict';
import {readSheetMetal,sheetMetalOperations} from './solid-sheet-metal.mjs';

const length=value=>({literal:{value,dimension:'length',unit:'millimeter'}});
const scalar=value=>({literal:{value,dimension:'scalar',unit:null}});

test('a new part is an L bracket with a linked flat blank',()=>{
  const state=readSheetMetal(null);
  const {sheet,flat}=sheetMetalOperations(state,'bracket');
  const d=sheet.sheet_metal.definition;
  assert.deepEqual(d.flanges,[length(40),length(30)]);
  assert.ok(Math.abs(d.bends[0].angle_radians.literal.value-Math.PI/2)<1e-15);
  assert.deepEqual(d.width,length(30));assert.equal(d.edge_flanges,undefined);
  assert.deepEqual(flat,{id:'bracket-flat',operation:{sheet_metal_flat:{input:'bracket',neutral_factor:scalar(0.44)}}});
  state.flat.enabled=false;assert.equal(sheetMetalOperations(state,'bracket').flat,undefined);
});

test('saved parts with hems, edge flanges, cutouts and formulas round-trip',()=>{
  const definition={
    origin:{literal:{x:length(0),y:length(0),z:length(0)}},width_axis:{parameter:'across'},start_direction:{literal:{x:scalar(1),y:scalar(0),z:scalar(0)}},
    width:{parameter:'w'},thickness:length(2),flanges:[length(20),{multiply:[{parameter:'w'},scalar(2)]},length(20)],
    bends:[{angle_radians:scalar(Math.PI/2),inside_radius:length(3)},{angle_radians:scalar(-Math.PI/2),inside_radius:length(3)}],
    cutouts:[{id:'hole',flange:1,along:length(10),across:length(5),shape:{kind:'slot',length:length(8),width:length(3)}}],
    start_hem:{kind:'teardrop',side:'negative',inside_radius:length(2),angle_radians:scalar(5.03)},
    end_hem:{kind:'closed',side:'positive',length:length(6)},
    edge_flanges:[{id:'wall',flange:1,edge:'far',start:length(0),length:length(20),angle_radians:scalar(Math.PI/2),inside_radius:length(1),height:length(8),relief_depth:length(4)}],
  };
  const flat={id:'blank',operation:{sheet_metal_flat:{input:'part',neutral_factor:{parameter:'k'},bend_table:{name:'AL'}}}};
  const state=readSheetMetal(definition,flat);
  const out=sheetMetalOperations(state,'part');
  assert.deepEqual(out.sheet,{sheet_metal:{definition}});
  assert.deepEqual(out.flat,flat);
  // A changed value is written fresh; others stay as saved.
  state.flanges[0].value=25;
  assert.deepEqual(sheetMetalOperations(state,'part').sheet.sheet_metal.definition.flanges[0],length(25));
});

test('sheet-metal values are checked before preview',()=>{
  const bad=(change,pattern)=>{const s=readSheetMetal(null);change(s);assert.throws(()=>sheetMetalOperations(s,'part'),pattern);};
  bad(s=>s.flanges[0].value=0,/flange 1 length/);
  bad(s=>s.bends[0].angle.value=180,/bend 1 angle/);
  bad(s=>{s.endHem.kind='teardrop';s.endHem.angle.value=170;},/end hem angle/);
  bad(s=>s.flat.k.value=1.5,/neutral factor/);
  bad(s=>s.widthAxis.values=[0,0,0],/width direction/);
  bad(s=>{s.cutouts.push({id:'a',flange:0,along:{kind:'length',source:'literal',value:5},across:{kind:'length',source:'literal',value:5},shape:'circle',diameter:{kind:'length',source:'literal',value:2}});s.cutouts.push({...s.cutouts[0]});},/unique/);
  bad(s=>{s.flat.id='part';},/different/);
});
