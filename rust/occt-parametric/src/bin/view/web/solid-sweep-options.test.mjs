import test from 'node:test';
import assert from 'node:assert/strict';
import {readSweepOptions,sweepOptionsDefinition} from './solid-sweep-options.mjs';

const scalar=value=>({literal:{value,dimension:'scalar',unit:null}});
const features=[
  {id:'inlet',operation:{sketch_face:{sketch:{}}}},
  {id:'outlet',operation:{sketch_face:{sketch:{}}}},
  {id:'route',operation:{sketch_open_wire:{sketch:{}}}},
  {id:'rail',operation:{sketch_open_wire:{sketch:{}}}},
];
const context={input:'inlet',path:'route',orientation:'corrected_frenet'};

test('saved twists, scales, guides and sections read back and write unchanged',()=>{
  const sweep={profile:'inlet',path:'route',orientation:'corrected_frenet',
    twist:{angle_radians:{parameter:'twist'},law:'smooth'},
    sections:[{profile:'outlet',position:scalar(1)}]};
  const state=readSweepOptions(sweep);
  assert.equal(state.twist.source,'parameter:twist');
  assert.deepEqual(sweepOptionsDefinition(state,features,context),{twist:sweep.twist,sections:sweep.sections});
  // A kept formula, a guide that scales, and nothing at all.
  const formula={multiply:[{parameter:'k'},scalar(2)]};
  const scaled=readSweepOptions({scale:{end:formula}});
  assert.equal(scaled.scale.source,'preserve');
  assert.deepEqual(sweepOptionsDefinition(scaled,features,context),{scale:{end:formula,law:'linear'}});
  const guided=readSweepOptions({guide:{input:'rail',scales:true}});
  assert.deepEqual(sweepOptionsDefinition(guided,features,context),{guide:{input:'rail',scales:true}});
  assert.deepEqual(sweepOptionsDefinition(readSweepOptions(),features,context),{});
});

test('entered values become radians and factors, and conflicts are refused',()=>{
  const state=readSweepOptions();
  state.twist.source='literal';state.twist.degrees=180;
  state.scale.source='literal';state.scale.end=1.5;state.scale.law='smooth';
  const fields=sweepOptionsDefinition(state,features,context);
  assert.ok(Math.abs(fields.twist.angle_radians.literal.value-Math.PI)<1e-15);
  assert.deepEqual(fields.scale,{end:scalar(1.5),law:'smooth'});
  const refused=(change,pattern,orientation='corrected_frenet')=>{
    const s=readSweepOptions();change(s);
    assert.throws(()=>sweepOptionsDefinition(s,features,{...context,orientation}),pattern);
  };
  refused(s=>{s.twist.source='literal';s.twist.degrees=0;},/nonzero twist/);
  refused(s=>{s.twist.source='literal';},/least-twist/,'fixed');
  refused(s=>{s.twist.source='literal';s.guide.input='rail';},/remove the twist or the guide/);
  refused(s=>{s.scale.source='literal';s.scale.end=0;},/positive/);
  refused(s=>{s.guide.input='route';},/separate from the profile and path/);
  refused(s=>{s.sections=[{profile:'outlet',position:0.5,saved:null}];},/path end/);
  refused(s=>{s.sections=[{profile:'outlet',position:0.6,saved:null},{profile:'outlet',position:0.4,saved:null}];},/increase/);
  refused(s=>{s.sections=[{profile:'inlet',position:1,saved:null}];},/separate profile/);
  refused(s=>{s.scale.source='literal';s.sections=[{profile:'outlet',position:1,saved:null}];},/remove the scale/);
  refused(s=>{s.scale.source='literal';s.guide={input:'rail',scales:true};},/scaling guide/);
});
