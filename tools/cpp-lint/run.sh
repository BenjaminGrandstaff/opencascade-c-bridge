#!/usr/bin/env bash
# Builds with clang -Werror, then runs clang-tidy (.clang-tidy) and cppcheck
# over the C/C++ sources inside a podman container, so no host packages are required. Exits non-zero when
# either tool reports a finding.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
image="occt-cpplint"

podman build -q -t "$image" -f "$root/tools/cpp-lint/Containerfile" "$root/tools/cpp-lint" >/dev/null

# Share the read-only checkout's SELinux label with other gate containers.
podman run --rm -v "$root:$root:ro,z" "$image" bash -c "
    set -euo pipefail
    cmake -S '$root' -B /tmp/build -DCMAKE_EXPORT_COMPILE_COMMANDS=ON \
        -DCMAKE_C_COMPILER=clang -DCMAKE_CXX_COMPILER=clang++ >/dev/null
    echo '== clang -Werror build'
    if ! cmake --build /tmp/build -j >/tmp/clang-build.log 2>&1; then
        cat /tmp/clang-build.log
        exit 1
    fi
    status=0
    echo '== clang-tidy'
    clang-tidy -p /tmp/build --quiet '$root'/src/*.cpp '$root'/tests/*.cpp '$root'/tests/*.c \
        2>/dev/null | tee /tmp/tidy.txt
    grep -q 'warning:' /tmp/tidy.txt && status=1
    echo '== cppcheck'
    cppcheck --project=/tmp/build/compile_commands.json --quiet --inconclusive \
        --enable=warning,style,performance,portability --inline-suppr \
        --suppress=missingIncludeSystem --suppress='*:/usr/include/*' \
        --error-exitcode=1 --template='{file}:{line}: {severity}: {message} [{id}]' || status=1
    exit \$status
"
