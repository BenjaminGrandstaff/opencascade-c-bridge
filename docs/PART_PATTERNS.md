# Linear patterns inside a part

Schema 88 adds `LinearPattern { input, step, count }`, using the existing native
translation and compound APIs. Native ABI remains 52. This creates repeated
geometry within a feature tree; assembly instance patterns remain separate.

`input` names one earlier feature. `step` is a length-valued vector in family
coordinates. `count` is a dimensionless scalar expression whose evaluated value
must be an exact integer from 1 to 10,000. The source placement is member zero;
member i is translated by i × step. Negative step components are supported.
Multiple copies require a nonzero step. Each transform uses the original
source, so placement does not accumulate transform drift.

Members share source geometry through native locations. The result is a
compound: copies are grouped without fusion or sewing, and overlaps remain.
Compound volume can double-count overlapping solids. Use a Boolean union for
net material geometry. A nested pattern repeats the whole input group, allowing
row/column grids. Patterns do not generate per-copy feature or assembly IDs.

The [patterned-plate example](../tools/model/patterned-plate.request.json) repeats
one cylindrical cutter into columns, then repeats that column group into rows.
One Boolean cut removes the grid from a plate. Row/column counts, spacing, hole
radius and plate thickness are editable; count changes also resize the plate.
The example limits each grid count to 20 and keeps cutters separated.

Source, step and count dependencies enter regeneration signatures. Count-only
edits reuse the cutter; changed patterns and downstream cuts rebuild. Failed
edits retain accepted geometry, and temporary copy handles are released.
Older saved documents migrate to schema 88 without changing existing features.

Grouping retains the seed topology and located copies, and aggregates their
operation histories. A source face can resolve to all its patterned counterparts.
Nested patterns explicitly carry the input group's history through each copy.
Saved-model cuts, unions and intersections compose directly grouped operands,
so history selectors can resolve every repeated bore from the original cutter.

Direct pattern outputs show linked copy count, step length and derived
first-to-last placement span. Span uses source mass-centre positions and
excludes source size. Labels are in family coordinates and follow assembly
placement. Downstream cut views expose the defining controls in their panel.

Bookkeeping costs O(N) time/memory; native topology validation costs O(NT).
A conservative estimate sums source subshape counts, adds a root allowance,
and multiplies by count. More than 1,000,000 estimated entries is rejected
before placing copies, bounding nested expansion. Finite placement checks
reject arithmetic overflow. The geometry/viewer budgets remain in force.

Tests cover changing a nine-hole grid to eight holes, analytical volume,
source reuse, negative steps, invalid counts/units, nested expansion bounds,
rollback and cleanup. Release gates cover 100 tool-grid cuts and one 10,000-copy
pattern, each within 15 seconds.

Release gates passed in 1.946 seconds for 100 grid cuts and 7.241 seconds for
the 10,000-copy pattern, which retains only the source and final output handles.


## Circular patterns (schema 89)

`CircularPattern { input, origin, axis, count, angle_step_radians }` groups
absolute rotations of the source about an axis. Origin is a length vector;
axis is a dimensionless finite nonzero vector. Count is an integer-valued
scalar from 1 to 10,000, including the unrotated seed. Angular step is signed
scalar radians. Every member rotates the original source, avoiding cumulative
rotation drift. Copies share source geometry through native locations.

For multiple copies, spacing must be nonzero and the magnitude of the
first-to-last sweep, (count − 1) × step, must be below one full turn. This
rejects wraparound and repeated end placement. A full bolt circle uses
step = 2π/count, with no member duplicated at 2π. Negative steps reverse the
winding. Partial groups use a smaller signed step. The result stays an unfused
compound; overlapping geometry remains. Grouping aggregates per-copy histories,
so source-based selectors can select repeated output topology.

The [bolt-circle example](../tools/model/bolt-circle.request.json) repeats a
cylindrical cutter and removes the group from a disk in one cut. Count,
`sweep_angle`, bolt radius, hole radius and thickness are editable. Spacing is
sweep_angle/count, so partial sweeps exclude the terminal sweep angle. An
illustrative chord-distance guard keeps holes separated; it does not establish
bolt strength, fastener fit or a manufacturing standard.

Circular patterns share the linear pattern's conservative 1,000,000-topology
expansion bound. Bookkeeping is O(N), and native topology validation is O(NT).
Temporary copy handles are released after grouping. Viewer labels show count
and signed angular spacing; a bounded 33-point arc lies in the plane through
the source mass centre, at its radial distance from the axis. An on-axis source
has no radial arc extent. Family coordinates follow assembly placement.

Tests cover analytical bolt-hole volume, count edits with source reuse,
signed partial turns, invalid counts/units/axes, duplicate wraparound and
failed-edit retention. Release gates cover 100 bolt-circle cuts and 10,000
radial copies, each within 15 seconds.

Circular release gates passed in 0.946 seconds for 100 bolt-circle cuts and
2.267 seconds for 10,000 radial copies, with only source/output handles retained.


## Retained pattern ancestry

Compound construction merges child histories using indexed source/target
identity sets. Generated ancestry takes precedence over modified ancestry for
the same target; related targets are unique and belong to the actual group.
Unchanged children keep identity without being falsely marked modified, and a
source kept by any branch is not marked deleted. Histories outlive released
child handles. The native group API retains existing child histories; callers
can explicitly compose intermediate histories before grouping deeper chains.

The saved-model layer does that composition for nested group patterns and for
`Cut`, `Fuse` and `Common` with direct compound/pattern operands. An original
cutter's cylindrical face now identifies all nine grid bores or all six
bolt-circle bores via `FaceSelector::History`. Arbitrary transform chains still
use their existing explicit composition semantics.

This improvement leaves schema 89 and ABI 52 unchanged. Rebuild the native
library and Rust binaries together. Merge cost is O(child topology + child history records/relations) with O(retained records/relations) memory. There is no per-copy
composition against a growing aggregate. Queries necessarily store one
relationship per retained counterpart. Updated 10,000-member gates verify all
10,000 source-face counterparts and released handles within 15 seconds.
