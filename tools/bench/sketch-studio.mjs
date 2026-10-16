// Sketch studio at 10,000 lines: what each mouse move costs (snapping and
// automatic constraints) and what one redraw of the sketch overlay costs.
import assert from 'node:assert/strict';
import { performance } from 'node:perf_hooks';
import { viewProjection } from '../../rust/occt-parametric/src/bin/view/web/viewer.mjs';
import { basePlane, lookAt } from '../../rust/occt-parametric/src/bin/view/web/sketch-studio-plane.mjs';
import {commitShape,studioGhost} from '../../rust/occt-parametric/src/bin/view/web/sketch-studio-tools.mjs';
import { infer } from '../../rust/occt-parametric/src/bin/view/web/sketch-studio-infer.mjs';
import { sketchPaths, sketchPoints } from '../../rust/occt-parametric/src/bin/view/web/sketch-studio-overlay.mjs';

const N = 10_000;
const mm = (value) => ({ literal: { value, dimension: 'length', unit: 'millimeter' } });
// A 100 × 100 lattice of points joined by 10,000 short lines.
const at = (i) => [(i % 101) * 2, Math.floor(i / 101) * 2];
const definition = {
  points: Array.from({ length: N + 1 }, (_, i) => ({ id: `p${i}`, x: mm(at(i)[0]), y: mm(at(i)[1]), fixed: false })),
  lines: Array.from({ length: N }, (_, i) => ({ id: `l${i}`, start: `p${i}`, end: `p${i + 1}` })),
};
// The server samples each curve at 33 points.
const scene = {
  points: Object.fromEntries(definition.points.map((p, i) => [p.id, [...at(i), 0]])),
  entities: definition.lines.map((line, i) => ({
    id: line.id,
    points: Array.from({ length: 33 }, (_, k) => {
      const [a, b] = [at(i), at(i + 1)];
      const t = k / 32;
      return [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, 0];
    }),
  })),
};

const moves = 50;
// The first move builds the spatial index; later moves reuse it.
let start = performance.now();
infer({ xy: [37.3, 41.1], tool: 'line', definition, scene, tolerance: 0.2, anchor: { xy: [10, 41] } });
const build = performance.now() - start;
start = performance.now();
for (let i = 0; i < moves; i += 1) {
  const result = infer({
    xy: [37.3 + i * 0.01, 41.1], tool: 'line', definition, scene, tolerance: 0.2,
    anchor: { xy: [10, 41] },
  });
  assert.ok(result.xy.every(Number.isFinite));
}
const perMove = (performance.now() - start) / moves;

const frame = basePlane('xy');
const camera = lookAt({ target: [0, 0, 0], distance: 0.4, yaw: 0, pitch: 0 }, frame, [100, 100]);
const matrix = viewProjection(camera, 1);
start = performance.now();
const paths = sketchPaths(scene, frame, matrix, 1000, 1000);
const points = sketchPoints(scene, frame, matrix, 1000, 1000);
const redraw = performance.now() - start;
assert.equal(paths.length, N);
assert.equal(points.length, N + 1);

console.log(`Sketch studio: ${build.toFixed(1)} ms first move (index build), ${perMove.toFixed(2)} ms per later move (snapping and inference); ` +
  `${redraw.toFixed(1)} ms per overlay redraw (10,000 lines, 330,000 samples; budgets 16 ms per move, 500 ms per build and 100 ms per redraw)`);
assert(perMove < 16 && build < 500, 'Studio snapping exceeded its budget (16 ms per move, 500 ms per index build).');
assert(redraw < 100, 'Studio overlay exceeded its redraw budget.');

// Largest explicit pole set; preview remains 65 samples and never calls native geometry.
const clicks=Array.from({length:256},(_,i)=>({xy:[i,Math.sin(i/20)*10],point:null,rules:[]}));
start=performance.now();
for(let i=0;i<20;i++){
 const result=commitShape(definition,'control_spline',clicks,{degree:5});
 assert.equal(result.definition.splines.at(-1).points.length,256);
 const ghost=studioGhost('control_spline',clicks.slice(0,-1),clicks.at(-1),{degree:5});
 assert.equal(ghost.length,65);assert(ghost.flat().every(Number.isFinite));
}
const primitives=performance.now()-start;
console.log(`Studio primitives: 20 maximum-pole spline commits/previews on 10,000 curves ${primitives.toFixed(1)} ms (budget 2,000 ms)`);
assert(primitives<2000,'Maximum-pole studio primitive authoring exceeded its budget.');
