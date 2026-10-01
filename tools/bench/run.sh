#!/usr/bin/env bash
# Runs the scale benchmark suite (rust/occt-parametric/benches/scale.rs)
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
