# Roadmap

Where the project stands and what comes next. The design rationale behind each
item lives in [Parametric architecture](PARAMETRIC_ARCHITECTURE.md); this file
tracks status and order.

## Current status

| Layer | Version | State |
|---|---|---|
| C ABI (`src/`, `include/`) | ABI 17 | Stable; exact version match required |
| `occt-bridge` (safe Rust wrapper) | — | Covers the full ABI |
| `occt-recipes` (application constructors) | — | Stone and wall torch |
| `occt-parametric` (engineering layer) | Schema 18 | Active development |

| Quality gate | Result | Command |
|---|---|---|
| Tests | C 2/2, bridge 27, recipes 3, parametric 61 | `ctest`, `cargo test` (see README) |
| SonarQube (Rust) | 0 issues, A ratings | local SonarQube scan |
| clang-tidy, cppcheck, clang `-Werror` | Clean | `tools/cpp-lint/run.sh` |
| Coverage | 92% lines overall; C++ 94% lines, 88% branches, 100% functions | `tools/coverage/run.sh` |

## Done

### Kernel (C ABI)

- Sessions, integer shape handles, history-preserving duplicates, exception
  containment at every entry point.
- Primitives, wires, faces, prisms, polyline tubes, lofts, compounds.
- Booleans, fillets, chamfers, offsets, hollowing, transforms.
- Topology traversal, adjacency, recorded tangency, topological identity.
- Measurements, BREP validity, BREP load/save, operation history.
- Curvature: midpoint, sampled range, and exact (line, conic) or
  error-bounded (Bezier, B-spline) extrema.

### Parametric layer

- Typed, unit-aware parameters; derived scalar and vector expressions;
  pre-generation constraints.
- Dependency-ordered feature graphs with incremental reuse of unchanged
  features; required, preferred, and advisory verification.
- Semantic edge and face selectors: extrema, size, curvature (midpoint,
  sampled, proven-bound), normals, adjacency, tangency, set composition,
  operation history.
- Linked clones with sparse overrides, detachment, freezing, and managed
  regeneration.
- Nested assembly frames; shared generation for clones that differ only in
  placement.
- Patterns: linear and circular rules; `LinearFit` and `CircularFit`
  constraint rules that solve count and spacing; editable rules and counts;
  stable member slots; per-member placement overrides and suppression.
- Versioned JSON documents with migrations from every schema since v1.

### Tooling

- Containerized C/C++ lint and merged LLVM coverage for both languages.
- Argument-validation conformance test for every C entry point.

## Next

Ordered by priority. Each item should land with tests, a schema bump when the
document format changes, and updates to this file.

1. **Generic sewing and shell-to-solid (kernel).** Unblocks moving the
   faceted-stone recipe off its compatibility entry point and enables solids
   built from arbitrary faces.
2. **Patterns driven by family parameters or assembly geometry (parametric).**
   Pattern counts and spans bound to parameters or measured geometry, such as
   a bolt count from a flange diameter or a pew row from an aisle length.
3. **Multi-family graphs (parametric).** Let one `InstanceGraph` and document
   hold instances of several part families, the basis for subassemblies.
4. **Broader exchange formats (kernel).** STEP first, then mesh export for
   visualization. BREP remains the lossless interchange.
5. **Coverage in SonarQube (tooling).** Feed `build/coverage/lcov.info` to the
   scanner so coverage appears alongside the quality gate.

## Later

- Assembly relationships, configurations, materials, and named datums.
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
