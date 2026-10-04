# Generated drawings

Schema 42 adds `ModelDocument::drawings`, a list of `DrawingDefinition` values.
Earlier documents migrate with an empty list. Drawing, view, dimension, and
note declarations use stable IDs in semantic comparisons and three-way merges.
`ModelDocument::from_graph` starts a new document with no drawings; retain or
edit the document's drawing declarations when updating its model graph.

Each drawing specifies its title, paper size in mm, views, dimensions, notes,
and title-block metadata. Generate it from a graph at its current poses:

```rust
let graph = document.instance_graph()?;
let drawing = document.drawings[0].generate(
    &graph,
    &session,
    DrawingRenderOptions::default(),
)?;
std::fs::write("part.svg", drawing.to_svg())?;
std::fs::write("part.dxf", drawing.to_dxf())?;
```

## Views

A `DrawingView` selects one named feature output per distinct unsuppressed
instance. Different views can select the same instance. Local parameter variants
regenerate once across an entire drawing batch. Instance placements, enclosing frames,
and joint values determine the geometry and datums shown.

The view origin is a length-valued world point. Its direction and x axis are
dimensionless nonzero perpendicular vectors. Direction points toward the viewer,
x points rightward in the image, and image y is direction cross x. Projection
produces local XY coordinates; `paper_origin_mm` places their origin on the
paper and the positive `scale` converts model mm to paper mm. Paper coordinates
increase upward; the SVG exporter converts them to SVG's downward y direction.

`DrawingViewKind::Orthographic` removes hidden lines from the full selected
geometry. `Section { origin, normal, keep_positive }` first intersects valid
solids with one side of an infinite cutting plane, retaining closed cut geometry,
then projects the retained solid. It supports arbitrary cutting-plane directions.
A cut that removes all geometry yields an empty view. Section views currently
show cut outlines without automatic section hatching.

Schema 57 includes `Slice`: the view origin and direction define a plane, and only
the boundary of the solid material on that plane is exported, including holes.
Unlike `Section`, it does not project geometry behind the cut. This is useful
for 1:1 cutting templates, gauges and cross-section inspection of any solid.
Root/tip boundary planes are supported; planes outside the solid are empty.
Loose topology and shell-only inputs are rejected.

An optional `DrawingDetail` crops projected polylines to a rectangular window
in view-local model mm. Its minimum maps to `paper_origin_mm`; the view's scale
controls enlargement. Disconnected clipped fragments remain separate paths.
Set `show_hidden` to include hidden edges. Both exporters draw hidden geometry
first and visible geometry afterward, preserving visible outlines where projected
edges coincide.

Visibility comes from OCCT's exact BREP hidden-line algorithm. It can retain
superimposed edges. Curves in SVG and DXF are **polyline approximations** sampled
uniformly along each edge's parameter range, rather than exact exported arcs or
splines. `curve_samples` controls resolution (2–100,000 points per edge);
it does not certify a chordal tolerance. The default is 64. See
[OCCT hidden-line removal](https://github.com/Open-Cascade-SAS/OCCT/blob/master/dox/user_guides/modeling_algos/modeling_algos.md).

## Annotations and exports

`DrawingDimension` uses two named datum origins projected into a chosen view.
Aligned, horizontal, and vertical dimensions display the projected model distance
in mm by default, independent of view scale. A signed paper-mm offset places the dimension
line and extension lines; arrowheads and a precision-controlled label regenerate
with the datum values. Zero projected extents and invalid datum references fail.

`DrawingNote` positions literal text or a parameter value on the paper. Parameter
notes support scalar, integer, boolean, and choice values. Length scalars normalize
to mm; dimensionless scalars retain their value. Prefix, suffix, and precision
control presentation. The title and ordered metadata fields populate a framed
title block. XML text is escaped and invalid XML control characters are removed;
DXF text controls become spaces, preventing line-based tag injection. Text fields
are limited to 2,049 UTF-8 bytes for DXF compatibility.

SVG is standalone, with physical mm dimensions and a matching viewBox. DXF uses
R2007/UTF-8, mm insertion units, LWPOLYLINE geometry, and separate `VISIBLE`,
`HIDDEN`, and `ANNOTATIONS` layers. Its hidden layer defines a dashed linetype.
The example exports have been parsed as XML and read by ezdxf with zero audit
errors or repairs. DXF structure follows Autodesk's
[polyline reference](https://help.autodesk.com/cloudhelp/2018/ENU/AutoCAD-DXF/files/GUID-748FC305-F3F2-4F74-825A-61F04D757A50.htm)
and [value-type reference](https://help.autodesk.com/cloudhelp/2019/ENU/AutoCAD-DXF/files/GUID-2553CF98-44F6-4828-82DD-FE3BC7448113.htm).

## Resource bounds and verification

`DrawingDefinition::generate_many` accepts 1–10,000 drawings with distinct IDs,
validates them before kernel work, and regenerates their participating instances
once. It returns drawings in input order. `maximum_vertices` is shared across
the entire batch, including every page and annotation.

`maximum_vertices` defaults to 1,000,000. The budget includes dimension arrows,
page/title-block lines, and sampled edges. Detail views conservatively reserve
twice the samples per edge because clipping may split every segment. Exceeding
the budget fails with all temporary geometry released. Drawing generation leaves
the input graph and accepted generations unchanged on success and failure.

Validation caches instance resolutions and family feature IDs. Generation costs
local variant construction plus participating frame paths, annotation resolution,
kernel edge/face projection work, and linear work in sampled/clipped vertices.
Exact HLR can require edge-by-face comparisons, with geometry-dependent
intersection costs. ABI 34's bulk subshape traversal indexes projected topology
once per pass, avoiding a topology walk for every exported edge. Storage is
generated geometry, projected topology, temporary edge handles, and exported
vertices. Projection privately copies geometry; plane clipping is non-destructive
and preserves operation history.

Tests cover visible/hidden XY projections, curved sampling, eight simultaneous
sessions, exact clipped volumes and history, section/detail views, annotation
edits, document migration and comparisons, argument errors, vertex limits, and
cleanup. Internal C++ conformance forces handle-space exhaustion between outputs
to verify that projection and bulk traversal retract partial results. Scale cases
cover 1,000 mixed views and a single view of 1,000 shared-geometry components.

The [drawing-export command](tools/drawing-export/README.md) writes safe numbered
SVG/DXF files, a manifest and a reloadable model with drawing definitions. Tests
cover true swept cross-sections, holes, boundary planes, oblique mounted parts,
batch budgets and publication errors. A 1,000-template benchmark verifies one
shared generated variant and handle cleanup. Native session creation initializes
OCCT’s shared plane once before concurrent geometry operations; a regression
test exercises first-use projection in 20 fresh processes with 16 threads each.

## Manufacturing dimensions and tolerances (schema 58)

Generated `DrawingLabel` values now carry an optional structured `stack` for
tolerance/limit layout; plain notes and titles use `None`.

`DrawingDimension::presentation` defaults to untoleranced millimeters, preserving
older JSON documents. Rust struct literals now supply
`presentation: DimensionPresentation::default()`. `DimensionDirection` is Clone
rather than Copy because angular dimensions hold a third datum reference.

- `Radius` and `Diameter`: `first` identifies the center and `second` a rim point.
  Nominal values are the center/rim distance and twice that distance, respectively.
  Diameter lines cross the center; radius leaders point to the rim.
- `Angular { vertex }`: `first` and `second` identify ray points from a third
  vertex datum. The minor angle is measured in the view plane, between 0 and
  180 degrees; coincident rays and zero-length rays fail. `offset_mm` is a
  strictly positive arc radius on paper. Arc geometry uses 64 segments and
  tangent arrowheads. Reflex angles are not represented.
- Radial/angular datum origins must lie in the view plane within numerical
  tolerance, preventing silent foreshortening. Existing linear dimensions
  continue measuring their projected distance. All values are independent of
  paper scale and detail-window translation.

`DimensionPresentation::length_unit` selects mm, cm, m or inches for labels;
geometry and paper coordinates remain millimeters. Angular labels use degrees.
`DimensionTolerance` selects:

| Variant | Meaning |
|---|---|
| `None` | Nominal dimension |
| `Symmetric { deviation }` | Nonnegative ± deviation |
| `Deviations { lower, upper }` | Signed lower ≤ 0 and upper ≥ 0 deviations |
| `Limits { lower, upper }` | Nonnegative absolute limits containing nominal |
| `Basic` | Nominal label enclosed in a rectangular box |
| `Reference` | Entire label enclosed in parentheses |

Length tolerances use `Quantity::length` in any supported length unit. Angular
values use `Quantity::scalar` in radians and are converted to degrees for display.
Incompatible units, nonfinite values and negative lower permissible dimensions
fail. Presentation precision is 0–12 digits after the decimal point; choose
sufficient precision to avoid rounding away a specified tolerance. Deviations
are displayed `nominal +upper/-lower`; limits are displayed `upper/lower`.
The generated label text keeps that compact notation for API consumers; SVG
and DXF render deviations and limits as separate stacked values alongside the
nominal/prefix and unit suffix. SVG includes the complete accessible label and
DXF preserves it as a comment alongside the separate TEXT entities. Basic/reference dimensions cannot
simultaneously carry direct tolerances in this enum.

```rust
let dimension = DrawingDimension {
    id: "bore-size".into(),
    view: "top".into(),
    first: DatumRef::new("part", "bore-center"),
    second: DatumRef::new("part", "bore-rim"),
    direction: DimensionDirection::Diameter,
    offset_mm: 8.0,
    precision: 3,
    presentation: DimensionPresentation {
        length_unit: LengthUnit::Millimeter,
        tolerance: DimensionTolerance::Deviations {
            lower: Quantity::length(0.0, LengthUnit::Millimeter),
            upper: Quantity::length(0.025, LengthUnit::Millimeter),
        },
        hole: Some(InstanceOutputRef {
            instance: "part".into(), output: "bore".into(),
        }),
    },
};
```

An optional `presentation.hole` references an unsuppressed instance and a `Hole`
feature, and is valid only for diameter dimensions. Its current feature parameters
supply the nominal bore diameter, blind depth or THRU indication, counterbore
size/depth, countersink size/included angle, and recorded thread designation,
nominal diameter, pitch and handedness. The datum pair locates the callout leader;
when a Hole is referenced its feature diameter supplies the label instead of the
leader's datum distance. Tolerance applies to the bore diameter; recess and thread
values remain untoleranced. Thread intent is identified separately from the bore
(e.g. `Ø4.000 mm THRU; THREAD M5x0.8 (...)`). Custom literal notes remain available.
Callouts regenerate on instance overrides and survive document reloads. They do
not infer thread classes, verify fits, or certify the caller's designation.

Annotation vertices, including angle arcs and basic boxes, count against the shared
export budget. Callout generation indexes features and resolves parameters once
per referenced instance, avoiding a feature scan for each annotation. With fixed
expression complexity, annotation work and storage are linear in dimension count
plus the indexed features/parameters and datum-frame resolution work. Benchmarks
exercise 10,000 mixed dimensions and 10,000 live hole callouts while checking
shared geometry and handle cleanup. These capabilities are not an ASME conformity
claim; GD&T and standards-verified drawing conventions remain on the roadmap.

## Center guides and cutting-plane indicators (schema 59)

`DrawingDefinition::guides` is an optional list of `DrawingGuide { id, view,
kind }` declarations. Older documents default to an empty list. Rust drawing
struct literals supply `guides: Vec::new()`; generated drawings now expose
`guides: Vec<DrawingGuideLine>` alongside geometry polylines. Guides have stable
IDs for comparisons and three-way merges, including independent guide edits.

| Kind | Inputs and behavior |
|---|---|
| `CenterMark` | A center datum and positive `half_length_mm`; draws two solid thin strokes along the view's paper X/Y axes |
| `Centerline` | Two datum origins and nonnegative `extension_mm`; projects the endpoints and extends the line at both ends |
| `CuttingPlane` | Two datum origins, a `section_view` ID and plain-text `label`; draws the cut trace, viewing arrows, endpoint labels and `SECTION label-label` beneath the linked view's paper origin |

Mark half-lengths and centerline extensions are **paper millimeters** and do not
change with drawing scale. Datum positions follow current instance parameters,
placements and parent frames. Detail-window offsets affect their paper position;
guides are intentional annotations and are not automatically clipped to a detail
window or selected geometry. Declare the datum locations and annotation sizes
appropriate to the view; the exporter does not infer hole centers or optimize
annotation placement.

```rust
page.guides.push(DrawingGuide {
    id: "bore-center".into(),
    view: "top".into(),
    kind: DrawingGuideKind::CenterMark {
        center: DatumRef::new("part", "bore-center"),
        half_length_mm: 3.0,
    },
});
page.guides.push(DrawingGuide {
    id: "cut-A".into(),
    view: "top".into(),
    kind: DrawingGuideKind::CuttingPlane {
        first: DatumRef::new("part", "cut-start"),
        second: DatumRef::new("part", "cut-end"),
        section_view: "section-A".into(),
        label: "A".into(),
    },
});
```

Cutting-plane indicators currently represent straight cuts. Both world datum
origins must lie on the linked `Section` view's plane within numerical tolerance;
that view must look normal to the plane, and the plane must be edge-on in the
source view. The projected endpoints must differ. The source and target must be
different views. Arrows show sight direction, opposite the target view's
`toward viewer` direction; reversing that direction reverses the arrows.
`keep_positive` continues controlling retained material independently. The guide
does not alter section geometry or supply automatic hatching. Its caption starts
8 paper mm below the section's `paper_origin_mm`; endpoint labels and arrows have
fixed paper offsets. Labels cannot be empty or contain control characters.

SVG centerlines/cut traces use a `6 1 1 1` long-short pattern. Center marks remain
solid so the datum intersection stays visible. Thin center guides use 0.18 mm
strokes; cutting traces and arrows use 0.50 mm. DXF adds CENTER and CUTTING_PLANE
layers, a CENTER linetype with the same pattern, per-entity solid overrides for
marks/arrows, and corresponding lineweights. These are explicit drawing styles,
not a standards-conformity claim. Standards-verified layout, projection symbols,
automatic hatching and exact curve export remain on the roadmap.

Each center mark reserves 4 vertices, centerline 2, and cutting-plane indicator
14 against the shared export budget. Guide validation/generation is linear in
annotation count plus datum-frame resolution, with indexed view lookup and
proportional output storage. The 10,000-guide benchmark covers all three types,
SVG/DXF export, shared native generation and handle cleanup.
