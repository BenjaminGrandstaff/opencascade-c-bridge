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
  "model": { "schema_version": 120, "...": "complete ModelDocument" },
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

Use the complete examples here for valid model structure: [bracket](bracket.request.json),
[enclosure](enclosure.request.json), [shaft](shaft.request.json), and
[mating shaft/sleeve](mating-parts.request.json). Their dimensions and wall
limits are demonstration assumptions, not manufacturing qualification.

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
Required verification failures abort regeneration; their context is in the
message. The current library discards those rejected results, so the command
cannot provide their complete measured verification table.

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
Existing drawing definitions can be exported with `occt-drawing-export`.

## Checks and scale

The command tests cover bracket creation, spacing edits, failed-wall rejection,
repair, STEP re-import, STL/SVG generation, source/accepted-output preservation,
schema migration, and malformed requests/edits/selections. The example and
scale checker builds all four examples and verifies a 1,000-instance report
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
