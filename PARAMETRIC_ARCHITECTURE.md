# Parametric architecture

The bridge is the geometry-kernel boundary, not the complete engineering
model. Engineering applications built on it should preserve **design intent**
so they can generate and regenerate families of related parts.

## Implementation status

The architecture in this document is both a description of implemented
boundaries and a roadmap. As of ABI version 26, the repository contains three
Rust layers:

1. **`occt-bridge`** safely wraps session-owned OCCT handles. It includes
   generic primitives, reusable wires and faces, transforms, booleans, sweeps,
   lofts, face sewing with operation history, single- and multi-shell solid
   construction, selected-edge treatments, offsets, hollowing, topology traversal,
   oriented face normals, face planarity, edge length, circular radius, and
   midpoint, deterministic sampled, and exact or error-bounded full-edge
   curvature, direct topology
   adjacency, recorded face tangency, physical measurements, BREP persistence,
   STEP import/export, configurable ASCII/binary STL tessellation export,
   validated multi-shell solids with internal voids,
   generated/modified/deleted operation history, and
   history-preserving duplicate handles for transactional reuse.
2. **`occt-recipes`** owns application-level construction. Its wall-torch
   recipe is composed entirely from generic bridge operations. The faceted
   stone recipe builds planar facets, sews them, and closes the shell into a
   solid, matching the volume of the legacy compatibility constructor it
   replaces.
3. **`occt-parametric`** implements the first executable engineering layer:
   typed scalar, vector, integer, Boolean, and choice parameters; explicit
   length units; versioned families; persistent instance identity; sparse
   instance overrides; dependency-ordered feature execution; named results;
   requirement priorities and provenance; validity, volume, mass, datum
   clearance, and relationship-satisfaction verification;
   clone inheritance with cycle detection and explicit detachment; accepted
   result revisions with stale-result retention and explicit freezing;
   independent translation/axis-angle placement; nested assembly frames;
   linear and circular clone patterns; shared generation for clones that
   differ only in placement;
   unit-aware derived scalar arithmetic and functions, derived vector
   composition, and pre-generation constraints;
   semantic face and edge selection with extrema, size, circular and general
   midpoint, sampled, and proven-bound full-edge curvature radius, oriented
   normals,
   adjacency, G1 tangent neighbors, recursive set composition, multi-result and
   operation-history tracking; serializable sewing and single- or multi-shell
   solid feature operations; versioned JSON persistence with schema migration
   and validation; incremental dirty-feature rebuilding; transactional
   cleanup when regeneration or placement fails; and kernel failures reported
   with OCCT's codes and located at the failing feature's selector or input.

Result validation retains OCCT's `BRepCheck` verdicts and diagnostic statuses.
Faces with fewer than 16 wires take the same unpruned checks as
`BRepCheck_Analyzer`. Larger faces check each subshape against a proxy carrying
only its own wire and use bounding boxes to prune wire-containment candidates,
reducing classification from unconditional quadratic comparisons to
O(w log w + candidate pairs).
A differential C++ test compares every reported subshape status across valid
and deliberately invalid planar, periodic, located, intersecting, nested, and
misoriented topology. On the final benchmark build, validation costs 1.59x on
a 100-cut chain and 1.12x on a 400-hole single cut; validating the resulting
400-hole plate directly takes 0.045 s instead of 0.212 s with
`BRepCheck_Analyzer`.

The following major capabilities remain planned:

- additional domain-specific expression functions;
- additional schema migrations and integration with the broader EIL source
  model;
- broader requirement rules such as clearance, interference, minimum radius,
  wall thickness, connectivity, and manufacturing checks;
- sketch profiles feeding extrude, revolve, hole,
  draft, rib, variable-fillet, and later sheet-metal features;
- joints, interference and clearance detection, and motion studies;
- generated drawings with projected views and dimensions;
- full mass properties, FEA mesh hand-off, manufacturability checks, and
  glTF export;
- semantic diff, merge, revision history, and change impact for model
  documents.

## Assembly semantics

Assembly semantics live in the `assembly` module and serialize with the
document.

- **Named datums.** A family declares points, axes, and planes from parameter
  expressions in its own coordinates, such as a hinge axis at a parameterized
  offset. `InstanceGraph::datum` resolves one for an instance in model
  coordinates through its parameters, placement, and enclosing frames.
  Families reject duplicate datum ids, wrong dimensions, and zero directions.
- **Relationships.** `Coincident`, `Parallel`, `Perpendicular`, and
  `Distance` relate two instance datums. They record design intent:
  `check_relationships` reports each as satisfied or
  violated with its linear residual in millimeters and angular residual in
  radians. Each model stores finite positive linear and angular tolerances,
  used by both `solve_placements` and the final checks; defaults retain the
  former 1e-6 mm and 1e-9 rad behavior. `set_relationship_tolerances` changes
  them without exposing mutable assembly state. Coincidence covers
  point, axis, and plane pairs, including an axis lying in a plane; parallel
  and perpendicular treat a plane by its normal, so an axis is parallel to a
  plane when it is normal to the plane's normal. Unsupported pairs, such as a
  distance between an axis and a plane, are rejected when added.
- **Configurations.** A configuration layers per-instance parameter
  overrides and instance suppression over the base graph without changing it.
  Its overrides apply after each instance's own overrides and are inherited
  by clones like ordinary overrides, while a clone's own override still wins
  over a configured value on its source. Setting the active configuration
  makes resolution, datums, relationship checks, pattern drivers, and graph
  regeneration follow it. A configuration change is rejected when any
  instance would no longer resolve. Documents always store the base graph;
  the active configuration is evaluation state.
- **Materials.** Named materials carry a density. An instance uses its own
  assignment or inherits its clone source's; detaching keeps the inherited
  material. `mass` multiplies a generated output's volume by that density.
- **Assembly requirements.** `MassRange`, `DatumClearance`, and
  `RelationshipSatisfied` rules are evaluated after full graph regeneration.
  Required failures release every result; preferred and advisory failures stay
  visible in `GraphRegeneration::verification`. Rules validate instance,
  output, datum, relationship, material, unit, and range references before
  use. `add_assembly_requirements` validates a batch atomically in O(existing
  requirements + new requirements times referenced clone depth). Partial
  instance regeneration intentionally does not evaluate them
  because it may omit referenced instances. Adding and validating 10,000
  datum-clearance requirements takes 0.013 s; evaluating them is
  O(requirements times clone depth), included in a 0.379 s shared regeneration
  at that scale.

- **Placement solving.** `solve_placements` moves named free instances so
  that every relationship touching them holds, keeping all other instances
  fixed. Each free instance contributes a rotation vector and a translation
  of its local placement, inside any frames it already belongs to; the
  rotation turns the instance about its own pivot, the centroid of its
  involved datums, so rotation and translation stay independent for parts
  far from the model origin. Every involved relationship becomes a smooth
  residual vector, with angular components scaled by a characteristic length
  so millimeters and radians weigh alike, and Levenberg–Marquardt drives it
  to zero. The Jacobian is sparse: each relationship touches at most two
  free instances, so its rows are differentiated by relative-step central
  differences in those twelve unknowns only, and its normal matrix is
  solved by symmetric elimination in minimum-degree order. Elimination
  drops variables whose pivot vanishes, which leaves directions no
  relationship constrains unmoved instead of regularizing them, so weak but
  real modes of long chains are solved exactly; disconnected groups of
  instances never interact, and chains and trees produce almost no fill-in,
  while cycles such as grids cost more. Damping starts near Gauss–Newton
  with a small Levenberg floor, since chains of parts are close to linear but
  poorly conditioned. After convergence, the solution is moved back toward
  the starting placements along the directions no relationship constrains
  (the Jacobian's null space, measured in millimeters) and polished, so
  under-constrained instances move as little as possible; a restoration
  round that would grow the residual is abandoned. Convergence is judged
  relative to the model's length scale. The report gives the free degrees
  and redundant equations from the rank of the same elimination at the best
  fit, and checks every involved
  relationship there. Placements are applied only when every involved
  relationship is satisfied; conflicting sets leave the graph unchanged and
  show which relationships disagree. Solved pattern members keep their
  placements as overrides.

Pattern resizing refuses to delete a member that a relationship,
configuration, or material assignment names, and document validation rejects
references to unknown instances, datums, configurations, or materials.

## Definition, instance, and result

A parametric model has three distinct layers:

1. A **part-family definition** declares named parameters, units, limits,
   relationships, and an ordered feature graph.
2. A **part instance** supplies values for one member of that family.
3. A **generated result** contains OCCT shape handles plus named outputs and
   diagnostics from one regeneration.

A generated BREP is therefore an output, not the source of truth.

For example, `WingRib` should be a reusable definition. Each rib instance can
provide its station, local chord, airfoil, thickness, lightening-hole pattern,
spar intersections, and utility-pass-through locations. Changing the wing
span or spar position should regenerate every affected rib from the same
definition.

## Constraint-solved sketches

Schema 26 extends the schema 25 line sketches with exact circles, arcs, and
tangency. A `SketchDefinition` places parameter-driven 2D points and named
entities in a typed 3D plane. `SketchCircle` names a center and rim point;
`SketchArc` names a center, start, end, and clockwise flag. An implicit equation
enforces equal arc radii. Existing dimensional distance expressions can drive
center-to-boundary radii. Coincident, horizontal, vertical, parallel,
perpendicular, equal-length, distance, and tangent constraints are solved before
kernel generation. `Tangent { first, second, point }` requires a shared named
endpoint (or circle rim); it compares line directions and circular tangents at
that contact, not tangency to extensions of the entities.
`SketchSolution` reports convergence, maximum residual, free degrees, and
redundant equations; conflicting constraints reject generation. A
`SketchFace` feature emits an exact planar face from an ordered, closed
`profile` of entity IDs, or a single circle. Entities omitted from an explicit
profile are construction geometry. For compatibility an empty profile uses
all lines in order, or a sole circle when no lines/arcs exist; other curved
sketches require an explicit profile. Arcs retain their directed minor/major
sweeps. Invalid references, repeated profile IDs, degenerate curves, open
boundaries, and invalid generated faces are rejected with temporary handles
released. Schema 25 documents default the new arrays to empty. Rust callers
now supply `Box<SketchDefinition>` to `SketchFace` to keep the feature enum
compact; this indirection does not alter JSON structure.

Schema 27 adds `SketchWire`, which emits the same exact closed profile as a
reusable wire output, and optional `SketchDefinition::datum_plane` references
to a plane datum in the owning family. The datum supplies the origin and unit
normal. The sketch's explicit x-axis must be perpendicular to that normal;
the y-axis is normal cross x, preserving a deterministic right-handed frame.
The inline origin/y-axis expressions are unused for linked sketches.
Missing or non-plane datums and incompatible axes are rejected. Datum lookup
is indexed during definition validation and generation. Feature signatures
include the referenced datum definition and its expression parameters, so
datum parameter changes and definition edits rebuild sketches and dependent
features while unrelated outputs remain reusable. Older documents default
`datum_plane` to none and retain their inline plane behavior.

Sketch solving reuses the assembly solver's sparse normal-matrix algebra.
Each constraint differentiates only its referenced points (at most eight
coordinates), using central differences and indexed line lookup. Jacobian
assembly takes O(points + constraints) time and memory per iteration;
minimum-degree elimination depends on fill-in, which stays small for chains
and disconnected components. Rank is computed from the undamped normal matrix,
and coordinates absent from constraints remain unchanged. Non-finite residuals
and derivatives are rejected.

One sketch with 10,000 independent lines (20,000 free coordinates) solves in
0.036 s; a connected 1,000-line chain solves in 0.005 s. The separate benchmark
of 10,000 small sketches takes 0.037 s; an additional 10,000-solve
arc/tangent case takes 0.117 s and checks implicit radii and contact coordinates
(both have a 2 s budget). A further benchmark creates 10,000 wire features on
10,000 named datum planes, checks the final placement, and verifies all handles
are released in 0.082 s (5 s budget).

## Requirements preserve intent

A feature graph records **how** geometry was constructed. It does not fully
record **why** that geometry must exist. Each family and assembly therefore
owns a set of stable, machine-readable requirements that survive regeneration
and changes to the construction method.

Every requirement has:

- a stable identifier and version;
- a kind and human-readable statement of intent;
- scope: family, instance, feature, interface, or assembly;
- priority: required, preferred, or advisory;
- typed inputs, units, limits, and tolerances;
- references to related requirements and external standards;
- one or more verification rules;
- trace links to the parameters, features, and named results that satisfy it;
- provenance recording whether it came from a user, imported specification,
  analysis result, or AI proposal;
- status and diagnostics from the most recent regeneration.

Requirement kinds should include at least:

- **dimensional:** length, thickness, clearance, angle, and tolerance;
- **geometric:** parallelism, concentricity, continuity, envelope, and shape;
- **topological:** connected, closed, manifold, through-hole, and face count;
- **functional:** load path, fluid or wire passage, motion, access, and field of
  view;
- **interface:** mating geometry, attachment points, keep-out zones, and datum
  references;
- **manufacturing:** minimum radius, stock size, tool access, draft, and wall
  thickness;
- **assembly:** quantity, placement rule, symmetry, spacing, and allowed
  degrees of freedom;
- **validation:** mass, bounds, volume, interference, and kernel validity.

Requirements form a graph because one intent may refine or depend on another.
For example, a wing may require a continuous wire route; each rib then derives
a local pass-through requirement from that assembly-level requirement.

The model must distinguish a hard failure from an unmet preference. Required
conditions prevent publication of a successful generation. Preferred and
advisory conditions produce visible diagnostics without discarding otherwise
valid geometry.

An AI creating or changing a model should emit requirements before or with the
feature plan. It must not silently replace a requirement merely because a
different shape is easier to construct. When intent is uncertain, it should
record an explicit assumption with provenance so a person can confirm or
replace it later.

### AI authoring contract

For every created or modified family, part, structure, or assembly, an AI must:

1. identify the requested function and interfaces;
2. declare requirements and label uncertain statements as assumptions;
3. attach executable verification rules wherever the kernel can measure the
   condition;
4. create a feature plan and trace every required feature to its requirement;
5. instantiate the model with explicit units and provenance;
6. regenerate and report which requirements passed, failed, or remain
   unverified;
7. preserve existing requirement identifiers when revising implementation.

For example, the intent for a rib utility opening is not merely an anonymous
subtraction:

```yaml
requirements:
  - id: rib.wire_passage
    kind: functional
    priority: required
    statement: Provide a continuous wire passage through each wing rib.
    inputs:
      minimum_diameter: { value: 8, unit: mm }
      spar_clearance: { value: 12, unit: mm }
    verify:
      - through_hole_diameter_gte: minimum_diameter
      - clearance_from: [front_spar, rear_spar]
    satisfied_by: [features.utility_passage]
    provenance: user
```

The feature may later change from a round drilled hole to a reinforced slot
without erasing the functional requirement or its verification history.

## Part instances

A **part instance** is a persistent engineering-model entity, not an OCCT
shape handle. It contains:

- a stable instance identifier and human-readable name;
- the family definition and definition version it uses;
- resolved parameter values and their units;
- instance-scoped requirements and approved overrides;
- placement, coordinate system, datums, and assembly relationships;
- configuration, material, and other engineering metadata;
- named inputs and outputs used by connected parts;
- provenance, revision, and regeneration state;
- a reference to its latest generated result.

Multiple part instances may use the same family while retaining distinct
identity. Two ribs can currently resolve to identical geometry and still be
different parts because they occupy different stations, participate in
different joints, or have different inspection history.

An instance resolves its effective model in this order:

1. family defaults and family requirements;
2. selected configuration or variant;
3. inherited clone values;
4. explicit instance parameter overrides;
5. assembly-derived values and interface constraints;
6. evaluated derived parameters and requirements.

The resolved model drives regeneration. The result is accepted only after its
required verification rules pass. A failed regeneration retains the previous
accepted result but marks it stale; it must never present stale geometry as if
it satisfies the new requirements.

`ManagedPartInstance` implements this accepted-result rule. Each attempt gets a
monotonic revision. A successful attempt replaces and releases the previous
accepted geometry. A failed attempt retains the accepted revision, exposes the
error, and reports `Stale`; an initial failure with no accepted result reports
`Failed`. An accepted generation can be explicitly frozen. While frozen,
regeneration is rejected without consuming a revision or replacing geometry;
unfreezing restores normal regeneration.

## Parametric clones

A **parametric clone** is a linked instance, not a copy of generated geometry.
It records:

- its own stable identity and placement;
- a reference to a family definition or another instance;
- a sparse set of parameter overrides;
- optional suppression or configuration choices;
- dependency links used to propagate regeneration.

Unmodified values are inherited from the clone's source. A change to the
source propagates through the clone graph unless that value is explicitly
overridden. Removing an override restores inheritance. Clone graphs must be
acyclic, and regeneration must report the dependency path when a cycle or an
invalid inherited value is encountered.

Clones support two useful execution modes:

- **Shared-shape instance:** when only placement, metadata, or appearance
  differs, clones may share one generated OCCT shape and apply distinct
  transforms.
- **Regenerated variant:** when dimensional or feature parameters differ, the
  clone is regenerated from the family definition and receives its own shape.

The system may cache equivalent generated variants, but that optimization must
not change clone identity or inheritance behavior.

Patterns are collections of parametric clones driven by a rule. A rib array,
row of pews, bolt circle, or repeated arch should record the pattern rule and
per-member overrides rather than expand permanently into unrelated copies.

A clone may be deliberately **detached** to create an independent instance, or
**frozen** to preserve a specific generated result. Both are explicit modeling
operations; saving or exporting geometry must not silently break the link.

The current `InstanceGraph` implements stable base and clone identities,
source references, sparse overrides, multi-level inheritance, override removal
to restore inheritance, cycle diagnostics, explicit detachment, independent
placement, and linear and circular patterns. Clone inheritance is resolved by
one iterative O(depth) leaf-to-base walk with a visited-position map; the same
collected chain supplies the family and is folded base-to-leaf for ordinary and
active-configuration overrides. This preserves exact cycle-path diagnostics
without consuming call stack. Bulk operations retain resolved parents in a
local cache, stopping later walks at the nearest cached ancestor. The cache is
discarded after document validation, configuration validation, or regeneration,
so graph mutations cannot expose stale resolutions. The 20,000-link benchmark
resolves one leaf on a default-stack thread in 0.010 s and validates all 20,000
instances in 0.035 s. A placement rotates every named result about a
typed origin and scalar axis, then translates it; failed transforms clean up
both generated and intermediate shapes. Patterns remain linked clones and
record their source, member identities, and a typed rule. A linear rule
translates member `i` by `i` times a length step; a circular rule rotates member
`i` by `i` times an angle step about a typed origin and scalar axis. Each
member receives an independent placement that can be edited afterwards. A zero
axis, non-finite angle, or dimensionally invalid step is rejected before any
member is created.

Placement can be assembly-relative. An `AssemblyFrame` is a named coordinate
frame with its own placement and an optional parent frame, so frames nest
(building, room, pew row). A base or clone node may name a frame; its
placement is then local to that frame, and regeneration applies the node
placement followed by each enclosing frame placement, innermost first,
releasing every intermediate shape. Moving a frame moves everything inside it
without editing member placements. A pattern may also name a frame: its rule
and every member placement are expressed in that frame, and members move with
the pattern frame rather than being reassigned individually. Detaching a clone
keeps its frame and placement and removes it from its pattern, since it is no
longer linked to the pattern source; a pattern left without members is
removed. Unknown frames, frame cycles (reported with their path), and
pattern members outside their pattern frame are rejected.

An `InstanceGraph` has a primary family and may register additional family
definitions. A base instance selects a registered family; linked clones and
pattern members inherit the family of their root base, and detachment records
that resolved family explicitly. Overrides, constraints, features, and
requirements are resolved against the selected family. Regeneration groups
equivalent parameter sets only within the same family id and version, so two
families with coincidentally identical parameter maps never share incompatible
feature results. Family ids must be nonempty and unique, and unknown family
references are rejected before a document is accepted.

Pattern members record a rule slot, an optional placement override, and a
suppression flag. A member's placement is its override or the rule placement
for its slot. Slots stay fixed when a neighbor is detached, so removing one
bolt from a circle does not shift the others. Replacing the rule re-places
every member without an override; setting a member's placement records an
override, and clearing it returns the member to the rule. A suppressed member
keeps its identity, inheritance, and overrides but is skipped by graph
regeneration, and requesting it explicitly is an error. Parameter overrides
on members use ordinary sparse clone overrides. Documents are rejected when a
slot repeats or a member's placement disagrees with its override or rule.

A pattern records its slot count and a member prefix. `set_pattern_count`
grows a freely counted pattern by adding linked members named `prefix[slot]`
in the pattern frame, or shrinks it by deleting members in removed slots.
Slots vacated by detaching stay empty. Shrinking is refused when another
instance is cloned from a member it would delete, and growing is refused when
a new member id is already taken; neither changes the graph on failure.

Constraint-driven rules derive the count. `LinearFit` spreads members evenly
from the source placement to a span, with an exact count, the most members
whose gaps are at least a minimum spacing, or the fewest whose gaps are at
most a maximum spacing. `CircularFit` does the same over a sweep in (0, 2π]
by angle; a full turn is closed, so six bolts sit 60° apart rather than
doubling up at 360°. Ratios within floating-point noise of an integer snap to
it, so a 9 m span at 3 m maximum spacing yields exactly three gaps. Fits are
capped at 10,000 members. Replacing a fitted rule re-solves and resizes the
pattern; the count of a fitted pattern cannot be set directly, and documents
whose slot count disagrees with their constraints are rejected.

A pattern may bind a freely counted linear or circular rule to an integer
parameter on any resolvable instance, or fit its count to the measured bounds
extent of a named output and a maximum spacing. A `LinearFit` may bind its span
to a length parameter, or measure an output extent and apply that length along
a typed direction. Measurements use exact bounds without tolerance
enlargement, so an extent that is an exact multiple of the spacing does not
gain a member, and the output must belong to the measured instance's own
family. `refresh_driven_patterns`,
`regenerate_instances`, and `regenerate_all` resolve all drivers before
mutation, then reuse the existing stable-slot resize and placement machinery.
Geometry measurements use a temporary placed generation and release every
shape afterwards. Invalid parameter types, missing instances or outputs, zero
directions, nonpositive spans, incompatible rules, and counts outside
1..=10,000 are rejected. Drivers and their resolved rules survive document
round-tripping; normalized placement comparisons tolerate serialization noise
without accepting material placement changes.

`InstanceGraph::regenerate_instances` and `regenerate_all` implement the
shared-shape execution mode. Requested instances are grouped by their complete
resolved parameter set, with defaults filled in, so an explicit override equal
to the default still shares. The feature graph, constraints, and verification
run once per group; every other member duplicates the group's local result
and is placed independently through its own placement and assembly frames.
Grouping on the full parameter set, not only on referenced feature
parameters, keeps constraints on unused parameters from being skipped.
Parameters with equal values in different units are conservatively treated
as distinct. The result reports the representative each instance was shared
from and how many variants were generated; shared members report every
feature as reused. Clone identity and inheritance are unchanged. A failure in
any group releases every handle created by the call. Rigid placement in the
kernel only attaches a location, so placed members share the group's
geometry and each adds about a handle's worth of memory.

## Reuse levels

The system should support reuse at more than one scale:

- **Features:** hole, pocket, fillet, flange, shell, pattern, and sweep.
- **Part families:** rib, spar segment, stringer, hinge bracket, pew, column,
  arch, or floor tile.
- **Subassemblies:** wing bay, control surface, wall module, or furniture row.
- **Assemblies:** complete wing, building, room, or map.

Composition is preferred over copying geometry. A family may contain other
families and map parent parameters into child parameters.

## Parameter model

Parameters need more meaning than untyped numeric values. Each declaration
should include:

- stable identifier and human-readable name;
- scalar, integer, Boolean, enum, vector, profile, or reference type;
- physical dimension and display unit;
- default value and optional minimum/maximum;
- whether it is an input, derived value, or measured output;
- validation and relationships to other parameters.

Internally, values should use a consistent unit system. User interfaces and
JSON/YAML documents may use other units but must convert them explicitly.

The current implementation supports named derived scalar parameters with
addition, subtraction, multiplication, division, negation, absolute value,
minimum, maximum, and clamp. Addition, subtraction, minimum, maximum, and clamp
require matching dimensions; multiplication requires at least one
dimensionless operand; division accepts a dimensionless divisor or produces a
dimensionless ratio from matching dimensions. Clamp also validates ordered
bounds. Derived dependencies may be declared out of order and cycles report
their dependency path. Hard
less-than-or-equal, greater-than-or-equal, and tolerance-based equality
constraints run after parameter resolution and before geometry creation.
Conditional scalar expressions use those same comparison and tolerance
semantics, require dimensionally matching result branches, and track
dependencies from the comparison and both possible branches.
Derived vector parameters support dimension-checked component construction,
addition, subtraction, scalar scaling, and normalization. Vector dependencies
may also be declared out of order; cycles and zero-vector normalization fail
before geometry creation.

## Regeneration model

Definitions form a directed feature graph. Regeneration should:

1. validate and normalize parameters;
2. resolve requirements, inheritance, assumptions, and constraints;
3. evaluate derived values;
4. rebuild dirty features in dependency order;
5. validate generated topology;
6. evaluate requirement verification rules;
7. publish named shapes, reference geometry, measurements, and trace links;
8. retain actionable diagnostics when a feature or requirement fails.

The current `occt-parametric` implementation performs steps 1 through 7 for
its supported parameter, constraint, feature, and verification set. It resolves
defaults and explicit instance overrides, evaluates derived scalar and vector values and
hard constraints, executes features in dependency order even when declared
out of order, detects cycles and missing inputs, publishes results by feature
name, and rejects required verification failures. Deterministic signatures
cover each feature definition and only the resolved parameters it references.
An unchanged feature is reused through a duplicate session handle; changed
features and every downstream dependency rebuild. Operation history is copied
with reused handles so dirty semantic features can still trace reused source
topology. Verification reruns for every attempt. A failed incremental attempt
releases all temporary outputs and retains the prior accepted generation.
Successful results report rebuilt and reused feature identifiers. Preferred
and advisory failures remain attached to a published result. Resolved instance
placement is applied transactionally to every named output after local
generation. Derived vectors, domain-specific functions, and specialized
mathematical functions are not implemented yet.

Generated OCCT handles are ephemeral. Application code should refer to named
results such as `front_spar`, `rib[7].utility_hole`, or `aileron.hinge_axis`,
not session-local integer handles saved from an earlier regeneration.

## Stable references

Topology can change when parameters change, so raw face and edge indices are
not durable references. The higher-level layer should use semantic names and
selection rules, backed where practical by OCCT history information. Examples
include `largest_planar_face(+Z)`, `edge_created_by(feature_id)`, and
`intersection(front_spar, rib_07)`.

Ambiguous or missing references must fail regeneration with a diagnostic
instead of silently attaching a later feature to the wrong topology.

The current face and edge selectors support a bounded nearest-center rule and
minimum/maximum extrema along a model-coordinate axis. Extrema selectors may
return several matches, such as all four edges around the top of a box. Edge
selectors can choose the longest edge set within a relative tolerance, all
circular edges within a radius range, or any curved edge whose midpoint radius
of curvature is in range. A sampled range selector deterministically evaluates
the complete edge parameter interval at a caller-selected sample count and can
require either full containment or any overlap with a radius range. This is an
explicit sampling contract, not an exact analytic extremum. The bounded
curvature-radius selector instead uses proven extrema. Line and conic edges
are evaluated exactly at their analytic critical parameters. Bezier and
B-spline edges, rational or not, are split into Bernstein pieces; on each
piece squared curvature is a ratio of two Bernstein polynomials of equal
degree, so its coefficient ratios bound it and converge quadratically under
subdivision. A branch-and-bound search refines only pieces that could still
move an extremum until both gaps are within a caller-supplied relative
tolerance. The selector matches only when the bounds prove containment or
overlap; bounds that straddle a range boundary fail regeneration and name the
edge, and tightening the tolerance resolves them. Offset curves and edges
without a 3D curve are rejected rather than sampled. Straight edges are
excluded from curvature-radius matches. Face selectors can choose the largest
area set, optionally restricted to planar faces, match an oriented normal by a
minimum dot product, or select a face adjacent to a required number of semantic
edges. A face selector can also find neighbors with recorded G1-or-better
continuity to a semantic source-face set. Selectors can trace their semantic
source subshapes through an operation using OCCT's generated or modified
history relation. Both edge and face selectors support recursive union,
intersection, and difference using OCCT topological identity rather than handle
numbers. Fillet, chamfer, and
hollow features consume the resolved handles and release all temporary
subshapes, including on ambiguity or kernel failure. These rules do not store
topology indices. An equal nearest match, disallowed size tie, missing history
result, empty rule result, or out-of-range value fails with an actionable
diagnostic. Geometric tangency inference when continuity metadata is absent, and semantic naming
beyond feature outputs remain planned.

## Responsibility boundary

The C ABI exposes general, language-neutral OCCT capabilities:

- primitives, profiles, wires, faces, solids, and transforms;
- booleans, sweeps, lofts, offsets, shells, fillets, and chamfers;
- topology traversal, geometry queries, validation, and measurements;
- BREP persistence, STEP import/export, configurable STL mesh export, and
  operation-history information.

The Rust parametric layer owns or is intended to own:

- parameter schemas and unit-safe values;
- feature graphs and dependency tracking;
- part-family definitions and instances;
- parametric clones, inheritance, patterns, and override tracking;
- requirements, verification, assumptions, and intent traceability;
- constraints, regeneration, semantic naming, and serialization;
- assembly relationships and configuration variants.

This keeps the ABI small and reusable while allowing Rust, JSON/YAML tools,
and user interfaces to work with engineering concepts rather than C++ types.

Application-specific constructors belong in `occt-recipes`. The stone and
torch C entry points remain exported only as compatibility APIs; new Rust
application code should use the recipe crate.

## Compatibility rule

The C interface currently requires an exact ABI version match. ABI version 26
adds ordered mixed line/circular-arc wires (`occt_bridge_create_segment_wire`,
Rust `Session::create_segment_wire`). Arcs pass through three supplied points,
preserving exact circular geometry, including major arcs; requested closure
and segment connectivity are checked. Schema 26 uses this API for mixed
line/arc sketch profiles and adds circle entities and contact tangency.
ABI version 25 added structured kernel-failure diagnostics that identify OCCT's code and the
input at fault; ABI version 24 added a diagnostics-preserving handle release
used by the Rust bindings to free shapes when handles are dropped; ABI version
23 added per-session result validation, optional healing with history, fuzzy
booleans, and per-call warnings; ABI version 22 added exact bounds without
tolerance enlargement, used for measured pattern drivers, to the existing
validated multi-shell solid construction with internal voids, STEP exchange,
configurable ASCII/binary STL tessellation export, sewing, single-shell solid
construction,
curvature-extrema, topological-identity, elliptical-wire, tangency, traversal,
measurement, generic-modeling, and operation-history API while preserving the
rule that OCCT objects never cross the boundary. The ELF library retains symbol version `OCCT_BRIDGE_1.0`; the
explicit runtime ABI number is the authoritative API contract checked during
session creation.

Changing a parameter value creates a new generation of the model; it does not
mutate the meaning of an existing shape handle. Family definitions and their
serialized schemas should carry explicit versions so old designs can be
migrated deliberately.

Clone serialization must preserve the source reference and sparse overrides,
not flatten every inherited value into a disconnected part. Source and clone
identifiers must remain stable across save/load and regeneration.

Part-instance serialization must preserve all supported engineering intent.
The current schema preserves requirements, provenance, accepted-result
revisions, and regeneration status. Assumptions and requirement-to-feature
trace links are not represented by the current runtime model and remain
planned schema additions. Export formats such as BREP or mesh may omit this
information, so they are delivery artifacts and cannot replace the parametric
source model.

`ModelDocument` is the implemented local persistence boundary. Schema version
25 serializes the primary and additional family definitions with their datums,
assembly relationships, configurations, materials and material assignments, requirements, derived parameters,
constraints, base and clone nodes, sparse overrides, placements, linear and
circular pattern rules, linear and circular fit constraints, slot counts,
member slots, placement overrides, suppression, count drivers, and parameter-
or bounds-driven fitted spans,
nested assembly frames, semantic selectors, provenance, and regeneration audit records. Live
OCCT handles and generated BREPs are never serialized. Loading reconstructs a
validated `InstanceGraph`; regeneration creates fresh session-owned handles.
Schema versions 1 through 26 migrate to version 27, supplying explicit defaults
for fields absent from older documents. Version 27 adds sketch-wire feature
outputs and optional family plane-datum references; older sketches retain
their inline planes. Version 26 adds circle/arc sketch
entities, contact tangency, and explicit ordered profile IDs; old sketches
load with empty circles, arcs, and profile arrays. Version 25 added constraint-solved line
sketches and exact closed polygon face features. Version 24 added assembly-level mass,
datum-clearance, and relationship-satisfaction requirements. Older documents
load with none. Version 23 added per-model relationship
tolerances and migrates older documents to 1e-6 mm and 1e-9 rad. Version 22
added family datums and assembly semantics; older documents load with none.
Version 21 added sewing and single- or
multi-shell solid feature operations. Version 20 added additional family
definitions and per-base family references. Version 19 added optional pattern count
and span drivers. Version 18 added slot counts, taken
from the highest member slot, and member prefixes, taken from a first member
named `prefix[n]` or else the pattern id. Version 17 replaced member id strings
with slot records numbered by position; a stored member placement that differs
from its rule slot becomes an explicit override, so geometry is preserved.
Version 16 added the bounded curvature-radius selector, version 15 added assembly frames, and
older documents load with every node and pattern at model level. Version 14 replaced the flat linear
pattern `step` with a tagged `rule`; older documents are rewritten to
`{"linear": {"step": ...}}` before decoding, and a legacy pattern without a
step is rejected. Missing versions, version zero, and
unknown future versions are rejected. Document validation rejects invalid
defaults, units, constraints, placements, clone cycles, missing links,
inconsistent pattern membership, and invalid regeneration revisions before the
model is accepted.

The next cross-layer work should prioritize extrude and revolve features from sketch profiles,
measured by the scale
benchmark suite (`tools/bench/run.sh`). Scale is a requirement for every change; see the
[Roadmap](ROADMAP.md) for target sizes and the checks each change must pass.

The broader serialized source model lives in the sibling
[`engineering-intent-language`](../engineering-intent-language) project. Its
versioned EIL documents are intended to drive this feature compiler and record
verification results against generated geometry.
Its design specifications cover
[shape intent](../engineering-intent-language/SHAPE_INTENT.md),
[linked variables](../engineering-intent-language/VARIABLES_AND_BINDINGS.md),
[mathematical expressions](../engineering-intent-language/EXPRESSION_LANGUAGE.md),
[analysis integration](../engineering-intent-language/ANALYSIS_INTEGRATION.md),
and the
[feature operation registry](../engineering-intent-language/FEATURE_OPERATION_REGISTRY.md).
