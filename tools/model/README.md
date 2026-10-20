# AI model build command

`occt-model` accepts one versioned JSON request containing a saved model,
selected instance outputs, and optional parameter edits. It validates/migrates
the model, regenerates all unsuppressed instances and their requirements, then
publishes an editable model, a machine-readable report, and requested exports.

```sh
LD_LIBRARY_PATH="$PWD/build" cargo run \
  --manifest-path rust/occt-parametric/Cargo.toml --bin occt-model -- \
  tools/model/bracket.request.json /tmp/my-bracket-build
```

The destination must be new. A successful build writes `model.json` and
`report.json`; STEP defaults on, STL defaults off, and SVG previews default on.
Numbered filenames map to the ordered `outputs` entries in the report and never
use user-provided IDs as filesystem paths. STEP is one named assembly; STL and
SVG are per selected output. Select at most one output per instance, up to
10,000 outputs and 10,000 edits. Intermediate features are excluded unless
explicitly selected.

## Request and report contract

```json
{
  "schema": "occb-model-request-v1",
  "model": { "schema_version": 179, "...": "complete ModelDocument" },
  "outputs": [{ "instance": "bracket", "output": "body" }],
  "edits": [{
    "instance": "bracket",
    "parameter": "hole_spacing",
    "value": { "scalar": {
      "value": 35, "dimension": "length", "unit": "millimeter"
    }}
  }],
  "step": true,
  "stl": true,
  "preview": true
}
```

Add `"step_pmi": "<drawing id>"` to write that drawing's GD&T into
`parts.step` as AP242 semantic PMI on the faces its attachments name; the
report's `pmi` then says whether every item read back (`verified`), whether
anything was skipped or is not representable (`complete`), and lists each
item. The drawing's `annotation_views` are written as saved views, and the
report's `views` lists what each shows and anything that did not read back.
A drawing `dataset` is written on the root product and reported in the
report's `dataset`; `pmi.release` gives the drawing's release status.
See [Semantic PMI](../../docs/DRAWINGS.md#semantic-pmi-on-persistent-faces-schema-144-abi-70)
and [Annotation views](../../docs/DRAWINGS.md#annotation-views-schema-145-abi-71).

### Rendered views and sections

Add `"renders"` for shaded PNG images of each selected output, so a client
that reads images can check what it built: interference, bend direction,
missing or misplaced features. `{}` renders the iso, front, top and right
views at 512 px. All fields are optional:

```json
"renders": {
  "views": ["iso", "front", "top", "right"],
  "sections": [{ "id": "mid", "origin_mm": [20, 15, 12], "normal": [1, 0, 0] }],
  "size_px": 512
}
```

Views are in model coordinates with +Z up: `front` looks along +Y, `top`
along −Z, `right` along −X, and `iso` from the front-right-top corner; `back`,
`bottom` and `left` are the opposites. A section removes the material on the
side its normal points to, hatches the cut face and looks back along the
normal, which shows wall thickness, bores and hems in profile. A plane that
removes the whole part reports `"empty": true` instead of an image. Faces are
flat-shaded with their B-rep boundaries and silhouettes outlined, and a
corner triad shows X (red), Y (green) and Z (blue). Sheet-metal flat patterns
are ordinary outputs, so a `top` render of the flat output shows the blank.

Each output's `renders` lists `{view|section, file, width_px, height_px}` in
request order, and files are named `NNNN-<view>.png` or
`NNNN-section-<id>.png`. Limits: 8 views and 4 sections per output, 64 to
1,024 px, and 256 images per build; invalid options fail the `request` stage
before anything is built. Renders use the tessellation, not the exact
geometry, and are for visual review only; the report's measurements stay the
evidence. Edit previews and edits take the same `renders` field and render the
candidate. Measured cost: 256 images at 512 px took 3.95 s and 55 MB; 16
images at 1,024 px took 1.38 s and 100 MB.

Use the complete examples here for valid model structure: [bracket](bracket.request.json),
[enclosure](enclosure.request.json), [shaft](shaft.request.json), and
[mating shaft/sleeve](mating-parts.request.json). Their dimensions and wall
limits are demonstration assumptions, not manufacturing qualification.
[`printed-spar`](printed-spar.request.json) checks mass, heat-deflection
margin and root bending against the [material catalog](../../docs/MATERIALS.md).

The final **stderr line** is a JSON object with schema `occb-model-report-v1`.
Exit status is zero for a completed build and nonzero for failure. Native STEP
progress can appear on stdout; use `report.json` or the final stderr line for
machine consumption. Successful reports contain selected output identities,
exact model-space bounds in millimeters, volume in cubic millimeters, BREP
validity, artifact paths, distinct generated-variant count, and all generated
part/assembly verification results. Results preserve exact versus sampled
evidence, measurements with limits/units, and witnesses where available.
`built` means regeneration and requested publication completed; inspect
individual results for unmet preferred/advisory requirements. A model without
requirements has no verified engineering intent.

Failures report `stage`, `message`, and structured kernel diagnostics when
available, including feature/operand/selector IDs and native code/name.
When required requirements fail, nothing is published and the failure (stage
`regeneration`) carries the same verification table an accepted build reports,
so a client can repair from the evidence:

- `accepted: false` and `required_failures`, the `scope/requirement` labels
  that rejected the build (the message names the first ten);
- `verification`: every result with its status, measured value and limits,
  exact or sampled evidence and witness points, failures first;
- `verification_counts`: `total`, `passed`, `failed`, `required_failed` and
  `omitted`;
- `outputs`: validity, volume and exact bounds of each selected output, as
  measured on the rejected geometry, and `outputs_omitted`;
- `generated_variants`.

Rows and outputs are capped at 500 each, so the report stays one bounded JSON
line; 600 failing instances produced a report under 700 KB. Only required
failures reject a build; preferred and advisory failures stay in an accepted
build's table. Failed geometry or failed requirement evaluation still fail
with their own message and kernel diagnostics.

## Edit and repair loop

1. Build `bracket.request.json` into a new directory.
2. Add the `hole_spacing` edit above and build into another directory.
3. Change `thickness` to 1 mm: the required 2 mm minimum-wall check rejects it.
4. Change `thickness` to 5 mm and build again.
5. For subsequent edits, place the last accepted `model.json` into the next
   request's `model` field. Stable requirement IDs and review history persist.

The command never edits the input or overwrites a previous output directory.
No files are published for validation/regeneration/selection failures. Export
failures attempt to remove only the newly created directory. Treat the report
as the completion marker; abrupt process termination can leave partial files.
The saved model preserves drawing/mesh definitions and authoring metadata;
transient generation-state records are cleared so old accepted-state snapshots
are not misrepresented as the new build's managed revision.

SVGs are isometric hidden-line previews using 32 samples per edge, bounded to
one million vertices per selected output. They support visual review; exact
CAD and requirement measurements come from the BREP. This is a one-shot CLI. The [MCP adapter](MCP.md) exposes generated schemas,
examples, builds and artifact resources to AI clients. Interactive shared
sessions, concurrency revision checks, automatic design planning and
stress-analysis certification remain outside this tool.
Existing drawing definitions can be exported with `occt-drawing-export` or
MCP `occt_export_drawings`. MCP also exposes measurement inspection, motion,
balance, linkage branch search and semantic document merge; see the
[command workflow](MCP.md#drawing-inspection-and-mechanism-workflow).

## Checks and scale

The command tests cover bracket creation, spacing edits, failed-wall rejection,
repair, STEP re-import, STL/SVG generation, source/accepted-output preservation,
schema migration, and malformed requests/edits/selections. The example and
focused scale checker builds the four original examples and verifies a 1,000-instance report
with one shared variant within a 10-second budget:

```sh
OCCT_BRIDGE_LIB_DIR="$PWD/build/bench" LD_LIBRARY_PATH="$PWD/build/bench" \
  cargo build --release --manifest-path rust/occt-parametric/Cargo.toml --bin occt-model
LD_LIBRARY_PATH="$PWD/build/bench" \
  python3 tools/model/check.py rust/occt-parametric/target/release/occt-model
```

[The MCP adapter](MCP.md) now exposes this contract as AI tools with generated
feature/parameter schemas and example/artifact retrieval.

Read-only [model inspection](MCP.md#inspect-existing-parts-before-editing) now
returns paged parameters, feature inputs, requirements and reference geometry,
plus optional native face/edge measurements and semantic selector queries.
Use `occt-model --inspect REQUEST.json NEW_REPORT.json` directly or the MCP
`occt_inspect_model`/`occt_inspect_build` tools. Geometry queries use family-local
feature-authoring coordinates; assembly placement is reported separately.
Use `instance` plus `loft_feature` for isolated exact profile/correspondence
diagnosis, including a failing loft. This reports paged source/aligned edge
counts and alignment issues without constructing or certifying the solid.
For hollow lofts, paged station records also validate native planar material
regions and report remaining area, coplanarity and hole-clearance errors.

[Guarded model edits](MCP.md#guarded-edits-and-revision-history) add/replace/remove
features by family and stable ID, add new parameters/requirements/references,
and record verified changes in the saved revision ledger. Use
`occt-model --edit REQUEST.json NEW_BUILD_DIRECTORY` or `occt_edit_build` with
the fingerprint of an accepted snapshot. Existing requirements and source
builds are preserved; stale guards and rejected/no-op edits publish no new build.

[Annotated sketch and 3D previews](VIEWER.md) now add `viewer.html`, `view.json`
and annotated SVG snapshots whenever previews are enabled. Labels link to
parameters/requirements, sketch symbols show actual solver results, and failed
checks are visible. Use `--visualize` / `occt_visualize_model` for diagnostic
proposals that cannot pass normal build verification.

Sketch authoring now supports [guarded editing and repair](MCP.md#sketch-edit-and-repair-workflow):
inspect `sketch_feature`, propose typed `edit_sketch` operations, preview with
`occt_preview_edit_build`, and publish through `occt_edit_build` after review.
Use `--preview-edit REQUEST.json NEW_DIRECTORY` for the same diagnostic preview
without MCP. The `sketch-repair` example contains a duplicated driving angle.

`constrained-loft.request.json` demonstrates exact profile-loft start/end tangent
directions and parameter-linked strengths (schema 169); see [constrained lofts](../../docs/LOFTS.md).

`normal-profile-loft.request.json` uses profile-derived end normals and
parameter-linked strengths (schema 170, ABI 77 unchanged).

`aligned-profile-loft.request.json` uses schema 171 / ABI 78 to align both circular
seams with parameter-driven `profile_loft.correspondence` anchors. The map is keyed
by source profile ID and supports optional `start_point` length vectors and
`reverse` flags. Manual correspondence requires a smooth end-constrained loft;
unknown IDs or anchors off regenerated boundaries are errors. See
[the supported anchor geometry](../../docs/LOFTS.md#profile-correspondence-schema-171-abi-78).

`partitioned-profile-loft.request.json` (schema 172 / ABI 79) uses three exact
`split_points` on a copied circle boundary to match a four-edge diamond. Both
profile-normal end constraints remain enforced, and original source-edge
ancestry reaches every generated wall. Split vectors support parameters and
participate in incremental invalidation. All executable examples use schema 172.
MCP exposes `partitioned-profile-loft`, `constrained-loft`, `normal-profile-loft`
and `aligned-profile-loft` through `occt_get_example` and example resources.

## Current AI authoring catalog

MCP discovers every local `*.request.json` automatically and exposes the
generated `occt://catalog` resource with the running binary’s model schema,
feature operations and capability counts. See [MCP discovery](MCP.md). Wire
tests validate all examples against their generated build, view or command schemas.

Manufacturing build requests include [hemmed tray](hemmed-tray.request.json)
(folded and flat outputs, closed hem, side walls and a bend-crossing slot) and
[tapped shell](tapped-shell.request.json) (open-top shell and a cosmetic M6
thread declaration on its bottom-wall bore). Both export STEP and interactive
viewers. Dimensions, bend allowances and thread engagement are example
assumptions; see [the workflow](MCP.md#manufacturing-examples).
