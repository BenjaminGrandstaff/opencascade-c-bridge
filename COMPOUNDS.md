# Compound tool groups

Schema 86 adds `Compound { inputs }` using the existing native compound API.
Native ABI remains 52. Inputs are 1–10,000 distinct feature IDs. The group
contains the existing shapes with their original geometry and locations.
Empty lists, duplicate IDs, unknown references, oversized lists and invalid
grouped geometry are rejected.

A compound groups geometry without fusion or sewing. It can contain solids,
faces, wires and edges. Overlaps remain; grouping does not repair gaps or make
one connected solid. Compound volume sums its solids and can double-count
overlapping material. Use `Fuse` when a material union is required, or `Sew`
and `MakeSolid` for closed shell construction.

The [multi-hole plate example](tools/model/multi-hole-plate.request.json)
groups nine independent cylindrical tools, then cuts them from a plate in one
Boolean operation. Spacing, hole radius and plate thickness are editable.
The example uses a fixed 3×3 layout; this feature does not create patterned
copies or infer a hole count. Existing transforms can provide placed children.

The group depends on each input. Child edits rebuild the group and dependent
features while reusing unaffected sources. Failed edits retain accepted
geometry. Saved documents round-trip; older schemas migrate to 86.

Grouping preserves child topology identity rather than creating generated or
modified history. A downstream cut records history against child faces and
edges contained in its compound tool. A group's own `Generated`/`Modified`
queries do not substitute for selecting a child feature directly.

The viewer draws each child's geometry. A direct compound output shows a
structural label with referenced input count and source IDs. Measured extents
and linked ancestor controls remain available. Selecting `tools` alongside
`body` in the example shows both the cutters and the completed plate.

Lookup and uniqueness checks cost O(N) time and memory; native validation and
inspection follow combined topology size. Location-only child transforms
share source geometry. Scale gates check 100 nine-tool cuts and a 10,000-child
group of located boxes, including volume, bounds and released handles, each
within 15 seconds.

Release gates passed in 1.461 seconds for 100 nine-tool cuts and 7.303 seconds
for the 10,000-child grouping case.
