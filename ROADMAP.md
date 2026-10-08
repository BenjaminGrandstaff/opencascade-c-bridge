# Roadmap

Where the project stands and what comes next. The design rationale behind each
item lives in [Parametric architecture](PARAMETRIC_ARCHITECTURE.md); this file
tracks status and order.

## Current status

| Layer | Version | State |
|---|---|---|
| C ABI (`src/`, `include/`) | ABI 49 | Stable; exact version match required |
| `occt-bridge` (safe Rust wrapper) | — | Covers the full ABI |
| `occt-recipes` (application constructors) | — | Stone and wall torch |
| `occt-parametric` (engineering layer) | Schema 77 | Active development |

| Quality gate | Result | Command |
|---|---|---|
| Tests | C 5/5, bridge 102 + first-use integration 1 (+1 doc test), recipes 3, parametric 409 + merge driver 3 + motion command 16 + balance command 4 + drawing command 3 + inspection command 5 + view command 13 + viewer Node 17 + branch command 2 + model command 20, MCP Python 8, mesh Python 4, wing model 6 + CAD 1 | `ctest`, `cargo test` (see README) |
| SonarQube (indexed Rust) | Gate OK, 0 issues, 93.9% line coverage (2026-10-04); Rust unit tests classified as tests | `tools/sonar/run.sh` |
| clang-tidy, cppcheck, clang `-Werror` | Compiler build passes; full lint flags existing sketch/extrusion complexity and sketch C/header parameter-name mismatches. New profile-loft code passes targeted lint. | `tools/cpp-lint/run.sh` |
| Rust formatting and Clippy | Clean across all three crates, including all targets | `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` |
| Coverage | Last recorded: 93.61% lines overall, test code excluded; C++ 94.13% lines, 87.40% branches, 100% functions; Rust 93.42% lines | `tools/coverage/run.sh` |
| Scale benchmarks | 117 Rust cases plus 5 model-command and 8 MCP cases and a 10,000-face Python matcher passing within budget | `tools/bench/run.sh` |

## Done

- Native sweep route annotations (no ABI/schema change): solid views show
  native summed edge length and sampled route curves, with links to upstream
  path controls. Both viewers and SVG snapshots display curves; assembly
  overlays transform routes through family placement and clip hidden samples.
  Route samples obey the global vertex budget. AI example `curved-pipe` exposes
  a tangent line/arc sweep. Model, MCP and JavaScript tests cover edits, exact
  length, controls, rendering, placement and budget boundaries.
  The 100-scene release-scale gate passes in 6.278 s (10 s budget).
- Saved-profile lofts (ABI 49, schema 77): native closed planar sketch
  faces/wires become ordered ruled or smooth solid transitions. Circles,
  ellipses, arcs, splines and mixed edge counts retain native section geometry;
  compatibility operates on copies and source-edge history is preserved.
  Dependencies support selective section rebuilds. Viewer centroid-spacing
  dimensions, sketch scenes and AI example `profile-loft` expose the result.
  Four library tests, three bridge tests and C argument checks cover volumes,
  history, immutable inputs, malformed profiles and cleanup. The 1,000-case
  regeneration gate passes in 1.515 s (10 s budget). The native rebuild also
  corrects old C error tests that used void shape-release calls as status values.
- Geometry-driven holes (schema 76, native ABI unchanged at 48): up-to-face
  and up-to-next depth with exact bounded planar/inclined/curved cutoffs.
  Selected/named references and selector parameters enter dependencies and
  incremental invalidation. Recesses are checked against the same cutoff;
  shallow wide counterbores can open into smaller internal cavity faces.
  Derived centre-ray measurements have linked upstream controls; drawing
  callouts and AI example `hole-limits` preserve the mode. Seven library tests
  cover geometry, selectors, edits, history and cleanup; command/MCP checks
  cover measured views. The 100 two-hole curved-limit gate passes in 6.200 s
  (10 s budget), with no retained handles; prior extrusion gates also pass.
- Blind drill points (schema 75, native ABI unchanged at 48): optional included
  angle adds a conical tip below the full-diameter bore depth. Exact native
  containment rejects tip breakout; entry recesses remain compatible. Angle
  edits rebuild the hole and downstream features while reusing the blank.
  Drawings distinguish full-diameter depth and point angle; viewer annotations
  show bore/point dimensions with linked controls. AI example `drill-point` is
  exposed in generated schemas and MCP resources. Four geometry tests, drawing
  and viewer integration checks cover volumes, history, cleanup and rejection.
  The 500-regeneration gate passes in 3.059 s (10 s budget).
- Inclined/curved extrusion limits (ABI 48, schema 74): native finite face
  splitting with whole-base, far-cap and positive-separation checks. First
  base-connected spherical and inclined caps preserve source-edge history;
  crossing next-face limits are rejected by exact containment checks. Viewer
  measurements use native centroid-ray surface witnesses. AI example resources
  include `curved-extrusions`; old distance and symmetric requests migrate.
  The 250 inclined/spherical regeneration gate passes in 6.612 s (10 s budget),
  with exact volume/ray checks and no retained handles.
- Extrusion end conditions (schema 73): symmetric full length, selected-face
  and next-face limits with exact bounded coverage. Planar parallel limits
  support oblique and negative travel, named selectors, incremental rebuilds,
  AI schema/example resources and actual viewer dimension anchors.
  The 1,000 three-extent regeneration gate passes in 6.454 s (10 s budget),
  checks exact volumes/validity and releases all handles.
- Sketch completion (ABI 47, schema 72): native ellipses, signed angle,
  radius/diameter, symmetry and point-on-line/circle/arc/ellipse/spline equations;
  saved exact trims, natural conic and C1 spline extensions, and planar offsets
  derive profiles without overwriting source identities. Sparse damped solves
  handle underconstrained curved targets; diagnostics use the same equations.
  AI schemas/examples and both viewers expose new dimensions and derived
  profiles. Six library tests, a command integration test, bridge tests and C
  argument conformance cover geometry, conflicts, migration and cleanup.
  Three 1,000-case scale gates pass in 0.063/0.115/0.088 s (10 s budgets).
  See [Sketches](SKETCHES.md).

- Assembly dimension/check overlays: native anchors follow the selected part's
  placement and frames, using an explicit family-local glTF matrix independent
  of render-mesh centering and shared variants. Labels share the WebGL camera,
  support clipping, visibility toggles, mouse/keyboard selection and inline
  parameter edits without leaving Assembly. Failed candidates switch to a
  marked diagnostic view. Three projection/interaction tests and the extended
  real-page logic test cover transforms, true values, scoped edits and rejection;
  native glTF tests compare transformed anchors against exact rotated/framed
  geometry. See [Model viewer](tools/view/README.md).

- Combined editable model and annotation viewer: the studio defaults to native
  dimension/sketch inspection, with an Assembly view for placed colored parts.
  Selecting a label opens linked parameter fields; edits regenerate geometry
  and checks while retaining the camera/selection. Failed candidates show a
  marked diagnostic preview without changing or saving accepted geometry.
  Shared native scene/renderer code serves both studio and offline artifacts;
  lazy, bounded, versioned requests prevent stale scope updates. Save waits
  for pending edits. Two native integration tests and seven JavaScript
  integration/concurrency tests cover regeneration, instance isolation,
  rejection, save/revert, unit preservation and racing replies. Browser UI
  automation was unavailable. See [Model viewer](tools/view/README.md).

- Annotated sketches and 3D shape viewer (no ABI/model-schema change):
  self-contained HTML orbit/pan/zoom views and SVG snapshots show measured versus
  driving dimensions, native sketch-constraint residuals/symbols, linked controls
  and failed requirement witnesses. Normal previews include the viewer; separate
  diagnostic visualization preserves failed checks without accepting a build.
  Three additional command tests and one MCP test cover solved/conflicting
  sketches, geometric failures, budgets, safe labels and acceptance separation;
  JavaScript logic tests and rendered SVG inspection pass. Interactive browser
  QA was unavailable. Release 1,000-constraint sketch visualization takes 0.119 s
  (10 s budget); 1,000 annotated solid scenes take 2.451 s (30 s budget), both
  included in `tools/bench/run.sh`. See [Annotated viewer](tools/model/VIEWER.md).

- Guarded AI feature edits and revision history (no ABI/model-schema change):
  typed feature additions/replacements/removals, new parameters/requirements/
  references and instance parameter edits use stable IDs and expected-feature
  guards; MCP also checks the accepted model's SHA-256. Existing requirements
  remain intact, changed family definitions increment their version, and verified
  immutable child builds append actual semantic revision records with readable
  `changes.json`. Stale guards, invalid models, required failures and no-ops
  preserve sources. Three additional command tests and one MCP test cover
  geometry, guard conflicts, additions/removal, requirement preservation,
  revision chains and cleanup. A 10,000-feature edit/verification/ledger/resource
  roundtrip takes 1.684 s (30 s budget), included in `tools/bench/run.sh`.
  See [Guarded edits](tools/model/MCP.md#guarded-edits-and-revision-history).

- Read-only AI model inspection (no ABI/model-schema change): CLI and MCP tools
  return paged declarations, inherited/derived parameter values, complete
  feature inputs including named references, requirements/datums/references and
  optional native face/edge measurements with actual semantic selector queries.
  Geometry queries use an additional family-local authoring snapshot, with
  assembly placement reported separately and indices marked snapshot-local.
  Successful inspection removes its scratch work and preserves accepted builds.
  Three additional command tests and one MCP test cover inventory/paging,
  inheritance, named inputs, local coordinates, query cleanup, errors and
  preservation. A 10,000-instance inventory takes 0.134 s; a 1,000-feature
  regenerated geometry inspection takes 0.110 s (10 s budgets), included in
  `tools/bench/run.sh`. See [AI inspection](tools/model/MCP.md#inspect-existing-parts-before-editing).

- AI MCP interface and generated authoring schemas (no ABI/model-schema change):
  six initial serde-derived schema targets cover all 28 feature operations; the local
  stdio server exposes schema/example/build tools and accepted artifact resources.
  Independent worker processes contain kernel logs and geometry lifetimes;
  timeouts, repeated cancellation and stdin closure clean unaccepted builds.
  Five Python wire/schema tests independently validate all schemas/examples and
  exercise bracket creation/edit/rejection/repair, artifact reads and errors.
  Release 1,000-call schema discovery takes 0.080 s; a 1,000-part single-variant
  build/report/accepted-model resource roundtrip takes 0.283 s (10 s budgets),
  included in `tools/bench/run.sh`. See [AI MCP interface](tools/model/MCP.md).

- AI model build command (no ABI/schema change): `occt-model` accepts versioned
  JSON models, selected outputs and typed parameter edits, preserves structured
  kernel diagnostics and accepted requirement evidence, and publishes editable
  models, STEP/STL and bounded isometric SVG previews into new directories.
  Three command tests cover bracket creation/edit/rejection/repair, re-import,
  source/accepted-output preservation, malformed inputs and migration. Four
  complete authoring examples pass; a 1,000-instance single-variant measurement,
  verification and persistence/report case takes 0.245 s (10 s budget), checked
  by `tools/bench/run.sh`. See [AI model command](tools/model/README.md).

- Saved cone and sphere features (schema 70; no ABI change): typed length
  expressions define cone/frustum radii and height, origin, sphere radius and
  center; cone axes are dimensionless and normalized without overflow/underflow.
  Either cone radius may be zero, and equal radii use a cylinder. Every input
  participates in incremental signatures; only changed branches and dependents
  rebuild. Four tests cover analytic volumes/centroids, transform history,
  unit conversion, apex/cylinder limits, small/large/distant geometry, extreme
  axes, repeated regeneration, failure rollback, persistence, migration and
  independent merges. Full core scale checks pass; 1,000-feature build/edit cases
  take 0.319 s for cones and 0.448 s for spheres (10 s budgets), with unaffected
  geometry reused and all handles released. Older feature operations are unchanged.
  See [Parametric architecture](PARAMETRIC_ARCHITECTURE.md#cone-and-sphere-features).

- Explicit material hatch families (schema 69): per-view material-ID maps select
  up to eight angle/spacing/phase line families for paired lines or crosshatching.
  Clone inheritance, fallback patterns, explicit suppression, same-material union,
  shared work/vertex limits, cached material lookup and reused cut faces work in
  both render modes. Three tests cover rendering, sections/details, edits,
  validation, migration, semantic merges and cleanup. A 1,000-part case with
  10,000 material mappings exports 12,500 hatch segments in 5.910 s against a
  30 s budget, with one generated variant and no retained handles. Older views
  keep empty maps and their existing shared pattern. Standards-verified material
  presets and automatic adjacent-component alternation remain future work.
  See [Drawings](DRAWINGS.md#material-hatch-families-schema-69).

- Exact section-hatch intersections (no ABI/schema change): `exact_curves`
  now trims bounded batches of hatch lines against native cut faces, preserving
  curved/spline boundaries and holes without boundary sampling. Material unions,
  disconnected islands, actual face planes, detail windows, native resolution,
  work/vertex budgets and cleanup are covered by three tests with analytic circle
  and parabola oracles. The 1,000-part/10,000-segment case takes 8.773 s; the
  1,000-cylinder/40,000-segment case with both exports takes 7.754 s (30 s budgets),
  with one shared variant and all handles released. Legacy setups stay sampled.
  See [Drawings](DRAWINGS.md#kernel-trimmed-hatching).

- Exact drawing geometry (ABI 46): optional `exact_curves` preserves lines,
  circles, ellipses, Bézier/B-spline curves, parabolas and hyperbolas, including
  exact detail trims, as native DXF geometry. SVG uses exact line/conic and
  quadratic/cubic paths where representable, and positive-weight control-hull
  subdivision with a paper-space tolerance elsewhere. Tests cover conic/spline
  correctness, periodic edges, crop-perimeter exclusion, far rotated placement,
  error/work/vertex bounds, legacy defaults, CLI counts and cleanup. Optimized
  benchmarks export 10,000 lines in 4.211 s (10 s budget), 26,000 Bézier spans
  across 1,000 spline views in 14.330 s and 8,000 spans across 1,000 cropped views
  in 15.364 s (30 s budgets), with one shared variant and no retained handles.
  Model schema stays 67. Native hatch intersections are described above;
  offset/other curves fail explicitly in exact mode.
  See [Drawings](DRAWINGS.md#exact-drawing-geometry-abi-46).

- Scriptable inspection reports: `occt-inspection-report` reads saved model drawings
  and dimensional/position measurement JSON, writes ordered typed evaluations and
  summary counts, and returns 2 for explicit limit/zone violations. Dimensions
  without acceptance limits retain distinct statuses. Invalid input creates no
  report; existing files and source inputs are protected. Position batches validate
  and index named/inline controls once. Five command tests cover mixed reports, measured points,
  rejection, output protection, named batches and bit-exact floating JSON round
  trips. A benchmark checks 100,000 position measurements across 10,000 controls
  in 0.055 s (10 s budget), without kernel handles. Groups may also carry raw `points`, evaluated by the
  measured GD&T inspection below and summarized as conforming, nonconforming
  or not evaluated. Composite freedoms, datum shift, uncertainty and
  standards-conformity certification remain future work.
  See [Drawings](DRAWINGS.md).

- Dimensional measurement checks: symmetric, signed-deviation and explicit-limit
  comparisons with unit-normalized nominal/measured values, signed deviations,
  margins and inclusive boundaries. Basic/reference/untoleranced dimensions
  return distinct dispositions without inferred acceptance limits. Saved batches
  resolve live projected lengths, radii, diameters, angles and hole-feature sizes;
  repeated measurements share cached nominal values. Four tests cover limits,
  units, rejection, paper independence, angular values and edited hole parameters.
  A benchmark checks 100,000 measurements across 10,000 saved dimensions in
  0.036 s (10 s budget), without kernel handles. Measurement acquisition, uncertainty/guard bands and geometric
  conformity remain future work. See [Drawings](DRAWINGS.md).

- Fixed cylindrical position sample checks: single-row diameter controls with
  three RFS datum references, arbitrary nominal axis direction and supplied
  axis points in an established reference frame. Results report controlled-feature
  bonus, required diameter, margin and first worst-sample index. Named references
  resolve through saved drawing validation. Four tests cover exact boundaries,
  units, offset/tilted axes, material conditions, invalid inputs and named frames;
  a 100,000-sample benchmark runs in 0.003 s (10 s budget), using constant
  extra storage and no kernel handles.
  This entry point takes an already-established frame; fitted datums and
  envelopes come from measured GD&T inspection. See [Drawings](DRAWINGS.md).

- Feature-size limits and bonus allowances (schema 68): persisted internal/external
  size limits, unit-aware MMC/LMC/RFS arithmetic, and independent total allowances
  for both position-composite rows. Out-of-limit sizes and invalid/overflowing
  values fail. Drawing exports preserve the specified control. Four tests cover
  material conditions, endpoints, units, rejection, persistence, migration and
  independent merges; a 10,000-allowance benchmark runs in 0.005 s
  (10 s budget) and uses no kernel handles.
  Mating envelopes fitted from measured points use these limits (see measured
  GD&T inspection below). See [Drawings](DRAWINGS.md).

- Composite controls and named datum frames (schema 63): two-segment position/
  profile frames with a shared characteristic cell, a tighter lower tolerance and
  an unchanged prefix of upper datum references. Named frames preserve ordered
  precedence and boundary conditions, work in controls and stable-ID merges,
  and resolve current world-space datum geometry in a batch. Nominal planar 3-2-1
  coordinates use three orthogonal model planes at RFS, with explicit rejection
  of partial/non-planar/skew/material-boundary cases. Four tests cover exports,
  exact budgets, validation, migration, merges, scaled and far rotated frames.
  Benchmarks cover 10,000 composites with both exports in 0.714 s and 10,000
  named frames with nominal coordinates in 0.015 s (10 s budgets). Fitted simulators, datum shift
  and actual tolerance-zone inspection remain future work. See [Drawings](DRAWINGS.md).

- Structured drawing GD&T intent (schema 62): datum-feature symbols and
  single-row feature-control frames for twelve form/profile/orientation/position/
  runout characteristics, dimensioned tolerance values, characteristic/diameter
  zones, explicit feature-of-size declarations, MMC/LMC tolerance modifiers and
  MMB/LMB datum references in primary/secondary/tertiary order. Live model anchors
  drive leaders; symbols use vector strokes, with a dedicated DXF GD_T layer.
  Validation rejects unsupported combinations, missing/duplicate references,
  rounded-zero values and exhausted export budgets. Four tests cover symbols,
  units, edits, paper sizing, migration and semantic merges. A 10,000-frame
  benchmark verifies generation, both exports and native cleanup in 0.504 s
  (10 s budget).
  These are persisted manufacturing declarations, not measured conformity results.
  Datum simulators, datum-shift calculations and tolerance-zone
  inspection remain future work. See [Drawings](DRAWINGS.md).

- Standard paper presets and projection symbols (schema 61): ANSI A–E and ISO
  A0–A4 in portrait/landscape, a bounded lower-right title block with drawing
  number, revision, scale, sheet numbering and metadata, and explicit first- or
  third-angle symbols. Presets override custom paper dimensions; legacy drawings
  retain their earlier frame. Three tests cover all 20 size/orientation pairs,
  symbol direction, paper-space sizing, exports, persistence, migration and budgets.
  A 1,000-sheet benchmark verifies shared regeneration and SVG/DXF exports in
  1.717 s (10 s budget). Views remain explicitly positioned; prescribed zones, approval and
  revision tables, lettering and a full standards-conformity audit remain future
  work. API and references: [Drawings](DRAWINGS.md).

- Automatic section hatching (schema 60): saved paper-space angle, spacing and
  phase for Slice/Section views; holes retain clear interiors, overlapping
  components share a material union, and disconnected cut regions remain separate.
  Per-solid cutting preserves overlapping components in section outlines too.
  Detail clipping, work/vertex budgets and legacy migration are covered by six
  tests. SVG uses thin hatch strokes; DXF uses a SECTION_HATCH layer. A benchmark
  checks 10,000 hatch segments across 1,000 placed parts in 8.406 s (30 s budget).
  Sampled mode retains its earlier boundary approximation; exact intersections
  and explicit material maps are described above. Standards-verified presets
  remain future work. See [Drawings](DRAWINGS.md).

- Datum-linked drawing guides (schema 59): center marks with fixed paper sizes,
  projected centerlines with paper extensions, and straight cutting-plane
  indicators linked to section views. Source endpoints must lie on the cut
  plane; the section looks normal to it and the cut is edge-on in the source
  view. Arrows follow viewing direction and captions identify the linked section.
  SVG and DXF distinguish thin center guides, thick cutting-plane lines and
  solid arrowheads; DXF adds CENTER/CUTTING_PLANE layers and a center linetype.
  Guides regenerate at current placements, persist, and merge by stable ID.
  Four tests cover edits, viewing direction, migration, merges, invalid guides,
  budgets, far rotated origins, scale and crop behavior; 10,000 mixed guides
  generate and export within a 10 s budget with one shared variant. This does
  not certify ASME line weights or layouts. See [Drawings](DRAWINGS.md).

- Manufacturing dimensions and tolerances (schema 58): radial, diametric and
  minor-angle dimensions join aligned/horizontal/vertical dimensions. Display
  units support mm, cm, m and inches; tolerance values carry units, with scalar
  radians for angles. Symmetric ±, signed deviations, explicit limits, boxed
  basic and parenthesized reference dimensions are persisted and exported as
  SVG/DXF annotations, including stacked deviations and limit values. Live Hole
  callouts resolve bore diameter, through/blind
  extent, counterbore/countersink and recorded thread intent from current
  parameter values. Invalid units, ranges, datums and references fail cleanly;
  radial/angular datums must lie in the view plane. Legacy drawings default to
  their existing untoleranced millimeter presentation. Six new tests cover
  exports, edits, migration, budgets and cleanup; benchmarks cover 10,000 mixed
  annotations and 10,000 live callouts. This implements dimension capabilities,
  not a clause-by-clause ASME conformity claim. See [Drawings](DRAWINGS.md).

- Sliding components in the motion-study command: bounded prismatic joints,
  normalized local axes, millimeter start/end offsets, reverse travel, and
  coordinated hinge/slider studies. Parent frames, source placements, material
  inheritance, collision exclusions and saved models are preserved. Seven new
  command tests cover exact placed bounds, reloads, continuous crossings between
  clear samples, invalid setup publication, and 10,000 mixed joints in 0.140 s (10 s budget).
  Existing hinge setups retain their frame IDs. The native Boolean cleanup
  feature is integrated at ABI 42; schema 57 unifies both branches' schema 56
  additions. See [Motion studies](tools/motion-study/README.md).


- Integrated assembly/motion and geometry features (ABI 42, schema 57): spline
  sketches, path sweeps, structured STEP assemblies, persistent/named references,
  measured tangency, slicing drawings, balance reports, joint solving and
  continuous collision checks are available in one branch. Schema 57 unifies the
  drawing additions with the geometry branch's schema 56; older documents migrate
  with additive defaults. The combined suite verifies both feature sets.


- Explicit collision pair exclusions for static generations, sampled motion,
  continuous translation/rotation, and solved linkage studies. Exact selected
  output pairs are validated before geometry work; reversed duplicates,
  self-pairs and unknown outputs reject. Excluded pairs spend no continuous
  candidate/query budget; other pairs and relationship checks stay active.
  The motion command accepts component-ID pairs and writes the exact exclusions
  into both study and report artifacts. Older study JSON checks all pairs.
  Tests cover retained third-party crossings, native cleanup, round trips,
  closed-linkage propagation and command publication. A 10,000-part/5,000-pair
  benchmark completes sampled plus continuous checks in 0.302 s (10 s budget).
  No ABI or model schema change. See [Assembly motion](ASSEMBLY_MOTION.md).


- Shared rigid carrier collision checks: group identical rigid motion paths,
  ignoring only constant inner mount placements, and use initial-position BVHs
  within each group. Inter-group pairs retain swept bounds. Candidate pairs in
  a shared group need at most one initial exact query, with numeric uncertainty
  still reported as unresolved. Regression tests cover fixed mounts, translation,
  unwrapped turns, distinct moving children, contact, interference and clearance.
  A 10,000-component carrier benchmark takes 0.156 s with one local variant and
  zero candidate pairs or exact queries (10 s budget). No ABI or model schema change.


- Tighter continuous rotating bounds: propagate conservative corner/arc boxes
  through rigid frame chains, retaining angular extrema and axial thickness,
  and intersect with the existing sphere enclosure. Separated subinterval boxes
  skip exact midpoint queries; reports expose `bounds_rejected_intervals`.
  Tests cover interior extrema, reverse/multiple turns, nested paths, thin plate
  stacks, axial crossings, overflow rejection, guards and handle cleanup. The
  10,000-plate stack clears in about 0.19 s with one variant and zero pair queries
  (10 s budget). The 1,000 obstacle-crossing oracle uses 6,720 queries versus the
  preceding 7,490. Correlated dense/nested mechanisms can still be unresolved.
  No ABI/model schema change. See [Assembly motion](ASSEMBLY_MOTION.md).


- Bounded linkage branch discovery: deterministic Cartesian starting poses,
  optional current seed, authoritative closure checks, periodic or unwrapped
  coordinate equivalence, and explicit search/result limits. The source graph
  stays unchanged; reports include failed/repeated starts and the best failed
  pose. The command exports numbered reloadable branch models and a report.
  Seven library tests and two command tests cover analytic alternative poses,
  singular four-bar recovery, physical limits, units, budgets and persistence.
  A 962-start benchmark discovers both analytic branches in about 0.07 s within
  a 10 s budget. This does not certify exhaustive global enumeration. No ABI
  or model schema change. See [Joint branches](tools/joint-branches/README.md).


- Larger sparse joint-coordinate solving: frame ancestry indexes affected
  relationships once, and finite-difference Jacobians reuse one private graph
  instead of cloning and evaluating the entire assembly per coordinate.
  Limits increase to 10,000 coordinates and 10,000 relationships, with explicit
  traversal/normal-matrix work bounds and a closed-motion report-size bound.
  Tests cover 1,000 mounted sliders, a coupled 64-frame chain, dense influence
  rejection, and oversized report rejection. The 10,000-coordinate benchmark
  takes 0.42 s and the coupled 64-frame chain takes about 0.93 s, each within a
  10 s budget. No ABI/schema change; local seeds
  still select assembly branches. See [Assembly motion](ASSEMBLY_MOTION.md).


- General drawing batches and cutting templates (integrated schema 56): true planar slices
  export only the cross-section boundaries, including holes, without projecting
  geometry behind the plane. The command writes numbered SVG and millimeter DXF,
  a manifest and persisted drawing definitions. Eight mirrored starter-wing
  station profiles exercise the same generic workflow. A 1,000-template scale
  case shares one variant and enforces a global vertex budget. Concurrent native
  projection now initializes OCCT's shared plane once; fresh-process regression
  tests cover simultaneous first sessions. See [Drawing export](tools/drawing-export/README.md).


- Mass and balance reports: selected outputs retain their current frame/joint
  poses and inherited or explicitly supplied densities. Reports include
  component and per-material mass/central inertia, assembly CG, and signed
  distance and percentage along a world-space chord. A symmetric wing reference
  integrates piecewise-linear stations exactly for MAC and its area-weighted
  leading edge. Data only, without a target CG recommendation. Library grouping
  reuses existing measurements; the command preserves inputs and rejects report
  overwrites. Five library tests and four command tests cover analytic planforms,
  units, far origins, materials, invalid selections and output behavior. Scale
  cases cover 100 MAC calculations over 10,000 stations and a 10,000-component
  report with one generated variant. No ABI/schema change. See
  [Mass and balance command](tools/balance-report/README.md).

- Runnable hinge assembly studies: `occt-motion-study` clones selected outputs
  into separate fixed or revolute components, retains source placements and
  enclosing frames, validates travel limits, and writes a reloadable assembly,
  coordinated study, and sampled plus continuous interference report. Wing
  examples exercise mirrored elevons through ±25 degrees with explicit assumed
  straight hinge lines. Seven command tests cover inter-sample interference,
  nested placement, coordinated travel, clearance, validation and persistence;
  10,000 hinges prepare in about 0.15 s within a 10 s budget using batch insertion.
  No ABI or model schema change. See [Motion-study command](tools/motion-study/README.md).

### Kernel (C ABI)

- STEP assembly trees (ABI 44): `occt_bridge_step_save_assembly_tree` writes
  named sub-assemblies from parent-indexed nodes with rigid 3x4 transforms
  (validated as proper rotations), locating each component relative to its
  node so model-space geometry matches the flat export; nodes must hold
  components; optional per-face colors index each component's faces and
  color its part. The flat export is the zero-node case.
  `occt_bridge_subshape_lookup` resolves many candidates in one traversal,
  reporting missing ones instead of failing. Shared-part occurrences
  at the identity location now keep their own names (OCCT's name writer had
  attached them to other occurrences). A C++ XCAF test checks a three-level
  tree's nesting, names, recomposed placements, and a face color, and
  argument errors.

- Exact pull ranges (ABI 43): `occt_bridge_shape_face_pull_ranges` reports
  each face's range of outward normal along a pull direction, with the points
  attaining it; exact on planes, cylinders, cones, and spheres or tori whose
  extremes lie on the face, otherwise left unmeasured. C tests check a box and
  argument errors; bridge tests check analytic values and witness points and
  that every exact range brackets and is reached by a fine tessellation's
  facets on fillets (tori), fused, drilled, and trimmed parts.

- Same-domain merging (ABI 42): `occt_bridge_unify_same_domain` merges
  adjacent faces on the same surface and edges on the same curve within
  linear and angular tolerances, recording OCCT's history so merged faces are
  modified and untouched ones keep their identity. C and bridge tests check a
  fused stadium (10 faces to 6, same volume, split top pieces leading to the
  merged top) that the offset can shell only after merging, plus argument
  errors.

- Measured face tangency (ABI 41):
  `occt_bridge_shape_faces_are_tangent_within` uses continuity recorded on a
  shared edge and, where none is recorded, samples both faces' normals along
  the edge against an angular tolerance in (0, pi/2). Shared edges are found
  through an edge map, O(edges of both faces). C, C error, and bridge tests
  check a block fused flush with a cylinder (no recorded tangency; six
  measured tangent pairs, two of them curved) and tolerance validation.

- Structured STEP export (ABI 40): `occt_bridge_step_save_assembly` writes one
  named assembly through OCCT XCAF, with a named component per placed shape and
  shared, named, sRGB-colored parts for shapes that share geometry. A C++ test
  reads it back through XCAF and checks structure, names, shared parts, and
  colors; argument errors write nothing.

- Sweeps along paths (ABI 39): `occt_bridge_sweep` carries a wire or
  single-boundary face along an edge or wire with corrected-Frenet, Frenet,
  binormal, or fixed orientation, mitering sharp corners, closing faces and
  closed wires into solids, and recording generated-face history. A guard
  rejects profiles reaching as far as the path's smallest bend radius, which
  the kernel's validity check misses. Three bridge tests check exact volumes on
  straight, curved (Pappus), fixed-orientation (sheared), and mitered paths.

- Curve wires and accurate freeform measurements (ABI 38):
  `occt_bridge_create_curve_wire` joins lines, arcs, and B-splines interpolated
  through any number of points, with optional end tangents and periodic
  (corner-free closed) splines. All measurements now share one integration
  policy: fixed order on all-analytic shapes (exact to roundoff), span-aware
  Gauss-Kronrod volume (1e-7 relative target) and adaptive area when any face
  or edge is freeform; blend volumes are faster than before.
  This fixes 0.03% volume and 0.003% area errors on prisms of spline-bounded
  sketches, now exact. Centers and inertia of freeform shapes use adaptive Gauss
  integration (about 1e-4 relative), because span-aware moments cost seconds
  per blend surface.

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

- Surface texture (schema 71; 69 and 70 are reserved for the material hatch
  families and cone/sphere features in progress elsewhere): drawing
  requirements with B46.1 roughness limits (Ra, Rq, Rz, Rmax in µm or µin),
  standard sampling lengths, waviness, lay, material-removal requirement,
  method note and all-around, drawn as Y14.36 symbols with exact vertex
  budgets; inspection records check readings by the maximum rule. Four tests
  cover rendering in SVG and DXF, validation, persistence and inspection.
  10,000 symbols generate with SVG and DXF exports in 0.102 s (1 s budget).
  See [Drawings](DRAWINGS.md#surface-texture-schema-71).

- Measured GD&T inspection (schema 68): `evaluate_inspection` checks an
  inspection record of measured points against a drawing. Planar datum
  simulators are fitted in precedence order (constrained L∞) into a measured
  3-2-1 frame. Flatness and planar parallelism/perpendicularity/angularity
  report minimum-zone widths at the basic orientation. Position of axis
  features normal to the primary datum uses the related mating envelope, with
  MMC/LMC bonus from new optional `size_limits`. Fits are minimax linear
  programs whose final widths are exact, so they never understate deviation.
  Unsupported controls are reported, not guessed. Seven tests cover tilt
  removal, datum contact, free rotation, bonus/size failure, far rotated
  placements, validation and persistence. 10,000 position controls plus a
  100,000-point surface (821,200 points) evaluate in 0.200 s (1 s budget).
  See [Drawings](DRAWINGS.md#measured-inspection-schema-68).

- Traceability (schema 67): family assumptions and requirement traces to
  features, parameters, and assumptions, validated and omitted when empty.
  Change impact lists the requirements to re-verify per instance: edited
  ones and those whose rule output or traced items changed, so restating an
  assumption flags only what rests on it. Two tests cover assumption,
  parameter, requirement, and addition edits, validation, and persistence.
  10,000 instances with 1,000 traced requirements report one restated
  assumption in 0.111 s (10 s budget).

- Undercut requirements (schema 66): `VerificationRule::Undercut` clips the
  output at a parting plane and requires faces above it to release along the
  pull and faces below against it, exact on analytic faces through the pull
  ranges and facet-sampled elsewhere; faces in the parting plane are
  skipped. For a planar parting and straight pull the per-face test is
  complete: trapping material is entered through a face turned against the
  pull on the same side. Two tests cover a split cube, a tee parted below and at its head,
  a rod parted at and above its axis (release -asin(0.4) measured exactly,
  witnessed at the plane), a sampled loft, and validation. Both halves of a
  400-hole plate are clipped and bounded in 0.90 s (3 s budget); the
  10,000-instance manufacturing case adds the rule at 0.50 s.

- Feature colors (schema 64): `FamilyDefinition::feature_colors` gives a
  feature a color for the faces it creates; later features carry colors to
  the faces they keep or modify, recomputed on every regeneration (also for
  reused outputs, so recoloring rebuilds no geometry), and `export_step`
  writes them as STEP face colors. Three tests follow a red block, a blue
  notch tool, and green fillets through a cut and a fillet, recolor without
  rebuilding, validate, persist, and export. Coloring a plate and 100
  sequential holes costs 1.00x of the uncolored regeneration (3.0 s; 15 s
  budget).

- STEP sub-assemblies: `InstanceGraph::export_step` writes every frame that
  holds an exported output as a named sub-assembly, nested as the frame tree
  is and placed by its frame placement and current joint motion. Tests check
  a turned wing frame with a hinged flap (model-space bounds and center of
  mass match the generation; frames without outputs are omitted) and
  placement-to-transform conversion. 10,000 members in 110 nested frames
  export in 0.702 s (3 s budget).

- Exact draft requirements: `DraftAngle` measures analytic faces exactly
  from the BREP and screens only the remaining faces on the tessellation, with
  `Exact` evidence when no face needed sampling. A post drafted 3 degrees
  measures 3 degrees to 1e-9 with a witness on the cone; a smooth loft falls
  back to sampling. Bounding all 410 faces of a 400-hole plate takes 0.011 s
  (2 s budget, 0.638 s with the booleans building it).

- `Unify` feature (schema 56): merges the faces a boolean left split, so fused
  shapes can be shelled and drafted. Two tests shell a fused stadium through
  a persistent reference to the bare block's top, followed through the split
  and the merge (exact tray volume; without `Unify` the shell fails), check
  that the tolerance parameter rebuilds only the unify and its users, and
  persist the feature. Unifying a 400-hole stadium plate takes 0.125 s
  (0.819 s with the booleans building it; 2 s budget).

- Measured tangency in selectors (schema 55): `FaceSelector::TangentTo` takes
  an optional `angular_tolerance`; when set, unrecorded junctions are measured,
  so tangent and coplanar faces that a boolean split apart can be selected.
  The tolerance's parameters join the consuming feature's signature, and an
  absent tolerance is omitted from documents. Three tests cover recorded-only
  failure on a fused stadium, the measured selection's exact area, tolerance
  errors, signatures, and persistence. Checking all 410 faces of a 400-hole
  stadium plate against its top takes 0.016 s (0.644 s with the booleans
  building it; 2 s budget).

- Named references (schema 54): `FamilyDefinition::references` declares a face
  or edge selector once under a name, and `FaceSelector::Named` and
  `EdgeSelector::Named` use it in any fillet, chamfer, hollow, or draft. A
  reference's features and parameters count as dependencies of each feature
  that uses it, so editing one rebuilds exactly its users. Validation rejects
  empty or repeated names, unknown names, a face reference used for edges (and
  the reverse), references that name other references, and unknown features;
  a reference to a downstream feature is reported as a cycle. Three tests
  cover two shells sharing one reference, equivalence with the inline
  selector, incremental rebuilds, every validation error, and persistence. 200
  fillets through named references among 10,000 declarations, regenerated
  twice, take 0.482 s (2 s budget); lookups are indexed by name.

- Native viewer export: `InstanceGraph::export_draw_view` writes a placed B-rep
  compound and a DRAW script that opens every part shaded, named after its
  instance, and colored from its material. Two tests check the script, the
  reloaded volume, name sanitizing, and an off-screen DRAW run. A 10,001-part
  view exports in 0.440 s (2 s budget).
  `occt-view MODEL.json` regenerates a saved model and starts DRAW on it
  (see [Model viewer](tools/view/README.md)), and
  `InstanceGraph::set_material_appearance` sets the persisted material colors
  it uses. `--watch` regenerates after each settled save and has the open
  viewer `source reload.tcl`, which swaps parts and keeps the camera; invalid
  saves keep the previous view. Five command tests cover inherited colors,
  output selection, detached and missing viewers, a watch cycle with a broken
  save, and invalid arguments; the DRAW test also runs the reload script.
  Each poll is O(1) and each reload one full regeneration and export.
  `occt-view --serve` serves a dependency-free WebGL page on 127.0.0.1 that
  renders the model's glTF and edits family parameter defaults: edits merge,
  apply atomically or are rejected with the model's message, and are written
  only by Save (temporary file and rename); saves made elsewhere reload the
  page. Hosts other than 127.0.0.1/localhost and unmarked POSTs are refused.
  `InstanceGraph::export_gltf_output` exports one named output of every
  instance. Two server tests, one library test and four Node renderer tests
  cover rendering math, edits, saves, followed and broken files, and request
  checks; the page was also exercised in Chrome. glTF export now tessellates
  once per shared variant and places the other instances by node transforms
  (matrices when rotated, frames and joint motion included): a 10,000-part
  viewer export takes 0.253 s (2 s budget), and the 1,000-part glTF case fell
  from 1.354 s to 0.009 s. A test checks rotated, framed, far-placed clones
  against regenerated bounds. The page also edits one instance at a time:
  click a part (ray picking through node transforms) or choose it in a list
  to see its effective values tagged own, inherited or default, set its own
  overrides, or reset them; the other parts fade while it is selected.
  Its placement (translation, axis, angle in degrees, origin) is shown and
  edited too; a pattern member's placement edit is refused because its rule
  re-derives it. `Quantity::normalized` is now public. **Add copy** clones
  the selected instance (inheriting it, a part-width along X), **Delete**
  removes one unless clones or a pattern depend on it (the message names
  them), and **Revert** reloads the file to undo unsaved changes. The panel
  lists requirement results from the same regeneration (per variant, with
  assembly checks and their witness points marked in 3D);
  `InstanceGraph::export_gltf_generated` builds glTF from an existing
  regeneration so geometry and results are generated once.

- Persistent references (schema 53): `FaceSelector::Persistent` and
  `EdgeSelector::Persistent` choose topology on an earlier feature's output and
  follow it forward through every later feature by operation history; split
  faces yield every piece and removed ones fail, naming the feature. Three
  tests cover a front face followed through a quarter turn and a splitting
  notch (where a plain normal rule picks other faces), a width edit, removal
  and unrelated-reference errors, a hollow consuming the reference, and
  persistence. Four top edges followed through 100 sequential holes to a
  chamfer take 3.082 s (10 s budget), most of it the holes.

- Graph STEP export: `InstanceGraph::export_step` writes generated outputs as an
  assembly of instance-named components sharing one part per geometry variant,
  colored from material appearances (linear RGB converted to sRGB). Frames are
  flattened into component placements. Two tests cover shared variants, volume
  after reloading, misspelled outputs, and color conversion. 10,001 pattern
  instances export as one shared part in 0.828 s (4 s budget), at 617
  bytes per component.

- Sweep features (schema 52): `Sweep { profile, path, orientation }` sweeps a
  sketch face or wire along an open sketch path, with a parameter-driven
  binormal. Two tests cover exact volumes, parameter edits that rebuild only
  the profile and sweep, rejected self-intersecting edits that keep the
  accepted result, validation, and persistence. 1,000 sweeps rebuilt after a
  radius edit take 0.974 s (4 s budget).

- Spline sketch entities (schema 51): `SketchSpline` interpolates through named
  sketch points, joins lines and arcs in profiles, closes into a smooth loop
  when it repeats its first point, and takes end directions from `Tangent`
  constraints with lines or arcs. Three sketch tests cover tangent arches,
  solving, periodic loops, extrusion, validation, and persistence; two bridge
  tests cover curve wires. 1,000 spline-arch sketches extruded and rebuilt
  after a crown edit take 1.671 s (6 s budget); one spline through 1,000
  points solves into a face within 1e-6 of the circle's area in 0.015 s
  (250 ms budget).

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
  a view of a 1,000-part assembly takes 0.903 s (20 s budget). Certified curve approximation remains a future extension.
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
  persistence, nullity, and atomic failures. Sparse scaling now supports 10,000
  coordinates and relationships with explicit work bounds (see above), and
  up to 1,000 local iterations. Bounded alternative-pose search is available;
  complete enumeration and broader connected assembly scaling remain open. The 1,000-pose analytic
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
- Constraint-solved 2D sketches: lines, exact arcs/circles, splines (schema 51), construction
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
file. The project is a code-first engine: every item below is defined in
documents and the API, and verified in tests. The local viewer
(`occt-view --serve`) edits those documents but adds no capability of its own.

### Capabilities

The earlier geometry and assembly milestones are complete. Manufacturing
product definition is the next general CAD priority; additional rib variants
remain lower priority. The ASME comparison below is based on public standard
scopes and inspected repo capabilities, not a full conformity audit.

1. **Drawing conventions and manufacturing sheets** — [Y14 family](https://www.asme.org/codes-standards/y14-standards):
   standards-verified material hatch presets and sheet formats,
   and standards-verified dimension
   placement and typography.
   Standard drawing curves and detail trims now support exact DXF export, with
   bounded SVG approximation for curves SVG cannot represent. Direct deviations
   and limits render stacked values.
2. **Structured GD&T** — [Y14.5](https://www.asme.org/codes-standards/find-codes-standards/y14-5-dimensioning-tolerancing):
   datum features of size and datum shift, non-planar or non-orthogonal
   datum simulators, pattern and composite evaluation, profile against
   nominal surfaces, cylindricity and runout, common datums and advanced
   modifiers, and measurement uncertainty. Measured planar datum frames,
   flatness, planar orientation zones and position with MMC/LMC bonus are
   evaluated (schema 68).
3. **3D manufacturing annotations and exchange** — [Y14.41](https://www.asme.org/codes-standards/find-codes-standards/y14-41-digital-product-definition-data-practices):
   structured PMI attached to persistent geometry, annotation views, dataset
   authority and revision rules, and verified semantic PMI exchange. Geometry
   and assembly STEP export alone does not preserve this product definition.
4. **Standards-backed thread specifications** — [B1.1](https://www.asme.org/codes-standards/find-codes-standards/b1-1-unified-inch-screw-threads-un-unr-thread-form):
   thread limits/classes, fit validation and standard representations. Live drawing
   callouts now show recorded intent; nominal drill catalogs and thread strings
   still do not verify compliance or engagement/strength.
5. **Manufacturing surface texture** — [B46.1](https://www.asme.org/codes-standards/about-standards/technology-highlights/advanced-manufacturing):
   requirements attached to persistent faces rather than leader anchors, the
   16% rule and filtering for measured readings, and Y14.36 symbol placement
   checked against the standard. Drawing requirements, symbols and maximum-rule
   reading checks exist (schema 71). Rendering-material roughness is
   independent of these requirements.
6. **Controlled drawing release and assembly lists** — [Y14.100](https://www.asme.org/codes-standards/find-codes-standards/engineering-drawing-practices):
   structured approval/release records, drawing revision tables, parts lists,
   assembly balloons, and links between released drawing/model revisions.

## Later

- Investigate intermittent native STEP transfer/healing in
  `invalid_results_are_rejected_healed_or_allowed_by_option`: one coverage run
  rejected the bowtie STEP fixture with unorientable/self-intersecting diagnostics;
  the full rerun passed. No native code changed in the GD&T increment.

- General sheet-metal edge flanges, bend reliefs, hems, cutouts, bend tables,
  and unfolding edited solids beyond constant-width strips.
- Hole-catalog tolerance classes and optional under-head countersink relief.
- Wing structure beyond solid printed segments: hollow shells with internal
  ribs, alignment pins independent of the spar, twist-exact hinge lines, and
  print-bed fit in arbitrary orientations.
- Material property sheets with sources (stiffness, softening temperature),
  service-temperature and manufacturing-step temperature checks, and a beam
  theory spar bending check; stress analysis still needs an external solver.

- Advanced ribs with general support-following and nonuniform closure.
- Complete linkage branch enumeration, broader connected assembly solving, and
  bounds that preserve motion correlations in dense or deeply nested mechanisms.
- Exact (not sampled) minimum wall thickness, exact draft on freeform BREP
  faces, and non-planar parting surfaces.
- Domain-specific expression functions (airfoil sections, material and
  catalog lookups).
- Integration with the broader EIL source model in the sibling
  [`engineering-intent-language`](../engineering-intent-language) project.

## Keeping this current

When a feature lands, confirm it meets the scaling requirement, move it from
**Next** to **Done**, refresh the status tables (ABI and schema versions,
test counts, coverage, benchmark results), and adjust the
"next work" paragraph in [Parametric architecture](PARAMETRIC_ARCHITECTURE.md)
to match.
