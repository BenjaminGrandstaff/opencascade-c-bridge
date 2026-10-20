# AI tools over local MCP

The stdio server exposes the `occt-model` build loop and existing design commands
to an MCP client.
It uses Python 3.11+ standard-library modules and built Rust executables;
there are no Python runtime package dependencies. It targets POSIX pipe I/O.
The server implements the MCP [stdio transport](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports),
[lifecycle](https://modelcontextprotocol.io/specification/2025-11-25/basic/lifecycle),
[tools](https://modelcontextprotocol.io/specification/2025-11-25/server/tools), and
[resources](https://modelcontextprotocol.io/specification/2025-11-25/server/resources)
for protocol versions `2025-11-25` and `2025-06-18`.

## Launch and connect

From the repository root:

```sh
OCCT_BRIDGE_LIB_DIR="$PWD/build" LD_LIBRARY_PATH="$PWD/build" cargo build \
  --manifest-path rust/occt-parametric/Cargo.toml --bins
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
per-worker timeout in seconds (default 120). Up to eight requests may be
pending; workers run independently. Cancellation and stdin closure terminate
pending workers and remove their unaccepted work directories. No HTTP listener
or remote service is started.

Build all companion commands before running the full wire test suite. The
server discovers optional design executables beside `--binary`. Use
`--commands-dir /absolute/directory` when they are installed elsewhere. Only
installed commands are advertised as tools; the `unavailable_tools` field of
`occt://catalog` reports missing tools. Rebuild the commands alongside the model
binary when updating the engine. A model-only installation retains the original
part tools.

## Tools and resources

| Tool | Arguments | Result |
| --- | --- | --- |
| `occt_get_schema` | `name`: a schema name from `occt://catalog` | Complete generated JSON Schema |
| `occt_get_example` | Any name listed by `tools/list`, including `sketch-repair` | Complete editable build, visualization or command request |
| `occt_visualize_model` | `occb-model-view-v1` request | Annotated sketch/solid viewer and diagnostic snapshots |
| `occt_inspect_model` / `occt_inspect_build` | Model or accepted build ID, instance and output, sketch feature or loft feature | Read-only declarations, geometry queries or focused sketch/loft diagnosis |
| `occt_preview_edit_build` | Accepted build ID/hash, guarded changes, outputs and proposed revision metadata | Diagnostic candidate with no accepted build or new revision |
| `occt_edit_build` | Accepted build ID/hash, guarded changes, outputs and revision metadata | Verified new build and semantic change record |
| `occt_export_drawings` | Model and drawing setup | SVG, DXF, retained drawing model and manifest |
| `occt_inspection_report` | Model and supplied measurements | Complete dimensional/GD&T/texture/thread report, including failed checks |
| `occt_motion_study` | Model and component drives | Sampled and continuous collision reports, assembly and study |
| `occt_balance_report` | Model, materials and inline chord reference | Mass, inertia, per-material totals and geometric CG |
| `occt_joint_branches` | Model and explicit seed axes | Bounded search report and reloadable branch models |
| `occt_merge_documents` | Base, current and other model snapshots | Merged model or complete semantic conflicts |
| `occt_build` | The entire `occb-model-request-v1` object | Accepted report, build ID, local directory, and artifact resource URIs |
| `occt_list_templates` | none | Design templates: purpose, checks and typed inputs |
| `occt_instantiate_template` | `template` and optional `inputs` | A complete build request from the template, or a `template` failure for bad inputs |
| `occt_session_read` | Optional local studio `url` | The studio's current model, `model_sha256`, version, unsaved state and last shared change |
| `occt_session_apply` | Accepted `build_id`, `expected_model_sha256`, `message`, optional `author` and `url` | The model applied to the studio, or a `conflict` when the person changed it first |

`tools/list` provides the generated complete request schema as `occt_build`'s
input schema. All feature operations and their nested request-schema definitions are
discoverable. Schemas derive from serde-compatible Rust types;
new serialized variants require schema support at compilation. Authoring schemas
include the current model version as
`$defs.ModelDocument.properties.schema_version.const` in the request schema.
The engine still migrates older documents.
Schemas describe serialization and basic request bounds; units, dependency
references, dimensional constraints, selector resolution, and geometry validity
are checked by the build engine.

Schema discovery also works without MCP:

```sh
LD_LIBRARY_PATH="$PWD/build" rust/occt-parametric/target/debug/occt-model \
  --schema request > /tmp/occt-request.schema.json
```

`resources/list` advertises `occt://schema/NAME`, `occt://example/NAME`, and
`occt://catalog` resources. The catalog reports the current model schema version,
supported schema names, feature operation names, example request kinds, and
counts of tools, schemas, examples, resources and request definitions. Counts
come from the loaded schemas and server declarations. Every `*.request.json`
in this directory is automatically exposed; restart the server after adding one.
Read the catalog at runtime instead of relying on a count copied into prose.
Accepted builds return specific
`occt://build/BUILD_ID/ARTIFACT` URIs; a resource template advertises this form.
JSON and SVG resources return text, while STEP, STL and PNG return base64 blobs.
Reads are limited to 32 MiB; larger artifacts remain available at the returned
local directory. Only listed artifacts from completed builds can be read.
Request files, worker logs, arbitrary paths, and incomplete builds are excluded.

Tool results provide both `structuredContent` and equivalent JSON text content.
When a build, edit or edit preview sets `renders`, the result also carries up
to eight of its PNG renders as `image` content (at most 4 MiB together, in
report order) and lists them in `inline_images`; every render stays readable
as an `image/png` resource. See [rendered views and sections](README.md#rendered-views-and-sections).
Build/verification failures return `isError: true` with the native structured
failure report. Protocol errors use JSON-RPC errors. Native STEP progress stays
inside the worker subprocess and cannot appear on MCP stdout. Builds and edits
rejected by required requirements return the complete verification table,
measured values, witnesses and output measurements in the failure, with
`accepted: false`. See the [build contract](README.md).

## AI workflow

1. For a new design, `occt_list_templates` and instantiate one that fits
   ([templates](../../docs/TEMPLATES.md)); otherwise read `occt_get_schema`
   and an appropriate example.
2. Describe the function/interfaces as requirements; record uncertain inputs as
   explicit assumptions with provenance, rather than inventing verified facts.
3. Call `occt_build` with explicit units and selected final outputs.
4. Inspect the verification table and its exact/sampled evidence. Set
   `renders` to look at the result: standard views, plus section planes
   through walls, bores or bends. Read model/report resources for details.
5. Read accepted `model.json`, put it into the next request, and provide typed
   parameter edits. Required failures preserve the preceding accepted build.
6. Return STEP/STL artifacts and the editable model to the user.

This adapter provides discovery, execution, inspection and guarded shared studio
updates. Natural-language design planning, research, domain knowledge graphs and
specialist-agent coordination belong in the calling system. See
[using the engine from an AI design system](../../docs/AI_DESIGN_SYSTEMS.md) for
that usage pattern. The adapter does not replace a client's approval policy or
prove unverified strength/manufacturability.

## Printable dungeon map examples

The examples `map-crossroads`, `map-catacombs`, `map-shrine`, `map-bridge`, and
`map-cavern` build five distinct tabletop maps with existing engine operations.
Each exports one connected solid as STL and STEP, plus native top/isometric
renders and an editable model. The replay script retrieves and builds every
example through stdio MCP, checks exported mesh edges, and makes a colored
interactive gallery. See [the five maps and reproducible workflow](../../docs/PRINTABLE_MAPS.md)
for construction plans, downloads, verification and printing limits.

## Validation and scale

Wire tests use the actual server and model executable. They independently check
every exposed schema with a Draft 2020-12 validator, validate every example
against its build, visualization or command schema and current model version, test
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

With `instance` and `loft_feature`, inspect the loft's source boundaries in
isolation, even when its correspondence prevents constructing the solid.
The paged `loft.profiles` contains station/track IDs, source/aligned edge counts,
exact starts, reversal and stable issue codes. `loft.issues` contains global
track/mode/end-condition problems. Paged `loft.stations` checks hollow station
coplanarity, strict containment and separate holes, reporting native material
area or a `station_region` issue with profile IDs. `checks_passed` applies only to these checks;
`solid_checked` is false. This mode is mutually exclusive with `output` and
`sketch_feature`, and does not certify between-station hole containment, fold-back, final solid
validity or requirements. See [profile diagnosis](../../docs/LOFTS.md#read-only-profile-diagnosis).

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
`occt_get_schema` and `resources/list` expose the installed schema targets. The
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
CLI, discovery tools and resources.

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
into builds. `view` is available as a schema, and `sketch-block`/`sketch-conflict`
are additional diagnostic examples.

`sketch-advanced` demonstrates the schema-72 sketch additions. Its view request
contains all new constraints, an ellipse solid, and native trim/extend/offset
profile operations. See [Sketches](../../docs/SKETCHES.md).


`extrusion-limits` demonstrates symmetric, up-to-face and up-to-next extrusion.
Change `depth` to move the target body: `body` follows its nearest face and
`selected` follows its far face. `symmetric` uses `depth` as total centered
length. Limit faces must terminate the complete profile strictly forward; see [extrusion semantics](../../docs/EXTRUSIONS.md).


`curved-extrusions` demonstrates schema-74 inclined and spherical limits.
Its view request uses the native centroid ray measurement and tracks both
surfaces when `depth` changes. Crossing next-face limits require an explicit
up-to-face selector. Native ABI 54 is required by the current build.


`drill-point` demonstrates schema-75 blind-hole bottoms. Its `point_angle`
parameter is the included angle in radians; `depth` remains the full-diameter
bore depth. The engine verifies tip containment before cutting, and the viewer
links angle, diameter and depth dimensions to edits. See [hole semantics](../../docs/HOLES.md).


`hole-limits` demonstrates schema-76 holes up to a selected or next input face,
including spherical exits. Change `depth` to move the cap; native bore-centre
measurements follow it. Named/persistent selectors use the normal reference and
dependency rules. Entry recesses must remain before the selected face, and drill
points require numeric blind depth. See [geometry-driven holes](../../docs/HOLES.md).


`profile-loft` demonstrates schema-77 lofts directly between saved sketch
profiles. Edit section radii or `height`; dependencies regenerate the native
solid and viewer spacing. Profiles must be closed planar single boundaries.
The authoring schema includes `profile_loft` with 2–1,000 section IDs and a
`ruled` flag. See [saved-profile loft semantics](../../docs/LOFTS.md).

`curved-pipe` demonstrates a native circular sketch swept along a tangent line
and quarter-circle route. The solid view measures native route length, exposes
`run` and `bend_radius` controls, and overlays the sampled route. Changing
`radius` changes the section; it does not change route length. Dimensions are
family-local and follow placement in the assembly viewer.

`revolved-ring` defines a rectangular radial/axial sketch and revolves it about
its Z axis. `inner_radius`, `wall`, `height` and signed `angle` controls regenerate
the part. Angles are dimensionless radians; a zero sweep is rejected by the
kernel, and the magnitude may not exceed one full turn. The solid view shows
a linked angular arc around the native axis.

`symmetric-revolve` demonstrates schema-78 `revolve.extent: "symmetric"`. Its
90-degree total angle runs from -45 to +45 degrees about the source sketch
plane. Changing the signed angle regenerates geometry and the linked arc.
The feature schema exposes `angle` and `symmetric` extent values; older missing
extents default to `angle`. See [revolution semantics](../../docs/REVOLUTIONS.md).

`hollow-profile` demonstrates schema-79 `planar_region` between independent
outer/inner sketch profiles, followed by extrusion. All three dimensions link
to the solid; changing the hole radius rebuilds the inner sketch, region and
body. Invalid containment and overlapping/touching holes are rejected. The
feature schema exposes 1–100 inner profile references. See
[planar region semantics](../../docs/PLANAR_REGIONS.md).

`hollow-sweep` uses the existing schema-79 `sweep` feature over a holed planar
region and a tangent line/arc route. Inner/outer section controls and native
route length link to the solid. ABI 50 retains composed wall ancestry when
extracting the final solid. The current engineering layer handles up to 100
inner boundaries; see [hollow sweeps](../../docs/HOLLOW_SWEEPS.md).

`hollow-loft` demonstrates schema-80 optional `profile_loft.holes` arrays of
inner section tracks. Each track has the same section count as the outer track.
The example exposes two bore radii and all four section sketches, with linked
solid controls and measured spacing. Station containment and whole-loft crossing
checks reject invalid tracks; older missing hole arrays remain solid lofts.
See [hollow lofts](../../docs/LOFTS.md#explicit-hollow-tracks-schema-80).

`mirrored-part` demonstrates schema-81 plane reflection of an asymmetric bracket.
`plane_x` moves the plane and `plane_tilt` changes its normal direction. Native
geometry and source-face history remain valid after reflection, with correct
outward normals. Viewer annotations expose plane data and related controls.
See [mirrors](../../docs/MIRRORS.md); current builds require native ABI 54.


`scaled-part` demonstrates schema-83 uniform scaling of an asymmetric bracket.
`scale_factor` is dimensionless and positive; `scale_center_x` moves the centre
in millimetres. Native geometry, bounds, volume and source-face ancestry follow
edits. The viewer links both controls to the scale label and displays measured
result spans. See [scaling](../../docs/SCALING.md). Current capabilities are available in `occt://catalog`.


`spring` combines the schema-82 helix operation with a parameter-driven circular
section and native binormal sweep. Radius, pitch, fractional turns and wire
radius are editable. The section plane follows radius and pitch edits; the
viewer links helix dimensions and samples the route per turn. Geometric guards
are illustrative assumptions. See [springs and coils](../../docs/HELIX.md).


`offset-part` demonstrates schema-85 native joined skin offsets. Positive
`allowance` expands the spherical example; negative values contract it. Both
`allowance` and `offset_tolerance` are lengths; distance must be nonzero and
tolerance positive. Geometry, native ancestry and the signed driving annotation
follow edits. See [offset semantics](../../docs/OFFSETS.md).


`multi-hole-plate` demonstrates schema-86 `compound`: nine independent
cylindrical cutters are grouped and used in one `cut`. Edit `spacing`,
`hole_radius` or `thickness`; geometry and linked controls regenerate. Selecting
`tools` as an additional output shows the group and its source IDs. The schema
allows 1–10,000 distinct input IDs. See [compound groups](../../docs/COMPOUNDS.md).


`patterned-plate` demonstrates schema-88 `linear_pattern`. One cutter is
repeated into columns, then the column group into rows; one cut removes the
grid. `columns` and `rows` are integer-valued scalar parameters, and `spacing`
is a length. Count changes resize the plate and tool grid. Pattern outputs
show linked count, step and first-to-last placement span.
See [linear part patterns](../../docs/PART_PATTERNS.md).


`bolt-circle` demonstrates schema-89 circular part patterns with one repeated
cutter and one cut. Count and sweep derive angular spacing; an illustrative
chord-distance constraint keeps holes apart. Selecting `tools` as an output
shows linked count and angular-step labels with a native source-radius arc.
See [circular patterns](../../docs/PART_PATTERNS.md#circular-patterns-schema-89).


History selectors now inspect all bores created from a repeated source cutter:
`face_selector.history` can name `cutter`, select its largest nonplanar face,
and request `modified` on `body`. The grid example returns nine faces and the
bolt-circle example six. Native grouped history and Boolean composition retain
those links; MCP wire tests build both examples and check actual selected counts.
See [pattern ancestry](../../docs/PART_PATTERNS.md#retained-pattern-ancestry).


`threaded-rod` demonstrates the existing modeled-thread operation with linked
major diameter, pitch, run and derived turns. Its source radius and height
follow diameter/run edits, with an illustrative unthreaded margin at both ends.
Native validation and removed-volume checks cover the generated solid. See [thread viewer semantics](../../docs/THREADS.md#ai-example-and-viewer-dimensions).


`edge-treatments` demonstrates constant-radius fillets and equal-distance
chamfers of four semantically selected vertical block edges. Its two view
outputs compare independent rounded and beveled alternatives. Labels link the
treatment value and selector controls; overlays reference the selected source
edges. For a build/export request, select one output per instance.
See [edge treatments](../../docs/EDGE_TREATMENTS.md).


`variable-fillet` exposes the existing endpoint/interior radius law and
from-point spine direction. Its linked controls edit endpoint and middle
radii plus normalized station position. Visualization returns the complete
nominal law and source references, with explicit nonspatial station metadata.
See [variable contour laws](../../docs/EDGE_TREATMENTS.md#variable-radius-contour-laws).


`equal-radius-plate` demonstrates schema-92 `equal_radius` sketch constraints.
Two hole-wire definitions retain the same explicit circle relationship and
select different profile boundaries. A planar region and extrusion build the
plate. Source sketches, real equality residuals and related controls are
visible alongside the solid.
See [equal circular radii](../../docs/SKETCHES.md#equal-circular-radii-schema-92).


`face-pocket` demonstrates schema-93 planar face attachment. A semantic top-face
selector places the sketch at the block face centre; height edits propagate
through the cutter and final pocketed solid. The example visualizes both the
body and native profile face, alongside the local sketch's support metadata.
For build/export select only `body`. See [face-attached sketches](../../docs/FACE_SKETCHES.md).


`projected-pocket` demonstrates schema-94 linked edge projections. A named
imported line drives point-on-curve and distance constraints, positioning a
pocket from the block's actual boundary. Source-depth edits move both reference
and pocket without a copied depth expression in the sketch. The viewer colours
external geometry and exposes linked source controls. See
[sketch projections](../../docs/SKETCH_PROJECTIONS.md).

`concentric-bushing` demonstrates schema-95 shared-centre constraints driving
an annular solid. Both source sketches expose `CONC` annotations with actual
millimetre residuals and links to centre controls. `projected-pocket` also
uses the new midpoint relation to position its cut from linked geometry.

The current `projected-pocket` example also demonstrates schema-96
`point_line_distance`: a signed margin from a solved midpoint to a linked
source edge. Its dimension has perpendicular foot/point anchors, measured
millimetre residuals and a linked `margin` control. Editing that control
rebuilds the native pocket while reusing the source block.

`tangent-boss` demonstrates schema-97 independent full-circle tangencies.
A side-selected line/circle relation and external circle/circle relation solve
the boss location without shared point IDs, then a native profile is extruded.
The viewer publishes calculated contact anchors, target entity IDs and real
millimetre residuals. The request schema exposes `SketchLineSide` and
`SketchCircleTangency`.

`arc-tangent-boss` demonstrates schema-98 directed arc contacts. The independent
line/circular tangencies accept arcs in their circular roles and measure both
supporting-circle tangency and directed-span membership. Its boss arc and
closing chord generate an exact native segment face and solid. The viewer
exposes span-inclusive millimetre residuals and calculated contacts.

`rational-spline-cap` demonstrates schema-99 explicit B-spline bases with a
parameter-driven rational weight. Native ABI 53 builds the supplied poles,
knots, multiplicities and weights directly; the viewer publishes their basis
metadata, linked controls and dashed control polygon.

`periodic-spline-pad` demonstrates schema-100 periodic explicit bases, native
seam closure, closed control polygons and a linked rational corner weight.
Native ABI 54 introduced the periodic flag semantics.

The `strict-sketch-block` example demonstrates sketch `acceptance`:
`fully_constrained` requires satisfied equations and zero local free degrees.
Set the `lift` override to 0 mm to obtain diagnostic scenes with
`solver.solved: true`, `solver.accepted: false`, and a failed policy annotation.
`occt_build` rejects that model at regeneration; `occt_visualize_model` retains
the source sketch and solver evidence. See
[sketch acceptance](../../docs/SKETCHES.md#sketch-acceptance-policy-schema-115).

The `projected-spline-pad` example imports an exact rational spline into a tilted
sketch plane (`kind: spline`), closes its stable generated endpoints, and
extrudes the face. Source weight edits update the linked curve; viewer data
exposes its explicit basis/control polygon and source controls. The same kind
imports trimmed elliptic edges, periodic and non-clamped source domains. See
[exact sketch projections](../../docs/SKETCHES.md#exact-spline-and-trimmed-conic-projections-schema-116).

The `projected-chain-pad` example uses `kind: chain` to import an entire connected
multi-edge boundary as one rational spline. It supports mixed degrees with
exact elevation and linked source edits, with branch/disconnect/ambiguity
rejection. The composite is one native edge; see
[ordering and topology limits](../../docs/SKETCHES.md#connected-multi-edge-projection-schema-117).

The `projected-junction-pad` example adds named chain `junctions` and uses their
fixed point IDs in a coincident constraint and width dimension. Set `width` to
60 mm to move the marker, or `junction_shift` to 0.1 mm to exercise a guarded
missing-match failure. Hint parameters invalidate the projection independently
of source geometry. See
[named references](../../docs/SKETCHES.md#named-chain-junction-references-schema-118).


`occt_get_example {"name":"projected-intersection-pad"}` returns a
view request with circle–line crossings driving a solid pad. Named intersection
annotations target both projected curves and their fixed reference point, and
include controls from both sources. Change `radius` to move the pad; overly broad
`match_radius` or a shifted missing hint rejects the build.
See [intersection rules](../../docs/SKETCHES.md#named-projected-intersections-schema-119).


ABI 61 adds `projected-spline-intersection-pad` and
`projected-ellipse-intersection-pad`, available through `occt_get_example` and
example resources. Both are view requests with exact native crossings,
fully constrained pads, direct crossing dimensions and both source controls.
See
[native crossings](../../docs/SKETCHES.md#native-projected-intersections-abi-61).


`occt_get_example {"name":"projected-tangent-intersection-pad"}` returns the
isolated-touch demo (introduced in schema 119). A projected line touches a quadratic spline,
and its named contact drives a pad and its dimension. Change `width`/`rise` to
move it; positive `offset` rejects no contact, negative `offset` rejects two
contacts. See [touch rules](../../docs/SKETCHES.md#isolated-line-spline-touches-schema-119--abi-61).

## Sketch edit and repair workflow

Use `occt_get_example` with `sketch-repair` for a buildable sketch containing a
deliberately duplicated driving angle. Examples use the current model schema;
this workflow preserves the model’s existing requirements.

1. Build the example and retain its accepted `build_id` and `model_sha256`.
2. Call `occt_inspect_build` with `instance: "part"` and
   `sketch_feature: "profile"`. This is mutually exclusive with output geometry
   inspection. It returns the exact authored definition/configuration, solved
   points in sketch-local millimeters, paged indexed constraint snapshots,
   residual checks, solver/rank diagnostics and bounded repair candidates.
3. Make a guarded `edit_sketch` change using that authored definition as
   `expected`. Changes run sequentially on a private candidate. Entity updates
   preserve their IDs; constraint indices refer to the current candidate after
   earlier changes, and each replacement/removal checks its expected value.
4. Call `occt_preview_edit_build` with the accepted ID/hash, changes, outputs and
   proposed revision metadata. The result has `visualization_id`, diagnostic
   artifact URIs, `status: "previewed"` and `accepted: false`. A failed solver
   can still produce inspectable diagnostic geometry and conflict evidence.
5. Review the candidate's `view.json` or viewer. Adjust the proposal against the
   original accepted snapshot if it fails. A preview cannot be used as the
   source of `occt_inspect_build` or `occt_edit_build`.
6. Submit the intended proposal to `occt_edit_build`. Publication reruns normal
   output generation and requirement verification, records one semantic revision
   and leaves the source build unchanged. Preview does not consume a revision ID.

```json
{
  "action": "edit_sketch",
  "family": "sketch-repair",
  "feature": "profile",
  "expected": "COPY THE AUTHORED DEFINITION OBJECT FROM INSPECTION",
  "changes": [{
    "action": "remove_constraint",
    "index": 6,
    "expected": {"line_angle": {
      "line": "AB",
      "value": {"literal": {"value": 0, "dimension": "scalar", "unit": null}}
    }}
  }]
}
```

The `expected` placeholder above must be replaced with an object. Read the
`sketch_change` schema for complete typed operations:

- `upsert_element`: omitted/null expected value adds an unused ID; an expected
  point/line/circle/arc/ellipse/spline/shape/text/pattern/projection/measurement
  replaces exactly that element. Fixed coordinates drive geometry; free point
  coordinates are solver seeds. Other driving expressions remain untouched.
- `remove_element`: removes an exact expected element. Dangling references fail
  validation and do not publish.
- `add_constraint`, `replace_constraint`, `remove_constraint`: explicit typed
  constraints with expected-value guards where existing equations change.
- `configure`: replaces exact expected non-entity configuration: sketch frame,
  support/datum, acceptance, profiles/holes/regions/discovery and saved curve or
  boundary operations. It does not replace points or constraints.

`replace_reference` and `remove_reference` provide expected-value guards for
family named references. A replacement preserves the reference name. Geometry
and reference changes can be submitted atomically when a curve/region identity
intentionally changes. Requirements cannot be removed through these changes.

`add_datum`, `replace_datum` and `remove_datum` edit family datums the same
way. Besides `point`, `axis` and `plane`, a datum may be a constructed plane
(schema 165): `offset_plane` (a base `xy`, `xz`, `yz` or `{"datum": id}`
moved by `distance`), `angle_plane` (a base turned by `angle_radians` about an
in-plane `hinge`), `face_plane` (one planar face of `input` selected by `face`,
moved by `distance`), `midplane` (halfway between two parallel faces of
`input`) and `three_point_plane`. Each is fully defined by its references and
expressions; planes on faces follow those faces on every regeneration, and a
sketch on any of them keeps its x axis projected into the plane.

Focused diagnosis builds only the selected sketch and required source geometry;
unrelated/downstream failures do not prevent source-sketch analysis. It does not
certify the rest of the part or its requirements. Structurally invalid requests
and unavailable source geometry fail explicitly; local solve errors appear in
`sketch.error`. Repair candidates describe
local redundancy, conflict groups and free motion; they never choose design
intent, execute automatically or add arbitrary fixed-point constraints. Local
redundancy does not prove equivalence across every parameter configuration.

## Manufacturing examples

`hemmed-tray` is a build request for a 2 mm sheet with two main bends,
relieved side walls, a closed end hem, and a slot spanning the first bend.
Its `folded` and `flat` instances select different features from the same
parameter variant. The linked blank uses a neutral factor of 0.5; its exact
volume matches the folded part. `neutral_factor` can be edited to explore bend
allowances; production allowances require material and tooling calibration.

`tapped-shell` opens the top of a block with an inward 6 mm shell, then drills
a 5 mm bore through its bottom wall with an M6x1 thread declaration. Thread
geometry is cosmetic (no helical material removal); the exact volume check
therefore measures the shell minus the cylindrical tap-drill bore. `tap_drill`
is editable. Dimensions and thread engagement are demonstration assumptions.

Retrieve either with `occt_get_example`, then pass the complete request to
`occt_build`. Inspect the accepted build before guarded parameter edits. Both
examples retain required shape-validity checks and export STEP plus annotated
viewer artifacts. Wire tests build them, compare exact volumes, and rebuild
a changed tap drill without modifying the original accepted build.

## Shared studio sessions

An AI client and a person can edit one model together. The person runs the
studio (`occt-view MODEL.json --serve`, by default on port 37391); the MCP
server talks to it at `--studio-url`, `$OCCT_STUDIO_URL` or
`http://127.0.0.1:37391`, or at a tool call's `url`. Only
`http://127.0.0.1:PORT` and `http://localhost:PORT` are accepted.

1. `occt_session_read` returns the studio's in-memory model (including the
   person's unsaved edits) and its `model_sha256`.
2. Build or edit that model with `occt_build`, `occt_edit_build` or
   `occt_preview_edit_build` and review the evidence and renders.
3. `occt_session_apply` sends the accepted build's model with the read hash
   and a short `message`. The studio applies it only if its model still has
   that hash; otherwise the result is `isError` with stage `conflict`, and the
   client reads the session again and rebases its change. Builds that were not
   accepted (previews, visualizations, unknown IDs) are refused.

The studio validates and regenerates the model before showing it. An applied
change enters the studio's undo history, so the person can undo it in one step, and
it stays unsaved until they save. Both studio pages name the author and
message (`AI assistant changed the model: …`) while that change is the
version on screen. There is no lock: a person's edit or an undo simply changes the
hash and makes the client's next apply fail with a conflict. Session replies
come from the studio's single request loop, so an apply is atomic.
Stages are `connection` (no studio at the URL), `conflict`, `request` and
`studio` (the studio's validation or regeneration error). Reads and applies are
O(model size): a 1,000-instance model (0.32 MB) read in 0.011 s and applied,
validated and regenerated in 0.033 s.

## Drawing, inspection and mechanism workflow

Each command tool takes a complete generated request with `schema`, `model`
and `setup`. The corresponding schema names are `drawing_export`,
`inspection_report`, `motion_study`, `balance_report` and `joint_branches`.
`document_merge` instead takes `schema`, `base`, `current` and `other`. Each
command executable also prints its complete MCP request schema with `--schema`;
its ordinary positional CLI contract remains available. All model snapshots
use the current model schema. Retrieve a complete example with `occt_get_example`:

| Example | Tool | Demonstrates |
| --- | --- | --- |
| `drawing-sheet` | `occt_export_drawings` | Saved dimensioned sheet, exact curves, SVG and DXF |
| `measured-sheet` | `occt_inspection_report` | Supplied width against explicit limits |
| `slider-clearance` | `occt_motion_study` | Clear slider travel; change end travel to −60 mm for a crossing |
| `block-balance` | `occt_balance_report` | Explicit density and geometric CG relative to a chord |
| `linkage-branches` | `occt_joint_branches` | Alternative crank-slider poses and bounded search status |
| `merged-block` | `occt_merge_documents` | Independent edits to box dimensions |

To continue from an accepted build, read its `model.json` resource and pass that
snapshot as `model`. Drawing export uses saved definitions when `setup.drawings`
is empty or omitted; supplied definitions are retained in `drawings.model.json`.
Use that retained model for subsequent measurement inspection. An inspection
report evaluates supplied measurements; it does not acquire measurements from
a machine or infer their uncertainty. Balance requires explicit/inherited
material density and an inline chord (`reference.kind: "chord"`); filesystem
`wing_project` references are available only through the direct CLI.

Command results return `analysis_id`, `command`, `status`, the complete `report`,
a server-owned `directory`, and named `resources` under
`occt://analysis/ANALYSIS_ID/ARTIFACT`. JSON, SVG and millimeter DXF are readable
text resources with the same 32 MiB read limit as build artifacts. Input paths,
logs, partial outputs and unlisted files are excluded. Timeouts and cancellation
terminate the worker and remove partial outputs. The original model snapshots
and accepted builds remain unchanged.

A completed inspection with nonconforming measurements, motion collision or
unresolved continuous check, or semantic merge conflict returns `isError: true`
and `status: "failed_checks"` **with readable reports**. Malformed requests or
worker failures publish no analysis artifacts. Branch searches expose their
termination status and bounds; a successful tool call does not prove exhaustive
discovery. Analysis results do not receive a `build_id`. Build a merged or
branch model separately to verify geometry and requirements before accepting it.

The merge CLI also supports `--report BASE CURRENT OTHER NEW_OUTPUT_DIRECTORY`
for the same nonmutating report behavior; exit code 2 retains a conflict report.
The ordinary Git merge-driver invocation continues to replace CURRENT only on
a successful merge.
