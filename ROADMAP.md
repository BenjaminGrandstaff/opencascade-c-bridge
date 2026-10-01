# Roadmap

Where the project stands and what comes next. The design rationale behind each
item lives in [Parametric architecture](PARAMETRIC_ARCHITECTURE.md); this file
tracks status and order.

## Current status

| Layer | Version | State |
|---|---|---|
| C ABI (`src/`, `include/`) | ABI 24 | Stable; exact version match required |
| `occt-bridge` (safe Rust wrapper) | — | Covers the full ABI |
| `occt-recipes` (application constructors) | — | Stone and wall torch |
| `occt-parametric` (engineering layer) | Schema 22 | Active development |

| Quality gate | Result | Command |
|---|---|---|
| Tests | C 2/2, bridge 40, recipes 3, parametric 82 | `ctest`, `cargo test` (see README) |
| SonarQube (indexed Rust) | Gate OK, 0 issues, 93.3% line coverage | `tools/sonar/run.sh` |
| clang-tidy, cppcheck, clang `-Werror` | Clean | `tools/cpp-lint/run.sh` |
| Coverage | 93.42% lines overall; C++ 94% lines, 88% branches, 100% functions | `tools/coverage/run.sh` |
| Scale benchmarks | 15 pass within budget; 1 known gap (km-scale tolerances) | `tools/bench/run.sh` |

## Done

### Kernel (C ABI)

- Sessions, integer shape handles, history-preserving duplicates, exception
  containment at every entry point.
- Primitives, wires, faces, prisms, polyline tubes, lofts, compounds.
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
  reason. Measured overhead on a 100-cut chain: 2.05x.
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

- Typed, unit-aware parameters; derived scalar and vector expressions;
  pre-generation constraints.
- Dependency-ordered feature graphs with incremental reuse of unchanged
  features; required, preferred, and advisory verification.
- Sewing and single- or multi-shell solid construction as serializable feature
  operations.
- Semantic edge and face selectors: extrema, size, curvature (midpoint,
  sampled, proven-bound), normals, adjacency, tangency, set composition,
  operation history.
- Linked clones with sparse overrides, detachment, freezing, and managed
  regeneration.
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
  each instance, angular residuals are length-scaled, damping starts near
  Gauss–Newton, and free directions are restored toward the start; measured
  50-part stacks solve in 0.17 s (seated) and 2.5 s (fully constrained) at
  the origin and 1 m from it, with no drift in unconstrained directions.
- Versioned JSON documents with migrations from every schema since v1.

### Recipes

- Wall torch and faceted stone, both built only from generic operations; the
  stone now sews planar facets and matches the legacy constructor's volume.

### Tooling

- Containerized C/C++ lint and merged LLVM coverage for both languages.
- Sonar generic-coverage conversion, containerized scanning, quality-gate
  enforcement, and issue-count enforcement. Community Build reports the Rust
  portion; C/C++ records remain in the report for servers with CFamily.
- Argument-validation conformance test for every C entry point.
- Scale benchmark suite with time budgets and correctness checks at the
  target sizes, reporting known gaps against open roadmap items.

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
  (`rust/occt-parametric/benches/scale.rs`, run by `tools/bench/run.sh`) at
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

### Robustness and scale

1. **Scalable relationship solving.** Analytic Jacobians, sparse linear
   algebra, and decomposition into independent connected groups, so
   1,000-part assemblies solve in seconds.
2. **Kernel failure diagnostics.** Report which edge, face, or input caused a
   fillet, chamfer, offset, or boolean failure, with OCCT's own error codes.
3. **Cheaper validation of faces with many holes.** Result validation
   costs 1.65x a boolean chain at 25 cuts and 2.2x at 200, because checking
   a face with many holes includes pairwise wire-intersection tests; check
   only wires an operation changed, or use spatial indexing.
4. **Iterative clone resolution.** Resolving an instance recurses once per
   clone link, so a 20,000-deep chain needs a 16 MiB stack and would
   overflow a default 2 MiB thread. Resolve chains iteratively with
   memoized parents so depth costs neither stack nor repeated work.
5. **Configurable tolerances.** Per-model linear and angular tolerances for
   relationships and checks, replacing the fixed 1e-6 mm and 1e-9 rad.
   Measured: 50-part stacks 1 km from the origin converge to residuals near
   1e-8 mm but are reported unsolved because the fixed tolerances sit at
   double precision for million-millimeter coordinates.

### Capabilities

6. **Datum- and mass-based requirements.** Verification rules for mass
   limits, datum clearances, and relationship satisfaction alongside the
   existing validity and volume rules.
7. **Constraint-solved 2D sketches.** Lines, arcs, and circles on a datum
    plane with geometric constraints (coincident, tangent, parallel,
    perpendicular, horizontal, vertical, equal) and dimensional constraints
    driven by parameter expressions. Solve with the relationship solver's
    machinery, report free degrees and conflicts, and emit closed profiles as
    wires and faces for features.
8. **Feature breadth.** Extrude and revolve from sketch profiles (adds a
    revolve operation to the C ABI), holes with standard sizes, counterbores,
    countersinks, and recorded thread specifications, draft, ribs, and
    variable-radius fillets; sheet metal (flanges, bends, flat patterns)
    after the rest.
9. **Assembly depth.** Joints (revolute, prismatic, cylindrical, planar,
    fixed) with limits on top of relationships; interference and minimum
    clearance between instances using exact boolean and distance queries; and
    motion studies that sweep joint values and report collisions.
10. **Generated drawings.** Projected views (orthographic, section, detail)
    by hidden-line removal, exported as SVG and DXF; dimensions and notes
    placed from datums and parameters; title blocks from document metadata.
    Drawings regenerate with the model rather than being edited by hand.
11. **Analysis and manufacturing hand-off.** Full mass properties (center of
    mass, inertia tensor) per instance and assembly; tagged surface and
    volume meshes for external FEA; manufacturability checks (minimum wall
    thickness, draft angle, 3D-printing overhang); glTF export with material
    appearance for rendering.
12. **Model data management.** Semantic diff and three-way merge of model
    documents (parameters, features, instances, relationships), recorded
    revision history inside documents, and change-impact reports listing the
    instances and features a change affects, with a git merge driver.

## Later

- Richer requirement rules: interference, minimum radius, wall thickness,
  connectivity, manufacturing checks.
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
