# Patterns inside a part

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


## Skipped copies, curves, fills and tables (schema 184)

All patterns make an unfused compound of copies. Fuse it with the source or
cut with it, as with linear and circular patterns.

### Skipped copies

`linear_pattern`, `circular_pattern` and `curve_pattern` take an optional
`skip` list of copy indices to leave out; copy 0 is the original. Indices must
be below the count, and at least one copy must remain.

### Along a curve: `curve_pattern`

```json
{ "curve_pattern": { "input": "bolt", "path": "bolt-arc", "count": { "literal": { "value": 7, "dimension": "scalar", "unit": null } },
                     "orientation": "fixed", "skip": [3] } }
```

`path` is a feature whose output is one edge or wire, such as a
`sketch_open_wire` or `sketch_wire`. Model the input at the path's start:
- On an open path, copy *i* moves to the point *i*/(count − 1) of the way
  along by arc length, ends included.
- On a closed path, copies divide it into count equal arcs, with none
  doubled at the seam.

`fixed` copies only move. `tangent` copies also turn with the path. Their
rotation is carried from the start in at least 256 small steps (at least 8
per gap), always the least turn from one tangent to the next, so copies do not
twist about the path and a U-turn is unambiguous. A path that turns back on
itself (a cusp) is refused. Count is 1 to 10,000.

### Fill a region: `fill_pattern`

```json
{ "fill_pattern": { "input": "vent", "region": "vent-region", "anchor": { "...": "site point of the input" },
                    "direction": { "...": "row direction" }, "spacing": { "parameter": "pitch" },
                    "row_spacing": { "parameter": "pitch" }, "layout": "staggered",
                    "margin": { "literal": { "value": 3, "dimension": "length", "unit": "millimeter" } } } }
```

**The region.** It is a planar face: the only face of `region` (a sketch face,
for example), or the one an optional `face` selector picks on a solid (the
top of a plate, say).

**The grid.** It passes through `anchor` projected onto the face's plane.
`anchor` is the point of the input that marks its site, such as a hole's
axis. Rows run along `direction`, projected into the plane, with sites every
`spacing`. Rows are `row_spacing` apart, along the plane normal × direction.
`staggered` shifts every other row by half a spacing.

**Which sites are kept.** A site is kept when it lies inside the face at
least `margin` from its boundary, holes included; set the margin to the
copy's radius plus the web you need. Copies move by site − anchor, all in
the plane.

**How sites are tested.** The boundary is sampled into segments in the plane.
Each row is tested against its own crossings, and a spatial hash finds
boundary segments within the margin. The sampled answer decides only sites
clearly away from the boundary and from the margin. Sites within the
sampling error of either are decided by OCCT's exact face classification and
distance, so a site exactly at the margin is kept and one on the boundary is
not.

**Limits.** At most 1,000,000 grid sites are examined and 10,000 copies
placed.

### Listed offsets: `table_pattern`

`offsets` is 1 to 10,000 length vectors; the input is copied by each. Include
a zero offset to keep its own place. Use it for uneven spacing or a hand-placed
layout.

### Example, tests and scale

[`vent-panel`](../tools/model/vent-panel.request.json) cuts a 160 × 100 × 3
panel with:
- a staggered vent field, filled into a square kept clear of a cable hole and
  driven by `pitch`;
- six bolt holes on an arc, with the middle of seven skipped;
- three mounting holes at listed offsets.

A command test matches its volume to a grid counted independently, at two
pitches.

Kernel tests check skipped copies and positions on open and closed paths,
and that tangent copies turn. Fill sites are compared with a brute-force
count from the exact rectangle-with-hole geometry, for grid and staggered
layouts at five margins, including a margin exactly reaching the outer
sites. Further checks cover a face selected on a solid, table offsets, and
refused grids.

The scale suite (`--pattern-types-only`, release build) runs:

| Pattern | Copies | Time |
|---|---|---|
| fill: 1,000 mm region with a hole | 9,668 | 5.6 s |
| curve: tangent copies round a circle | 1,000 | 0.6 s |
| table: listed offsets | 10,000 | 5.4 s |

The budget is 60 s. Most of the time is placing and validating the solids, as
for linear patterns.

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
