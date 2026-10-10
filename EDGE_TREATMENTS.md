# Fillets and chamfers

Constant `fillet` and equal-distance `chamfer` features use native OpenCascade
geometry with semantic edge selectors. The
[AI example](tools/model/edge-treatments.request.json) defines a 30 × 20 × 10 mm
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

This increment covers direct constant Fillet and Chamfer outputs. Variable
radius station annotations and downstream treatment-specific labels remain
future viewer work; existing variable-radius geometry operations are available.

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
