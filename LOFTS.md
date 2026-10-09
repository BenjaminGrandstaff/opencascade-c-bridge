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
For a hollow transition, loft the outer and inner profiles separately and cut
the inner solid. Native generated source-edge history is retained for later
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
