# Thickened sheets

Schema 121 adds a `thicken` feature that turns a sheet into a solid, and
ABI 62 adds the native `occt_bridge_thicken` behind it:

```json
{ "id": "shell", "operation": { "thicken": {
    "input": "sheet",
    "thickness": { "parameter": "thickness" },
    "side": "normal" } } }
```

- `input` is a sheet: a face, a shell, or a compound of them, with no
  solids. Sketch faces, sewn shells, and sweeps of open profiles all produce
  sheets.
- `thickness` is a positive length expression.
- `side` is `normal` (the default), growing along the sheet's face normals;
  `reverse`, growing against them; or `symmetric`, growing half the
  thickness to each side.
- Sharp edges between sheet faces are mitred: the offset faces are extended
  until they meet, as in sheet metal without bend radii. Smooth (tangent)
  joins stay smooth.

Face normals of a sweep follow the profile's direction, so which way
`normal` grows is easiest to read from the viewer, which draws the wall
dimension on the side grown (see below).

## How it is built

The bridge closes the sheet into a solid with one OCCT skin offset in
thickening mode (`BRepOffset_MakeOffset`, intersection joins). `symmetric`
first offsets the sheet by half the thickness against its normals, then
thickens that by the full thickness. Both are O(sheet faces).

History lets later features select the new faces:

- each sheet face generates its offset face; one-sided, the sheet face itself
  stays in the solid, and symmetric, its half offset does (recorded as
  modified);
- each free edge of the sheet generates the side wall that closes it.

Thickness must be positive and finite, and the input must be a sheet of at
most 10,000 faces. Walls that collapse, such as a curved sheet thickened
toward its centre by more than its radius, fail with OCCT's offset
diagnostics. Failures release temporary shapes, and incremental regeneration
keeps the last accepted result.

The viewer draws a `thicken` wall dimension on the first sheet face: from
where the face normal through its centroid meets the face, across the side
grown. Its detail reports the side, the direction, the solid count and the
wall measured by a native ray through the result.

## Example and tests

The [thickened strap](../tools/model/thickened-strap.request.json) is a pipe
clamp strap. An open profile (foot, half circle, foot) is swept across the
strap's width into a sheet, thickened outward, and drilled with a bolt hole
in each foot. Pipe radius, thickness, width, foot length and hole diameter
are parameters. Tests check:

- exact solid volumes (1e-9 relative) for all three sides, against a closed
  form that includes the mitred corners where the feet meet the half ring,
  across radius and thickness edits; the bore stays at the pipe radius for
  `normal`;
- thickness edits reuse the swept sheet;
- a planar sketch face thickens to exactly area × thickness with six faces;
- at the bridge level, L-shaped and half-cylinder sheets thicken to exact
  volumes, and face and edge history reaches the offset faces and side walls;
- wires, solids, zero or negative thickness and a collapsing wall fail
  without leaking handles, and the accepted strap stays valid.

Two required scale cases bound the work: a 400-face corrugated sheet with 399
mitred sharp edges thickens within 5 s (0.4 s measured), and 500 thickness
edits of the strap, which reuse its sheet, run within 15 s (about 16 ms each,
mostly the two hole cuts). The MCP renders 20 straps (40 solids) within a
10 s budget.

## Not yet included

Thickening selected faces of a solid in place, per-face thicknesses, and
rounded (arc) joins at sharp edges.
