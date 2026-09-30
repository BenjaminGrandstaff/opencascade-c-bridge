#!/usr/bin/env bash
# Generate merged coverage, convert it to SonarQube's language-neutral format,
# run a local/containerized scanner, and print the quality gate and coverage.
#
# Usage: tools/sonar/run.sh [--no-coverage]
# Environment:
#   SONAR_HOST_URL   default: http://127.0.0.1:9000
#   SONAR_TOKEN      analysis token
#   SONAR_TOKEN_FILE file containing the token
#   SONAR_ADMIN_AUTH user:password used to create and revoke a temporary token
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
host="${SONAR_HOST_URL:-http://127.0.0.1:9000}"
project="opencascade-c-bridge"
token_file="${SONAR_TOKEN_FILE:-${XDG_CONFIG_HOME:-$HOME/.config}/opencascade-c-bridge/sonar-token}"

if [ "${1:-}" != "--no-coverage" ]; then
    "$root/tools/coverage/run.sh"
fi

test -s "$root/build/coverage/lcov.info" || {
    echo "Missing build/coverage/lcov.info; run tools/coverage/run.sh first" >&2
    exit 1
}
python3 "$root/tools/sonar/lcov_to_generic.py" \
    --root "$root" \
    "$root/build/coverage/lcov.info" \
    "$root/build/coverage/sonar-generic-coverage.xml"

# The scanner runs as an unprivileged user in a container and must be able to
# traverse coverage output created by the host toolchain.
chmod -R a+rX "$root/build/coverage"

cmake -S "$root" -B "$root/build" -DCMAKE_EXPORT_COMPILE_COMMANDS=ON >/dev/null

if [ -z "${SONAR_TOKEN:-}" ] && [ -r "$token_file" ]; then
    SONAR_TOKEN="$(tr -d '[:space:]' < "$token_file")"
fi

curl -fsS "$host/api/system/status" | grep -q '"status":"UP"' || {
    echo "SonarQube is not ready at $host" >&2
    exit 1
}

if [ -n "${SONAR_TOKEN:-}" ] && ! curl -fsS -u "$SONAR_TOKEN:" \
    "$host/api/authentication/validate" | grep -q '"valid":true'; then
    echo "Configured SonarQube token is not valid; trying administrator authentication" >&2
    SONAR_TOKEN=""
fi

if [ -z "${SONAR_TOKEN:-}" ] && [ -n "${SONAR_ADMIN_AUTH:-}" ]; then
    token_name="opencascade-c-bridge-$(date +%s)"
    SONAR_TOKEN="$(curl -fsS -u "$SONAR_ADMIN_AUTH" -X POST \
        "$host/api/user_tokens/generate" -d "name=$token_name" -d type=GLOBAL_ANALYSIS_TOKEN \
        | python3 -c 'import json, sys; print(json.load(sys.stdin)["token"])')"
    trap 'curl -fsS -u "$SONAR_ADMIN_AUTH" -X POST "$host/api/user_tokens/revoke" -d "name=$token_name" >/dev/null || true' EXIT
fi
test -n "${SONAR_TOKEN:-}" || {
    echo "Set SONAR_TOKEN, SONAR_TOKEN_FILE, or SONAR_ADMIN_AUTH before scanning" >&2
    exit 1
}

podman run --rm --network host \
    -e SONAR_HOST_URL="$host" \
    -e SONAR_TOKEN="$SONAR_TOKEN" \
    -v "$root:/usr/src:Z" \
    docker.io/sonarsource/sonar-scanner-cli:latest \
    -Dsonar.qualitygate.wait=true

SONAR_AUTH="$SONAR_TOKEN:" python3 - "$host" "$project" <<'PY'
import base64
import json
import os
import sys
import urllib.parse
import urllib.request

host, project = sys.argv[1:]
auth = base64.b64encode(os.environ["SONAR_AUTH"].encode()).decode()

def get(path, **query):
    url = f"{host}{path}?{urllib.parse.urlencode(query)}"
    request = urllib.request.Request(url, headers={"Authorization": f"Basic {auth}"})
    with urllib.request.urlopen(request) as response:
        return json.load(response)

gate = get("/api/qualitygates/project_status", projectKey=project)["projectStatus"]
measures = get(
    "/api/measures/component",
    component=project,
    metricKeys="coverage,line_coverage,branch_coverage,lines_to_cover,uncovered_lines",
)["component"].get("measures", [])
issues = get("/api/issues/search", componentKeys=project, resolved="false", ps=1)["total"]
print(f"Quality gate: {gate['status']}")
print("Coverage: " + ", ".join(f"{item['metric']}={item.get('value', 'n/a')}" for item in measures))
print(f"Open issues: {issues}")
if gate["status"] != "OK" or issues:
    raise SystemExit(1)
PY
