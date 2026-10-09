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

The [patterned-plate example](tools/model/patterned-plate.request.json) repeats
one cylindrical cutter into columns, then repeats that column group into rows.
One Boolean cut removes the grid from a plate. Row/column counts, spacing, hole
radius and plate thickness are editable; count changes also resize the plate.
The example limits each grid count to 20 and keeps cutters separated.

Source, step and count dependencies enter regeneration signatures. Count-only
edits reuse the cutter; changed patterns and downstream cuts rebuild. Failed
edits retain accepted geometry, and temporary copy handles are released.
Older saved documents migrate to schema 88 without changing existing features.

Grouping retains the seed topology and located copied geometry, but does not
aggregate each translation's operation history. Use semantic selectors on the
pattern output to choose copied faces or edges. A history query against the
original source alone does not identify every repeated member.

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
