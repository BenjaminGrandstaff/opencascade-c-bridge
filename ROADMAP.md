# Roadmap

Where the project stands and what comes next. The design rationale behind each
item lives in [Parametric architecture](PARAMETRIC_ARCHITECTURE.md); this file
tracks status and order.

## Current status

| Layer | Version | State |
|---|---|---|
| C ABI (`src/`, `include/`) | ABI 85 | Stable; exact version match required |
| `occt-types` (shared value types) | — | No native dependency |
| `occt-bridge` (safe Rust wrapper) | — | Covers the full ABI; re-exports `occt-types` |
| `occt-recipes` (application constructors) | — | Stone and wall torch |
| `occt-parametric-core` (kernel-free engineering layer) | Schema 181 | Definitions, documents, solving and checks without OCCT |
| `occt-parametric` (kernel regeneration) | Schema 181 | Active development; re-exports the core API |

| Quality gate | Result | Command |
|---|---|---|
| Tests | C 7/7, bridge 176 + first-use integration 1 (+1 doc test), recipes 3, parametric core 61 + kernel 800 + merge driver 3 + motion command 16 + balance command 4 + drawing command 4 + inspection command 9 + view command 100 + viewer Node 473 + branch command 2 + model command 102, MCP Python 27, mesh Python 4, wing model 6 + CAD 1 | `ctest`, `cargo test` (see README) |
| SonarQube (indexed Rust) | Gate OK, 0 issues, 93.9% line coverage (2026-10-04); Rust unit tests classified as tests | `tools/sonar/run.sh` |
| clang-tidy, cppcheck, clang `-Werror` | Compiler build passes; full lint flags existing sketch/extrusion complexity and sketch C/header parameter-name mismatches. Profile-loft and updated history/inspection and mirror code pass targeted lint; the updated transform and curve-intersection code also pass. | `tools/cpp-lint/run.sh` |
| Rust formatting and Clippy | Clean across all three crates, including all targets | `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` |
| Coverage | Last recorded: 93.61% lines overall, test code excluded; C++ 94.13% lines, 87.40% branches, 100% functions; Rust 93.42% lines | `tools/coverage/run.sh` |
| Scale benchmarks | Last full suite: 194 Rust cases (199 with the parallel glTF, 1,000-part view, advanced sweep, tray and bend-cutout cases) plus 7 model-command cases (including 256 renders in one build) and a 10,000-face Python matcher. Sketch authoring integration rechecked 60 sketch cases and 60 MCP cases within budget. | `tools/bench/run.sh` |

## Done

- Design templates (no schema or ABI change). `occb-model-template-v1`
  files in `tools/model/templates/` are complete build requests whose values
  come from typed, range-checked inputs (lengths, numbers, choices, allowed
  catalog sizes). `$input`, `$product` and `$sum` placeholders fill
  parameters, materials, loads and derived moments. There are four templates:
  `l-bracket`, `printed-enclosure`, `shaft` and `sheet-metal-channel`. Each
  carries its own checks (valid, exact wall, mass, bending, bed or stock fit,
  heat margin). MCP adds `occt_list_templates` and `occt_instantiate_template`,
  and `templates.py` does the same from a shell. Tests build every template
  at its defaults with all checks passing, show inputs changing the
  verdict, and refuse bad inputs. See [Templates](docs/TEMPLATES.md).

- Exact minimum wall and refined freeform draft (schema 181, ABI 85).
  - `exact_minimum_wall` measures the thinnest wall between faces that share
    no edge and face each other through material. It uses the kernel's exact
    face extrema on box-swept candidate pairs, and a ray confirms the segment
    stays in material. It matches plate, ligament, tube and cup walls to 1e-7,
    and does not count gaps between bodies. A 400-hole plate took 1.2 s.
  - Per-face pull ranges no longer leave freeform faces to facets. B-splines,
    surfaces of extrusion and revolution, and trimmed spheres and tori are
    refined on the surface: an on-face lattice plus boundary curves, then a
    compass search, reported as `exact = 2` / `refined`. Draft and undercut
    checks use them and say how many faces were refined. An elliptical prism
    matches ±sin of the pull's tilt to 1e-9. 500 freeform faces took 2.3 s.
  - See [Requirement rules](docs/REQUIREMENTS.md#exact-minimum-wall).

- Metric standard parts (schema 180, no ABI change). The frozen
  `occb_standard_parts_v1` catalog holds ISO 4762 socket head cap screws,
  ISO 4032 nuts and ISO 7089 washers (M3–M20), and ISO 8734 dowel pins
  (Ø1–20), from fasten.it's tables of the cited DIN EN ISO editions. Two
  inconsistent source entries are excluded, and the tables are tested to grow with
  size. A `standard_part` feature places one at its seat along any axis.
  `standard_part_dimension_v1` reads head, socket, nut, washer and chamfer
  dimensions in expressions. Volumes match closed forms to 1e-9. Example
  [`bolted-joint`](tools/model/bolted-joint.request.json); see
  [Standard parts](docs/STANDARD_PARTS.md).

- Material catalog and material requirements (schema 179, no ABI change).
  The frozen `occb_materials_v1` catalog holds 13 materials. Eleven come from Proto Labs
  2026 CNC data sheets and two (PLA, PETG, as horizontal prints) from
  Prusament data sheets, kept in printed units with conservative ends of
  ranges. Missing values are errors, never guesses. The
  `material_property_v1` expression reads density, modulus, yield,
  ultimate, heat-deflection, service and layer-bond values. The new requirements
  are `mass_range` (exact), `temperature_limit` (exact) and
  `bending_stress`. The bending check takes section I from the kernel's
  exact inertia of a thin slab and samples the extreme fiber. Tests match
  bh³/12, πd⁴/64 and hollow tubes to 1e-6. Example
  [`printed-spar`](tools/model/printed-spar.request.json). See
  [Materials](docs/MATERIALS.md).

- Shared studio sessions for an AI client and a person (no schema or ABI
  change). `occt-view --serve` gains `GET /api/session`, which returns the
  in-memory model with its SHA-256, and `POST /api/session/apply`, which
  replaces the model only while that hash is current (409 otherwise), after
  validating and regenerating it. MCP adds `occt_session_read` and
  `occt_session_apply`. Apply takes only accepted builds and only loopback
  studio URLs. Applied changes join the studio's undo history, stay unsaved,
  and both studio pages show the author and message. Tests cover the
  guarded apply, conflicts from either side, undo, validation and limits in
  the server, the notice in the pages, and a full MCP-to-studio round trip
  against a running `occt-view`. A 1,000-instance model read in 0.011 s and
  applied in 0.033 s. See [shared studio sessions](tools/model/MCP.md#shared-studio-sessions).

- Complete failure reports for rejected builds (no schema or ABI change).
  `regenerate_all_reporting` regenerates every instance and keeps failed
  required requirements in the results instead of stopping at the first. A
  build or edit it rejects still publishes nothing, but its failure now
  carries `required_failures`, the full verification table (measured values,
  limits, evidence, witnesses; failures first), counts, and each output's
  validity, volume and bounds. Stage names stay unchanged for existing
  clients. Rows and outputs are capped at 500 each; 600 failing instances
  reported in under 700 KB. See [build contract](tools/model/README.md#request-and-report-contract).

- Rendered views and sections for AI review (no schema or ABI change).
  Build, edit and edit-preview requests take an optional `renders` field
  that writes shaded PNG images of each selected output. The images cover
  standard views (iso, front, back, top, bottom, left, right) and section
  planes that hatch the cut face. MCP returns up to eight of them as image
  content and keeps all of them as `image/png` resources. A built-in z-buffer
  rasterizer outlines B-rep face boundaries and silhouettes, and a built-in
  PNG encoder writes the images, so no dependency is added. Limits are 8
  views and 4 sections per output, 64 to 1,024 px, and 256 images per build;
  256 images took 3.95 s and 55 MB. Tests decode the PNGs and check framing,
  hatched cut faces, empty sections, determinism, limits and the MCP inline
  budget. See [rendered views](tools/model/README.md#rendered-views-and-sections).

- Studio sheet-metal editor (no schema or ABI change). Sketch to solid
  builds and edits sheet-metal parts with placement, width and thickness,
  flanges and bends, hems of all four kinds, edge flanges with reliefs,
  cutouts, and a linked flat blank with its K factor. Every value can be
  entered, parameter-linked or kept as a formula. Edits replace the part
  and its blank together and keep its bend table. A browser-built
  relieved, hemmed channel previewed at its analytic volume. See
  [sheet-metal parts](docs/SKETCH_STUDIO.md#sheet-metal-parts).

- Studio thread authoring on holes (no schema or ABI change). Holes take a
  thread from the frozen Carr Lane V1 tap catalog: 28 metric and 36
  Unified inch sizes, served with the studio's feature data. Inch sizes
  get their B1.1 series, standard designation and 1B/2B class. Threads
  can be left-handed. The bore can follow the catalog tap-drill expression,
  and the thread form can be cut in the ISO 68-1 basic profile. Saved
  catalog threads reopen in the controls; others are kept unchanged.
  Tests cover B1.1 limit checks with exact bore volumes, and a browser
  check previewed a modeled M6x1 hole.

- Studio shells, drafts, mirrors and patterns (no schema or ABI change).
  The solid editor creates and edits the engine's hollow, draft, mirror,
  linear and circular pattern features on accepted solids. Shell and draft
  faces are picked in the canvas. Mirrors and patterns can join their
  source. Unchanged saved values, formulas and face selectors are kept.
  Exact-volume previews cover each operation, and a browser check opened a
  block's top face into a 7,152 mm³ shell and edited it. Opening the solid
  panel on a model without sketches no longer fails. See
  [shells, drafts, mirrors and patterns](docs/SKETCH_STUDIO.md#shells-drafts-mirrors-and-patterns).

- Sheet-metal cutouts across bends (schema 178, ABI 84). A cutout may run
  from its flange through bends into further flanges, measured along the
  developed midplane. In the folded sheet its pieces on flanges are cut
  square to them, and its pieces in bends radially, by a new native radial
  cutter (`occt_bridge_create_radial_cutter`: the outline laid on the bend's
  cylinder with exact ellipse pcurves and radial ruled walls). On the blank
  each bend's piece is stretched by allowance over arc, as exact ellipse
  arcs. Every crossing cutout removes exactly its area times the thickness,
  and blanks match the predicted stretch at K = 0.3. 90 crossing holes take
  5 s (about 55 ms each, in the boolean). See
  [cutouts across bends](docs/SHEET_METAL.md#across-bends-schema-178-abi-84).

- Closed and teardrop sheet-metal hems (schema 177, no ABI change). A
  closed hem folds flat onto its flange; a teardrop rolls past half a turn
  and lands on its flange around a void that runs through the width. Both
  cap the strip's outline (the void as an inner loop) and unfold exactly:
  folded and flat volumes agree at K = 0.5, at either end and on either
  side, and with edge flanges and reliefs. Each costs 1–2 ms per sheet,
  like an open hem. See [hems](docs/SHEET_METAL.md#closed-and-teardrop-hems-schema-177).

- Hollow loft station diagnostics (schema 176 / ABI 83 unchanged): source
  diagnosis now shares construction's exact planar-region validation, checking
  coplanarity, strict outer-boundary clearance and disjoint/non-nested holes.
  Per-station records retain outer/hole IDs and native remaining material area;
  failed stations return `station_region` issues while other stations remain
  inspectable. The editor displays areas/errors and AI inspection pages
  `loft.stations`. Scratch regions are released, accepted geometry/history/disk
  remain unchanged, and invalid track counts skip region indexing. Tests cover
  outside, crossing, touching and wrong-plane holes, overlapping/nested/touching
  tracks, and valid stations whose 3D tracks still cross: profile diagnosis
  never substitutes for full solid validation. Kernel 784, model 95, viewer 95
  and Node 462 pass. The 500-hollow diagnosis gate takes 1.06 s, and two
  100-hole stations take 0.12 s, each against a 10 s budget with no retained
  handles. See [station regions](docs/LOFTS.md#hollow-station-material-checks).


- Sheet-metal edge flanges and bend reliefs (schema 176, ABI 83). Flanges
  fold up from either width edge of any strip flange, over any stretch of
  its straight region, at a signed angle and inside radius, with a leg.
  Where they reach a bend, rectangular reliefs notch the whole bend zone.
  The blank becomes a polygon with tabs and notches, following K or a bend
  table. The folded sheet is built from width slabs and edge-flange strips
  joined by a new glued fuse (`occt_bridge_fuse_glued`: OCCT glue mode
  plus face merging). Folded and flat volumes agree exactly at K = 0.5. 500
  trays with linked blanks build or rebuild in about 10 s each way. See
  [edge flanges](docs/SHEET_METAL.md#edge-flanges-and-bend-reliefs-schema-176-abi-83).

- Advanced sweeps (schema 175, ABI 82). Sweeps now take these options:
  - a twist about the path, linear or smooth;
  - a scale law;
  - a guide rail that turns the profile, and optionally resizes it about
    the path;
  - further sections blended along the path.

  `path_plane` datums place profile and section sketches normal to the
  path, so they follow its constraint and parameter edits. Holed profiles
  twist and scale together. Exact-volume tests cover each option, and a
  constrained duct is re-dimensioned through its route. 250 twisted,
  sectioned ducts build or rebuild in about 3 s each way.

  In the studio, the sweep form edits twist, scale, guide and sections, and
  editing keeps every saved option and formula. The plane tool adds *Normal
  to path*. Browser checks covered the duct example: editor read-back, a
  180° twist previewed at its exact volume, and a path plane resolved onto
  the bend. See [advanced sweeps](docs/ADVANCED_SWEEPS.md).

- Size dimensions drawn in 3D (schema 174, ABI 81). Annotation views list
  the size dimensions they show; each is drawn above its frame or datum
  feature symbol as one-line limits (`2X Ø9.9-10.1`, `59.9-60.1`), a
  diameter or width as its faces give, written as the presentation of the
  AP242 size dimension and verified on read-back with the view's callouts.
  400 drawn sizes add about 0.15 s to the 400-hole plate's export. See
  [annotation views](docs/DRAWINGS.md).

- Per-view part visibility in annotation views (schema 173, ABI 80). A
  saved view may list the instance outputs it shows; each is written as a
  mapped item of its part's shape representation in the view's draughting
  model, and the read-back checks that OCCT's reader shows the same
  distinct parts, by volume and centroid. A view showing 500 of 1,000
  distinct parts adds about 0.1 s to a 3 s export and read-back. See
  [annotation views](docs/DRAWINGS.md).

- Read-only loft profile diagnosis (schema 172 / ABI 79 unchanged): the editor's
  **Check loft profiles** and AI inspection's `loft_feature` share native exact
  alignment, source/aligned edge counts, start coordinates, station/hole IDs and
  stable issue codes. Source dependencies build in isolation even when a loft
  fails; invalid anchors, split formulas, track/mode settings and end directions
  remain identifiable without constructing the solid. Reports are bounded to
  8,192 profiles before source generation; AI results are paged and stale editor
  responses are discarded. Passing profile checks never replaces final solid,
  containment, fold-back or all-instance requirement validation. Kernel 773,
  model 94, viewer 93, Node 456 and MCP 13 tests pass, with browser correction,
  Preview/Apply, history and Save checks. The 1,000 isolated failed-loft diagnosis
  gate takes 0.70 seconds / 10-second budget with zero retained handles. See
  [profile diagnosis](docs/LOFTS.md#read-only-profile-diagnosis).

- Loft canvas boundary splits (schema 172 / ABI 79 unchanged): **Pick … split
  in canvas** snaps new points onto exact native trimmed curves, including
  multi-edge profiles with saved draft partitions. Existing corners/splits,
  ambiguous hits and edge-budget overflow fail before a point is returned.
  The editor appends fixed family coordinates while retaining existing formulas;
  all-instance Preview gates Apply. Purpose/version/profile/partition guards
  discard stale replies, and Escape, fields, Preview and Close cancel picks.
  Native snapshots are bounded, with no hover requests. Viewer Rust 92 and
  viewer Node 455 pass, including immutable native queries and editor lifecycle;
  the sketch performance gate initially passes (offset guide 49.9 ms / 100 ms),
  with subsequent unchanged offset runs varying to 103–128 ms on the shared host.
  Thresholds remain unchanged. The maximum 128-edge picker gate
  measures 0.76 ms per hover / 1.8 ms per redraw (16/100 ms budgets), with one
  snapshot request. Browser Apply/history/Save checks pass. See [canvas splits](docs/LOFTS.md#pick-new-boundary-splits-in-the-studio).

- Explicit loft boundary partitions (schema 172, ABI 79): saved parameter-aware
  split points refine exact copied curves before start/reversal alignment,
  allowing different source edge counts to share ordered constrained topology.
  Original edge ancestry composes through every generated fragment and bore;
  original sketches remain unchanged. Native point/edge budgets, unique boundary
  validation and all existing end-derivative/fold-back checks remain enforced.
  The editor preserves split formulas and profile IDs, supports add/remove and
  using a split expression as the start, and includes draft vertices in canvas
  picks. All-instance Preview, history and Save retain these settings. The new
  circle-to-diamond example is available through MCP with the advanced loft
  examples. Existing reversed wire traversal is preserved during native copies.
  The 1,000-regeneration partitioned-circle benchmark includes native seam/end
  inspection, exact volume and released handles: 8.98 seconds / 10-second budget.

- Loft canvas starts and traversal markers (schema 171, ABI 78 unchanged):
  native read-only seam inspection exposes exact aligned starts and short
  boundary traversal samples for every outer/hole station. The studio shows
  labelled rings/arrows for accepted or verified direct constrained lofts and
  clears them with stale previews. Canvas picks snap existing corners or
  supported full periodic boundaries through a bounded native snapshot/query.
  Confirmed picks author fixed family coordinates; cancellation preserves
  formulas. Preview, Escape, fields, model/instance changes and Close cancel
  pending picks, with late-response guards and all-variant Apply validation.
  Native inspection is included in the 1,000-regeneration performance checks
  (aligned lofts: 3.93 seconds against the existing 10-second budget).

- Loft profile correspondence (schema 171, ABI 78): exact start anchors and
  traversal reversal for outer and hole station profiles. Saved settings use
  profile IDs and parameter-aware length vectors; station reordering retains
  them and removal prunes them. Native alignment copies geometry, retains exact
  periodic curves, and composes original edge ancestry through lofts and bores.
  Unknown/stale anchors and free/ruled usage fail. The editor preserves formulas,
  validates every variant before Apply, and supports history and Save. The new
  aligned-profile example and 1,000-regeneration benchmark cover this path
  (3.11 seconds against the existing 10-second budget).
  Existing vertex starts and full single-edge periodic seams are supported;
  explicit boundary splitting is now available in schema 172.

- Loft end visualization (schema 170, ABI 77 unchanged): native annotations
  expose resolved end directions, full strengths and true one-third-strength
  control handles through a shared read-only regeneration query. The studio
  shows blue start/purple end arrows and labels for accepted direct lofts and
  verified previews. Fields, rejected previews, model changes and Close clear
  stale guides; all overlays ignore pointer events. Explicit and profile-normal
  directions use the construction resolver, with no guessed parameter values.
  Kernel 763, model command 93, view command 88 and viewer Node 435 pass;
  all-target Clippy and real browser checks pass. Benchmarks include resolved
  inspection without retained native handles: 1,000 ordinary/explicit/normal
  lofts take 1.27/2.62/2.85 s against separate 10 s budgets. Sketch budgets pass.
  See [loft guides](docs/LOFTS.md#tangent-guides-and-inspection).

- Profile-normal loft ends (schema 170, ABI 77 unchanged): start/end constraints
  can derive forward tangent directions from their terminal section planes.
  Native normals follow parameter-driven profile-frame edits and reversed
  station order; ambiguous forward normals and conflicting direction modes fail
  before Apply. The studio offers Normal to profile alongside explicit tangent
  directions and Free, retaining strength formulas and entered direction drafts.
  Saved constraints keep the derived mode rather than baking in a vector.
  All examples use schema 170. Core 57, kernel 763, view 88, model 92 and viewer
  Node 431 pass; Clippy and browser checks pass. Tests include tilted/reversed
  profiles, incremental plane edits, positive/negative station variants,
  rejected mode combinations, model history and Save. 1,000 normal-profile lofts
  take 2.69 s / 10 s; explicit constraints 2.51 s and ordinary lofts 1.29 s.
  Sketch performance budgets pass. See [profile normals](docs/LOFTS.md#normal-to-profile-schema-170).

- Constrained profile lofts (schema 169, ABI 77): optional start/end tangent
  directions and parameter-linked strengths create exact rational cubic Hermite
  spans through every section and hole track. End derivatives are verified on
  generated surfaces; independent strength changes rebuild only the loft.
  Existing unconstrained fitting remains unchanged. Constrained mode requires
  smooth surfaces, matching ordered edge topology, 2–64 sections, <=128 edges
  and <=100,000 compatible section poles. Parallel directions, nonpositive
  strengths, longitudinal fold-back and invalid solids are rejected. The studio
  edits both ends while preserving formulas. All examples use schema 169.
  C 7/7, bridge 163 (+ integration/doc), core 57, kernel 761, view 87, model 92
  and viewer Node 429 pass; Clippy and browser checks pass. Analytic volumes,
  cross-sections, multiple stations, hollow history, incremental edits,
  invalid-variant rejection and Save/history are covered. 1,000 constrained
  regenerations take 2.50 s / 10 s; existing lofts 1.33 s / 10 s. Sketch budgets
  pass. See [loft constraints](docs/LOFTS.md#end-constraints-schema-169-abi-77).

- Studio canvas hole placement (schema 168, ABI 76 unchanged): click an actual
  entry point on a planar source face and use the inward native face normal as
  the drilling axis. A blue entry marker/direction line accompanies the source
  overlay. Placement saves fixed family coordinates, explicitly replacing
  position/axis formulas while retaining the other hole settings. It does not
  create an associative face attachment; moving the face requires re-picking.
  Escape retains the form; stale field/model/instance replies are discarded.
  All 428 viewer Node tests and 85 view-command tests pass, covering exact
  nearest ray hits, unit conversion, invalid normals/planes, stale picks and
  inward top/bottom/side normals with validated hole volumes and saved graphs.
  Browser checks cover real off-center and side-face placement, cancellation,
  marker/preview, editing, Apply, Undo/Redo and Save. Clippy and sketch
  performance budgets pass.

- Studio hole finishes (schema 168, ABI 76 unchanged): author and edit
  counterbores, countersinks, optional 60-degree counterbore floor relief and
  blind-hole drill points. Lengths link length parameters; included angles use
  entered degrees or scalar-radian parameters. Existing formulas and thread
  metadata survive editing; switching finish types retains draft values while
  plain/no-relief explicitly removes those features. Native validation checks
  recess relationships and complete tip containment across every variant.
  All 424 viewer Node tests and 84 view-command tests pass, including analytic
  recess/tip volumes, invalid recess and thinner-variant rejection, saved
  drivers and history. Real browser checks cover finishes, corrections, linked
  parameters, editing, Apply, Undo/Redo and Save. All-target Clippy and sketch
  performance budgets pass.

- Studio hole authoring (schema 168, ABI 76 unchanged): create and edit native
  holes with family-coordinate placement and axis, positive entered or
  parameter-linked diameter/depth, blind, through-all, up-to-next and selected
  face extents. Guarded canvas face picks retain semantic selectors and cancel
  without discarding the form. Editing preserves placement and size formulas,
  existing drill points, recesses, relief and threads. New holes are plain with
  flat bottoms; recess/thread authoring remains future work. All 422 viewer Node
  tests and 83 view-command tests pass, including analytic volumes across
  thickness variants, drill-point breakout rejection, history and persistence.
  Browser checks cover creation, face pick/cancel, native previews, edits,
  Apply, Undo/Redo and Save. Clippy and sketch performance budgets pass.

- Studio solid edge finishing (schema 168, ABI 76 unchanged): create and edit
  native fillets and chamfers with semantic canvas edge picks, multiple edge
  sets, positive entered sizes or length parameter links. Existing selectors
  and compound size formulas survive editing. Target overlays, Fit, hover and
  cancellation share the guarded source picker with limiting faces. Edge
  extrema use enough tolerance for padded kernel bounds, with unique identity
  matching before a selector is published. Feature-view dependencies choose a
  downstream final output when relevant and the edited result otherwise.
  All 419 viewer Node tests and 82 view-command tests pass, covering analytic
  volumes, thinner-variant rejection, saved drivers, edits and history. Browser
  checks cover real edge picks, duplicate suppression, native previews,
  corrections, Apply, Undo/Redo and Save. Sketch performance budgets and
  all-target Clippy pass. Main's parallel variant generation and thread-view
  updates are included.

- Thread representation for through-all and up-to holes and oblique views
  (no schema or ABI change). Threaded holes without a modeled length are
  measured on the generated solid (material along a line just outside the
  crests, and the bore's bottom down the axis), and views at any angle to
  the axis draw the root silhouettes and the projected crest circle at the
  thread end. See [drawing representation](docs/THREADS.md#drawing-representation-schema-154).

- Parallel regeneration of distinct variants (no schema or ABI change).
  `regenerate_gltf_parallel` spreads distinct parameter variants over
  worker threads, each with its own OCCT session, generating each variant
  once in local coordinates and placing its instances as glTF nodes; the
  viewer uses up to eight workers and falls back to serial regeneration
  for assembly requirements. 2,000 distinct brackets load in about 8 s
  instead of 51 s; beyond eight threads OCCT's shared state costs more than
  it gains. See [glTF rendering](docs/MESH_HANDOFF.md#gltf-rendering).

- Studio limiting-face canvas picks (schema 168, ABI 76 unchanged): choose an
  extrusion's up-to-face target directly from an accepted feature overlay.
  Amber edges, hover/selected face fill and target-aware Fit guide picking;
  native semantic selectors are saved instead of transient mesh face indices.
  Escape/Cancel retain the solid form. Field, tool, model and instance changes
  clear overlays and guard late source/pick replies. Source picking remains
  read-only, and verified Preview/Apply still enforce every family instance.
  All 413 viewer Node tests and 80 view-command tests pass, including stale picks,
  cancellation, invalid sources/faces, native saved selectors and moving limits.
  Browser checks cover real hover/click, Fit, preview, Apply, Undo/Redo, Save and
  instance cleanup. Sketch performance budgets and all-target Clippy pass.

- Studio geometric extrusion end conditions (schema 168, ABI 76 unchanged):
  create/edit up-to-next and up-to-face extrusions, including union/cut tools.
  Limiting feature references stay live; direction controls replace depth for
  geometric extents. Face controls use family-axis center extrema, while edits
  preserve existing arbitrary face selectors and direction formulas. Native
  previews enforce complete forward coverage in every instance. Preview
  restoration also retains the accepted model's GPU buffers instead of trying
  to re-upload its discarded CPU normals; storage is bounded to one parked
  accepted model and released on reload. All 406 viewer Node tests and 79
  view-command tests pass. Checks cover moving limits, per-instance volumes,
  backward-variant rejection, saved references, preview restoration and buffer
  release. Browser checks cover face extrema, geometric editing, booleans,
  Apply, Undo/Redo and Save. Sketch performance budgets and all-target Clippy
  pass.

- Cleaner, lighter solid rendering (no schema or ABI change). glTF meshes
  are indexed and welded per B-rep face with smooth normals and carry their
  B-rep edges as a line primitive; the viewer loads binary glTF, draws dark
  feature edges under camera-fixed key and fill lights, instances shared
  meshes and merges single-use meshes into world-space batches. For 2,000
  distinct parts: 74 to 27 MB downloaded, about 0.5 s to under 0.1 s to
  load, 74 to about 12 MB of JavaScript heap, 54 to about 19 MB of GPU
  vertex data, and about 1 ms to 0.14 ms a frame. See
  [mesh handoff](docs/MESH_HANDOFF.md#gltf-rendering).

- Short-wave filtering and waviness (schema 168; ABI 76 unchanged). Raw
  stylus traces (`"raw": true`) take the Gaussian λs filter, from the
  requirement's new `short_cutoff_mm` (drawn as the λs-λc band) or ISO
  3274's λs for its cutoff, before roughness. Requirements' waviness
  limits are now checked: Wt and WSm of the λc mean line less its form,
  per profile, with the texture conforming only when both roughness and
  waviness do. A 1,000,000-sample raw trace takes about 70 ms. See
  [checking measured roughness](docs/DRAWINGS.md#checking-measured-roughness).

- Datum boundaries enlarged by geometric tolerances (no schema or ABI
  change). A datum feature of size referenced at MMB or LMB now takes its
  boundary from its own tolerance relative to the higher-precedence datums
  (its virtual condition, or the worst-case boundary at the opposite
  condition), for bores, pins, slots and tabs: a Ø9.92 bore perpendicular
  to A within Ø0.04 at MMC may shift 0.03 mm instead of 0.01. See
  [datum shift](docs/DRAWINGS.md#measured-inspection-schema-68).

- Common width datums (no schema or ABI change). Coplanar slot or tab
  width datum features named together (`A-B`) are now simulated by one
  shared center plane: each member starts at its own mating width and all
  change equally, minimized, until one plane fits them all. Two 20,000-point
  slots fit in about 20 ms. See
  [measured inspection](docs/DRAWINGS.md#measured-inspection-schema-68).

- Separate requirement (schema 167; ABI 76 unchanged). A position or
  profile control with datums may carry `separate_requirement` (SEP REQT,
  noted beneath the frame): in measured inspection it, or its pattern, no
  longer shares a datum shift with other controls on the same datum
  references. STEP PMI export reports it as unrepresented, since the
  writer does not set AP242's modifier yet. See
  [modifiers](docs/DRAWINGS.md#structured-gdt-intent).

- Datum targets (schema 166; ABI 76 unchanged). Planar datum features may
  list point, line and circular-area targets at basic locations on their
  datum plane; drawings mark them (×, phantom line, hatched circle) with
  the divided-circle symbol, and measured inspection establishes such a
  datum from the points recorded at its targets alone, down to a 3-2-1
  frame of six points. See
  [datum targets](docs/DRAWINGS.md#datum-targets-schema-166).

- Studio profile lofts (schema 165, ABI 76 unchanged): create and edit native
  lofts through ordered saved profiles, including hollow sections with explicit
  hole tracks and lofted union/cut tools. Smooth/ruled controls, section adding,
  removal and ordering keep outer and inner tracks aligned. Saved profile
  references and source sketch parameters remain linked. Shared solid previews,
  graph/version guards, all-instance checks and model history cover the new
  operation. All 398 viewer Node tests and 77 view-command tests pass, including
  hollow volumes, booleans, editing, saved tracks and another instance's invalid
  bore. Browser checks cover three-section hollow lofts, station/hole ordering,
  section activation, validation, native preview, Apply, Undo/Redo and Save.
  Sketch performance budgets and all-target Clippy pass.

- Studio path sweeps (schema 165, ABI 76 unchanged): create or edit native
  solid sweeps from applied profiles and separate sketch wires or helices,
  including hollow profiles and swept union/cut tools. The active open sketch
  defaults the path; otherwise open paths precede closed-wire candidates.
  Least-twist, Frenet, fixed and binormal orientations are available; editing
  preserves authored binormal formulas unless explicitly replaced. Verified
  meshes use the existing preview guards, all-instance checks, material-change
  rules and model history. All 394 viewer Node tests and 75 view-command tests
  pass. Native checks cover hollow volumes, booleans, saved formula drivers and
  another instance's zero binormal. Browser checks cover path activation, 3D
  preview/restoration, rejected cuts/orientations, corrections, editing, Apply,
  Undo/Redo and Save. Sketch performance budgets and all-target Clippy pass.

- Studio revolutions (schema 165, ABI 76 unchanged): create or edit turned
  solids directly in Sketch to solid, including revolved union/cut tools.
  Signed degree inputs, scalar radian parameter links and symmetric angular
  extents use the existing native operation. The studio defaults the axis to
  sketch-local Y through the sketch origin. Existing axis/origin and angle
  formulas survive editing; explicit entered values replace them. Verified 3D
  previews and Apply share existing graph/version guards, all-instance checks,
  material-change checks and model history. All 392 viewer Node tests and 73
  view-command tests pass, including signed/symmetric volumes, boolean material,
  invalid axes, another instance's invalid angle and saved formula preservation.
  Browser checks cover 3D preview/restoration, driven revolve editing, Apply,
  Undo/Redo, a revolved cut and Save. Existing sketch performance budgets pass.

- Studio sketch-to-solid authoring (schema 165, ABI 76 unchanged): create
  extrusions, unions and cuts or edit an existing extrusion directly from the
  active sketch. Default direction follows its plane normal; parameter/formula
  drivers survive editing. Native verified previews now return their existing
  glTF mesh and identity metadata, and the studio renders it without replacing
  its accepted model. Field/context changes, Close and stale replies cannot
  retain an old preview. Apply serializes feature mutations with sketch/model
  history and Save through reload; rejection keeps the form, while an accepted
  refresh failure cannot reapply. Current planar outputs fall back to suitable
  boolean targets/edit outputs. All 390 viewer Node tests and 71 view-command
  tests pass; native tests check preview meshes and all-variant rejection.
  Browser checks cover 3D preview/restoration, booleans, symmetric bounds,
  off-plane normals, parameter preservation, rejection/retry, model history
  and Save; sketch scale checks remain within budget.
  See [making a solid](docs/SKETCH_STUDIO.md#making-a-solid-from-a-sketch).

- Measured straightness (no schema or ABI change). Inspection now
  evaluates straightness instead of reporting it: surface line elements
  (sections normal to the control's view for a plane, generator lines for a
  cylinder) by their narrowest strip, and a cylinder's derived median line
  (Ø zone) by the thinnest cylinder holding its cross-section centers, with
  MMC/LMC bonus from local size and per-section size checks. 1,000 cross
  sections take about 10 ms. See
  [measured inspection](docs/DRAWINGS.md#measured-inspection-schema-68).

- Studio material boundary authoring (schema 165, ABI 76 unchanged): choose
  ordered outer profiles, holes and additional material regions directly on
  the canvas, including verified pattern/projection geometry and split pieces.
  Initialize native arrangement discovery from picked curves and cell seeds,
  or explicitly return to automatic single-boundary selection. Forms isolate
  unfinished picks, validate references and exclusive modes, preserve geometry
  and edit definitions, and stage one draft Undo entry. Rejected loops remain
  repairable. Native browser checks verify explicit and discovered material
  producing two solids with the expected volume, generated holes, Apply,
  draft/model history, repair and Save. All 375 viewer Node tests and 71
  view-command tests pass. A 10,000-curve benchmark checks 1,000 picks and staging
  within one second each. See
  [material boundaries](docs/SKETCH_STUDIO.md#choosing-material-boundaries).

- Studio instance menu (no schema or ABI change). With several instances
  of the family, a header menu chooses the one whose overrides, placement
  and faces the shared sketches are solved, drawn and attached with; it
  lists each instance's overrides and refuses to switch while a draft has
  edits. This closes the studio's last parity gap with the classic
  editor. See [Instances](docs/SKETCH_STUDIO.md#instances).

- Pattern tools in the sketch studio (no schema or ABI change). Mirror,
  Linear and Circular take selected or clicked source curves, then the
  mirror line, direction line or center point with a dashed guide, and a
  count with spacing or step angle; they record linked sketch patterns
  validated like the classic panel. A mirror shares source points on its
  axis and closes a half outline through it; hole loops copied whole get
  copied holes. Palette tools may now have no shortcut key. See
  [Pattern tools](docs/SKETCH_STUDIO.md#pattern-tools).
- Offset in the sketch studio (no schema or ABI change). Offset (`Z`)
  records the whole-profile `offset` operation from a typed signed
  distance or parameter with round or sharp corners, drawing an
  approximate guide while typing; clicking again changes or removes it.
  Fillets and chamfers now go before an existing offset, as validation
  requires. The studio bench times trim crossings, delete and the offset
  guide on 10,000 curves. See [Modify tools](docs/SKETCH_STUDIO.md#modify-tools).
- Trim and split in the sketch studio (no schema or ABI change). Trim
  marks the piece of a line, arc or circle between the curves crossing it
  and cuts it away by editing the geometry: new ends at the crossings held
  by point-on-curve constraints (or shared points), a middle piece leaving
  two collinear lines or equal-radius arcs, a circle becoming a same-named
  arc, constraints on the old extent dropped. Split cuts a line or arc in
  two at a shared point. Trimmed outlines reopen and close like deleted
  ones, so a circle on an edge trims to a closed notch whose corners Fillet
  can round. See [Modify tools](docs/SKETCH_STUDIO.md#modify-tools).
- Fillet, chamfer and extend in the sketch studio (no schema or ABI
  change). A Modify group in the palette: Fillet and Chamfer take a corner
  of the outer profile and a typed value or parameter, recorded as profile
  operations; Extend shows where a line, arc or spline end would run and
  records an `extend_to` curve operation to the first curve it meets. The
  edited outline draws solid over its dashed sources. Curve edits with
  crossings (split at a crossing, extend-to) now work on every sketch
  plane: they were rejected off XY ("intersection curves must lie in the
  XY plane") and now run on sketch-local curves placed rigidly in the
  frame; a scale case regenerates 10,000 curves with 1,000 extensions on a
  tilted plane in 0.13 s (0.10 s on XY). See
  [Modify tools](docs/SKETCH_STUDIO.md#modify-tools).

- Studio accepted-source links (schema 165, ABI 76 unchanged): project native
  accepted edges and reattach to planar faces directly from a world-space
  source overlay. Existing native controls retain semantic bindings, exact
  spline refinement/detachment and dependency summaries. Each accepted action
  stages one isolated draft Undo entry; cancellation, history, context changes
  and late responses cannot overwrite newer edits. Source/name/instance checks
  reject mismatched native replies, construction references require explicit
  boundaries, and source picks bypass dimension-label click targets. Node
  coverage includes source loads, picks, detachment, stale callbacks and history.
  Native browser checks verify line/curved projections, face reattachment,
  degree refinement, canvas inspection, dependencies, detachment, history and
  Save. All 336 viewer Node tests and 71 view-command tests pass. A 10,000-edge
  benchmark checks projected-catalogue caching and click latency.
  See [source links](docs/SKETCH_STUDIO.md#linking-accepted-source-geometry).

- Studio linked-pattern canvas authoring (schema 165, ABI 76 unchanged): mirror,
  linear and circular patterns use the existing native controls directly in
  the studio, with repeated source picks and axis/direction/center picks.
  Earlier generated geometry remains usable; self and later dependencies are
  rejected. Isolated forms preserve parameter/formula drivers, create one draft
  Undo entry on submission and support rejected-draft repair. Shared operation
  guards cancel stale forms and preserve camera controls. Native browser checks
  verify solved copy coordinates, parameter updates, draft/model history, Apply
  and Save. The new 20,000-curve benchmark checks 1,000 picks within one second;
  312 viewer Node tests and 71 view-command tests pass. See
  [linked patterns](docs/SKETCH_STUDIO.md#picking-linked-patterns).

- Studio curve and region editing (schema 165, ABI 76 unchanged): reuse the
  native trim, split, extend, fillet, chamfer and offset controls directly on
  the studio plane. Canvas picks fill isolated forms; submitting an operation
  or discovery selection creates one draft Undo entry. Split pieces retain
  pickable saved names after Apply, boundary replacement is atomic, and stale
  forms cannot overwrite a changed sketch. Rejected operations can be repaired
  or removed; returning to the accepted definition skips redundant Preview
  while retaining history. Native browser checks cover operation workflows,
  draft/model history and Save; 300 viewer Node tests and 71 view-command tests
  pass, with sketch performance checks within budget. See
  [curve and region editing](docs/SKETCH_STUDIO.md#editing-curves-and-regions).

- Studio weighted-point canvas authoring (schema 165, ABI 76 unchanged): pick
  authored/projected/generated source points, review positive literal or scalar
  parameter weights, then create or relink one named point through draft Preview
  and Apply. Existing parameter/formula terms survive relinking; literal-weight
  markers use current solved source coordinates, while Preview evaluates full
  expressions and model variants. Staging is one Undo entry and context changes
  cancel pending picks. Source appends reuse the shared 10,000-point catalogue,
  with a tested 256-term cap. See
  [weighted points](docs/SKETCH_STUDIO.md#picking-weighted-points).

- Studio named-point canvas picking (schema 165, ABI 76 unchanged): the Named
  points panel reuses the existing crossing and spline-sample controls on the
  studio plane. Native finite intersections drive search-window markers and
  unique-candidate staging; native spline samples support closest-click queries
  and exact fraction review before staging. Each link is one draft Undo entry.
  Cancellation, context changes and late replies cannot stage old candidates,
  and competing camera/face/plane workflows remain separate. Tests cover native
  response identity, ambiguity, fractions, history and delayed replies. See
  [canvas point picking](docs/SKETCH_STUDIO.md#picking-named-points-on-the-canvas).

- Studio driving-dimension editing (schema 165, ABI 76 unchanged): double-click
  or keyboard-activate an authored dimension label, or Edit it in the selection
  inspector. Updates replace one constraint's value, retain its roles/index,
  and use draft Preview, Apply and Undo/Redo. Values support mm, degrees and
  parameter names; untouched units/formulas are preserved, invalid edits stay
  open, and stale editors cannot overwrite changed sketches. Selecting curves
  includes their defining-point dimensions. Reference measurements now appear
  as parenthesized read-only labels; internal generated constraints remain
  display-only. See [dimension editing](docs/SKETCH_STUDIO.md#editing-driving-dimensions).

- Studio generated geometry references (schema 165, ABI 76 unchanged): bind
  native point/curve catalogues on load and current successful Preview, retain
  them across compatible drafts and Undo/Redo, and refresh suggestion membership
  after Preview. Generated sides, pattern copies and projected curves use
  solved defining roles for dimensions, shared-endpoint tangency, midpoint and
  line/radius inference; generated corners can anchor newly drawn geometry.
  Display-only scene names cannot produce snapping constraints. A persistent
  10,000-generated-line benchmark checks indexed snapping, and tests cover
  catalogue invalidation, native kinds, history and late Preview replies. See
  [generated references](docs/SKETCH_STUDIO.md#generated-geometry-references).

- Studio asynchronous edit guards (schema 165, ABI 76 unchanged): Apply,
  model Undo/Redo and Save serialize through their reload; editing and draft
  history stay locked while requests are pending. Rejected Apply preserves
  the draft; accepted Apply with a failed refresh reports the accepted change.
  Solved drags check draft revisions, model versions, instances, layers and
  request order before creating one local Undo entry. New edits invalidate
  old Preview success/errors during the debounce, and old reloads cannot
  overwrite a newer model operation. Tests cover delayed and overlapping
  replies, rejection, history and context changes. See
  [pending edits](docs/SKETCH_STUDIO.md#pending-edits-and-solver-replies).

- Studio draft history boundary (schema 165, ABI 76 unchanged): Undo to the
  starting sketch retains Redo for existing and new sketches, and stays in
  local history until explicit Discard or verified Apply. Toolbar, Save and
  Advanced editing respect the held session. Snapshot restores cancel scheduled
  previews, ignore late success/errors and restore accepted baseline feedback.
  Tests cover multi-step history, branching, layer changes and the model-history
  boundary, including versioned model navigation after Apply/Discard. A scale
  case checks 20,000 snapshot transitions on a 10,000-curve sketch. See
  [draft history](docs/SKETCH_STUDIO.md#draft-undo-and-redo).

- Delete geometry in the sketch studio (no schema or ABI change). With
  Select, Delete or Backspace (or the inspector's Delete selection, which
  lists what will go) removes the selection and what can no longer stand
  without it: curves on a deleted point, endpoints no other geometry uses,
  and constraints, reference dimensions and edits that name it; loops and
  pattern lists drop the members, and generated names delete their shape,
  pattern or projection. A loop opened by a deletion is closed again by
  drawing the missing edge. The change is a draft edit, so Undo restores
  it. See [Deleting](docs/SKETCH_STUDIO.md#deleting).
- Planes in the sketch studio (schema 165; ABI 76 unchanged). A Planes panel
  lists the model's datum planes and draws them in the view; New plane
  builds any of the five constructed kinds from a base plane, one or two
  picked faces, or three picked (corner-snapped) or typed points, with a
  live preview through `POST /api/datum`. Each driving value becomes a
  parameter named after the plane, edited in the plane's inspector (angles
  in degrees); the plane and its sketches follow. Sketch on this plane
  starts a sketch on it. See [Planes](docs/SKETCH_STUDIO.md#planes).
- Constructed datum planes (schema 165; ABI 76 unchanged). Plane datums may
  be offset or angled from a base plane (XY, XZ, YZ or another plane), on a
  selected planar face, midway between two parallel faces, or through three
  points, each fully defined by its references and parameter expressions.
  Planes on faces resolve during regeneration and make their sketches
  depend on the faces' features; each plane resolves once per regeneration,
  so 500 sketches on a 500-deep chain regenerate and follow a height edit
  in 0.3 s (2 s budget). Sketch x axes are projected into their planes.
  The model command adds `add_datum`, `replace_datum` and `remove_datum`,
  and view data lists resolved `datum_planes`, with failures carried to the
  sketches on them. See [Datum planes](docs/DATUM_PLANES.md).
- Studio primitive palette completion (schema 164, ABI 76 unchanged): elliptical
  arcs, conics, text, three-point arcs, center rectangles and explicit rational
  control splines now have gestures, icons, shortcuts, options and bounded
  ghosts. Center relationships and through-points persist as native constraints.
  Degree/weight/rho/font validation uses the existing model contract. A gallery
  covers all six commit paths, native curves/text, closed control splines and
  guarded missing-font rejection through history and Save/reopen. The studio
  scale benchmark adds maximum-pole authoring on 10,000 existing curves.
  See [primitive tools](docs/SKETCH_STUDIO.md#additional-primitive-tools).


- Sketch studio (no schema or ABI change). `occt-view --serve` serves
  `/studio`, a plane-layered 3D sketcher beside the classic editor: each
  sketch is drawn on its plane in the model view (sketch scenes now carry
  their resolved frame), with layers, Look at, and new sketches on base
  planes or picked faces. A toolbar draws every shape with typed values and
  automatic constraints (coincident, midpoint, on curve, horizontal,
  vertical, parallel, perpendicular, equal, tangent); selections offer all
  36 typed constraints; a Dimension tool takes values or parameters; the
  sketch is colored by solve state; labels are laid out without overlaps;
  and an Advanced drawer hosts the classic panels. Snapping costs 0.1 ms
  per mouse move at 10,000 lines. See [Sketch studio](docs/SKETCH_STUDIO.md).
- Socket-head countersink relief (schema 164; ABI 76 unchanged). The Carr
  Lane catalog gains column C, the optional 60° relief under the head, and
  a counterbore may take a relief diameter that cuts a 60° cone at its
  floor into the bore. A relief no wider than the bore, as for several
  normal-fit holes, cuts nothing. The counterbore and cone are one revolved
  solid, with no boolean between them. Callouts print the relief. A scale
  case edits 100 relieved counterbores in one plate in 13.5 s against a
  20 s budget; each hole's cut follows the plate's face count, which a
  relief raises by a third. See
  [Countersink relief](docs/HOLE_SIZE_CATALOG.md#countersink-relief-schema-164).
- ISO 286 tolerance classes (schema 163; ABI 76 unchanged). A diameter or
  linear dimension, including a live hole callout, may carry a tolerance
  class H6–H13 (holes) or h6–h13 (shafts). The label shows the class and
  its limits, and measured sizes are judged against them. The IT6–IT13
  grades to 500 mm are a frozen V1 table, each value checked against the
  standard's generating formula; sizes beyond it are errors. Lookup is
  O(1). See [Tolerance classes](docs/DRAWINGS.md#tolerance-classes-schema-163).
- Sheet-metal hems (schema 162; ABI 76 unchanged). Either end of a strip
  may be hemmed: open hems fold 180° back alongside their flange at a gap of
  at least a tenth of the thickness; rolled hems curl past 180° and are
  limited where the roll would re-enter its flange. Hems are bends on the
  blank, at K or from an allowance table, which may now list angles to
  360°. The origin and cutout flange numbers are unchanged by a start hem.
  The 1,000-bracket scale case now hems every bracket. See
  [Sheet metal](docs/SHEET_METAL.md#hems-schema-162).
- Sheet-metal bend tables (schema 161; ABI 76 unchanged). A flat pattern
  can take its bend allowances from a shop's table for its thickness,
  holding allowances or deductions by inside radius and bend angle, instead
  of the neutral factor. Values interpolate bilinearly; a bend outside the
  table or a sheet of another thickness is an error, never an
  extrapolation. Deductions convert through the outside setback. Cutouts
  and bend lines move with the table's allowances, and the drawing names
  the table. 100 blanks of 1,023 bends each through a 256 × 256 table take
  14 ms against a 1 s budget. See [Sheet metal](docs/SHEET_METAL.md#bend-tables-schema-161).
- Sheet-metal cutouts (schema 160; ABI 76 unchanged). A sheet's circle,
  rectangle and slot cutouts sit on a flange's straight region. Each is cut
  square to that flange in the folded solid, in one boolean, and placed on
  the blank after the earlier flanges and bend allowances. The blank is
  built as a face with holes, with no boolean, and its drawing outlines
  them. The bridge's face-with-holes check no longer reruns OCCT's
  quadratic whole-face check, which its own checks already cover; 10,000
  holes build in 2.6 s. See [Sheet metal](docs/SHEET_METAL.md#cutouts-schema-160).
- Intermittent STEP test failures explained and guarded (schema 159 and
  ABI 76 unchanged). Both flakes in `Later` were unsynchronized OCCT STEP
  translation, which shares process-wide state:
  - `assembly_frames_export_as_step_sub_assemblies` wrote four STEP
    assembly occurrences instead of five;
  - `invalid_results_are_rejected_healed_or_allowed_by_option` read the
    healed bowtie fixture as invalid.

  Both predate the process-wide STEP lock added with annotation views
  (schema 145). With the lock disabled in a scratch build, the STEP kernel
  tests failed 95 of 100 parallel runs, 44 with the four-occurrence count.
  A race program doing first STEP calls from 16 threads per fresh process
  loaded the bowtie invalid or failed in 16 of 300 processes and crashed in
  25. With the lock, 200 parallel test runs and 300 race processes passed.
  The new `occt_bridge_step_concurrency` C test races 100 fresh processes of
  16 threads; it caught the unlocked library in 19 of 20 runs and passed 30
  of 30 with the lock, in about 2 s.
- Multi-sheet revisions (schema 159; ABI 76 unchanged). Sheets sharing a
  drawing number state `same_revision`, where sheet 1 records the releases
  and every sheet shows its revision, or `per_sheet`, where each sheet is
  revised alone and sheet 1 may draw a SHEET / REV status table. Documents
  check that every sheet is present once with one method. See
  [Multi-sheet revisions](docs/DRAWINGS.md#multi-sheet-revisions-schema-159).
- Approval workflow and content digests (schema 158; ABI 76 unchanged). A
  drawing's `approval_workflow` orders the sign-off roles; releases are
  pending until all have signed, and the title block shows the latest
  released revision. `sign_release` adds each sign-off with a SHA-256 digest
  of the drawing as signed, and `verify_approvals` reports later changes.
  Digests are not identity-bound signatures. See
  [Approval workflow](docs/DRAWINGS.md#approval-workflow-and-content-digests-schema-158).
- Merged identical sub-assemblies (schema 157; ABI 76 unchanged). Nested
  parts lists with `merge_identical` list sibling sub-assemblies with the
  same contents as one row with their count. Rows beneath keep per-unit
  quantities and every copy's instances, so balloons resolve. Shared part
  numbers on differing sub-assemblies are rejected. See
  [Drawings](docs/DRAWINGS.md#parts-lists-and-balloons-schema-87).
- Change markers and zones (schema 156; ABI 76 unchanged). A release's
  `changes` draw revision symbols (the revision in a delta) on the sheet,
  with optional leaders to a datum in a view. A revision table's
  `zone_column` lists each release's marker zones from the sheet's zone
  grid. See
  [Change markers and zones](docs/DRAWINGS.md#change-markers-and-zones-schema-156).
- Surface textures on persistent faces (schema 155 and ABI 76 unchanged). A
  texture attachment's `faces` now name the surface the requirement governs.
  They are validated, and the drawing's leader lands on them, at the point
  nearest the anchor. `surface_texture_faces` reports each texture's face
  indices, area and landing point. It rejects a face governed by two
  differing requirements. AP242 export lists textures as skipped, since
  OCCT has no representation for them. See
  [Textures on persistent faces](docs/DRAWINGS.md#textures-on-persistent-faces).
- Surface texture acceptance and profile filtering (schema 155; ABI 76
  unchanged). A texture requirement's `acceptance` may invoke ISO 4288's
  16% rule, decided in stages (a first reading under 70% of the maximum,
  three readings, then twelve), with `readings_required` reported until the
  readings decide. Inspection records may supply stylus profiles, filtered
  by the Gaussian profile filter at the requirement's cutoff (0.8 mm by
  default) to Ra, Rq, Rz or Rmax over five sampling lengths. See
  [Checking measured roughness](docs/DRAWINGS.md#checking-measured-roughness).
- Conventional thread representation in drawings (schema 154; ABI 76
  unchanged). Views with `thread_representation` draw thread features and
  blind or modeled-length tapped holes by the ISO 6410-1 convention: root
  and thread-end lines in side views, dashed for internal threads outside
  sections, and a three-quarter root circle in end views. See
  [Drawing representation](docs/THREADS.md#drawing-representation-schema-154).
- Thread engagement strength (schema 153; ABI 76 unchanged). A tapped hole's
  Unified spec may give the mating screw's and the part's tensile strengths.
  Regeneration then requires a length of engagement at least what B1.1's
  published engagement formulas need for the screw to break before either
  thread strips. `unified_engagement` reproduces the published 3/8-16 worked
  values. See
  [Engagement strength](docs/THREADS.md#engagement-strength-schema-153).
- Thread inspection against Unified limits (schema 152 and ABI 76
  unchanged). Inspection records may list measured pitch, major and minor
  diameters of threads and tapped holes that name a Unified class. Each is
  judged against the class's limits under the record's decision rule and
  uncertainty, and `occt-inspection-report` counts conforming and
  nonconforming threads. 10,000 measured tapped holes are a required scale
  case. See
  [Inspecting threads](docs/THREADS.md#inspecting-threads-against-the-limits).
- Unified inch thread classes (schema 152; ABI 76 unchanged). A modeled
  thread or tapped-hole callout may name a B1.1 series (UNC, UNF, UNEF, UN,
  UNS) and class (1A, 2A, 3A, 1B, 2B). Regeneration checks the size against
  the standard series, the class against the side, a tapped hole's
  designation against the standard one, and its bore against the class's
  minor-diameter limits. `UnifiedThread::limits` gives limits of size from
  B1.1's published tolerance formulas. They match published 2A and 2B
  tables for ten sizes except four values one rounding step off.
  `unified_fit` checks an external thread against an internal one. See
  [Unified classes](docs/THREADS.md#unified-inch-classes-asme-b11-schema-152).
- Dataset identification and authority (schema 151, ABI 76). A drawing's
  `dataset` gives the product identifier, the responsible organization and
  whether the model, the drawing or both govern. When the model governs,
  every release must name the model revision it releases. `export_step_pmi`
  writes the identifier, authority statement, latest release revision and
  its approvals (AP242 approvals with dates, approvers and roles) on the
  root product, and verifies them from the file's entities. `occt-model`
  responses report the drawing's release status against the revision
  ledger. See
  [Dataset identification](docs/DRAWINGS.md#dataset-identification-and-authority-schema-151-abi-76).
- Section and clipped annotation views (schema 150, ABI 75). An annotation
  view may list up to 16 `clipping_planes`, each with an origin, a normal
  and a kept side as for section drawing views. They are written as an
  AP242 multi-clipping camera whose planes intersect, and the read-back
  checks that OCCT recreates the same planes for the view. See
  [Annotation views](docs/DRAWINGS.md#annotation-views-schema-145-abi-71).
- Projected tolerance zones in AP242 exchange (schema 145 and ABI 71
  unchanged). OCCT 7.9's writer drops projected zones. It also gives every
  diameter zone a runout zone definition, which its reader then reports as
  a runout modifier on position and other diameter-zone controls. The bridge
  now replaces each of those definitions before writing: a projected zone
  definition carrying the height for an Ⓟ control, and nothing otherwise.
  Verification counts a stray zone modifier as a lost zone. Ⓟ controls now
  read back intact. See
  [Semantic PMI](docs/DRAWINGS.md#semantic-pmi-on-persistent-faces-schema-144-abi-70).
- Linked sketch pattern authoring (schema 149, ABI 74 unchanged): typed mirror,
  linear and circular recipes support multiple authored/generated source curves,
  shared mirror points, count, spacing/angle drivers and retained formulas.
  Existing IDs/types stay stable; source ordering excludes own/later copies.
  Annotation editing rejects stale recipe matches. Tests cover all three kinds,
  generated shape boundaries, Preview/Apply,
  history, Save/reopen and guarded deletion with dependent references.
  A persistent Node scale case covers a 20,000-curve menu and maximum-count
  recipe validation. See [sketch patterns](docs/SKETCHES.md#sketch-patterns-and-holes-schema-112--abi-60).


- Weighted point authoring (schema 149, ABI 74 unchanged): Named point links
  now adds/edits positive weighted source terms, parameter weights and retained
  compound formulas. Weighted annotations open the recipe editor. Linking
  unfixes the authored target, replaces its prior point links and preserves
  other dimensions. Sources include verified generated/projected points.
  A shared source datalist bounds the maximum 256-row editor with 10,000 choices
  to about 32 ms in the persistent Node scale case. Native tests cover generated
  sources, parameter updates, history, Save/reopen and atomic source loss.
  See [weighted points](docs/SKETCHES.md#weighted-dependent-points-schema-146).


- Advanced sketch relationship controls (schema 149, ABI 74 unchanged): typed
  symmetry, equal curve length, coradial, G2 continuity and seven seeded/analytic
  contact tangencies now support menus, canvas selection and annotation editing.
  Forms explain shared endpoints, contact seeds, full-ellipse/arc restrictions
  and G2 redundancy. Native tests restore G2 and three contact families through
  Preview/Apply, history and Save/reopen, with atomic invalid-contact rejection.
  A persistent Node scale case checks 10,000 authored plus 10,000 generated
  curves and a 1,000-row editor. See [G2 and relationship controls](docs/SKETCHES.md#curvature-g2-continuity-schema-128--abi-65).


- Reference dimension authoring (schema 149, ABI 74 unchanged): the live editor
  adds, edits, renames and removes all ten native reference dimension types,
  using canvas selections and authored/generated/projected entity menus.
  Selecting an unchanged native measurement label opens the matching readout;
  stale definition matches are rejected. Native Preview keeps points, solver
  degrees and driving constraints unchanged. Tests cover source-driven updates,
  generated curve readouts, history, Save/reopen and atomic lost-source rejection.
  See [reference dimensions](docs/SKETCHES.md#axis-distances-curve-lengths-and-reference-dimensions-schema-122).


- Exact spline sample fractions (schema 149, ABI 74 unchanged): candidate review
  accepts a typed fraction or start/midpoint/end parameter shortcuts and refreshes
  the native marker before staging. Fraction edits clear stale candidates;
  pending, failed and canceled queries cannot stage. Exact queries share the
  solved source builder with nearest picks and retain explicit periodic seam
  fraction 1. Checks cover generated, interpolated, periodic, conic and projected
  sources, source edits and history/Save/reopen. A persistent benchmark of 25
  exact queries with 10,000 extra fixed points passed in about 61 ms against a
  300 ms budget. See [canvas samples](docs/SKETCHES.md#canvas-spline-samples-abi-74).

- Spline sample candidate review (schema 149, ABI 74 unchanged): a native pick
  displays a marker, hint connector, coordinates and parameter fraction before
  an explicit **Stage spline sample** action changes the draft. Staging reuses
  the exact result with no extra query. New picks, failed queries, cancellation
  and stale scenes/sources clear feedback and block stale staging. Candidate
  text is rounded without reducing the saved fraction precision. See
  [canvas samples](docs/SKETCHES.md#canvas-spline-samples-abi-74).

- Canvas spline samples (schema 149, ABI 74): click an authored, generated or
  projected spline to stage a named point at its native closest parameter.
  A read-only guarded query includes endpoints, canonicalizes closed seams,
  and rejects ambiguous branches. New targets remain free and save a literal
  `spline_point` link. Current Preview, version/source guards, cancellation and
  late-response checks protect picking. Native tests cover orientation/location,
  rational/interpolated/periodic/projected sources and history/Save/reopen.
  Ten native pick queries took about 21 ms. See [canvas samples](docs/SKETCHES.md#canvas-spline-samples-abi-74).

- Generated/projected spline-parameter links (schema 149, ABI 73 unchanged):
  named point controls offer core-catalogued spline copies, generated conics and
  live projected splines/chains. Links retain source IDs and fraction drivers,
  preserve verified catalogues across clones, and guard authored endpoint aliases
  against self-links. Native checks cover rational/interpolated/periodic copies,
  mirrored traversal, circular transforms, projection source edits, atomic
  rejection and history/Save/reopen. 100 local link operations with 10,000
  available sources took about 164 ms. See [named point links](docs/SKETCHES.md).

- Generated-curve selection (schema 149, ABI 73 unchanged): the core catalogue
  groups shape and pattern curves by native kind for dimensions, constraints,
  crossings and curve/region edits. JavaScript shape-name guesses are removed.
  Verified names survive crossing-point clones; split names cannot shadow them.
  Native checks verify copied-curve equal lengths, midpoint-driven splits,
  source edits, history, Save/reopen and lost-reference rejection. 100 curve-list
  queries over 10,000 references took about 78 ms. See [sketch drawing](docs/SKETCHES.md).

- Generated-point canvas snaps (schema 149, ABI 73 unchanged): a kernel-free
  structural catalogue exposes rectangle, slot, polygon and chained pattern
  point names to drawing and typed constraints. Native positions guide snaps;
  saved geometry retains the generated IDs. Recipe/source-curve edits disable
  stale names until successful Preview. Checks cover shared mirror points,
  expansion budgets, source edits, atomic lost-reference rejection and
  history/Save/reopen. 100 snaps over 10,000 references took about 93 ms.
  See [sketch drawing](docs/SKETCHES.md).

- Source-linked canvas snaps (schema 149, ABI 73 unchanged): drawing reuses
  projected endpoints, named chain junctions and named intersections instead of
  freezing duplicate points. Explicit point picks resolve coincident-reference
  ties; native point coordinates, a visible snap label and bounded-scale checks
  support interactive drawing. Typed point constraints expose the same named
  projection references. Native checks verify source edits, history, Save/reload
  and lost-reference rejection. See [sketch drawing](docs/SKETCHES.md).

- Native crossing-pick feedback (schema 149, ABI 73 unchanged): one read-only
  curve-pair query supplies exact bounded candidates for all cursor positions.
  Current solved geometry, including projected splines, reports unique, missing,
  ambiguous or failed results before staging. Incomplete/stale/loading results
  cannot stage a point. The radius can be adjusted without reselecting curves or
  querying again; invalid input blocks staging until corrected. Queries share the
  saved selector's native curve builder,
  finite intersections and budgets; full Preview/Apply remains required. See
  [native picking feedback](docs/SKETCHES.md#native-feedback-while-picking).

- Canvas-selected crossings (schema 149, ABI 73 unchanged): click two authored
  or projected source curves and a hint to stage a literal selected-crossing
  window. The picker creates a named non-fixed target or relinks an existing
  target, previews the cursor radius, and leaves verification to native
  Preview/Apply. Drags, cancellation, stale views and tool changes cannot
  complete an old pick. See [canvas crossing picks](docs/SKETCHES.md#pick-a-selected-crossing-from-the-canvas).

- Saved crossing-window review (schema 149, ABI 73 unchanged): the sketch editor
  lists native resolved hints/radii, source curves and parameter drivers, and can
  show one or multiple search areas with source highlights. Filtering and bounded
  lists support large sketches. Draft changes and stale native views invalidate
  overlays until a successful Preview; review actions do not edit model data.
  See [window review](docs/SKETCHES.md#review-saved-crossing-windows).

- Interpolated spline parameter links (schema 149, ABI 73): the existing
  `spline_point` definition now follows exact native open/closed interpolation,
  including fit-point edits and endpoint tangent neighbors. Each target adds two
  coordinates and two equations, allowing dimensions to drive free source
  geometry. Shared cycle checks include tangent dependencies; numeric samples
  invalidate on source movement and cache sizes are bounded. Viewer controls,
  guarded Preview/Apply, drag protection, history and Save cover both spline
  modes. See [interpolated links](docs/SKETCHES.md#interpolated-spline-parameter-links-abi-73).

- Explicit selected crossing points (schema 149, ABI 72): saved sketch-local
  hint/radius windows select exactly one finite native intersection independently
  of the target seed. Loss, ambiguity, overlaps and exhausted budgets reject
  regeneration. Viewer controls preserve expression drivers and offer guarded
  Preview/Apply, history and Save. Numeric curve-pair caching invalidates on
  source movement; 1,000 shared-pair targets pass a two-second budget. See
  [selected crossings](docs/SKETCHES.md#selected-crossing-points-schema-149).

- Saved spline-parameter point links (schema 148, ABI 72): named non-fixed
  points follow the current rational basis after pole/weight/knot/degree edits,
  without additional solver unknowns. Shared dependency validation rejects
  cycles and self-references. Viewer controls reattach detached named points to
  spline parameters or seeded two-curve crossing constraints, with dependency
  inspection and guarded Preview/Apply, undo/redo and Save. Periodic links now
  wrap their pole/weight indices and give identical seam positions at fractions
  0 and 1, matching OCCT across degrees, nonuniform knots, seam multiplicities
  and native refinement. Interpolated links are also available, as described above.
  A 1,000-link solve adds no unknowns and completed in about 20 ms in the integration check. See
  [named-point links](docs/SKETCHES.md#saved-named-point-links-schema-148).

- Read-only sketch dependency inspection: direct uses/consumers and transitive
  source/dependent paths cover projection aliases, weighted points, generated
  shapes/patterns, edited boundaries, constraints, profiles and downstream model
  features. Canvas picks and projection review controls highlight references;
  large lists are capped and generated references resolve lazily. Schema 147,
  ABI 72. See [dependency inspection](docs/SKETCHES.md#geometry-dependency-inspection).

- Detach exact projected splines/chains into authored spline drafts: fresh native
  draft resolution preserves rational poles/basis and named references. Source
  geometry is frozen from the selected instance, with support links retained;
  downstream live intersection dependencies are rejected. Guarded Preview/Apply,
  undo/redo and Save use schema 147 and ABI 72. See [sketch detach](docs/SKETCHES.md#detach-projected-splines-into-authored-geometry).

- Annotation views (schema 145, ABI 71). A drawing's `annotation_views`
  name a view direction, image right, a scale and the datum features and
  frames they show. `export_step_pmi` draws each shown annotation in 3D in
  the plane of the first view that shows it, using the sheet layout and a
  built-in single-stroke letter set. It writes these drawings as AP242
  tessellated presentations and each view as a saved view (camera plus
  annotations), which OCCT's writer cannot do itself. The read-back verifies
  each presentation segment by segment and each view's camera and
  annotations. Two OCCT 7.9 writer gaps are worked around: tolerance values
  were written in metres when a file had no size dimension, and
  presentations failed the write without one. STEP reads and writes now
  share a process-wide lock, since OCCT's translators failed when sessions
  wrote concurrently. The 400-hole plate's 400 drawn controls and two views
  export and verify in 2.5 s. See
  [Annotation views](docs/DRAWINGS.md#annotation-views-schema-145-abi-71).
- Semantic PMI on persistent faces (schema 144, ABI 70). GD&T attachments
  may name the `faces` they annotate, and `export_step_pmi` writes a
  drawing's datums, tolerances (per composite segment, one per n× pattern)
  and size limits as AP242 semantic GD&T on those faces of each part. It then
  reads the file back to verify each item, and reports what the format cannot
  represent, what was lost and what was skipped. `occt-model` takes
  `step_pmi`. OCCT's writer drops projected zones (reported as lost), keeps
  one datum per label per file, and writes no saved views. A 400-hole plate's
  400 controls and sizes export and verify in one pass. Face and edge
  selectors now fetch subshapes once rather than one index at a time
  (O(n), was O(n²)), and `NearestCenter` and `AtExtreme` share each result's
  subshape centers within a regeneration or selection context. 400 named
  holes resolved in 23 s before the fix and resolve in 0.05 s after. See
  [Semantic PMI](docs/DRAWINGS.md#semantic-pmi-on-persistent-faces-schema-144-abi-70).
- Simulated measurement uncertainty (schema 143 and ABI 69 unchanged). An
  inspection record's uncertainty may carry a `simulation`: every measured
  point is perturbed by seeded normal probing error, the record is
  evaluated again for each trial, and each control's U is the coverage
  factor times its deviation's standard deviation. It is combined in
  quadrature with any supplied U and applied by the decision rules. 1,000
  position controls and a 10,000-point flatness over 32 trials take 1.7 s
  (3 s budget; noisy trial points take the fits about 2.5 times the
  pivots of exact ones).

- Free state, statistical, continuous feature, dynamic profile and
  translation modifiers (schema 143, ABI 69 unchanged). Controls carry a
  `modifiers` list (Ⓕ, ⟨ST⟩, CF, △) and datum references a `translation`
  flag (▷), all drawn in the frame. A dynamic profile zone offsets
  uniformly in the fit, and statistical controls are reported as judged on
  process data. ▷ frees a tertiary width (three-plane frames) or a
  secondary hole or pin (plane-axis frames, with the tertiary face then
  locating) from its basic location. 10,000 composite surface profiles
  with dynamic upper zones evaluate in 0.19 s (1 s budget).

- Projected tolerance zones and tangent planes (schema 142, ABI 69
  unchanged). Position of a cylindrical feature may carry Ⓟ with a height
  and a starting plane: the unrelated mating envelope's axis is extended
  beyond the part and checked where it enters and leaves the zone, through
  datum shift and composite fits. Orientation of a plane may carry Ⓣ,
  judging only the plane on the surface's high points. Both draw in the
  frame. 10,000 position controls with projected zones evaluate in 0.28 s
  (1 s budget).

- Simultaneous common datum axis simulators (schema 141 and ABI 69
  unchanged). A common datum axis A-B is now the axis of coaxial cylinders
  contracting about shafts, or expanding inside holes, at one rate from
  each member's own envelope. This replaces the line through the members'
  envelope-axis midpoints, which now only breaks ties. One minimax LP over
  both members' points with a 1e-6 pull toward that line. 1,000 circular
  plus 1,000 total runout controls against 100 common axes from 200
  journals take 0.060 s (1 s budget).

- Composite controls beyond two segments (schema 141, ABI 69 unchanged). A
  composite refinement may hold a further `refinement`, to any depth; each
  segment must be tighter than the one above it, and its datums a prefix of
  the upper segment's. Frames draw one row per segment under one
  characteristic cell. Composite position and profile fit each lower
  segment on its own, and the report nests each segment's result in the one
  above. 10,000 three-segment composite position controls in 100 patterns
  evaluate in 0.29 s (1 s budget).

- Datum simulators at basic angles other than 90° (schema 140 and ABI 69
  unchanged). Secondary and tertiary plane datums keep their basic angles
  to the higher-precedence simulators: a bevel tilted to A can be the
  secondary, and a chamfer at 45° to B the tertiary. Nominal and measured
  three-plane frames accept any planes meeting in a point, orientation
  zones use the secondary's part square to A, and a width tilted to A at a
  material boundary is reported. 10,000 position controls against a frame
  whose secondary is a 45° bevel evaluate in 0.22 s (1 s budget).

- Position at an angle to a primary datum axis (schema 140 and ABI 69
  unchanged). Cross holes and inclined holes located to a primary datum
  axis and a stopping face are measured against a zone at their basic angle,
  and a tilt of the axis at MMB/LMB raises or lowers them along it. Without
  a clocking datum the frame turns freely about the axis, shared by controls
  with the same datum references, so bolt circles and cross-hole patterns
  located only to the axis are evaluated. 10,000 radial holes around a bore
  at MMB share one turn in 0.71 s (1 s budget).

- Common datum axis shift (schema 140 and ABI 69 unchanged). A common datum
  axis A-B referenced at MMB or LMB shifts and tilts: each member uses its
  own envelope and clearance, and the axis must pass within that clearance
  of the member's axis at both ends of its measured length. Coaxial and
  clocked position controls share one motion. 10,000 coaxial controls
  between two datum bores at MMB share one shift in 0.24 s (0.5 s budget).

- Clocking and rotational shift from a hole or pin (schema 140 and ABI 69
  unchanged). A tertiary hole or pin clocks a plane-axis frame A | B | C, and
  a hole or pin parallel to a primary datum axis clocks that axis's frame:
  x runs from the locating axis toward the hole's mating axis. At MMB/LMB
  its boundary cylinder, at its basic place, moves with the frame and stays
  within half its departure of the measured axis, so the frame can turn,
  together with any shift of the bore. 10,000 controls located from a bore
  and a dowel hole both at MMB share one translation and rotation in 0.18 s
  (0.5 s budget).

- Off-axis position from a primary datum axis (schema 140 and ABI 69
  unchanged). A primary datum axis may be followed by a face perpendicular to
  it (a stop that sets the origin along the axis) and a plane or slot/tab
  width parallel to it (a clock that turns only about the axis), and the
  measured B | A | C frame is reported. Features parallel to the axis are
  located from their mating envelope axis between their measured ends, so a
  lean counts by its worse end. With the bore at MMB/LMB its axis ends float
  as before, and a clocking width at MMB/LMB turns the frame about the axis
  with its boundary pair moving with the axis; one motion is shared under the
  simultaneous requirement. 10,000 controls off a bore at MMB clocked by a
  slot at MMB share one shift and turn in 0.24 s (0.5 s budget).

- Primary datum axis position and shift (schema 140 and ABI 69 unchanged).
  Position controls on features coaxial with a primary datum axis (or common
  axis) are evaluated from the feature's mating envelope axis between its
  measured ends, with MMC/LMC bonus. With the datum bore or shaft at MMB or
  LMB, its simulator axis shifts and tilts: each end, at the datum feature's
  measured extent, floats within half its departure from the boundary, and
  one shift is shared under the simultaneous requirement. Off-axis features
  are reported. 10,000 coaxial controls sharing one primary-axis MMB shift in
  0.23 s (0.5 s budget).

- Width and rotational datum shift (schema 140 and ABI 69 unchanged). Slot
  and tab width datum features referenced at MMB or LMB shift the frame: the
  boundary pair of planes, carried with the frame, must stay clear of every
  measured face point. A tertiary slot or tab after a primary plane and a
  bore lets the frame turn about the bore, together with the bore's own MMB
  translation; secondary and tertiary widths in three-plane frames move the
  origin along their normals, and a shifting secondary width turns the frame.
  One translation and rotation is shared under the simultaneous requirement
  (a relinearized min-max LP, also used for composite pattern fits), and
  controls report `datum_shift_rotation_rad`. 10,000 controls sharing a bore
  and slot MMB shift and rotation in 0.17 s (0.5 s budget).

- Measurement uncertainty and decision rules (schema 140 and ABI 69
  unchanged). Inspection records take an expanded uncertainty U, per-control
  overrides and a rule: simple acceptance (default), guarded acceptance or
  guarded rejection (ISO 14253-1, ASME B89.7.3.1). Controls report the U
  applied and whether either segment is indeterminate (within U of its
  limit); the inspection report command counts them. 10,000 composite
  controls decided under guarded acceptance in 0.18 s (1 s budget).

- Runout of cones and other surfaces of revolution (schema 140 and ABI 69
  unchanged). Runout controls accept nominal points, from the record or a
  `surface` selector projected onto the part, and then read each point
  square to its nominal surface, with the nominal geometry placed on the
  measured datum axis. Circles group by nominal axial position and radius.
  1,000 circular plus 1,000 total runout controls on 30° cones (471,600
  points) in 0.046 s (1 s budget).

- Profile of a line (schema 140 and ABI 69 unchanged). Line elements are
  sections normal to the control's view direction; each is fitted alone with
  the surface-profile zone, its free motions narrowed to its plane, for single
  and composite controls. Reports count the elements. One line profile of
  1,000,000 points in 1,000 elements in 0.20 s (1 s budget).

- Profile against the model's surfaces (schema 140, ABI 69). Profile
  controls may name their faces with a `surface` selector.
  `project_profile_nominals` regenerates each attachment instance once,
  selects those faces, and projects measured points (through the instance's
  placement) onto them with the new batched
  `occt_bridge_faces_project_points`, which returns nearest points, outward
  normals and distances, boundaries included. `evaluate_inspection_on_model`
  and the inspection report command use it, so records no longer need CMM
  nominals. 1,000 profiles projecting 1,000,000 points onto a cylinder in
  0.59 s (2 s budget).

- Surface profile evaluation (schema 139, ABI 68 unchanged). Measured
  points may carry CMM nominal points and outward normals; plane-anchored
  profiles may omit them. The nominal surface is placed on the measured datum
  simulators, and the rigid motions those datums leave free (all six without
  datums) are fitted by a relinearized active-set Chebyshev LP. n× profile
  patterns move as one simultaneous group, and composite profile refits with
  translation free and refinement datums orienting only. Profile controls take
  an `unequal` (Ⓤ) disposition, validated and drawn. 10,000 composite
  profiles on 720,000 bore points in 100 patterns in 0.18 s (1 s budget); one
  1,000,000-point surface with six free motions in 0.30 s (2 s budget).

- Composite position over patterns (schema 138, ABI 68 unchanged).
  Feature-control frames take a `pattern` ID, so controls sharing it act as
  one n× control. Composite position is evaluated in two segments. The upper
  segment locates as any position control, with datum shift and the
  simultaneous requirement. The lower segment fits each pattern as one rigid
  group, translating and, unless a refinement plane fixes it, rotating, by an
  active-set min-max LP. Results report both segments and the pattern's fitted
  motion. The simulator cache now keys datum boundaries too, fixing MMB/RMB
  reuse of one fit. 10,000 composite controls in 100 patterns in 0.19 s (1 s
  budget).

- Slot and tab width datums (schema 137, no format change). A plane datum
  feature of size with a declared kind is simulated by the center plane of its
  actual mating envelope: the widest parallel planes between a slot's walls or
  the narrowest around a tab, orientation constrained by precedence. Slots
  clock A | B | C bore frames, and reports list each width's center plane and
  mating size. 10,000 position controls clocked by a slot in 0.16 s (1 s
  budget).

- Datum shift (schema 137, ABI 68 unchanged). Datum features declare
  `size_limits`, whose maximum or least material size is the boundary of an
  MMB or LMB reference. Position is measured in plane-axis frames (A face, B
  bore, optional C clocking), and B at MMB/LMB allows a translation of half
  its envelope's departure from the boundary. Position controls with identical
  datum references share one shift (simultaneous requirement), solved by an
  active-set min-max LP. Each control reports the applied shift and its limit.
  10,000 controls sharing one shift in 0.17 s (1 s budget).

- Common datums (schema 136, ABI 68 unchanged). A datum reference's
  `common_with` joins up to three nominally coplanar planes or coaxial axes
  into one compartment, drawn `A-B` and validated. Measured common planes use
  one constrained-L∞ simulator over all members' points. Common axes join the
  midpoints of each journal's mating-envelope axis (a CMM construction), and
  report both mating sizes. The simulator cache keys common datums by every
  member. 2,000 runout controls on 100 common axes from 200 journals in
  0.037 s (1 s budget).

- Cylindrical datum features of size and runout (schema 135, ABI 68
  unchanged). Datum features declare `size_kind`. A measured cylindrical datum
  is simulated by its actual mating envelope's axis: the largest inscribed
  cylinder for holes, the smallest circumscribed for shafts. The axis is free
  as primary, or held normal to a primary plane. Circular and total runout
  read full indicator movement about that axis, radially on coaxial surfaces
  and axially on perpendicular faces. Reports list measured datum axes and
  mating sizes. 1,000 circular plus 1,000 total runout controls on 100 fitted
  datum axes in 0.03 s (1 s budget).

- Standards-referenced sheet conventions (schema 134, ABI 68 unchanged).
  Optional sheet presets set text heights by role, from the DOD-STD-100D table
  for inch sheets and the Y14.2-2014 minimums for ISO sheets. They draw ISO 5457
  reference zones (50 mm fields from the centring axes, published field counts)
  or explicit equal grids, with `zone_at` lookups, and export ANSI Y14.2M
  0.7/0.35 mm line weights in SVG and DXF. Title wrapping and table cells use
  measured widths at the larger heights. Example:
  `tools/drawing-export/sheet-conventions-example.json`. 1,000 sheets with all
  presets plus 100,000 zone lookups in 1.72 s (10 s budget). Material
  section-lining symbols, Y14.1 zone defaults and dimension spacing remain
  open: no public normative source was found.

- Measured circularity and cylindricity (schema 133 / ABI 68 unchanged).
  Inspection reports evaluate round-form controls about the anchor's nominal
  axis: circularity as the worst section's minimum-zone concentric circles
  with the center free, cylindricity as the minimum-zone coaxial cylinders
  with the axis free to shift and tilt. Both are regardless of feature size.
  1,000 cylindricity plus 1,000 circularity controls (612,000 points) in
  0.13 s (1 s budget).

- Kernel-free parametric core (schema 133, ABI 68 unchanged). The engineering
  layer is now two crates: `occt-parametric-core` (definitions, documents,
  expressions, sketch validation, placement solving, diff/merge, change
  impact, drawing/GD&T data, hole catalogs and validation; about 25k lines)
  builds and tests without OpenCASCADE, and `occt-parametric` regenerates
  core models with the kernel and re-exports the core API unchanged. Plain
  value types moved from `occt-bridge` into `occt-types`, which the bridge
  re-exports. Kernel operations on core types are extension traits, in scope
  through `use occt_parametric::*` or `occt_parametric::prelude::*`. The same
  655 parametric library tests pass (35 now in the core crate), and every
  model example builds and visualizes byte-identically to the previous
  release. Sketch solving stays in the kernel crate because spline and
  ellipse equations evaluate natively.

- Sketch milestone 5: AI sketch edit/repair workflow (schema 133 / ABI 68
  unchanged). Focused `sketch_feature` inspection isolates source geometry and
  returns constraint snapshots, solver/rank checks and bounded repair candidates.
  Typed guarded edits cover sketch entities, constraints and configuration;
  guarded named-reference replacements allow atomic geometry/reference repair.
  `occt_preview_edit_build` / `--preview-edit` creates diagnostic candidates
  without accepting geometry or recording revisions. Verified publication
  preserves source builds, requirements and semantic revision history. Example:
  `sketch-repair`; 74 examples, 13 schemas and 87 fixed MCP resources.
  Twenty complete MCP diagnose/preview/repair/publication cycles in 1.32 s
  (10 s budget).

- Sketch milestone 4: solver reliability (schema 133 / ABI 68 unchanged).
  Bounded adaptive recovery, safe rejection of invalid trial geometry, scaled
  objective comparison, local/representable finite differences and periodic
  angular derivatives preserve native contact behavior. Undamped equilibrated
  rank is cross-checked before strict acceptance. Named-point warm starts are
  opt-in through `solve_with_seed`; runtime reports and viewer annotations
  distinguish convergence, stationarity, failed descent and iteration limits.
  Example: `reliable-sketch`. Performance: 1,000 mixed-scale components in
  0.024 s; 1,000 cold/warm edit comparisons in 0.042 s, reducing warm solve
  iterations from 4,000 to 2,000 (10 s budgets).

- Sketch milestone 3: stable sketch references (schema 133, ABI 68 unchanged).
  Exact-overlap `sketch_curve` selectors retain authored/split lineage across
  boundary edits and native arrangement splits; optional complete coverage
  detects trimming. `sketch_region` witnesses and excluded points diagnose
  lost, ambiguous, split and merged region selections. Existing persistent
  references now follow geometry inputs instead of support-only dependencies,
  traversal retains descendants from all contributing branches, and copied
  prism bases record modified face/edge ancestry for distance/symmetric and
  face-limited extrusions. Example:
  `stable-sketch-references`, an attached boss, fusion and witnessed hollow.
  Performance: 100 reference-driven edits in 1.53 s; 1,000 exact boundary
  descendants selected and projected in 0.11 s (10 s budgets).

- Sketch milestone 2: geometry editing (schema 132, ABI 68). Named native
  split pieces feed profiles, discovery and later curve edits; bounded
  extend-to searches stop at native intersections. Seed-selected region
  fillets, chamfers and material offsets work on main/additional/discovered
  regions and their holes. Source constraints remain intact, previews expose
  names/source links, and rejected edits preserve accepted generations.
  Exact circular offsets avoid an OCCT 7.9 equal-radius-ellipse medial-axis
  crash. Example: `edited-regions`; 1,000 splits in 0.02 s and 200 part edits
  in 1.39 s (10 s budgets).
- Sketch milestone 1: region composition (schema 131, ABI 67 unchanged).
  `region_discovery.merge_adjacent` removes shared boundaries between selected
  cells, preserves excluded holes and disconnected/point-touching regions,
  and produces one extruded solid per connected material region. Source
  constraints remain intact. Example: `composed-regions`; 1,000 grid cells
  discovered, 999 merged into one holed face and extruded in 0.19 s.
- Automatic sketch regions (schema 130, ABI 67): native curves split at
  crossings into bounded cells selected by named interior points, or all
  cells without seeds. Native holes and exact curved boundaries feed face
  outputs and solids; the viewer shows selected boundaries and seed anchors.
  Example: `discovered-lens`. 1,000 circles discovered, 500 selected and
  extruded in 0.24 s; 300 lens edits in 0.66 s (10 s budgets).
- Sketch text (schema 129, ABI 66): `texts` turn single lines in strictly
  matched installed fonts into exact glyph faces (FreeType outlines,
  fontconfig lookup, nesting-based holes, kerning). Example:
  `engraved-nameplate`; 1,000 characters build and extrude in 5.1 s
  (10 s budget). Text input rejects malformed UTF-8 and control characters;
  the runtime enforces the 100-text sketch limit.
- Curvature continuity (schema 128, ABI 65): `curvature_continuous`
  makes curves tangent with equal curvature vectors at a shared end (native
  for splines via `occt_bridge_curve_curvature_at`). Example: `g2-paddle`
  (quintic blends, four G2 joints); 500 edits in 2.0 s (10 s budget).
- Sketch regions (schema 127): `regions` add disjoint outer boundaries
  with their own holes; the face output becomes a compound (overlaps found
  by a bounds sweep plus native commons) and extrude/revolve sweep each face.
  Example: `spacer-set`; 1,000 regions extrude into 1,001 solids in 0.79 s.
- Trims between bounds (schema 126, ABI 64): `trim_between` keeps a
  profile entity's span between its start/end, named points or native
  crossings with other curves (nearest a hint point); closed curves wrap
  through their seam (`occt_bridge_trim_curve_between`). Example:
  `keyhole-tag`; 500 neck edits regenerate in 1.52 s (10 s budget).
- Conic arcs (schema 125): the `conic` shape (start, control, end, `rho`)
  expands into an exact rational quadratic spline (elliptic below 0.5,
  parabola at 0.5, hyperbolic above) that inherits all spline constraints.
  `constraint_checks` and `measure` now expand shapes and patterns first.
  Example: `conic-nose` (revolved); 500 rho edits in 0.92 s (10 s budget).
- Elliptical arcs (schema 124, ABI 63): ellipses with named `start`/`end`
  become exact native trimmed-ellipse profile segments (curve-wire kind 4)
  with on-ellipse end equations, true-tangent `tangent`, span-checked
  membership, native lengths and mirrored copies. Example: `arched-panel`.
  500 rise edits regenerate in 0.62 s (10 s budget).
- Sketch shapes (schema 123): `rectangle` (axis or rotated), `slot` and
  regular `polygon` expand before patterns into named points, lines, arcs and
  existing constraints, two equations per generated point; shape IDs in
  profiles and holes stand for their boundaries. Example: `slotted-plate`.
  500 slot holes plus a 1,000-gon expand, solve and build in 1.13 s (10 s).
- Sketch dimensions (schema 122, ABI 62 unchanged): horizontal/vertical
  distances, line and arc angles, curve lengths (native adaptive length for
  ellipses and splines), equal curve lengths, collinear lines, coradial
  curves, and driven reference `measurements` that add no equations. Fixes
  `occt_bridge_shape_edge_length`, whose fixed-order Gauss integration was
  off by up to 1.6% on full ellipses. Example: `dimensioned-plate` (fully
  constrained, five reference dimensions). A 1,000-segment chain with 1,000
  measurements solves in 0.016 s; 200 length-driven splines in 0.046 s; the
  MCP renders 20 plates in 0.29 s (10 s budgets).
- Thickened sheets (schema 121, ABI 62): `thicken` closes faces or open
  shells into a solid along, against or across their normals with one
  native intersection-joined skin offset (two when symmetric); sharp edges
  are mitred. Sheet faces generate their offset faces and free edges their
  side walls. Example: `thickened-strap`, a pipe clamp strap swept from an
  open profile. Strap volumes match a closed form including the mitres to
  1e-9 for all three sides. A 400-face corrugated sheet
  thickens in 0.40 s (5 s budget); 500 strap thickness edits reusing the
  sheet take 7.97 s (15 s); the MCP renders 20 straps in 0.81 s (10 s).
- Split bodies (schema 120, no ABI change): a `split` feature cuts a solid
  with an editable plane and keeps the normal side, the other side, or both
  as a compound. Each side is one cut by a plane-aligned prism beyond the
  input's bounds, keeping input history; the plane must cross the input,
  checked from tight bounds of a rotated copy instead of volume integration.
  Example: `split-housing` (cap and base from two splits). A 400-hole plate
  splits through a hole column in 1.076 s (5 s budget); split-height edits
  cost about 31 ms each with the bored housing reused (500 edits, 30 s
  budget); the MCP renders 20 housings (40 pieces) in 2.94 s (10 s).
- Isolated line–spline touches (schema 119 / ABI 61 unchanged): rational
  Bernstein distance polynomials resolve stable repeated and endpoint roots,
  including a fourth-order flat contact. Derivative-chain partitions preserve
  nearby crossing pairs; finite line/spline bounds remain enforced. No sampling.
  Degree-squared evaluation work is capped at 1,000,000 units per query;
  collinear/uncertain contacts and exhausted budgets fail without handle leaks.
  Example: `projected-tangent-intersection-pad` drives a fully constrained pad
  and direct dimension from `reference:intersection:touch`. Width/height edits
  move the part; missing/ambiguous line offsets retain accepted geometry.
  Catalog: 58 examples, 70 resources, 154 request definitions; all 60 saved
  models remain at schema 119. Other conic/spline tangential regions remain
  an explicit limitation. A 1,000-alias touch build takes 0.006 s; 1,000
  width/height profile edits take 0.297 s; MCP renders 20 touch pads in 0.183 s
  (10 s budgets). All 43 advanced sketch and 44 MCP scale gates pass. Native
  exports and live failure preservation are verified.
- Native projected intersections (schema 119 unchanged, ABI 61): rational
  spline/chain and remaining ellipse pairs resolve isolated finite crossings
  from exact native curves. Analytic fast paths remain. Cached unordered pairs,
  10,000 span pairs/pair, 100,000 span pairs/sketch and bounded candidate lists
  limit work; overlaps/unresolved tangential regions reject explicitly.
  No sampled intersections or topology splitting. Tests verify contacts against
  both curves, trims, rational weights, placements, tilted planes, source edits,
  short C buffers, span budgets and handle cleanup. Viewer/MCP examples:
  `projected-spline-intersection-pad`, `projected-ellipse-intersection-pad`.
  Catalog: 57 examples, 69 resources, 154 request definitions; all 59 saved
  model examples remain at schema 119. Native C tests, full Rust tests and
  targeted clang-tidy/cppcheck/compiler checks pass. A 1,000-alias native pair
  takes 0.005 s; 1,000 width/weight edits take 0.291 s; 1,000 ellipse-pair edits
  take 0.263 s. MCP renders 20 native spline/ellipse pads in 0.184/0.179 s
  (10 s budgets); all 41 advanced sketch and 43 MCP gates pass. STEP/STL exports
  and live failure preservation are verified.
- Named projected intersections (schema 119, ABI 60 unchanged): finite
  line/circle/arc pairs and line–ellipse crossings generate fixed named points
  for constraints, dimensions and native part positioning. Unique sketch-plane
  hints reject missing/ambiguous crossings, overlaps and unsupported pairs.
  Pair caching and 1,000-per-projection/10,000-per-sketch limits bound work;
  hint-only edits invalidate the profile and preserve accepted parts on failure.
  Viewer/MCP annotations link both curves and both source controls. Native
  topology stays unchanged. Example: `projected-intersection-pad`; catalog:
  55 examples, 67 resources, 154 request definitions. All 57 saved model
  examples use schema 119. A 1,000-reference pair takes 0.005 s; 1,000
  driven profile regenerations take 0.236 s; MCP renders 20 pads in 0.188 s
  (10 s budgets). All 38 advanced sketch and 41 MCP scale gates pass.
- Named chain junction references (schema 118, ABI 60 unchanged): authored
  names resolve unique selected source-edge junctions within a bounded
  family-coordinate hint. Fixed sketch points drive constraints/dimensions
  independently of pole numbers and traversal seams. Hint/radius parameters
  invalidate projections; missing, ambiguous, invalid and duplicate references
  reject without leaking handles or replacing accepted geometry. Viewer/MCP
  annotations expose reference points and controls; composite topology stays
  unchanged. Example: `projected-junction-pad`. Catalog: 54 examples,
  66 resources; all saved model examples use schema 118. A 1,000-reference
  loop takes 0.360 s; 1,000 driven profile edits take 0.307 s; MCP renders
  20 annotated pads in 1.308 s (10 s budgets). All 36 advanced sketch gates
  pass. Live source/hint edits, recovery and STEP/STL export are verified.

- Connected multi-edge sketch projection (schema 117, ABI 60 unchanged):
  `kind: chain` joins a selected open chain or closed boundary into one
  rational B-spline, with exact mixed-degree elevation, deterministic
  source-space ordering and bounded endpoint matching. Disconnected,
  branched, ambiguous and over-limit selections reject; fixed references
  retain source controls and strict acceptance. Junctions are spline knots
  in one native edge, with topology limits documented. Example:
  `projected-chain-pad`. Catalog: 53 examples, 65 resources; all saved
  model examples use schema 117. Source edits, native area, failed-edit
  retention, STEP/STL export and viewer control polygons are verified.
  A 1,000-edge loop plus 1,001-edge rejection takes 0.332 s; 1,000
  mixed-degree profile edits take 0.432 s; MCP renders 20 tilted chain pads
  in 1.275 s (10 s budgets). All 34 advanced sketch gates pass.

- Exact spline sketch projections (schema 116, ABI 60 unchanged): `kind: spline`
  imports finite rational B-spline/Bézier and convertible conic edges using
  exact native Bézier spans, projected poles and normalized shared weights.
  Periodic and non-clamped active domains and trimmed ellipses are preserved
  geometrically; span parameterization is rebuilt. Stable fixed start/end
  references drive profile lines and point constraints; internal poles remain
  bounded runtime data. Source edits, locations, collapse rejection, duplicate
  IDs and failed incremental/live edits are verified, with handle cleanup.
  Example: `projected-spline-pad`. Catalog: 52 examples, 64 resources;
  all saved model examples use schema 116. 1,000 projected-profile weight edits
  take 0.850 s; 250 full pad builds take 4.139 s; MCP renders 20 linked tilted
  pads in 0.309 s (10 s budgets). All 32 advanced sketch gates pass.
  An exploratory 1,000-full-pad gate took 11.651 s and exceeded its 10 s
  budget; projection/profile work is measured separately from the established
  250-solid gate. Valid STEP/STL export and viewer control polygons are verified.

- Sketch acceptance policy (schema 115, ABI 60 unchanged): optional
  `fully_constrained` requires satisfied equations and zero local free degrees
  before native face/closed-wire/open-wire creation. The compatible default
  remains `solved`; diagnostic solves remain available. Viewer/MCP distinguish
  solver convergence from policy acceptance and expose a linked policy
  annotation. All points count, including construction geometry and linked
  copies. Example: `strict-sketch-block`; `lift = 0` demonstrates a converged
  singular sketch rejected by local rank. Catalog: 51 examples, 63 resources;
  every request example is schema 115. Migration, export, rejected live-edit
  retention and recovery are verified. Performance: 1,000 valid strict builds
  plus 1,000 free-motion rejections take 1.815 s / 10 s; MCP renders 20 strict
  sketches in 0.141 s / 10 s. All 30 advanced sketch gates pass.

- Local sketch solver explanations (schema 114 / ABI 60 unchanged): final
  Jacobian analysis exposes locally redundant constraints, satisfied fixed
  geometry, related conflicting equation groups and coupled free-motion
  directions. Viewer/MCP annotations link evidence to geometry and controls.
  Stationary derivatives, partial dependency traces, numerical rank differences
  and bounded output are explicit; solver acceptance remains unchanged.
  Example: `sketch-diagnosis`. Catalog: 50 examples, 62 resources.
  10,000 independent duplicate-equation diagnoses take 0.034 s;
  a 5,000-point coupled chain takes 0.008 s; 10,000 fixed conflicts take
  0.013 s (10 s budgets). All 29 advanced sketch gates pass, including
  the 1,000-copy circular pattern with coupled motion and native holes.
  MCP renders 20 diagnosed sketches in 0.144 s / 10 s. Live conflict edits
  preserve accepted geometry, and STEP/STL export remains valid.

- Thread runout (schema 114, no ABI change): optional `runout` on threads
  and modeled tapped holes fades the groove out of the material over up to
  ten pitches, along a tangent interpolated spiral joined to the helix and
  swept once. A two-pitch runout removes 37% (external) / 34% (internal) of
  two full turns, equal for both hands; the last runout pitch removes 8% of
  the first. A 100-turn thread with a ten-pitch runout cuts in 4.198 s
  (12 s budget, +0.17 s over no runout). The tapped-block example's blind tap
  has a 2 mm runout; the MCP renders 10 tapped blocks in 15.9 s (30 s budget).

- Modeled tapped holes (schema 113, no ABI change): a hole's thread callout
  with `modeled` cuts an ISO 68-1 basic internal thread from the hole entry,
  fused into the hole cutter so one cut keeps input history. Tap drills must
  be at least the basic minor diameter; blind threads stop half a pitch
  above the bore floor; countersinks act as entry chamfers. Example:
  `tapped-block`. Catalog: 49 examples, 61 resources. A 100-turn M10×1.5 tap
  cuts in 3.644 s (12 s budget); the MCP renders 10 tapped blocks in 14.3 s
  (30 s budget). Thread runout follows in schema 114.

- Sketch patterns and holes (schema 112 / ABI 60): linked mirror, linear and
  circular copies of sketch entities as solver unknowns with rigid link
  equations; mirrored copies reverse traversal so half outlines close through
  shared on-axis points. Sketch faces take inner `holes`, built by the new
  native `create_face_with_holes` (no booleans; strict containment, 1e-7 mm
  clearance, no nesting, box-filtered pairs). Example: `mirrored-flange`.
  Catalog: 48 examples, 60 resources. 1,000 flange profiles take 2.17 s, a
  1,000-instance circular pattern solves and cuts its holes in 2.07 s, and
  the MCP renders 20 flanges in 0.90 s (10 s budgets). Existing example
  output is byte-identical apart from empty `holes`/`patterns` scene keys.

- Spline and ellipse sketch corner neighbors (schema 111 / ABI 59): fillets
  and chamfers at corners touching splines or trimmed ellipses, on open and
  closed profiles. Fillets keep only the native iterative solver's arc and
  trim the source curves at its tangent points; minor-arc, radius, tangency
  and bounded-shrink checks reject wrong-tip lens solutions and mid-span
  circles. Line/arc corners are unchanged (open-corner pipe output is
  byte-identical). Example: `spline-corner-pad`. Catalog: 47 examples,
  59 resources. 1,000 pad profile regenerations take 1.287 s and the MCP
  renders 20 annotated pads in 0.268 s (10 s budgets).

- Open-profile sketch fillets and chamfers (schema 110 / ABI 58): internal
  line/arc corners use analytical native fillets or arc-length trims and a
  chamfer edge. Remote and open endpoints stay fixed, trimmed neighbors stay
  within source bounds, and the result remains a valid connected open wire.
  Native tests verify orientation, circular arcs, excluded endpoint selections,
  consumed/oversized edges, handle cleanup, rotated planes and small/large
  distant geometry. The viewer preserves source constraints and linked corner
  labels. The pipe example verifies path length, swept volume, section/path
  reuse and failed-edit retention. Catalog: 46 examples, 12 schemas,
  58 resources; request schema: 149 `$defs`. A 1,000-path native edit
  gate passes in 0.207 s; 250 complete pipe builds pass in 2.929 s
  (10 s budgets). MCP renders 20 annotated pipes in 1.657 s / 10 s.
  An exploratory 1,000-pipe run took 11.776 s; the solid gate uses the
  established 250-sweep size, with path editing measured independently.

- Native sketch profile fillets and chamfers (schema 109 / ABI 57): named
  remaining corners on closed line/arc profiles use OpenCascade's planar
  editor. Positive radius and asymmetric incoming/outgoing distances are
  parameterized; arcs use curvilinear distance. Concave corners, reversed
  traversal, rotated planes, immutable inputs, handle cleanup, size failures
  and unsupported geometry are verified. Trims/extensions precede corner
  edits, which precede offsets. The viewer labels source corners, links size
  controls and renders the derived boundary or native error. The plate example
  verifies area/volume, size edits, depth-only profile reuse and failed-edit
  retention. Catalog: 45 examples, 12 schemas, 57 resources; request schema:
  149 `$defs`. A 1,000-plate native radius-edit gate passes in 3.425 s;
  MCP renders 20 annotated plates in 0.236 s (10 s budgets). Open-profile edits follow in schema 110; spline/ellipse corners in schema 111.

- Independent ellipse–ellipse tangency (schema 108; native ABI 56 unchanged):
  two distinct full ellipses share a seeded native contact and tangent alignment.
  Sparse dependencies include both sets of defining points. External/internal
  contacts, independent rotations, equal axes, seams, reversed role order,
  fixed conflicts and non-tangent shared positions are verified. The viewer
  exposes both measured contacts and related dimension controls. The solid
  example verifies native volume/bounds, axes/rotation edits, height-only
  profile reuse and failed-edit retention. Catalog: 44 examples, 12 schemas,
  56 resources; request schema: 149 `$defs`. A 1,000-solid native axis-edit
  gate passes in 1.837 s; MCP renders 20 annotated pairs in 0.252 s
  (10 s budgets).

- Independent ellipse–circle/arc tangency (schema 107; native ABI 56 unchanged):
  measured native ellipse contacts and derivatives share circular contact and
  directed arc-span equations with spline-circle tangency. External/internal
  contacts, rotated axes, editable radius, fixed conflicts, non-tangent
  crossings, arc endpoints and excluded spans are verified. The viewer links
  ellipse dimensions and circle radius controls. The new circular boss example
  verifies native volume/bounds, placement edits, height-only profile reuse
  and failed-edit retention. Catalog: 43 examples, 12 schemas, 55 resources;
  request schema: 149 `$defs`. A 1,000-solid native axis-edit gate
  passes in 1.202 s; MCP renders 20 annotated bosses in 0.191 s
  (10 s budgets).

- Independent spline–ellipse tangency (schema 106; native ABI 56 unchanged):
  bounded native closest points and derivatives drive four positional equations
  and a dimensionless tangent equation. Sparse dependencies include ellipse axes
  and spline poles. The viewer exposes measured contacts and related dimension
  controls. The new elliptical solid example verifies weight, axes, rotation,
  native volume/bounds, profile reuse and failed-edit retention. Catalog: 42
  examples, 12 schemas, 54 resources; request schema: 149 `$defs`.
  A 1,000-solid native weight-edit gate passes in 1.747 s; MCP renders
  20 annotated elliptical solids in 0.248 s (10 s budgets).

- Independent spline–spline tangency (schema 105; native ABI 56 unchanged):
  a seeded point belongs to two distinct bounded native splines and their
  native tangent directions align. Sparse equations touch both sets of
  poles/interpolation points and existing tangent neighbours. Rational weight
  edits, all spline modes, opposite directions, bounded endpoints, free-pole
  movement, fixed conflicts and non-tangent shared positions are tested.
  Viewer anchors show both native contacts, with separate positional and
  tangent residuals and indexed links to coordinate, dimension and weight
  controls. The spline-tangent-pair example verifies native cap volume/bounds,
  weight/size edits, narrow/steep geometry, height-only profile reuse and
  failed-edit retention. The catalog has 41 examples, 12 schemas and 53
  resources, with 149 request definitions. A 1,000-pair native build/edit gate
  passes in 2.380 s; MCP renders 20 annotated pairs in 0.199 s (10 s budgets).
  Spline–ellipse contacts follow in schema 106; ellipse–circle contacts follow in schema 107; ellipse–ellipse contacts follow in schema 108.

- Independent spline–circle/arc tangency (schema 104; native ABI 56 unchanged):
  a seeded contact slides along the bounded native spline and satisfies the
  supporting circular radius and tangent alignment. Directed arcs add a
  separate domain residual. Sparse dependencies include poles/interpolation
  points, tangent neighbours, circular defining points and the contact point.
  All spline modes, both local contact branches, free-pole movement, excluded
  arc contacts, wrapped spans, endpoint contacts and fixed conflicts are tested.
  Viewer anchors and separate position/tangent/span residuals expose failures,
  with indexed links to radius, coordinate and weight controls. The
  spline-tangent-boss example verifies native volume and placement, radius,
  rise and rational-weight edits, height-only profile reuse and failed-edit
  retention. The catalog has 40 examples, 12 schemas and 52 resources, with
  149 request definitions. Native and MCP gates pass 1,000 bosses in 1.235 s
  and 20 annotated bosses in 0.180 s (10 s budgets). Spline–spline tangency
  follows in schema 105; spline–ellipse contact follows in schema 106.

- Independent line–spline tangency (schema 103 / ABI 56): a seeded contact
  point slides along the bounded native spline and lies on the supporting
  line, with measured tangent alignment. The native closest-point/derivative
  query includes endpoints, location and orientation; zero tangents and C0
  knot contacts are rejected without storing shape handles. Sparse equations
  include line endpoints, the contact point, poles/interpolation points and
  existing tangent neighbours. Clamped rational, non-clamped, periodic and
  interpolated curves, moving free poles, conflicts and bounded exclusions
  are tested. Viewer anchors show the seed, native contact and line projection,
  with separate millimetre position and dimensionless tangent residuals.
  The spline-tangent-cap example verifies native volume/bounds, weight and
  size edits, height-only profile reuse and failed-edit retention. The catalog
  has 39 examples, 12 schemas and 51 resources, with 149 request definitions.
  Native and MCP performance gates pass 1,000 caps in 1.795 s and 20 annotated
  caps in 0.179 s (10 s budgets). Rebuild native and Rust consumers together.
  Spline–circle contacts follow in schema 104, spline–spline in schema 105,
  and spline–ellipse in schema 106.

- Independent line–ellipse tangency (schema 102; native ABI 55 unchanged):
  signed side selection uses the exact support of the rotated native ellipse.
  The sparse equation touches both line endpoints and all three ellipse
  defining points. Contacts lie on the supporting line, including outside its
  finite segment; viewer anchors show the ellipse contact and its projection,
  measured millimetre residuals and fixed-geometry conflicts. Free axis points
  link to their distance/angle controls. The ellipse-tangent-pad example
  verifies native area/volume/bounds, axis and rotation edits, and profile reuse
  on height edits. The catalog has 38 examples, 12 schemas and 50 resources,
  with 149 request definitions. Native and MCP performance gates exercise
  1,000 native pad builds in 0.739 s and 20 annotated pads in 0.181 s
  (10 s budgets).
  Line–spline contacts follow in schema 103; independent curve–curve contacts
  follow in schemas 104–108.

- Non-clamped explicit B-spline bases (schema 101 / ABI 55): named geometric
  endpoints are separate from control poles and constrained by four native
  endpoint equations. Endpoint tangency uses native derivatives with sparse
  dependencies on all poles. Lines and arcs connect at actual native vertices;
  closed non-periodic loops can share one endpoint ID. Viewer endpoint links
  report measured residuals, including fixed-point conflicts. The new cap
  example checks analytical volume, active-domain bounds, mixed profiles,
  size/weight edits, profile reuse on height edits and failed-edit retention.
  The catalog has 37 examples, 12 schemas and 49 resources, with 149 request
  definitions. Rebuild native and Rust consumers together. Native and MCP
  performance gates pass 1,000 rational cap builds in 1.306 s and 20 annotated
  caps in 0.169 s (10 s budgets).
  Independent general curve contact remains open.

- Periodic explicit B-spline bases (schema 100 / ABI 54): periodic knot/pole
  count rules, positive weights and native closure are checked. Curves are
  standalone closed profiles; their seams use native geometry instead of
  control-pole endpoints. Viewer control polygons close and expose periodic
  basis/weight metadata. The periodic-spline-pad AI example verifies a rounded
  cubic rectangle, native area/volume/bounds, seam tangent continuity, size and
  rational weight edits, and profile reuse on height edits. Closed loops reject
  free-end tangency and mixed/open profiles. Clamped/interpolated defaults
  remain. The catalog has 36 examples, 12 schemas and 48 resources, with 148
  request definitions at that increment. Non-clamped bases follow below;
  general contact remains open. A 1,000-profile native
  build/rational-edit gate passes in 1.921 s; MCP renders 20 periodic pads
  and closed control polygons in 0.253 s (10 s budgets).

- Explicit clamped B-spline bases (schema 99 / ABI 53): spline entries may
  provide degree, distinct knots, multiplicities and positive dimensionless
  weights, making their named points control poles. A native segment kind
  constructs the supplied basis directly, with bounded sizes and validation;
  existing interpolation stays the default. Point membership uses the native
  rational curve; endpoint tangency constrains adjacent control-pole direction
  with measured residuals. Weight dependencies trigger profile/solid rebuilds.
  Viewer metadata, linked controls and dashed control polygons are implemented.
  The rational-spline-cap AI example verifies quarter-circle and polynomial
  quadratic volumes, exact native weights, internal knots, endpoint relations
  and incremental edits. The catalog has 35 examples / 12 schemas / 47
  resources, with 148 request definitions. Rebuild native/Rust consumers
  together. A 1,000-profile native build/weight-change gate passes in 1.221 s;
  MCP renders 20 rational caps and basis controls in 0.148 s (10 s budgets).
  Periodic and non-clamped bases follow in subsequent increments; general
  tangency remains open.

- Directed arc tangency (schema 98; native ABI 52 unchanged): independent
  line/circular tangencies accept arcs and require their calculated contacts
  inside each directed span. Supporting-circle residuals are accompanied by
  millimetre span checks; sparse derivatives include the arc end points.
  Clockwise, wraparound, endpoint, external/internal and excluded-contact cases
  pass, including free endpoint solving and viewer failure status when only
  supporting circles touch. The AI arc-tangent-boss example extrudes an exact
  arc-and-chord segment and verifies radius/reference/height edits and native
  volume/bounds. The catalog has 34 examples and 12 schemas (46 resources),
  147 request definitions, targeting schema 98. Ellipse/general-curve tangency
  remains open. A 1,000-component arc tangent solve/diagnostics gate passes
  in 0.064 s; MCP renders 20 exact segment bosses with span diagnostics in
  0.155 s (both 10 s budgets).

- Independent sketch tangency (schema 97; native ABI 52 unchanged): full circles
  can be tangent to supporting lines on an explicit left/right side, or to
  other circles in external/ordered-internal mode, without shared point IDs.
  Sparse millimetre equations, rank/residual diagnostics, checked references
  and degeneracy rejection are implemented. Viewer contact anchors show
  `T LEFT`, `T RIGHT`, `T EXT` and `T INT`; endpoint/spline tangency is preserved.
  The AI tangent-boss example derives its position from two relations, builds
  a native profile/solid, and verifies radius, reference and height edits.
  The catalog has 33 examples and 12 schemas (45 resources); request schemas
  have 147 definitions. At that increment, catalog examples target schema 97;
  arc-span and ellipse tangencies follow in schemas 98 and 102–108. A 1,000-component sparse
  solve/diagnostics gate passes in 0.027 s; MCP renders 20 tangent bosses and
  contact annotations in 0.152 s (both 10 s budgets).

- Signed point-to-line sketch dimensions (schema 96; native ABI 52 unchanged):
  one perpendicular millimetre equation over a point and the named line's
  endpoints, with positive-left/negative-right orientation and zero support.
  Sparse solving, units/reference/degeneracy validation, rank/residual
  diagnostics, target parameter dependencies and perpendicular viewer anchors
  are implemented. The projected-pocket AI example drives its cut clearance
  directly from the imported edge; margin edits rebuild the profile and cut
  while reusing the source block. Tilted, reversed, zero, negative, extension,
  underconstrained and fixed-conflict cases pass. A 1,000-dimension tilted-line
  solve/diagnostics gate passes in 0.008 s; MCP renders 20 projected pockets
  with signed dimensions in 0.239 s (both 10 s budgets). Catalog examples
  target schema 96.

- Midpoint and concentric sketch relations (schema 95; native ABI 52 unchanged):
  midpoint contributes two coordinate equations over a point and line endpoints;
  concentric contributes two centre equations for circles, arcs or ellipses.
  Checked references, sparse derivatives, real residuals and `MID`/`CONC` viewer
  annotations are implemented. The projected pocket uses midpoint; a new AI
  bushing example drives an annular body and checks centre, bore and height
  edits against native bounds, radius and volume. The catalog has 32 examples
  and 12 schemas (44 resources), all targeting schema 95. A 1,000-component
  solve/diagnostics gate passes in 0.011 s; MCP renders 20 bushings and 40
  solved source sketches in 0.221 s (both 10 s budgets).

- Linked sketch edge projections (schema 94; native ABI 52 unchanged): named
  semantic source edges project into fixed line/circle/arc/ellipse references.
  Imported points can drive constraints or form profile boundaries. Native
  analytic coefficients and scaled conic axes preserve geometry without
  sampled replacements; ambiguous, degenerate or unsupported imports fail.
  Source/named/history dependencies rebuild sketches and solids. The pocket
  example follows block-depth edits without copying depth into sketch-point
  expressions. Teal references and source-control annotations are shared by
  live and standalone viewers. Native geometry, area/volume, named placed
  references, conflicts, large-scale precision, migration, handle cleanup,
  live edits and STEP/STL exports pass. A 1,000-sketch generation/snapshot/edit
  gate passes in 0.936 s; MCP renders 20 bodies, 20 profiles and 20 source
  sketches in 0.242 s (both 10 s budgets). Catalog examples now use schema 94.
  See [sketch projections](docs/SKETCH_PROJECTIONS.md).

- Face-attached sketch definitions (schema 93; native ABI 52 unchanged):
  semantic selection requires one planar face, whose area centre and oriented
  normal resolve the sketch plane with a signed length offset and explicit
  tangent X axis. Source/selector/history dependencies and named references
  enter ordering and incremental signatures. The pocket example follows block
  height through its profile and cutter; ambiguous, nonplanar and missing
  supports fail while accepted geometry is retained. Native faces and local
  sketch support metadata share one diagnostic generation. Tests cover height
  and offset edits, pocket volume, translated/rotated persistent supports,
  invalid axes/units, dependency failures, live edit/revert and STEP/STL.
  A 1,000-profile generation/query/edit gate passes in 0.694 s; MCP renders
  20 pockets, 20 attached profile faces and 20 local sketches in 0.231 s
  (both 10 s budgets). Catalog examples now use schema 93.
  See [face-attached sketches](docs/FACE_SKETCHES.md).

- Equal-radius sketch/solid definitions (schema 92; native ABI 52 unchanged):
  circle/circle, circle/arc and arc/arc constraints compare supporting radii
  through one sparse residual equation. Reference validation, native arc
  geometry, conflict diagnostics and `=R` viewer labels are covered. The AI
  plate example retains source equations and boundary IDs through a holed
  planar region and extrusion; edits update both bores and reuse the outer
  sketch. Analytical volume, selective rebuilds, failed edits, migration,
  live edit/revert and STEP/STL exports pass. A 1,000-pair solver/diagnostic
  gate passes in 0.009 s; MCP generates 20 solids and 60 source sketches with
  matched-radius annotations in 0.316 s (both 10 s budgets). Catalog examples
  now use schema 92, with 29 examples and 12 generated schemas.
  See [equal circular radii](docs/SKETCHES.md#equal-circular-radii-schema-92).

- Upright drawing dimension layout (schema 91 / ABI 52 unchanged): shared
  SVG/DXF text components and estimated extents centre labels and keep them
  clear of linear dimension lines, radial leader extensions and angular arc
  tangents. Both tolerance rows, unit suffixes and basic frames participate;
  linear offset signs choose the label side and spacing stays in paper mm.
  Character-aware advances account for wide unit letters. Tests cover stacked
  tolerances, both offset sides, oblique/basic frames, angular clearance, view
  scales and exports. The 10,000 mixed-dimension gate passes in 0.111 s
  (10 s budget), checking basic-frame clearance and released handles.
  Font metrics, other annotation/geometry collisions and standards verification
  remain separate work. See [drawing layout](docs/DRAWINGS.md#upright-dimension-label-layout).

- Variable-fillet contour-law controls (schema 91 / ABI 52 unchanged): direct
  outputs show complete evaluated endpoint/interior radii and spine direction,
  with linked endpoint and station controls. Source-edge references stay bounded;
  radius labels explicitly carry nonspatial station metadata rather than
  inferring native contour positions from individual selected edges. The AI
  example passes validity, changed-volume, selective rebuild, rejected-edit
  retention, MCP, live edit/revert and STEP/STL checks. The scale gate passes
  20 scenes in 0.847 s (10 s budget).
  See [variable laws](docs/EDGE_TREATMENTS.md#variable-radius-contour-laws).

- Linked constant fillet/chamfer dimensions (schema 91 / ABI 52 unchanged):
  direct outputs show nominal radius/distance, unique selected-edge count and
  linked controls. Bounded overlays identify original source edges, explicitly
  distinguished from final blend measurements. The AI example compares rounded
  and beveled blocks; analytical volumes, selective rebuilds, failed-edit
  retention, MCP discovery, live edit/revert and STEP/STL exports pass. The MCP
  scale gate passes 20 scenes in 0.209 s (10 s budget).
  See [edge treatments](docs/EDGE_TREATMENTS.md).

- AI threaded-rod example and thread dimensions (current schema 91 / ABI 52
  unchanged): the catalog exposes modeled threads with linked major diameter,
  pitch, run and derived turns. Run anchors follow evaluated origin/axis;
  metadata identifies mode and hand. Source rod dimensions follow diameter/run
  edits. Core/model/MCP checks cover native geometry, removed volume, bounds,
  anchors, controls and failed-edit retention. Examples now match schema 91's
  authoring schemas, retaining drawing-release and nested-parts-list changes.
  The MCP scale gate passes: 10 thread scenes in 2.451 s (30 s budget).
  See [threads](docs/THREADS.md#ai-example-and-viewer-dimensions).

- Retained compound/pattern ancestry (schema 89 / ABI 52 unchanged): native
  groups union child histories with indexed, deduplicated target sets, keeping
  unchanged identity and branch deletion state. Nested patterns carry source
  group history through placements; model Cut/Fuse/Common compose directly
  grouped operands. Original cutter-face selectors resolve all nine grid bores
  or six bolt-circle bores, including through the MCP inspection interface.
  Bridge/core tests cover target membership, generated/modified ancestry,
  surviving children, Boolean composition and cleanup. Updated 10,000-member
  gates verify source-face counterpart counts: linear 8.124 s, radial 4.274 s,
  explicit grouping 8.533 s (15 s budgets). Targeted native lint passes.
  See [pattern ancestry](docs/PART_PATTERNS.md#retained-pattern-ancestry).

- Circular part patterns (schema 89; native ABI 52 unchanged): `CircularPattern`
  rotates shared source geometry at absolute signed angular steps into an
  unfused compound. Count includes the seed; first-to-last sweep stays below a
  full turn. Shared bounds limit copy count and nested topology. AI example
  `bolt-circle` derives spacing from count/sweep and cuts one tool group, with
  a geometric hole-separation guard. Viewer labels show count and angular
  spacing with source-radius arcs. Tests cover volume, reuse, signed partial
  turns, invalid inputs, duplicate wraparound, rollback and cleanup. Scale gates
  pass: 100 bolt-circle cuts in 0.946 s and 10,000 radial copies in 2.267 s
  (15 s budgets). The AI catalog exposes 38 operations and 25 examples.
  See [part patterns](docs/PART_PATTERNS.md#circular-patterns-schema-89).

- Linear part patterns (schema 88; native ABI 52 unchanged): `LinearPattern`
  repeats one source at absolute multiples of a length-valued step into an
  unfused compound. Integer-valued scalar count includes the seed, with up to
  10,000 copies. Located members share source geometry; a conservative topology
  budget bounds nested expansion before allocation. AI example `patterned-plate`
  creates rows/columns with two patterns and one cut. Linked viewer labels show
  count, step and placement span. Tests cover resized grids, volume, selective
  reuse, negative steps, invalid counts/units, overflow, rollback and cleanup.
  Scale gates pass: 100 grid cuts in 1.946 s and a 10,000-copy pattern
  in 7.241 s (15 s budgets). The AI catalog exposes 37 operations and 24 examples.
  See [part patterns](docs/PART_PATTERNS.md).

- Compound tool groups (schema 86; native ABI 52 unchanged): `Compound`
  groups 1–10,000 distinct input IDs without fusion or sewing. Child topology
  and placements remain intact; downstream cuts retain ancestry to original
  cutter faces. AI example `multi-hole-plate` cuts a nine-cylinder group in one
  Boolean operation. Radius, spacing and thickness controls regenerate the
  plate; group outputs show membership metadata. Tests cover volume, topology,
  child identity, edits, invalid input sets, rollback and cleanup. Scale gates
  pass: 100 nine-tool cuts in 1.461 s and a 10,000-member group in 7.303 s
  (15 s budgets). The AI catalog exposes 36 operations and 23 examples.
  See [compounds](docs/COMPOUNDS.md).

- Signed skin offsets (schema 85; native ABI 52 unchanged): `Offset` exposes
  native joined offsets with length-valued signed nonzero distance and positive
  tolerance. Native ancestry, validation and diagnostics are retained; edits
  reuse source geometry and failed changes retain accepted results. AI example
  `offset-part` and viewer annotations link distance/tolerance controls without
  inventing a wall-thickness measurement. Tests cover outward/inward geometry,
  mass, bounds, source history, units, rollback and cleanup. Scale gates pass:
  500 signed offsets in 0.246 s and 10,000 linked parts in 0.086 s
  (10 s budgets). The AI catalog exposes 35 operations and 22 examples.
  See [offsets](docs/OFFSETS.md).

- AI spring example and helix visualization (schema 83 / ABI 52 unchanged):
  `spring` combines an expression-driven section with a native helix sweep.
  Radius/pitch edits update section orientation; fractional turns resize the
  coil. Viewer labels expose radius, pitch, turns and axial rise with actual
  source-route anchors. Wire outputs render native lines without attempting
  surface triangulation. Helix routes and swept outlines sample per turn,
  charging global budgets before allocation and rejecting the native 100k
  point bound. Core, model, MCP and live HTTP checks cover geometry, volume,
  edits, overlays, guard rejection and revert. MCP scale gates pass: 10 spring
  scenes in 6.773 s (30 s budget), and 100 helix wire scenes in 0.278 s
  (10 s budget). The catalog now exposes 21 examples and 12 schemas.
  See [helices](docs/HELIX.md).

- Uniform geometry scaling (schema 83; native ABI 52 unchanged): `Scale`
  references a source, length-valued centre and positive dimensionless factor.
  Centre/factor edits rebuild only the transform and dependent features, with
  source geometry and modification ancestry retained. Native near-unit scales
  now copy geometry instead of being mistaken for rigid locations; copied
  transforms obey the session validation/healing policy. Viewer controls and AI
  example `scaled-part` expose centre and factor. Bridge, core, model and MCP
  tests cover units, small/large factors, mass, bounds, edits, rollback and
  cleanup. Scale gates pass: 500 resized parts in 2.433 s and 10,000 linked
  copies sharing one geometry variant in 0.426 s (10 s budgets).
  See [scaling](docs/SCALING.md).

- Plane mirrors (ABI 51, schema 81): native negative-determinant reflection
  creates handed geometry from a length-valued origin and dimensionless normal.
  Normal normalization handles extreme finite magnitudes, and source data stays
  unchanged. Native modification targets use actual output orientation, fixing
  face normals returned through history after reflection. Plane-only edits reuse
  source features; linked clones share mirrored geometry. Viewer controls and AI
  example `mirrored-part` expose plane position and tilt. Two bridge and two
  library tests, C argument checks and model/MCP checks cover geometry, mass,
  outward normals, ancestry, units, edits, inverse reflection and cleanup.
  Scale gates pass: 500 handed parts in 2.412 s and 10,000 linked parts with one
  geometry variant in 0.361 s (10 s budgets). See [mirrors](docs/MIRRORS.md).
- Explicit hollow profile lofts (schema 80; ABI 50 unchanged): optional
  `ProfileLoft::holes` defines up to 100 ordered inner section tracks. Matching
  station counts, distinct output IDs, coplanarity, containment and pair
  separation are validated; complete inner lofts are also checked for crossings
  and containment between stations. One native cut preserves outer/inner wall
  ancestry and one valid solid. Hole dependencies support selective rebuilds;
  old documents retain solid loft behavior. AI example `hollow-loft` keeps four
  source sketches and linked bore controls. Five library tests plus model/MCP
  checks cover volume, history, edit reuse, invalid tracks and migration. Native
  subtraction/history logic is shared with hollow sweeps. Scale gates pass:
  500 tapered ducts in 6.864 s and a 100-bore loft in 0.995 s (10 s budgets),
  with zero retained handles; hollow-sweep gates also pass after the refactor.
- Hollow path sweeps and retained subset history (ABI 50; schema 79 unchanged):
  holed planar faces sweep each native boundary under one transport rule, check
  containment/separation and cut the inner volumes. One valid solid retains
  inner/outer wall ancestry through composition and explicit history-preserving
  extraction. Planar regions now retain their cut face history as well. Native
  subset extraction filters targets, handles located ancestry and outlives parent
  handles. Three library tests, two bridge tests, C argument checks and model/MCP
  checks cover curved/straight modes, multiple bores, volume, source history,
  edit reuse and failures. AI example `hollow-sweep` keeps three source sketches
  and linked solid controls. Scale gates pass: 250 curved cases in 9.007 s and
  a 100-bore case in 0.567 s (10 s budgets); region gates also pass with retained
  history, and native handle counts return to zero. See [hollow sweeps](docs/HOLLOW_SWEEPS.md).
- Native hollow planar profiles (schema 79; ABI 49 unchanged):
  `PlanarRegion` combines an outer boundary with 1–100 strictly contained,
  disjoint inner boundaries. Native area/distance checks reject misplaced,
  noncoplanar, open, nested, crossing and touching inputs. One native face cut
  retains curved boundaries; its result feeds extrusion and revolution,
  including symmetric hollow toroidal parts. Independent sketch dependencies
  support selective rebuilds and retained accepted results. AI example
  `hollow-profile` keeps both sketch scenes and linked solid controls. Four
  library tests plus model/MCP checks cover topology, volume, source edges,
  failures and reuse. Scale gates pass: 500 hollow extrusions in 1.059 s and
  a 100-hole region in 0.140 s (10 s budgets), with zero retained handles.
  See [hollow profiles](docs/PLANAR_REGIONS.md).
- Symmetric sketch revolutions (schema 78; ABI 49 unchanged): optional
  `RevolveExtent::Symmetric` sweeps from minus half the signed angle to plus
  half about the source sketch plane. Location-based copies and composed native
  history preserve source-edge identities and immutable inputs. Angle and mode
  edits rebuild the solid while reusing its profile; old documents retain
  one-sided behavior. Viewer arcs match actual endpoints, and AI example
  `symmetric-revolve` exposes the mode. Three library tests plus model/MCP
  checks cover volume, centered bounds, history, reuse, failed edits and
  migration. The 1,000-case scale gate passes in 2.276 s (10 s budget),
  with zero retained handles. See [revolutions](docs/REVOLUTIONS.md).
- Revolve angle annotations (no ABI/schema change): direct revolved solid
  scenes expose signed right-hand sweep angles and linked controls, with arcs
  around the native axis through the profile area centroid. Face/wire inputs,
  shifted axes, non-unit axes, negative partial turns and full turns retain
  correct anchors. Both viewers and SVG snapshots render the same arc. Arc
  samples obey the global vertex budget. AI example `revolved-ring`, analytical
  volume tests, parameter/anchor checks and assembly rendering tests cover the
  sketch-to-turned-part path.
  The 100-scene release-scale gate passes in 0.871 s (10 s budget).
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
  See [Sketches](docs/SKETCHES.md).

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
  See [Drawings](docs/DRAWINGS.md#material-hatch-families-schema-69).

- Exact section-hatch intersections (no ABI/schema change): `exact_curves`
  now trims bounded batches of hatch lines against native cut faces, preserving
  curved/spline boundaries and holes without boundary sampling. Material unions,
  disconnected islands, actual face planes, detail windows, native resolution,
  work/vertex budgets and cleanup are covered by three tests with analytic circle
  and parabola oracles. The 1,000-part/10,000-segment case takes 8.773 s; the
  1,000-cylinder/40,000-segment case with both exports takes 7.754 s (30 s budgets),
  with one shared variant and all handles released. Legacy setups stay sampled.
  See [Drawings](docs/DRAWINGS.md#kernel-trimmed-hatching).

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
  See [Drawings](docs/DRAWINGS.md#exact-drawing-geometry-abi-46).

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
  See [Drawings](docs/DRAWINGS.md).

- Dimensional measurement checks: symmetric, signed-deviation and explicit-limit
  comparisons with unit-normalized nominal/measured values, signed deviations,
  margins and inclusive boundaries. Basic/reference/untoleranced dimensions
  return distinct dispositions without inferred acceptance limits. Saved batches
  resolve live projected lengths, radii, diameters, angles and hole-feature sizes;
  repeated measurements share cached nominal values. Four tests cover limits,
  units, rejection, paper independence, angular values and edited hole parameters.
  A benchmark checks 100,000 measurements across 10,000 saved dimensions in
  0.036 s (10 s budget), without kernel handles. Measurement acquisition, uncertainty/guard bands and geometric
  conformity remain future work. See [Drawings](docs/DRAWINGS.md).

- Fixed cylindrical position sample checks: single-row diameter controls with
  three RFS datum references, arbitrary nominal axis direction and supplied
  axis points in an established reference frame. Results report controlled-feature
  bonus, required diameter, margin and first worst-sample index. Named references
  resolve through saved drawing validation. Four tests cover exact boundaries,
  units, offset/tilted axes, material conditions, invalid inputs and named frames;
  a 100,000-sample benchmark runs in 0.003 s (10 s budget), using constant
  extra storage and no kernel handles.
  This entry point takes an already-established frame; fitted datums and
  envelopes come from measured GD&T inspection. See [Drawings](docs/DRAWINGS.md).

- Feature-size limits and bonus allowances (schema 68): persisted internal/external
  size limits, unit-aware MMC/LMC/RFS arithmetic, and independent total allowances
  for both position-composite rows. Out-of-limit sizes and invalid/overflowing
  values fail. Drawing exports preserve the specified control. Four tests cover
  material conditions, endpoints, units, rejection, persistence, migration and
  independent merges; a 10,000-allowance benchmark runs in 0.005 s
  (10 s budget) and uses no kernel handles.
  Mating envelopes fitted from measured points use these limits (see measured
  GD&T inspection below). See [Drawings](docs/DRAWINGS.md).

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
  and actual tolerance-zone inspection remain future work. See [Drawings](docs/DRAWINGS.md).

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
  inspection remain future work. See [Drawings](docs/DRAWINGS.md).

- Standard paper presets and projection symbols (schema 61): ANSI A–E and ISO
  A0–A4 in portrait/landscape, a bounded lower-right title block with drawing
  number, revision, scale, sheet numbering and metadata, and explicit first- or
  third-angle symbols. Presets override custom paper dimensions; legacy drawings
  retain their earlier frame. Three tests cover all 20 size/orientation pairs,
  symbol direction, paper-space sizing, exports, persistence, migration and budgets.
  A 1,000-sheet benchmark verifies shared regeneration and SVG/DXF exports in
  1.717 s (10 s budget). Views remain explicitly positioned; prescribed zones, approval and
  revision tables, lettering and a full standards-conformity audit remain future
  work. API and references: [Drawings](docs/DRAWINGS.md).

- Automatic section hatching (schema 60): saved paper-space angle, spacing and
  phase for Slice/Section views; holes retain clear interiors, overlapping
  components share a material union, and disconnected cut regions remain separate.
  Per-solid cutting preserves overlapping components in section outlines too.
  Detail clipping, work/vertex budgets and legacy migration are covered by six
  tests. SVG uses thin hatch strokes; DXF uses a SECTION_HATCH layer. A benchmark
  checks 10,000 hatch segments across 1,000 placed parts in 8.406 s (30 s budget).
  Sampled mode retains its earlier boundary approximation; exact intersections
  and explicit material maps are described above. Standards-verified presets
  remain future work. See [Drawings](docs/DRAWINGS.md).

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
  not certify ASME line weights or layouts. See [Drawings](docs/DRAWINGS.md).

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
  not a clause-by-clause ASME conformity claim. See [Drawings](docs/DRAWINGS.md).

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
  No ABI or model schema change. See [Assembly motion](docs/ASSEMBLY_MOTION.md).


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
  No ABI/model schema change. See [Assembly motion](docs/ASSEMBLY_MOTION.md).


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
  still select assembly branches. See [Assembly motion](docs/ASSEMBLY_MOTION.md).


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

- Helix wires (ABI 52): `occt_bridge_create_helix_wire` builds a helix as a
  line on its exact cylinder with a 3D curve within kernel tolerance, from an
  axis, start direction, radius, pitch, fractional turns (at most 10,000) and
  handedness. Tests check exact length, points on the cylinder, end points for
  both hands, invalid inputs, and binormal sweeps of a circle into valid
  springs within 0.1% of the tube-formula volume. A 200-turn spring sweeps in
  about 0.15 s; its benchmark, dominated by volume integration over the
  per-turn faces, takes 1.81 s (5 s budget).

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

- Nested parts lists (schema 91): with `nested`, parts lists follow the
  assembly frames holding the shown instances, listing sub-assemblies as rows
  with their parts and sub-frames indented beneath and numbered
  hierarchically (`1.2.1`), which balloons show. One test covers the tree,
  numbering, indentation, balloons, budgets, flat fallback and persistence.
  10,000 shown instances inside a sub-assembly group into item 1.1 in 0.399 s
  (2 s budget), as fast as the flat list (0.395 s).

- Drawing releases and revision tables (schema 90): dated, calendar-checked
  release records with approvals, optional links to the model revision ledger
  (validated to exist and move forward), a title-block revision that must match
  the latest release, `drawing_release_status` reporting current, model-changed,
  untracked or unreleased drawings, and a revision table drawn with the sheet;
  tables (and parts lists) must fit on the paper. Three tests cover status,
  13 validation cases, exact budgets, shortened descriptions and persistence.
  10,000 releases with approvals validate in 0.003 s (50 ms budget). See
  [Drawings](docs/DRAWINGS.md#releases-and-revision-tables-schema-90).

- Parts lists and balloons (schema 87): drawings group shown, unsuppressed
  instances into numbered items by family, effective parameters, output and
  material (with optional part numbers and variant suffixes), draw them as a
  table with the sheet linework, and point item balloons at instances from
  their placed origin or a datum. Three tests cover grouping, exact vertex
  budgets, SVG/DXF output, leader placement, validation and persistence.
  10,000 shown instances group into items with 100 balloons validated in
  0.394 s (2 s budget). See [Drawings](docs/DRAWINGS.md#parts-lists-and-balloons-schema-87).

- Modeled threads (schema 84):
  `FeatureOperation::Thread` sweeps an ISO 68-1 basic-profile groove along a
  helix and cuts it from a rod (external) or a minor-diameter hole (internal),
  either hand. Four tests check the ISO proportions, removed volume against
  the screw-motion formula within 1e-4, a single valid solid, pitch and hand
  edits with reuse, invalid pitches without leaked handles, and persistence.
  A 100-turn thread cuts in 3.99 s (12 s budget) and 10,000 pattern members
  of a threaded rod regenerate as one variant in 0.838 s (4 s budget); the
  helix now has one edge per turn, which made the 100-turn cut linear (15.1 s
  before). See [Modeled threads](docs/THREADS.md).

- Helix features (schema 82): `FeatureOperation::Helix` outputs a
  parameter-driven helical wire for `Sweep` paths, so springs and coils follow
  radius, pitch, turns and handedness edits. Two tests sweep springs, change
  turns with the section sketch reused, compare hands, persist, and reject
  zero turns. 10,000 pattern members of a helix family regenerate as one
  variant in 0.374 s (2 s budget). See [Helices](docs/HELIX.md).

- Surface texture (schema 71): drawing
  requirements with B46.1 roughness limits (Ra, Rq, Rz, Rmax in µm or µin),
  standard sampling lengths, waviness, lay, material-removal requirement,
  method note and all-around, drawn as Y14.36 symbols with exact vertex
  budgets; inspection records check readings by the maximum rule. Four tests
  cover rendering in SVG and DXF, validation, persistence and inspection.
  10,000 symbols generate with SVG and DXF exports in 0.102 s (1 s budget).
  See [Drawings](docs/DRAWINGS.md#surface-texture-schema-71).

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
  See [Drawings](docs/DRAWINGS.md#measured-inspection-schema-68).

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
  [Requirement rules](docs/REQUIREMENTS.md).

- Minimum-radius requirements (schema 47; ABI 36): `MinimumRadius` checks
  convex, concave, or both radii against a positive length, optionally treating
  sharp edges on that side as radius zero (inside corners a round cutter cannot
  reach). Results report the smallest radius, `Exact` or `Sampled` evidence,
  and the face or edge with a witness point. Two parametric tests and three
  bridge tests cover bores, blind-hole floors, cylinders, cones, spheres, tori,
  inside-corner fillets, outside edges, sampled variable blends, validation,
  and persistence. See [Requirement rules](docs/REQUIREMENTS.md).

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
  budget). See [Requirement rules](docs/REQUIREMENTS.md).

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
  [Assembly motion](docs/ASSEMBLY_MOTION.md).

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
  [Sheet metal](docs/SHEET_METAL.md) and [Hole-size catalog](docs/HOLE_SIZE_CATALOG.md).

- Analysis and manufacturing hand-off (schema 44): semantic face tags, source
  BREP and surface meshes for an isolated Gmsh tetrahedral runner, material-aware
  glTF with shared buffers, and sampled wall/draft/overhang checks. Five API
  tests, independent positive-volume tetrahedron checks, and Khronos validation
  cover the exports. A 1,000-part glTF scene takes 1.393 s; 10,000 indexed wall
  rays on 100,518 triangles take 3.284 s (10 s budgets). Screening is sampled;
  external volume meshing validates tag matching and element quality.
  See [Mesh hand-off](docs/MESH_HANDOFF.md).

- Model data management (schema 43): explicit linear revision records with
  nonrecursive semantic changes, resolved instance/feature change-impact
  reports, and a semantic Git merge driver with atomic successful writes.
  Seven API tests and three driver tests cover rollback, inheritance,
  downstream dependencies, current joints, materials, configuration suppression,
  history conflicts, and file preservation. Actual Git merges verify successful
  combination and conflict stages. Impact for 10,000 instances and 100 dependent
  features takes 0.144 s; appending after 10,000 revisions takes 0.185 s
  (10 s budgets). See [Model history](docs/MODEL_HISTORY.md) and
  [Document comparisons](docs/DOCUMENT_DIFF.md).

- Regenerated drawings (schema 42): orthographic, section, and cropped detail
  views with exact hidden-line removal, datum dimensions, parameter notes,
  and framed metadata title blocks. SVG and millimeter DXF exports use bounded
  sampled polylines. Seven tests cover regeneration, validation, persistence,
  semantic diff, export escaping, and cleanup. Independent DXF parsing reports
  zero errors or repairs. The 1,000-view case takes 1.235 s (10 s budget);
  a view of a 1,000-part assembly takes 0.903 s (20 s budget). Certified curve approximation remains a future extension.
  API and limits: [Drawings](docs/DRAWINGS.md).

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
  [Assembly motion](docs/ASSEMBLY_MOTION.md).

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
  See [Assembly motion](docs/ASSEMBLY_MOTION.md).

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
  zero issues. API: [Assembly motion](docs/ASSEMBLY_MOTION.md).

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
  API and limits: [Assembly motion](docs/ASSEMBLY_MOTION.md).
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
  API and limits: [Semantic document comparisons and merges](docs/DOCUMENT_DIFF.md).
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
  [Semantic document comparisons](docs/DOCUMENT_DIFF.md).
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
  Source, API, and supported sizes: [Hole-size catalog](docs/HOLE_SIZE_CATALOG.md).
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

For continued sketch authoring, fillets and chamfers cover line, arc, spline
and trimmed-ellipse corners, and sketches support linked mirror, linear and
circular patterns with inner holes. Local solver explanations now identify
conflicting equation groups, redundancy and representative free motion. The
optional `fully_constrained` acceptance policy now supports AI-authored parts
that require zero local free degrees. Finite spline and trimmed-conic edges now
project exactly into sketch planes, with stable endpoint references and source
parameter links. Connected multi-edge selections now import as composite
spline boundaries, with named junction references usable in constraints and
dimensions. Separate junction edge topology and conic/spline tangential contact regions
remain extensions to that workflow. Automatic bounded-region discovery now
selects native cells with named interior points; optional region composition
merges adjacent selections before extrusion while preserving holes and
separate bodies. Geometry editing now adds named splits, extend-to crossings,
and seed-selected boundary operations on explicit and discovered regions.
Named curve and witnessed region selectors now feed downstream history with
explicit lost/split/merge diagnostics. Safeguarded solving now includes rank
certification, stopping reports and opt-in warm starts. The five sketch-engine milestones are complete: composition, geometry editing,
stable references, solver reliability and guarded AI editing/repair. Interactive
sketch authoring UI and broader solver/geometry extensions remain separate work.

The earlier geometry and assembly milestones are complete. Manufacturing
product definition is the next general CAD priority; additional rib variants
remain lower priority. The ASME comparison below is based on public standard
scopes and inspected repo capabilities, not a full conformity audit.

1. **Drawing conventions and manufacturing sheets** — [Y14 family](https://www.asme.org/codes-standards/y14-standards):
   material section-lining presets, ASME Y14.1 zone and revision-block
   formats, and dimension spacing need the standards' own tables, which no
   public source provided. Standards-referenced lettering heights, ISO 5457
   or explicit zones, and Y14.2M line weights exist (schema 134).
   Standard drawing curves and detail trims now support exact DXF export, with
   bounded SVG approximation for curves SVG cannot represent. Direct deviations
   and limits render stacked values.
2. **Structured GD&T** — [Y14.5](https://www.asme.org/codes-standards/find-codes-standards/y14-5-dimensioning-tolerancing):
   the listed gaps are closed; the remaining refinements, listed in docs/DRAWINGS.md, are smaller. Measured planar datum frames, flatness,
   circularity, cylindricity, planar orientation zones and position with
   MMC/LMC bonus are evaluated (schema 68), and cylindrical datum features of
   size with circular and total runout (schema 135), common datums A-B
   (schema 136), and position in plane-axis frames with MMB/LMB datum shift
   under the simultaneous requirement (schema 137), and slot/tab width
   datum center planes, and two-segment composite position over n×
   patterns (schema 138), and surface profile against CMM nominal points
   with unequal disposition, n× patterns and composite profile (schema 139),
   and profile nominals projected onto the model's own faces (schema 140,
   ABI 69), and profile of a line by sections normal to its view, and runout
   of cones and other surfaces of revolution read square to their nominals,
   and supplied measurement uncertainty with guarded decision rules, and
   width and rotational datum shift from slot and tab datums, and position
   from a primary datum axis, stopped and clocked by planes or widths, with
   primary-axis shift, tilt and clocking rotation, and clocking and
   rotational shift from a hole or pin, and common datum axis shift, and
   position at an angle to a primary datum axis with a free turn when
   unclocked, and plane datum simulators at non-square basic angles, and
   composites of any number of segments (schema 141), and simultaneous
   common datum axis simulators, and projected tolerance zones and
   tangent planes (schema 142), and free state, statistical, continuous
   feature, dynamic profile and translation modifiers (schema 143), and
   measurement uncertainty simulated from the measurement itself.
3. **3D manufacturing annotations and exchange** — [Y14.41](https://www.asme.org/codes-standards/find-codes-standards/y14-41-digital-product-definition-data-practices):
   the listed gaps are closed; the remaining refinement is checking
   against Y14.41's
   own rules, which no public source provided. Dataset identification,
   authority and the model-revision rule exchange with the PMI (schema 151,
   ABI 76). Drawing GD&T attached to persistent faces exports as verified semantic
   AP242 PMI (schema 144, ABI 70); continuous feature, dynamic profile,
   unequal zones and composite relations are reported as unrepresented.
   Annotation views export as verified saved views of 3D presentations
   (schema 145, ABI 71), including section and clipped views (schema 150,
   ABI 75), and projected zones now exchange too. Views list the parts
   they show (schema 173, ABI 80) and draw size dimensions in 3D (schema
   174, ABI 81).
4. **Standards-backed thread specifications** — [B1.1](https://www.asme.org/codes-standards/find-codes-standards/b1-1-unified-inch-screw-threads-un-unr-thread-form):
   Unified classes 1A–2B with limits of size, designation and bore checks,
   fit checks and inspection of measured diameters exist (schema 152).
   Engagement length for screw strength is checked (schema 153), and
   drawings represent threads by the ISO 6410-1 convention (schema 154),
   including through-all and up-to holes measured on the solid and oblique
   views. Basic-profile thread geometry can be modeled (schema 84).
   Remaining: class 3B and metric ISO 965 classes need the standards' own
   tables, which no public source provided, and metric callouts and drill
   catalogs are not yet checked against a standard.
5. **Manufacturing surface texture** — [B46.1](https://www.asme.org/codes-standards/about-standards/technology-highlights/advanced-manufacturing):
   Y14.36 symbol placement checked against the standard, which no public
   source provided, and AP242 exchange of textures, which OCCT 7.9 cannot
   write. Drawing requirements, symbols and maximum-rule reading checks
   exist (schema 71), and requirements govern persistent faces with leaders
   landing on them; the 16% rule and Gaussian filtering of stylus
   profiles to Ra, Rq, Rz and Rmax exist (schema 155), as do short-wave
   (λs) filtering of raw traces and waviness height and spacing checks
   (schema 168). Instrument settings (stylus tip radius, traverse speed)
   are not modeled. Rendering-material roughness is independent of these
   requirements.
6. **Controlled drawing release and assembly lists** — [Y14.100](https://www.asme.org/codes-standards/find-codes-standards/engineering-drawing-practices):
   identity-bound electronic signatures, which need keys outside the
   document, and checking the revision practices against Y14.35's own
   text. Release records with approvals, model revision links and status,
   revision tables (schema 90), change markers with a zone column (schema
   156), approval workflows with content digests (schema 158), multi-sheet
   revisions with a sheet status table (schema 159), and flat or nested
   parts lists with item balloons (schemas 87, 91) that merge identical
   sub-assemblies (schema 157) exist.

### AI design tool

The single-part AI loop works end to end: [MCP](tools/model/MCP.md) exposes
generated schemas and examples, builds with verified requirements, read-only
inspection, and guarded previews and edits with revision history. The gaps
below keep an AI from using the rest of the engine as a design tool. They are
ordered by priority; the catalog/documentation foundation is complete.

1. **Current AI documentation and examples — complete.** MCP automatically
   exposes every local request example and publishes `occt://catalog` with the
   running binary’s schema version, operations, example kinds and derived
   capability counts. Wire tests validate the entire catalog against generated
   build/view schemas and require the current model version. The new
   `hemmed-tray` builds folded and flat outputs with side walls, a closed hem
   and a bend-crossing slot; `tapped-shell` builds an open-top shell with a
   cosmetic tapped bore. Wire tests check native exact volumes, parameter edits and
   preservation of accepted builds. Broader assembly/drawing/GD&T examples
   belong with the command exposure below.
2. **MCP access to drawings, inspection and mechanisms — complete.** The
   adapter discovers the existing drawing, inspection, motion, balance,
   joint-branch and merge commands and exposes generated schemas, complete
   examples and readable analysis artifacts. Negative measurements, collisions
   and semantic conflicts retain complete reports. Model inputs and accepted
   builds stay unchanged; merged and branch models require a separate build
   for verification. Wire tests exercise all commands, artifact boundaries,
   optional installations, timeout/cancellation cleanup and negative results.
3. **Visual feedback for the AI — complete.** Builds, edits and edit
   previews render shaded PNG views and hatched section planes on request,
   and MCP returns them as image content. Flat patterns render as ordinary
   outputs. Remaining refinements: bend lines on flat-pattern images, several
   outputs in one scene for checking interference, and dimension labels in
   the images.
4. **Complete failure reports — complete.** Builds and edits rejected by
   required requirements return the whole verification table with measured
   values, evidence and witnesses, plus output measurements, bounded at 500
   rows. Renders of rejected geometry are not written; use
   `occt_visualize_model` to see failed witnesses on the geometry.
5. **Shared sessions with the studio — complete.** MCP reads the model open
   in a local studio and applies accepted builds to it, guarded by the model
   hash it read, and the studio names the change and can undo it. Remaining:
   a client cannot yet watch for the person's changes without polling, and
   only whole accepted models are applied (no merge of concurrent edits).
6. **Design knowledge — complete.** Materials (schema 179), metric standard
   parts (schema 180), exact minimum wall (schema 181) and draft refined on
   freeform surfaces (ABI 85). Remaining refinements: rolling bearings and
   inch hardware (no machine-readable public table was reachable), walls
   at knife edges between faces that share an edge, and proof that a refined
   draft extreme is global. Stress beyond beam theory still needs an external
   solver.
7. **Design starting points — complete.** Four templates (L-bracket,
   printed enclosure, shaft, sheet-metal channel) with typed inputs and
   checks wired to them, listed and instantiated over MCP. Remaining: more
   templates (gussets, lids, hinges, spars), and templates that compose into
   assemblies.

## Later

- Sheet-metal closed and teardrop hems, cutouts across bends, edge
  flanges on edge flanges, and unfolding edited solids beyond strips with
  edge flanges (cutouts within flanges exist, schema 160; bend tables,
  schema 161; open and rolled hems, schema 162; edge flanges with bend
  reliefs, schema 176).
- Tolerance classes beyond H and h (H6–H13 and h6–h13 exist, schema 163).
- Wing structure beyond solid printed segments: hollow shells with internal
  ribs, alignment pins independent of the spar, twist-exact hinge lines, and
  print-bed fit in arbitrary orientations.
- Manufacturing-step temperature checks (sourced material sheets, service
  temperature limits and a beam-theory bending check exist, schema 179);
  stress analysis beyond beam theory still needs an external solver.

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
