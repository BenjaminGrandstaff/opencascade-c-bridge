# opencascade-c-bridge

A small, stable C ABI over Open Cascade (OCCT), designed to be wrapped safely
from Rust and other languages. Open Cascade C++ objects never cross the ABI.

The current C ABI version is **24**.

## Current API

- Opaque, independently owned sessions
- Integer shape handles scoped to a session
- History-preserving duplicate handles for transactional geometry reuse
- Boxes, cylinders, cones, spheres, and arbitrary planar polygon prisms
- Reusable polyline, circular, and elliptical wires, planar faces, and face
  extrusion
- Non-mutating translation, axis-angle rotation, and uniform scaling; rigid
  moves share geometry through locations, so placed copies stay small
- Compatibility constructors for existing natural-stone and wall-torch callers
- Circular tubes swept along arbitrary 3D polylines for rails, scrollwork, and ornament
- Multi-section solid/shell lofts and assembly compounds
- Face and shell sewing with operation history, single-shell solid
  construction, and multi-shell solids with internal voids
- Fuse, cut, and common boolean operations
- Selected-edge fillets and chamfers, joined offsets, and face-selected hollowing
- Shape-kind and unique-subshape traversal
- OCCT topological-identity comparison for independently owned handles
- Oriented face-normal, face-planarity, edge-length, circular-edge-radius,
  midpoint edge-curvature, deterministic sampled full-edge curvature ranges,
  exact (line, conic) or error-bounded (Bezier, B-spline) curvature extrema,
  and direct topology-adjacency queries
- Recorded G1-or-better tangency queries between adjacent faces
- Generated, modified, and deleted operation-history queries
- Tolerance-padded and exact bounds, surface area, volume, center-of-mass, and
  BREP validity inspection
- BREP persistence, STEP import/export, and configurable ASCII/binary STL export
- Per-session result validation (on by default) for booleans, fillets,
  chamfers, offsets, hollowing, sewing, and STEP and BREP import, with
  optional shape healing that carries operation history, fuzzy booleans,
  and per-call warnings
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
patterns, parameter- or geometry-driven counts and fitted spans,
multi-family instance graphs, explicit generation freezing, unit-aware derived scalar arithmetic
with negate, absolute, minimum, maximum, and clamp functions, derived vector
composition with add, subtract, scale, and normalize operations, dimension-safe
comparison-driven conditional scalar expressions,
pre-generation parameter constraints, semantic face and edge selectors, and
versioned JSON model documents are implemented. Feature graphs include sewing
and single- or multi-shell solid construction. Selectors support orientation,
adjacency, extrema, nearest-center, longest-edge, circular-radius,
curvature-radius, sampled full-edge curvature-radius range, proven-bound
curvature-radius range,
largest-planar-face, tangent-neighbor, set composition, and operation-history
rules. Families declare named datums; graphs record checked datum
relationships that can be solved to place free instances, configurations, and
materials with mass. Schema v1 through
v21 documents migrate to v22 during load; unsupported
future versions are rejected.
Managed regeneration incrementally reuses unchanged outputs and
rebuilds dirty features plus their downstream dependents. Graph regeneration
runs the feature graph once per distinct resolved parameter set and places
clones that differ only in placement or assembly frame independently. Richer
verification remains planned work.
See [Roadmap](ROADMAP.md) for current status and what comes next, and
[Parametric architecture](PARAMETRIC_ARCHITECTURE.md) for the
definition/instance/clone/result model and the boundary between that layer and
the C ABI.

The sibling [`engineering-intent-language`](../engineering-intent-language)
project provides the broader serialized source model, including semantic
validation and clone inheritance resolution. The local `occt-parametric` crate
is the kernel-facing execution layer for an initial subset of that model.

## Repository layout

- [`include/occt_bridge.h`](include/occt_bridge.h): stable C ABI contract.
- [`src/occt_bridge.cpp`](src/occt_bridge.cpp): OCCT ownership, operations,
  diagnostics, topology traversal, measurements, and operation history.
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

Rust unit tests live inside each crate's `lib.rs`, so Rust percentages include
test code. Rust branch coverage needs a nightly toolchain and is not reported.

The scale benchmark suite enforces the roadmap's scaling requirement. It
builds an optimized copy of the library in `build/bench`, runs every case at
the target sizes (10,000-member patterns, deep clone chains, 1,000-part solver
stacks and grids, repeated regeneration), and fails when a required case misses its
time budget or correctness check. Cases tied to open roadmap items are
reported as known gaps:

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
regeneration audit records. Generated OCCT handles and BREPs are deliberately
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
