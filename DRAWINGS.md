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
not a standards-conformity claim. Standards-verified layout,
material-specific hatch conventions and exact curve export remain on the roadmap.

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
individually so overlapping components remain present. Sampled boundary parity
preserves holes; interval union across faces preserves disconnected islands and
avoids double fill where components overlap. A view uses one shared pattern;
material-specific patterns and alternating adjacent-component angles remain future
work. Detail windows clip the resulting lines alongside the outlines.

`GeneratedDrawing.hatches` contains separate two-point polylines. SVG draws them
behind outlines with a 0.13 mm stroke. DXF uses LWPOLYLINE entities on the
SECTION_HATCH layer with lineweight 13. These exports retain the existing sampled
curve limitation: `curve_samples` controls hole and curved-boundary resolution,
without certifying chordal error or ASME conformance.

For E sampled segments and K scanline crossings, fill generation takes
O(E + K log K) time and O(E + K) temporary storage, in addition to one native
plane intersection per solid. A hard limit of 2,000,000 samples/crossings per view
rejects excessively dense patterns before emitting unbounded output. Line indices
must stay below 2^52 in magnitude. Hatch endpoints share `maximum_vertices` with
all other drawing geometry. The assembly benchmark checks 10,000 hatch segments
across 1,000 placed parts, exports and native handle cleanup within 30 seconds.


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
projected zones, advanced modifiers, measured datum-reference-frame solving, datum-shift
calculations, tolerance-zone inspection beyond fixed supplied-axis samples and semantic PMI exchange remain
on the roadmap. References: [ASME Y14.5 scope and contents](https://www.asme.org/getmedia/da2ff89e-067b-4160-8e2e-53e6c7da1d3b/17707.pdf)
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

Schema 64 adds optional `DrawingFeatureControlFrame.size_limits`:
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
without kernel handles. Saved document schema remains 64.
