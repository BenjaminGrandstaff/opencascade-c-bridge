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

The [drawing-export command](../tools/drawing-export/README.md) writes safe numbered
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
| `FitClass { class }` | ISO 286 tolerance class, `"H6"`–`"H13"` or `"h6"`–`"h13"` (schema 163) |

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

### Tolerance classes (schema 163)

`FitClass` gives a size its limits from an ISO 286 tolerance class:

```json
"tolerance": { "fit_class": { "class": "H7" } }
```

H classes are for holes: the lower limit is the nominal size and the upper
limit adds the standard tolerance IT of the grade. h classes are for shafts,
with the upper limit at nominal and the lower limit IT below. The label
shows the nominal, the class and both limits in the display unit, as
`Ø4.000 H7 (4.012/4.000) mm`. A live hole callout uses the Hole feature's
diameter as the nominal, so the limits follow edits to it. A class applies
only to a size, meaning a diameter or a horizontal, vertical or aligned
length. Radius and angular dimensions with one are rejected.

The grades IT6–IT13 for sizes over 0 and up to 500 mm are a frozen V1
table (`ToleranceClass::deviations_v1`). Its values come from the ISO 286-1
table as [Wikipedia's IT Grade article](https://en.wikipedia.org/wiki/IT_Grade)
reproduces it, checked on 2026-10-15. A test confirms each value lies
within 10% of the standard's generating formula `k · i`, with
`i = 0.45 ∛D + 0.001 D` µm at the geometric mean `D` of the size range,
except the first range, which is the standard's own exception. That check
catches transcription errors such as swapped digits. Published H7 limits of
+12 µm over 3–6 mm and +57 µm over 355–400 mm match.

Each size range includes its upper limit (6 mm is in 3–6 mm), within 10⁻⁹ mm
for unit roundoff. A size over 500 mm is an error, never an extrapolation.
Other fundamental deviations (such as G, F or g6) and grades outside 6–13
are not included. Like the hole catalogs, V1 values never change.

### Upright dimension label layout

Dimension labels use shared paper-space text layout for both SVG and DXF.
Linear labels centre their estimated text rectangle along the dimension span,
then place it on the side selected by the signed offset. Vertical and oblique
placement accounts for text width; stacked deviations/limits include both
rows and the unit suffix. A basic dimension's frame is included in the extent.
The estimated rectangle has 2 mm clearance from the dimension line, radial
leader extension, or angular arc's midpoint tangent, independent of view scale.
Text stays upright. Measurement values, datum references, arrowheads, and
extension-line geometry are unchanged.

The width model uses deterministic character advances, including wider `m`/`w`
unit letters; it does not measure the installed font. The renderer does not
automatically avoid other labels, model outlines, or sheet borders, or move
arrows outside short spans. Those remain layout work. This is a readability
improvement, not a standards-conformity claim. No persisted fields changed:
schema 91 / ABI 52 remain unchanged. Layout work/storage are O(total label
characters), with no pairwise annotation scans and no additional vertices.

The [linear/angular example](../tools/drawing-export/dimension-layout-example.json)
and its [source model](../tools/drawing-export/dimension-layout.model.json)
exercise both offset sides, stacked tolerances and limits, basic/reference
dimensions, and angular labels. See the
[export command](../tools/drawing-export/README.md#dimension-layout-example).

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
size/depth and any relief (`; RELIEF Ø6.800 mm x 60°`, schema 164), countersink size/included angle, and recorded thread designation,
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
[Sheet conventions](#sheet-conventions-schema-134) add referenced lettering,
zones and line weights. Prescribed revision/approval blocks and sheet formats
are still not checked for conformity. References:
[ASME Y14.1 scope](https://www.asme.org/codes-standards/find-codes-standards/drawing-sheet-size-and-format/2020),
[ISO 5457 sheet sizes and layout](https://www.iso.org/standard/29017.html),
[ISO 5456-2 projection methods](https://www.iso.org/obp/ui?_escaped_fragment_=iso%3Astd%3Aiso%3A5456%3A-2%3Aed-1%3Av1%3Aen),
and the projection-symbol examples in the
[government engineering-drawing training manual](https://bharatskills.gov.in/pdf/E_Books/CTS/35/English/ED/Engineering%20Drawing%20-%20Group%207%20%282022%29.pdf).

## Sheet conventions (schema 134)

Three optional `DrawingSheet` fields apply drafting conventions taken from
public references. Each is off by default, and documents saved before schema
134 load with all three off, so existing drawings export unchanged.

```json
"sheet": {
  "size": "ansi_d",
  "drawing_number": "BRK-200",
  "revision": "A",
  "sheet_number": 1,
  "sheet_count": 1,
  "lettering": "standard",
  "zones": {"grid": {"columns": 8, "rows": 4}},
  "line_weights": "y14_2m"
}
```

**Lettering (`"lettering": "standard"`).** Text heights follow its role.
Inch sheets use the minimum of each range in the
[DOD-STD-100D](https://ibiblio.org/apollo/Documents/assist.dla.mil/MIL_STD_100D.pdf)
lettering table (Figure 100-1), which applies ANSI Y14.2M. ISO sheets use the
ASME Y14.2-2014 minimums as summarized in
[public training material](https://www.isbe.net/CTEDocuments/TEE-600173.pdf).

| Role | ANSI A, B | ANSI C, D, E | ISO A0, A1 | ISO A2–A4 |
|---|---|---|---|---|
| Drawing number | 6.10 mm (.24 in) | 6.10 mm (.24 in) | 6 mm | 3 mm |
| Title | 4.83 mm (.19 in) | 6.10 mm (.24 in) | 6 mm | 3 mm |
| Revision in the title block | 3.56 mm (.14 in) | 4.06 mm (.16 in) | 6 mm | 3 mm |
| Scale, size, sheet and metadata entries | 4.83 mm (.19 in) | 6.10 mm (.24 in) | 3 mm | 3 mm |
| Dimensions, notes, GD&T and tables | 3.56 mm (.14 in) | 4.06 mm (.16 in) | 3 mm | 3 mm |
| SECTION/VIEW captions | 3.56 mm (.14 in) | 4.83 mm (.19 in) | 6 mm | 6 mm |
| Section letters at cutting planes | 4.83 mm (.19 in) | 6.10 mm (.24 in) | 6 mm | 6 mm |
| Zone labels | 6 mm | 6 mm | 6 mm | 6 mm |

Zone labels use the Y14.2-2014 6 mm minimum for zone letters on every sheet.
Stacked tolerance rows are 11/15 of the dimension height, matching the legacy
2.2/3 ratio. The title wraps at word boundaries to the measured 176 mm cell,
with lines spaced 1.5 times the letter height. The Y14.2 minimum is half the
height. Validation rejects a title needing more than two lines, a single word
wider than the cell, and a drawing number, revision or metadata entry wider
than its cell. Parts-list and revision-table cells are cut with an ellipsis to
their measured column width. Widths use the deterministic advance estimates of
[upright label layout](#upright-dimension-label-layout), not measured fonts.

**Zones (`"zones"`).** Grid references sit in a band between the drawing frame,
12.7 mm from the trimmed edge, and a border 4 mm from it. Columns are numbered
from 1 at the left and rows are lettered from A at the top, skipping I and O.
Labels repeat on opposite sides.

- `"iso5457"` (ISO sizes only) follows ISO 5457 as summarized by
  [CAD Setter Out](https://cadsetterout.com/drawing-standards/grid-reference-frame/).
  Fields are 50 mm, measured outward from the sheet's centring axes, with the
  outer fields taking the remainder. Field counts (long × short side) are 24 × 16
  for A0, 16 × 12 for A1, 12 × 8 for A2, 8 × 6 for A3 and 6 × 4 for A4. A4 is
  zoned on the top and right only. Thick centring marks run from the border to
  10 mm inside the frame.
- `{"grid": {"columns": c, "rows": r}}` divides any sheet into equal fields,
  with 1–40 columns and 1–24 rows. It uses the same labeling. ASME Y14.1 zone
  counts and ordering were not available from a public source, so ANSI sheets
  use explicit counts rather than a claimed Y14.1 default.

`GeneratedDrawing::zones` and `DrawingSheet::zone_grid()` return the field
boundaries. `DrawingZoneGrid::zone_at(point)` returns a reference such as `C4`
for a paper point inside the frame, in O(log n) time.

**Line weights (`"line_weights": "y14_2m"`).** Exports use the ANSI Y14.2M
widths reprinted in
[Machinery's Handbook](https://www.globalspec.com/reference/68262/203279/Standards-for-Drawings): 0.7 mm thick and 0.35 mm thin.
Visible outlines, the drawing frame, cutting-plane lines and centring marks are
thick. Hidden lines, dimension and extension lines, arrows, GD&T frames, hatch,
centre lines, title-block and table rules, and zone ticks are thin.
`DrawingPolyline::thin` records the class. SVG writes the width as
`stroke-width`; DXF writes group 370 (`70` or `35`) on each entity. The
handbook notes that these widths distinguish line classes and are not
acceptance limits.

**Not covered.** No public source gave ANSI section-lining symbol geometry for
individual materials, so material hatching stays the explicit
[material hatch families](#material-hatch-families-schema-69). Dimension-line
spacing from the part and between parallel dimensions, and extension-line gaps,
also had no public normative values; dimensions keep their saved offsets.
Title-block and revision-block layouts are not checked against ASME Y14.1.
These presets are standards-referenced, not certified conformant.

**Cost.** Lettering and line weights add O(1) work per label or entity. Zones
add O(columns + rows) lines and labels per sheet, and each lookup is
O(log columns + log rows). The templates benchmark generates and exports 1,000
sheets with all three presets, alternating ANSI D 8 × 4 grids and ISO A2 zones,
plus 100,000 zone lookups, in 1.72 s (10 s budget). The same sheets without
presets take 1.67 s. The
[sheet-conventions example](../tools/drawing-export/sheet-conventions-example.json)
renders an ANSI B sheet and an ISO A3 sheet with every preset.

## Structured GD&T intent

Schema 62 adds stable-ID `datum_features` and `feature_control_frames` collections.
Both default to empty for legacy documents. Their object order is irrelevant for
semantic diffs and merges; the order of a frame's `datums` array is significant.

`DrawingDatumFeature` holds `id`, a unique one-to-three-letter uppercase `label`,
excluding I/O/Q, `feature_of_size`, and a `DrawingGdtAttachment`. The attachment names a `view`,
a selected `output: InstanceOutputRef`, an `anchor: DatumRef` on that instance,
and `offset_mm` in paper coordinates, and optionally the persistent `faces`
it annotates (see [Semantic PMI](#semantic-pmi-on-persistent-faces-schema-144-abi-70)).
The output must belong to the selected view and be unsuppressed. Leaders follow the current world-space datum position;
labels and frames retain fixed paper sizes through scales and detail offsets.
Offsets must be finite and at least 8 mm long. Symbols use an open attachment
triangle, leader and boxed datum label.

### Datum targets (schema 166)

A planar datum feature may list `targets` (ASME Y14.5-2018 §7.24): the
points, lines or circular areas on its surface that alone establish its
datum, as when a rough casting rests on fixture pins.

```json
"targets": [
  {"index": 1, "offset_mm": [-12, -12],
   "shape": {"point": {"at": {"x": {"value": 10, "dimension": "length", "unit": "millimeter"}, "y": {"value": 10, "dimension": "length", "unit": "millimeter"}, "z": {"value": 0, "dimension": "length", "unit": "millimeter"}}}}},
  {"index": 2, "offset_mm": [10, -12],
   "shape": {"line": {"start": {"x": {"value": 40, "dimension": "length", "unit": "millimeter"}, "y": {"value": 5, "dimension": "length", "unit": "millimeter"}, "z": {"value": 0, "dimension": "length", "unit": "millimeter"}},
                      "end": {"x": {"value": 55, "dimension": "length", "unit": "millimeter"}, "y": {"value": 5, "dimension": "length", "unit": "millimeter"}, "z": {"value": 0, "dimension": "length", "unit": "millimeter"}}}}},
  {"index": 3, "offset_mm": [12, 12],
   "shape": {"area": {"center": {"x": {"value": 30, "dimension": "length", "unit": "millimeter"}, "y": {"value": 30, "dimension": "length", "unit": "millimeter"}, "z": {"value": 0, "dimension": "length", "unit": "millimeter"}},
                      "diameter": {"value": 6, "dimension": "length", "unit": "millimeter"}}}}
]
```

Each target is named by its feature's label and `index` (1 to 99, unique in
the feature): `A1`, `A2`. Locations are basic, in the anchor instance's
model coordinates, and must lie on the feature's nominal datum plane
(within 1e-6 of their distance from its origin); a line needs distinct ends
and an area a positive diameter. Up to 100 targets per feature. Features of
size and axis datums cannot have targets.

In the feature's view a point target is drawn as an ×, a line target as a
phantom line (five long-short-short cycles scaled to its length, so its
vertex count is fixed), and an area as its projected circle with five hatch
chords. Each has the divided-circle symbol (Ø10 mm) at `offset_mm` from the
target: its name below the line, an area's size (`Ø6`) above, and a plain
leader to the target. Targets count toward the drawing's GD&T vertex budget.
Semantic PMI export carries the datum feature on its faces but not its
targets.

For measured inspection, see [datum targets in a record](#datum-targets-in-a-record).

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
  Since schema 136, `common_with` lists up to two further datum feature IDs that
  form a common datum with it in the same compartment, drawn as `A-B`. Members
  must be nominally coplanar planes (same outward normal) or coaxial axes, and
  no feature may appear twice in a frame. The boundary applies to every member.

```json
{"datum_feature": "A", "common_with": ["B"]}
```

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
faces or establish datum simulators. Targets,
and semantic PMI exchange remain
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
prefix restriction is the supported subset; other composite arrangements are
rejected.

Since schema 141 a refinement may hold its own `refinement`, a further lower
segment, to any depth: Ø0.8 to A | B | C, then Ø0.25 to A | B, then Ø0.1 to
A. Each segment must be tighter than the one above it, and its references an
unchanged prefix of the upper segment's (so a lower segment may name more
datums than the one above it). The frame draws one row per segment under one
characteristic cell spanning them all, with a stepped right edge.

```json
"refinement": {
  "tolerance": {"value": 0.25, "dimension": "length", "unit": "millimeter"},
  "datums": [{"datum_feature": "A", "boundary": "regardless"},
             {"datum_feature": "B", "boundary": "regardless"}],
  "refinement": {
    "tolerance": {"value": 0.1, "dimension": "length", "unit": "millimeter"},
    "datums": [{"datum_feature": "A", "boundary": "regardless"}]
  }
}
``` The refinement is not a second independent position tolerance:
its datum references express orientation of the refining pattern rather than
independently locating that pattern by translation. Measured composite position
is evaluated since schema 138, and composite surface profile since schema 139;
see [Measured inspection](#measured-inspection-schema-68). The lower segment's
profile zone is always equally disposed.

Schema 138 adds `pattern: Option<String>` to feature-control frames. Controls
sharing a pattern ID act as one n× control over several features. They must
match except for their ID and attachment, and only position and profile
controls may form patterns. Each still draws its own frame.

Schema 139 adds `unequal: Option<Quantity>` to profile controls: the Ⓤ
unequally disposed zone, giving how much of the tolerance lies outside the
material (from zero, all inside, to the whole tolerance, all outside). It must
be a length within that range and draws as a circled U followed by the value in
the tolerance's display unit and precision. Without it the zone is equally
disposed about the true profile.

```json
"characteristic": "profile_surface",
"tolerance": {"value": 0.1, "dimension": "length", "unit": "millimeter"},
"unequal": {"value": 0.03, "dimension": "length", "unit": "millimeter"}
```

Schema 142 adds two modifiers:

- **Projected tolerance zone Ⓟ:** `projected: {height, from?}` on position
  of a cylindrical feature of size, for example a tapped or press-fit hole
  whose mating part extends beyond it. The zone runs `height` from the
  plane datum `from` (a datum of the attachment instance, the face the
  mating part seats on) along that plane's outward normal. Without `from`
  it starts at the primary datum plane. It draws as a circled P followed
  by the height.
- **Tangent plane Ⓣ:** `tangent_plane: true` on parallelism,
  perpendicularity or angularity of a plane. Only the plane contacting the
  surface's high points must lie in the zone, so form error below them does
  not count. It draws as a circled T.

```json
"characteristic": "position",
"projected": {"height": {"value": 20, "dimension": "length", "unit": "millimeter"},
              "from": "top"}
```

Schema 143 adds `modifiers`, a list drawn after the tolerance in the order
given, each at most once:

- `free_state` (Ⓕ): the tolerance applies with the part unrestrained. It is a
  declaration, and points are taken as measured.
- `statistical` (⟨ST⟩, a hexagon): a statistical tolerance, judged on process
  data. A single part's measurement is not evaluated against it, so the
  control is reported as not evaluated.
- `continuous_feature` (CF), on features of size only: interrupted surfaces
  form one feature, and their measured points are taken together as one
  measured feature.
- `dynamic_profile` (△), on profile controls only: the zone may grow or
  shrink uniformly, so it controls form and orientation but not size.
- `separate_requirement` (schema 167; SEP REQT, noted beneath the frame and
  taking no width in it), on position and profile controls with datums,
  not composite: the control, or its `pattern`, is a requirement of its
  own. In measured inspection it no longer shares a datum shift (or a turn
  about an unclocked datum axis) with other controls on the same datum
  references; members of one `pattern` still shift together. Profile
  controls are grouped only by `pattern` already, so for them it records
  intent.

Each datum reference may also set `translation: true` (▷, drawn after the
label and any material boundary modifier). The datum feature simulator is
then free to translate from its basic location to engage the feature. The
primary reference has no basic location and may not translate.

SVG and DXF show one shared characteristic cell 8 mm tall per segment, an 8 mm
row per segment and a stepped right edge where row widths differ. The same leader attaches the
entire control. Both rows and modifiers count in the exact vertex reservation;
large batches fail their shared budget before allocating output geometry.

`resolve_datum_reference_frame(id, graph)` returns a named frame with datum IDs,
labels, explicit primary/secondary/tertiary precedence, material boundaries and
current world-space nominal datum geometry. `resolve_datum_reference_frames(graph)`
validates once and resolves all frames using an indexed datum-feature lookup.
The returned `ResolvedDrawingDatumReferenceFrame::nominal_planar_321()` supports
three plane datums at RFS meeting at any basic angles. It intersects those
planes for the origin, uses the primary normal for +Z, the secondary normal's
part square to it for +X, and their cross product for +Y. Reversing a tertiary normal does not reverse
the right-handed axes. `DrawingDatumCoordinateFrame::coordinates_mm(point)`
transforms world points into that frame in millimeters.

The planes must meet in a point (the unit normals' triple product at least
1e-6) with the secondary not parallel to the primary. Intersection offsets
are evaluated relative to the primary plane origin to reduce cancellation far
from the world origin. Non-finite results, partial frames, point/axis datums,
planes that do not meet in a point and material-boundary shift requests fail
explicitly. This
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
position-composite refinement total (the second segment's; further segments add
the same bonus to their own tolerances). For internal features MMC is the lower size
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
nonnegative measurement against symmetric deviations, signed deviations,
explicit limits or a tolerance class's limits. Lengths accept mixed units and return millimetre quantities;
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
holds `datum_features` and `controls` lists of `{ "id", "points_mm" }` entries
(controls may add `nominals`), evaluated as described in
[Measured inspection](#measured-inspection-schema-68). When a drawing has
profile controls with a `surface`, the command starts a kernel session and
takes missing nominals from the regenerated part;
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
were not evaluated, plus how many evaluated controls are indeterminate. A
`points` object may carry `uncertainty` as in an inspection record. Length results use mm, angles use scalar radians; output
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
- **Secondary plane:** held at its basic angle to the primary, free to rotate
  about the primary normal.
- **Tertiary plane:** fixed at its basic angles to both and placed against its
  high point.

Basic angles need not be 90°. Each simulator starts from its nominal normal
carried by the rotation that takes the higher-precedence datums' nominal
directions onto their simulators': the smallest rotation for a primary alone,
the rotation of the frame they span for two. A bevel at 45° to A can be a
secondary, and a chamfer square to A at 45° to B a tertiary. A secondary
parallel to the primary, or a tertiary whose normal lies in the plane of the
primary's and secondary's (so it cannot place the origin along the line where
they meet), is reported. Orientation zones use the secondary normal's part
square to A. Datum shift at MMB/LMB moves the frame within A's plane, so it
is evaluated only for widths square to A; a width tilted to A at a material
boundary is reported. The same carried start orients a tertiary plane
clocking a plane-axis frame at an angle to A.
- **Axis datum feature of size (schema 135):** a datum feature with
  `feature_of_size: true`, an axis anchor and `size_kind` (`internal` or
  `external`) is simulated by the axis of its actual mating envelope. That is
  the largest inscribed cylinder of a hole or the smallest circumscribed
  cylinder of a shaft, regardless of material boundary. As the primary datum
  the axis is free to shift and tilt. After one primary plane it is held
  perpendicular to that plane, and the envelope is a circle in the plane. A
  primary axis may be followed by one plane perpendicular to it and one
  parallel to it, in either order (see **Primary datum axis frames**). Other
  precedences with axis datums are not evaluated.
- **Width datum feature of size:** a datum feature with
  `feature_of_size: true`, a plane anchor on the nominal center plane and a
  `size_kind` (or `size_limits`) is a slot (`internal`) or tab (`external`).
  Its measured points split into the two faces by side of the nominal center
  plane. The simulator is the center plane of the actual mating envelope: the
  widest pair of parallel planes between a slot's walls, or the narrowest pair
  enclosing a tab. The planes turn only as a plane datum in that precedence
  may: free as primary, about the primary normal as secondary, fixed as
  tertiary of three planes, and about A's normal when clocking a plane-axis
  frame. A slot through a bore therefore clocks an A | B | C frame. Plane
  datum features of size without a kind keep the contact-plane simulator. The
  report lists each width datum in a named frame under `datum_widths` (center
  plane point, normal, mating width).
- **Common width datum:** coplanar width members `A-B` (two slots or tabs,
  or one of each, on one nominal center plane) share one center plane, the
  simultaneous simulator. Each member starts at its own actual mating width;
  all then change by the same amount, slots narrowing and tabs widening,
  until one center plane fits every member, and that change is minimized
  (one linear program per linearization, turning only as the precedence
  allows). Two slots offset 0.03 mm either way place the common plane
  midway. The members' widths differ, so a common width is not listed in
  `datum_widths`. A common datum mixing a width and a plain surface, and
  datum shift of a common width, are reported as not evaluated. Two slots
  of 20,000 points each fit in about 20 ms.
- **Plane, axis, plane frames (schema 137):** a primary plane A, a
  secondary axis datum feature of size B normal to it, and an optional
  tertiary plane or hole/pin C form a frame. Its origin is where B's axis
  meets A, z runs along A's normal, and x is C's normal projected into A or,
  for a hole or pin, the direction from B's axis to C's. A tertiary plane
  turns only about A's normal; a tertiary hole or pin is simulated by its
  mating envelope held normal to A (the circle in A's plane). Either way C
  only clocks rotation about B. Without C, only features coaxial with B can
  be located, since rotation about B is free.
- **Common datum plane (schema 136):** coplanar members `A-B` share one
  simulator, the constrained-L∞ plane over the union of their points. It
  contacts the high points of both surfaces at once. It can take any plane
  precedence.
- **Common datum axis (schema 136):** two coaxial cylindrical datum features
  of size establish the axis of the Y14.5 simultaneous simulator: one
  cylinder per member about one shared axis, contracting about shafts or
  expanding inside holes at one rate. Each starts from its member's own
  envelope (the minimum-material one at LMB, so they run the other way), and
  the axis minimizes the largest growth or shrinkage any member needs.
  Journals that are each straight but not in line therefore pull the axis to
  the minimax line between them. With one journal twice as long as the
  other, that line differs from the one through their midpoints. Among
  equally small simulators (point sampling leaves a flat set), the axis
  nearest the line through the members' own envelope-axis midpoints is
  taken. It is evaluated as the primary datum only, and the report lists
  both mating sizes in `datum_axes[].mating_sizes_mm`. At MMB
  or LMB each member uses its own envelope (the minimum-material one at LMB)
  and clearance, and the axis may shift and tilt as described under
  **Position to a primary datum axis**.

```json
{"id": "D", "label": "D", "feature_of_size": true, "size_kind": "internal",
 "attachment": {"view": "top", "output": {"instance": "part", "output": "body"},
                "anchor": {"instance": "part", "datum": "bore"}, "offset_mm": [20, 30]}}
```

The three simulators build the measured frame the same way
`nominal_planar_321` builds the nominal one, so a feature's measured coordinates
can be compared directly with its nominal (basic) coordinates. The report lists
the measured frame of each named three-plane datum reference frame. In
`datum_axes`, it lists each named frame that ends in an axis datum, with the
axis origin, direction and mating size.

Supported controls:

| Control | Measured as |
|---|---|
| Straightness of surface elements | Worst line element's width between the narrowest pair of parallel lines, oriented freely in its section (exact, by rotating calipers on the convex hull). A planar control's elements are sections normal to its attachment view, which must show the surface edge-on; a cylinder's (axis anchor) are its generator lines, grouped by angle about the nominal axis and split at gaps wider than a quarter of the widest, each projected onto its plane through the axis. Each element needs at least two points |
| Straightness of a derived median line | Ø zone on a cylindrical feature of size: diameter of the thinnest cylinder, axis free to shift and tilt, holding the centers of the cross sections normal to the nominal axis; each section's center and actual local size come from its actual mating circle (at least four points, at least two sections). At MMC or LMC the bonus is the departure of the section nearest that condition; every local size must lie within `size_limits`, which MMC and LMC require. Rule #1 does not apply, so the feature may exceed its maximum-material envelope |
| Flatness | Minimum-zone width over all orientations |
| Circularity | Worst section's radial width between the narrowest concentric circles, center free. Sections are points sharing a position along the anchor's nominal axis (within 1e-3 mm or 1e-6 of the axial span); each needs at least four points |
| Cylindricity | Radial width between the narrowest coaxial cylinders, axis free to shift and tilt from the anchor's nominal axis; at least six points |
| Parallelism, perpendicularity, angularity (planar zone) | Zone width at the basic orientation to the measured datums; with a single datum, rotation about its normal stays free. With Ⓣ (schema 142), the points are first projected onto their tangent plane: the minimum-zone orientation placed against the outermost points, like a primary datum simulator |
| Position (diameter zone, axis normal to primary plane) | Twice the radial offset of the related actual mating envelope from true position, in the measured frame: three planes (3-2-1) or a plane-axis frame (schema 137), after any datum shift |
| Profile with a dynamic zone (schema 143, △) | The same fit with one more free motion, a uniform offset of the zone along every normal, so the zone width is the spread of the deviations. `profile.offset_mm` reports the offset, and deviations are measured from the offset zone |
| Position in a projected zone (schema 142, Ⓟ) | The axis of the unrelated actual mating envelope (free to tilt; the minimum-material envelope's at LMC), extended beyond the part: twice the larger offset from true position where it enters and leaves the zone. Datum shift and composite fits treat the two ends as members of the pattern. Frames on a primary datum axis are reported |
| Circular runout (schema 135) | Full indicator movement about the datum axis, for the worst circular element. On a surface coaxial with the datum axis, the reading is radial and elements are points sharing an axial position. On a face perpendicular to it, the reading is axial and elements are points sharing a radius. Elements group within 1e-3 mm or 1e-6 of the span; each needs at least three points |
| Total runout (schema 135) | The same readings over all points of the surface |
| Runout of a surface of revolution with nominals (schema 140) | Readings square to the nominal surface: each point's signed distance from its nominal point along the nominal normal, with the nominal geometry placed on the measured datums. Circles are points whose nominal points share an axial position and then a radius, so cones, spheres and faces all work |
| Composite position (schema 138) | Upper segment: position as above. Lower segment: the pattern's true positions translate, and rotate about A's normal unless a refinement datum after the primary is a plane, as one rigid group minimizing the largest excess over the refined zones; each feature's deviation from that fit |
| Profile of a surface (schema 139) | Twice the largest distance, along the nominal normals, of any point from the zone center, after the rigid motions the datums leave free are fitted; a pattern's surfaces move together |
| Composite profile of a surface (schema 139) | Upper segment as above. Lower segment: the same fit with every translation free and the refinement datums constraining orientation only |
| Profile of a line, single or composite | The same zone for each line element on its own: a section normal to the control's view direction, moving only within its plane (in-plane translations and rotation about the view direction that the datums leave free) |

For position, the mating envelope is the largest inscribed cylinder for
internal features and the smallest circumscribed cylinder for external ones,
both perpendicular to the primary datum. Circularity and cylindricity need
axis geometry on the control's anchor and are evaluated regardless of feature
size, with the characteristic zone. Position evaluation requires the
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
their center. Round-form zones linearize the radial distance the same way.

**Datum shift (schema 137).** A secondary axis datum feature of size
referenced at maximum or least material boundary (`"boundary": "maximum"` or
`"least"`) needs `size_limits` on its datum feature. The kind must match
`size_kind`; either may be given alone.

- **MMB:** the boundary is the maximum material size, the lower limit for a
  hole and the upper for a shaft. The measured actual mating envelope may float
  inside or around it.
- **LMB:** the boundary is the least material size. The minimum-material
  envelope (circumscribed for a hole, inscribed for a shaft) floats against it
  and also locates the axis.

The largest radial shift is half the envelope's departure from the boundary.
A feature beyond its boundary is reported.

**Boundaries enlarged by geometric tolerances.** When the datum feature of
size itself carries a geometric tolerance relative to the datums before it
in the referencing frame (ASME Y14.5-2018 §7.11.4), its boundary is moved
out of the material by that tolerance: a bore perpendicular to A within
Ø0.04 at MMC, referenced A | B(MMB), has its virtual condition Ø9.86 as
MMB instead of Ø9.9. The tolerance counts when its control is on a
declared feature of size anchored where the datum feature is, can take a
material condition (straightness, flatness, orientation, position), is not
statistical, and names, in order, exactly the higher-precedence datum
features (none for a primary datum, such as straightness of the derived
median line). At the matching condition or regardless of size it is the
virtual condition; at the opposite condition the worst-case boundary adds
the size tolerance. When several controls count, the largest applies. The
same holds for width datums (a slot's MMB narrows, its LMB widens) and the
members of a common datum axis (their own form tolerances).

The shift is a rigid motion within plane A: a translation of the frame
origin and, where a width datum allows it, a rotation about A's normal.
Under the default simultaneous requirement, every position control with
identical datum references (features, precedence and boundaries) shares one
shift; a control marked `separate_requirement` (SEP REQT) shifts alone, or
with the other members of its `pattern`. The shift minimizes the largest excess of any control's offset over
its allowed radius (half of tolerance plus bonus). An active set of at most a
few controls is solved as a small LP with 256-gon zones, within 0.008% of
optimal, relinearized in the rotation; ties resolve to the smallest shift.
Each control then reports its deviation exactly at that shift, with
`datum_shift_mm` (applied translation), `datum_shift_rotation_rad` (applied
rotation, signed about A's outward normal) and `datum_shift_limit_mm`. A
uniformly displaced pattern can shift back into tolerance; holes displaced in
opposite directions cannot.

**Width and rotational datum shift.** A slot or tab width datum feature of
size can also be referenced at MMB or LMB, with `size_limits` on its datum
feature. Its boundary is a pair of parallel planes at the boundary width
(MMB: a slot's lower limit or a tab's upper; LMB: the reverse), centered on
the datum's basic center plane and carried with the frame. Every measured
point of the slot or tab must stay on its own side of that pair. These are
linear constraints on the shift, so the LP holds the pair inside the
measured faces exactly rather than through a single departure value.

- **Rotational shift:** in an A | B | C frame of a primary plane, a bore B
  and a tertiary slot or tab C, C at a boundary lets the frame turn about B's
  axis. C's pair passes through B at its basic offset; how far the frame can
  turn depends on the clearance and on how far the slot lies from B. B at MMB
  or LMB adds its own translation, and C's pair moves with it.
  A tertiary hole or pin C at a boundary does the same: its boundary
  cylinder, at C's basic place from B, moves with the frame and must stay
  within half C's departure of C's measured mating axis (the
  minimum-material axis at LMB), as for a shifting secondary bore. This is
  exact for C of perfect form, and the frame can turn about as far as that
  clearance over C's distance from B.
- **Width shift:** in a frame of three planes, a secondary or tertiary width
  at a boundary lets the origin move along its normal. A secondary width at a
  boundary also lets the frame turn, so its tertiary must then be a width at
  a boundary too: a contacting plane, or a width regardless of size, would
  have to re-contact as the frame turns, and is reported.

**Primary datum axis frames.** A primary datum axis A (or common axis A-B)
fixes four degrees of freedom. Two plane datums may follow it, one of each
kind, in either order:

- **Stop:** a plane perpendicular to A (a face) is a contacting plane held
  square to A's simulator. It fixes translation along A, and the frame origin
  is where it meets the axis. Zones parallel to A do not depend on it; a
  feature at an angle to A needs it.
- **Clock:** a plane parallel to A (a flat, or a slot or tab width) turns only
  about A's simulator axis, as a contacting plane or the width's mating
  center plane, and its normal gives the frame's x axis. A hole or pin
  parallel to A clocks it too: its mating envelope is held parallel to A's
  simulator, and x runs from A's axis toward its axis. Without a clock the
  frame turns freely about A (see below).

The report lists a named frame B | A | C (axis, stop, clock) under
`datum_frames` with z along the axis.

**Position to a primary datum axis.** Position of a feature located to A is
measured from the related actual mating envelope's axis between the
feature's measured ends, or the minimum-material envelope's at LMC. The zone
is a cylinder about the true position axis in the frame: parallel to A's
simulator axis for a bore or a bolt circle, or at its basic angle for a
cross hole or an inclined hole. Both ends must lie in it, so the deviation
is twice the larger end offset from the zone axis, and a feature leaning
relative to its zone is out by its worse end. A feature at an angle to A
needs a stop to place it along A, and is reported without one.

Without a clock, rotation about A is not constrained: the frame turns freely
about A's simulator axis, and controls with the same datum references turn
together (the simultaneous requirement), even regardless of material
boundary. A pattern of cross holes or a bolt circle turned as a whole is at
true position; one hole turned against the others is not. The turn is
reported as `datum_shift_rotation_rad`, with `datum_shift_limit_mm` zero
when A is regardless of material boundary.

Referenced at MMB or LMB, A's simulator may translate and tilt: each end of
its axis, at the datum feature's measured extent, floats within half the
feature's departure from the boundary (the same clearance as a secondary
axis). This is exact for a datum feature of perfect form; form error does
not restrict the motion further. Tilting A carries the frame about its
origin, so a zone at an angle to A also rises or falls along A by the tilt
times its distance from the axis. For a common axis A-B the ends span both
members, and the axis must pass within each member's own clearance of that
member's axis at both ends of the member's measured length. A tight member
therefore holds the axis near it while a loose one lets it tilt away.
A clocking width at MMB or LMB lets the
frame turn about A while its boundary pair, at its basic offset from A and
moving with A's ends, stays clear of the measured faces. A clocking hole or
pin at MMB or LMB lets it turn as in a plane-axis frame, its boundary
cylinder moving with A's axis at the middle of its measured length. A clock
regardless of material boundary, or a contacting plane, fixes the turn. Controls with
the same datum references share one motion, chosen by the same active-set
min-max LP in the ends and the turn (relinearized in the turn). They report
the larger end displacement as `datum_shift_mm` and, as
`datum_shift_rotation_rad`, the axis tilt (end displacement difference over
the datum length) combined with the turn about A, signed by the turn
(positive counterclockwise about A's direction). Composite position to a
primary datum axis is reported.

**Translation modifier ▷ (schema 143).** A datum at MMB or LMB sits at its
basic location relative to the higher-precedence datums, and ▷ frees it:

- **Tertiary width after a fixed secondary**, in a three-plane frame: it no
  longer locates along its normal, which is then free.
- **Secondary hole or pin**, in a plane-axis frame: it no longer locates the
  frame. A tertiary face at RFS then locates it along the face's normal, at
  its measured departure from its basic place, and still clocks it.

Other uses at a material boundary are reported. At RFS the simulators already
engage the feature where it is (contacting planes, mating envelopes), so ▷
changes nothing there. `datum_shift_limit_mm` leaves out freed directions.

`datum_shift_limit_mm` is the largest origin translation the boundaries allow
with the frame square to its mating envelopes: the bore's radial limit plus
the combined half departures of locating widths. A width narrower than its
MMB (or wider than its LMB) violates the boundary and is reported.

```json
{"id": "B", "label": "B", "feature_of_size": true,
 "size_limits": {"kind": "internal",
                 "lower": {"value": 9.9, "dimension": "length", "unit": "millimeter"},
                 "upper": {"value": 10.1, "dimension": "length", "unit": "millimeter"}},
 "attachment": {"view": "top", "output": {"instance": "part", "output": "body"},
                "anchor": {"instance": "part", "datum": "bore"}, "offset_mm": [20, 30]}}
```

**Composite position (schema 138).** The upper, pattern-locating segment is
evaluated like any position control, including datum shift and the
simultaneous requirement shared with other controls on the same datums. The
lower, feature-relating segment fits each pattern (controls with the same
`pattern` ID, or a lone control) as one rigid group, each lower segment on its
own. It translates freely and
rotates about the primary normal unless the refinement references a plane
after its primary datum, such as `A, B` with B a face. The fit minimizes the
largest excess of any feature over its refined zone (half of refined tolerance
plus bonus). Like the datum-shift fit, it uses an active set with small
256-gon LPs, relinearized in the angle. `ControlMeasurement.refinement` reports
the refined deviation, tolerance, bonus and conformance with the pattern's
fitted translation and rotation (radians about the measured frame's z axis, the
primary datum's outward normal). The control conforms only when both segments
do. Because the fit is min-max, one feature out of relation spreads its error
across the pattern, and the pattern fails its relating zone as a whole. A lone
feature's refinement only re-checks orientation, which the planar measurement
already holds, so it reads zero.

**Profile of a surface (schema 139).** Each measured point of a profile
control may carry its nominal surface point and outward normal in
`MeasuredFeature.nominals`, as coordinate measuring software reports them:

```json
{"id": "bore_profile",
 "points_mm": [[35.02, 20.0, 1.0], ...],
 "nominals": [{"point_mm": [35.0, 20.0, 1.0], "normal": [-1.0, 0.0, 0.0]}, ...]}
```

There must be exactly one nominal per point, with finite values and a nonzero
normal (it is normalized). Runout controls accept them too (see below).
Nominals on datum features or other controls fail the record. A profile anchored on a plane datum may omit them: each point
is then compared with its projection onto that plane. Other anchors need them,
or a `surface` on the control.

**Profile against the model (schema 140, ABI 69).** A profile or runout
control may name the faces it covers with `surface`, a `FaceSelector` on its attachment output,
such as a family's named face reference. Only face references of that
instance's family may be named, and pattern members each name their own
faces:

```json
"characteristic": "profile_surface",
"surface": {"named": "bore_wall"}
```

`DrawingDefinitionInspection::project_profile_nominals(graph, session,
record)` fills the nominals of every measured profile or runout control that
names a surface and was recorded without them. Each attachment instance is
regenerated once at its own parameters, the selector picks its faces there,
and every measured point is moved into the instance's local coordinates
(undoing its placement and enclosing frames) and projected onto the nearest
of those faces with `occt_bridge_faces_project_points`. The nearest point and
the face's outward normal there are moved back into model coordinates as the
point's nominal. Nominals already in the record are kept.
`evaluate_inspection_on_model` projects and then evaluates. The projection
uses each face's surface, starting from the previous point's parameters, and
falls back to an exact point-to-face distance when the surface projection
lands outside the face's boundary; faces whose bounding boxes are farther than
the best candidate are skipped.
Deviations are signed along the nominal normal (positive out of the material),
measured from the nominal point, so they are exact for planes and first-order
for curved surfaces near their nominal points.

The nominal geometry is first placed on the measured datum simulators (planes,
width center planes or axes, regardless of material boundary). The first two
independent datum directions fix its orientation; a single one, such as A
alone, leaves rotation about that direction free. Planes fix translation along
their normals and axes fix translation across themselves. Profile without
datums controls form only, so all six motions are free. The free motions are
then fitted to minimize the largest distance of any point from the zone center
(a Chebyshev fit of at most six parameters), relinearized up to 16 times.
Motions that do not move the points along their normals, such as sliding a
plane within itself, are not fitted. Point sets larger than 512 use an active
set: a strided sample, then the worst violators, until every point is inside
the sample's zone.

Profile controls with the same `pattern` ID are one simultaneous requirement:
their surfaces move as one rigid group, so two bores spread apart cannot each
be fitted separately. A composite profile's lower segment refits the same
points with every translation free and only the refinement datums'
orientation fixed, with an equally disposed refined zone. The control
conforms only when both segments do.

**Profile of a line.** Line elements are sections normal to the direction
of the view the control is attached to, as Y14.5 reads the control in that
view: points sharing a position along the view direction (within 1e-3 mm or
1e-6 of the span) form one element, and each needs at least two points. Each
element is fitted alone with the same zone, its free motions narrowed to its
section plane: the free translations within it, and rotation about the view
direction when the datums leave that rotation free. A straight but displaced
row therefore fits back onto its line where a surface profile would keep the
bow between rows. Elements of different pattern members are not coupled. The
report's `elements` counts the fitted elements (1 for a surface profile), and
its translation and rotation are the largest of any element.

`ControlMeasurement.profile` reports the lowest and highest signed deviation,
the zone center (zero, or `unequal − tolerance / 2`), the fitted translation
and rotation angle, and the number of line elements. For a composite profile, `refinement` reports the lower
segment with the pattern's fitted translation and its unsigned rotation angle.
Each reported lower segment holds the next in its own `refinement`, and the
control conforms only if every segment does.
Profile measures size as well as form and location: a uniformly oversized
bore stays out of its zone even when it is free to move.

**Measurement uncertainty and decision rules.** A record's optional
`uncertainty` gives the expanded uncertainty U of each control's measured
deviation, as the measuring process states it (usually with coverage factor
k = 2), and the decision rule that applies it, following ISO 14253-1 and
ASME B89.7.3.1:

```json
"uncertainty": {"expanded_mm": 0.004, "rule": "guarded_acceptance",
                "controls": {"bore_profile": 0.006}}
```

- `simple_acceptance` (default): conforms when the deviation is within the
  allowed zone (tolerance plus bonus); U is reported but not applied.
- `guarded_acceptance`: conforms only when deviation + U is within the zone,
  proving conformance as a supplier must.
- `guarded_rejection`: fails only when deviation − U exceeds the zone, proving
  nonconformance as a customer must before rejecting.

`controls` overrides U by control ID. U must be finite and nonnegative, and
unknown IDs fail the record. Each evaluated control reports `uncertainty_mm`
and `indeterminate`, true when either segment's deviation lies within U of its
allowed zone, so the measurement alone proves neither outcome. Composite lower
segments are decided the same way. Size conformance and surface texture
readings are kept as measured. The decision is one O(1) pass per control.

**Simulated uncertainty.** `uncertainty.simulation` estimates each control's U
from its own measurement, by task-specific Monte Carlo simulation in the
manner of ISO 15530-4:

```json
"uncertainty": {"rule": "guarded_acceptance",
                "simulation": {"point_mm": 0.001, "trials": 32, "coverage": 2, "seed": 7}}
```

Each trial perturbs every measured point, datum and feature alike, by
independent normal errors of standard uncertainty `point_mm` in each
coordinate, and evaluates the whole record again. The simulated U is
`coverage` (default 2) times the standard deviation of the control's
deviation over the trials, the largest over a composite's segments. It
combines in quadrature with the supplied U (`expanded_mm`, now defaulting to
zero, or the control's override), which stands for the other contributions
such as temperature and machine geometry. So U follows the task: its datum
fits, point count, fit lever arms and the evaluation itself. A deterministic
SplitMix64/Box-Muller sequence from `seed` makes it repeatable. `trials` runs
from 2 to 1000 (default 32), and controls not evaluated in every trial get
no simulated part. The cost is `trials` further evaluations of the record.

**Report results.** Each control has one of three results:
- `Evaluated`: deviation, tolerance, bonus, actual size and conformance.
- `NotMeasured`: the record has no points for the control.
- `NotEvaluated`: a stated reason, for unsupported cases such as a line
  element with a single point, profile datums at a material boundary, a non-planar profile anchor
  without nominals, straightness of surface elements at a material
  condition or of a planar surface not seen edge-on, round
  form at a material condition or with a non-characteristic zone, a datum
  feature that was not measured, or an axis datum without a `size_kind`.
  Width datums report measurements missing a face, and common datums
  mixing a width with a plain surface.
  Datum shift also reports a boundary without `size_limits`, a feature beyond
  its boundary, shift of a common axis or of a common plane, and a
  tertiary plane or regardless-of-size width after a secondary width at a
  boundary. Orientation reports datums referenced at a material boundary.
  Runout reports any datum referenced at a material boundary.
  Runout also reports a frame without a datum axis, and a surface whose nominal
  axis or normal is not along the datum feature's nominal axis. Orientation and
  position controls report frames that contain an axis datum.

The call fails outright for unknown or repeated IDs, non-finite points, bad
nominals, a
different drawing, or more than `MAX_INSPECTION_POINTS` (10 million) points.
`InspectionReport::conforms()` is true only when every control was evaluated
and conforms.

**Cost.** Simulators are fitted once per distinct datum precedence prefix and
shared across controls. Time is O(controls · datums) plus the fit costs, and each
fit is linear in its points per simplex pivot. The scale benchmark evaluates
10,000 MMC position controls of 72 points each, plus a 100,000-point flatness
surface, against one shared frame, in 0.19 s, or in 0.22 s with datum B a
bevel at 45° to A, or in 0.28 s with every zone projected 10 mm (Ⓟ). With
uncertainty simulated over 32 trials, 1,000 controls and a 10,000-point
flatness take 1.7 s (3 s budget): about 53 ms a trial, as noisy points
take the fits about 2.5 times the pivots of exact ones; and 1,000 cylindricity plus 1,000
circularity controls (612,000 points) in 0.13 s. It also evaluates 1,000
circular plus 1,000 total runout controls (471,600 points), with each of 100
bore datum axes fitted once, in 0.03 s. Against 100 common datum axes from
200 bore journals (511,200 points), each a simultaneous simulator, the same
controls take 0.060 s. All six
cases have 1 s budgets. One derived-median-line straightness of 1,000
cross sections (36,000 points) takes about 10 ms (2 s budget in its test):
its zone is one linear program over a circumscribed 64-gon, recomputed
exactly for the axis found, so it can overstate the minimum by at most
0.13% and never understates it.
Runout readings cost O(n log n) per control after the datum fit. 10,000
position controls located A | B(MMB) | C from a center bore share one datum
shift in 0.17 s. The active set keeps each LP small whatever the
pattern size. With a slot through the bore at MMB as well (2.5 mrad of
rotational shift), the same 10,000 controls on a pattern turned 2 mrad and
moved 0.05 mm share one translation and rotation in 0.17 s. 10,000 position
controls on bands coaxial with a primary datum bore at MMB share one shift of
its axis in 0.23 s, and to a common axis of bores either side of them in
0.24 s. Located B(MMB) | A | C(MMB) instead, from the bore as
primary axis stopped by the face and clocked by the slot, the turned and
moved pattern shares one axis shift and turn in 0.24 s. Clocked instead by a
Ø6.06 dowel hole at MMB 25 mm from the bore (A | B(MMB) | C(MMB)), the same
pattern shares one translation and rotation in 0.18 s. All six cases have
0.5 s budgets. 10,000 radial cross holes square to a bore at MMB, located
A(MMB) | B with B the face that stops them and no clock, turned 1 mrad
together, share one turn of the frame in 0.71 s (1 s budget: each hole's
two envelope fits relinearize their turn). 10,000 composite position controls in
100 patterns of 100 evaluate in 0.19 s (1 s budget). Ten patterns with a
0.1 mm outlier fail their relating fit. With a Ø0.15 segment to A | B between
the two (three segments), they evaluate in 0.29 s (1 s budget). 10,000 composite surface profiles on
the same bores (720,000 points with nominals, 100 patterns of 7,200 points
fitted as rigid groups) take 0.18 s, or 0.19 s with dynamic upper zones (△)
(1 s budgets), and one 1,000,000-point
surface profile with all six motions free takes 0.30 s (2 s budget). 1,000
profiles recorded without nominals on bands of a cylinder (1,000,000 points)
regenerate the part once and project every point onto the named side face in
0.59 s (2 s budget). One line profile of 1,000,000 points in 1,000 elements
takes 0.20 s (1 s budget).

### Datum targets in a record

A record's `datum_targets` lists the points measured at each target, by
name (`A1`). A datum feature with targets is simulated from the union of
its targets' points, and only once all of them are measured; otherwise
its controls report "datum targets of 'A' were not all measured". Surface
points recorded under `datum_features` for such a feature are refused,
because the targets alone establish it. The simulators are the same
constrained-L∞ planes as for surfaces, so an area target contacts its high
points, but they need only one point more than their free rotations:
three for a primary, two for a secondary turning about the primary's
normal, one for a tertiary. So a 3-2-1 frame of six point targets works.
Uncertainty simulation perturbs target points like any others. Three Ø10
area targets of 1,000 points each evaluate in a few milliseconds.

**Not yet covered.** Datum shift of common planes; datum shift of widths
tilted to the primary; an axis datum at an angle to a primary plane;
composite position to a primary datum axis; a tertiary
hole or pin's form error limiting its rotational shift; a tertiary plane
re-contacting as a shifting secondary width turns; datum shift of a common
width; common axes of more than two members or
after a primary plane,
reporting n× patterns as one frame on drawings, datum targets on axes
or features of size,
and profile datums at a material boundary. Straightness has no
per-unit-length zone, and a derived median line's cross sections are
normal to the nominal axis rather than to the actual mating envelope's.
Simulated uncertainty covers isotropic, uncorrelated probing
error only. Correlated machine errors, form-dependent sampling error and
temperature are supplied as `expanded_mm`.

## Semantic PMI on persistent faces (schema 144, ABI 70)

A datum feature's or feature control frame's `attachment` may name `faces`:
a `FaceSelector` over the attachment output's family face references, such
as `{"named": "bore_wall"}`. These are the toleranced feature or datum
feature itself, as ASME Y14.41 attaches product definition to model geometry,
and they make the annotation semantic product manufacturing information. The
leader `anchor` still places it on the sheet. A profile or runout control
without a `surface` takes its measurement nominals from these faces.

`DrawingStepPmi::export_step_pmi` writes outputs as `export_step` does, but in
AP242 with the drawing's GD&T attached to those faces of each part:

- each datum feature, by label;
- each tolerance, with its characteristic, value, Ø zone, material condition,
  free state, statistical and tangent-plane modifiers, projected zone height,
  and its datum reference frame in compartment order, with common datums
  (A-B), MMB/LMB boundaries and translation modifiers;
- size limits, as a diameter over cylindrical faces or a width between two
  parallel planes (a datum feature's and its own control's identical limits
  become one size);
- n× pattern members on one output, as one tolerance over all their faces;
- each composite segment, as a tolerance of its own.

The file is then read back with OCCT's AP242 reader and every item matched by
kind and faces (part-local area and centroid). The returned
`PmiExchangeReport` lists, per item, what the format cannot represent
(continuous feature, dynamic profile, unequal zone, how a composite's
lower segment relates to the one above, and SEP REQT, which AP242 has but
the bridge's writer does not set yet) and what did not read back. It also
lists the annotations skipped and why: no faces, an output not exported, or a
datum reference that is skipped or lies on another output, and every surface
texture, which AP242 export through OCCT cannot represent. `verified()` holds
when nothing was lost, and `complete()` when nothing was skipped or left
unrepresented either. Faces that select nothing fail the export, naming the
annotation.

`occt-model` requests take `"step_pmi": "<drawing id>"` with `"step": true`.
The response's `pmi` carries `verified`, `complete` and the report.

Faces are selected on one local regeneration of each annotated instance,
where its selectors are defined, and carried to the placed output by face
index, since placement keeps topology. All selectors on an instance share one
selection context, so its face centers are measured once, and a target
shared by a tolerance and its size limits is resolved once. Datums are verified from the file's
own `DATUM` and `DATUM_FEATURE` entities, because OCCT's reader makes datum
labels only for datums that tolerances reference.

OCCT 7.9 limits:

- Its writer drops projected tolerance zones. It also writes every
  diameter zone with a runout zone definition whose angle is the zone
  modifier's value, which its reader turns into a runout modifier. Before
  writing, the bridge replaces each such definition: an Ⓟ control's
  becomes a projected zone definition with its height in the part's length
  unit, bounded by the toleranced feature, and the others are removed. A
  zone modifier read back on a control that has none counts as `zone`
  lost.
- Its writer shares one `DATUM` entity per label across the whole file, so a
  label may name only one datum in an export. Labels are already unique
  within a drawing.
- Its writer takes the unit of tolerance values, and the context of its
  presentation model, from the first size dimension it writes. Without one,
  tolerance values are written in metres (so they were reported lost before
  ABI 71), and a file with presentations fails to write. The bridge now sets
  both from the part's own context before writing.
- It writes no saved views; the bridge writes them itself (see
  [Annotation views](#annotation-views-schema-145-abi-71)).
- Its STEP translators share process-wide state, and concurrent writes from
  independent sessions failed spuriously. Every STEP read and write in the
  bridge now holds one process-wide lock. Without it, stress runs crash,
  drop assembly occurrences, and read a healed fixture as invalid; the
  `occt_bridge_step_concurrency` C test races fresh processes to keep it so
  (see the ROADMAP entry on intermittent STEP tests).

At scale, a 400-hole plate's datums, flat top and 400 position controls with
their sizes are written and verified in one export in about 1.9 s, as are the
same holes under one 400× control. Two regenerations of the plate (the
graph's and the local one) take 1.2 s of that, and the AP242 write and
read-back 0.5 s.

## Annotation views (schema 145, ABI 71)

ASME Y14.41 organizes a dataset's annotations into saved views: a named view
direction and the annotations it shows, drawn in 3D on the model.
`DrawingDefinition.annotation_views` holds them as `DrawingAnnotationView`:

- `id`, and `name`: the exchanged view name, 1–64 printable ASCII
  characters, unique within the drawing.
- `direction` (toward the viewer) and `x_axis` (image right), perpendicular,
  as for drawing views; image up is `direction × x_axis`.
- `scale` (default 1): the size of the annotations it presents. At 1 a frame
  is as large in model millimetres as on the sheet.
- `datum_features` and `feature_control_frames` it shows, by ID. Listing one
  member of an n× pattern shows the pattern.
- `clipping_planes` (schema 150, ABI 75): up to 16 cutting planes, each an
  `origin`, `normal` and `keep_positive` as for section drawing views. A
  section or clipped view keeps only the region on the kept side of every
  plane.

```json
{"id": "top", "name": "TOP GD&T",
 "direction": {"x": {"value": 0.0, "dimension": "scalar"},
               "y": {"value": 0.0, "dimension": "scalar"},
               "z": {"value": 1.0, "dimension": "scalar"}},
 "x_axis": {"x": {"value": 1.0, "dimension": "scalar"},
            "y": {"value": 0.0, "dimension": "scalar"},
            "z": {"value": 0.0, "dimension": "scalar"}},
 "datum_features": ["A", "B"], "feature_control_frames": ["flat", "bore"]}
```

Each annotation a view shows gets a 3D presentation in the plane of the
first view that shows it. The plane passes through the annotation's anchor
and faces that view's viewer. The leader, box, symbols and text use the
sheet layout, with the attachment's `offset_mm` measured along the view's
right and up and multiplied by its `scale`. Text is drawn in a built-in
single-stroke gothic letter set (capitals, digits and GD&T punctuation) at
the drawing's lettering height, so presentations do not depend on fonts. An
n× pattern draws its first member's frame with `nX` above it. A composite
frame's top segment carries the outline, characteristic, leader and top
row; each lower segment carries its own row. Size limits are not drawn on
sheets, so they have no presentation.

`export_step_pmi` writes each presentation as an AP242 tessellated
annotation, linked to its datum or tolerance and lying in its annotation
plane. It writes each view as a draughting model holding a parallel camera
and the annotations the view shows. The camera looks along the view toward
the centre of the export and its presentations, framing them. On read-back:

- each item's presentation must match polyline segment for segment, or it
  reports `presentation` lost;
- OCCT's reader must make a view of each name with the same direction and
  up, or the view's report gives `lost: view` or `lost: camera`;
- each view must show the annotations it lists, or they appear in its
  `lost_annotations`.

Listed annotations that were skipped appear in the view's `omitted`.
`verified()` now also needs every view read back intact, and `complete()`
needs nothing omitted.

A view with clipping planes is written as a multi-clipping camera
(`CAMERA_MODEL_D3_MULTI_CLIPPING`) whose planes intersect, each plane's
normal pointing into the side the view keeps. On read-back, OCCT must
recreate the same planes for the view, matched by normal and position, or
the view reports `lost: clipping`. Views have no per-view model visibility:
every view shows the whole export, clipped by its planes.

At scale, the 400-hole plate's three datums, flat top and 400 position
controls (or one 400× control) are drawn in a top view showing everything
and an isometric section view of the datums and flat top. The 400 drawn controls
write and verify in 2.5 s, of which the AP242 write and read-back take
1.8 s (0.5 s without presentations); the one 400× control takes 1.9 s.

## Dataset identification and authority (schema 151, ABI 76)

ASME Y14.41 asks a dataset to identify the product it defines, its
revision, and whether the model, the drawing or both govern. A drawing's
optional `dataset` states this:

- `identifier`: the part or dataset number, and `organization`: who is
  responsible for it and signs its approvals. Each is 1–64 printable ASCII
  characters.
- `authority`: `model` (the model and its annotations are the definition; a
  drawing is for reference), `model_and_drawing` (a reduced-content drawing
  completes the model), or `drawing` (the model is for reference).

```json
{"identifier": "PLT-1001", "organization": "Example Works", "authority": "model"}
```

The dataset's revision is the drawing's latest release (see
[Releases](#releases-and-revision-tables-schema-90)). One
revision rule is enforced: when the model governs (`model` or
`model_and_drawing`), every release must name the `model_revision` it
releases, so each dataset revision traces to the model content it covers.
`ModelDocument::drawing_release_status` then says whether the model has
changed since the latest release.

`export_step_pmi` writes the dataset on the export's root product:

- the identifier as the product's ID;
- the authority as its product definition's description, such as
  `authority: model`;
- when released, the revision as the product definition formation's ID,
  with the release description;
- each approval of that release as an approved AP242 `APPROVAL` with its
  date, approver (a person of the organization) and role, assigned to the
  formation.

OCCT's reader keeps none of this, so the read-back checks the file's own
entities. The report's `dataset` gives the identifier, authority and
revision, and lists in `lost` whichever of `identifier`, `revision`,
`authority` and `approvals` did not read back. `verified()` needs none
lost. The `occt-model` response's `pmi.release` adds the drawing's release
status against the document's revision ledger.

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
- `acceptance` (schema 155): `max_rule` (the default, not written) or
  `sixteen_percent`, how inspection accepts measured readings. It is not
  drawn in the symbol.

The roughness value sits above the short leg, ending before the long leg. Any
method, sampling length, waviness, lay or all-around turns on the extension bar:
the method above it, `Lc 0.8  W 0.05-25` below it, and the lay symbol at its
end; the bar grows to fit its notes. Symbols are vector strokes plus text in
`gdt_lines`/`gdt_labels` (DXF layer `GD_T`) and count exactly in the vertex
budget. Text positions use a 2 mm per character estimate; the layout follows
Y14.36's arrangement but has not been checked against the standard's figures.
Validation rejects repeated IDs, nonpositive or nonfinite values, a minimum at
or above the maximum, a nonstandard cutoff, an invalid method, attachment
`faces` naming anything but the output family's face references, and the
usual attachment errors.

### Textures on persistent faces

A texture's `attachment` may name `faces`, a `FaceSelector` over the output
family's face references (as GD&T attachments do since schema 144). They are
the surface the requirement governs. The anchor still places the symbol, at
`offset_mm` from the anchor's paper point. The leader, though, lands on the
faces, at the point nearest the anchor datum's origin, so it touches the
governed surface wherever the anchor lies.

`DrawingSurfaceTextureFaces::surface_texture_faces(graph, session)` returns,
in drawing order, each face-attached texture's output, governed face indices
(ascending, valid on the placed output since placement keeps face order),
their total area and the leader's landing point in model coordinates.
Selections that select nothing fail. So does a face governed by two textures
whose requirements differ: unit, roughness limits, cutoff, waviness, lay,
material removal, method or acceptance. A requirement repeated identically,
for example in a second view, is allowed.

Faces are selected on one local regeneration per textured instance. Each
distinct selector on an output is resolved once, and its textures' anchors are
projected onto it in one batch. Drawing generation calls this only when some
texture names faces. A scale benchmark draws 10,000 textures on a part's six
named faces in about 0.12 s against a 1 s budget.

Surface textures are not exchanged as AP242 PMI: OCCT 7.9's writer has no
representation for them. `export_step_pmi` lists every texture under
`skipped` with the reason `surface texture: no AP242 representation`, so
`complete()` is false for drawings that have textures.

### Checking measured roughness

An `InspectionRecord` may list `surface_textures: [{ "id", "values",
"profiles" }]`: readings in the requirement's parameter and unit (for
example several Ra traces), stylus profiles, or both. Each profile's
parameter is computed and becomes a reading after the given values, in
order. The report's `surface_textures` evaluates each requirement by its
acceptance rule, reporting the count, highest and lowest readings, how many
lie above the maximum and below the minimum, and any `profile_values` with
the `cutoff_mm` used. Requirements without readings are `not_measured`, and
`InspectionReport::conforms()` then fails. Unknown or repeated IDs, no
readings, negative or nonfinite readings, and profiles that cannot be
filtered fail the record. Readings and profile samples count toward
`MAX_INSPECTION_POINTS`, and profile samples also toward
`MAX_PROFILE_SAMPLES` (1,000,000). The `occt-inspection-report` command
accepts the same list inside a group's `points` and counts textures with
the controls.

**Acceptance rules.** By the maximum rule every reading must be at or below
the maximum and at or above any minimum. The 16% rule follows ISO 4288's
staged procedure as Mitutoyo's public quick guide states it, with readings
taken in order:

1. A first reading below 70% of the maximum accepts the upper limit.
2. Otherwise the first three readings accept it when none exceeds it.
3. Otherwise, with twelve readings, at most two may exceed it. With more,
   at most 16% may (rounded down, and never fewer than two).

A minimum is decided the same way without the first step: the first three
readings, then at most two of twelve below it. When the readings so far
decide neither way, the result reports `readings_required` (3 or 12) and
does not conform.

**Profiles.** A `MeasuredProfile` is a primary profile from a stylus
instrument: `heights_um` at equal `spacing_um`, after any short-wave (λs)
filtering, which is not modeled. The cutoff λc is the requirement's
`cutoff_mm`, or 0.8 mm when it names none. The spacing must divide the
cutoff into 20–2,000 samples (ISO 3274's standard spacings give 160–1,667),
and the profile must span six cutoffs: the evaluation length of five
sampling lengths and half a cutoff of run-up and run-out at each end, as
instruments trace them. Longer profiles are evaluated over their central
five cutoffs.

The mean line is the Gaussian filter of ASME B46.1 and ISO 16610-21, with
weights exp(−π(x/αλc)²), α = √(ln 2/π), truncated at ±λc/2 to fit the
run-up and normalized. The truncation passes 48.9% of a wave one cutoff
long into the roughness, rather than the untruncated filter's 50%, and
leaves a few parts in 10,000 of short waves in the mean line. On the
roughness profile, Ra and Rq are the mean absolute and root-mean-square
heights over the evaluation length. Each sampling length's peak-to-valley
height is also taken: Rz is their mean, and Rmax the largest. A microinch
requirement converts at 0.0254 µm per µin. Filtering costs about one
operation per sample per sample in a cutoff, which the two limits bound.

A scale benchmark generates one drawing with 10,000 texture symbols and exports
it as SVG and DXF. Another filters 83 profiles at 2,000 samples per cutoff
(996,000 samples, the record's budget) and accepts them by the 16% rule, in
about 0.9 s against a 3 s budget.

A record's `threads` list checks measured thread diameters against Unified
class limits; see
[Inspecting threads](THREADS.md#inspecting-threads-against-the-limits).

## Parts lists and balloons (schema 87)

`DrawingDefinition.parts_list` draws a bill of materials for the instances the
drawing's views show, and `balloons` point numbered item balloons at them.

```json
"parts_list": { "position_mm": [250, 280], "part_numbers": { "BlockFamily": "BLK-100" } },
"balloons": [ { "id": "b1", "view": "top", "instance": "plate", "offset_mm": [-15, 20] } ]
```

**Items.** Shown, unsuppressed instances are grouped by part identity: family,
effective parameter values (defaults, inherited and own overrides), the shown
output and material. Items are numbered by first appearance, view by view and
output by output, so numbers stay put while the views keep their order. Each
row lists item, quantity, part and material. The part is the family's part
number from `part_numbers` (1–40 characters) or its ID; when one family appears
with different parameter values, each gets `/ variant n` in order of
appearance. `DrawingDefinition::parts_list_items(graph)` returns the same rows
(with each item's instances) without drawing anything.

**Table.** The table's top-left corner is at `position_mm` on the paper, with
7 mm rows and ITEM, QTY, PART and MATERIAL columns (12, 12, 64 and 40 mm). It
is drawn with the sheet linework (`sheet_lines`/`sheet_labels`), so it exports
alongside the title block.

**Balloons.** Each balloon is a 4 mm-radius circle holding its item number,
placed `offset_mm` (at least 8 mm) from where its leader touches the part, with
a small dot there. The leader starts at an optional `anchor` datum on the same
instance, or by default the instance's placed origin. The instance must be
shown, unsuppressed, in the balloon's view. Balloons are annotation linework
(`gdt_lines`/`gdt_labels`) and both table and balloons count exactly in the
vertex budget.

**Nested lists (schema 91).** With `"nested": true`, the list follows the
assembly frames that hold the shown instances. Each frame becomes a
sub-assembly row (quantity 1, its part number from `part_numbers` keyed by
frame ID, or the frame ID), with the parts and sub-frames inside it beneath
it, indented 3 mm per level in the PART column and numbered hierarchically:
`1`, `1.1`, `1.2`, `1.2.1`. Identical parts still group within each level;
the same part in two frames appears under each. Balloons show the
hierarchical number. `PartsListItem` reports each row's `label`, `depth` and,
for sub-assembly rows, `frame`; `item` stays the row number. Flat lists keep
plain numbers and ignore frames.

Grouping is O(shown instances × (parameters + frame depth)); validation
indexes each view's shown instances once. Scale cases group 10,000 shown
pattern members, flat and inside a sub-assembly, and validate 100 balloons.

**Merged sub-assemblies (schema 157).** With `"merge_identical": true` (which
needs `nested`), sibling sub-assemblies with identical contents become one
row whose quantity is their count, as a bill of materials lists one assembly
used several times.

- **Identical** means the same parts, with the same identity (family,
  parameter values, output and material) and quantities. It also means the
  same sub-assemblies, compared the same way with their counts. Order does not
  matter, and merging works bottom-up, so identical hubs merge inside each
  wheel before wheels are compared.
- **Part numbers.** Frames with different part numbers are different parts
  and never merge. Frames without part numbers merge on contents alone, and
  the row shows the first frame's ID. Frames sharing a part number must have
  identical contents anywhere in the list; otherwise listing and validation
  fail and name both frames.
- **Rows.** The merged row is the first frame's: its `frame`, plus the
  others in `merged_frames`. Rows beneath it give quantities for one
  sub-assembly, but their `instances` include every copy's, so a balloon on
  any copy shows the merged row's number.

Signatures are built once per frame, bottom-up, so merging adds O(rows ×
depth) work. A scale case merges 1,000 sub-assemblies of ten parts each
(10,000 instances, 2,000 frames) into four rows in about 0.02 s. Not yet
included: custom columns and automatic balloon placement.

## Releases and revision tables (schema 90)

`DrawingDefinition.releases` records the drawing's released revisions, oldest
first, and `revision_table` draws them:

```json
"releases": [
  { "revision": "A", "description": "Initial release", "date": "2026-10-01",
    "approvals": [ { "role": "CHECKED", "name": "R. Checker", "date": "2026-09-30" },
                   { "role": "APPROVED", "name": "A. Approver", "date": "2026-10-01" } ],
    "model_revision": "r1" }
],
"revision_table": { "position_mm": [240, 260] }
```

- `revision` is 1–8 letters and digits, unique within the drawing. Releases are
  in date order; dates are calendar-checked `YYYY-MM-DD` (leap years included).
- Approvals carry a role, a name and a date on or before the release date.
- `model_revision` optionally names the model document revision (an ID in the
  document's revision ledger) the release was made from. It must exist, and a
  later release may not name an earlier model revision.
- When the drawing has a sheet, its title-block `revision` must equal the
  latest release, so the sheet and the release history cannot disagree.

**Status.** `ModelDocument::drawing_release_status(id)` reports `unreleased`,
`current` (the latest release names the latest model revision),
`model_changed` (the model has recorded revisions since; the drawing should be
re-released, with both revision IDs) or `untracked` (no model revisions are
linked or recorded).

**Table.** The revision table's top-left corner is at `position_mm`, with REV,
DESCRIPTION, DATE and APPROVED columns (12, 80, 24 and 34 mm) and 7 mm rows. It
must fit on the paper; descriptions longer than the column (38 characters)
are shortened with an ellipsis on the drawing only. APPROVED shows the
approver in an `APPROVED` role, or else the last sign-off. The table is sheet
linework and counts exactly in the vertex budget. Parts lists are likewise
refused when their rows would run off the paper.

Validation is O(releases + approvals + model revisions). A scale case
validates 10,000 releases with approvals and reports their status.

### Change markers and zones (schema 156)

A release's `changes` mark where it changed the drawing, as ASME Y14.35
practice identifies changes: revision symbols in the field of the drawing
and their zones in the revision table's ZONE column.

```json
"changes": [
  { "position_mm": [150, 200], "leader": { "view": "top", "anchor": { "instance": "part", "datum": "corner" } } },
  { "position_mm": [300, 120] }
],
"revision_table": { "position_mm": [240, 260], "zone_column": true }
```

- **Symbol.** Each marker draws the release's revision inside an apex-up
  triangle (a delta) centred on `position_mm`. The triangle is at least 8 mm
  on a side and grows to fit the revision at the notes lettering height. The
  triangle is common US practice; the shape has not been checked against the
  standard's own text. The whole symbol must lie on the paper.
- **Leader.** An optional `leader` draws an arrow from the triangle's edge to
  a datum's projection in a view. The datum must be on an unsuppressed
  instance the view shows, and must lie at least 4 mm beyond the symbol's
  corners.
- **Zones.** With `zone_column`, the revision table gains a 24 mm ZONE column
  before REV. Each release's row lists the distinct sheet zones of its
  markers in marker order (`B3, C6`), or `—` when it has none. Lists longer
  than ten characters are shortened with an ellipsis on the drawing only.
  The sheet must have zones, and every marker must lie inside them.

Symbols and leaders are annotation linework (`gdt_lines` and `gdt_labels`),
and count exactly in the vertex budget: 4 vertices a symbol and 5 a leader.
Markers of every release are drawn. A scale case draws 10,000 markers in 100
releases, with zones listed for each, in about 0.01 s against a 1 s budget.

### Approval workflow and content digests (schema 158)

`approval_workflow: { "roles": ["CHECKED", "APPROVED"] }` lists the sign-offs
every release collects, in order (1–8 unique roles).

- **States.** A release is `pending` until each role has signed, then
  `released`. `DrawingDefinition::release_state(index)` reports which, and
  for a pending release names the role awaited. Only the latest release may
  be pending. Each release's approvals must be the workflow's roles in
  order, dated in order.
- **Title block and status.** The title block shows the latest released
  revision, or stays blank until one is released. The revision table shows
  `PENDING` in a pending release's APPROVED cell.
  `ModelDocument::drawing_release_status` reports `pending`, with the role
  awaited and the latest released revision. `latest_released()` returns that
  release.
- **Signing.** `sign_release(role, name, date)` adds a sign-off to the latest
  release. With a workflow, the role must be the one awaited and the date must
  be on or after the previous sign-off and on or before the release. A
  release completed this way becomes the title-block revision. Without a
  workflow, any role may sign.

**Digests.** Each approval may carry a `digest`, which `sign_release` sets.
It is the SHA-256 of the drawing's canonical JSON (keys sorted) at the
moment of signing:

- the releases up to this one, and this release's earlier sign-offs with
  their digests;
- this approval without its own digest;
- the title-block revision left blank, since it follows the releases.

Each digest therefore covers the sign-offs before it, and any later change
to views, annotations, the sheet, model links or earlier approvals changes
it. `approval_digest(release, approval)` computes the expected value.

`verify_approvals()` checks the latest release's approvals:

- `valid`: the drawing is as signed;
- `changed`: the drawing or an earlier sign-off changed since;
- `unsigned`: the approval carries no digest.

Earlier releases signed earlier states of the drawing and are not checked.
The digest covers the drawing, not the model; a release's
`model_revision` link is the drawing's tie to the model's revision ledger.

**What a digest is not.** A digest shows that content has not changed. It
is not a cryptographic signature: it involves no key and does not prove who
signed. Anyone can recompute it after an edit. Identity-bound electronic
signatures (keys, certificates, signing services) are outside the document
format.

SHA-256 is implemented in `occt_parametric_core::digest`, checked against
the FIPS 180-4 test vectors, so the core gains no cryptographic dependency.
Each digest is O(drawing size). A scale case signs three roles on the next
release of a drawing with 10,000 released revisions, then verifies them, in
about 0.27 s against a 1 s budget.

### Multi-sheet revisions (schema 159)

A multi-sheet drawing is a set of drawings whose sheets share a
`drawing_number`, numbered 1 to `sheet_count`. A sheet's `revisions` states
how the set is revised; every sheet of the drawing must state the same
method and count, and all sheets must be in the document, each once.

```json
"sheet": { "drawing_number": "BLK-100", "sheet_number": 1, "sheet_count": 3, "revision": "A",
           "revisions": { "method": "per_sheet", "status_table_mm": [300, 150] }, ... }
```

- **`same_revision`.** Every sheet shows the drawing's revision. Only sheet
  1 records releases and a revision table. Every other sheet records none,
  and its title block shows sheet 1's latest released revision, or stays
  blank before the first release. Revising any sheet therefore revises the
  whole drawing.
- **`per_sheet`.** Each sheet keeps its own releases, revision table and
  title-block revision. Sheet 1 may draw a SHEET / REV status table at
  `status_table_mm`, listing each sheet's latest released revision, or `—`
  for an unreleased sheet. The table has 16 mm columns and 7 mm rows, and
  must fit on the paper.

These follow the practices that ASME Y14.35 names: revising at the drawing
level, at the sheet level, or with all sheets at the same revision. The
mapping has not been checked against the standard's own text.

The rules apply to whole documents (`ModelDocument` validation), since one
sheet cannot see its siblings. Sheets without `revisions` keep the earlier,
independent behaviour. The status table is drawn by
`DrawingDefinition::generate_many`, which needs every sheet of the drawing
in the batch. Generating sheet 1 alone, or a partial batch, fails. The
table counts exactly in the vertex budget.

Set checks are O(drawings). A scale case validates a 1,000-sheet
same-revision drawing and generates a 30-sheet per-sheet drawing with its
status table, in about 0.02 s against a 1 s budget.
