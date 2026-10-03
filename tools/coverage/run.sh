#!/usr/bin/env bash
# Measures line, function, and region coverage of the C++ library and the
# Rust crates with LLVM source-based coverage inside a podman container.
# The C API test and every Rust test suite exercise the instrumented
# library, so C++ coverage reflects all of them. Writes a summary to stdout
# and HTML plus LCOV reports to build/coverage/.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
out="$root/build/coverage"
image="occt-coverage"
mkdir -p "$out"

podman build -q -t "$image" -f "$root/tools/coverage/Containerfile" "$root/tools/coverage" >/dev/null

podman run --rm -v "$root:$root:ro,Z" -v "$out:/out:Z" "$image" bash -c "
    set -euo pipefail
    flags='-fprofile-instr-generate -fcoverage-mapping'
    cmake -S '$root' -B /tmp/build -DCMAKE_BUILD_TYPE=Debug \
        -DCMAKE_C_COMPILER=clang -DCMAKE_CXX_COMPILER=clang++ \
        -DCMAKE_C_FLAGS=\"\$flags\" -DCMAKE_CXX_FLAGS=\"\$flags\" \
        -DCMAKE_SHARED_LINKER_FLAGS=-fprofile-instr-generate \
        -DCMAKE_EXE_LINKER_FLAGS=-fprofile-instr-generate >/dev/null
    cmake --build /tmp/build -j >/dev/null

    mkdir -p /tmp/profiles
    export LLVM_PROFILE_FILE='/tmp/profiles/%p-%m.profraw'
    ctest --test-dir /tmp/build --output-on-failure >/dev/null

    export OCCT_BRIDGE_LIB_DIR=/tmp/build LD_LIBRARY_PATH=/tmp/build
    export CARGO_TARGET_DIR=/tmp/target RUSTFLAGS='-C instrument-coverage'
    objects=()
    for crate in occt-bridge occt-recipes occt-parametric; do
        manifest='$root/rust/'\$crate/Cargo.toml
        cargo test --quiet --locked --manifest-path \"\$manifest\" >/dev/null
        while read -r executable; do
            objects+=(-object \"\$executable\")
        done < <(cargo test --quiet --locked --no-run --message-format=json \
            --manifest-path \"\$manifest\" 2>/dev/null \
            | grep -o '\"executable\":\"[^\"]*\"' | cut -d'\"' -f4)
    done

    llvm-profdata merge -sparse /tmp/profiles/*.profraw -o /tmp/coverage.profdata
    ignore='^/usr/|/[.]cargo/|/rustc/|/builddir/|/library/(core|std|alloc)/|/tests/|/tests[.]rs$'
    report=(-instr-profile=/tmp/coverage.profdata /tmp/build/libocct_bridge.so
        -object /tmp/build/occt_bridge_c_test \"\${objects[@]}\"
        -ignore-filename-regex=\"\$ignore\")
    llvm-cov report \"\${report[@]}\" | sed 's|$root/||'
    llvm-cov show \"\${report[@]}\" -format=html -output-dir=/out/html >/dev/null
    llvm-cov export \"\${report[@]}\" -format=lcov > /out/lcov.info
"
echo "HTML: $out/html/index.html  LCOV: $out/lcov.info"
