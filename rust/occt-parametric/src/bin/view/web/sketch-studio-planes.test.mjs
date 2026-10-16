import test from 'node:test';
import assert from 'node:assert/strict';
import {
  DEGREE, PLANE_ICONS, PLANE_TYPES, baseOf, describePlane, drivingParameters, freeName, pickPoint, planeEdit, planeOutline, planeType,
} from './sketch-studio-planes.mjs';
import { toGltf, directionToGltf } from './sketch-studio-plane.mjs';

const face = (input, z) => ({
  face_support: { input, face: { normal_aligned: { direction: { literal: {} }, minimum_dot: { literal: {} }, z } } },
  normal: [0, 0, z > 0 ? 1 : -1],
});

test('every plane type has an icon', () => {
  assert.equal(PLANE_TYPES.length, 5);
  for (const type of PLANE_TYPES) assert.match(PLANE_ICONS[type.id], /<(path|circle)/, type.id);
});

test('offset and angle planes are driven by new parameters named after the plane', () => {
  const offset = planeEdit('offset', { name: 'lid', base: 'xz', distance: 12 });
  assert.deepEqual(offset.datum, { id: 'lid', kind: { offset_plane: { base: 'xz', distance: { parameter: 'lid_distance' } } } });
  assert.deepEqual(offset.parameters[0].default, { scalar: { value: 12, dimension: 'length', unit: 'millimeter' } });
  const planes = [{ id: 'lid', status: 'resolved', origin_mm: [0, 5, 0], x_axis: [1, 0, 0], y_axis: [0, 0, 1] }];
  const angle = planeEdit('angle', { name: 'tilt', base: 'datum:lid', angle: 30, hinge: 'y' }, planes, ['tilt_angle']);
  const body = angle.datum.kind.angle_plane;
  assert.deepEqual(body.base, { datum: 'lid' });
  assert.deepEqual(Object.values(body.hinge.literal).map((q) => q.value), [0, 0, 1]);
  assert.deepEqual(body.angle_radians, { parameter: 'tilt_angle-2' });
  assert.ok(Math.abs(angle.parameters[0].default.scalar.value - Math.PI / 6) < 1e-12);
  assert.throws(() => planeEdit('angle', { name: 'x', base: 'datum:gone', angle: 1 }, planes), /base plane/);
  assert.throws(() => planeEdit('offset', { name: ' lid', base: 'xy', distance: 1 }), /surrounding spaces/);
  assert.throws(() => planeEdit('offset', { name: 'lid', base: 'xy', distance: NaN }), /finite/);
});

test('face planes and midplanes read picked faces of one solid', () => {
  const on = planeEdit('face', { name: 'top', faces: [face('body', 1)], distance: 0 });
  assert.equal(on.datum.kind.face_plane.input, 'body');
  assert.deepEqual(on.datum.kind.face_plane.distance, { parameter: 'top_distance' });
  const middle = planeEdit('midplane', { name: 'mid', faces: [face('body', 1), face('body', -1)] });
  assert.equal(middle.parameters.length, 0);
  assert.equal(middle.datum.kind.midplane.second.normal_aligned.z, -1);
  assert.throws(() => planeEdit('midplane', { name: 'mid', faces: [face('body', 1)] }), /second face/);
  assert.throws(() => planeEdit('midplane', { name: 'mid', faces: [face('a', 1), face('b', -1)] }), /same solid/);
});

test('three-point planes make one point parameter per point', () => {
  const edit = planeEdit('three_point', { name: 'slope', points: [[0, 0, 0], [10, 0, 0], [0, 10, 10]] });
  assert.deepEqual(edit.datum.kind.three_point_plane.third, { parameter: 'slope_p3' });
  assert.deepEqual(edit.parameters.map((p) => p.parameter_type), [{ vector: 'length' }, { vector: 'length' }, { vector: 'length' }]);
  assert.equal(edit.parameters[2].default.vector.z.value, 10);
  assert.throws(() => planeEdit('three_point', { name: 's', points: [[0, 0, 0], null, null] }), /three points/);
});

test('driving parameters show angles in degrees and describe each kind', () => {
  const kind = { angle_plane: { base: 'xy', hinge: {}, angle_radians: { parameter: 'a' } } };
  const [driver] = drivingParameters(kind);
  assert.equal(driver.id, 'a');
  assert.ok(Math.abs(driver.toView(Math.PI / 2) - 90) < 1e-12);
  assert.ok(Math.abs(driver.fromView(180) - Math.PI) < 1e-12);
  assert.equal(planeType(kind), 'angle');
  assert.equal(describePlane({ kind }), 'Angle from XY');
  assert.equal(describePlane({ kind: { offset_plane: { base: { datum: 'lid' } } } }), 'Offset from lid');
  assert.equal(drivingParameters({ three_point_plane: { first: { parameter: 'p' }, second: { literal: {} }, third: { parameter: 'q' } } }).length, 2);
  assert.equal(freeName('a', ['a', 'a-2']), 'a-3');
  assert.deepEqual(baseOf('yz').frame.x_axis, [0, 1, 0]);
  assert.ok(Math.abs(DEGREE * 180 - Math.PI) < 1e-15);
});

test('picked points snap to the nearest corner of the hit triangle', () => {
  const triangles = [{ face: 0, points: [[0, 0, 0], [10, 0, 0], [0, 10, 0]] }];
  const down = (x, y) => ({ origin: toGltf([x, y, 50]), direction: directionToGltf([0, 0, -1]) });
  const hit = pickPoint(triangles, down(3, 3));
  assert.ok(Math.abs(hit[0] - 3) < 1e-9 && Math.abs(hit[1] - 3) < 1e-9 && Math.abs(hit[2]) < 1e-9);
  assert.deepEqual(pickPoint(triangles, down(9.5, 0.2), 1), [10, 0, 0]);
  assert.equal(pickPoint(triangles, down(20, 20)), null);
});

test('plane outlines are squares around the model centre dropped onto the plane', () => {
  // An orthographic-like identity matrix: x and y map straight to screen.
  const identity = [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1];
  const plane = { origin_mm: [0, 0, 0], x_axis: [1, 0, 0], y_axis: [0, 1, 0] };
  const path = planeOutline(plane, 100, identity, 200, 200, [100, 0, 300]);
  // Centre (100, 0) mm, half 100 mm: x from 0 to 0.2 m in glTF.
  assert.match(path, /^M100\.0 100\.0L120\.0 100\.0L120\.0 /);
});
