# Parametric architecture

The bridge is the geometry-kernel boundary, not the complete engineering
model. Engineering applications built on it should preserve **design intent**
so they can generate and regenerate families of related parts.

## Implementation status

The architecture in this document is both a description of implemented
boundaries and a roadmap. As of ABI version 37, the repository contains three
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
   requirement priorities and provenance; validity, volume, connectivity,
   minimum radius, sampled wall thickness, draft, and overhang, mass, datum
   clearance, relationship-satisfaction, no-interference, and minimum-clearance
   verification with measured values, evidence quality, and witnesses;
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
- broader hole-size catalogs (countersink relief, tolerance classes) and
  sheet metal beyond single constant-width strips;
- advanced ribs;
- closed-linkage solving and rotating continuous collision detection beyond
  the available translation-only interval checker;

Continuous translation paths can be checked between motion samples using exact
BREP distances and conservative relative-displacement bounds. Query limits and
uncertain intervals report unresolved results rather than a clear path. Joint
angles must remain constant. See [Assembly motion](ASSEMBLY_MOTION.md) for
interpolation semantics, numeric guards, budgets, and limits.

Tagged surface meshes, external tetrahedral hand-off, material-aware glTF, and
sampled manufacturing checks are available in schema 44 with ABI 35. Mesh
exports resolve semantic selectors against current geometry and release kernel
handles after producing numeric data. See [Mesh hand-off](MESH_HANDOFF.md) for
units, topology matching, scale budgets, and sampling limits.

Identity-based semantic document comparisons are available through
`ModelDocument::semantic_diff`. They match declaration collections by stable IDs
and emit typed field/entity paths with exact serialized before/after values,
while preserving ordered profile and expression arrays. Comparisons do not
regenerate geometry or allocate kernel handles; the 10,000-instance repeated
comparison benchmark runs within a five-second budget. See
[Semantic document comparisons](DOCUMENT_DIFF.md) for the API, complexity,
change-record format, and remaining data-management work.

`base.three_way_merge(&left, &right)` combines independent edits using the same
canonical document representation. It returns a validated document or typed
conflicts containing base/left/right values; incompatible edits never return
a partial document. Inputs and combined results undergo document validation so
cross-branch reference and parameter failures are rejected before regeneration.
The 10,000-instance repeated merge benchmark includes input/output validation
and runs within an eight-second budget, without allocating kernel handles.

## Assembly semantics

Assembly semantics live in the `assembly` module and serialize with the
document.

Schema 41 adds fixed, revolute, prismatic, cylindrical, and planar joints driven
through named frames with checked unit-aware coordinates and optional limits.
Exact BREP interference and clearance checks use an indexed broad phase.
Sampled motion reuses each local parameter variant across independent poses,
reporting collisions and datum relationships while preserving the source graph.
The scale suite checks 10,000 joints, 10,000 sparse collision participants, and
1,000 motion samples. See [Assembly motion](ASSEMBLY_MOTION.md) for frame
conventions, resource bounds, and limits of sampled kinematics.

Bounded joint-coordinate solving adjusts selected freedoms to satisfy all
recorded relationships, keeping driver coordinates and rest placements fixed.
Closed motion studies continue from the last successful pose and close every
sample before generating shared geometry and checking sampled collisions.
A failed pose or exhausted iteration budget returns closure reports with no
partial motion study; the source graph and accepted geometry remain unchanged.

Continuous joint checks retain unwrapped angular travel, including full and
reverse multiple turns. Swept bounds and conservative point-speed bounds
through nested frame paths reject clear intervals using exact BREP separation;
uncertain intervals or exhausted budgets remain unresolved. These interpolated
coordinate paths do not automatically maintain linkage closure between solved
sample poses. Local branch selection and continuously constrained path
certification remain separate limitations.

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
- **Assembly requirements.** `MassRange`, `DatumClearance`,
  `RelationshipSatisfied`, `NoInterference`, and `MinimumClearance` rules are
  evaluated after full graph regeneration. The collision rules (schema 46) take
  explicit or all-instances output sets and reuse the exact indexed checks,
  including a cross-set query that never inspects pairs within one set; see
  [Requirement rules](REQUIREMENTS.md).
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

## Extrude and revolve features

Schema 28 adds `Extrude { input, direction }` and
`Revolve { input, origin, axis, angle_radians }`. `input` is a named planar
face or closed planar wire output, including `SketchFace` and `SketchWire`.
These references participate in feature dependency ordering, allowing a sweep
to be declared before its profile. Extrusion's `direction` is the complete
length-valued displacement vector, allowing oblique and negative sweeps.
Revolution's origin is length-valued, its axis is dimensionless and nonzero,
and its signed scalar angle is in radians, with `0 < abs(angle) <= 2*pi`.
The axis/origin use family-local coordinates; existing instance placement
and assembly frames position the resulting solids afterward.

Wire inputs create temporary planar faces, released on success and failure.
Profiles must define valid planar faces; sweep results must be valid solids
with positive volume. Exact lines/arcs/circles retain their geometry. Kernel
history maps profile edges to generated faces, including when the temporary
face has been released. Parameter edits to displacement or angle rebuild
the solid while reusing an unchanged profile; profile and datum edits rebuild
dependent sweeps. Failed incremental attempts release new handles and preserve
the prior accepted output. Tests check exact polygon/arc/circle prism volumes,
full and signed partial torus volumes, dependency ordering, history, units,
invalid inputs, serialization/migration, selective reuse, and cleanup.
The scale suite builds and edits 1,000 sweeps in 0.562 s for extrude and
0.700 s for revolve (5 s budgets), verifies every edited volume and profile
reuse, and checks that all handles are released.

## Hole features

Schema 29 adds `Hole { input, position, axis, diameter, extent }`, using the
existing cylinder and boolean APIs without an ABI change. `input` is a named
output containing exactly one valid solid, including a kernel result wrapper.
Position and diameter use lengths; the dimensionless nonzero axis is normalized.
Coordinates are family-local, with instance placement applied afterward.

`HoleExtent::Blind { depth }` uses a positive length-valued depth from the
supplied position along the axis, producing a flat-bottom cylindrical cut.
The caller supplies the starting position; the feature does not infer a surface
or guarantee that a floor remains when depth exceeds the body.
`HoleExtent::ThroughAll` projects the input's exact axis-aligned bounds onto
the axis line and extends the cutter beyond both ends with scale-aware padding
(at least 1e-6 mm). It therefore covers both axis directions even if the
supplied position is far outside the body. It does not need a guessed depth.

Cuts must remove material and leave exactly one valid solid with positive
volume; misses, complete removal, and split-solid results fail with feature
context. The input remains unchanged. The kernel result wrapper retains cut
history, while temporary cylindrical tools are released on success and failure.
Diameter, position, axis, and blind-depth expressions participate in incremental
signatures: unchanged inputs are reused, affected downstream features rebuild,
and failed edits preserve the previous accepted generation. Tests cover exact
volumes, distant and rotated axes, history, units, invalid inputs, cleanup,
selective reuse, and schema round trips/migration. Building and editing 100
sequential holes takes 5.490 s (10 s budget), checking volume and handle cleanup.
Schema 30 extends the hole with `finish`, defaulting to `HoleFinish::Plain`
when older documents omit it. `Counterbore { diameter, depth }` adds a
cylindrical entry recess; `Countersink { diameter, angle_radians }` adds a
conical entry recess with included scalar angle strictly between zero and pi
radians. All recess diameters and counterbore depths are length-valued.
The entry diameter must exceed the bore diameter. Countersink depth is
`(entry_radius - bore_radius) / tan(angle_radians / 2)`; either recess depth
must be finite, positive, and less than the bore depth for blind holes.
Recesses start at the supplied hole position along the normalized axis, not
at an inferred body surface. Through-all still spans both axis directions for
the cylindrical bore; the recess remains anchored at the entry position.
The bore and recess cutters are fused before one cut, preserving history from
the original input and avoiding overlapping-solid compounds. Temporary tools
are released. Recess diameter, depth, and angle expressions participate in
incremental regeneration. Tests verify exact counterbore/frustum volumes,
history, parameter edits, invalid dimensions/units/angles, rollback, cleanup,
and schema 29 migration. The scale suite builds and edits 100 counterbores in
10.195 s and 100 countersinks in 8.447 s (15 s budgets), checking exact final
volumes, unchanged input reuse, and cleanup.
Schema 31 adds optional `thread` metadata, represented by
`Option<Box<ThreadSpecification>>` in Rust. It stores a nonblank caller-supplied
designation, length-valued nominal diameter and pitch, and right/left
handedness. Nominal diameter must exceed the explicit cylindrical bore diameter;
pitch must be positive. This is recorded internal-thread intent, not modeled
helical geometry. No standard, designation/dimension agreement, tap drill,
tolerance class, engagement length, or machinability is inferred or verified.
All metadata participates in the feature signature, and nominal diameter/pitch
expressions participate in parameter dependency tracking. Metadata edits rebuild
the hole and downstream branch, retaining unchanged cylindrical geometry and
preserving the previous accepted generation on failure. Older holes default to
no thread record. Tests cover serialization/migration, both handedness values,
unchanged volume, invalid metadata/units, selective reuse, rollback, and cleanup.
The scale suite builds and edits thread records on 100 holes in 5.418 s
(10 s budget), checking unchanged volume, input reuse, branch rebuilds, and
handle cleanup.
Schema 32 adds `ScalarExpr::Iso273ClearanceV1 { nominal_diameter, series }`
and the public `iso273_clearance_v1` helper. The nominal diameter is a length;
the frozen lookup returns millimeter clearance diameters in fine, medium, or
coarse series for 19 supported metric sizes. Derived and resolved expression
evaluation use the same lookup, and dependency collection traverses the nominal
expression. Unsupported sizes fail; only 1e-9 mm unit-conversion roundoff is
allowed, with no interpolation or nearest-size selection. Catalog V1 is frozen
so future table corrections/expansions require another explicitly versioned
catalog, preserving saved-model regeneration. Four tests check every catalog
value, unit conversion, invalid quantities, derived evaluation, geometry,
incremental edits/rollback, round trips, and migration. See
[Hole-size catalog](HOLE_SIZE_CATALOG.md) for source, API, and limitations.
100,000 checked lookups take about 1 ms (200 ms budget).
Schema 45 adds `ScalarExpr::CarrLaneTapDrillV1 { nominal_diameter, pitch, system }`
and `ScalarExpr::CarrLaneSocketHeadV1 { nominal_diameter, system, dimension }`,
frozen metric and inch tables from Carr Lane's Rev. 9/2021 booklet: 64 tap-drill
pairs and 38 socket-head sizes with counterbore diameter/depth and normal/close
clearance. They share the ISO 273 contract (exact keys, no interpolation,
derived and resolved evaluation, dependency collection through every operand).
Four tests check published values in both unit systems, rejection of unknown
pairs and wrong dimensions, and a hole whose tap drill, clearance, and
counterbore are all catalog-driven, including incremental edits and rollback.
Manufacturing tolerance selection remains planned. The catalogs do not verify
fit or standards compliance.

## Sheet metal

Schema 45 adds `FeatureOperation::SheetMetal`, a constant-width strip of
flanges joined by exact circular bends, and `FeatureOperation::SheetMetalFlat`,
its blank for an explicit neutral factor K. The blank is computed from the
folded feature's definition rather than by unfolding geometry, so it is exact
and needs no topology matching; `flat_pattern` gives the same lengths, bend
allowances, and bend lines without kernel work, and they export as a drawing.
Evaluation is linear in flanges, and overflowing dimensions fail before kernel
calls. Five tests cover analytic volumes for bends of each sign, incremental
reuse when only K changes, distant and very small or large sheets,
self-intersecting outlines, and drawing export. See [Sheet metal](SHEET_METAL.md).

## Draft features

Schema 33 adds `Draft { input, faces, neutral_origin, neutral_normal,
pull_direction, angle_radians }`. Face selectors and their history references
participate in dependency ordering. The neutral origin uses lengths; the normal
and pull direction are finite, nonzero scalar vectors; the angle uses signed
scalar radians, with `0 < abs(angle) < pi/2`. All coordinates are family-local.
The neutral plane fixes its intersection with the tapered face. Positive angles
remove material on the pull side; negative angles add it.

ABI 28 exposes `occt_bridge_draft`; the safe Rust wrapper accepts `DraftOptions`.
Selections must be descendant faces and must not contain duplicates, including
overlaps between selector results. Planar, cylindrical, and conical faces are
supported; OCCT may propagate to tangential neighbors. Topology-changing drafts
are outside the supported scope. These restrictions follow the
[OCCT draft API](https://occt3d.com/dev/doc/refman/html/class_b_rep_offset_a_p_i___draft_angle.html).
The parametric operation additionally requires one valid input solid and one
valid result solid with positive volume. Inputs are unchanged. Temporary
selection handles drop on success and failure.

Kernel failures record `Draft_ErrorStatus` and the problematic subshape, with
the selected face index when the failure occurs while adding a face. Failure
during final construction may not identify a selection index. Result validation
and optional healing retain operation history. Where OCCT's list-based history
omits a changed source, its corrected `ModifiedShape` counterpart is recorded.
Angle, neutral-plane, pull-direction, and selector expressions all participate
in incremental signatures; failed edits preserve the prior accepted generation.
Tests check exact signed wedge and cylinder-to-frustum volumes, modified history,
input preservation, units, duplicates, missing/unsupported faces, directions,
kernel diagnostics, incremental reuse/rollback, cleanup, and schema migration.
The scale suite builds and edits 1,000 drafts in 2.922 s (10 s budget), checking
each signed volume, valid results, unchanged input reuse, and handle cleanup.

## Variable-radius fillet features

Schema 35 adds `VariableFillet { input, edges, start_radius, end_radius }`.
Its semantic edge selectors identify open tangent contours. Both radii are
finite positive lengths and vary linearly between OCCT's first and last
spine vertices, using ABI 29's `occt_bridge_variable_fillet` and safe
`Session::variable_fillet`. This follows the
[OCCT linear radius law API](https://www.occt3d.com/dev/doc/refman/html/class_b_rep_fillet_a_p_i___make_fillet.html).
Contour direction belongs to OCCT and does not
follow the caller's selected edge orientation. Tangent neighbors can extend
the contour; selecting several of its edges applies one shared law. Duplicate
edges and closed contours are rejected. Multi-station laws, explicit direction
control, and distinct radius pairs for different contours remain future work.

The input and result must contain one valid solid, and the result must have
positive volume. Input geometry is preserved. Validation and optional healing,
generated/modified/deleted history, bounded failure-isolation diagnostics, and
selector attribution use the same path as constant fillets. Endpoint-radius
and selector parameters participate in feature signatures; selector history
references contribute graph dependencies. Failed edits release temporary
handles and retain accepted generations. Edge membership is indexed once per
kernel call in O(input topology + selected edges) time and memory; kernel
construction cost depends on the contour geometry. Fillet and chamfer builder
calls serialize across sessions because concurrent variable blends produced
reproducible OCCT 7.9 walking failures. Tests cover independent simultaneous
sessions as well as geometric invariants, edits, migration, and cleanup. The
scale suite builds and edits 1,000 features and verifies volume bounds,
validity, body reuse, and released handles.

## Rib features

Schema 34 adds `Rib { input, profile, thickness, direction }`. This first
implementation is a bounded, closed-profile reinforcement, not an open-sketch
rib that extends automatically to support faces. `profile` is a named valid
planar face or closed wire, including sketch outputs. `input` must contain one
valid solid. Both references participate in dependency ordering, even when the
rib is declared before its body or sketch.

Thickness is a positive finite length. Direction is a nonzero dimensionless
vector, normalized internally, and must be parallel or antiparallel to the
profile's unit normal (`abs(dot) >= 1 - 1e-9`). Schema 36 adds
`Rib.thickness_mode: RibThicknessMode`, with `OneSided` as the migration default
for older ribs. In `OneSided` mode the profile is extruded by total thickness
along the direction; reversing direction chooses the other side. In `Centered`
mode half the total thickness lies on each side of the profile plane;
reversing direction gives the same geometry. There is no oblique thickness
sweep. Schema 38 adds explicit bounded
open-sketch closure and schema 39 adds uniform first-contact closure, as
described below.
All coordinates are family-local.

The temporary prism is fused into the input using the extrusion and fuse operations.
Centering shifts the extruded wall by negative half thickness along the
normalized direction. This adds one temporary location-only handle and O(1)
placement data, with no graph scans. ABI 30 adds explicit history composition:
`Session::compose_history` returns a separate handle sharing the final geometry
while tracing an intermediate operation's inputs through its descendants.
Direct history is retained and neither input's history changes. Generated
ancestry remains generated after modification; modified ancestry followed by
generation becomes generated. Removed targets are excluded and removed sources
can still generate topology. Shape-identity indexes make composition expected
O(topology + history records + expanded target relations) in time and memory.
Located histories expand only on this explicit composition path; ordinary
placed copies still retain O(1) history storage.
The result must contain one valid solid and add material beyond a small
floating-point volume margin (`64 * f64::EPSILON * max(abs(input volume), wall volume)`).
This rejects fully contained translated walls whose fuse changes only the
last few bits of the measured volume, without imposing a fixed minimum rib
volume across differently sized models. Face-connected and overlapping walls
are accepted; disconnected,
edge/vertex-only, fully contained, and invalid walls are rejected. The input
remains unchanged. Temporary faces and prisms drop on success and failure.
Ribs now compose the original profile-edge history through the implicit prism,
optional centered placement, and final fuse. The final result retains body and
profile history after temporary handles are released. Schema 37 adds
`FaceSelector::GeneratedFromEdges { source_feature, source }`, where `source`
is an edge selector on an earlier feature. It selects only generated faces;
every selected source edge must yield a surviving face. This supports downstream
draft and hollow features without naming the rib's temporary prism. Source
feature dependencies and selector expressions participate in ordering and
incremental signatures. Existing documents keep their geometry and defaults.

Thickness, direction, and thickness mode participate in feature signatures;
body/profile edits invalidate dependent ribs and downstream features. Failed
edits release new handles and preserve the previous accepted generation. Twelve
tests cover exact triangle/overlap volumes, sketch faces/wires, both directions,
body history, input preservation, units, centered placement, mode edits,
small and 1 km-offset models, connection failures, selective reuse,
rollback, cleanup, schema migration, composed history, generated-face selection,
and downstream draft edits. Schema 38 adds the bounded open-sketch mode below;
uniform extend-to-next is described below.
The scale suite builds and edits 1,000 ribs in 6.017 s (15 s budget), checking
every volume, result validity, unchanged body/profile reuse, and handle cleanup.
An additional 1,000-centered-rib case takes 6.924 s with the same 15 s budget and checks center
of mass for both thickness values as well as volume, reuse, validity, and cleanup.
Both cases also verify the exact area of the face generated from a profile edge
on all 1,000 outputs before and after thickness edits.

### Bounded open-sketch ribs

`SketchOpenWire` uses the same constraints, line/arc geometry, datum planes,
and unit rules as closed sketch outputs, but requires one ordered open chain
with distinct endpoints. Circles, closed chains, and disconnected paths fail.
`Rib.profile_mode` defaults to `RibProfileMode::Closed` for earlier documents.
`OpenStrip { offset }` closes the open profile with a translated reversed copy
and straight endpoint bridges. The offset is length-valued, finite, nonzero,
and must produce a simple planar boundary. The thickness direction must be
normal to that boundary, as for closed-profile ribs. The offset defines the
bounded region explicitly; there is no support-face discovery or extend-to-next
behavior. It is a translated closure, rather than a constant-distance offset
of curved segments.

ABI 31 exposes `create_open_profile_face`. Original edges remain boundary
edges, translated edges are generated from them, and composed extrusion/fuse
history preserves their surviving generated faces. Construction and history
indexing take O(edges) time and storage. OCCT's geometric self-intersection
check is O(edges²) in the worst case; this topology check is necessary to reject
crossing boundaries and does not scan graph instances or supporting faces.
Temporary handles are released on all paths. Offset expressions participate
in signatures; unchanged profiles and bodies are reused when only the closure
offset changes, and failed edits retain the accepted generation.

Six new tests cover line/arc profiles, exact volume and generated-face area,
one-sided/centered thickness, reversed directions, small and kilometer-sized
models, 1 km offsets, invalid closures/chains/units, rollback, and schema defaults.
The 1,000-open-rib build/edit case takes 8.413 s against a 15 s budget, including
volume, centroid, generated-face, reuse, validity, and cleanup checks.

### Uniform extend-to-next ribs

Schema 39 adds `RibProfileMode::OpenToNext { direction, maximum_length }`;
ABI 32 exposes `create_open_profile_face_to_next`. Direction is dimensionless
and normalized independently of the normal thickness direction. Maximum length
is a positive finite length bounding the search. The profile must be a valid
straight open chain perpendicular to advance; collinear multi-edge chains are
accepted. Earlier `Closed` and `OpenStrip` modes retain their geometry and
serialization. Documents in schemas 1–38 migrate without changing their modes.

The kernel builds a planar strip out to the maximum reach and intersects it
with the body's volume and boundary. Exact geometry bounds in a frame at the
source endpoint find the minimum positive advance distance. Boundary sections
include tangencies and contacts exactly at the maximum reach, which a
volume-only common operation can omit. No mesh or sampled rays are used.
A cut of the translated wire against the body checks that the entire chain
meets the first contact. Partial nearer obstacles fail rather than being
skipped for a farther support. Profiles on or inside the body, no hit within
reach, nonuniform profiles, and nonplanar or self-intersecting closures fail.
Search happens in the profile plane; the existing thickness placement and
single-solid/material-addition guards validate the final fused wall. General
curve-dependent closure and automatic support-following remain future work.

Support discovery uses a fixed number of kernel boolean/section operations,
with cost determined by body and profile topology. Search operations use
non-destructive mode so repeated searches retain stable input geometry and cost.
There is no iterative
length search or unbounded extrusion. Exact bounds and history indexing are
linear in resulting topology; boolean/section costs remain kernel-dependent.
Original and translated edge ancestry composes through extrusion, centered
placement, and fuse just as for explicit strip closure. Reach/direction
parameters participate in incremental signatures, and body changes invalidate
support discovery. Failed edits release all temporary handles and preserve
accepted generations. Tests cover exact geometry/history, nearer and partial
supports, both thickness placements/signs, units, reach/body edits, rollback,
migration, small/large/distant models, and handle cleanup.
The 1,000-feature build/edit scale case takes 13.665 s with a 30 s budget,
checking every exact volume, centroid, generated face, input reuse, and released
handle. This is more expensive than explicit strip closure because it performs
body intersection, boundary section, and full-chain coverage checks.

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

The C interface currently requires an exact ABI version match. ABI version 37
adds spline-section lofts and adaptive volume integration for freeform faces.
ABI version 36
adds signed per-face radius bounds (exact on analytic surfaces, sampled
elsewhere) and per-edge concavity. ABI version 35
adds bounded surface tessellation and batch topology indices. ABI 34 adds exact
hidden-line projection, plane clipping, edge sampling, and bulk traversal.
ABI 33 adds exact distance, overlap, central mass properties, and multi-station
variable fillets. ABI 30 composes operation history; ABI 31 and 32 add open-profile
closure and uniform first-contact closure, respectively. ABI 29 adds linear
variable fillets. ABI version 28
adds selected-face draft with structured draft diagnostics, validation, and
corrected modified topology history. ABI version 27
adds face revolution (`occt_bridge_create_revolve_from_face`, Rust
`Session::create_revolve_from_face`), signed partial/full angles up to one turn,
topology history, and session result validation. Schema 28 uses this API and
the existing prism API for revolve and extrude features. ABI version 26
added ordered mixed line/circular-arc wires (`occt_bridge_create_segment_wire`,
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
34 serializes the primary and additional family definitions with their datums,
assembly relationships, configurations, materials and material assignments, requirements, derived parameters,
constraints, base and clone nodes, sparse overrides, placements, linear and
circular pattern rules, linear and circular fit constraints, slot counts,
member slots, placement overrides, suppression, count drivers, and parameter-
or bounds-driven fitted spans,
nested assembly frames, semantic selectors, provenance, and regeneration audit records. Live
OCCT handles and generated BREPs are never serialized. Loading reconstructs a
validated `InstanceGraph`; regeneration creates fresh session-owned handles.
Schema versions 1 through 35 migrate to version 36, supplying explicit defaults
for fields absent from older documents. Version 38 adds open sketch wires and
explicit translated rib-profile closure; earlier ribs default to closed profiles.
Version 37 adds generated-face selectors
from earlier feature edges; existing features and selectors remain unchanged.
Version 36 adds centered rib thickness,
defaulting earlier ribs to one-sided geometry. Version 35 adds linear variable-radius
fillets; earlier constant-radius operations remain unchanged. Version 34 adds
bounded closed-profile
ribs; existing features remain unchanged. Version 33 adds selected-face draft;
older documents retain their existing features. Version 32 adds the frozen metric
clearance expression; existing expressions remain unchanged.
Version 31 adds optional internal-thread
records; older holes default to none. Version 30 adds hole entry recesses;
holes without a finish load as plain, preserving their geometry.
Version 29 adds cylindrical hole
operations with blind and through-all extents; existing features remain
unchanged. Version 28 adds extrude and revolve
feature operations referencing face/wire outputs; older documents keep their
existing features unchanged. Version 27 adds sketch-wire feature
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

The next cross-layer work should prioritize advanced ribs and richer variable fillet laws,
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

## Generated drawings

Schema 42 stores drawing definitions separately from model geometry. Views select
explicit instance outputs and use exact OCCT hidden-line removal, optionally
after solid section clipping or with a detail crop. Datum dimensions and
parameter notes resolve from the current document. Shared variants regenerate
once, and temporary handles are released before numeric drawing data is returned.
SVG and DXF include a page frame and metadata title block. Bounded uniform curve
sampling is an approximation; section hatching is not yet generated. See
[Drawings](DRAWINGS.md) for coordinates, budgets, examples, and export contracts.

## Model revisions and change impact

Schema 43 records an explicit linear revision ledger, separate from generation
audits. Semantic change payloads exclude that ledger. Kernel-free impact reports
resolve inherited/configured parameters and propagate feature signatures through
the union of old/new dependencies, caching shared variants and clone ancestry.
Placement and material changes have separate flags from local feature rebuilds.
A Git merge driver validates merged intent before atomically replacing its
current file. See [Model history](MODEL_HISTORY.md) and
[Document comparisons](DOCUMENT_DIFF.md) for persistence and conflict contracts.
