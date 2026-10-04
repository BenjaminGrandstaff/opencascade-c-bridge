# Roadmap

Where the project stands and what comes next. The design rationale behind each
item lives in [Parametric architecture](PARAMETRIC_ARCHITECTURE.md); this file
tracks status and order.

## Current status

| Layer | Version | State |
|---|---|---|
| C ABI (`src/`, `include/`) | ABI 37 | Stable; exact version match required |
| `occt-bridge` (safe Rust wrapper) | — | Covers the full ABI |
| `occt-recipes` (application constructors) | — | Stone and wall torch |
| `occt-parametric` (engineering layer) | Schema 50 | Active development |

| Quality gate | Result | Command |
|---|---|---|
| Tests | C 4/4, bridge 81 (+1 doc test), recipes 3, parametric 256 + merge driver 3, mesh Python 4, wing model 6 + CAD 1 | `ctest`, `cargo test` (see README) |
| SonarQube (indexed Rust) | Gate OK, 0 issues, 93.2% line coverage (2026-10-04); Rust unit tests classified as tests | `tools/sonar/run.sh` |
| clang-tidy, cppcheck, clang `-Werror` | Clean | `tools/cpp-lint/run.sh` |
| Rust formatting and Clippy | Clean across all three crates, including all targets | `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` |
| Coverage | 93.23% lines overall, test code excluded; C++ 94.24% lines, 87.32% branches, 100% functions; Rust 92.78% lines | `tools/coverage/run.sh` |
| Scale benchmarks | 69 Rust cases plus a 10,000-face Python matcher passing within budget | `tools/bench/run.sh` |

## Done

### Kernel (C ABI)

- Spline lofts and reliable volumes (ABI 37): `occt_bridge_create_spline_loft`
  interpolates each section as one B-spline closed back to its first point,
  smooth except for that corner (an airfoil trailing edge). Volume, center of
  mass, and solid-boundary measurements now integrate adaptively when any face
  is freeform; the fixed-order default had reported a lofted B-spline airfoil
  about 20% too small. All-analytic shapes keep fixed-order integration, which
  is exact to roundoff there, and no benchmark case changed by more than 25%.

- Signed face radius bounds and edge concavity (ABI 36): per-face smallest
  convex and concave principal radii relative to the outward normal, with
  witness points; exact on planes, cylinders, cones (including apices), spheres,
  and tori, and sampled on an in-face grid elsewhere with the sample count
  reported. Per-edge smooth/convex/concave/mixed classification uses OCCT's
  offset analysis. Both are batch queries in `subshapes` order that create no
  handles. On a 1,000-hole plate, 1,006 exact faces take 0.008 s and 3,012
  edges 0.867 s; 65,536 freeform samples take 1.306 s.

- Bounded surface triangulation and batch topology indices (ABI 35): copied
  geometry preserves source BREP, face indices follow original topology,
  triangle winding follows outward orientation, and caller buffers publish
  transactionally. Degenerate facets are removed; unsafe deflections and
  output budgets fail explicitly.

- Exact orthographic hidden-line projection, closed solid plane clipping,
  bounded edge sampling, and transactional bulk subshape traversal (ABI 34).
  Projection and clipping preserve their input shapes; failed multi-handle
  publication retracts every partial result.

- Multi-station variable fillets (ABI 33): ordered normalized radius samples,
  reversed spines or an explicit endpoint-nearest start point. Each tangent
  contour receives one smooth interpolated law; duplicate edges, closed
  contours, ambiguous start points, and invalid samples fail with cleanup.
  Interpolation can overshoot supplied radii. Existing linear calls remain
  supported. Schema 40 persists interior radius samples and spine control;
  incremental edits rebuild dependents and preserve the last accepted result
  on failure. The 1,000-feature build/edit scale case passes its 60-second budget.
- Exact BREP separation/witness points, non-destructive overlap volume, and
  adaptive solid mass properties (ABI 33). Central unit-density inertia is
  evaluated near the shape to avoid far-origin cancellation. Analytic box,
  rotated tensor, scale/translation, contact, containment, invalid-argument,
  session, and cleanup tests cover the new entry points.
- Open-profile translated closure (ABI 31): one valid open wire is joined to
  its translated reversed copy by straight endpoint bridges. The resulting
  face must be planar and non-self-intersecting, with original/translated
  edge ancestry retained. Construction and history indexing are O(edges)
  time/storage; OCCT's geometric self-intersection check is O(edges²) in the
  worst case, without graph or support searches. Three bridge tests and C
  conformance cover exact line/arc/multi-segment areas, downstream extrusion
  history, invalid offsets and boundaries, foreign/stale handles, and cleanup.
- Explicit operation-history composition (ABI 30): a new handle shares the
  result geometry and retains direct history while tracing an intermediate's
  original sources. Generated and modified ancestry follow OCCT's history
  rules; absent targets are removed and deleted sources may still generate
  topology. Inputs remain unchanged. Shape-identity indexes bound composition
  to expected O(topology + records + expanded relations) time and memory;
  only explicitly composed rigid histories expand beyond O(1) storage.
  Four bridge tests check rigid and extrusion chains, modified-to-generated
  relations, deleted targets, shared geometry, released intermediates, wrong
  sessions, stale handles, invalid chains, repeated-composition deduplication,
  and bounded cleanup. C argument
  conformance covers the new entry point.
- Linear variable-radius fillets (ABI 29): finite positive endpoint radii
  along open tangent contours, using OCCT's spine direction. Tangent
  neighbors share one law; duplicate edges and closed contours are rejected.
  Result validation/healing, input preservation, history, and bounded failure
  diagnostics follow the constant-radius path. Edge membership is indexed
  once per call, costing O(input topology + selected edges) time and memory.
  Fillet and chamfer builds serialize across sessions after reproducible
  concurrent OCCT 7.9 blend failures; eight-thread mixed-treatment tests
  exercise this path with independent sessions and bounded handle counts.
- Selected-face draft (ABI 28): signed radians, neutral plane and pull
  direction, descendant face validation, duplicate rejection, and planar,
  cylindrical, or conical surfaces. OCCT draft status and problematic subshapes
  are exposed through structured diagnostics. Result validation/healing and
  corrected modified-shape history are retained; input handles remain unchanged.
- Sessions, integer shape handles, history-preserving duplicates, exception
  containment at every entry point.
- Primitives, wires, faces, prisms, polyline tubes, lofts, compounds.
- Face revolution (ABI 27), with finite nonzero axes, signed partial/full
  sweeps up to one turn, generated topology history, and session result
  validation. Tests cover exact annular-solid volumes, negative sweeps,
  invalid arguments, wrong-session/stale handles, and cleanup.
- Exact mixed line/circular-arc wires (ABI 26), including major arcs, with
  ordered connectivity and optional closure checks. Safe Rust bindings expose
  `WireSegment` and `Session::create_segment_wire`; schema 26 uses them for
  exact curved sketch faces.
- Location-only sharing for placed copies: translation and rotation attach
  a location to shared geometry and record located history computed on
  demand. Measured: 0.7 KiB per placed copy of a 20-hole plate instead of
  168 KiB, and regenerating 10,000 copies takes 0.36 s instead of 0.85 s.
- Automatic handle cleanup: dropping a Rust shape handle releases the
  kernel shape and its history without disturbing the session's last error
  or warnings, so repeated regeneration returns the session to its starting
  shape count. Measured cost: about 10-15% on regenerating 10,000 copies.
- Result validation after booleans, fillets, chamfers, offsets, hollowing,
  sewing, and STEP and BREP import (on by default), optional shape healing
  that carries operation history to the repaired faces, fuzzy booleans, and
  per-call warnings with OCCT alert names; boolean failures name OCCT's
  reason. Faces with at least 16 wires use equivalent per-wire proxy checks
  and bounding-box-pruned classification instead of repeated whole-face
  checks. The differential test matches `BRepCheck_Analyzer` verdicts and
  statuses across valid and deliberately invalid topology. Final benchmarks:
  1.58x validation overhead on a 100-cut chain and 1.13x on a 400-hole
  single cut; direct validation of that plate takes 0.045 s instead of
  0.212 s with `BRepCheck_Analyzer`.
- Structured failure diagnostics: a failed fillet, chamfer, offset, hollow,
  or boolean, or a result rejected by validation, reports what OCCT said
  caused it, using OCCT's own codes and names (`ChFiDS_ErrorStatus`,
  `BRepOffset_Error`, BOPAlgo alert keys, `BRepCheck_Status`), the index of
  the selected edge, face, or boolean operand at fault, and a handle to the
  offending subshape on request. Chamfers, and fillets for which OCCT names
  no faulty contour, are diagnosed by rebuilding each contour alone. The
  work runs only after a failure and is bounded: at most 64 contours are
  rebuilt and 64 diagnostics kept, with the rest counted. The parametric
  layer reports the failing feature and the selector or boolean input at
  fault.
- Sewing faces and shells (with operation history), closing a single shell,
  and constructing validated solids with one outer shell and internal void
  shells. Multi-shell construction rejects open, intersecting, overlapping,
  and disjoint boundaries.
- Booleans, fillets, chamfers, offsets, hollowing, transforms.
- Topology traversal, adjacency, recorded tangency, topological identity.
- Measurements, BREP validity, BREP persistence, STEP import/export,
  configurable ASCII/binary STL export, and operation history.
- Curvature: midpoint, sampled range, and exact (line, conic) or
  error-bounded (Bezier, B-spline) extrema.

### Parametric layer

- Print-bed fit requirements (schema 50): `FitsWithin` checks exact bounding-box
  extents against a length envelope in any axis-aligned orientation, reporting
  the largest extent. One test covers rotated fits, failures, and validation.

- Loft features (schema 49): `FeatureOperation::Loft` through planar
  `LoftSection` outlines placed by parameter expressions (origin, axes, scale,
  and rotation about a pivot), as smooth B-spline or polygon sections, ruled or
  smoothed between them. Three tests cover parameter edits, rotation, smooth
  versus polygon accuracy, validation, and persistence. A 200-station,
  80-point smooth airfoil loft builds and rebuilds after a chord edit in
  1.309 s (5 s budget), with the edit scaling volume by exactly 1.21.

- Sampled manufacturing requirements (schema 48): `MinimumWall`, `DraftAngle`,
  and `Overhang` store their mesh settings in the family and screen the output's
  tessellation on every regeneration with `Sampled` evidence. Wall failures
  report the ray's entry and exit; draft checks each face's smallest draft
  magnitude, the usual "requires draft" analysis, without undercut detection.
  Two tests cover a 2 mm-walled cup, vertical and tilted faces, a T-shaped
  overhang, validation, and persistence. 10,002 instances with all three rules
  regenerate in 0.477 s (3 s budget), screening the shared variant once. This
  completes the richer requirement rules. See
  [Requirement rules](REQUIREMENTS.md).

- Minimum-radius requirements (schema 47; ABI 36): `MinimumRadius` checks
  convex, concave, or both radii against a positive length, optionally treating
  sharp edges on that side as radius zero (inside corners a round cutter cannot
  reach). Results report the smallest radius, `Exact` or `Sampled` evidence,
  and the face or edge with a witness point. Two parametric tests and three
  bridge tests cover bores, blind-hole floors, cylinders, cones, spheres, tori,
  inside-corner fillets, outside edges, sampled variable blends, validation,
  and persistence. See [Requirement rules](REQUIREMENTS.md).

- Exact connectivity and collision requirements (schema 46): part
  `Connectivity` counts solids, voids, and faces, edges, or vertices outside any
  solid; assembly `NoInterference` and `MinimumClearance` run the exact indexed
  collision checks over explicit or all-instances output sets, with a cross-set
  query that never inspects pairs within one set. Every result now reports a
  normalized measured value, evidence quality, and collision witnesses. Six
  tests cover disjoint, hollow, and sewn-shell topology, contact versus
  overlap, within- and between-set clearance, set validation, suppression,
  pattern-member protection, and persistence. 10,002 instances with
  connectivity, no-interference, and clearance rules regenerate and verify in
  0.574 s (3 s budget), checking the part rule once for the shared variant; a
  5,000 x 5,000 cross-set clearance with 500 violations takes 1.305 s (5 s
  budget). See [Requirement rules](REQUIREMENTS.md).

- Continuous translation paths (no schema or ABI change): exact BREP distance
  and a relative-motion bound check intervals between motion samples. Swept
  bounds index candidate pairs; local variants generate once. Witnesses report
  contact, interference, or insufficient clearance. Query/depth limits and
  uncertain grazing intervals report Unresolved. Six tests cover thin obstacles
  missed by sampled endpoints, independent analytic slab checks, nested rotated
  frames, co-moving parts, limits, numeric guards, and bounded cleanup. Rotating
  paths are covered below; first-time-of-contact computation remains open.
  The 10,000-body moving assembly passes in 0.285 s (10 s budget); 1,000 independent crossing
  checks pass in 11.808 s (30 s budget). See
  [Assembly motion](ASSEMBLY_MOTION.md).

- Sheet metal and expanded hole catalogs (schema 45): `SheetMetal` builds a
  constant-width strip of flanges joined by exact circular bends;
  `SheetMetalFlat` builds its blank for an explicit neutral factor, and
  kernel-free `flat_pattern` metrics export as SVG/DXF drawings with bend lines.
  Frozen Carr Lane V1 catalogs add 64 metric and inch tap-drill pairs and 38
  socket-head sizes (counterbore diameter/depth, normal/close clearance), each
  value checked against the Rev. 9/2021 booklet. Nine tests cover analytic
  volumes, incremental reuse, extreme scales, invalid outlines, drawings,
  published values, and catalog-driven holes. 1,000 folded brackets with linked
  blanks build and edit in 5.493 s (15 s budget); 100,000 worst-row catalog
  lookups take 2 ms (200 ms budget). Edge flanges, reliefs, hems, bend
  tables, and countersink relief are not supported. See
  [Sheet metal](SHEET_METAL.md) and [Hole-size catalog](HOLE_SIZE_CATALOG.md).

- Analysis and manufacturing hand-off (schema 44): semantic face tags, source
  BREP and surface meshes for an isolated Gmsh tetrahedral runner, material-aware
  glTF with shared buffers, and sampled wall/draft/overhang checks. Five API
  tests, independent positive-volume tetrahedron checks, and Khronos validation
  cover the exports. A 1,000-part glTF scene takes 1.393 s; 10,000 indexed wall
  rays on 100,518 triangles take 3.284 s (10 s budgets). Screening is sampled;
  external volume meshing validates tag matching and element quality.
  See [Mesh hand-off](MESH_HANDOFF.md).

- Model data management (schema 43): explicit linear revision records with
  nonrecursive semantic changes, resolved instance/feature change-impact
  reports, and a semantic Git merge driver with atomic successful writes.
  Seven API tests and three driver tests cover rollback, inheritance,
  downstream dependencies, current joints, materials, configuration suppression,
  history conflicts, and file preservation. Actual Git merges verify successful
  combination and conflict stages. Impact for 10,000 instances and 100 dependent
  features takes 0.144 s; appending after 10,000 revisions takes 0.185 s
  (10 s budgets). See [Model history](MODEL_HISTORY.md) and
  [Document comparisons](DOCUMENT_DIFF.md).

- Regenerated drawings (schema 42): orthographic, section, and cropped detail
  views with exact hidden-line removal, datum dimensions, parameter notes,
  and framed metadata title blocks. SVG and millimeter DXF exports use bounded
  sampled polylines. Seven tests cover regeneration, validation, persistence,
  semantic diff, export escaping, and cleanup. Independent DXF parsing reports
  zero errors or repairs. The 1,000-view case takes 1.235 s (10 s budget);
  a view of a 1,000-part assembly takes 0.903 s (20 s budget). Section hatching
  and certified curve approximation remain future extensions.
  API and limits: [Drawings](DRAWINGS.md).

- Closed-linkage sampled motion (no schema or ABI change): each driven pose
  seeds its free coordinates from the preceding successful solve. All poses
  must close before shared local geometry and sampled collision checks run.
  Closure failures and a total iteration budget report the failed/unattempted
  sample and best fits without exposing a partial study or changing the graph.
  Six tests cover analytic positions on both seeded branches, collision checks,
  late limits, omissions, budgets, invalid roles, native errors, and cleanup.
  10,000 closures take 0.445 s; 1,000 closed poses with collision checks take
  0.534 s (5 s budgets). This provides local continuation and sampled checks;
  it does not certify a continuously closed mechanism's path. See
  [Assembly motion](ASSEMBLY_MOTION.md).

- Continuous rotating joint paths (no schema or ABI change): unwrapped angle
  interpolation preserves full, reverse, and multiple turns. Enclosing swept
  spheres and point-speed bounds through nested frames support revolute,
  cylindrical, and planar motion together with translations. Exact BREP queries
  and adaptive subdivision return witnessed collisions or unresolved intervals
  when the numeric margins or budgets prevent clearance. Six tests cover
  independent pose/bound oracles, unsampled collisions, invalid/overflowing
  paths, large-angle uncertainty, state preservation, and handle cleanup.
  10,000 sparse rotors take 0.180 s; 1,000 obstacle crossings checked against
  planar separating-axis geometry take 10.640 s (10 s and 30 s budgets).
  See [Assembly motion](ASSEMBLY_MOTION.md).

- Bounded closed-linkage solving (no schema or ABI change): explicitly selected
  joint coordinates adjust while driven coordinates and rest placements stay
  fixed. Coordinate limits constrain trial steps; only a candidate satisfying
  all assembly relationships is applied. Failed solves report their best fit
  without changing the graph. Six tests cover analytic crank-slider/four-bar
  closure, scales, distant origins, planar alignment, conflicts, units, limits,
  persistence, nullity, and atomic failures. Limits: 32 coordinates, 256
  relationships, and up to 1,000 local iterations. Global branch search and
  large assembly solver scaling remain open. The 1,000-pose analytic
  crank-slider benchmark takes 0.040 s (10 s budget). SonarQube passes with
  zero issues. API: [Assembly motion](ASSEMBLY_MOTION.md).

- Full per-component and assembly mass properties: inherited material density,
  current instance/frame/joint poses, center of mass, and central inertia in
  world axes. Weighted central aggregation and parallel-axis corrections retain
  accuracy far from the origin. Explicit outputs prevent counting intermediate
  features; overlapping components contribute their full masses. Two analytic
  tests cover mixed densities, joint rotations/translations, distant geometry,
  invalid selections/materials, underflow, source preservation, and cleanup.
  The 1,000-component shared-generation case takes 0.576 s (5 s budget).
- Driven assembly joints and sampled motion (schema 41): fixed, revolute,
  prismatic, cylindrical, and planar frame joints with unit-aware coordinates
  and checked optional limits. Atomic edits preserve accepted state. Exact
  BREP interference/contact/clearance checks use a median BVH to find candidate
  pairs. Motion generates local parameter variants once and places shared
  copies for each independent sample; it reports collision and relationship
  checks and releases temporary geometry on success and failure. Four tests
  cover joint kinds, parent transforms, persistence, invalid edits, indexed
  versus exhaustive collisions, sampled crossings, geometry reuse, and cleanup.
  Scale cases cover 10,000 joints, 10,000 sparse bodies, and 1,000 motion samples.
  Translation path checks and bounded closed-linkage solving are available;
  continuous rotation checking supports nested frame paths.
  API and limits: [Assembly motion](ASSEMBLY_MOTION.md).
- Three-way semantic document merge (no schema or ABI change):
  `base.three_way_merge(&left, &right)` combines independent field/entity edits
  and identical concurrent edits. Typed conflict records retain base/left/right
  values for delete/edit, same-ID additions, incompatible variant replacements,
  and ordered-array conflicts. Input and combined-document validation reject
  broken references, dependencies, and parameter constraints; conflicting merges
  return no partial document. Fifteen tests cover conflicts, nullable payloads,
  omitted dictionaries, additional families, symmetry, input preservation,
  round trips, and regenerated geometry with exact volume and bounded handles.
  Ten validated merges of 10,000 reordered instances take 2.463 s (8 s budget),
  with both independent overrides retained and no kernel handles allocated.
  API and limits: [Semantic document comparisons and merges](DOCUMENT_DIFF.md).
- Identity-based semantic document diffs (no schema or ABI change):
  `ModelDocument::semantic_diff` reports deterministic typed field/entity paths
  with before/after values. Declaration lists match by stable IDs; ordered
  profiles, operands, pattern members, and audit records retain their order.
  Duplicate IDs fail, and absent values remain distinct from JSON null across
  serialized change-record round trips. Seven tests cover parameter, feature,
  instance, additional-family, and relationship edits, additions/removals,
  reordering, duplicate IDs, and punctuation in IDs. Ten comparisons of 10,000
  reordered instances with one exact override edit take 1.265 s (5 s budget),
  without creating kernel handles. Revision history, impact
  reports, and a git merge driver remain planned. API and limits:
  [Semantic document comparisons](DOCUMENT_DIFF.md).
- Uniform extend-to-next ribs (schema 39; ABI 32):
  `RibProfileMode::OpenToNext { direction, maximum_length }` advances a straight
  open chain perpendicularly to its first contact with the input body. Exact
  common/section geometry includes tangencies and the exact reach boundary;
  the entire translated chain must meet the first contact. Partial nearer
  obstacles, nonuniform profiles, on/inside profiles, and missing support
  fail. Thickness placement, composed profile/body history, selective rebuilds,
  and accepted-generation rollback are preserved. Six parametric and two
  bridge tests plus C error conformance cover geometry/history, bounded reach,
  nearer/partial supports, units, body edits, migration, scales, and cleanup.
  A 1,000-rib build/edit benchmark passes in 13.665 s (30 s budget), checking
  exact volumes, centroids, generated-face history, input reuse, and cleanup.
  Support searches use non-destructive kernel operations to avoid accumulating
  geometry changes on reusable inputs. General curve-dependent closure and
  support-following remain future work.
- Bounded open-sketch strip ribs (schema 38): `SketchOpenWire` emits ordered
  open line/arc chains. `RibProfileMode::OpenStrip { offset }` defines an
  explicit translated closure before applying one-sided or centered thickness
  and fusing into one body. Earlier ribs default to `Closed`. Offset parameters
  drive selective rebuilds; failures preserve accepted generations. Six new
  tests cover exact volume/history, arcs, small/large/distant models, reversed
  directions, units, invalid chains/closures, rollback, migration, and cleanup.
  The new 1,000-open-rib build/edit benchmark passes in 8.413 s (15 s budget),
  checking every volume, centroid, generated face, reuse, and released handle.
  Uniform extend-to-next landed in schema 39; general support-following
  remains future work.
- Rib profile-history composition and generated-face selectors (schema 37):
  original profile edges trace through extrusion, optional centered placement,
  and fusion while body history is preserved. `FaceSelector::GeneratedFromEdges`
  selects surviving faces from an earlier feature's semantic edge selection.
  Four new tests cover faces/wires, both thickness modes and direction signs,
  exact generated-face areas/centers, downstream draft, document round trips,
  parameter signatures, failed edits, removed faces, missing references, and
  cleanup. Both 1,000-rib benchmark cases verify composed history before and
  after thickness edits in 6.017 s (one-sided) and 6.924 s (centered), within
  their existing 15 s budgets.
- Centered closed-profile ribs (schema 36; ABI 29 unchanged):
  `RibThicknessMode::{OneSided, Centered}` places total thickness on one side
  or equally on both sides of the profile plane. Earlier ribs default to
  one-sided geometry. Centering adds one temporary location-only placement;
  direction reversal preserves centered geometry, while mode edits rebuild
  the rib and downstream features and reuse unchanged inputs. Four new tests
  check exact volume/center of mass, faces and wires, normal signs, 1 km
  model offsets, small models, body history, connection failures, contained
  walls, rollback, cleanup, round trips, and legacy defaults. A small
  operand-relative volume margin rejects contained-wall fuse roundoff.
  The scale suite also builds and edits 1,000 centered ribs, checking volume,
  center of mass, validity, reuse, and handle cleanup (15 s budget).
- Linear variable-radius fillet features (schema 35): semantic selectors,
  length-valued start/end radii, one valid input/result solid, dependency
  ordering, selective reuse, and rollback. Tests cover geometry bounds,
  endpoint reversal, constant-radius equivalence, tangent propagation,
  history, units, selector and radius edits, diagnostics, migration, and
  cleanup. Arbitrary multi-station laws, per-contour radius pairs, and explicit
  spine-direction control remain future work. The scale case builds and edits
  1,000 features, verifies volume bounds and validity, reuses the body, and
  releases every handle in 18.234 s (60 s budget, set from an initial 18 s measurement).
- Bounded closed-profile ribs (schema 34; no ABI change): a planar sketch
  face/wire is extruded normally by positive length-valued thickness along a
  normalized scalar direction, then fused into one input solid. The result
  must add material and remain one valid solid. Disconnected, edge-only,
  fully contained, and invalid ribs fail. Body/profile references and their
  expressions drive ordering, reuse, and downstream rebuilds; failures preserve
  accepted generations. Four tests cover exact triangular and overlap volumes,
  both extrusion directions, body history, input preservation, units,
  connection failures, selective reuse, rollback, cleanup, and migration.
  Uniform extend-to-next landed in schema 39; general support-following ribs
  remain future work. Bounded
  open-sketch profiles landed in schema 38, and
  profile-history composition landed
  in schema 37.
  Building and editing 1,000 ribs takes 6.017 s (15 s budget), checking every
  volume, valid results, unchanged body/profile reuse, and cleanup.
- Draft features (schema 33): semantic face selectors, length-valued neutral
  origin, scalar normal/pull direction and signed angle. Input and result must
  contain one valid solid with positive result volume. Selector references and
  all expressions participate in ordering and incremental signatures; failed
  edits preserve previous generations and release all new/selected handles.
  Four bridge and four parametric tests cover exact wedge/frustum volumes,
  positive/negative drafts, history, input preservation, invalid selections,
  units/directions, OCCT diagnostics, parameter edits, cleanup, and migration.
  Topology-changing drafts and surfaces outside OCCT's supported set remain
  unsupported.
  Building and editing 1,000 draft features takes 2.922 s (10 s budget),
  checking every signed volume, valid results, input reuse, and cleanup.
- Frozen metric clearance-hole catalog (schema 32; no ABI change).
  `ScalarExpr::Iso273ClearanceV1` and `iso273_clearance_v1` resolve 19 nominal
  fastener sizes in fine, medium, and coarse series to millimeter diameters.
  Length-valued inputs support unit conversion; unsupported sizes and invalid
  quantities fail without interpolation. Catalog expressions work in derived
  parameters and hole dimensions, track dependencies, and rebuild only affected
  branches. Four tests check all 57 values, units, unsupported sizes, exact
  volumes, edits, rollback, round trips, and unchanged schema 31 migration.
  The V1 snapshot is immutable; manufacturing tolerances, tap-drill catalogs,
  inch catalogs, and standard recess dimensions remain out of scope.
  Source, API, and supported sizes: [Hole-size catalog](HOLE_SIZE_CATALOG.md).
  100,000 checked lookups take about 1 ms (200 ms budget).
- Recorded internal-thread specifications (schema 31; no ABI change):
  caller-supplied designation, positive length-valued pitch, length-valued
  nominal diameter greater than the bore diameter, and right/left handedness.
  Existing documents default to no thread record. Metadata and parameter edits
  rebuild the affected hole branch without changing cylindrical geometry.
  Three tests cover round trips, migration, unchanged volume, invalid metadata
  and units, selective reuse, failed-edit rollback, and handle cleanup.
  No helical geometry, standards lookup, tap-drill inference, tolerance-class
  validation, engagement-length inference, or machinability claim is made.
  Building and editing thread records on 100 holes takes 5.418 s (10 s budget),
  checking unchanged volume, input reuse, branch rebuilds, and cleanup.
- First-class cylindrical holes (schema 29; no ABI change): positive
  length-valued diameter, family-local position and normalized scalar axis,
  with flat-bottom blind depth or through-all extent. Through-all projects
  the input bounds along both directions of the axis line; blind depth starts
  at the supplied position. Cuts must remove material and leave one valid
  solid with positive volume. History is preserved, temporary tools are
  released, and parameter edits rebuild only the affected branch; failures
  preserve the prior accepted generation. Five tests cover exact volumes,
  distant/rotated axes, units, invalid cuts, history, cleanup, selective reuse,
  and schema migration. Building and editing 100 holes takes 5.490 s
  (10 s budget). Broader size catalogs remain planned.
- Counterbore and countersink entry recesses (schema 30; no ABI change).
  `HoleFinish` defaults to plain for older documents; counterbores use a
  length-valued diameter/depth, countersinks a length-valued diameter and
  scalar included angle in radians. Entry diameter must exceed bore diameter,
  and recess depth must be positive and shallower than a blind bore. Recesses
  start at the caller-supplied hole position along its normalized axis.
  Fused cutters make one history-preserving cut from the original input;
  all temporary handles are released. Four additional tests cover exact
  volumes, history, parameter edits, invalid dimensions/units/angles,
  failed-edit rollback, and schema 29 migration. Building and editing 100
  counterbores takes 10.195 s and 100 countersinks 8.447 s (15 s budgets),
  checking exact final volumes, input reuse, and cleanup.
- Typed, unit-aware parameters; derived scalar and vector expressions;
  pre-generation constraints.
- Dependency-ordered feature graphs with incremental reuse of unchanged
  features; required, preferred, and advisory verification.
- Sewing and single- or multi-shell solid construction as serializable feature
  operations.
- Extrude and revolve from sketch face or closed-wire outputs (schema 28).
  Length-valued extrusion vectors allow oblique and negative directions;
  revolution uses a typed origin/axis and signed scalar radians. Both reject
  invalid profiles and non-solid/invalid/zero-volume results. Temporary faces
  from wires are released; generated history tracks original profile edges.
  Tests cover exact polygon/arc/circle extrusion volumes, full and partial
  tori about offset axes, dependency ordering, selective incremental reuse,
  failed-edit rollback, units, invalid inputs, and schema 27 migration.
  Building and editing 1,000 sweeps takes 0.562 s for extrude and 0.700 s
  for revolve (5 s budgets), checking every volume, profile reuse, and cleanup.
- Constraint-solved 2D sketches: lines, exact arcs/circles, construction
  geometry, coincident, horizontal, vertical, parallel, perpendicular,
  equal-length, distance, and contact-tangent constraints. Radius dimensions
  use center-to-boundary distance expressions; arcs enforce equal radii.
  The sparse solver reports convergence, free degrees, and redundancy;
  conflicting constraints reject generation. `SketchFace` and `SketchWire`
  emit exact closed profiles, with minor/major and clockwise arc sweeps.
  Schema 27 adds optional family plane-datum linkage; origin and normal come
  from the datum, x-axis defines the in-plane orientation, and y-axis is
  normal cross x. Datum definitions and their parameters participate in
  incremental regeneration; missing/non-plane datums and incompatible axes
  are rejected. Tests cover exact area/perimeter, parameter and datum edits,
  selective reuse, invalid inputs, schema migration, and handle cleanup.
  Measured: 10,000 independent lines solve in 0.036 s, a 1,000-line chain in
  0.005 s, and 10,000 arc/tangent sketches in 0.117 s. Generating 10,000 wires
  on 10,000 indexed datum planes takes 0.082 s (5 s budget), including cleanup.
- Semantic edge and face selectors: extrema, size, curvature (midpoint,
  sampled, proven-bound), normals, adjacency, tangency, set composition,
  operation history.
- Linked clones with sparse overrides, detachment, freezing, and managed
  regeneration.
- Iterative clone inheritance resolution preserves exact cycle paths and
  base-to-leaf override precedence without consuming call stack. Bulk graph
  operations memoize every resolved parent in an operation-local cache, so
  document validation, configuration validation, and regeneration resolve a
  clone forest in O(nodes + inherited override copies) without stale cache
  state after mutation. A 20,000-link chain resolves on a default-stack thread
  in 0.010 s, and validating all 20,000 instances takes 0.035 s; the recursive
  single-leaf implementation took 0.386 s on a custom 16 MiB stack.
- Nested assembly frames; shared generation for clones that differ only in
  placement.
- Multi-family graphs and documents; clones inherit their root family's
  definition, and generation sharing is isolated by family identity.
- Patterns: linear and circular rules; `LinearFit` and `CircularFit`
  constraint rules that solve count and spacing; editable rules and counts;
  stable member slots; per-member placement overrides and suppression;
  counts driven by integer family parameters or measured output bounds;
  fitted spans driven by length parameters or measured output bounds.
- Assembly semantics: family datums (points, axes, planes) resolved through
  placement and frames; checked coincident, parallel, perpendicular, and
  distance relationships; configurations layering overrides and suppression;
  materials with inheritance and mass.
- Relationship solving: free instances placed by Levenberg–Marquardt so
  their relationships hold, with free-degree and redundancy reporting and
  no change to the graph when relationships conflict. Rotations pivot about
  each instance, angular residuals are length-scaled, and free directions
  are restored toward the start.
- Scalable relationship solving: each relationship is differentiated only
  in the at most twelve unknowns of the instances it touches, and the
  normal equations are solved by sparse minimum-degree elimination that
  leaves unconstrained directions unmoved, so disconnected groups never
  interact and chains produce almost no fill-in. Measured: 1,000-part
  stacks solve in 0.02 s (seated) and 0.35 s (fully constrained), a
  1,019-part 30x34 grid with cycles in 7.3 s, and 50-part stacks 1 km from
  the origin converge within the default tolerances.
- Configurable relationship tolerances: each model records finite positive
  linear (millimeter) and angular (radian) tolerances, used by both placement
  solving and relationship checks. Schema 23 preserves the former 1e-6 mm
  and 1e-9 rad values as migration defaults. A 50-part stack 1 km from the
  origin solves under a tighter 1e-8 mm model tolerance in 0.001 s.
- Assembly requirements for mass ranges, datum-clearance ranges, and recorded
  relationship satisfaction. They use the same required, preferred, and
  advisory semantics as family requirements; required failures release the
  complete graph generation. References, units, ranges, outputs, and material
  availability are validated before use. Atomically adding and validating
  10,000 datum requirements takes 0.013 s; shared regeneration and evaluation takes
  0.379 s. Schema 24 persists them and older documents default to none.
- Versioned JSON documents with migrations from every schema since v1.

### Recipes

- Wall torch and faceted stone, both built only from generic operations; the
  stone now sews planar facets and matches the legacy constructor's volume.

### Tooling

- Containerized C/C++ lint and merged LLVM coverage for both languages.
- Sonar generic-coverage conversion, worktree-aware containerized scanning,
  rejection of empty analyses, quality-gate
  enforcement, and issue-count enforcement. Community Build reports the Rust
  portion; C/C++ records remain in the report for servers with CFamily.
- Argument-validation conformance test for every C entry point.
- Differential scalable-validator conformance tests against
  `BRepCheck_Analyzer`, including many-wire valid and invalid topology.
- Scale benchmark suite with time budgets and correctness checks at the
  target sizes, reporting known gaps against open roadmap items.
- Wing layout workshop (`tools/wing-layout`): a browser station editor and a
  CAD CLI. Project files now become a parametric wing family with smooth
  airfoil lofts, per-station parameters, stored validity and connectivity
  requirements, and a saved model document beside the STEP and BREP files.
  A build file adds printable structure on the parametric wing: a swept spar
  channel, optional elevons behind a gapped hinge line, and spanwise print
  segments, exported as STL parts. Each part is checked for a single solid
  (required), print-bed fit, and overhang (preferred), and failures print
  their location. On the starter wing, ten parts export in about six seconds.

## Scaling requirement

Scale is a requirement for every change, not a later optimization. Target
sizes are 10,000 instances or pattern members per graph, assemblies of
1,000 parts, and long-running processes that regenerate repeatedly in one
session. Before a change lands:

- **Complexity is stated.** Note the time and memory complexity of new
  algorithms in their doc comments; avoid scans of every node, member, or
  relationship inside per-item loops, and justify anything above
  O(n log n) in graph size.
- **It is measured.** Add or extend a case in the scale benchmark suite
  (`rust/occt-parametric/benches/scale/`, run by `tools/bench/run.sh`) at
  the target sizes and keep it inside its budget; a case tied to an open
  roadmap item is marked as a known gap until that item lands.
- **Handles are bounded.** Session shape counts must return to their
  starting value after a regeneration result is released, and repeated
  regeneration must not grow memory.
- **Geometry is not duplicated needlessly.** Prefer shared or
  location-only shapes when only placement differs.
- **Numerics are scale-aware.** Tolerances and solver conditioning must hold
  for parts far from the model origin and for both millimeter and
  kilometer-sized models.

## Next

Ordered by priority. Each item should land with tests, a schema bump when the
document format changes, the scaling requirement above, and updates to this
file. The project is a code-first engine, not an interactive application:
every item below is defined in documents and the API, and verified in tests.

### Capabilities

1. **Moving elevons.** Elevons as separate instances on revolute joints, with
    a motion study over their deflection range checking for interference.
2. **Mass and balance report.** Per-material mass and the wing's center of
    gravity relative to its mean aerodynamic chord, reported as data only.
3. **Rib templates.** Section drawings through the wing at each rib station,
    exported as DXF for cutting.

## Later

- General sheet-metal edge flanges, bend reliefs, hems, cutouts, bend tables,
  and unfolding edited solids beyond constant-width strips.
- Hole-catalog tolerance classes and optional under-head countersink relief.
- Wing structure beyond solid printed segments: hollow shells with internal
  ribs, alignment pins independent of the spar, twist-exact hinge lines, and
  print-bed fit in arbitrary (not only quarter-turn) orientations.

- Advanced ribs with general support-following and nonuniform closure.
- Global linkage branch search, large assembly coordinate solving, and tighter
  swept bounds for dense or deeply nested rotating mechanisms.
- Undercut detection against a parting line, and exact (not sampled) minimum
  wall thickness and draft on curved BREP faces.
- Assumptions and requirement-to-feature trace links in the document schema.
- Semantic naming beyond feature outputs, and geometric tangency inference
  when continuity metadata is absent.
- Additional domain-specific expression functions.
- Integration with the broader EIL source model in the sibling
  [`engineering-intent-language`](../engineering-intent-language) project.

## Keeping this current

When a feature lands, confirm it meets the scaling requirement, move it from
**Next** to **Done**, refresh the status tables (ABI and schema versions,
test counts, coverage, benchmark results), and adjust the
"next work" paragraph in [Parametric architecture](PARAMETRIC_ARCHITECTURE.md)
to match.
