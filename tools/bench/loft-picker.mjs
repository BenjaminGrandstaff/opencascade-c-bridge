import assert from 'node:assert/strict';
import {performance} from 'node:perf_hooks';

// Maximum loft split snapshot: 128 edges × 65 samples, with no native hover work.
{
  const {createLoftAnchorPicker} = await import('../../rust/occt-parametric/src/bin/view/web/sketch-studio-loft-pick.mjs');
  // Face the family XY plane so center hovers test all segments, away from corners.
  const matrix = [1,0,0,0,0,0,1,0,0,-1,0,0,0,0,0,1];
  const ring = Array.from({length:128},(_,i)=>[500*Math.cos(i*Math.PI/64),500*Math.sin(i*Math.PI/64),0]);
  const lines = ring.map((a,i)=>Array.from({length:65},(_,j)=>a.map((v,k)=>v+(ring[(i+1)%128][k]-v)*j/64)));
  let requests=0;
  const context={matrix,width:1000,height:1000,element:(tag,attrs)=>({tag,attrs})};
  const studio={version:1,instance:'part',tools:new Map(),redraw(){},say(){},viewport:()=>context,decorate:()=>[],api:async()=>{
    requests++;
    return {version:1,instance:'part',profile:'section',purpose:'split',split_count:0,accepted:true,coordinate_system:'family-local mm',continuous:true,origin_mm:[0,0,0],normal:[0,0,1],vertices:ring,lines};
  }};
  const picker=createLoftAnchorPicker(studio,()=>{}),pending=picker.pick('section',()=>true,[],'split');
  for(let i=0;i<8;i++)await Promise.resolve();
  assert(picker.source);
  let started=performance.now();
  for(let i=0;i<200;i++)studio.tools.get('solid-loft-anchor').move({clientX:500+i%10,clientY:500});
  const moveTime=(performance.now()-started)/200;
  started=performance.now();let nodes;
  for(let i=0;i<20;i++)nodes=studio.decorate(context);
  const drawTime=(performance.now()-started)/20;
  assert.equal(requests,1);assert.equal(nodes.length,256);
  assert(nodes.every(n=>n.attrs['pointer-events']==='none'));
  picker.cancel();assert.equal(await pending,null);
  console.log(`Studio loft split picker: 128 edges / 8,320 samples, ${moveTime.toFixed(2)} ms per hover, ${drawTime.toFixed(1)} ms per redraw, one snapshot request (budgets 16/100 ms)`);
  assert(moveTime<16&&drawTime<100,'Studio loft split picker exceeded its scale budget.');
}
