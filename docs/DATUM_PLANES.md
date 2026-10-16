# Datum planes

Schema 165 adds constructed datum planes: planes defined from other planes,
from faces of the model, or from points, rather than by a fixed origin and
normal. Each one is fully determined by its references and its parameter
expressions. It cannot be dragged or left floating; it moves only when a
parameter it reads, a plane it is built on, or a face it reads changes.
Sketches attach to any of them with `datum_plane`, as they attach to plain
`plane` datums.

## Kinds

Each kind is a `DatumKind` variant in the family's `datums` list.

| Kind | Defined by | Resolves to |
|---|---|---|
| `offset_plane` | `base`, `distance` (length) | The base moved `distance` along its normal |
| `angle_plane` | `base`, `hinge` (direction), `angle_radians` | The base turned about `hinge` through its origin |
| `face_plane` | `input`, `face` (face selector), `distance` | The selected planar face, moved `distance` along its outward normal |
| `midplane` | `input`, `first`, `second` (face selectors) | Halfway between two parallel planar faces, with the first face's normal |
| `three_point_plane` | `first`, `second`, `third` (lengths) | Through the three points, normal (second − first) × (third − first) |

A `base` is `xy` (normal +Z), `xz` (normal −Y), `yz` (normal +X), or
`{"datum": id}` for another plane datum of any kind, so planes chain. A
hinge is projected into the base plane; it must not be normal to it.
Positive angles turn the normal toward hinge × normal.

```json
{"id": "lid", "kind": {"face_plane": {
  "input": "block",
  "face": {"normal_aligned": {"direction": {"literal": {"x": …, "y": …, "z": …}}, "minimum_dot": {"literal": …}}},
  "distance": {"parameter": "lid_gap"}}}}
```

## Resolution

`resolve_datum` resolves a datum through its chain of bases. Planes that
need no geometry (offset and angle planes on principal or such planes,
three-point planes) resolve from parameters alone, so assembly mates and
drawings can use them. Planes that read faces resolve during regeneration,
against the outputs generated so far, the same way face-supported sketches
do. Elsewhere they fail with a clear message rather than guessing.

Within one regeneration each plane resolves once and is cached, so n
sketches on a chain d planes deep cost O(n + d) plane constructions, not
O(n × d). The scale bench regenerates 500 sketches on a 500-deep chain over
one face, then edits the face's height, in about 0.3 s (2 s budget).

## Dependencies and rebuilds

A sketch on a plane depends on every feature output its plane chain reads,
including named references used by its face selectors. Those outputs are
ordered before the sketch, and the sketch rebuilds when any of them does.
Its feature signature includes the whole chain's definitions and the
parameters they read, so editing a plane's distance rebuilds only the
sketches on it and what follows them.

## Validation

- Every base names an existing plane datum; chains that come back to
  themselves are rejected.
- Face planes and midplanes name an existing feature output, and their
  named references must exist and select faces.
- Expressions are checked with the family defaults; faces are stood in by a
  unit plane, so validation needs no geometry.
- At regeneration, a face selector must pick exactly one planar face, and
  midplane faces must be parallel. Three-point planes reject coincident or
  collinear points.

## Sketch frames

A sketch's `x_axis` is projected into its plane (datum or face support)
instead of having to lie in it exactly, so a sketch keeps a frame when its
plane turns with a parameter. An x axis along the plane's normal is still
rejected. Earlier documents, whose axes already lie in their planes, are
unchanged.

## Editing

The model command edits datums with `add_datum`, `replace_datum` and
`remove_datum` (with expected-snapshot guards). The view data lists each
plane datum of an instance under `datum_planes`, with its definition, status,
`origin_mm`, `normal`, and the in-plane `x_axis` and `y_axis` new sketches
take; a failed plane carries its error, and so does every sketch on it.

The [sketch studio](SKETCH_STUDIO.md#planes) creates the five kinds from the
view: a base plane and value, a picked face or two, or three picked or typed
points, with a live preview. Each driving value becomes a family parameter
named after the plane (`<plane>_distance`, `<plane>_angle` in radians and
shown in degrees, `<plane>_p1`…), which its inspector edits.
