# AI tools over local MCP

The stdio server exposes the `occt-model` build loop directly to an MCP client.
It uses Python 3.11+ standard-library modules and the built Rust executable;
there are no Python runtime package dependencies. It targets POSIX pipe I/O.
The server implements the MCP [stdio transport](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports),
[lifecycle](https://modelcontextprotocol.io/specification/2025-11-25/basic/lifecycle),
[tools](https://modelcontextprotocol.io/specification/2025-11-25/server/tools), and
[resources](https://modelcontextprotocol.io/specification/2025-11-25/server/resources)
for protocol versions `2025-11-25` and `2025-06-18`.

## Launch and connect

From the repository root:

```sh
LD_LIBRARY_PATH="$PWD/build" cargo build \
  --manifest-path rust/occt-parametric/Cargo.toml --bin occt-model
LD_LIBRARY_PATH="$PWD/build" python3 tools/model/mcp_server.py \
  --binary "$PWD/rust/occt-parametric/target/debug/occt-model" \
  --output-root /tmp/occt-ai-builds
```

Configure the MCP client's stdio connection to launch that Python command with
absolute script/binary paths and `LD_LIBRARY_PATH` pointing at the corresponding
native build directory. The client owns stdin/stdout; human logs use stderr.
The server is ready for connection, but launching it does not register it in a
particular AI application's settings.

`--output-root` selects the persistent local artifact directory. Tool callers
cannot choose filesystem destinations or executables. Each build uses a fresh
UUID directory and preserves all earlier accepted builds. `--timeout` sets the
per-build worker timeout in seconds (default 120). Up to eight requests may be
pending; workers run independently. Cancellation and stdin closure terminate
pending workers and remove their unaccepted work directories. No HTTP listener
or remote service is started.

## Tools and resources

| Tool | Arguments | Result |
| --- | --- | --- |
| `occt_get_schema` | `name`: `request`, `model`, `feature`, `parameter`, `sketch`, `requirement`, `inspection`, `face_selector`, `edge_selector`, `edit`, `change`, or `view` | Complete generated JSON Schema |
| `occt_get_example` | `name`: `bracket`, `enclosure`, `shaft`, or `mating-parts` | Complete editable build request |
| `occt_visualize_model` | `occb-model-view-v1` request | Annotated sketch/solid viewer and diagnostic snapshots |
| `occt_edit_build` | Accepted build ID/hash, guarded changes, outputs and revision metadata | Verified new build and semantic change record |
| `occt_build` | The entire `occb-model-request-v1` object | Accepted report, build ID, local directory, and artifact resource URIs |

`tools/list` provides the generated complete request schema as `occt_build`'s
input schema. All 28 current feature operations and 136 nested request-schema
definitions are discoverable. Schemas derive from serde-compatible Rust types;
new serialized variants require schema support at compilation. Authoring schemas
target current model schema 75. The engine still migrates older documents.
Schemas describe serialization and basic request bounds; units, dependency
references, dimensional constraints, selector resolution, and geometry validity
are checked by the build engine.

Schema discovery also works without MCP:

```sh
LD_LIBRARY_PATH="$PWD/build" rust/occt-parametric/target/debug/occt-model \
  --schema request > /tmp/occt-request.schema.json
```

`resources/list` advertises twelve `occt://schema/NAME` and ten
`occt://example/NAME` resources. Accepted builds return specific
`occt://build/BUILD_ID/ARTIFACT` URIs; a resource template advertises this form.
JSON and SVG resources return text, while STEP and STL return base64 blobs.
Reads are limited to 32 MiB; larger artifacts remain available at the returned
local directory. Only listed artifacts from completed builds can be read.
Request files, worker logs, arbitrary paths, and incomplete builds are excluded.

Tool results provide both `structuredContent` and equivalent JSON text content.
Build/verification failures return `isError: true` with the native structured
failure report. Protocol errors use JSON-RPC errors. Native STEP progress stays
inside the worker subprocess and cannot appear on MCP stdout. Required failures
currently have the engine's error context/diagnostics rather than a complete
rejected measurement table. See the [build contract](README.md).

## AI workflow

1. Read `occt_get_schema` and an appropriate example.
2. Describe the function/interfaces as requirements; record uncertain inputs as
   explicit assumptions with provenance, rather than inventing verified facts.
3. Call `occt_build` with explicit units and selected final outputs.
4. Inspect the verification table and its exact/sampled evidence. Read the SVG
   preview resource for visual review and model/report resources for details.
5. Read accepted `model.json`, put it into the next request, and provide typed
   parameter edits. Required failures preserve the preceding accepted build.
6. Return STEP/STL artifacts and the editable model to the user.

This adapter provides discovery, execution, and inspection. It does not perform
natural-language planning, replace a client's approval policy, provide mutable
shared sessions/revision locking, or prove unverified strength/manufacturability.

## Validation and scale

Wire tests use the actual server and model executable. They independently check
all twelve schemas with a Draft 2020-12 validator, validate all four examples, test
initialization/discovery/errors, exercise bracket build/edit/reject/repair and
artifact reads, reject path traversal, and stop/clean timed-out and cancelled
workers (including repeated cancellation).

```sh
python3 -m venv /tmp/occt-mcp-tests
/tmp/occt-mcp-tests/bin/pip install -r tools/model/requirements-dev.txt
LD_LIBRARY_PATH="$PWD/build" /tmp/occt-mcp-tests/bin/python \
  -m unittest discover -s tools/model/tests -v
```

Set `OCCT_MODEL_BINARY` to test a different executable. Release-scale checks are
part of `tools/bench/run.sh`, with no Python package dependencies: 1,000 schema
calls took 0.080 s, and a 1,000-part single-variant build with validity, analytic
volumes, requirement evidence and accepted-model resource retrieval took 0.283 s.
Both have 10-second budgets.

## Inspect existing parts before editing

Two read-only tools expose the authoring plan and selectable geometry:

| Tool | Input | Result |
| --- | --- | --- |
| `occt_inspect_model` | `schema: "occb-model-inspection-v1"`, complete `model`, optional `instance`/`output` and selectors | Paged saved declarations, resolved instance values, and optional regenerated geometry |
| `occt_inspect_build` | `build_id` from `occt_build`, plus the same optional inspection fields | Inspection of the accepted model without resending its JSON |

For example, call `occt_inspect_build` with:

```json
{
  "build_id": "BUILD_ID_RETURNED_BY_OCCT_BUILD",
  "instance": "shaft",
  "output": "body",
  "limit": 20,
  "face_selector": {
    "normal_aligned": {
      "direction": { "literal": {
        "x": { "value": 0, "dimension": "scalar", "unit": null },
        "y": { "value": 0, "dimension": "scalar", "unit": null },
        "z": { "value": 1, "dimension": "scalar", "unit": null }
      }},
      "minimum_dot": { "literal": {
        "value": 0.99, "dimension": "scalar", "unit": null
      }}
    }
  }
}
```

Without `instance`, inspect the saved family/instance/pattern declarations and
assembly requirements. With `instance`, inspect its inherited overrides,
validated input and derived parameter values, parameter definitions/limits,
feature operations and complete inputs (including named-reference dependencies),
requirements, constraints, datums, references and assumptions. Metadata-only
inspection creates no geometry; saved driven-pattern members are not refreshed
until geometry is requested.

With `output`, the engine first regenerates and verifies the graph, refreshing
driven members, then regenerates one **family-local authoring snapshot**. Face
and edge queries use that snapshot, matching the coordinate system in which
feature selectors execute. Assembly placement and frame-chain values are
reported separately. This prevents a translated/rotated instance from turning
world-space measurements into incorrect downstream feature selectors. Reports
record graph variant count and the one additional authoring regeneration.

Optional `face_selector` and `edge_selector` run the actual selector engine,
including named references and history/persistent selection. Without selectors,
all faces/edges of the selected output are eligible. Faces report area, center,
bounds, planarity and planar normal; curved faces have no whole-face normal.
Edges report length, center, bounds and native circle radius when available.
Shape type, validity and volume are also measured. Requirement evidence comes
from graph regeneration; required failures return the existing failure report.

`offset` defaults to zero and `limit` to 100 (allowed 1..1000). Each declaration
or geometry collection returns `total`, `offset`, `next_offset` and `items`.
The same offset/limit applies independently to each collection. Topology
`selection_index` values belong only to that selection in that regenerated
snapshot; use the tested semantic selector or a named reference for edits.
Only the requested pages are measured, while regeneration and topology traversal
still depend on the model's full complexity.

Inspection produces no accepted build/artifact directory and cannot change an
accepted model. Worker scratch files are removed even on success. The native
CLI exposes the same operation:

```sh
LD_LIBRARY_PATH="$PWD/build" rust/occt-parametric/target/debug/occt-model \
  --inspect /tmp/inspection.request.json /tmp/new-inspection-report.json
```

Additional schema names are `inspection`, `face_selector`, and `edge_selector`.
`occt_get_schema` and `resources/list` expose all twelve schema targets. The
`occt_inspect_model`/`occt_inspect_build` input schemas are discoverable through
`tools/list`. The report file must be new; the CLI does not overwrite it.

Three additional command tests cover paging, inherited values, named-reference
inputs, local coordinates, face/edge queries, handle cleanup, invalid requests
and report preservation. One additional MCP test checks both inspection tools,
accepted-build preservation, selector measurements and paging. Release checks
include a 10,000-instance metadata inventory and a 1,000-feature geometry
inspection with two-item pages, each with a 10-second budget.

## Guarded edits and revision history

`occt_edit_build` edits an accepted snapshot by stable family/entity IDs and
publishes a new build. It requires `build_id`, `expected_model_sha256`, `changes`,
selected `outputs`, and explicit `revision` metadata. Optional `edits` apply
instance parameter overrides; STEP/STL/preview flags match `occt_build`.
`occt_build` now returns `model_sha256`, and `occt_inspect_build` returns
`source: { build_id, model_sha256 }`. Use the fingerprint from the snapshot
actually inspected, rather than guessing the current feature definitions.

```json
{
  "build_id": "SOURCE_BUILD_ID",
  "expected_model_sha256": "SHA256_RETURNED_BY_BUILD_OR_INSPECTION",
  "changes": [{
    "action": "add_feature",
    "family": "shaft",
    "feature": {
      "id": "moved",
      "operation": { "translate": {
        "input": "body",
        "offset": { "literal": {
          "x": { "value": 10, "dimension": "length", "unit": "millimeter" },
          "y": { "value": 0, "dimension": "length", "unit": "millimeter" },
          "z": { "value": 0, "dimension": "length", "unit": "millimeter" }
        }}
      }}
    }
  }],
  "outputs": [{ "instance": "shaft", "output": "moved" }],
  "revision": {
    "id": "shaft-move-1",
    "author": "AI acting for the user",
    "recorded_at": "2026-10-06T00:00:00Z",
    "message": "Add a translated shaft output for the requested interface."
  },
  "step": true,
  "preview": true
}
```

Replace the ID/hash placeholders and use actual caller-supplied review metadata.
Attach appropriate new requirements and trace links when adding functional
features. Existing requirements retain their exact definitions; checks tied
to an earlier output do not automatically become checks on a new output.

| Action | Payload after `family` | Guard/behavior |
| --- | --- | --- |
| `add_feature` | `feature` | ID must be new |
| `replace_feature` | `expected`, `feature` | Entire expected definition must match; replacement keeps the ID |
| `remove_feature` | `expected` | Entire expected definition must match; dangling dependencies/traces are rejected |
| `add_parameter` | `parameter` | ID must be absent from input and derived parameters |
| `add_requirement` | `requirement` | ID must be new; new required checks run before publication |
| `add_reference` | `reference` | Name must be new; selector references are validated |

Take `expected` from the inspected/saved feature definition (`id` and
`operation`). Each request may target a family/entity identity only once.
Family-scoped declaration edits affect every instance using that family;
parameter `edits` affect their named instances according to clone inheritance.
The tool preserves existing requirements, assumptions, parameter definitions,
and named references; this first version adds those declarations and edits
features rather than rewriting engineering intent. Existing requirements
cannot be weakened/deleted through this tool. Modified family definitions
increment their version once; parameter-only edits retain the family version.
At most 10,000 declaration changes and 10,000 parameter edits are accepted,
with at least one of them present.

The MCP gateway verifies the SHA-256 of the accepted model bytes before starting
work. Feature guards then check the typed definitions in that exact captured
snapshot. A different source fingerprint, stale/missing expected feature,
colliding ID, renamed replacement, duplicate edit, invalid model or required
verification failure rejects the edit. Semantically unchanged edits are rejected;
a newly explicit override can change model intent even if geometry is unchanged.
These are immutable branches from an explicit source build, without a shared
mutable “current” build. Concurrent edits create separate children; they cannot
overwrite each other's sources. This is snapshot checking, not shared-head
revision locking or a merge service.

Successful builds include `revision`, `source`, `change_count` and the
`changes.json` artifact. The saved model appends one existing-format
`DocumentRevision` containing the actual semantic changes after generation,
including parameter overrides and refreshed patterns. Review metadata preserves
the previous revision as `parent`; IDs must be new and all metadata fields
nonempty. `changes.json` is the same revision record and is readable through
MCP resources. Source models and earlier build directories remain intact.
Native failures and unsuccessful/no-op revision finalization remove only the
new build's directory. As with all builds, the completion report is the success
marker; process termination can leave partial scratch output.

CLI use requires a complete baseline model and the same edit fields, with
`schema: "occb-model-edit-v1"`:

```sh
LD_LIBRARY_PATH="$PWD/build" rust/occt-parametric/target/debug/occt-model \
  --edit /tmp/edit.request.json /tmp/new-edited-build
```

The CLI relies on the supplied model and feature guards; it has no parent-build
store to verify a fingerprint. Optional `source` is caller-provided provenance
there. The MCP tool loads/verifies the accepted snapshot and supplies source
provenance itself. Schema targets `edit` and `change` are available through the
CLI, discovery tools and resources, bringing the total to eleven schemas.

Three additional command tests cover guarded feature replacement, additions,
removal, new requirements/references/parameters, parameter-only revision chains,
requirement preservation, stale guards, dependency rejection, required failures
and no-op cleanup. One additional MCP test covers fingerprint/feature conflicts,
verified geometry, revision resources, requirements and source preservation.
The scale suite edits all 10,000 feature identities, checks the new dimensions
and volume, verifies the complete revision ledger and unchanged source, and
retrieves the accepted model within a 30-second budget.

[The annotated viewer](VIEWER.md) adds sketch constraint symbols, native residual
verdicts, dimension/control links, failed geometric-check witnesses and
interactive solid orbiting. Accepted previews include it automatically;
`occt_visualize_model` can display unaccepted proposals without promoting them
into builds. `view` is the twelfth schema, and `sketch-block`/`sketch-conflict`
are additional diagnostic examples.

`sketch-advanced` demonstrates the schema-72 sketch additions. Its view request
contains all new constraints, an ellipse solid, and native trim/extend/offset
profile operations. See [Sketches](../../SKETCHES.md).


`extrusion-limits` demonstrates symmetric, up-to-face and up-to-next extrusion.
Change `depth` to move the target body: `body` follows its nearest face and
`selected` follows its far face. `symmetric` uses `depth` as total centered
length. Limit faces must terminate the complete profile strictly forward; see [extrusion semantics](../../EXTRUSIONS.md).


`curved-extrusions` demonstrates schema-74 inclined and spherical limits.
Its view request uses the native centroid ray measurement and tracks both
surfaces when `depth` changes. Crossing next-face limits require an explicit
up-to-face selector. Native ABI 48 is required.


`drill-point` demonstrates schema-75 blind-hole bottoms. Its `point_angle`
parameter is the included angle in radians; `depth` remains the full-diameter
bore depth. The engine verifies tip containment before cutting, and the viewer
links angle, diameter and depth dimensions to edits. See [hole semantics](../../HOLES.md).
