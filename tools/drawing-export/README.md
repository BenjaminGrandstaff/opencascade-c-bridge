# Drawing and cutting-template export

`occt-drawing-export` exports general orthographic, section and detail drawings,
and true planar slices for cutting profiles or inspection. It uses persisted
`DrawingDefinition` values; no additional rib feature is needed.

```sh
OCCT_BRIDGE_LIB_DIR="$PWD/build/bench" cargo build --release \
  --manifest-path rust/occt-parametric/Cargo.toml --bin occt-drawing-export
LD_LIBRARY_PATH="$PWD/build/bench" \
  rust/occt-parametric/target/release/occt-drawing-export \
  structure.model.json tools/drawing-export/wing-example.json new-templates
```

The setup schema is `occb-drawing-export-v1`, with `drawings` and optional
`options` (`curve_samples`, default 64; `maximum_vertices`, default 1,000,000).
An omitted or empty `drawings` list selects drawings already stored in the model.
See [drawing definitions](../../DRAWINGS.md) for view frames and annotations.
Definitions with an existing ID must match the stored definition exactly.

The command validates and generates everything before creating a new output
directory; existing destinations are rejected. It preserves the source model.
Disk write failures can leave a partial destination. Outputs are `0001.svg`,
`0001.dxf`, etc. `manifest.json` maps numbers to drawing IDs and records empty
views, polyline counts and shared variant counts. `drawings.model.json` retains
existing definitions and adds supplied definitions, ready to reload as schema 56.
All drawings share generation and a cumulative vertex budget.

`wing-example.json` selects `wing:right_body` and `wing:left_body` at the four
stations of `tools/wing-layout/example.json`. It exports eight profiles with
scale 1, physical millimeter SVG dimensions and millimeter DXF insertion units.
Update its plane origins if the source stations or placement change. The planes
remain world-space views of the current posed geometry, including existing cuts.
Print SVG at 100% without fit-to-page scaling. In DXF, cut only the `VISIBLE`
layer; page/title-block lines and labels are on `ANNOTATIONS`.

Slice views export filled-section boundaries, including internal holes. They
accept valid solids and produce empty geometry outside the solid. Separate
sampled edge polylines may need joining for a cutter workflow. Curves are sampled
uniformly in parameter space; sample count does not certify chordal error.
There is no automatic kerf compensation, toolpath generation or section hatching.
The example is illustrative wing geometry, not verified historical dimensions.
