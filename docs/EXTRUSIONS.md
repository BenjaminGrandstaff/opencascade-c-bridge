# Extrusion end conditions

`extrude` accepts a valid planar face or closed planar wire. Schema 73 adds
an optional `extent`; omitted extents retain the original `distance` behavior.
Schema 74 extends face limits to inclined and curved surfaces. Native ABI 48
adds bounded face termination and exact forward surface-ray intersections.

| Extent | Direction vector | Result |
| --- | --- | --- |
| `"distance"` | Length and orientation | Starts at the sketch and travels the full vector |
| `"symmetric"` | Total length and orientation | Half the vector on each side of the sketch |
| `{"up_to_face":{"target":"stop","face":SELECTOR}}` | Orientation; magnitude ignored | Ends at exactly one selected face of `stop` |
| `{"up_to_next":{"target":"stop"}}` | Orientation; magnitude ignored | Ends at the nearest forward face of `stop` that covers the entire profile |

Vectors remain length-valued for compatibility, including geometric extents.
They must be finite and nonzero. Negative and oblique directions are supported.
Face limits may be inclined planes or curved bounded surfaces. Parallel planar
limits use exact coplanar coverage intersections. Other limits split a finite
search prism with the actual OCCT face. The chosen valid solid must retain the
entire base, include a limiting cap, avoid the search end, and keep the cap
strictly separated from the base. Small, misplaced, backward, coincident or
partially covering faces fail. Target face boundaries are respected; the
surface is not silently extended. The retained region is the base-connected
solid, including the first enclosing cutoff for a surface crossed twice.

`up_to_next` searches only its explicit target output. It skips faces that do
not terminate the entire profile. Among complete cutoff candidates, the chosen
solid must be contained in every competing candidate. Crossing limits with no
single earliest cutoff produce an ambiguity diagnostic; use `up_to_face` to
resolve it. This does not imply assembly-wide obstacle searches or fusion with
the target. Select `up_to_face`
when a specific topological face must be the limit. Existing semantic, named,
persistent and history face selectors are accepted. Ambiguous selections fail.

The target output and selector references enter the feature dependency graph.
Moving the target rebuilds the prism while reusing an unchanged sketch.
Selectors' parameter expressions also participate in incremental invalidation.
Source-edge generated-face history is retained, including centered prisms.

The shared standalone/studio viewer measures an exact forward surface hit from
the profile area centroid along the extrusion direction. For nonuniform caps,
this is the travel on that centroid ray, not a claim of constant thickness;
metadata records `measurement: "profile_centroid_ray"`. A holed or concave
profile may have its centroid outside material: if the ray misses, the viewer
omits its distance glyph/value rather than estimating one. Symmetric anchors
remain centered about the sketch. Edit target parameters to change geometric
limits; distance and symmetric lengths remain driving dimensions.

The [example request](../tools/model/extrusion-limits.request.json) is available
through MCP `occt_get_example` as `extrusion-limits`. Its `depth` parameter moves
the target box: `body` ends at its near face, `selected` at its far face, and
`symmetric` splits the total depth about the sketch.

The [curved example](../tools/model/curved-extrusions.request.json), also named
`curved-extrusions` in MCP, demonstrates a spherical next limit and an inclined
selected limit. Changing `depth` moves both targets.

Candidate discovery traverses target topology once. Eligible nonparallel faces
use native face splitting and exact base/end/cap coverage checks; Boolean cost
depends on boundary/profile complexity. Next-face selection needs at most one
containment intersection per competing candidate, with linear candidate storage.
There is no face-pair matrix or tessellated coverage test. All temporary handles
are released after regeneration. The `extrusion_extents` gate retains its 1,000
three-extent workload; `curved_extrusion_extents` checks 250 spherical/inclined
regenerations, volumes, ray witnesses and zero retained handles. Both have a
10-second budget.
