#!/usr/bin/env bash
# Runs the scale benchmark suite (rust/occt-parametric/benches/scale/)
# against an optimized build of the C++ library. Exits non-zero when any
# required case misses its time budget or correctness check; known gaps tied
# to open roadmap items are reported without failing the run.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
build="$root/build/bench"

cmake -S "$root" -B "$build" -DCMAKE_BUILD_TYPE=Release -DBUILD_TESTING=OFF >/dev/null
cmake --build "$build" -j >/dev/null

OCCT_BRIDGE_LIB_DIR="$build" LD_LIBRARY_PATH="$build" \
    cargo bench --quiet --manifest-path "$root/rust/occt-parametric/Cargo.toml" --bench scale

OCCT_BRIDGE_LIB_DIR="$build" LD_LIBRARY_PATH="$build" \
    cargo bench --quiet --manifest-path "$root/rust/occt-parametric/Cargo.toml" --bench document_diff

OCCT_BRIDGE_LIB_DIR="$build" LD_LIBRARY_PATH="$build" \
    cargo bench --quiet --manifest-path "$root/rust/occt-parametric/Cargo.toml" --bench document_merge

OCCT_BRIDGE_LIB_DIR="$build" LD_LIBRARY_PATH="$build" \
    cargo bench --quiet --manifest-path "$root/rust/occt-parametric/Cargo.toml" --bench document_management

# Schema-72 constraints, native spline projection and saved sketch profile edits.
OCCT_BRIDGE_LIB_DIR="$build" LD_LIBRARY_PATH="$build" \
    cargo bench --quiet --manifest-path "$root/rust/occt-parametric/Cargo.toml" --bench sketch_advanced

# Includes the bounded 10,000-face geometric matcher case.
python3 -m unittest discover -s "$root/tools/mesh/tests"

OCCT_BRIDGE_LIB_DIR="$build" LD_LIBRARY_PATH="$build" \
    cargo bench --quiet --manifest-path "$root/rust/occt-parametric/Cargo.toml" --bench linkage

OCCT_BRIDGE_LIB_DIR="$build" LD_LIBRARY_PATH="$build" \
    cargo bench --quiet --manifest-path "$root/rust/occt-parametric/Cargo.toml" --bench rotating_motion

OCCT_BRIDGE_LIB_DIR="$build" LD_LIBRARY_PATH="$build" \
    cargo bench --quiet --manifest-path "$root/rust/occt-parametric/Cargo.toml" --bench balance

OCCT_BRIDGE_LIB_DIR="$build" LD_LIBRARY_PATH="$build" \
    cargo bench --quiet --manifest-path "$root/rust/occt-parametric/Cargo.toml" --bench templates

OCCT_BRIDGE_LIB_DIR="$build" LD_LIBRARY_PATH="$build" \
    cargo bench --quiet --manifest-path "$root/rust/occt-parametric/Cargo.toml" --bench large_linkage

OCCT_BRIDGE_LIB_DIR="$build" LD_LIBRARY_PATH="$build" \
    cargo bench --quiet --manifest-path "$root/rust/occt-parametric/Cargo.toml" --bench branch_search

# AI command examples and 1,000-instance report/publication budget.
OCCT_BRIDGE_LIB_DIR="$build" LD_LIBRARY_PATH="$build" \
    cargo build --quiet --release --manifest-path "$root/rust/occt-parametric/Cargo.toml" --bin occt-model
LD_LIBRARY_PATH="$build" \
    python3 "$root/tools/model/check.py" "$root/rust/occt-parametric/target/release/occt-model"

# MCP protocol roundtrips, generated schemas, and accepted model resources.
LD_LIBRARY_PATH="$build" \
    python3 "$root/tools/model/check_mcp.py" "$root/rust/occt-parametric/target/release/occt-model"
