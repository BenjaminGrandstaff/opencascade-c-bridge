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
`options` (`curve_samples`, default 64; `maximum_vertices`, default 1,000,000;
`exact_curves`, default false; `curve_tolerance_mm`, default 0.01 paper mm).
Setting `exact_curves` to true preserves finite standard curves and detail trims
as native DXF geometry, and intersects hatch lines with native cut faces. SVG uses exact conic/quadratic/cubic paths and bounded
subdivision for rational or higher-degree splines; `curve_tolerance_mm` controls
that approximation. Unsupported curve types or exhausted budgets fail.
An omitted or empty `drawings` list selects drawings already stored in the model.
See [drawing definitions](../../docs/DRAWINGS.md) for view frames and annotations.
Definitions with an existing ID must match the stored definition exactly.

The command validates and generates everything before creating a new output
directory; existing destinations are rejected. It preserves the source model.
Disk write failures can leave a partial destination. Outputs are `0001.svg`,
`0001.dxf`, etc. `manifest.json` maps numbers to drawing IDs and records empty
views, polyline and exact curve counts, and shared variant counts. `drawings.model.json` retains
existing definitions and adds supplied definitions, ready to reload as schema 92.
All drawings share generation and a cumulative vertex budget.
Views can use `material_hatching` maps for material-ID overrides, paired line
families, crosshatching or suppression. See [material hatch families](../../docs/DRAWINGS.md#material-hatch-families-schema-68) for defaults and validation.

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
Section hatching is optional in drawing definitions. There is no automatic kerf
compensation or toolpath generation.
The example is illustrative wing geometry, not verified historical dimensions.


## Dimension layout example

Export the reproducible linear and angular layout sheets:

```sh
LD_LIBRARY_PATH="$PWD/build" \
  rust/occt-parametric/target/release/occt-drawing-export \
  tools/drawing-export/dimension-layout.model.json \
  tools/drawing-export/dimension-layout-example.json /tmp/dimension-layout
```

The first sheet shows positive/negative offsets, basic/reference dimensions,
stacked deviations and limits. The second shows an angular tolerance stack.
Labels remain upright, with estimated paper-space extents shared by SVG and
DXF. This layout example uses illustrative dimensions and font advances;
see [placement limits](../../docs/DRAWINGS.md#upright-dimension-label-layout).
