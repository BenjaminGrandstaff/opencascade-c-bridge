# Signed skin offsets

Schema 85 adds `Offset { input, distance, tolerance }` using the existing
`Session::offset` API. Native ABI remains 52. The
[offset-part example](../tools/model/offset-part.request.json) exposes a spherical
source, signed `allowance`, and `offset_tolerance` through MCP and the viewer.

Distance is a finite, nonzero length. Tolerance is a finite, positive length.
The offset follows the source surface normals; positive distance expands a
properly oriented closed solid, and negative distance contracts it. This is a
joined skin offset with the native API's default join behavior. Sharp outward
corners can gain rounded transition faces. Offsets are not uniform scaling:
they move surfaces by a distance rather than multiplying point coordinates.

Use `Hollow` when removing selected faces to create an open thin-walled part.
An offset alone does not specify wall thickness or guarantee a particular
result topology. Large distances, narrow features, self-intersections and
unsupported surfaces can fail in the kernel. Native diagnostics and the
session validation/healing policy remain in force; this feature does not add
join-style controls or a separate face-thickening operation.

Source geometry remains unchanged. Native generated/modified face history
supports downstream semantic selectors. Distance and tolerance expressions
enter incremental signatures, so their edits reuse the source and rebuild
the offset and dependent features. Failed edits retain accepted geometry.
Older documents migrate to schema 85 without changing existing features.

The viewer shows a signed driving label and measured result spans. Distance
and tolerance controls link to the label, which is anchored at the result
bounding centre. Metadata records the source and evaluated values. This label
does not claim a measured wall thickness or show a speculative normal arrow.

Feature bookkeeping follows native topology/history size; kernel cost depends
on surface intersections and joins. Linked instances share one generated
variant. Tests check analytical spherical volume and bounds for outward and
inward offsets, retained source ancestry, selective rebuilds, dimensional
errors, failed edits, document round trips and released handles. Scale gates
cover 500 signed offsets and 10,000 linked offset parts, each within 10 seconds.

The release gates passed in 0.246 seconds for 500 offsets and 0.086 seconds
for 10,000 linked parts.
