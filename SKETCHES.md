# Sketch geometry, constraints and profile edits

Schema **72** completes the listed sketch additions: ellipses, signed line
angles, circular radius/diameter dimensions, point symmetry, point-on-curve
constraints, and saved trim/extend/offset operations. Native editing and curve
projection use C ABI **47**. Older documents migrate with empty `ellipses` and
`profile_operations`, preserving their previous geometry and constraints.

A sketch contains named points, lines, circles, arcs, interpolated splines and
ellipses in a typed 3D plane. `profile` chooses the ordered boundary used by
`SketchFace`, `SketchWire` or `SketchOpenWire`; omitted entities are construction
geometry. A sole ellipse, circle or closed spline can supply a closed profile.
Mixed geometry needs an explicit profile. Length expressions normalize to mm;
angle expressions are dimensionless **radians**.

## Added constraints

| JSON variant | Meaning |
|---|---|
| `angle: {first, second, value}` | Signed angle from the first directed line to the second, in `[-pi, pi]` radians; line direction follows its start/end IDs. |
| `radius: {curve, value}` | Positive radius of a circle or arc. |
| `diameter: {curve, value}` | Positive diameter of the circle supporting a circle or arc. |
| `symmetric: {first, second, axis}` | Two points reflected across the infinite named line. |
| `point_on_curve: {point, curve}` | Point on a supporting line, full circle, bounded arc, full ellipse, or bounded/periodic interpolated spline. |

These share the sparse solver and its native residual diagnostics. Arc
membership also enforces the directed arc span. Ellipse membership uses its
implicit equation, scaled in mm; spline membership projects onto the native
interpolated curve, including specified endpoint tangency, with two coordinate
residuals. Splines are never replaced by a sampled polyline for this constraint.
Analytic constraints touch at most eight coordinates; spline constraints also
touch their defining/tangent-neighbor points. Local finite differences and
normal-matrix fill depend on those references. A small relative damped step
allows underconstrained curves to reach their targets; free degrees and
redundancy are calculated from the undamped Jacobian.

An ellipse is `{"id":"e","center":"c","major":"a","minor":"b"}`.
Its defining points must be distinct; radii must be positive with major >=
minor. An implicit solver equation keeps its axis vectors perpendicular.
Axis lengths can be driven with point-distance constraints or fixed parameter
coordinates. Radius/diameter constraints describe circular geometry, rather
than an ellipse's varying curvature radius. Tangency retains the existing
shared-endpoint/rim convention; spline endpoint tangency is imposed during
native construction and shown as constructed, rather than a measured zero.

## Saved profile operations

`SketchDefinition.profile_operations` derives the generated boundary **after**
solving source constraints. The source point/entity identities remain intact.
Constraints and dimensions refer to that source geometry, not silently moved
trim/extend endpoints. The derived profile is what subsequent solid features
use and what the viewer draws in purple alongside faded source curves.

```json
"profile_operations": [
  {"trim": {
    "entity": "circle",
    "first": {"literal": {"value": 0, "dimension": "scalar", "unit": null}},
    "last": {"literal": {"value": 0.25, "dimension": "scalar", "unit": null}}
  }},
  {"extend": {
    "entity": "circle",
    "start": {"literal": {"value": 0, "dimension": "length", "unit": "millimeter"}},
    "end": {"parameter": "extension"}
  }},
  {"offset": {"distance": {"parameter": "offset"}, "join": "arc"}}
]
```

- **Trim** retains `first..last` of the entity's oriented **native parameter
  interval**, with `0 <= first < last <= 1`. Fractions are not arc-length
  fractions. Circle/ellipse zero follows its rim/major-axis direction. Lines,
  circular/elliptic curves and interpolated splines keep their native geometry.
- **Extend** takes nonnegative lengths, at least one positive. Lines and conics
  continue naturally by arc length. Splines use OCCT tangent-continuous
  extension to targets that far along the original endpoint tangents; that
  distance is not the resulting spline's arc length. Trim a full closed curve
  before extending it; periodic overlaps and degenerate tangents fail.
- **Offset** edits the entire assembled planar profile. Positive grows a closed
  profile; for an open profile it selects the right side of traversal relative
  to the sketch's plane normal. Negative selects the opposite direction.
  `join` is `arc` (round corners, default) or `intersection` (extended edges).
  Single open curves use native offset curves; multi-edge/closed profiles use
  native joining/offset algorithms. A split or disconnected result fails
  explicitly rather than selecting a region arbitrarily.

Entity trim/extend operations must precede whole-profile offsets; multiple
successive offsets are allowed. Entities must belong to the selected profile.
At most 1,024 operations are accepted. Edited segments must connect; closed
features require closure, and `SketchOpenWire` requires distinct ends. Native
precision and validity checks still apply, including downstream face/solid
validation. Each operation allocates a new handle and releases superseded
intermediate handles; the source definition and input shapes stay available.

## AI tools and viewer

Generated `sketch`, `feature`, `model`, `edit`, and `view` schemas include these
fields. Use guarded feature edits to change an embedded sketch or add a sketch
feature; parameter edits regenerate its equations and profile operations.
`occt_get_example` now includes **sketch-advanced**, a diagnostic view request
with an ellipse extrusion, construction dimensions/constraints and an edited
circular path. `occt_visualize_model` shows it without accepting failed geometry.

```sh
LD_LIBRARY_PATH="$PWD/build" cargo run \
  --manifest-path rust/occt-parametric/Cargo.toml --bin occt-model -- \
  --visualize tools/model/sketch-advanced.request.json /tmp/advanced-sketch
LD_LIBRARY_PATH="$PWD/build" cargo run \
  --manifest-path rust/occt-parametric/Cargo.toml --bin occt-view -- \
  /tmp/advanced-sketch/model.json --serve --output body
```

The combined studio exposes linked controls, radial/full-diameter labels,
angular arcs and radian residuals, symmetry/on-curve markers, and profile
operation controls. A native profile failure remains visible with its source
geometry; failed edits remain diagnostic and cannot replace accepted geometry.
Interactive point drawing/dragging and adding constraints through canvas tools
remain separate viewer work; these capabilities are defined in the model/API.

## Verification and scale

Tests cover solved/conflicting constraints, units and references, native spline
projection, exact rotated-plane ellipse areas, trim/extend geometry, signed
open offsets, closed corner joins, source preservation, schema migration,
geometry acceptance, and handle cleanup. C error-path tests exercise each added
entry point. Viewer/API tests inspect the same constraint and derived-profile
data used for native regeneration.

`cargo bench --manifest-path rust/occt-parametric/Cargo.toml --bench sketch_advanced`
(with the native library environment above) enforces three 10-second budgets:
1,000 coupled advanced components, 1,000 point-on-spline components, and 1,000
trim/extend/offset profile regenerations. Measured release times were 0.063 s,
0.115 s and 0.088 s. The benchmark is included in `tools/bench/run.sh`.
