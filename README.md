# opencascade-c-bridge

A small, stable C ABI over Open Cascade (OCCT), designed to be wrapped safely
from Rust and other languages. Open Cascade C++ objects never cross the ABI.

The current C ABI version is **52**.

## Current API

- Opaque, independently owned sessions
- Integer shape handles scoped to a session
- History-preserving duplicate handles for transactional geometry reuse
- Boxes, cylinders, cones, spheres, and arbitrary planar polygon prisms
- Reusable polyline, mixed line/circular-arc, circular, and elliptical wires,
  planar faces, explicit translated closure of open planar profiles, and face
  extrusion and signed partial/full revolution
- Non-mutating translation, axis-angle rotation, and uniform scaling; rigid
  moves share geometry through locations, so placed copies stay small
- Compatibility constructors for existing natural-stone and wall-torch callers
- Circular tubes swept along arbitrary 3D polylines for rails, scrollwork, and ornament
- Multi-section solid/shell lofts and assembly compounds
- Face and shell sewing with operation history, single-shell solid
  construction, and multi-shell solids with internal voids
- Fuse, cut, and common boolean operations
- Selected-edge constant, linear, and multi-station variable-radius fillets and chamfers,
  joined offsets, and face-selected hollowing
- Shape-kind and unique-subshape traversal
- OCCT topological-identity comparison for independently owned handles
- Oriented face-normal, face-planarity, edge-length, circular-edge-radius,
  midpoint edge-curvature, deterministic sampled full-edge curvature ranges,
  exact (line, conic) or error-bounded (Bezier, B-spline) curvature extrema,
  and direct topology-adjacency queries
- Same-domain face and edge merging after booleans, with operation history
- G1-or-better tangency queries between adjacent faces, from recorded
  continuity or measured within an angular tolerance where none is recorded
- Generated, modified, and deleted operation-history queries, with explicit
  composition through intermediate operations on a new shared-geometry handle
- Tolerance-padded and exact bounds, surface area, volume, center-of-mass, and
  BREP validity inspection
- BREP persistence, STEP import/export, and configurable ASCII/binary STL export
- Per-session result validation (on by default) for booleans, fillets,
  chamfers, offsets, hollowing, sewing, and STEP and BREP import, with
  optional shape healing that carries operation history, fuzzy booleans,
  and per-call warnings. Faces with many holes use scalable checks that
  preserve `BRepCheck_Analyzer` verdicts and diagnostic statuses.
- Structured failure diagnostics: failed fillets, chamfers, offsets,
  hollows, and booleans, and results rejected by validation, report OCCT's
  own codes and names, the selected edge, face, or operand at fault, and a
  handle to the offending subshape
- Caller-owned diagnostic buffers
- Exception containment at every C entry point
- Dependency-free safe Rust wrapper

Operations that create geometry return new handles; they do not mutate their
inputs. Destroying a session releases all shapes belonging to it. In Rust,
dropping a shape handle releases it (and its operation history) through
`occt_bridge_shape_release`, which leaves the session's last error and
warnings untouched; `Session::remove` releases immediately and reports
errors.

The C ABI is a direct-modeling API. The Rust engineering layer uses these
operations to regenerate reusable part families from typed, unit-aware
parameters and dependency-ordered feature graphs. Part instances carry
machine-readable requirements and verification results, preserving why
geometry exists instead of only how it was constructed. Linked clone graphs,
sparse inherited overrides, explicit detachment, accepted-result revisions,
stale-result retention, independent axis-angle placement, linear clone
patterns, iterative stack-safe clone inheritance, parameter- or
geometry-driven counts and fitted spans,
multi-family instance graphs, explicit generation freezing, unit-aware derived scalar arithmetic
with negate, absolute, minimum, maximum, clamp, square root, power,
trigonometric, hypotenuse, interpolation, step-rounding, vector length, and
dot-product functions, derived vector
composition with add, subtract, scale, and normalize operations, dimension-safe
comparison-driven conditional scalar expressions,
pre-generation parameter constraints, constraint-solved line/arc/circle/spline sketches that
emit exact closed wires and planar faces on inline or named datum planes,
semantic face and edge selectors with persistent references that follow
topology through later features and named references declared once per
family, and
versioned JSON model documents are implemented. Feature graphs include sewing
and single- or multi-shell solid construction. Schema 69 adds saved cone/frustum
and sphere features, including apex and cylindrical limits, typed expressions
and incremental regeneration. Selectors support orientation,
adjacency, extrema, nearest-center, longest-edge, circular-radius,
curvature-radius, sampled full-edge curvature-radius range, proven-bound
curvature-radius range,
largest-planar-face, tangent-neighbor, set composition, operation-history,
and faces generated from earlier feature edges
rules. Families declare named datums; graphs record checked datum
relationships that can be solved to place free instances under finite positive
per-model linear and angular tolerances, plus configurations and materials
with mass. Full graph regeneration verifies mass ranges, datum clearances,
recorded relationship satisfaction, exact no-interference, and minimum
clearance between instance outputs, and part regeneration verifies validity,
volume, solid connectivity, minimum convex or concave radius, and sampled wall
thickness, draft, and overhang, all with required, preferred, or advisory
priority. Results carry measured values, evidence quality, and collision
witnesses; see [Requirement rules](docs/REQUIREMENTS.md). Schema v1 through
v68 documents migrate to v69 during load; unsupported
future versions are rejected.
Managed regeneration incrementally reuses unchanged outputs and
rebuilds dirty features plus their downstream dependents. Graph regeneration
runs the feature graph once per distinct resolved parameter set and places
clones that differ only in placement or assembly frame independently.
See [Roadmap](ROADMAP.md) for current status and what comes next, and
[Parametric architecture](PARAMETRIC_ARCHITECTURE.md) for the
definition/instance/clone/result model and the boundary between that layer and
the C ABI.

The sibling [`engineering-intent-language`](../engineering-intent-language)
project provides the broader serialized source model, including semantic
validation and clone inheritance resolution. The local `occt-parametric` crate
is the kernel-facing execution layer for an initial subset of that model.

## AI part authoring

[The `occt-model` command](tools/model/README.md) builds a saved model from a
single JSON request, applies typed parameter edits, verifies requirements, and
writes an editable model, structured measurements/diagnostics, STEP/STL, and
isometric SVG previews. Complete bracket, enclosure, shaft, and mating-part
examples demonstrate the authoring and repair loop. Earlier build directories
and the source document remain intact after rejected changes.
[The local MCP server](tools/model/MCP.md) exposes generated schemas for all
feature operations, complete examples, build tools and readable artifact
resources to AI clients. Read-only inspection exposes resolved parameters,
feature dependencies and semantic face/edge queries before edits. Guarded
feature edits branch from accepted snapshots, retain requirements, and publish
verified revision records without overwriting previous builds. The
[annotated viewer](tools/model/VIEWER.md) shows dimensions and constraint symbols
on sketches and 3D shapes, links controls to geometry, and displays diagnostic
failures while keeping model acceptance separate.

## Repository layout

- [`include/occt_bridge.h`](include/occt_bridge.h): stable C ABI contract.
- [`docs/`](docs/README.md): one page per modeling capability (API, schema,
  limits, and examples); [`ROADMAP.md`](ROADMAP.md) and
  [`PARAMETRIC_ARCHITECTURE.md`](PARAMETRIC_ARCHITECTURE.md) cover the whole project.
- [`src/`](src): the C ABI implementation, one file per area:
  - [`session.cpp`](src/session.cpp): sessions, last error, warnings,
    diagnostics, options, and handle removal.
  - [`primitives.cpp`](src/primitives.cpp): boxes, cylinders, cones, and spheres.
  - [`wires.cpp`](src/wires.cpp): polyline, segment, curve, circle, and
    ellipse wires.
  - [`construction.cpp`](src/construction.cpp): faces, prisms, revolutions,
    tubes, lofts, and compounds.
  - [`open_profile.cpp`](src/open_profile.cpp): translated open-chain closure.
  - [`rib_support.cpp`](src/rib_support.cpp): bounded uniform first-contact closure.
  - [`recipes.cpp`](src/recipes.cpp): the faceted stone and wall torch.
  - [`solids.cpp`](src/solids.cpp): sewing and solid construction.
  - [`operations.cpp`](src/operations.cpp): booleans, same-domain
    unification, and transforms, with failure diagnostics.
  - [`edge_treatments.cpp`](src/edge_treatments.cpp): fillets, variable
    fillets, and chamfers, with per-contour failure isolation.
  - [`draft.cpp`](src/draft.cpp) and [`offsets.cpp`](src/offsets.cpp): draft
    angles, skin offsets, and hollowing.
  - [`inspection.cpp`](src/inspection.cpp): topology, adjacency, identity,
    operation history, and validity.
  - [`geometry_queries.cpp`](src/geometry_queries.cpp): bounds, areas,
    volumes, centroids, face normals, edge lengths and radii, and tangency.
  - [`mesh.cpp`](src/mesh.cpp): source-preserving surface tessellation and
    indexed topology matching for tagged mesh exports.
  - [`curvature.cpp`](src/curvature.cpp): curvature sampling and exact or
    error-bounded extrema.
  - [`exchange.cpp`](src/exchange.cpp): BREP, STEP, and STL.
  - [`core.cpp`](src/core.cpp) and
    [`bridge_internal.hpp`](src/bridge_internal.hpp): helpers shared by the
    entry points (failure reporting, result storage with history, validation,
    healing, and diagnostics).
  - [`shape_validator.cpp`](src/shape_validator.cpp): BRepCheck validation
    that scales to faces with many holes.
- [`rust/occt-bridge`](rust/occt-bridge): safe Rust wrapper with session-owned,
  generation-checked shape handles.
- [`rust/occt-recipes`](rust/occt-recipes): application-level geometry recipes
  composed over the generic bridge.
- [`rust/occt-parametric`](rust/occt-parametric): typed parameters, families,
  instances, linked clones and patterns, placement, managed/frozen revisions,
  incremental feature rebuilding, derived expressions and constraints, feature
  graphs, named results, requirement verification and semantic face/edge
  selection, plus versioned JSON persistence and migration.
- [`tests/c_api_test.c`](tests/c_api_test.c): end-to-end C ABI conformance test.
- [`tests/fixtures/curvature_edges.brep`](tests/fixtures/curvature_edges.brep): Bezier,
  rational and multi-span B-spline, conic, and straight edges with known
  curvature, regenerated by `generate_curvature_fixtures.cpp` in the same folder.

## Build and test

```bash
cmake -S . -B build -DCMAKE_BUILD_TYPE=RelWithDebInfo
cmake --build build
ctest --test-dir build --output-on-failure

LD_LIBRARY_PATH="$PWD/build" \
  cargo test --manifest-path rust/occt-bridge/Cargo.toml

LD_LIBRARY_PATH="$PWD/build" \
  cargo test --manifest-path rust/occt-recipes/Cargo.toml

LD_LIBRARY_PATH="$PWD/build" \
  cargo test --manifest-path rust/occt-parametric/Cargo.toml
```

The project currently requires OCCT 7.9 or newer. Fedora's Open Cascade CMake
package is discovered automatically.

C and C++ static analysis runs clang-tidy (configured by
[`.clang-tidy`](.clang-tidy)) and cppcheck inside a podman container, so no
host packages are needed. It exits non-zero on any finding:

```bash
tools/cpp-lint/run.sh
```

Coverage uses LLVM source-based instrumentation for both languages in the
same kind of container. The C API test and every Rust suite run against the
instrumented library, and the merged result is printed as a summary and
written to `build/coverage/html/index.html` and `build/coverage/lcov.info`:

```bash
tools/coverage/run.sh
```

Rust unit tests live in `tests` modules inside each crate's `src/` (for example
`src/tests/`, `src/tests.rs`, and `src/assembly/tests.rs`); like the C tests,
they are excluded, so the percentages measure only code under test. Rust
branch coverage needs a nightly toolchain and is not reported.

The scale benchmark suite enforces the roadmap's scaling requirement. It
builds an optimized copy of the library in `build/bench`, runs every case at
the target sizes (10,000-member patterns, deep clone chains, 1,000-part solver
stacks and grids, repeated regeneration, validation chains, and many-hole
faces), and fails when a required case misses its time budget or correctness
check. The current suite has 68 passing Rust cases, including single-leaf and
memoized all-node resolution of a 20,000-link clone chain and a 50-part stack
1 km from the origin solved at a 1e-8 mm model tolerance, plus 10,000 checked
datum-clearance requirements, 10,000 small constrained-sketch solves,
10,000 arc/tangent sketch solves, 10,000 datum-linked sketch-wire features,
build/edit cases for 1,000 extrude, 1,000 revolve, and 100 each of plain hole,
counterbore, countersink, and thread-recorded hole features,
100,000 checked metric clearance-catalog lookups, 100,000 Carr Lane tap and
socket-head lookups, 1,000 folded sheet-metal brackets with linked blanks, 1,000 draft, 1,000 one-sided
rib, 1,000 centered rib, and 1,000 variable-radius fillet features, and
individual sparse sketches with 10,000 independent or 1,000 connected lines:

```bash
tools/bench/run.sh
```

SonarQube analysis converts the merged LCOV report to Sonar's generic coverage
format, runs the containerized scanner, waits for the quality gate, and fails
on a failed gate or any open issue:

```bash
SONAR_TOKEN=... tools/sonar/run.sh
```

Set `SONAR_HOST_URL` when the server is not at `http://127.0.0.1:9000`. A token
may instead be read from `SONAR_TOKEN_FILE`; local automation can provide
`SONAR_ADMIN_AUTH=user:password` to create and revoke a temporary analysis
token. Pass `--no-coverage` to reuse an existing `build/coverage/lcov.info`.
By default, the token file is
`${XDG_CONFIG_HOME:-$HOME/.config}/opencascade-c-bridge/sonar-token`.
The script checks server readiness and authentication before generating
coverage. For the default local URL, it starts an existing `sonarqube-local`
container if needed and waits for startup; set `SONAR_CONTAINER` to use another
existing local container. Scanner scratch files stay inside the temporary
scanner container.
Rust unit-test modules are indexed as tests rather than production sources.
SonarQube Community Build indexes the Rust sources but not C/C++; the generic
report still contains both languages, so editions with the CFamily analyzer
can import the C/C++ records as well.

Application-specific construction belongs in the dependency-free
[`occt-recipes`](rust/occt-recipes) crate. Its wall-torch recipe is composed
entirely from generic bridge primitives, and its faceted-stone recipe sews
planar facets and closes them into a solid. The corresponding C entry points
remain exported only for compatibility.

The [`occt-parametric`](rust/occt-parametric) crate provides the first local
engineering-model layer: typed unit-aware parameters, versioned families,
instances with overrides and provenance, dependency-ordered feature graphs,
named results, linked clone inheritance, accepted and stale regeneration
states, axis-angle placement, nested assembly frames, linear and circular
patterns placed relative to those frames with editable rules and counts,
span- and sweep-fitting constraints, per-member placement overrides, and
member suppression, frozen accepted generations, and
required/preferred/advisory verification. Derived scalar parameters support
dimension-checked arithmetic and dependency-cycle diagnostics; hard parameter
constraints reject invalid instances before geometry is created.
Fillet, chamfer, and hollow features select edges or faces by bounded
nearest-center, coordinate-extremum, oriented-normal, edge-adjacency,
longest-edge, circular-radius, midpoint curvature-radius, sampled full-edge
curvature-radius range, proven-bound curvature-radius range, largest-planar-area, and tangent-neighbor rules,
including multi-edge matches, and can follow selections through generated or
modified operation history. Edge and face rules compose recursively through
topological union, intersection, and difference. Missing, out-of-range, or
disallowed tied matches fail regeneration with diagnostics instead of silently
selecting a topological index.
Managed instances compare deterministic feature signatures, reuse unchanged
outputs through history-preserving duplicate handles, and rebuild every dirty
feature and downstream dependency. Each accepted result reports which named
outputs were rebuilt or reused. Failed incremental attempts release all new
handles and retain the prior accepted generation.

`ModelDocument` persists the family definition, requirements, instance and
clone identities, sparse overrides, placements, assembly frames, pattern rules, provenance, and
regeneration audit records. Schema 43 adds explicit revision history and the
API reports resolved instance/feature change impact. The `occt-document-merge`
binary supplies a semantic Git merge driver. See [Model history](docs/MODEL_HISTORY.md)
and [Document comparisons](docs/DOCUMENT_DIFF.md). Schema 42 adds regenerated orthographic, section,
and detail drawings, datum dimensions, parameter notes, and metadata title blocks
exported as SVG or DXF. Optional exact geometry export preserves standard curves
and detail trims in DXF, with exact SVG curves where representable and
error-controlled SVG approximation otherwise. See [Drawings](docs/DRAWINGS.md) for
the API and export limits. Schema 69 adds per-view material hatch families,
including inherited assignments, paired lines, crosshatching and suppression.
Schema 71 adds drawing surface texture requirements (ASME B46.1 roughness
limits, sampling length, waviness, lay, material removal and method) drawn with
the Y14.36 symbol, and checks measured roughness readings in inspection
reports; see [Drawings](docs/DRAWINGS.md#surface-texture-schema-71). Schema 68 evaluates measured points against drawing GD&T: fitted datum
simulators, flatness and orientation zones, and position with MMC/LMC bonus from
stated size limits (`DrawingSizeLimits`, also used for bonus arithmetic and
supplied-axis position checks). Dimensional measurements are checked against
saved tolerances, and the `occt-inspection-report` command writes a JSON report
covering dimensions, supplied-axis positions and measured points; see
[Drawings](docs/DRAWINGS.md). Schema 63 adds two-row composite controls, reusable named datum-reference frames
and nominal planar 3-2-1 coordinates. Schema 62 adds datum-feature symbols and structured GD&T feature-control frames
with ordered datum references and material modifiers. Schema 61 adds ANSI/ISO paper presets, structured title blocks, sheet numbering
and first-/third-angle projection symbols. Schema 60 adds configurable automatic section hatching, preserving holes and
disconnected material regions in SVG/DXF. Schema 59 adds datum-linked center marks, centerlines and section cutting-plane
indicators in saved drawings and SVG/DXF exports. Schema 58 adds manufacturing dimension types, unit-aware tolerances, basic/reference
notation and live hole callouts; see [Drawings](docs/DRAWINGS.md). Schema 57 combines the newer geometry and reference features with planar slice
drawings and assembly motion workflows.

Continuous joint path checks handle translations, unwrapped rotations, and
nested frames, catching collisions between samples and reporting unresolved
intervals explicitly. See [Assembly motion](docs/ASSEMBLY_MOTION.md)
for the supported joint paths and numeric limits. Closed-linkage solving adjusts
selected joint coordinates while keeping driven coordinates fixed and enforcing
travel limits; unsuccessful solves preserve the accepted pose. Closed motion
studies continue those solves across driven samples before generating shared
geometry and checking sampled collisions.

`InstanceGraph::export_draw_view` writes a model's exact B-rep parts and a script that
opens them, named and colored, in OCCT's DRAW viewer (`DRAWEXE -i -f view.tcl`).
The `occt-view MODEL.json` command does this for a saved model document and starts
the viewer, and `--watch` reloads it on every save; see
[Model viewer](tools/view/README.md). `occt-view MODEL.json --serve` shows it in a
browser page that also edits and saves family parameters and per-instance
overrides. `set_material_appearance`
sets the colors it shows.
ABI 44 writes assembly frames as nested STEP sub-assemblies (joint motion
included) with per-face colors, adds a lenient batch subshape lookup, and
fixes component names on shared parts placed at the origin. Schema 64 adds
family feature colors, carried to faces through later features.
ABI 43 adds exact per-face pull ranges, so `DraftAngle` requirements are exact
on analytic faces. ABI 42 merges the same-domain faces and edges booleans leave split, with
history, and schema 56 adds the `Unify` feature. ABI 41 measures face tangency where booleans record no continuity, and schema 55
lets `TangentTo` face selectors use it. ABI 40 adds structured STEP assembly export (named components, shared parts,
and colors) used by `InstanceGraph::export_step`. ABI 39 adds profile sweeps along paths, and schema 52 the `Sweep` feature.
ABI 38 adds wires mixing lines, arcs, and interpolated splines with end
tangents, and schema 51 adds spline sketch entities. ABI 37 adds spline-section lofts and adaptive volume integration for freeform
faces, and schema 49 adds parameter-placed `Loft` features. ABI 36 adds signed per-face radius bounds and per-edge concavity for
minimum-radius requirements. ABI 35 adds bounded surface tessellation and
indexed topology matching. Schema 44
adds tagged FEA hand-off, material-aware glTF scenes, and wall, draft,
and printing-overhang checks. See [Mesh hand-off](docs/MESH_HANDOFF.md) for APIs,
units, external tetrahedral meshing, and screening limits. Schema 45 adds
folded sheet-metal strips with exact circular bends and linked flat patterns
using an explicit neutral factor, with flat-pattern drawings for SVG and DXF;
see [Sheet metal](docs/SHEET_METAL.md). It also adds frozen Carr Lane V1 tap-drill
and socket-head counterbore catalogs in metric and inch sizes; see
[Hole-size catalog](docs/HOLE_SIZE_CATALOG.md).

ABI 34 adds exact hidden-line projection, plane clipping, edge sampling, and
bulk subshape traversal. Schema 41 adds driven frame joints with limits and
the API supports exact interference/clearance checks and sampled motion with
shared local geometry. See [Assembly motion](docs/ASSEMBLY_MOTION.md) for coordinate
conventions, complexity, and sampled-motion limits. ABI 33 exposes BREP
distance/witness points, non-destructive overlap volume, and adaptive solid
center/inertia measurements. Schema 40 adds interior variable-fillet radius
stations and explicit spine control; laws use smooth interpolation and can
overshoot their samples. Earlier documents default to the existing linear law.

The [mass and balance command](tools/balance-report/README.md) reports selected
component and per-material mass, central inertia, and CG relative to an explicit
chord or a station-derived wing MAC. It retains current assembly poses and
uses supplied material densities, with no ABI or model schema change.

Schema 38 adds `SketchOpenWire` and
`Rib.profile_mode: RibProfileMode`, defaulting earlier ribs to `Closed`.
`OpenStrip { offset }` closes an open line/arc chain with a translated reversed
copy and straight endpoint bridges. The length-valued offset must define a
simple planar region; the rib's thickness direction must be normal to that
region. ABI 31 exposes `Session::create_open_profile_face` for this construction.
Schema 39 adds `OpenToNext { direction, maximum_length }` and ABI 32 adds
`Session::create_open_profile_face_to_next`. A straight open chain advances
perpendicularly along a dimensionless direction, stopping at its first body
contact within a positive length-valued reach. The whole translated chain must
meet that first contact; partial supports, profiles already touching/inside the
body, and unsupported or nonuniform profiles fail. Contact is found from exact
kernel intersections, including tangencies and the exact reach boundary.
Support search uses the profile plane before applying normal thickness.
More general support-following ribs remain planned.
Schema 36 adds `Rib.thickness_mode`, using
`RibThicknessMode::{OneSided, Centered}`. Existing documents default to
`OneSided`; centered ribs place half the total positive thickness on each
side of the profile plane. Reversing the required nonzero normal direction
leaves a centered rib's geometry unchanged. ABI 30 composes the original
profile-edge history through extrusion, centered placement, and fusion.
Schema 37 adds `FaceSelector::GeneratedFromEdges { source_feature, source }`
to select surviving faces generated from an earlier feature's edges for
downstream operations. Both inputs and their original histories remain intact;
body history, validation, incremental reuse, and failed-edit rollback are
retained. Fully contained walls are rejected using a small operand-relative
volume margin so floating-point roundoff is not accepted as added material.
Schema 35 adds
`VariableFillet { input, edges, start_radius, end_radius }`, using ABI 29's
`occt_bridge_variable_fillet` and safe `Session::variable_fillet`. Semantic
selectors choose open tangent contours; positive length-valued radii vary
linearly from OCCT's first spine vertex to its last. The spine direction is
defined by OCCT rather than the selected edge's orientation. One law applies
to the whole contour, including tangent neighbors, even if several of its
edges are selected. Duplicate edges and closed contours are rejected.
The parametric input and result must each contain one valid solid with
positive result volume. Inputs remain unchanged; validation, healing,
operation history, failure diagnostics, incremental reuse, and failed-edit
rollback follow the existing fillet path. Fillet and chamfer builds are
serialized across sessions to prevent observed concurrent OCCT 7.9 variable
blend failures. Arbitrary multi-station laws, per-contour radius pairs in
one feature, and explicit spine-direction control remain future work.
Earlier documents retain their existing constant-radius fillets.
Schema 34 adds
`Rib { input, profile, thickness, direction }`: a bounded reinforcement made
by extruding a closed planar face/wire normally and fusing it into one body.
Thickness is a positive length; direction is a nonzero scalar vector,
normalized and required to be normal to the profile plane. Its sign chooses
the side of the profile. The result must add material and contain one valid
solid; disconnected, edge-only, and fully contained walls fail. Body/profile
references and parameter edits participate in incremental regeneration.
Body and composed profile history are retained and temporary faces/walls are
released. Schema 38 adds the bounded open-sketch mode described above;
uniform extend-to-next landed in schema 39; general support-following ribs
remain planned. The original schema
34 feature did not change ABI 28; older features remain unchanged.
Schema 33 adds `Draft` operations with semantic
face selectors, a length-valued neutral-plane origin, dimensionless nonzero
neutral normal and pull direction, and signed scalar radians with
`0 < abs(angle) < pi/2`. Faces must be planar, cylindrical, or conical;
OCCT may propagate the taper to tangential neighbors. Positive angles remove
material on the pull side of the neutral plane; negative angles add it.
The input remains unchanged, modified topology history is retained, and failed
edits preserve the previous generation. Draft failures expose OCCT status
and problematic subshapes. This uses ABI 28's `occt_bridge_draft` and safe
`Session::draft` with `DraftOptions`. Topology-changing drafts and unsupported
surfaces are outside the supported scope. Older documents keep their features.
Schema 32 adds `ScalarExpr::Iso273ClearanceV1`,
a frozen metric clearance-hole diameter lookup with a length-valued nominal
fastener diameter and `ClearanceSeries::{Fine, Medium, Coarse}`. It works
directly in a hole's diameter or in derived parameters, preserves dependency
tracking, and rejects unsupported sizes rather than rounding or interpolating.
The public `iso273_clearance_v1` helper returns a millimeter `Quantity`.
See [Hole-size catalog](docs/HOLE_SIZE_CATALOG.md) for source data, supported sizes,
an example, and limitations. Older documents retain their existing expressions.
Schema 31 adds optional `Hole.thread` metadata
(`Option<Box<ThreadSpecification>>` in Rust). A thread record stores a nonblank
caller-supplied designation, length-valued nominal diameter and positive pitch,
and right/left handedness. Nominal diameter must exceed the explicit bore
diameter. This records internal-thread intent only: no helix is generated,
no tap drill is inferred, and no standard, tolerance class, engagement length,
or machinability is verified. Thread parameter and record edits invalidate the
hole and downstream features, even though the cylindrical geometry is unchanged.
Older documents default to no thread record; that schema addition did not change the ABI.
Schema 30 adds the optional `Hole.finish`, defaulting
to `HoleFinish::Plain` when absent in older documents.
`HoleFinish::Counterbore { diameter, depth }` makes a cylindrical entry recess;
`HoleFinish::Countersink { diameter, angle_radians }` makes a conical recess
using its included angle (scalar radians, strictly between zero and pi).
Both start at the supplied hole position along the axis, require a diameter
larger than the bore, and must be shallower than a blind bore. Countersink depth
is `(entry_radius - bore_radius) / tan(angle_radians / 2)`.
The caller supplies the entry position; no surface is inferred.
Schema 29 adds `Hole` operations referencing a
named one-solid input. Position and diameter are length-valued; the nonzero
axis is dimensionless and normalized. `HoleExtent::Blind { depth }` cuts a
flat-bottom bore for a positive length-valued depth from the supplied position
along the axis (it does not guarantee a remaining floor).
`HoleExtent::ThroughAll` spans the input bounds in both directions along the
axis line, without a guessed cutting depth. Cuts must remove material and leave
one valid solid with positive volume. Parameter edits rebuild dependent
features, operation history is retained, and temporary tools are released.
Older documents migrate without changing their existing features; that schema
addition did not change the ABI. Schema 45 adds tap-drill, inch, and
socket-head recess catalogs.
Schema 28 adds `Extrude` and `Revolve` operations
from named planar face or closed-wire outputs. Extrusion uses a length-valued
displacement vector (including oblique and negative directions); revolution
uses a length-valued origin, a dimensionless nonzero axis, and a signed scalar
angle in radians with `0 < abs(angle) <= 2*pi`. Both require valid profiles and
produce valid solids with positive volume. Wire inputs use a temporary face,
released after sweeping. Parameter edits reuse unchanged profiles and rebuild
the swept solids; generated topology history remains available to selectors.
Schema 27 adds `SketchWire` feature outputs and
optional sketch `datum_plane` references to family plane datums. A linked sketch
uses the datum's origin and normal with its own perpendicular x-axis; the
y-axis is normal cross x. Datum expressions and edits drive incremental
regeneration. Older sketches keep their inline planes.
Schema 26 adds exact circle/arc sketch entities,
implicit equal arc radii, tangent constraints at shared contact points, and
explicit ordered profiles with construction geometry. Schema 25 line sketches
load with empty curve/profile arrays. Sketches emit exact closed wires or planar faces.
Schema 24 persists assembly mass, datum-clearance,
and relationship-satisfaction requirements. Schema 23 persists per-model linear
(millimeter) and angular (radian) relationship tolerances; new graphs and older
documents default to 1e-6 mm and 1e-9 rad. Generated OCCT handles and BREPs are deliberately
excluded: loading a document reconstructs a validated `InstanceGraph`, which
then regenerates fresh session-owned geometry.

## Parametric example

```rust
use occt_parametric::{
    Dimension, FamilyDefinition, FeatureDefinition, FeatureOperation,
    LengthUnit, ParameterDefinition, ParameterType, ParameterValue,
    PartInstance, VectorExpr, VectorQuantity,
};
use occt_bridge::Session;
use std::collections::HashMap;

let family = FamilyDefinition {
    id: "Block".into(),
    version: 1,
    parameters: vec![ParameterDefinition {
        id: "size".into(),
        parameter_type: ParameterType::Vector(Dimension::Length),
        default: ParameterValue::Vector(VectorQuantity::lengths(
            10.0, 20.0, 30.0, LengthUnit::Millimeter,
        )),
        minimum: None,
        maximum: None,
    }],
    derived_parameters: vec![],
    derived_vector_parameters: vec![],
    constraints: vec![],
    features: vec![FeatureDefinition {
        id: "body".into(),
        operation: FeatureOperation::Box {
            origin: VectorExpr::Literal(VectorQuantity::lengths(
                0.0, 0.0, 0.0, LengthUnit::Millimeter,
            )),
            size: VectorExpr::Parameter("size".into()),
        },
    }],
    requirements: vec![],
};
let instance = PartInstance {
    id: "block-01".into(),
    definition: &family,
    overrides: HashMap::new(),
    provenance: "user".into(),
};
let session = Session::new()?;
let result = instance.regenerate(&session)?;
let body = result.shape("body").expect("named output");
assert!(session.is_valid(body)?);
```

## C example

```c
occt_bridge_session_t *session = NULL;
occt_bridge_session_create(OCCT_BRIDGE_ABI_VERSION, &session);

occt_bridge_shape_id_t box;
occt_bridge_create_box(
    session,
    (occt_bridge_vec3_t){0, 0, 0},
    (occt_bridge_vec3_t){100, 100, 10},
    &box
);

occt_bridge_brep_save(session, box, "floor.brep");
occt_bridge_step_save(session, box, "floor.step");
occt_bridge_session_destroy(session);
```

## Rust example

```rust
use occt_bridge::{Session, StlOptions, Vec3};

let session = Session::new()?;
let floor = session.create_box(
    Vec3::new(0.0, 0.0, 0.0),
    Vec3::new(400.0, 400.0, 10.0),
)?;
session.save_brep(&floor, "floor.brep")?;
session.save_step(&floor, "floor.step")?;
session.save_stl(&floor, "floor.stl", StlOptions::default())?;
```

## Scope

This repository is organized around the Open Cascade boundary and its first
dependency-free Rust layers:

- the C++ implementation that owns and operates on OCCT objects;
- the stable C ABI used by other languages;
- the dependency-free safe Rust wrapper for that ABI;
- application recipes composed from generic bridge operations;
- the initial unit-aware parametric family and verification model;
- focused C and Rust tests of each layer.

It is not a scene language, asset library, renderer, VTT integration, finite
element package, or image-reconstruction application. Those systems should
depend on this bridge rather than being implemented inside it.

## Possible higher-level uses

A higher-level application can use the wrapper to build parametric engineering
assemblies, architectural models, procedural environments, game maps, or assets
reconstructed from reference images. It can also add JSON/YAML schemas, tool
calls, materials, scene graphs, instancing, mesh generation, simulation inputs,
and renderer-specific exports without exposing OCCT C++ types across the ABI.

Experimental examples of those possibilities live in the sibling
[`occt-scene-recipes`](../occt-scene-recipes) project. They are intentionally
not part of this wrapper's API or compatibility contract.

General SVG/DXF drawing batches and 1:1 planar cutting templates are available
through [the drawing-export command](tools/drawing-export/README.md).

Alternative closed-linkage poses can be discovered and exported as reloadable
models with [the joint-branches command](tools/joint-branches/README.md).

The [editable model studio](tools/view/README.md) now includes selectable sketch
and solid annotations, linked dimension editing, regenerated constraint results
and rejected-edit diagnostic previews in the same workspace.

[Sketch capabilities](docs/SKETCHES.md) now include ellipses, angle/radius/diameter,
symmetry and point-on-curve constraints, with saved native trim/extend/offset
profiles (ABI 47, model schema 72).


[Extrusion end conditions](docs/EXTRUSIONS.md) support symmetric total length,
up to a selected face, and up to the next complete face limit, including
inclined and curved surfaces. Target
geometry participates in regeneration; the viewer shows the actual length.
The native ABI is 52; model schema is 91. Native surface-ray witnesses measure
viewer extents on nonuniform caps.


[Blind drill-point holes](docs/HOLES.md) add configurable conical bottoms, native tip
containment checks, full-diameter depth callouts and linked viewer dimensions.
The current model schema is 91 and native ABI is 52.


[Geometry-driven holes](docs/HOLES.md) stop at a selected or next covering input
face, including inclined and curved exits. Native witnesses provide measured
viewer depths; input and selector edits regenerate the cut. Schema 76 adds the
end conditions without changing ABI 48.


[Saved-profile lofts](docs/LOFTS.md) build solid transitions directly from sketch
faces and closed wires, including native circles, ellipses, arcs and splines.
Section edits regenerate the part; viewer spacing links to section controls.
This adds `profile_loft` in schema 77 and native wire lofts in ABI 49.

[Symmetric sketch revolutions](docs/REVOLUTIONS.md) split the total signed angle
equally about the source sketch plane, preserving native source-edge history
and linked viewer angle arcs. Schema 78 introduced the optional extent mode without a native API change.

[Hollow sketch profiles](docs/PLANAR_REGIONS.md) combine an outer profile and up to
100 separate inner profiles into one native planar face for extrusion or
revolution. Boundary checks reject crossing, touching and misplaced holes.
Schema 79 adds `planar_region`; ABI 50 also retains its extracted face history.

[Hollow path sweeps](docs/HOLLOW_SWEEPS.md) transport holed planar sections into
bent tubes and multiple-bore parts, preserving wall ancestry and linked route
controls. ABI 50 adds explicit history-preserving topology extraction; the
current model schema is 91. Rebuild the native library and Rust executables together.

[Hollow profile lofts](docs/LOFTS.md#explicit-hollow-tracks-schema-80) use explicit
inner section tracks to make tapered ducts and multiple-bore transitions.
Schema 80 adds optional `profile_loft.holes`, with native containment, crossing
checks and retained wall ancestry. Native ABI remains 50.

[Plane mirrors](docs/MIRRORS.md) create handed parts with native reflected geometry,
retained source-face ancestry and editable plane controls. Schema 81 adds
`mirror`; ABI 51 adds the native reflection operation.

[Helices](docs/HELIX.md) are exact helical wires to sweep along for springs and
coils, with parameter-driven radius, pitch, turns and handedness. ABI 52 adds
`occt_bridge_create_helix_wire`; schema 82 adds the `helix` feature.

[Uniform scaling](docs/SCALING.md) resizes feature geometry about a parameter-driven
centre with a positive dimensionless factor. Schema 83 adds `scale`, linked
viewer controls and the AI example `scaled-part`. Native ABI remains 52; rebuild
the native library for the correction to factors very close to one.


The [spring AI example](tools/model/spring.request.json) combines helices with a
parameter-driven circular sweep. Coil radius, pitch and fractional turn edits
update the section orientation, native solid, linked viewer dimensions and
route overlays. See [helix viewer semantics](docs/HELIX.md#ai-example-and-linked-viewer-dimensions).

Drawings record [releases and revision tables](docs/DRAWINGS.md#releases-and-revision-tables-schema-90):
dated revisions with approvals, links to the model's revision ledger, a status
that flags model changes since the latest release, and a drawn revision table.
Schema 90 adds them.

Drawings can carry [parts lists and item balloons](docs/DRAWINGS.md#parts-lists-and-balloons-schema-87):
shown instances group into numbered items by family, parameter values, output
and material, with optional part numbers. Schema 87 adds them; schema 91 adds
nested lists that follow sub-assembly frames with hierarchical item numbers.

[Modeled threads](docs/THREADS.md) cut ISO 68-1 basic-profile 60° threads into rods
and holes, external or internal, either hand, with parameter-driven diameter,
pitch and length. Schema 84 adds the `thread` feature; the native ABI is
unchanged.


[Signed skin offsets](docs/OFFSETS.md) expand or contract a part using native joined
surface offsets, with linked distance/tolerance controls and retained source
history. Schema 85 adds `offset`; the native ABI remains 52. The AI example
`offset-part` demonstrates both signed distances.


[Compound tool groups](docs/COMPOUNDS.md) collect shapes without fusion or sewing.
Schema 86 adds `compound`, allowing one Boolean cut to use many cutters. The
AI example `multi-hole-plate` groups nine cylinders and cuts a parameter-driven
plate, with editable spacing, hole radius and thickness.


[Linear part patterns](docs/PART_PATTERNS.md) repeat a source shape with editable
count and step. Schema 88 adds `linear_pattern`; the native ABI remains 52.
The AI example `patterned-plate` creates its cutter grid with two chained
patterns, exposing row and column counts alongside hole radius and spacing.


[Circular part patterns](docs/PART_PATTERNS.md#circular-patterns-schema-89) create
bolt circles and signed partial radial groups with editable count and angular
step. Schema 89 adds `circular_pattern`; native ABI remains 52. The AI example
`bolt-circle` derives spacing from count and sweep, then cuts the tool group.


Pattern and compound groups now retain child ancestry. A source cutter face can
select every repeated bore after a cut, including nested patterns. Union and
intersection preserve directly grouped ancestry too. Schema 89 / ABI 52 remain
unchanged; rebuild native and Rust binaries together. See
[retained pattern ancestry](docs/PART_PATTERNS.md#retained-pattern-ancestry).


The [threaded-rod AI example](tools/model/threaded-rod.request.json) exposes
modeled threads with linked major diameter, pitch, run and derived turn labels.
Its source rod follows diameter/run edits. See
[thread dimensions](docs/THREADS.md#ai-example-and-viewer-dimensions). Current AI
examples use schema 91; native ABI remains 52.


The [edge-treatment AI example](tools/model/edge-treatments.request.json)
compares rounded and chamfered blocks with linked radius/distance controls and
selected source-edge overlays. Live edits update geometry and annotations.
See [edge treatments](docs/EDGE_TREATMENTS.md); schema 91 / ABI 52 are unchanged.


The [variable-fillet AI example](tools/model/variable-fillet.request.json) adds
linked endpoint radii, interior station controls, and the complete directed
contour law. Labels show nominal law values; source-edge references are
separate from spatial station locations. See
[variable-radius viewer semantics](docs/EDGE_TREATMENTS.md#variable-radius-contour-laws).


Drawing dimension labels now use shared SVG/DXF text extents for upright,
centred placement and clearance from dimension lines, including stacked
tolerances and basic frames. See
[drawing label layout](docs/DRAWINGS.md#upright-dimension-label-layout) and the
[reproducible sheets](tools/drawing-export/README.md#dimension-layout-example).
