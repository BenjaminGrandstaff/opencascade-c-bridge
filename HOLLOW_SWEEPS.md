# Sweeping hollow sketch profiles

The existing `sweep` feature now accepts a valid planar face with up to 100 inner
boundaries. The current model schema is 89; the current native ABI is 52.

```json
"sweep": {
  "profile": "annular_region",
  "path": "route",
  "orientation": "corrected_frenet"
}
```

Build `annular_region` with `planar_region` over independent outer/inner sketch
profiles. Place the section across the route start, as with other sweeps. The
[hollow-sweep AI example](tools/model/hollow-sweep.request.json) combines a line
and tangent quarter arc, with outer radius, inner radius, run and bend-radius
controls. Both boundary sketches and the route remain visible beside the solid;
route length and all section controls stay linked to native geometry.

For a holed face, the engineering layer identifies the outer boundary by its
filled native area. Each boundary is swept using the same native orientation
and mitered-corner rule. Native common-volume checks require inner sweeps to
stay inside the outer solid, and native distances reject overlapping or touching
inner volumes. One native cut removes the compound of inner sweeps. The result
must be one valid positive-volume solid with volume equal to outer minus inner
volumes within a 1e-7 relative tolerance (1e-12 mm³ floor).

Each native boundary sweep applies the existing conservative bend-folding
check: exact bend radii for lines/circles and sampled curvature for other paths.
The shape must also pass native validity checks. Invalid bore, path and bend
edits preserve previously accepted geometry. Unchanged sections and routes are
reused during incremental regeneration. Open-wire surface sweeps and simple
single-boundary sweeps keep their existing execution path.

Corrected Frenet, Frenet and binormal modes are tested on curved annular sweeps;
fixed mode is tested on straight multiple-bore sweeps. Orientation and source
section placement continue to determine the native transport. The low-level
C `sweep` operation and `occt-bridge::Session::sweep` still handle one boundary
at a time; the engineering feature composes those operations for holed faces.

Native sweep/cut histories are composed before selecting the final solid.
ABI 50 adds `occt_bridge_shape_subshape_with_history` and safe Rust
`Session::subshape_with_history`. They share selected geometry and filter parent
history targets to that subset. Located history is expanded only on this
explicit path. Ordinary subshape extraction remains available without that
cost. Sources absent from the subset become deleted relative to the subset;
parent geometry and history remain unchanged. Retained ancestry survives parent
handle release. Planar regions now use the same API to retain their native cut
face history.

Subset extraction takes O(parent topology + history records and targets).
Boundary sweeps and booleans depend on native curve/surface topology. Pair
screening is O(H²) for H inner sweeps, using cached bounds before native distance
queries. H successive history compositions cost O(HM) for aggregate history size
M; source history and scratch geometry remain bounded by the 100-bore limit.
Copies share geometry, and temporary native handles are released on success or
failure. Scale gates check 250 curved hollow sweeps and one 100-bore straight
sweep, each within a 10-second budget, with analytical volumes, every bore's wall
ancestry and zero retained handles. Region scale gates also continue to pass.

Hollow sweeps and schema-80 hollow profile lofts share their native containment,
subtraction and history helper. Their source transport/fitting operations remain
specific to each feature.
