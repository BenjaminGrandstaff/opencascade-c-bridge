#!/usr/bin/env bash
# Generate first-order tagged tetrahedra from a new bridge FEA bundle.
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
environment="$root/build/mesh-python"
if [[ $# -lt 3 ]]; then
    echo "Usage: tools/mesh/run.sh BUNDLE OUTPUT.msh --size-mm SIZE [options]" >&2
    exit 1
fi
if [[ ! -x "$environment/bin/python" ]]; then
    python3 -m venv "$environment"
fi
if ! "$environment/bin/python" -c 'import gmsh; assert gmsh.__version__ == "4.15.2"; gmsh.initialize(); gmsh.finalize()' >/dev/null 2>&1; then
    "$environment/bin/pip" install --disable-pip-version-check -r "$root/tools/mesh/requirements.txt"
fi
exec "$environment/bin/python" "$root/tools/mesh/volume.py" "$@"
