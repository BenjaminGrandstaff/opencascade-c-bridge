# Roadmap

Where the project stands and what comes next. The design rationale behind each
item lives in [Parametric architecture](PARAMETRIC_ARCHITECTURE.md); this file
tracks status and order.

## Current status

| Layer | Version | State |
|---|---|---|
| C ABI (`src/`, `include/`) | ABI 22 | Stable; exact version match required |
| `occt-bridge` (safe Rust wrapper) | — | Covers the full ABI |
| `occt-recipes` (application constructors) | — | Stone and wall torch |
| `occt-parametric` (engineering layer) | Schema 22 | Active development |

| Quality gate | Result | Command |
|---|---|---|
| Tests | C 2/2, bridge 34, recipes 3, parametric 73 | `ctest`, `cargo test` (see README) |
| SonarQube (indexed Rust) | Gate OK, 0 issues, 93.3% line coverage | `tools/sonar/run.sh` |
| clang-tidy, cppcheck, clang `-Werror` | Clean | `tools/cpp-lint/run.sh` |
| Coverage | 93.34% lines overall; C++ 94.18% lines, 88.11% branches, 100% functions | `tools/coverage/run.sh` |

## Done

### Kernel (C ABI)

- Sessions, integer shape handles, history-preserving duplicates, exception
  containment at every entry point.
- Primitives, wires, faces, prisms, polyline tubes, lofts, compounds.
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

## Next

Ordered by priority. Each item should land with tests, a schema bump when the
document format changes, and updates to this file.

1. **Relationship solving.** Place instances to satisfy their relationships,
   starting with rigid coincident and distance chains, and report
   over-constrained or conflicting sets.
2. **Datum- and mass-based requirements.** Verification rules for mass
   limits, datum clearances, and relationship satisfaction alongside the
   existing validity and volume rules.

## Later

- Richer requirement rules: clearance, interference, minimum radius, wall
  thickness, connectivity, manufacturing checks.
- Assumptions and requirement-to-feature trace links in the document schema.
- Semantic naming beyond feature outputs, and geometric tangency inference
  when continuity metadata is absent.
- Additional domain-specific expression functions.
- Kernel-level (location-only) shape sharing for placed clones.
- Integration with the broader EIL source model in the sibling
  [`engineering-intent-language`](../engineering-intent-language) project.

## Keeping this current

When a feature lands, move it from **Next** to **Done**, refresh the status
tables (ABI and schema versions, test counts, coverage), and adjust the
"next work" paragraph in [Parametric architecture](PARAMETRIC_ARCHITECTURE.md)
to match.
