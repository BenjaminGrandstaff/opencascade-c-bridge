# Split bodies

Schema 120 adds a `split` feature that cuts a solid with a plane and keeps
one side, the other, or both:

```json
{ "id": "cap", "operation": { "split": {
    "input": "housing",
    "origin": { "components": { "x": {"literal": {"value": 0, "dimension": "length", "unit": "millimeter"}},
                                "y": {"literal": {"value": 0, "dimension": "length", "unit": "millimeter"}},
                                "z": {"parameter": "split_height"} } },
    "normal": { "literal": { "x": {"value": 0, "dimension": "scalar", "unit": null},
                             "y": {"value": 0, "dimension": "scalar", "unit": null},
                             "z": {"value": 1, "dimension": "scalar", "unit": null} } },
    "keep": "positive" } } }
```

- `origin` is a length-valued point on the plane and `normal` a nonzero
  dimensionless direction. Both are expressions.
- `keep` is `positive` (the side the normal points to, the default),
  `negative`, or `both`. `both` returns one compound of both sides' solids.
- A kept side may contain several solids, for example the two prongs of a
  U shape cut above its floor.
- To use both halves separately, add two `split` features on the same input,
  one keeping each side. The example below does this.

## How it is built

Each kept side is the input cut by a prism on the other side of the plane:
a square in the plane, extruded along the normal, both reaching beyond the
input's bounding box. That is one boolean cut per kept side, and each cut
keeps native face history from the input. There is no ABI change.

The input must contain valid solids. The plane must cross it: each kept side
must hold more than 1e-7 and less than 1 − 1e-7 of the input's volume, so a
plane at or beyond a face fails instead of returning the whole part or
nothing. `both` also requires the two sides' volumes to sum to the input's
within 1e-7 relative. Every piece must be valid. Failed splits release their
temporary shapes, and incremental regeneration keeps the last accepted result.

The viewer anchors a `split plane` annotation at the plane origin. It reports
which side is kept and how many solids resulted, and links the origin and
normal controls.

## Example and tests

The [split housing](../tools/model/split-housing.request.json) bores a
60 × 40 × 30 block along its length and drills two bolt holes, then splits it
at `split_height` into a `cap` (above) and a `base` (below). Tests check:

- cap plus base equal the housing's volume (1e-7 relative), each piece is one
  valid solid, and the cut lies exactly at the split height, across height
  edits that reuse the unsplit housing and rebuild both pieces;
- `both` with a tilted plane returns two solids conserving volume;
- a U-shaped input keeps two solids on one side;
- planes at or outside the block's faces, and a zero normal, fail without
  leaking handles while the accepted pieces stay valid.

1,000 full regenerations of the housing (box, three holes and two splits)
are a required scale case (20 s budget). The MCP renders 20 housings (40
pieces) within a 10 s budget.

## Not yet included

Splitting by a curved face or a bounded surface, and returning both sides as
separately addressable outputs of one feature.
