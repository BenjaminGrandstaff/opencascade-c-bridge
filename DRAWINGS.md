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
A cut that removes all geometry yields an empty view. Optional section hatching
is described below.

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
superimposed edges. By default SVG and DXF use **polyline approximations**
sampled uniformly along each edge's parameter range. `curve_samples` controls
resolution (2–100,000 points per edge); it does not certify a chordal tolerance.
The default is 64. See
[OCCT hidden-line removal](https://github.com/Open-Cascade-SAS/OCCT/blob/master/dox/user_guides/modeling_algos/modeling_algos.md).

### Exact drawing geometry (ABI 46)

Set `DrawingRenderOptions::exact_curves` to `true` (or `"exact_curves": true`
in the export setup's `options`) to preserve finite standard curves. Lines,
circles, ellipses and trimmed arcs retain native SVG paths and DXF `LINE`,
`CIRCLE`, `ARC` and `ELLIPSE` entities. Bézier/B-spline curves, parabolas and
hyperbolas convert to adjacent exact rational Bézier spans, exported as native
DXF `SPLINE` entities with clamped knots, degree, control points and weights.
Periodic and reversed edges retain their geometry and traversal direction.
Offset/other curve types are rejected in exact mode rather than silently sampled.

SVG emits exact polynomial line, quadratic and cubic Bézier commands. SVG's
[path syntax](https://www.w3.org/TR/SVG2/paths.html) cannot represent arbitrary
rational or higher-degree splines. For those spans, `curve_tolerance_mm`
(default 0.01 paper mm) controls adaptive positive-weight subdivision. Each
accepted control hull lies within that tolerance of its chord, including a
scale-aware floating-point margin. This is a numerical error bound, not formal
interval-arithmetic certification or certification of OCCT's projection/Boolean
accuracy. Exhausted vertex/work/depth budgets or unrepresentable precision fail
before returning a drawing. Subdivision charges at most two million work units
per visible/hidden edge set and has a depth limit of 48.

Exact-mode detail views intersect the **existing edges** with a rectangular
face in their current projection/slice plane. The crop's perimeter never becomes
model geometry. Trimming preserves curved boundaries and disconnected fragments;
paper scale and placement still map the crop minimum to `paper_origin_mm`.
The same option enables kernel-trimmed section hatching; see
[Section hatching](#section-hatching).

`GeneratedDrawing::curves` holds paper-space `DrawingCurve` records, separately
from default-mode `polylines`. Ellipse records use a center, major-axis vector,
minor radius and a counterclockwise parameter interval; a full turn is 2π.
Bézier records hold control points, positive weights and any SVG approximation
vertices. Hidden and visible styles apply to both representations together,
with visible geometry drawn last.

`Session::edge_analytic_curve` extracts exact located 3D lines/conics in O(1).
`Session::edge_bezier_spans` returns located rational spans following edge
orientation, using count/fill conversion on private geometry. Returned data is
bounded to at most 1,000,000 poles; the limit does not bound OCCT workspace.
Other/offset curves return no spans. Conversion and detail Boolean costs depend
on kernel topology/degree. Exported storage is O(E + P + V) for analytic edges,
Bézier poles and SVG vertices; subdivision costs O(d²) per node for degree d.

The shared vertex budget charges two entries per analytic line, four per conic,
and control-point plus SVG-vertex counts for Bézier spans, alongside existing
frame/annotation/hatch costs. Existing JSON setups default to `exact_curves: false`;
no model schema change is needed. The command manifest counts exact entities
as `curves` and considers both exact and sampled geometry when reporting emptiness.
`curve_samples` applies to default-mode polylines and sampled hatch boundaries;
exact standard curves, detail trimming and hatching are independent of sample count.

Tests compare native spans to analytic conics and B-spline samples, check periodic
edges, cubic/quadratic and rational SVG output, loop error bounds, exact cropped
arcs/splines, empty crops, far rotated placements, CLI manifests, budgets and
cleanup. Optimized benchmarks export 10,000 lines in 4.211 s (10 s budget),
1,000 spline views with 26,000 spans in 14.330 s, and 1,000 cropped spline views
with 8,000 spans in 15.364 s (30 s budgets), each sharing one generated variant
and releasing all kernel handles.

Entity definitions follow Autodesk's [ARC reference](https://help.autodesk.com/cloudhelp/2024/ENU/AutoCAD-DXF/files/GUID-0B14D8F1-0EBA-44BF-9108-57D8CE614BC8.htm),
[ELLIPSE reference](https://help.autodesk.com/cloudhelp/2023/ENU/AutoCAD-DXF/files/GUID-107CB04F-AD4D-4D2F-8EC9-AC90888063AB.htm),
and [SPLINE reference](https://help.autodesk.com/cloudhelp/2016/ENU/AutoCAD-DXF/files/GUID-E1F884F8-AA90-4864-A215-3182D47A9C74.htm).

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
not a standards-conformity claim. Standards-verified layout,
standards-verified material hatch presets remain on the roadmap; exact drawing geometry
is available as an export option.

Each center mark reserves 4 vertices, centerline 2, and cutting-plane indicator
14 against the shared export budget. Guide validation/generation is linear in
annotation count plus datum-frame resolution, with indexed view lookup and
proportional output storage. The 10,000-guide benchmark covers all three types,
SVG/DXF export, shared native generation and handle cleanup.

## Section hatching

Schema 60 adds optional `DrawingView.hatching: Option<SectionHatching>`. Existing
views load with `None`. Set `Some(SectionHatching::default())` for lines at 45°,
3 paper mm apart, with zero phase. Saved JSON can use `"hatching": {}` for these
defaults, or set `angle_radians`, `spacing_mm` and `phase_mm` explicitly. Spacing
and phase are perpendicular distances in paper millimeters, independent of scale.
Angles and phase must be finite; spacing must be finite and positive.

Hatching supports `Slice` and `Section`. A hatched section must look normal to
its cutting plane. The fill comes from that plane's actual material intersection,
even when the projection origin differs. Solids are intersected and clipped
individually so overlapping components remain present. In default sampled mode,
boundary parity preserves holes; interval union across faces preserves disconnected islands and
avoids double fill where components overlap. By default a view uses one shared pattern. Schema 69 adds explicit material
overrides, described below. Automatic adjacent-component alternation remains
future work. Detail windows clip the resulting lines alongside the outlines.

`GeneratedDrawing.hatches` contains separate two-point polylines. SVG draws them
behind outlines with a 0.13 mm stroke. DXF uses LWPOLYLINE entities on the
SECTION_HATCH layer with lineweight 13. In sampled mode, `curve_samples` controls
hole and curved-boundary resolution without certifying chordal error. Exact mode
uses kernel-trimmed boundaries as described below. Neither mode certifies ASME
conformance.

In sampled mode, for E segments and K scanline crossings, fill generation takes
O(E + K log K) time and O(E + K) temporary storage, in addition to one native
plane intersection per solid. A hard limit of 2,000,000 samples/crossings per view
rejects excessively dense patterns before emitting unbounded output. Line indices
must stay below 2^52 in magnitude. Hatch endpoints share `maximum_vertices` with
all other drawing geometry. The assembly benchmark checks 10,000 hatch segments
across 1,000 placed parts, exports and native handle cleanup within 30 seconds.

### Material hatch families (schema 69)

`DrawingView.material_hatching: BTreeMap<String, Vec<SectionHatching>>` maps
existing assembly material IDs to zero through eight line families. Each family
has its own paper-space angle, spacing and phase. Parallel families with offset
phases express paired-line patterns; different angles express crosshatching.
For example, inside a saved view:

```json
"material_hatching": {
  "steel": [
    {"angle_radians": 0.7853981633974483, "spacing_mm": 3, "phase_mm": 0},
    {"angle_radians": 0.7853981633974483, "spacing_mm": 3, "phase_mm": 0.6}
  ],
  "plastic": [
    {"angle_radians": 0, "spacing_mm": 3, "phase_mm": 0},
    {"angle_radians": 1.5707963267948966, "spacing_mm": 3, "phase_mm": 0}
  ]
}
```

These are illustrative explicit settings, not verified standard material
symbols. Material names do not select patterns automatically. Add a drawing
note or metadata field when a printed sheet needs a pattern legend.

The nearest material assignment along a clone's ancestry selects the override.
Unmatched or unassigned outputs use `view.hatching`; if that is `None`, they have
no hatch fill. An empty vector suppresses fill for the named material even when
there is a fallback. Outlines always remain. Maps apply only to Slice/Section
views and work with both sampled and exact rendering, including detail crops.
References to unknown materials, more than 10,000 mappings, more than eight
families per material and invalid family values fail validation. Older documents
load with an empty map, preserving their shared pattern.

Outputs are grouped by the selected override or fallback. Each solid is cut
once; its section faces are reused across line families. Interval union applies
within each material/family group. Different families/materials retain their
independent fills, including intentional overlaps. All groups and families in
one view share the existing two-million hatch work limit and the cumulative
export vertex limit. Identical supplied families may therefore emit duplicate
lines and consume their corresponding budget.

A material-ID index and memoized clone ancestry are shared across the drawing
batch. Cache construction/resolution is O(M + N) for materials and visited nodes;
per-view grouping is O(N log G), with G override/fallback groups. Temporary section
storage is O(N + G), beyond kernel topology and emitted hatches. Document and
batch validation also index material IDs once, avoiding a catalog scan for every
output or pattern reference.

Maps persist and participate in semantic comparison. Independent material-key
edits merge; competing edits to the same ordered family array conflict. Tests
cover inherited assignments, paired lines/crosshatching, fallback/suppression,
same-material overlap union, both render modes, section origins, scaled details,
invalid references, cumulative limits, migration, merges and cleanup. A benchmark
with 1,000 parts and 10,000 material mappings exports 12,500 hatch segments in
5.910 seconds (30-second budget), with one generated variant and no retained
kernel handles.

### Kernel-trimmed hatching

With `DrawingRenderOptions::exact_curves: true`, hatching trims straight scanlines
against each planar cut face using OCCT Boolean Common. Lines meet the actual
curve geometry, including spline boundaries and internal holes, rather than
sampled boundary chords. The result is exact to OCCT's intersection tolerances;
it is not a standards-conformity or formal arithmetic certification.

Per-face bounds restrict the candidate grid; detail windows restrict it further.
Batches contain at most 64 lines, bounding temporary input handles and avoiding
one Boolean over every scanline. The actual face plane and a centroid-relative
lift preserve section origins, paper scale, phase and far rotated placements.
Interval unions merge overlapping components and retain disconnected regions.
Nonzero ON-boundary line segments are included; isolated tangent points are not.
The conservative grid uses half-open bounding ranges, including the lower extent
and excluding an exactly aligned upper extent.

Preprocessing is O(F + E + L), and union/sorting is O(K log K), for F faces,
E boundary edges, L candidate lines and K returned intervals. Temporary returned
storage is O(F + K + 64), beyond source and kernel topology. Boolean costs depend
on boundary topology; the two-million work budget accounts for edge/line
candidates and returned intervals, not kernel workspace. Emitted endpoints use
the shared drawing vertex budget. Native spacing must be at least
max(1e-6 model mm, 256 × machine epsilon × face-anchor coordinate magnitude);
smaller spacing fails explicitly instead of merging indistinguishable lines.
Existing setups retain the sampled algorithm by default. Kernel-trimmed hatching
uses ABI 46; schema 69 adds the optional material maps described above.

Tests compare circular-hole and quadratic-spline endpoints to analytic curves,
verify sample-count independence, section origins, details, tangencies, multiple
batches, overlaps, islands, far placements, work/vertex limits and cleanup.
The optimized benchmark generates 10,000 exact hatch segments across 1,000 parts
in 8.773 s, and 40,000 circular-boundary segments across 1,000 cylinders with
both exports in 7.754 s; both have 30 s budgets and one shared generated variant.

Kernel semantics follow [OCCT Boolean operations](https://sso.opencascade.com/doc/occt-6.8.0/overview/html/occt_user_guides__boolean_operations.html).

## Standard paper presets and projection symbols

Schema 61 adds `DrawingDefinition.sheet: Option<DrawingSheet>`. With `None`,
custom `paper_size_mm` and the original full-width footer still apply. A saved
sheet preset controls the actual output dimensions; use
`effective_paper_size_mm()` to obtain them before positioning views.

```json
"sheet": {
  "size": "ansi_b",
  "orientation": "landscape",
  "drawing_number": "BRACKET-001",
  "revision": "A",
  "sheet_number": 1,
  "sheet_count": 2,
  "projection": "third_angle"
}
```

Sizes are ANSI A–E (exact decimal-inch dimensions converted to mm) and ISO A0–A4.
Orientation defaults to landscape; portrait exchanges width and height. The
preset creates a 12.7 mm inset frame and a 180 mm wide title block at the lower
right. Its base height is 60 mm, plus 10 mm per metadata row. Title, drawing
number, revision, common view scale, size and `SHEET n OF m` use separate cells.
Differing view scales produce `SCALE: AS SHOWN`. Sheet numbering must be positive
and within the declared count; this does not require other sheets to be present
in the same document.

`projection` may be `first_angle`, `third_angle`, or omitted. The symbol consists
of a truncated cone and two concentric end-view circles: first-angle places the
circles beside the large end, third-angle beside the narrow end. Symbols retain
paper sizes when view scales change. Circles use 64 segments, matching the
existing polyline export approach. Selecting a convention labels the drawing;
it does not reposition views or verify a multiview arrangement.

Structured fields reject control characters and bounded cell capacities: drawing
number 36 characters, revision 7, title 116 (two 58-character lines), up to eight
metadata rows of 58 characters including `key: value`. These conservative limits
reserve space for common fonts; they do not certify font metrics for every glyph.
Generated furniture lives in `sheet_lines` and `sheet_labels`, separately from
model geometry. Sheet line vertices count against the shared export budget.
Generation and export require O(sheets + views + text) additional time and
proportional storage; furniture per sheet is bounded. A 1,000-sheet benchmark
checks both symbols, numbering, exports, shared regeneration and handle cleanup.

These presets cover standard paper sizes and practical frame/title-block layout.
They do not claim complete conformity with prescribed zones, revision/approval
blocks, lettering or sheet-format requirements. References:
[ASME Y14.1 scope](https://www.asme.org/codes-standards/find-codes-standards/drawing-sheet-size-and-format/2020),
[ISO 5457 sheet sizes and layout](https://www.iso.org/standard/29017.html),
[ISO 5456-2 projection methods](https://www.iso.org/obp/ui?_escaped_fragment_=iso%3Astd%3Aiso%3A5456%3A-2%3Aed-1%3Av1%3Aen),
and the projection-symbol examples in the
[government engineering-drawing training manual](https://bharatskills.gov.in/pdf/E_Books/CTS/35/English/ED/Engineering%20Drawing%20-%20Group%207%20%282022%29.pdf).

## Structured GD&T intent

Schema 62 adds stable-ID `datum_features` and `feature_control_frames` collections.
Both default to empty for legacy documents. Their object order is irrelevant for
semantic diffs and merges; the order of a frame's `datums` array is significant.

`DrawingDatumFeature` holds `id`, a unique one-to-three-letter uppercase `label`,
excluding I/O/Q, `feature_of_size`, and a `DrawingGdtAttachment`. The attachment names a `view`,
a selected `output: InstanceOutputRef`, an `anchor: DatumRef` on that instance,
and `offset_mm` in paper coordinates. The output must belong to the selected
view and be unsuppressed. Leaders follow the current world-space datum position;
labels and frames retain fixed paper sizes through scales and detail offsets.
Offsets must be finite and at least 8 mm long. Symbols use an open attachment
triangle, leader and boxed datum label.

`DrawingFeatureControlFrame` holds the same attachment plus:

- `characteristic`: straightness, flatness, circularity, cylindricity, profile_line,
  profile_surface, parallelism, perpendicularity, angularity, position,
  circular_runout or total_runout.
- `tolerance: Quantity`, `display_unit: LengthUnit`, and `precision` (0–8).
  Values must be positive finite lengths, fit the display cell, and remain
  positive after rounding. Exported values include their units.
- `zone`: `characteristic` (default) or `diameter`.
- `material`: `regardless` (default, no symbol), `maximum` (circled M), or
  `least` (circled L), plus an explicit `feature_of_size` declaration.
- Ordered `datums: Vec<DrawingDatumReference>`. Each reference names a
  `datum_feature` ID and `boundary`: `regardless`, `maximum`, or `least`.

For example, a position frame may state a 0.10 mm diameter zone at maximum
material condition with primary A, secondary B at maximum material boundary,
and tertiary C at least material boundary. References use feature IDs, preserving
identity when labels or leader positions change. Datum letters themselves must
remain unique within the drawing.

The initial supported subset forbids datum references on form controls and
requires one to three for orientation, position and runout; profile controls may
have zero to three. Position currently requires a diameter zone. Diameter zones
are supported for straightness, orientation and position on declared features of
size. Tolerance material modifiers require declared features of size and are
supported for straightness, flatness, orientation and position. Datum material
boundaries require the referenced datum feature to be declared a feature of size.
Missing/repeated references, duplicate IDs, invalid attachments and unsupported
combinations fail before generation. These checks describe the supported subset;
they do not claim to accept every valid ASME control or prove feature geometry.

Glyphs, diameter marks and modifier circles use explicit vector strokes in both
exporters, avoiding dependence on a GD&T symbol font. Generated geometry and text
are available separately as `gdt_lines` and `gdt_labels`; DXF puts the linework on
GD_T. Circle/arc glyphs use 32 segments. The exact line-vertex reservation is
checked against the shared budget before allocating annotation geometry. Work is
linear in annotation/text output plus selected-output lookup and datum-frame
resolution, with indexed datum-feature reference lookup and proportional storage.
The 10,000-frame benchmark includes three ordered datum references and all
material modifiers, both exports, shared regeneration and native handle cleanup.

Feature-of-size flags and geometric controls are manufacturing declarations.
Anchors locate leaders on selected solid outputs; they do not identify persistent
faces or establish datum simulators. Multi-level composite frames, common datums, targets,
projected zones, advanced modifiers, datum shift and semantic PMI exchange remain
on the roadmap; measured simulators, bonus tolerance and zone evaluation for a
supported subset are described in [Measured inspection](#measured-inspection-schema-68). References: [ASME Y14.5 scope and contents](https://www.asme.org/getmedia/da2ff89e-067b-4160-8e2e-53e6c7da1d3b/17707.pdf)
and [NIST datum-system model](https://nvlpubs.nist.gov/nistpubs/jres/104/4/html/j44mac.htm).

## Composite controls and named datum-reference frames

Schema 63 adds `DrawingDefinition.datum_reference_frames`, a stable-ID collection
of `DrawingDatumReferenceFrame { id, datums }`. A named frame has one to three
ordered references to datum-feature IDs. Names must be unique and references
must satisfy the same feature-of-size/boundary rules as inline controls.

A feature-control frame may set `datum_reference_frame: "ABC"` instead of its
inline `datums`. Setting both is rejected. Renaming/moving datum-feature labels
or editing the shared named frame updates all dependent controls. Named-frame
object order is irrelevant to semantic merges; its datum order is significant.

Optional `refinement: DrawingCompositeRefinement` adds a lower segment for
position, profile-line or profile-surface controls:

```json
"refinement": {
  "tolerance": {"value": 0.05, "dimension": "length", "unit": "millimeter"},
  "datums": [{"datum_feature": "primary", "boundary": "regardless"}]
}
```

The original tolerance/datum fields describe the upper pattern-locating segment.
The lower segment describes feature-relating refinement and inherits zone shape,
material condition, units and precision. It must be strictly tighter both in
normalized length and after display rounding. Its references may be empty or an
unchanged prefix of the upper references, including material boundaries. This
prefix restriction is the initial supported subset; other composite arrangements
are rejected. The refinement is not a second independent position tolerance:
its datum references express orientation of the refining pattern rather than
independently locating that pattern by translation. No numerical zone evaluator
or pattern-fit solver is implied by these annotations.

SVG and DXF show one shared 16 mm tall characteristic cell, two 8 mm rows and a
stepped right edge when the lower row is shorter. The same leader attaches the
entire control. Both rows and modifiers count in the exact vertex reservation;
large batches fail their shared budget before allocating output geometry.

`resolve_datum_reference_frame(id, graph)` returns a named frame with datum IDs,
labels, explicit primary/secondary/tertiary precedence, material boundaries and
current world-space nominal datum geometry. `resolve_datum_reference_frames(graph)`
validates once and resolves all frames using an indexed datum-feature lookup.
The returned `ResolvedDrawingDatumReferenceFrame::nominal_planar_321()` supports
three mutually orthogonal plane datums at RFS. It intersects those planes for the
origin, uses the primary normal for +Z, the orthogonalized secondary normal for
+X, and their cross product for +Y. Reversing a tertiary normal does not reverse
the right-handed axes. `DrawingDatumCoordinateFrame::coordinates_mm(point)`
transforms world points into that frame in millimeters.

The orthogonality bound is 1e-9 on unit-normal dot products. Intersection offsets
are evaluated relative to the primary plane origin to reduce cancellation far
from the world origin. Non-finite results, partial frames, point/axis datums,
nonorthogonal planes and material-boundary shift requests fail explicitly. This
is a nominal coordinate utility over model planes, not a measured 3-2-1 simulator,
fitted datum establishment or evidence of manufacturing conformity.

Named-reference indexing and batch resolution require O(frames + references)
additional work and proportional storage, plus existing document validation,
selected-output lookup and datum-frame resolution. Each nominal planar solve
uses constant time/storage. Composite generation adds one bounded row per frame.
Benchmarks exercise 10,000 composite controls with both exports and 10,000 named
frames with nominal coordinates. References:
[NIST composite-tolerance data model](https://tsapps.nist.gov/publication/get_pdf.cfm?pub_id=821122),
[NIST datum-system definitions](https://nvlpubs.nist.gov/nistpubs/jres/104/4/html/j44mac.htm)
and [ASME training scope on single-segment and composite controls](https://www.asme.org/learning-development/find-course/vcpd757-gd-t-comprehensive-fundamentals-%28virtual-classroom%29).


## Feature size limits and tolerance allowances

Schema 68 adds optional `DrawingFeatureControlFrame.size_limits`:
`DrawingSizeLimits { kind, lower, upper }`, with internal/external
`FeatureOfSizeKind` and dimensioned positive length limits. Limits must be ordered,
may use different units, and require `feature_of_size: true`. Legacy controls
retain `None`. Limits are calculation inputs; annotate the size separately with
an existing limit dimension or hole callout.

`control.tolerance_allowance(supplied_size)` returns millimetre values for the
supplied size, maximum/least material sizes, bonus, total tolerance and optional
position-composite refinement total. For internal features MMC is the lower size
and LMC the upper; external features reverse these. MMC/LMC bonus is the size
departure from that condition; RFS bonus is zero. Each composite row adds that
bonus to its own specified tolerance. Out-of-limit, non-length, nonfinite,
nonpositive and overflowing inputs fail, without clamping or display rounding.
For example, an internal feature limited to 10–12 mm with position Ø0.1 mm at
MMC and supplied size 10.5 mm returns 0.5 mm bonus and 0.6 mm total allowance.

The API supports declared straightness, flatness, orientation and position size
controls, with position-only composites. Validate the drawing separately for
attachment and datum-reference semantics. It does not derive a mating size,
fit an envelope, establish simulators, calculate datum shift or determine
conformity. Datum MMB/LMB modifiers never contribute feature bonus. SVG/DXF
continue to show the specified tolerance, not an allowance for a supplied size.

The size-departure model follows the concepts described in the
[NIST comparison of ANSI/ISO tolerancing and STEP Part 47](https://tsapps.nist.gov/publication/get_pdf.cfm?pub_id=821115),
which distinguishes increased geometric allowance from boundary requirements.
This arithmetic API does not verify those geometric boundaries or certify ASME
conformity. A benchmark evaluates 10,000 composite allowances without native
geometry handles.


## Fixed cylindrical position sample checks

`DrawingFeatureControlFrame.evaluate_position_samples(size, nominal_axis, samples)`
checks dimensioned points on a supplied feature axis against a fixed cylindrical
zone. `PositionToleranceAxis` has a length-valued origin and scalar direction;
its direction is normalized. Points and the nominal axis must already be in the
same established datum coordinate frame. The checker supports single-row diameter
position controls with three distinct RFS datum references and saved size limits.
MMC/LMC controlled-feature bonus comes from `tolerance_allowance`; datum material
boundaries and composites fail explicitly rather than ignoring their freedoms.

`PositionSampleEvaluation` reports allowance, sample count, first worst-sample
index, maximum perpendicular distance, required diameter (twice that distance),
diameter margin (allowance minus required diameter), and `samples_within_zone`.
The zone includes its boundary; comparisons use normalized values without a
rounding or acceptance epsilon. Empty samples, inconsistent units, nonfinite
geometry, zero direction, arithmetic overflow and out-of-limit sizes fail.
The axis is an infinite nominal line: displacement along it does not affect
radial position, and no finite-depth or projected-zone requirement is inferred.

`DrawingDefinition.evaluate_position_samples(control_id, graph, size, axis, samples)`
validates saved intent and resolves named datum references before checking. It
does not transform samples or establish the supplied frame from the model datums.
Neither entry point fits measured surfaces, derives median/mating axes, solves
datum simulators or shift, evaluates composite pattern freedoms, or proves
whole-feature conformity between samples. A straight segment between samples
inside a fixed cylinder is contained by convexity; unsampled curved/bent geometry
requires more information. Use an existing resolved nominal 3-2-1 frame only
when nominal model coordinates are the intended reference.

This implements the fixed cylindrical-zone geometry, a subset of the position
zones described in [NIST's assembly tolerance model](https://tsapps.nist.gov/publication/get_pdf.cfm?pub_id=822117).
It does not constitute a standards-conformity inspection engine. Work is linear
in sample count with constant extra storage; the benchmark checks 100,000 samples
without kernel handles.


## Dimensional measurement limits

`DimensionTolerance.evaluate_measurement(nominal, measured)` checks a supplied
nonnegative measurement against symmetric deviations, signed deviations or
explicit limits. Lengths accept mixed units and return millimetre quantities;
angular values use scalar radians and return radians. Inputs are not rounded,
wrapped or converted to absolute values. Limits must be finite, nonnegative
and contain nominal; signed deviations must bracket zero.

`DimensionMeasurementEvaluation` returns normalized nominal/measured values,
signed deviation, optional `DimensionMeasurementLimits` and a
`DimensionMeasurementDisposition`. Limits include lower/upper values and signed
margin to the nearer limit. Boundaries are included, with no acceptance epsilon:
inside is `WithinLimits`, outside is `BelowLowerLimit` or `AboveUpperLimit`.
A 10 mm nominal with ±0.125 mm allows 9.875–10.125 mm; a 10.25 mm supplied
measurement returns +0.25 mm deviation and −0.125 mm margin.

`None`, `Basic` and `Reference` return no limits and distinct dispositions
`NoSpecifiedTolerance`, `BasicDimension` and `ReferenceDimension`. They do not
silently become zero-tolerance or unrestricted acceptance checks. No title-block
or note tolerance is inferred. Basic dimensions supply geometric-control intent;
reference dimensions supply information. This distinction follows the concepts
in [NIST's dimension and tolerance model](https://tsapps.nist.gov/publication/get_pdf.cfm?pub_id=821122)
and [NIST's ANSI/ISO tolerancing comparison](https://tsapps.nist.gov/publication/get_pdf.cfm?pub_id=821115).

`DrawingDefinition.evaluate_dimension_measurements(graph, measurements)` accepts
`DrawingDimensionMeasurement { dimension, value }` and returns ordered
`DrawingDimensionMeasurementResult` entries. It validates once and resolves
current model nominal values, including projected horizontal/vertical/aligned
lengths, radius, diameter and the minor angular dimension. Live hole callouts
use the Hole feature's evaluated diameter, independent of the leader's datum span.
Repeated IDs are allowed for repeated measurements; each nominal is cached in
the batch. Paper scale, display unit and precision do not affect acceptance.
Unknown IDs, inconsistent units, invalid tolerances and nonfinite/negative inputs
fail the entire request without changing drawing or graph state. An empty batch
returns no results after validation. This checks supplied values only; it does
not acquire measurements, apply uncertainty/guard bands, verify threads or
establish geometric/form conformity. A benchmark checks
100,000 measurements across 10,000 saved dimensions with no native handles.


## Inspection report command

Run saved dimensional and fixed cylindrical position checks from JSON:

```sh
cargo run --manifest-path rust/occt-parametric/Cargo.toml --bin occt-inspection-report -- MODEL.json MEASUREMENTS.json NEW_REPORT.json
```

The model must contain the referenced drawings. A minimal dimensional request is:

```json
{
  "schema": "occb-inspection-setup-v1",
  "drawings": [
    {
      "drawing": "page",
      "dimensions": [
        {
          "dimension": "width",
          "value": {"value": 10.25, "dimension": "length", "unit": "millimeter"}
        }
      ]
    }
  ]
}
```

Each drawing group accepts optional `dimensions` and `positions` arrays and an
optional `points` object, and must contain at least one measurement. `points`
holds `datum_features` and `controls` lists of `{ "id", "points_mm" }` entries,
evaluated as described in [Measured inspection](#measured-inspection-schema-68);
the report lists each measured control's `result` with a `status` of
`evaluated` (with `detail` holding deviation, tolerance, bonus, size and
conformance) or `not_evaluated` (with the reason). Drawing groups must have unique saved IDs;
repeated dimension/control IDs inside a group are allowed. A position record has
`control`, length `size`, `nominal_axis { origin, direction }` and nonempty
`samples` of `VectorQuantity` values. Length vectors use dimensioned x/y/z
quantities; the nominal direction uses scalar quantities. Axis and samples must
already share the established reference coordinates described above. Position
batches resolve and index saved inline/named references once via
`DrawingDefinition.evaluate_position_measurements`.

The versioned `occb-inspection-report-v1` JSON includes the model schema version,
ordered per-drawing results and summary counts of dimensional values inside or
outside limits, dimensions without acceptance limits, and position samples inside
or outside their zones, and measured controls that conform, do not conform, or
were not evaluated. Length results use mm, angles use scalar radians; output
includes the full typed evaluation, margins, allowance and worst-sample index.
Basic/reference/untoleranced dimensions retain their distinct dispositions and
are counted as having no acceptance limits. No model/drawing edits, native shape
regeneration or implicit measurement-coordinate transform occurs.

Exit status is 0 when a report was written with no explicit violations, 2 when a
report was written with dimensional, position or measured-control violations, and 1 for invalid
input or an I/O error. A 0 status with dimensions lacking limits does not assert
acceptance of those dimensions. Invalid/unsupported requests publish no report;
existing files are refused, with exclusive output creation after evaluation.
The command does not certify whole-feature/ASME conformity or release a drawing.

The crate enables serde_json's
[`float_roundtrip` feature](https://docs.rs/crate/serde_json/latest/features#float_roundtrip);
a regression test verifies bit-exact JSON round trips for position margins.
Command tests cover mixed results, absent limits, invalid requests, protected
outputs and named frames. A scale benchmark checks 100,000 position measurements
across 10,000 controls with shared named datum references and no kernel handles.
## Measured inspection (schema 68)

`DrawingDefinition::evaluate_inspection(graph, record)` checks measured points
against the drawing's controls and returns an `InspectionReport`. The
`InspectionRecord` names the `drawing` and lists `MeasuredFeature { id,
points_mm }` entries: `datum_features` by datum-feature ID and `controls` by
feature-control-frame ID. Records are separate from the model document, so
measuring parts does not change design intent.

Points are in model coordinates at the current poses, as from a coordinate
measuring machine aligned to the part. Fits start from the nominal orientations,
so the remaining misalignment must be small. Nominal datum plane normals must
point out of the material.

Datum simulators follow the precedence order:

- **Primary plane:** the minimum-zone orientation, placed against the outermost
  measured points (constrained L∞).
- **Secondary plane:** held perpendicular to the primary, free to rotate about
  the primary normal.
- **Tertiary plane:** fixed perpendicular to both and placed against its high
  point.

The three simulators build the measured frame the same way
`nominal_planar_321` builds the nominal one, so a feature's measured coordinates
can be compared directly with its nominal (basic) coordinates. The report lists
the measured frame of each named three-plane datum reference frame.

Supported controls:

| Control | Measured as |
|---|---|
| Flatness | Minimum-zone width over all orientations |
| Parallelism, perpendicularity, angularity (planar zone) | Zone width at the basic orientation to the measured datums; with a single datum, rotation about its normal stays free |
| Position (diameter zone, axis normal to primary plane) | Twice the radial offset of the related actual mating envelope from true position, in the measured frame |

For position, the mating envelope is the largest inscribed cylinder for
internal features and the smallest circumscribed cylinder for external ones,
both perpendicular to the primary datum. Position evaluation requires the
control's `size_limits` (see
[Feature size limits](#feature-size-limits-and-tolerance-allowances)), because
their `kind` selects the envelope type. At maximum material
condition (MMC), the bonus is the mating size's departure from the MMC size. At
least material condition (LMC), the zone is located on the minimum-material
envelope axis, and the bonus is that envelope's departure from the LMC size. A
size outside its limits fails the control.

Each fit linearizes, solves a small linear program (a dual simplex with an
n × n basis), and repeats. Widths and radii are then recomputed exactly for the
final orientation or center, so a fit that stops early can only overstate a
deviation. Plane points must not be collinear, and circle points must surround
their center.

**Report results.** Each control has one of three results:
- `Evaluated`: deviation, tolerance, bonus, actual size and conformance.
- `NotMeasured`: the record has no points for the control.
- `NotEvaluated`: a stated reason, for unsupported cases such as composite
  frames, datum shift at a material boundary, profile, runout, and form
  controls other than flatness, or a datum feature that was not measured.

The call fails outright for unknown or repeated IDs, non-finite points, a
different drawing, or more than `MAX_INSPECTION_POINTS` (10 million) points.
`InspectionReport::conforms()` is true only when every control was evaluated
and conforms.

**Cost.** Simulators are fitted once per distinct datum precedence prefix and
shared across controls. Time is O(controls · datums) plus the fit costs, and each
fit is linear in its points per simplex pivot. The scale benchmark evaluates
10,000 MMC position controls of 72 points each, plus a 100,000-point flatness
surface, against one shared frame.

**Not yet covered.** Datum features of size, datum shift, pattern and composite
evaluation, measured datum targets, profile against nominal surfaces,
cylindricity, and runout. Measurement uncertainty is not modeled.

## Surface texture (schema 71)

`DrawingDefinition.surface_textures` holds stable-ID `DrawingSurfaceTexture`
requirements, drawn with the ASME Y14.36 symbol and attached like GD&T controls
(a `DrawingGdtAttachment` whose leader points at the anchor datum). Each holds:

- `unit`: `micrometer` or `microinch`, and `roughness`: a B46.1 parameter
  (`ra`, `rq`, `rz` or `rmax`) with a `maximum` and optional `minimum`. A
  single value is a maximum; with a minimum it is a range (`Rz 32-63 µin`).
- `cutoff_mm`: the roughness sampling length, one of B46.1's standard values
  0.08, 0.25, 0.8, 2.5 or 8 mm (`STANDARD_CUTOFFS_MM`).
- `waviness`: maximum height and spacing in millimeters.
- `lay`: `parallel` (=), `perpendicular` (⊥, drawn as strokes), `crossed` (X),
  `multidirectional` (M), `circular` (C), `radial` (R) or `particulate` (P).
- `material_removal`: `any` (basic symbol), `required` (bar closing the V) or
  `prohibited` (circle in the V).
- `method`: a production-method note such as `GRIND` (1–40 printable ASCII
  characters), and `all_around`: a circle at the symbol's corner.

The roughness value sits above the short leg, ending before the long leg. Any
method, sampling length, waviness, lay or all-around turns on the extension bar:
the method above it, `Lc 0.8  W 0.05-25` below it, and the lay symbol at its
end; the bar grows to fit its notes. Symbols are vector strokes plus text in
`gdt_lines`/`gdt_labels` (DXF layer `GD_T`) and count exactly in the vertex
budget. Text positions use a 2 mm per character estimate; the layout follows
Y14.36's arrangement but has not been checked against the standard's figures.
Validation rejects repeated IDs, nonpositive or nonfinite values, a minimum at
or above the maximum, a nonstandard cutoff, an invalid method and the usual
attachment errors. Like GD&T anchors, a texture's anchor locates the leader;
it does not select a persistent face.

### Checking measured roughness

An `InspectionRecord` may list `surface_textures: [{ "id", "values" }]`:
readings in the requirement's parameter and unit (for example several Ra
traces). The report's `surface_textures` evaluates each requirement by the
maximum rule: every reading at or below the maximum and at or above any
minimum, reporting the count, highest and lowest readings. Requirements
without readings are `not_measured`, and `InspectionReport::conforms()` then
fails. Unknown or repeated IDs, empty, negative or nonfinite readings fail the
record; readings count toward `MAX_INSPECTION_POINTS`. The
`occt-inspection-report` command accepts the same list inside a group's
`points` and counts textures with the controls. The 16% rule, filtering and
instrument settings are not modeled.

A scale benchmark generates one drawing with 10,000 texture symbols and exports
it as SVG and DXF.
