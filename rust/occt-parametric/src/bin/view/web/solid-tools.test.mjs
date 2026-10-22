import test from 'node:test';
import assert from 'node:assert/strict';
import {readSolidTool,solidToolOperation,solidToolKind,inPlaceTool,isSolidTool} from './solid-tools.mjs';

const features=[{id:'body',operation:{extrude:{}}},{id:'other',operation:{extrude:{}}}];
const face={normal_aligned:{direction:{literal:{x:{value:0,dimension:'scalar',unit:null},y:{value:0,dimension:'scalar',unit:null},z:{value:1,dimension:'scalar',unit:null}}},minimum_dot:{literal:{value:0.999,dimension:'scalar',unit:null}}}};
const length=value=>({literal:{value,dimension:'length',unit:'millimeter'}});
const scalar=value=>({literal:{value,dimension:'scalar',unit:null}});

test('shells open picked faces with walls inside or outside, keeping formulas',()=>{
  const state=readSolidTool('hollow',null,'body');
  assert.equal(state.side,'inside');assert.equal(state.thickness.value,2);
  assert.throws(()=>solidToolOperation('hollow',state,features),/at least one face/);
  state.faces.push(face);
  const inside=solidToolOperation('hollow',state,features).hollow;
  assert.deepEqual(inside.thickness,length(-2));assert.deepEqual(inside.faces,[face]);assert.equal(inside.input,'body');
  state.side='outside';state.thickness.value=1.5;
  assert.deepEqual(solidToolOperation('hollow',state,features).hollow.thickness,length(1.5));
  // A parameter inside negates it; reading it back keeps the side.
  state.side='inside';state.thickness.source='parameter:wall';
  const linked=solidToolOperation('hollow',state,features).hollow;
  assert.deepEqual(linked.thickness,{negate:{parameter:'wall'}});
  const reread=readSolidTool('hollow',linked);
  assert.equal(reread.side,'inside');assert.equal(reread.thickness.source,'parameter:wall');
  assert.deepEqual(solidToolOperation('hollow',reread,features).hollow,linked);
  state.thickness.source='literal';state.thickness.value=0;
  assert.throws(()=>solidToolOperation('hollow',state,features),/wall thickness/);
});

test('drafts, mirrors and patterns write the engine operations and refuse bad values',()=>{
  const draft=readSolidTool('draft',null,'body');draft.faces.push(face);
  const op=solidToolOperation('draft',draft,features).draft;
  assert.ok(Math.abs(op.angle_radians.literal.value-3*Math.PI/180)<1e-15);
  assert.equal(op.neutral_normal.components.z.literal.value,1);
  draft.angle.value=90;assert.throws(()=>solidToolOperation('draft',draft,features),/draft angle/);
  const mirror=readSolidTool('mirror',null,'body');mirror.normal.values=[0,3,0];
  assert.equal(solidToolOperation('mirror',mirror,features).mirror.normal.components.y.literal.value,1);
  mirror.normal.values=[0,0,0];assert.throws(()=>solidToolOperation('mirror',mirror,features),/nonzero/);
  const linear=readSolidTool('linear_pattern',null,'body');linear.direction=[0,2,0];linear.spacing.value=15;linear.count.value=4;
  const step=solidToolOperation('linear_pattern',linear,features).linear_pattern;
  assert.deepEqual([step.step.components.y.literal.value,step.count.literal.value],[15,4]);
  assert.deepEqual(readSolidTool('linear_pattern',step).direction,[0,1,0]);
  linear.count.value=1.5;assert.throws(()=>solidToolOperation('linear_pattern',linear,features),/whole number/);
  const circular=readSolidTool('circular_pattern',null,'body');
  const ring=solidToolOperation('circular_pattern',circular,features).circular_pattern;
  assert.ok(Math.abs(ring.angle_step_radians.literal.value-Math.PI/3)<1e-15);
  circular.count.source='parameter:n';
  assert.throws(()=>solidToolOperation('circular_pattern',circular,features),/entered count/);
  circular.spacing='step';circular.angle.value=30;
  assert.ok(Math.abs(solidToolOperation('circular_pattern',circular,features).circular_pattern.angle_step_radians.literal.value-Math.PI/6)<1e-15);
  // Saved formulas round-trip untouched.
  const saved={input:'body',origin:{parameter:'pivot'},normal:{literal:{x:scalar(1),y:scalar(0),z:scalar(0)}}};
  assert.deepEqual(solidToolOperation('mirror',readSolidTool('mirror',saved),features).mirror.origin,{parameter:'pivot'});
  assert.throws(()=>solidToolOperation('mirror',readSolidTool('mirror',null,'gone'),features),/source solid/);
  assert.equal(solidToolKind({circular_pattern:{}}),'circular_pattern');
  assert.ok(inPlaceTool('hollow')&&!inPlaceTool('mirror')&&isSolidTool('draft')&&!isSolidTool('extrude'));
});

test('face edits delete, offset and move picked faces and refuse empty edits',()=>{
  const wall={nearest_center:{target:{literal:{x:length(10),y:length(15),z:length(10)}},maximum_distance:length(1e-5)}};
  for(const kind of ['remove_faces','offset_faces','move_faces']){
    assert.ok(isSolidTool(kind)&&inPlaceTool(kind));
    const state=readSolidTool(kind,null,'body');
    assert.throws(()=>solidToolOperation(kind,state,features),/at least one face/);
    state.faces.push(wall);
    const op=solidToolOperation(kind,state,features)[kind];
    assert.equal(op.input,'body');assert.deepEqual(op.faces,[wall]);
    assert.equal(solidToolKind({[kind]:op}),kind);
    // Saved edits read back to the same operation.
    assert.deepEqual(solidToolOperation(kind,readSolidTool(kind,op),features)[kind],op);
  }
  const offset=readSolidTool('offset_faces',null,'body');offset.faces.push(wall);
  assert.deepEqual(solidToolOperation('offset_faces',offset,features).offset_faces.distance,length(1));
  offset.distance.value=-2;
  assert.deepEqual(solidToolOperation('offset_faces',offset,features).offset_faces.distance,length(-2));
  offset.distance.value=0;assert.throws(()=>solidToolOperation('offset_faces',offset,features),/nonzero/);
  offset.distance.source='parameter:push';
  assert.deepEqual(solidToolOperation('offset_faces',offset,features).offset_faces.distance,{parameter:'push'});
  const move=readSolidTool('move_faces',null,'body');move.faces.push(wall);move.offset.values=[0,-4,0];
  assert.equal(solidToolOperation('move_faces',move,features).move_faces.offset.components.y.literal.value,-4);
  move.offset.values=[0,0,0];assert.throws(()=>solidToolOperation('move_faces',move,features),/nonzero move offset/);
});

test('curve, fill and table patterns and skipped copies write the engine operations',()=>{
  const all=[...features,{id:'path',operation:{sketch_open_wire:{sketch:{}}}},{id:'plate',operation:{sketch_face:{sketch:{}}}}];
  // Skip lists on the existing patterns.
  const linear=readSolidTool('linear_pattern',null,'body');linear.count.value=5;linear.skip='3, 1, 3';
  assert.deepEqual(solidToolOperation('linear_pattern',linear,all).linear_pattern.skip,[1,3]);
  linear.skip='';assert.equal(solidToolOperation('linear_pattern',linear,all).linear_pattern.skip,undefined);
  linear.skip='5';assert.throws(()=>solidToolOperation('linear_pattern',linear,all),/below the copy count/);
  linear.skip='-1';assert.throws(()=>solidToolOperation('linear_pattern',linear,all),/whole numbers/);
  // Curve: a path, count, orientation and skips; saved ones read back.
  const curve=readSolidTool('curve_pattern',null,'body');
  assert.throws(()=>solidToolOperation('curve_pattern',curve,all),/Choose the path/);
  Object.assign(curve,{path:'path',orientation:'tangent',skip:'0'});
  const along=solidToolOperation('curve_pattern',curve,all).curve_pattern;
  assert.deepEqual([along.path,along.orientation,along.count.literal.value,along.skip],['path','tangent',5,[0]]);
  assert.deepEqual(solidToolOperation('curve_pattern',readSolidTool('curve_pattern',along),all).curve_pattern,along);
  // Fill: region, grid, margin; a picked face is kept.
  const fill=readSolidTool('fill_pattern',null,'body');
  assert.throws(()=>solidToolOperation('fill_pattern',fill,all),/region to fill/);
  fill.region='plate';fill.layout='staggered';fill.margin.value=3;fill.direction.values=[0,2,0];
  const filled=solidToolOperation('fill_pattern',fill,all).fill_pattern;
  assert.deepEqual([filled.region,filled.layout,filled.margin.literal.value,filled.spacing.literal.value],['plate','staggered',3,10]);
  assert.equal(filled.direction.components.y.literal.value,1);
  assert.equal(filled.face,undefined);
  fill.face={at_extreme:{axis:'z'}};assert.deepEqual(solidToolOperation('fill_pattern',fill,all).fill_pattern.face,{at_extreme:{axis:'z'}});
  fill.spacing.value=0;assert.throws(()=>solidToolOperation('fill_pattern',fill,all),/spacing along rows/);
  // Table: one offset per line.
  const table=readSolidTool('table_pattern',null,'body');
  table.offsets.text='0, 0, 0\n7 0 0\n\n19, 3, 0';
  const listed=solidToolOperation('table_pattern',table,all).table_pattern.offsets;
  assert.deepEqual(listed.map(o=>['x','y','z'].map(k=>o.components[k].literal.value)),[[0,0,0],[7,0,0],[19,3,0]]);
  assert.equal(readSolidTool('table_pattern',{input:'body',offsets:listed}).offsets.text,'0, 0, 0\n7, 0, 0\n19, 3, 0');
  table.offsets.text='1, 2';assert.throws(()=>solidToolOperation('table_pattern',table,all),/line 1 needs three numbers/);
  for(const kind of ['curve_pattern','fill_pattern','table_pattern'])assert.ok(isSolidTool(kind)&&!inPlaceTool(kind));
});
