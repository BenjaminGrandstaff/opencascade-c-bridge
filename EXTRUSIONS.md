# Extrusion end conditions

`extrude` accepts a valid planar face or closed planar wire. Schema 73 adds
an optional `extent`; omitted extents retain the original `distance` behavior.
The native ABI remains 47.

| Extent | Direction vector | Result |
| --- | --- | --- |
| `"distance"` | Length and orientation | Starts at the sketch and travels the full vector |
| `"symmetric"` | Total length and orientation | Half the vector on each side of the sketch |
| `{"up_to_face":{"target":"stop","face":SELECTOR}}` | Orientation; magnitude ignored | Ends at exactly one selected face of `stop` |
| `{"up_to_next":{"target":"stop"}}` | Orientation; magnitude ignored | Ends at the nearest forward face of `stop` that covers the entire profile |

Vectors remain length-valued for compatibility, including geometric extents.
They must be finite and nonzero. Negative and oblique directions are supported.
Face limits must be planar and parallel to the profile plane. Native face
intersections check the entire translated profile, including holes. Small,
misplaced, behind-the-sketch or coincident faces do not qualify. Curved and
inclined limiting surfaces are not supported in this increment.

`up_to_next` searches only its explicit target output. It skips faces that do
not cover the entire profile; it does not imply partial contact, clipping,
assembly-wide obstacle searches or fusion with the target. Select `up_to_face`
when a specific topological face must be the limit. Existing semantic, named,
persistent and history face selectors are accepted. Ambiguous selections fail.

The target output and selector references enter the feature dependency graph.
Moving the target rebuilds the prism while reusing an unchanged sketch.
Selectors' parameter expressions also participate in incremental invalidation.
Source-edge generated-face history is retained, including centered prisms.

The shared standalone/studio viewer displays the actual generated travel for
face limits and centers symmetric dimension anchors about the sketch. Face
limit lengths are measured; edit the target parameters or direction to change
them. Distance and symmetric vector lengths remain driving dimensions.

The [example request](tools/model/extrusion-limits.request.json) is available
through MCP `occt_get_example` as `extrusion-limits`. Its `depth` parameter moves
the target box: `body` ends at its near face, `selected` at its far face, and
`symmetric` splits the total depth about the sketch.

Candidate discovery traverses target topology once. Each eligible face uses
one exact coplanar Boolean intersection; cost depends on its boundary and
profile complexity. Temporary handles are released immediately. The
`extrusion_extents` benchmark checks 1,000 three-extent regenerations, volumes,
validity and zero retained handles against a 10-second budget.
