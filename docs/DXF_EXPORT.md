# DXF export

A planar face of a part can be written as a DXF cutting profile for laser,
waterjet, plasma or router cutting. Typical faces are a sketch face, the top
of a plate, or a flat sheet-metal pattern. The file is the counterpart of
[DXF import](DXF_IMPORT.md): what one writes, the other reads back exactly.

## What is written

| Layer | Contents |
|---|---|
| `OUTLINE` | The face's outer boundary |
| `HOLES` | Its inner boundaries (holes, slots, cutouts) |
| `BEND` | A flat pattern's bend lines, each once |

Curves stay exact:
- straight edges become `LINE`;
- circles become `CIRCLE` and arcs `ARC`;
- ellipses and elliptical arcs become `ELLIPSE`;
- any other curve becomes one `SPLINE` per exact rational Bézier span of the
  kernel's edge, with its weights.

Each arc's DXF angles come from its actual start, middle and end points, so
clockwise arcs, mirrored planes and ellipses with swapped axes all come out
right. The header sets millimeters (`$INSUNITS` 4), and a layer table names
the three layers.

## Coordinates

Plane coordinates are the face as seen from outside it.
- **Faces facing ±Z:** X and Y are model X and Y, both seen from +Z, so a
  bottom face is not mirrored.
- **Other faces:** X runs horizontally, along +Z × normal, and
  Y = normal × X, which points up toward +Z.
- **Origin:** the plane's point nearest the model origin, so a face at
  z = h keeps its model X and Y.

The report gives the frame (`origin_mm`, `x_axis`, `y_axis`), the face's
`area_mm2`, the entity and layer counts, and `bounds_mm`.

## Where to use it

- **Builds:** add `"dxf": {}` to a build request. Each selected output then
  gets `NNNN.dxf`, from its largest planar face (the first of equal ones).
  `"face"` takes a face selector instead, which must pick exactly one planar
  face. `"bend_lines": false` leaves out a flat pattern's bend lines. Each
  output's report entry has `dxf` with the file and the report above. A
  selection that does not give one planar face fails the `export` stage.
- **MCP:** `occt_build` takes the same `dxf` field, and the files are readable
  as `image/vnd.dxf` resources.
- **Studio:** the **DXF** button in the top bar downloads the shown part's
  largest flat face, with any bend lines, as `<instance>.dxf`.

The library entry points are `occt_parametric::dxf_export::face_to_dxf` (one
face) and `output_to_dxf` (an output's face, with bend lines).

## Tests and scale

Every test reads the exported file back with the DXF importer, rebuilds the
face, and compares areas, which match to 1e-6:
- **Bracket:** the [bracket](../tools/model/dxf/bracket.dxf), with a
  clockwise fillet arc, three holes, and a bend line on its own layer.
- **Curved shapes:** an island region, an ellipse turned 53°, and a spline
  D shape.
- **Solid face:** a block's side face with a hole, in its own vertical
  plane.
- **Flat pattern:** the `hemmed-tray` flat pattern, whose five bend lines
  come once each and whose slot crosses a bend zone.
- **Selectors:** face selection and its refusal.
- **MCP and studio:** export, then the resource read or the studio download.

Export is linear in edges and Bézier spans. Each edge may convert to at most
100,000 poles, and bend lines are de-duplicated pairwise.
