import test from 'node:test';
import assert from 'node:assert/strict';
import {createThreadControls} from './solid-hole-threads.mjs';

const q=(value,unit)=>({value,dimension:'length',unit});
const sizes={
  metric:[{label:'M6x1',nominal:q(6,'millimeter'),pitch:q(1,'millimeter'),drill_mm:5}],
  inch:[{label:'1/4-20',series:'unc',designation:'1/4-20 UNC-2B',nominal:q(0.25,'inch'),pitch:q(0.05,'inch'),drill_mm:5.1054}],
};

test('new threads name catalog sizes, follow the tap drill and may be cut',()=>{
  const metric=createThreadControls(null,sizes);
  assert.equal(metric.state.mode,'none');assert.deepEqual(metric.definition(),{thread:null});
  metric.state.mode='metric';metric.state.hand='left';metric.state.modeled=true;metric.state.length=8;
  const {thread,diameter}=metric.definition();
  assert.equal(thread.designation,'M6x1-LH');assert.equal(thread.handedness,'left');
  assert.deepEqual(thread.modeled,{length:{literal:q(8,'millimeter')}});
  assert.deepEqual(diameter,{carr_lane_tap_drill_v1:{nominal_diameter:{literal:q(6,'millimeter')},pitch:{literal:q(1,'millimeter')},system:'metric'}});
  const inch=createThreadControls(null,sizes);inch.state.mode='inch';inch.state.class='1B';
  const unified=inch.definition().thread;
  assert.equal(unified.designation,'1/4-20 UNC-1B');assert.deepEqual(unified.unified,{series:'unc',class:'1B'});
  assert.equal(unified.modeled,undefined);
  inch.state.drill='keep';assert.equal(inch.definition().diameter,undefined);
  inch.state.modeled=true;inch.state.length=0;assert.throws(()=>inch.definition(),/positive/);
});

test('saved threads read back as catalog sizes, or are kept as they are',()=>{
  const saved={designation:'1/4-20 UNC-2B-LH',nominal_diameter:{literal:q(6.35,'millimeter')},pitch:{literal:q(1.27,'millimeter')},handedness:'left',unified:{series:'unc',class:'2B'}};
  const controls=createThreadControls(saved,sizes);
  assert.deepEqual([controls.state.mode,controls.state.hand,controls.state.class,controls.state.drill],['inch','left','2B','keep']);
  assert.equal(controls.definition().thread.designation,'1/4-20 UNC-2B-LH');
  const custom={designation:'M7x1 special',nominal_diameter:{literal:q(7,'millimeter')},pitch:{literal:q(1,'millimeter')},handedness:'right'};
  const kept=createThreadControls(custom,sizes);
  assert.equal(kept.state.mode,'preserve');assert.deepEqual(kept.definition(),{thread:undefined});
});
