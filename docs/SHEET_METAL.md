# Sheet metal

Schema 45 adds folded sheet-metal strips and linked flat patterns to
`occt-parametric`. Both are ordinary features: parameters drive them, edits
rebuild them incrementally, and documents persist them.

## Folded sheet

`FeatureOperation::SheetMetal { definition }` builds one solid from a
`SheetMetalDefinition`:

| Field | Meaning |
|---|---|
| `origin` | Start of the thickness midplane (length vector). |
| `width_axis` | Direction the sheet's width extends from `origin` (dimensionless). |
| `start_direction` | Direction of the first flange; must be perpendicular to `width_axis`. |
| `width`, `thickness` | Positive lengths. |
| `flanges` | Straight midplane lengths between bends, 1 to 1024 of them. |
| `bends` | One `SheetMetalBend { angle_radians, inside_radius }` between each pair of flanges. |

The profile lies in the plane spanned by `start_direction` and the sheet normal
`width_axis × start_direction`, and is extruded along `width_axis`. A positive
bend angle turns toward the normal; a negative angle turns away from it. Angle
magnitudes must lie between 1e-6 and pi − 1e-6 radians; inside radii must be
positive. Bends are exact circular arcs with midplane radius
`inside_radius + thickness / 2`, so the folded volume equals the midplane
length times width times thickness.

Outlines that cross themselves, such as a tight inward spiral, are rejected.
Every failure leaves no partial output and keeps the previous accepted
generation.

## Flat pattern

`FeatureOperation::SheetMetalFlat { input, neutral_factor }` builds the blank
for the sheet-metal feature named by `input`. It must name a `SheetMetal`
feature directly. The neutral factor K is a dimensionless expression in
[0, 1] and is always supplied explicitly; there is no material table. Each bend
contributes the allowance

```text
allowance = |angle| * (inside_radius + K * thickness)
```

and the blank length is the sum of the flanges and allowances. K is a
manufacturing assumption supplied by the caller. The ideal folded model uses
a geometric midplane at K = 0.5, so folded and flat volumes agree at that value;
other K values model a different blank allowance, without changing the folded
geometry. The blank starts
at `origin`, runs along `start_direction`, and has the sheet's width and
thickness. Editing the folded sheet's parameters rebuilds both features; editing
only K rebuilds just the blank.

`SheetMetalDefinition::flat_pattern(&parameters, k)` returns the same numbers
without kernel work as `FlatPatternMetrics`: blank length, width, thickness, K,
each bend allowance, and each bend line, measured from the start of the blank
to the center of that bend's allowance zone. `FlatPatternMetrics::drawing(id)`
turns them into a `GeneratedDrawing` with the blank outline and dashed bend
lines, which exports through the usual SVG and DXF writers (see
[Drawings](DRAWINGS.md)).

```rust
let metrics = sheet.flat_pattern(&parameters, 0.44)?;
std::fs::write("bracket.dxf", metrics.drawing("bracket")?.to_dxf())?;
```

Independent DXF parsing of a two-bend blank found no errors or repairs, and
SVG rendering verified the outline, bend lines, and thickness/K metadata.

## Cutouts (schema 160)

`SheetMetalDefinition.cutouts` lists holes and openings through flanges:

```json
"cutouts": [
  { "id": "hole", "flange": 1, "along": 10, "across": 5, "shape": { "kind": "circle", "diameter": 4 } },
  { "id": "slot", "flange": 2, "along": 10, "across": 5, "shape": { "kind": "slot", "length": 8, "width": 4 } }
]
```

- **Placement.** `flange` counts flanges from 0 at the origin. `along` is the
  center's distance on the midplane from the flange's start tangent point,
  and `across` its distance from the origin edge of the width.
- **Shapes.** A `circle` has a `diameter`. A `rectangle` has a `length`
  along the flange and a `width` across it. A `slot` is an obround with that
  `length` overall and semicircular ends of `width`; it must be at least as
  long as it is wide.
- **Validation.** All are length expressions, so parameters drive them. A
  cutout must lie strictly inside its flange's straight region (not into a
  bend) and inside the width. IDs are unique, and a sheet has at most 10,000
  cutouts.

**Folded sheet.** Each cutout is a prism square to its flange, a little
thicker than the sheet, and all are cut in one boolean. The result must be
one valid solid.

**Flat blank.** A cutout's center lies at its flange's start on the blank
plus `along`. The flange start is the earlier flanges plus their bend
allowances at the blank's K, so the blank moves cutouts with K exactly as it
moves bend lines. `FlatPatternMetrics.cutouts` lists each center, and the
flat-pattern drawing outlines them (arcs in 32 chords per half circle). The
blank solid is built as a face with the cutouts as holes, pushed through the
thickness, with no boolean. Cutouts that overlap, which one face cannot hold
as separate holes, fall back to a boolean cut.

Folded and blank volumes both drop by the cut-out area times the thickness.
A test checks that, the blank centers including the bend allowances, and
that a hole's center in the folded sheet lies its radius from the nearest
material.

## Hems (schema 162)

`start_hem` and `end_hem` fold either end of the strip back on itself:

```json
"end_hem": { "kind": "open", "side": "positive", "gap": 1.0, "length": 10 },
"start_hem": { "kind": "rolled", "side": "negative", "inside_radius": 1.0, "angle_radians": 4.712 }
```

- **Side.** `positive` folds onto the side of the adjacent flange that a
  positive bend turns toward; `negative` onto the other side.
- **Open hem.** A 180° bend with inside radius `gap / 2`, then a flange of
  `length` running back alongside its flange, `gap` from it. The gap must be
  at least a tenth of the thickness, so a cutout's cutter, which reaches
  0.05 × thickness past the sheet, never touches the hem. A closed hem (no
  gap) would make faces touch and is not a valid solid.
- **Rolled hem.** A bend of `angle_radians` (more than π) at
  `inside_radius`, ending in the roll with no flange. Past
  `2π − acos(r / (r + t))` the roll's outside would re-enter its own
  flange, so larger angles are rejected.
- **Placement.** The origin stays the start of flange 0, heading along
  `start_direction`; a start hem lies before it. Cutout flange numbers still
  count the user's flanges from 0.
- **Blank.** A hem is a bend like any other: its allowance at K, or from an
  allowance bend table, which may list angles up to 360° (deduction tables
  stay below 180°, where the outside setback is finite). The blank starts
  at the start hem's free end, so cutouts and bend lines include its
  allowance.

Tests check exact folded and flat volumes with hems, the 1 mm gap of an
open hem on each side by a probe at its center, a rolled start hem's extent
and that it leaves flange 0 and its hole in place, and the rejected cases.

### Closed and teardrop hems (schema 177)

```json
"end_hem": { "kind": "closed", "side": "positive", "length": 10 },
"start_hem": { "kind": "teardrop", "side": "negative", "inside_radius": 2, "angle_radians": 5.03 }
```

- **Closed hem.** Folded flat through 180° onto its flange, with no gap,
  for `length`. The fold's inside is a crease, and its outside a half
  circle of radius t about the crease. The leaf lies on the flange's
  face, so the hem and flange are one solid, a sheet's thickness thicker
  over the leaf.
- **Teardrop hem.** Rolled through `angle_radians` at `inside_radius`,
  then straight on until it lands on its flange's face. There it is cut
  on that face, so the hem joins the flange and encloses a teardrop void.
  The void runs through the width as a hole in the profile.
  `TeardropGeometry::new` gives the roll, the leg's landing points and its
  midplane length. The angle must exceed π, and stay below
  `2π − acos(r / (r + t))` so the roll stays clear of the flange. The
  leg must land behind the fold, within the flange.
- **On the flange.** Either hem must lie within its flange. Cutouts may not
  lie under it, and its bend cannot be relieved by an edge flange.
- **Blank.** A closed hem is a 180° bend at zero inside radius: its
  allowance at K is `π K t`. A teardrop is its roll's bend and then its
  leg's midplane length. At K = 0.5 the blank's volume equals the folded
  hem's exactly. That covers the half disc of the closed fold, and the
  mitred leg, whose area is t times its midplane length. A bend table
  must cover the hem's bend like any other, so a closed hem (radius 0)
  needs K.
- **Construction.** The strip's outline ends in a cap instead of a square
  end. The hem side's face is cut back to where the hem lands, then the
  cap runs round the fold to the other face. A teardrop adds the void as
  an inner loop. Each costs about what an open hem does, 1–2 ms per sheet.

Tests check both kinds at either end and on either side against analytic
volumes and bounds. They also check that a teardrop is one shell, with its
void, and a closed hem on a tray whose walls and reliefs take the slab path.
The rejected cases are covered too.

## Edge flanges and bend reliefs (schema 176, ABI 83)

`SheetMetalDefinition.edge_flanges` folds flanges up from the strip's width
edges, as on a tray or a box:

```json
"edge_flanges": [
  { "id": "wall", "flange": 1, "edge": "far", "start": 0, "length": 40,
    "angle_radians": 1.5708, "inside_radius": 1, "height": 15, "relief_depth": 5 }
]
```

- **Placement.** `flange` counts the strip's flanges from 0, as for
  cutouts. `edge` is `origin` (the start of the width) or `far` (its end).
  The edge flange runs `length` along the base flange from `start`, measured
  like a cutout's `along`. It must stay inside the base flange's straight
  region. Edge flanges on one edge of one flange may touch but not overlap.
- **Bend and leg.** A bend of `angle_radians` (positive toward the side the
  strip's positive bends turn to, magnitude below π) at `inside_radius`,
  then a straight leg of `height`.
- **Reliefs.** Where an edge flange reaches the end of its base flange and
  a bend follows, `relief_depth` cuts a rectangular notch that deep from
  its edge through the whole bend zone. The bend can then be formed without
  tearing the corner. A relief needs a bend to reach, and reliefs from both
  edges of one bend must leave material between them. Edge flanges on both
  sides of one bend share the deeper relief.
- **Flat blank.** Each edge flange unfolds as a tab off the blank's edge,
  its bend allowance (at K, or from the bend table) and then its leg. Tabs
  follow their base flange as K moves it. Reliefs are notches through the
  bend zones. `FlatPatternMetrics` lists `edge_tabs` and `reliefs`.
  `outline()` gives the blank's polygon, which the drawing outlines with
  each tab's bend line dashed. The blank is that polygon with the cutouts
  as holes, pushed through the thickness.
- **Folded sheet.**
  - The strip is built in slabs across its width: a slab inside a relief
    leaves out the relieved bend, so the strip falls apart there into
    separate chains of flanges.
  - Each edge flange is a strip of its own, starting on the side face it
    shares with its base flange.
  - The pieces only touch, so one glued fuse joins them (ABI 83,
    `occt_bridge_fuse_glued`). That is OCCT's glue mode followed by merging
    coplanar and coaxial faces, so no seams remain.
  - Cutouts are cut afterwards as before.

At K = 0.5 the folded and flat volumes agree exactly. Tests check this for
a tray with walls on both edges and four reliefs, against the analytic
volume. The walls must also stand on the bends' side, 1 + 2 mm outside the
width. A lip turning the other way, part way along a flange, follows a
rolled start hem on the blank. Validation is tested for each error.

At scale, 500 trays with two walls and four reliefs each, and their
blanks, build in 10.4 s and rebuild after a thickness edit in 10.5 s (30 s
budget). That is about 18 ms per folded tray and 3 ms per blank. The glued
fuse took the folded tray from 43 ms, with a general fuse and relief cuts,
to 18 ms.

## Bend tables (schema 161)

A shop that has measured its press brake can give the flat pattern a
`bend_table` instead of relying on the neutral factor:

```json
"operation": { "sheet_metal_flat": {
  "input": "folded",
  "neutral_factor": 0.44,
  "bend_table": {
    "name": "AL-2",
    "kind": "allowance",
    "thickness_mm": 2.0,
    "inside_radii_mm": [2.0, 3.0],
    "angles_degrees": [45, 90, 135],
    "values_mm": [[2.2, 4.3, 6.4], [3.0, 5.9, 8.8]]
  }
} }
```

- **Axes.** Rows follow `inside_radii_mm` and columns `angles_degrees`.
  Both axes hold 1–256 strictly increasing values; radii are positive and
  angles lie strictly between 0° and 180° for deductions and 0° and 360°
  for allowances, which may cover hems. The angle is the bend's change of
  direction (`angle_radians` in degrees, either sign), not the included
  angle between the flanges.
- **Kinds.** An `allowance` table gives the developed length of the bend
  zone, which must be positive. A `deduction` table gives what the bend
  takes off the sum of its flanges' outside mold-line lengths; the blank
  uses the equivalent allowance `2 (r + t) tan(θ/2) − deduction`, which
  must also be positive.
- **Lookup.** Values interpolate bilinearly between the four surrounding
  entries. A one-entry axis matches only its own value. A bend outside the
  table (beyond 10⁻⁶ mm or 10⁻⁶°) is an error naming the bend, as is a sheet
  whose thickness differs from `thickness_mm` by more than 10⁻⁶ mm. The
  table is never extrapolated and never mixed with K: it must cover every
  bend.
- **Effect.** The table changes only the blank. Bend lines and cutout
  centers move with its allowances, `FlatPatternMetrics.bend_table` names
  it, and the drawing's metadata shows "Bend table" instead of the K
  factor. `neutral_factor` is still required and validated but does not
  affect a tabled blank. `SheetMetalDefinition::flat_pattern_with_table`
  gives the metrics without kernel work.

Editing the table rebuilds only the blank, like editing K. Validation is
linear in the table's entries and each bend's lookup is two binary
searches.

## Scale

Evaluation is linear in the number of flanges, and accumulated dimensions are
checked for overflow before any kernel call. The scale suite builds 1,000
folded brackets with linked blanks in one part, edits their shared thickness,
each with an open hem, and checks every analytic volume and that all handles are released. Another
cuts 1,000 holes through ten flanges and their blank in about 4 s against a
10 s budget, and checks the exact removed volume. Most of that time is the
folded sheet's single boolean. A third computes 100 blanks of 1,023 bends
through a full 256 × 256 bend table, linear in radius and angle so every
interpolated length is checked exactly, against a 1 s budget.

## Limits

The strip itself is constant-width, with its bends sharing one axis
direction; edge flanges fold about the perpendicular axis (schema 176).
Edge flanges on edge flanges, cutouts in edge flanges, corner closures where
edge flanges meet, cutouts across bends,
corner treatments, tables keyed by material or across thicknesses,
springback, and mold-line (outside setback) dimensioning
are not supported. Flange lengths are measured on the midplane between tangent
points.
