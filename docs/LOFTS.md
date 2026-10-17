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
automatically split or reorder edges. Schema 172 adds explicit `split_points`
to make the resulting section topology compatible while retaining exact curves;
see [explicit boundary partitions](#explicit-boundary-partitions-schema-172-abi-79).

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
not approximate curves. Anchors alone do not split edges; use `split_points` to
create boundary vertices before alignment. Starts at interior points of a
multi-edge boundary still require a matching partition vertex.

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
saved correspondence participates in undo, redo and Save. Use **Pick … start in canvas** to select an exact corner or supported periodic
boundary point.

[The aligned-profile example](../tools/model/aligned-profile-loft.request.json)
uses radius/height parameters for both circular seams. The scale benchmark checks
1,000 regenerations, exact volume, original source edge counts, tangent queries
and released handles against the existing 10-second budget.

## Canvas starts and traversal markers

For a direct constrained smooth loft, **Edit loft** shows the native start of
every outer and hole profile. Teal outer markers and amber hole markers label
the station; short arrows follow the actual boundary traversal. These sample the
first eight percent of the first edge's parameter range, rather than displaying
an estimated analytic tangent. Blue/purple end-condition handles remain separate.
Verified previews replace the accepted markers. Fields, rejection, Close and
model/instance changes clear them; free/ruled lofts have no manual seam markers.

Choose **Pick outer station … start in canvas** or the corresponding hole button.
The accepted source boundary appears in amber. Multi-edge profiles expose only
existing corners; a supported full periodic single-edge curve allows a continuous
boundary pick. Native snapping verifies the chosen point. Fit frames the source,
dragging orbits, and Escape cancels while retaining the loft draft. Old requests
cannot restore overlays after cancellation, field edits or model changes.

A confirmed pick replaces that profile's start formula with fixed family-space
XYZ coordinates. Cancellation preserves the formula. Preview still checks all
variants and rejects an anchor that does not match another variant or a later
profile edit; retain a parameter formula when the anchor must follow edits.
No topology index is stored. Native queries and picking are read-only until the
usual Preview/Apply/Save transaction.

`GeneratedResult::loft_profile_seams` exposes `ResolvedLoftProfileSeam` records to
Rust callers, using the same alignment operation as construction and retaining
no native handles. Native annotations use `detail.measurement: loft_profile_seam`
with profile ID, zero-based station/hole indices, reversal and five family-space
samples in `anchors`. The read-only `/api/loft-profile-pick` endpoint returns a
bounded accepted boundary snapshot, or an exact snapped point for a finite hint
and positive distance tolerance. Both require the current version and instance.
These additions keep model schema 171 and ABI 78 unchanged.

## Explicit boundary partitions (schema 172, ABI 79)

A constrained loft can now match different source edge counts using exact
`correspondence[profile_id].split_points`. These are length-valued family-space
vectors, supporting the same parameters and formulas as start anchors. They
split copied boundary edges before the saved start and reversal are applied.
The original sketches keep their original curves and topology.

For example, one split point on a circle creates two exact arc edges:

```json
"correspondence": {
  "lower": {
    "split_points": [{
      "components": {
        "x": {"literal":{"value":0,"dimension":"length","unit":"millimeter"}},
        "y": {"parameter":"lower_radius"},
        "z": {"literal":{"value":0,"dimension":"length","unit":"millimeter"}}
      }
    }]
  }
}
```

The [circle-to-diamond example](../tools/model/partitioned-profile-loft.request.json)
uses three cardinal split points to match a single-edge circle to a four-edge
polygon. Both ends retain profile-normal directions and parameter-linked
strengths. Original circle edges retain all four generated wall faces. Splits
also work independently on explicit hole tracks, with ancestry composed through
partitioning, alignment, lofting and subtraction.

Each profile accepts at most 128 distinct split points and 128 resulting edges.
A point must lie within 1e-6 mm of a unique native boundary curve location.
An existing vertex is already a partition and is retained. Duplicate requests,
off-boundary or ambiguous points, wrong dimensions and edge-budget overflow fail.
Splits are ordered along the native curves, preserving traversal and the
original seam; they do not approximate curves. A start anchor can reference a
new partition vertex. Resulting station edge counts and correspondence must
still match; this does not automatically choose pairings between sections.
All existing derivative, fold-back, valid-solid and bore-containment checks apply.
Free and ruled lofts continue to reject explicit correspondence.

In **Edit loft**, choose **Add … split point**, enter XYZ family millimetres, or
retain an existing formula. **Use … split … as start** copies that point's
expression into the start anchor. Editing a copied literal split later requires
choosing the start again. Station reordering keeps splits attached to profile
IDs; removal prunes their entries. The canvas start picker shows the draft's
exact split vertices and snaps to them. Preview checks every instance before
Apply; Save and undo/redo preserve the split expressions. Parameter changes
invalidate the loft even when its source sketches are reused.

`Session::partition_profile_wire` wraps the immutable native partition operation.
Native alignment also now preserves the orientation of already-reversed copied
edges. Construction and seam inspection use the same partition/alignment helper.
The read-only `/api/loft-profile-pick` accepts optional draft `split_points`,
resolves them for the selected instance, and publishes their derived vertices
without editing the document. Constrained, normal and aligned examples are also
available through MCP, alongside `partitioned-profile-loft`.

The `partitioned-profile` scale case refines both circular stations into four
exact arcs. It checks 1,000 regenerations, analytic volume, native seam/end
inspection, unchanged source edge counts and released handles against the
existing 10-second budget (8.98 seconds in the recorded run).

### Pick new boundary splits in the studio

**Pick … split in canvas** selects a new point anywhere on a profile boundary,
including the interiors of lines, arcs and spline edges in multi-edge wires.
The amber boundary is a bounded display snapshot; the click is projected onto
native trimmed curves and verified with the exact partitioner. Existing corners,
already-authored split vertices, ambiguous curve hits and edge-budget overflow
are rejected. Picks near displayed corners snap to those exact vertices before
validation, avoiding tiny duplicate cuts caused by pixel rounding. Drag orbits,
Fit frames the profile, and Escape cancels the pick.

A successful pick appends a fixed family-coordinate length expression while
preserving the existing split and start formulas. It can then be chosen with
**Use … split … as start**. Parameter-dependent variants still require Preview:
a fixed point that leaves another instance's boundary prevents Apply. Editing
fields, Preview, closing the editor or changing the model/instance cancels a
pending pick; late responses cannot modify the draft. Apply, undo/redo and Save
use the existing correspondence definition. Schema 172 and ABI 79 are unchanged.

The read-only `/api/loft-profile-pick` query accepts `purpose: "split"` to snap
curve interiors; omitted `purpose` retains start-anchor behavior. Responses echo
this purpose as well as the model version, instance, profile and draft split
count. Snapshot generation is bounded to 128 edges with 65 samples each (257
samples for a single continuous edge); there are no native requests on hover.

The `tools/bench/loft-picker.mjs` gate exercises the maximum 128-edge / 8,320-sample
split snapshot. The recorded run takes 0.76 ms per hover and 1.8 ms per overlay
redraw against 16/100 ms budgets, and asserts one snapshot request with no hover
requests. Browser verification covers duplicate rejection/retry, retained
formulas and end strengths, valid Preview, Apply, undo/redo, Save and reload.

## Read-only profile diagnosis

**Check loft profiles** inspects the current editor draft without constructing
its solid. Each outer/hole station reports its profile ID, source and aligned
edge counts, exact start in family millimetres, reversal and alignment errors.
The native partition/alignment routines and end-condition resolver are shared
with construction. Different counts within a constrained track identify every
affected profile and suggest exact splits or revised profiles. Unknown alignment
IDs, duplicate station IDs, invalid track lengths, incompatible loft modes and
invalid end directions/strengths receive stable issue codes.

Existing split/start expressions are evaluated for the selected instance.
Only profile dependencies are regenerated, so a failing loft or unrelated
downstream geometry does not prevent inspecting its usable sources. Source
regeneration failures still return errors. Inspection retains no scratch shape
handles and never edits the document, accepted mesh, history or disk. Reports
are limited to 8,192 boundaries; the limit is checked before source generation.
Each native aligned boundary remains limited to 128 edges.

A passing report means the listed profile checks passed for this instance.
It does **not** certify edge pairing, twist freedom, longitudinal fold-back,
hole containment, final solid validity or family-wide requirements.
`solid_checked` is always false. Preview and Apply retain all existing native
solid and all-instance validation. Fields, new checks, model changes and Close
invalidate pending editor reports.

For AI use, pass `instance` and `loft_feature` to `occt_inspect_model` or
`occt_inspect_build`. These options are mutually exclusive with `output` and
`sketch_feature`; selectors still require `output`. The `loft` response has
paged `profiles` and `issues`, `checks_passed`, `constrained`, and
`solid_checked: false`. Normal inspection `offset`/`limit` apply to both pages.
For a previously accepted `partitioned-profile-loft` build, call
`occt_inspect_build` with:

```json
{
  "build_id": "BUILD_ID_RETURNED_BY_OCCT_BUILD",
  "instance": "block",
  "loft_feature": "body",
  "limit": 100
}
```

The studio endpoint `/api/loft-profile-report` accepts the current `version`,
`instance`, and draft `operation: {"profile_loft": ...}` and echoes them with a
`report`. The Rust entry points are `diagnose_loft_profiles` for isolated source
regeneration and `GeneratedResult::loft_profile_report` for existing source
geometry with its resolved parameters. Schema 172 / ABI 79 are unchanged.

The `loft_profile_diagnosis` scale gate checks 1,000 isolated diagnoses of a
loft with incompatible edge counts, including source regeneration, stable
per-profile issue codes and zero retained handles. The recorded run is 0.70
seconds against a 10-second budget. Browser checks cover mismatched counts,
invalid anchor isolation, stale report clearing, canvas correction, preserved
end strengths, Preview/Apply, undo/redo and Save.
