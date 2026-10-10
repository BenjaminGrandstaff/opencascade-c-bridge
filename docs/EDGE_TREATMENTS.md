# Fillets and chamfers

Constant `fillet` and equal-distance `chamfer` features use native OpenCascade
geometry with semantic edge selectors. The
[AI example](../tools/model/edge-treatments.request.json) defines a 30 × 20 × 10 mm
block and selects its four vertical corner edges by their nearest centres.
`rounded` applies a 2 mm fillet; `beveled` applies a 1.5 mm chamfer to the same
source block. These are independent alternatives. Schema 91 and ABI 52 remain
unchanged.

## Inspecting and editing

Generate the annotated viewer with:

```sh
LD_LIBRARY_PATH="$PWD/build" rust/occt-parametric/target/release/occt-model \
  --visualize tools/model/edge-treatments.request.json /tmp/edge-treatments
```

Open the resulting `viewer.html` and choose either scene. Select its treatment
label to inspect the nominal radius/distance, selected-edge count and related
parameters. For live edits, serve its saved model:

```sh
LD_LIBRARY_PATH="$PWD/build" rust/occt-parametric/target/release/occt-view \
  /tmp/edge-treatments/model.json --serve --output rounded --port 0 --no-open
```

Use `--output beveled` for the chamfer alternative. Changing the treatment
value updates native geometry and the linked label. Width, depth and height
also drive the selector centres, so selected corners follow size edits.

The paths show original selected source edges before rounding or beveling.
They may lie outside the finished solid. Annotation metadata sets
`source_reference: true`, `measurement: false`, and separately records
`selected_edge_count` and `displayed_reference_count`. Up to 64 references are
sampled at eight points each, charged to the global vertex budget before
sampling. Semantic selection still has its existing native query cost.
The radius/distance label is a driving value; it does not certify measured
blend curvature, bevel width or manufacturing tolerance.

Direct VariableFillet outputs also show a linked contour-law label, endpoint
radii and interior station labels. See the variable-radius section below.
Treatment-specific labels after downstream operations remain future viewer work.

## Validation and exports

For four vertical corners, rounded volume is
`width × depth × height − (4 − π) × radius² × height`.
Beveled volume is `width × depth × height − 2 × distance² × height`.
The default outputs measure 5965.663706 mm³ and 5955 mm³ respectively.
Regression tests check these volumes, native validity, linked controls and
source references. Changing only the fillet radius rebuilds `rounded`, while
reusing `block` and `beveled`; a rejected edit preserves the accepted geometry
and releases temporary handles.

The example is an `occb-model-view-v1` request and supports both alternatives
in one viewer. For STEP/STL export, use `occb-model-request-v1`, remove
`sketches`, enable `step`/`stl`, and select one output per instance. Both
alternatives have been exported separately. The MCP scale gate generates
20 valid treatment scenes with bounded references in 0.209 s (10 s budget).


## Variable-radius contour laws

The [variable-fillet example](../tools/model/variable-fillet.request.json) rounds
one vertical corner with a 1 mm start radius, a 2.5 mm interior radius at
normalized position 0.25, and a 2 mm end radius. Its `from_point` direction
starts at the contour endpoint nearest the block origin. Edit `start_radius`,
`end_radius`, `middle_radius` or `station_position` through their linked labels.
Positions are dimensionless, strictly interior and ordered; radii are positive
lengths. With no interior stations, the existing endpoint law is linear.

Visualize it with the same command above, substituting
`tools/model/variable-fillet.request.json` and a new output directory. Serve
the saved model with `--output blend`. The standalone viewer and live viewer
share the same annotation collector.

`driving-variable-fillet` records the complete evaluated `radius_law`,
`spine_direction`, selected source-edge references and related controls.
Endpoint labels and up to 64 interior labels link their own driving expressions.
The full law requires O(stations) work/storage; extra label generation is
bounded, and source-edge sampling retains the 64-edge/eight-point limit.

Normalized law positions belong to OCCT's tangent contour, which can include
unselected tangent neighbours. They are not parameters of an individual
selected edge. The viewer therefore places radius labels at the scene centre
and sets `spatial_stations: false` on the law and `spatial_station: false` on
each radius label. These are nominal law values, not measured curvature or
spatial station markers. Native contour-coordinate visualization remains
future work. Direction metadata supports kernel, reversed and from-point laws;
this example uses a fixed from-point expression.

Tests verify native validity and removed volume, station/radius edits, block
reuse, rejected-edit retention, linked law data, live edit/revert and STEP/STL
export. The example's default volume is approximately 5986.772779 mm³.
