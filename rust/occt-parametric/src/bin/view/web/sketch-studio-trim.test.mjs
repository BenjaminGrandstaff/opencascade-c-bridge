import test from 'node:test';
import assert from 'node:assert/strict';
import { continueOutline, openEnds } from './sketch-studio-delete.mjs';
import { crossings, splitAt, trimPiece, trimmed } from './sketch-studio-trim.mjs';

const mm = (value) => ({ literal: { value, dimension: 'length', unit: 'millimeter' } });
const ids = (records) => (records ?? []).map((r) => r.id);

/**
 * A scene for `definition` from authored coordinates `at`: lines sampled
 * straight, circles and arcs round their centers, as the view data does.
 */
function sceneOf(definition, at) {
  const point = (id) => definition.points.find((p) => p.id === id);
  const xy = (id) => at[id] ?? [point(id).x.literal.value, point(id).y.literal.value];
  const points = Object.fromEntries(definition.points.map((p) => [p.id, [...xy(p.id), 0]]));
  const line = (a, b) => Array.from({ length: 32 }, (_, i) => [a[0] + ((b[0] - a[0]) * i) / 31, a[1] + ((b[1] - a[1]) * i) / 31, 0]);
  const round = (c, r, from, sweep) => Array.from({ length: 65 }, (_, i) => {
    const angle = from + (sweep * i) / 64;
    return [c[0] + r * Math.cos(angle), c[1] + r * Math.sin(angle), 0];
  });
  const angle = (c, p) => Math.atan2(p[1] - c[1], p[0] - c[0]);
  const entities = [
    ...(definition.lines ?? []).map((l) => ({ id: l.id, point_ids: [l.start, l.end], points: line(points[l.start], points[l.end]) })),
    ...(definition.circles ?? []).map((c) => {
      const [center, rim] = [points[c.center], points[c.rim]];
      return { id: c.id, point_ids: [c.center, c.rim], points: round(center, Math.hypot(rim[0] - center[0], rim[1] - center[1]), angle(center, rim), 2 * Math.PI) };
    }),
    ...(definition.arcs ?? []).map((a) => {
      const [center, start, end] = [points[a.center], points[a.start], points[a.end]];
      let sweep = angle(center, end) - angle(center, start);
      if (a.clockwise) { while (sweep >= 0) sweep -= 2 * Math.PI; } else { while (sweep <= 0) sweep += 2 * Math.PI; }
      return { id: a.id, point_ids: [a.center, a.start, a.end], points: round(center, Math.hypot(start[0] - center[0], start[1] - center[1]), angle(center, start), sweep) };
    }),
  ];
  return { points, entities };
}

/** A 20 × 10 rectangle outline with a radius-3 circle on its bottom edge. */
function notch() {
  const p = (id, x, y) => ({ id, x: mm(x), y: mm(y), fixed: false });
  return {
    id: 's',
    points: [p('A', 0, 0), p('B', 20, 0), p('C', 20, 10), p('D', 0, 10), p('O', 10, 0), p('R', 13, 0)],
    lines: [{ id: 'AB', start: 'A', end: 'B' }, { id: 'BC', start: 'B', end: 'C' }, { id: 'CD', start: 'C', end: 'D' }, { id: 'DA', start: 'D', end: 'A' }],
    circles: [{ id: 'cut', center: 'O', rim: 'R' }],
    constraints: [{ horizontal: { line: 'AB' } }, { radius: { curve: 'cut', value: mm(3) } }],
    profile: ['AB', 'BC', 'CD', 'DA'],
  };
}
const ends = (d) => new Map([...(d.lines ?? []), ...(d.arcs ?? [])].map((r) => [r.id, [r.start, r.end]]));

test('crossings skip curves that only touch at an end', () => {
  const d = notch();
  const scene = sceneOf(d, {});
  const bottom = crossings(scene, 'AB', d);
  assert.deepEqual(bottom.map((c) => c.other), ['cut', 'cut']);
  assert.ok(Math.abs(bottom[0].at[0] - 7) < 1e-9 && Math.abs(bottom[1].at[0] - 13) < 1e-9);
  assert.deepEqual(crossings(scene, 'BC', d), [], 'AB and CD only touch its ends');
  assert.ok(!crossings(scene, 'AB', d).some((c) => c.point === 'O'), 'a center on the line is not a crossing');
});

test('trimming a notch: the edge inside the circle, then the circle outside the outline', () => {
  const d = notch();
  const scene = sceneOf(d, {});
  assert.equal(trimPiece(d, scene, 'AB', [10, 0.1]).kind, 'middle');
  const first = trimmed(d, scene, 'AB', [10, 0.1]).definition;
  assert.equal(d.lines.length, 4, 'the input is unchanged');
  const [left, right] = [first.lines.find((l) => l.id === 'AB'), first.lines.find((l) => l.id === 'L1')];
  assert.equal(left.start, 'A');
  assert.equal(right.end, 'B');
  assert.equal(right.start, 'R', 'the circle\'s rim point is already at the crossing');
  assert.deepEqual(first.constraints.slice(2), [
    { point_on_curve: { point: left.end, curve: 'cut' } },
    { collinear: { first: 'AB', second: 'L1' } },
  ], 'the rim needs no point-on-curve constraint');
  assert.deepEqual(first.profile, ['L1', 'BC', 'CD', 'DA', 'AB']);
  assert.deepEqual(openEnds(first.profile, ends(first)), [right.start, left.end]);
  // The circle's lower span is outside the outline; trimming it leaves the
  // upper arc, which ends at the same two points and closes the outline.
  const after = sceneOf(first, {});
  const piece = trimPiece(first, after, 'cut', [10, -3]);
  assert.equal(piece.kind, 'span');
  const second = trimmed(first, after, 'cut', [10, -3]).definition;
  assert.deepEqual(ids(second.circles), []);
  const arc = second.arcs.find((a) => a.id === 'cut');
  assert.deepEqual([arc.start, arc.end, arc.clockwise], [left.end, right.start, true], 'over the top, from 7 to 13, with the loop');
  assert.equal(second.points.length, first.points.length, 'the rim point still ends L1 and now the arc');
  assert.ok(second.constraints.some((c) => c.radius?.curve === 'cut'), 'the radius now dimensions the arc');
  assert.ok(!second.constraints.some((c) => c.point_on_curve), 'P1 now ends the arc, so its hold on the circle is implied');
  const closed = second;
  assert.deepEqual(closed.profile, ['L1', 'BC', 'CD', 'DA', 'AB', 'cut'], 'the arc joins the open outline');
  assert.equal(openEnds(closed.profile, ends(closed)), null);
  assert.equal(closed.arcs[0].start, 'P1', 'the arc runs with the loop');
});

test('an end piece moves that end onto the crossing curve and drops its unused point', () => {
  const d = notch();
  d.points.push({ id: 'S', x: mm(5), y: mm(5), fixed: false }, { id: 'T', x: mm(25), y: mm(5), fixed: true });
  d.lines.push({ id: 'spur', start: 'S', end: 'T' });
  d.constraints.push({ distance: { first: 'S', second: 'T', value: mm(20) } });
  const scene = sceneOf(d, {});
  const piece = trimPiece(d, scene, 'spur', [23, 5]);
  assert.equal(piece.kind, 'end');
  assert.ok(Math.abs(piece.from.at[0] - 20) < 1e-9);
  const next = trimmed(d, scene, 'spur', [23, 5]).definition;
  const spur = next.lines.find((l) => l.id === 'spur');
  assert.equal(spur.start, 'S');
  assert.ok(!next.points.some((p) => p.id === 'T'), 'T is no longer used');
  assert.ok(!next.constraints.some((c) => c.distance), 'the distance to T went with it');
  assert.deepEqual(next.constraints.at(-1), { point_on_curve: { point: spur.end, curve: 'BC' } });
  assert.equal(trimPiece(d, scene, 'spur', [8, 5]).kind, 'start', 'the part inside up to BC is the start piece');
});

test('a curve nothing crosses is deleted, and other curves are refused', () => {
  const d = notch();
  d.points.push({ id: 'U', x: mm(30), y: mm(0), fixed: false }, { id: 'V', x: mm(30), y: mm(10), fixed: false });
  d.lines.push({ id: 'post', start: 'U', end: 'V' });
  const scene = sceneOf(d, {});
  const result = trimmed(d, scene, 'post', [30, 5]);
  assert.deepEqual(ids(result.definition.lines), ['AB', 'BC', 'CD', 'DA']);
  assert.match(result.summary, /crosses nothing/);
  assert.match(trimPiece(d, scene, 'box:side0', [0, 0]).unsupported, /not a line, arc or circle/);
});

test('split cuts a line in two at a shared point, collinear and in loop order', () => {
  const d = notch();
  d.constraints.push({ tangent: { first: 'AB', second: 'BC', point: 'B' } });
  const scene = sceneOf(d, {});
  const { definition: next, point } = splitAt(d, scene, 'AB', [4, 0.2]);
  assert.deepEqual(next.lines.filter((l) => ['AB', 'L1'].includes(l.id)), [{ id: 'AB', start: 'A', end: point }, { id: 'L1', start: point, end: 'B' }]);
  assert.deepEqual(next.profile, ['AB', 'L1', 'BC', 'CD', 'DA']);
  assert.equal(openEnds(next.profile, ends(next)), null, 'still closed');
  assert.ok(next.constraints.some((c) => c.collinear?.second === 'L1'));
  assert.ok(next.constraints.some((c) => c.tangent?.first === 'L1' && c.tangent.point === 'B'), 'the tangent at B moves to the piece ending there');
  assert.throws(() => splitAt(d, scene, 'AB', [0.1, 0]), /away from its ends/);
  assert.throws(() => splitAt(d, scene, 'cut', [13, 0]), /lines and arcs/);
  // Arcs split into two arcs on the same center; their shared end keeps one radius.
  const arcs = trimmed(d, scene, 'cut', [10, -3]).definition;
  const { definition: halves } = splitAt(arcs, sceneOf(arcs, {}), 'cut', [10, 3]);
  assert.deepEqual(halves.arcs.map((a) => a.center), ['O', 'O']);
  assert.equal(halves.arcs[0].end, halves.arcs[1].start);
});

test('crossings stay linear in the number of curves', () => {
  const d = { id: 's', points: [], lines: [], constraints: [], profile: [] };
  const p = (id, x, y) => ({ id, x: mm(x), y: mm(y), fixed: false });
  d.points.push(p('a', 0, 0), p('b', 10000, 0));
  d.lines.push({ id: 'long', start: 'a', end: 'b' });
  for (let i = 0; i < 10000; i += 1) {
    d.points.push(p(`u${i}`, i + 0.5, -1), p(`v${i}`, i + 0.5, 1));
    d.lines.push({ id: `x${i}`, start: `u${i}`, end: `v${i}` });
  }
  const scene = sceneOf(d, {});
  const started = performance.now();
  assert.equal(crossings(scene, 'long').length, 10000);
  assert.equal(trimPiece(d, scene, 'long', [5000.2, 0]).kind, 'middle');
  assert.ok(performance.now() - started < 1000, `${performance.now() - started} ms`);
});

test('trim and split drop constraints on the old extent and keep directions', () => {
  const d = notch();
  d.constraints.push({ midpoint: { point: 'O', line: 'AB' } }, { curve_length: { curve: 'AB', value: mm(20) } });
  const scene = sceneOf(d, {});
  const result = trimmed(d, scene, 'AB', [10, 0.1]);
  assert.ok(!result.definition.constraints.some((c) => c.midpoint || c.curve_length));
  assert.ok(result.definition.constraints.some((c) => c.horizontal?.line === 'AB'));
  assert.match(result.summary, /2 constraints on its old length went/);
  const split = splitAt(d, scene, 'AB', [4, 0]);
  assert.ok(!split.definition.constraints.some((c) => c.midpoint));
  assert.match(split.summary, /2 constraints/);
});

test('a curve ending on a coarsely sampled circle still bounds a trim of it', () => {
  // Rim below the center: 32 chords never pass through the crossings at (±3, 0).
  const d = notch();
  d.points.find((p) => p.id === 'R').x = mm(10);
  d.points.find((p) => p.id === 'R').y = mm(-3);
  const first = trimmed(d, sceneOf(d, {}), 'AB', [10, 0.1]).definition;
  const coarse = (definition) => {
    const scene = sceneOf(definition, {});
    const circle = scene.entities.find((e) => e.id === 'cut');
    circle.points = circle.points.filter((_, i) => i % 2 === 0).slice(0, 32);
    circle.points.push(circle.points[0]);
    return scene;
  };
  const scene = coarse(first);
  const cuts = crossings(scene, 'cut', first);
  assert.equal(cuts.length, 2);
  assert.deepEqual(cuts.map((c) => c.point).sort(), ['P1', 'P2']);
  const second = trimmed(first, scene, 'cut', [10, -3]).definition;
  const arc = second.arcs.find((a) => a.id === 'cut');
  assert.deepEqual([arc.start, arc.end].sort(), ['P1', 'P2'], 'the arc ends on the trimmed edge ends');
  assert.equal(openEnds(continueOutline(first, second).profile, ends(continueOutline(first, second))), null);
});
