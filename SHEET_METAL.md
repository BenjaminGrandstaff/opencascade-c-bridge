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

and the blank length is the sum of the flanges and allowances. The blank starts
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

## Scale

Evaluation is linear in the number of flanges, and accumulated dimensions are
checked for overflow before any kernel call. The scale suite builds 1,000
folded brackets with linked blanks in one part, edits their shared thickness,
and checks every analytic volume and that all handles are released.

## Limits

This is a single constant-width strip whose bends all share one axis direction.
Edge flanges on other edges, bend reliefs, hems, cutouts, corner treatments,
K-factor or bend tables, springback, and mold-line (outside setback) dimensioning
are not supported. Flange lengths are measured on the midplane between tangent
points.
