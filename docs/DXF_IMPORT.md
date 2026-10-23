# DXF import

For the reverse, writing a part's face as a cutting profile, see
[DXF export](DXF_EXPORT.md).

A DXF file's 2D outlines become an ordinary sketch: points, lines, arcs,
circles, ellipses and splines, with the closed loops sorted into the outer
boundary, its holes and further regions. You can then extrude, revolve or
cut the sketch like a drawn one, or constrain and edit it. The import is an
authoring step, not a document feature, so the model never refers back to the
file and the schema is unchanged. The reader is in the kernel-free core crate
(`occt_parametric::dxf::import_dxf`).

## What is read

| DXF entity | Becomes |
|---|---|
| `LINE` | line |
| `ARC`, `CIRCLE` | arc (counterclockwise), circle |
| `ELLIPSE` | full ellipse or elliptical arc |
| `LWPOLYLINE`, 2D `POLYLINE` | a line or arc per segment; bulges become arcs |
| `SPLINE` with control points | B-spline with its degree, knots and weights (clamped knots only) |
| `SPLINE` with only fit points | interpolating spline |
| `INSERT` | the block's entities, placed with uniform scale and rotation |

Text, dimensions, hatches, points, viewports and similar annotation are
skipped and counted, as are entities the reader cannot use: 3D polylines and
meshes, unclamped or periodic splines, mirrored, non-uniform or arrayed
inserts, and entities outside the XY plane. Arcs and polylines drawn with a
−Z extrusion (mirrored) are converted to world coordinates. A nonzero Z is
dropped, and the report counts the entities that had one. Binary DXF is
refused; save the file as ASCII DXF.

## Units, layers and tolerance

Coordinates are converted to millimeters from the header's `$INSUNITS`
(inches, feet, millimeters, centimeters or meters). A file with no unit, or
marked unitless, is read as millimeters, and the report says so. An explicit
`unit` option overrides the file. `layers` keeps only the named layers. Block
contents on layer `0` take the layer of their insert, as in CAD programs.

Endpoints closer than `tolerance_mm` (default 0.001 mm, at most 1 mm) become one
sketch point, which closes small drafting gaps. Lines shorter than the
tolerance are dropped. Centers, circle rims, ellipse axis points, control
poles and fit points are never merged with anything.

## Loops and regions

Circles, full ellipses and closed splines are loops by themselves. Other curves
form a loop when they chain end to end, with exactly two curves meeting at
every point. Curves walked backwards are turned around (an arc flips
`clockwise`, a spline reverses its poles and knots), so each loop runs end to
start as the sketch profile requires. The largest loop not inside another is
the sketch `profile`. Loops inside a loop are its holes, a loop inside a hole
is an island and becomes another region, and so on. Loops that sit apart are
further `regions`. Curves in no loop, such as open chains, branches and
construction lines, stay in the sketch as construction geometry, and the
report counts them as `open_curves`.

## Report

Every import returns a report:
- `unit` and `unit_source`.
- `imported`: counts by DXF entity type. `skipped`: counts by `TYPE: reason`.
- `layers` used.
- `curves`, `points` and `merged_endpoints`.
- `loops`, `profile_curves`, `holes` and `regions`.
- `open_curves`, `flattened` and `bounds_mm`.

## Where to use it

- **Command line:** `occt-model --import-dxf REQUEST.json NEW_RESULT.json`,
  where the request is
  `{"schema": "occb-dxf-import-v1", "dxf": "<file text>", "feature_id": "outline", "options": {...}}`.
  The result holds the `sketch_face` feature, the report, and `face`: the face
  built once, with `area_mm2` and the face count, or the error if it does not
  build (for example an open outline).
- **MCP:** `occt_import_dxf` takes the same `dxf`, `feature_id` and `options`
  and returns the same result. Add the feature to a model or with an
  `add_feature` edit, then extrude it.
- **Studio:** New sketch has **Import DXF**, with optional layers and unit.
  The file is read, its summary shown, and **Start sketch** places the geometry
  on the chosen plane (XY, XZ, YZ, a datum plane or a picked face) as the
  first draft of the new layer, previewed at once. Edit or constrain it, then
  Apply.

Options: `id` (sketch id, default `dxf`), `layers`, `unit` (`millimeter`,
`centimeter`, `meter`, `inch`, `foot`) and `tolerance_mm`.

## Limits and scale

- **Size:** at most 64 MiB of DXF text (32 MiB through the studio).
- **Curves:** at most 10,000 after expanding blocks.
- **Loops:** at most 1,000 holes in one boundary and 1,000 separate regions.
- **Blocks:** inserts nest at most 8 deep and expand to at most 1,000,000
  visited entities.

Merging endpoints is O(1) per point on a tolerance grid, and nesting compares
loops' bounding boxes before testing containment.

A drawing of ten plates with 249 square holes each (10,000 lines) imports in
0.16 s (release build). Through `--import-dxf`, including building its 10
faces, it takes 1.8 s and 143 MB.

## Example and tests

[`tools/model/dxf/bracket.dxf`](../tools/model/dxf/bracket.dxf) is an L
bracket on layer `OUTLINE` (a closed polyline with a 5 mm inside fillet drawn
as a bulge), three Ø6 holes on layer `HOLES`, and a note on layer `NOTES`.
Imported with the `OUTLINE` and `HOLES` layers, it builds a face of exactly
2525 − 33.25π mm².

Tests cover:
- **Core:** units, bulges, layers, block inserts with rotation and scale,
  nesting four deep, splines, ellipses, mirrored arcs, gap tolerance,
  old-style polylines, every loop running end to start, the limits, and
  errors.
- **Kernel:** extruded volumes against closed-form areas for a plate with a
  hole and a slot, islands, an ellipse, spline D shapes, scrambled curve
  directions and the bracket.
- **Model command, MCP and studio:** the import, the face check, a build of
  the imported feature, and request errors.
