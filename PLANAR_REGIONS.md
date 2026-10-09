# Hollow sketch profiles

Schema 79 adds a `planar_region` feature; native ABI 49 is unchanged.

```json
"planar_region": {
  "outer": "outer_profile",
  "holes": ["inner_profile", "mounting_hole"]
}
```

Inputs are saved feature-output IDs. Each must be a valid closed planar wire
or a valid planar face with one boundary. The feature accepts 1–100 distinct
inner profiles, separate from the outer profile. It returns one valid connected
planar face with those inner boundaries. Existing sketch entities and profiles
remain unchanged; separate sketch outputs define each boundary.

Native common-area and BREP-distance checks require each hole to be coplanar
and wholly inside the outer boundary, with more than 1e-7 mm boundary clearance.
Holes must be disjoint, including their interiors, with the same clearance.
Touching, crossing, overlapping, nested, outside, open and nonplanar inputs are
rejected. Already-holed input faces are rejected rather than flattening their
boundary hierarchy. Area comparisons use a 1e-7 relative tolerance with a
1e-12 mm² floor, alongside the boundary-distance checks.

The inner faces are assembled into one native compound and removed by one
native face cut. The result must have one face, exactly one more boundary wire
than the number of holes, and finite positive area equal to the expected
remaining area within tolerance. Curves stay in their native BREP representation;
there is no polygon conversion. Temporary native shape handles are released.

Use the result directly as an `extrude` or `revolve` profile to make hollow
solids. Symmetric revolution is supported. The current native sweep and
saved-profile loft operations accept single-boundary profiles, so hollow sweep
and loft transitions still use separate outer/inner constructions and a cut.

All boundary outputs enter dependencies. Hole edits rebuild the region and its
downstream solid while reusing unaffected sketches. Invalid edits retain the
last accepted geometry. The [hollow-profile AI example](tools/model/hollow-profile.request.json)
builds an annular extrusion with `outer_radius`, `inner_radius` and `height`
controls. Both source sketches and their constraints remain visible in the
shared viewer, and their controls link to the solid.

Extrusions and revolutions record native generated history from region boundary
edges. Tests also verify original circular boundary-edge history for concentric
and displaced holes. The face is extracted from a native cut; that cut's
operation-level face history is not attached to the extracted face. Use region
boundary-edge selectors for downstream generated-face references.

Pair screening uses cached exact bounding boxes before native distance queries.
Bookkeeping is O(H²) for H holes (at most 4,950 pairs), with O(H) retained scratch
handles and bounds. Native face/common/cut work depends on curve topology and
intersection cost. The scale gates check 500 annular extrusions and one 100-hole
region, each with a 10-second budget, exact area/volume and zero retained handles.
Tests cover malformed inputs, multiple holes, dependencies, failed edits,
source edges and hollow toroidal revolutions.
