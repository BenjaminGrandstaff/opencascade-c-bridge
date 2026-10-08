# Symmetric sketch revolutions

Schema 78 adds the optional `extent` field to `revolve`; native ABI 49 is unchanged.

```json
"revolve": {
  "input": "profile",
  "origin": {"literal": {"x": {"value": 0, "dimension": "length", "unit": "millimeter"}, "y": {"value": 0, "dimension": "length", "unit": "millimeter"}, "z": {"value": 0, "dimension": "length", "unit": "millimeter"}}},
  "axis": {"literal": {"x": {"value": 0, "dimension": "scalar", "unit": null}, "y": {"value": 0, "dimension": "scalar", "unit": null}, "z": {"value": 1, "dimension": "scalar", "unit": null}}},
  "angle_radians": {"parameter": "angle"},
  "extent": "symmetric"
}
```

`extent: "angle"` is the default: start at the source profile and sweep through
the signed angle. Older documents with no extent preserve this behavior.
`extent: "symmetric"` starts at minus half the supplied angle and ends at plus
half, around the original sketch plane. A positive 90-degree angle therefore
covers -45 to +45 degrees; a negative angle reverses traversal. The signed
angle remains the total sweep, not the angle on each side. Zero angles and
magnitudes beyond one turn are rejected. Angle expressions are dimensionless
radians, origin expressions are lengths, and axis expressions are dimensionless.

Inputs remain valid planar faces or closed planar wires. The native kernel
must produce one valid solid with positive volume. Symmetric mode places a
location-based copy of the profile at its start angle, then uses the existing
native face-revolution operation. Explicit history composition traces generated
faces back through that placement to the original source edges. The source
sketch stays unchanged, and temporary shape handles are released.

Angle, axis, origin and extent edits invalidate the revolve and its downstream
features. Unchanged source sketches are reused. Invalid edits leave previously
accepted geometry usable. Saving preserves the mode; loading older documents
migrates them to schema 78 with `angle` extent.

Both viewers and SVG snapshots show the complete signed angle around the actual
axis. Symmetric arc endpoints agree with the placed start/end sections, using
the source face's area-centroid radius for display. This display radius is not
a part-size dimension. Arc samples consume the global vertex budget. The
[symmetric-revolve AI example](tools/model/symmetric-revolve.request.json) builds
a quarter-ring centered about its radial sketch plane and exposes linked controls.

Placement/history bookkeeping scales with source topology; the native revolution
cost depends on the profile's curves and topology. Copies share underlying
geometry. The `symmetric_revolves` benchmark runs 1,000 positive/negative cases
with a 10-second budget, verifying analytical volume, centered bounds, source-edge
history, immutable section edge counts and zero retained handles. Library tests
also cover face/wire inputs, complete turns, edit reuse, mode changes, failed edits
and schema migration. Viewer tests check signed arc endpoints and geometry bounds.
