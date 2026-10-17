# Lofts from saved sketch profiles

Schema 77 adds `profile_loft`, using native ABI 49:

```json
"profile_loft": {
  "profiles": ["lower", "upper"],
  "ruled": true
}
```

Profiles are ordered feature-output IDs, with 2–1,000 distinct sections. Each
output must be a valid planar face with one boundary wire, or a valid closed
planar wire. Sketch circles, ellipses, connected lines/arcs and native splines
are passed directly to OCCT. The existing `loft` operation remains available
for parameter-placed point-array sections.

OCCT compatibility aligns origins and orientations and can split copies of
edges to match different section topologies. Inputs remain immutable. A circle
and a rectangle can therefore have different edge counts. Section order and
geometry influence correspondence and twist; the supplied order is retained.
`ruled: true` creates straight generators between consecutive sections.
`ruled: false` uses native smooth surface approximation at the kernel's default
precision. Input profiles are not resampled into polygons; smooth lateral
surface fitting is still subject to OCCT approximation tolerance.

The result must be one valid solid with finite positive volume. Open wires,
nonplanar sections, duplicate sections and invalid topology are rejected.
Faces with inner boundaries are rejected rather than silently losing holes.
For hollow transitions, use explicit hole tracks as described below. Native generated source-edge history is retained for later
selection and modeling operations.

Profile outputs enter the feature dependency graph. Changing one section
rebuilds its loft and downstream features while reusing unaffected sections.
Profiles may be declared after their loft; evaluation follows dependencies.

The shared viewer keeps both sketch scenes and the resulting solid. It shows
measured spacing between consecutive section area centroids, with links to
section parameters. This is a centroid-to-centroid distance, not a wall
thickness or a guarantee of parallel section planes. Assembly annotations use
the same family-coordinate anchors and placement transforms.

The [profile-loft example](../tools/model/profile-loft.request.json) is available
through MCP as `profile-loft`. Edit `lower_radius`, `upper_radius` or `height`
to regenerate the transition. Authoring schemas expose all 30 feature types.

Profile lookup and topology indexing are linear in the supplied sections;
native compatibility and fitting depend on section count, edge correspondence
and curve degree. Compatibility operates on copies, and temporary handles are
released. The `profile_lofts` gate checks 1,000 regenerations against a 10-second
budget, including analytical frustum volume, validity, unchanged section edge
count and zero retained handles. Tests also cover ellipse/arc sections, mixed
edge counts, native periodic splines, source history and edit invalidation.

## Explicit hollow tracks (schema 80)

```json
"profile_loft": {
  "profiles": ["lower_outer", "upper_outer"],
  "holes": [["lower_bore", "upper_bore"]],
  "ruled": true
}
```

`holes` defaults to an empty array, preserving solid behavior in older documents.
Each of up to 100 tracks supplies one simple closed profile for every outer
station, in the same order. Profiles may be closed planar wires or single-boundary
planar faces. All section output IDs must be distinct across all tracks.
Already-holed faces remain rejected: explicit tracks define hole correspondence
without relying on native wire traversal order.

At each station, the inner profiles must be coplanar with the outer profile,
strictly contained, and disjoint, using the same native area/clearance checks as
`planar_region`. Outer and inner tracks are then lofted independently with the
same ruled/smooth mode. Native whole-volume containment and pair-separation
checks reject inner lofts leaving the outer solid or crossing/touching each other
between stations. One cut removes the inner-loft compound; the result must be
one valid positive-volume solid with the expected material volume within native
tolerances. Native histories are composed and retained through solid extraction,
including generated outer/inner wall ancestry. Source profiles stay unchanged.

Hole profiles enter feature dependencies. Bore edits rebuild affected sections
and the loft while reusing unaffected sections. Invalid edits retain accepted
geometry. The [hollow-loft AI example](../tools/model/hollow-loft.request.json)
shows a tapered duct, all four boundary sketches, linked bore controls and
outer-section centroid spacing. The current native ABI is 52; the current model schema
is 91. The low-level wire-loft API still constructs one boundary track at a time.

For N stations and H holes, station bookkeeping and pair screening are
O(NH²), with cached bounds before native pair distances. Complete inner-solid
pair screening is O(H²), and H history compositions cost O(HM) for aggregate
history size M. Native fitting/common/cut work depends on curve/surface topology.
Scratch handles are bounded by the track topology and released after validation
and construction. Scale gates check 500 tapered hollow lofts and a 100-bore loft,
with analytical volume, wall ancestry and zero retained handles, each within a
10-second budget. Hollow sweeps use the same validated subtraction/history helper
and continue to pass their scale gates.

## End constraints (schema 169, ABI 77)

Schema 169 and ABI 77 add optional start/end tangent directions and strengths to
`profile_loft`. Unconstrained lofts keep their existing native fitting behavior.
See [the complete example](../tools/model/constrained-loft.request.json).

### Definition

```json
{
  "profile_loft": {
    "profiles": ["lower", "upper"],
    "ruled": false,
    "start_constraint": {
      "direction": {
        "components": {
          "x": {"literal": {"value": 0, "dimension": "scalar", "unit": null}},
          "y": {"literal": {"value": 0, "dimension": "scalar", "unit": null}},
          "z": {"literal": {"value": 1, "dimension": "scalar", "unit": null}}
        }
      },
      "magnitude": {"parameter": "height"}
    }
  }
}
```

Either end may be constrained independently. Directions are finite, nonzero,
dimensionless vectors in family coordinates. Magnitudes are positive lengths.
Both directions follow **increasing station order**, including the end direction.
For profiles stacked along +Z, both directions normally point along +Z.
Each direction is normalized and multiplied by its magnitude during regeneration.

Strength is the longitudinal derivative length per station interval, not an angle
or a distance to an extra profile. Increasing the start strength changes how far
the loft travels along its start direction before turning toward the next
section. Direction and magnitude expressions participate in dependency tracking;
a change to an independent strength parameter rebuilds the loft without rebuilding
unchanged source sketches. Sketch dimensions and constraints continue to drive
all section curves.

### Studio

Choose **Loft through profiles**, select smooth surfaces, and set **Start loft
condition** or **End loft condition** to **Tangent direction**. Enter direction
components and a strength, or link a length parameter to the strength. Existing
constraint expressions start preserved. Choosing **Free** removes that end's
constraint; switching back retains its draft controls.

Preview verifies every variant before Apply. Unsupported conditions or invalid
strengths produce an error and no verified preview mesh. Accepted changes use
model Undo/Redo and Save.

### Geometry and limits

Constrained lofts use exact rational cubic Hermite surface spans, with one span
between successive stations. All section curves are interpolated; there are no
sampled replacement profiles or additional near-end sections used to imitate a
tangent. Repeated endpoint weights make the actual surface derivative equal the
requested derivative. Native construction independently evaluates the terminal
surface derivatives at 17 parameters on every terminal edge patch and rejects
violations. Interior station derivatives agree across adjacent spans.

The current supported range is 2–64 distinct closed planar sections, matching
ordered edge topology at every station, at most 128 edges per section and 100,000
combined compatible section poles. Corresponding wire edges and seams must be
authored in corresponding order. Unlike unconstrained lofts, this mode does not
automatically split or reorder edges. Changing a circle into a four-edge square
therefore requires compatible authored section topology first.

Ruled lofts cannot carry these constraints. A terminal direction must cross its
profile plane. Each span must progress monotonically along a common longitudinal
direction; excessive or backward strengths that fold the span back are rejected.
The sewn result must close into one valid solid. Sources remain immutable, and
source-edge/generated-wall ancestry is recorded. Explicit hole tracks receive
the same end constraints and are subtracted with retained wall history.

These constraints set longitudinal tangent directions and strengths. They do not
automatically match another face's curvature or supply guide rails.

### Verification

Tests check analytic circular and polygonal volumes, intermediate cross-section
radii, altered shape under a strength change, interpolation through three
stations, one-end and two-end constraints, hollow tracks, source immutability,
history, incremental parameter rebuilding, persistence, and rejected constraints
including a bad strength on another family variant. The profile-loft benchmark
checks 1,000 unconstrained and 1,000 constrained regenerations against separate
10-second budgets.

## Normal to profile (schema 170)

A loft end can derive its direction from the terminal outer section instead of
saving a fixed vector:

```json
"start_constraint": {
  "normal_to_profile": true,
  "magnitude": {"parameter": "height"}
}
```

The same form works for `end_constraint`. Supply exactly one of `direction` or
`normal_to_profile: true`, plus a positive length-valued magnitude. Existing
schema-169 explicit direction constraints load unchanged. The ABI remains 77.
See [the normal-profile example](../tools/model/normal-profile-loft.request.json).

The direction comes from the generated planar section's native normal. Its sign
is chosen using the adjacent section's area center, following increasing station
order at both ends. Reversing station order reverses the forward normals.
Profile plane edits and parameter changes therefore update the direction during
regeneration. Coincident centers or an adjacent center lying in the terminal
plane give no unambiguous forward normal and are rejected. Explicit hole tracks
use their outer station's normal, consistent with their required coplanarity.

In the studio, select **Normal to profile** for either end condition. Direction
component inputs are hidden, and the strength can still use an entered value or
length parameter. Existing strength formulas remain preserved. Switching back to
**Tangent direction** restores the entered direction draft; **Free** removes the
end constraint. Native derivative verification, station interpolation, monotonic
span checks, all-instance validation and model history remain the same.

Tests cover tilted planes, reversed order, parameter-driven frame edits, invalid
mode combinations, forward and backward family variants, saved mode/formulas and
editor switching. The normal-profile benchmark checks 1,000 regenerations within
10 seconds alongside the existing unconstrained and explicit-direction cases.

## Tangent guides and inspection

For a direct loft output, opening **Edit loft …** shows the accepted native end
conditions on the studio canvas. A verified **Preview solid** shows the draft's
conditions. Start handles are blue and end handles purple, with a point at each
terminal section's area center. Arrows follow increasing station order: away
from the start section and toward the end section. Each arrow is one-third of
the resolved strength, matching a cubic Hermite control handle; its label shows
the full strength in millimetres. They are read-only overlays.

Field edits clear the old guides until another verified Preview. Rejection,
Close, model/instance changes and pending Apply also clear them. Late replies
cannot restore old guides. Free ends have no guide. The canvas only displays
annotations for the exact selected instance and displayed loft output.

The native annotation stream includes `loft-start-tangent` and
`loft-end-tangent` constraint records. Their `detail.measurement` is
`loft_end_tangent`; details include the source profile, direction mode, area
center, normalized direction, full derivative vector, strength, and handle scale.
`anchors` give the arrow tail/head in family millimetres. Parameter expressions
stay in the saved feature graph; these resolved values are inspection metadata.

`GeneratedResult::loft_end_tangents` exposes the same resolved native values to
Rust callers as `ResolvedLoftEndTangent` records. Supply the operation and resolved
parameters used for that generation. It shares the construction resolver for
explicit and profile-normal directions, does not alter source geometry, and
retains no native handles. The profile-loft scale benchmarks now include these
inspection queries for every regeneration, with handle-count checks. No model
schema or C ABI change is needed (schema 170, ABI 77).

## Profile correspondence (schema 171, ABI 78)

Constrained smooth lofts can override the start and traversal of individual
outer or hole profiles. The `correspondence` map uses saved profile feature IDs,
so moving a station keeps its settings attached to that profile:

```json
"correspondence": {
  "upper": {
    "start_point": {
      "components": {
        "x": {"literal":{"value":0,"dimension":"length","unit":"millimeter"}},
        "y": {"parameter":"upper_radius"},
        "z": {"parameter":"height"}
      }
    },
    "reverse": true
  }
}
```

Both fields are optional. With no anchor, reversal retains the original seam.
The anchor is a family-space length vector, supporting parameter expressions.
For a boundary with multiple edges it must match a unique existing vertex within
1e-6 mm. For a single full periodic edge, such as a circle or ellipse, any point
on that curve can move the seam exactly within the same tolerance. This does
not split edges or approximate curves. Arbitrary starts on nonperiodic closed
splines and interior points of multi-edge boundaries are rejected.

At least one start/end constraint is required; free or ruled lofts reject manual
correspondence because their automatic compatibility step could undo the saved
ordering. Matching edge counts and all existing derivative, fold-back and valid
solid checks still apply. Alignment copies source geometry, composes original
edge ancestry through lofting and hole subtraction, and never edits the sketches.
Unknown profile IDs and anchors that no longer match the regenerated boundary
fail instead of falling back to another seam.

In **Edit loft**, each outer/hole station has **traversal** and **start anchor**
controls. Choose an entered point or retain the existing formula. Entered XYZ
coordinates use family millimetres; use **Show loft station** to inspect a source.
Removing a station or hole track removes its alignment entries; moving stations
retains their profile IDs. Preview checks every family variant before Apply, and
saved correspondence participates in undo, redo and Save. Anchor picking and
seam markers on the canvas are not yet implemented.

[The aligned-profile example](../tools/model/aligned-profile-loft.request.json)
uses radius/height parameters for both circular seams. The scale benchmark checks
1,000 regenerations, exact volume, original source edge counts, tangent queries
and released handles against the existing 10-second budget.
