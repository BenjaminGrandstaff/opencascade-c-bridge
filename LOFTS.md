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

The [profile-loft example](tools/model/profile-loft.request.json) is available
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
geometry. The [hollow-loft AI example](tools/model/hollow-loft.request.json)
shows a tapered duct, all four boundary sketches, linked bore controls and
outer-section centroid spacing. The current native ABI is 52; the current model schema
is 85. The low-level wire-loft API still constructs one boundary track at a time.

For N stations and H holes, station bookkeeping and pair screening are
O(NH²), with cached bounds before native pair distances. Complete inner-solid
pair screening is O(H²), and H history compositions cost O(HM) for aggregate
history size M. Native fitting/common/cut work depends on curve/surface topology.
Scratch handles are bounded by the track topology and released after validation
and construction. Scale gates check 500 tapered hollow lofts and a 100-bore loft,
with analytical volume, wall ancestry and zero retained handles, each within a
10-second budget. Hollow sweeps use the same validated subtraction/history helper
and continue to pass their scale gates.
