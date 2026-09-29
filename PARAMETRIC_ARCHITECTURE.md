# Parametric architecture

The bridge is the geometry-kernel boundary, not the complete engineering
model. Engineering applications built on it should preserve **design intent**
so they can generate and regenerate families of related parts.

## Implementation status

The architecture in this document is both a description of implemented
boundaries and a roadmap. As of ABI version 17, the repository contains three
Rust layers:

1. **`occt-bridge`** safely wraps session-owned OCCT handles. It includes
   generic primitives, reusable wires and faces, transforms, booleans, sweeps,
   lofts, selected-edge treatments, offsets, hollowing, topology traversal,
   oriented face normals, face planarity, edge length, circular radius, and
   midpoint, deterministic sampled, and exact or error-bounded full-edge
   curvature, direct topology
   adjacency, recorded face tangency, physical measurements, BREP persistence,
   generated/modified/deleted operation history, and
   history-preserving duplicate handles for transactional reuse.
2. **`occt-recipes`** owns application-level construction. Its wall-torch
   recipe is composed entirely from generic bridge operations. Faceted stone
   construction is exposed here but temporarily delegates to the compatibility
   ABI until generic sewing and shell-to-solid operations are available.
3. **`occt-parametric`** implements the first executable engineering layer:
   typed scalar, vector, integer, Boolean, and choice parameters; explicit
   length units; versioned families; persistent instance identity; sparse
   instance overrides; dependency-ordered feature execution; named results;
   requirement priorities and provenance; validity and volume verification;
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
   operation-history tracking;
   versioned JSON persistence with
   schema migration and validation; incremental dirty-feature rebuilding; and
   cleanup when regeneration or placement fails.

The following major capabilities remain planned:

- patterns driven by parameters or geometry, and kernel-level (location-only)
  shape sharing;
- additional domain-specific expression functions;
- additional schema migrations and integration with the broader EIL source
  model;
- broader requirement rules such as clearance, interference, minimum radius,
  wall thickness, connectivity, and manufacturing checks;
- assembly relationships, configurations, materials, and named datums.

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
placement, and linear and circular patterns. A placement rotates every named result about a
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
pattern members outside their pattern frame are rejected. All nodes in one
graph currently share one family definition.

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
any group releases every handle created by the call. The C transform still
copies geometry, so placed members own separate OCCT shapes; location-only
sharing inside the kernel remains a possible later optimization.

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
- BREP import/export and operation-history information.

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

The C interface currently requires an exact ABI version match. ABI version 17
adds exact or error-bounded edge curvature extrema to the existing
topological-identity, elliptical-wire, curvature, tangency, traversal, measurement, generic-modeling, and
operation-history API while preserving the rule that OCCT objects never cross
the boundary. The ELF library retains symbol version `OCCT_BRIDGE_1.0`; the
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
18 serializes the complete family definition, requirements, derived parameters,
constraints, base and clone nodes, sparse overrides, placements, linear and
circular pattern rules, linear and circular fit constraints, slot counts,
member slots, placement overrides, and suppression,
nested assembly frames, semantic selectors, provenance, and regeneration audit records. Live
OCCT handles and generated BREPs are never serialized. Loading reconstructs a
validated `InstanceGraph`; regeneration creates fresh session-owned handles.
Schema versions 1 through 17 migrate to version 18, supplying explicit defaults
for fields absent from older documents. Version 18 added slot counts, taken
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

The next kernel work should prioritize generic sewing, shell-to-solid
construction, broader exchange formats, and the remaining operations needed by
feature definitions. The next parametric work should prioritize additional
schema migrations, patterns driven by family parameters or assembly
geometry, and multi-family graphs.

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
