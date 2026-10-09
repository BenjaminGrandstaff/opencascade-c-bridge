# Mirrored parts

Schema 81 adds `mirror`; native ABI 51 adds plane reflection:

```json
"mirror": {
  "input": "source",
  "origin": {"components": {"x": {"parameter": "plane_x"}, "y": {"literal": {"value": 0, "dimension": "length", "unit": "millimeter"}}, "z": {"literal": {"value": 0, "dimension": "length", "unit": "millimeter"}}}},
  "normal": {"literal": {"x": {"value": 1, "dimension": "scalar", "unit": null}, "y": {"value": 0, "dimension": "scalar", "unit": null}, "z": {"value": 0, "dimension": "scalar", "unit": null}}}
}
```

The plane passes through `origin` (length-valued), with `normal` dimensionless,
finite and nonzero. Normal magnitude and sign do not change the plane.
Normalization first scales by the largest component, avoiding overflow and
underflow for extreme finite direction magnitudes. Invalid planes are rejected.

The feature produces the reflected input geometry. To retain both handed parts,
keep the source output as well, or combine outputs with an existing boolean or
compound workflow. Family datums and assembly placement frames remain separately
defined; this operation transforms geometry. Plane and source parameter changes
enter feature signatures and dependencies. Plane-only edits rebuild the mirror
while reusing its source features.

Native OCCT `BRepBuilderAPI_Transform` handles the negative-determinant reflection.
Unlike proper rigid moves, reflection requires transformed geometry copies.
Source geometry stays unchanged; the result passes session validation/healing
policy. Native modification history maps source faces/edges into the reflected
shape. History targets are canonicalized to actual output topology after healing:
OCCT's modification map can omit a face reversal applied within its result shell.
This keeps history-returned face normals consistent with the reflected solid.

The safe Rust `Session::mirror` wraps `occt_bridge_mirror`. Tests cover axial and
oblique planes, normal sign/magnitude invariance, remote plane origins, mass and
bounds, double-mirror recovery, outward normals, history, source immutability,
invalid input units and native handle cleanup. The [mirrored-part AI example](tools/model/mirrored-part.request.json)
builds a handed bracket with editable plane position and tilt. The viewer exposes
normalized plane data and linked controls; its label sits at the result's bounding
centre rather than pretending to draw the plane itself.

Native reflection cost depends on topology and transformed curve/surface data.
History canonicalization is O(T + R) for T output topology entries and R history
targets, with identity indexing. Reflected geometry/history storage is proportional
to that data. Linked clones share the generated mirrored geometry when their
parameters match. Scale gates check 500 reflected-part regenerations and 10,000
linked copies with one geometry variant, each within a 10-second budget and with
zero retained native handles after generation release.
