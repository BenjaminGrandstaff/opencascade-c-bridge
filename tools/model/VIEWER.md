# Dimensions and constraints on sketches and solids

For live editing, use the [combined model studio](../view/README.md) with
`occt-view MODEL.json --serve`. Select a dimension to edit its linked parameter
and regenerate both geometry and checks. The studio and these standalone
artifacts share their native scene builder and annotation renderer.

Model builds with `preview: true` now include **viewer.html**, **view.json** and
numbered **view-NNNN.svg** annotated snapshots alongside the existing HLR SVGs.
Open `viewer.html` in a browser: it is self-contained, loads without a server or
network access, and contains the actual native tessellation and sketch curves.
Solid views orbit on drag; sketch views pan. Scroll to zoom, use Fit view to
reset, and select a view from the dropdown.

Click a dimension/constraint label or its row in the side panel. Related sketch
entities and points or the related solid output highlight, while the selection
panel shows parameter values, requirement measurements and solver evidence.
A searchable annotation list and visibility toggles help with crowded models.
For more than 120 labels, the view shows selected/failed labels; the full list
remains available. Labels support Enter/Space; Escape clears selection and Home
fits the view. Solid rendering uses WebGL when available and an SVG surface
fallback otherwise. Static SVGs preserve labels and metadata tooltips for export
or clients that cannot run the interactive HTML.

## What is shown

**Sketches:** native-sampled lines, circles, arcs and interpolated splines in
sketch-local XY millimeters, with their actual solved point positions. Overlays
cover horizontal (H), vertical (V), coincident (≡), parallel (∥), perpendicular
(⊥), equal length (=), tangency (T), fixed points and distance dimensions.
Distance labels show the constraint's target value; failed constraints remain
red even when the geometry cannot reach that target. Select one to inspect its
actual residual and referenced controls. Free degrees, redundant equations,
iteration count and maximum residual are reported by the native solver.

Per-constraint verdicts reuse the solver's residual equations and tolerance,
including millimeter versus normalized dimensionless residuals. Spline endpoint
tangency is imposed by native curve construction and shown as `constructed`,
with no invented zero residual. Point-expression parameters on free points are
initial guesses; fixed-point expressions are driving coordinates. Native curve
construction failures are shown as unavailable geometry rather than fabricated
curves. Construction entities are included even outside the closed profile.

**3D shapes:** exact geometry bounds supply measured X/Y/Z spans, independently
of camera rotation. Direct boxes/cylinders/cones/spheres and extrusions also show
feature-definition driving dimensions, linked to the actual referenced
parameters. Measured extents are explicitly distinct from driving values.
Downstream feature inputs expose their linked controls in the annotation list;
these highlight the related output, not an inferred face or fitted dimension.
Part parameter constraints and part requirements appear with measured values,
limits and exact/sampled evidence where available. Failed geometric requirements
have red labels and their actual witness locations. Unverified checks remain
separate from measured failures.

Each solid is shown in **family-local authoring coordinates**, matching feature
selectors. This is a part viewer, not an assembly-mate inspector. Sketch axes
are the sketch's own XY coordinates; the solid view uses the native 3D geometry
of its owning family. Viewer topology indices are snapshot-local. The viewer
selects and inspects controls; model changes still use the guarded edit/build
workflow. These inspection graphics are not certified manufacturing drawings.
Use `occt-drawing-export` for datum-linked drawing dimensions, tolerances and GD&T.

## Visualize an unaccepted proposal

Use the dedicated diagnostic command/tool to see conflicting sketch constraints
and failed required shape checks. It produces a visualization rather than an
accepted build, keeps requirements intact, and never writes STEP/STL exports.
Native geometry creation still validates topology. A failed solid can be listed
as unavailable while its diagnostic sketch remains visible. Unmet part checks
are retained for display; errors evaluating a check are marked unverified.
Structural model/parameter validation still applies, and assembly verification
is not run by the family-local visualization path.

```sh
LD_LIBRARY_PATH="$PWD/build" cargo run \
  --manifest-path rust/occt-parametric/Cargo.toml --bin occt-model -- \
  --visualize tools/model/sketch-block.request.json /tmp/block-view
```

[The block example](sketch-block.request.json) contains a dimension-driven
rectangle and an extruded solid. [The conflict example](sketch-conflict.request.json)
fixes its points and requests an incompatible width, exposing the failed
constraint without accepting or altering a model.

Through MCP, get either example with `occt_get_example`, then pass it to
`occt_visualize_model`. Its generated `view` schema is available through
`occt_get_schema`/`resources/list`. A request contains:

```json
{
  "schema": "occb-model-view-v1",
  "model": { "schema_version": 73, "...": "complete model" },
  "outputs": [{ "instance": "block", "output": "body" }],
  "sketches": true,
  "options": {
    "maximum_triangles": 100000,
    "maximum_vertices": 1000000,
    "maximum_annotations": 10000,
    "maximum_scenes": 1000
  }
}
```

Empty `outputs` shows sketches from saved instances. With outputs, the viewer
includes the requested local solids and each selected instance's sketches.
Repeated output selections fail. Driven pattern members should first be
materialized by the normal build path. The report returns `visualization_id`,
`status: "visualized"`, and readable resources for HTML, scene JSON and SVGs.
Diagnostic IDs are rejected by accepted-build inspection/edit tools. Ordinary
builds still return `build_id` and pass their required checks before publishing.

Limits are global across all views. Hard supported caps are 1,000,000 triangles,
1,000,000 vertices, 100,000 annotations and 10,000 scenes, all positive. Native
meshing workspace is not bounded by the returned triangle cap. Curve previews
use 32 samples per edge and mesh deflection is at least 0.1 mm or span × 1e-5;
these are approximate visualizations. Dimensions/check evidence come from the
exact geometry/solver, not the tessellated screen image. Oversized data resources
retain the existing 32 MiB read limit and can be opened from their local artifact
paths.

## Validation

Three additional command tests cover solved dimensions/control links, actual
conflicting constraints, required-wall failures, native handle cleanup, global
budgets, and safe round-tripping of labels containing HTML/script characters.
One MCP test checks both diagnostic and accepted-build artifacts and prevents
promoting diagnostic IDs to accepted builds. Viewer application-logic tests
exercise scene selection, label selection, highlights, toggles, orbit/pan/zoom
and failed states using the actual JavaScript with a minimal DOM host. Annotated
SVGs are rendered and visually checked; an interactive browser surface was not
available in the development session, so that browser QA remains unverified.

```sh
node tools/model/viewer/test_viewer.cjs /tmp/block-view/view.json
```

`tools/bench/run.sh` additionally checks 1,000 native sketch constraints with
curve/annotation/snapshot output (10-second budget), and 1,000 annotated solid
views with global mesh/vertex budgets and dimension data (30-second budget).

Schema-72 views also include radial/full-diameter and angular dimensions,
symmetry and point-on-curve markers, and native edited profiles in purple.
Source curves remain visible, faded when a derived profile exists. Operation
rows expose linked controls and native failure details. See
[Sketches](../../SKETCHES.md) and the `sketch-advanced` example.
