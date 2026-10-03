# Hole-size catalog

Schema 32 adds a frozen V1 metric clearance-hole catalog. Dimensions were
verified on 2026-10-02 against the clearance table published by fastener
manufacturer [EKINSUN](https://www.ekinsun.com/custom-fasteners/clearance-hole-chart/),
which labels it ISO 273. This is a nominal-diameter reference snapshot, not a
complete implementation or certification of that standard.

Supported nominal sizes (mm): 1.6, 2, 2.5, 3, 4, 5, 6, 8, 10, 12, 14, 16,
18, 20, 22, 24, 27, 30, 36. `ClearanceSeries` selects fine, medium, or coarse.
The 57 frozen diameter values live in `src/hole_sizes.rs` in `occt-parametric`
and are checked individually by tests.

## API

Use a catalog expression instead of a literal bore diameter:

```rust
let diameter = ScalarExpr::Iso273ClearanceV1 {
    nominal_diameter: Box::new(ScalarExpr::Parameter("fastener_diameter".into())),
    series: ClearanceSeries::Medium,
};
```

It returns a length and can drive `Hole.diameter`, derived parameters, or other
length-valued expressions. Changing a referenced parameter or the series
invalidates dependent features. Unsupported sizes reject regeneration and
preserve the previous accepted output.

For a standalone lookup:

```rust
let diameter = iso273_clearance_v1(
    Quantity::length(6.0, LengthUnit::Millimeter),
    ClearanceSeries::Medium,
)?;
```

Inputs must be valid finite lengths. Supported units are normalized to
millimeters, and the returned `Quantity` uses millimeters. A 1e-9 mm comparison
tolerance permits unit-conversion roundoff only; there is no interpolation,
nearest-size selection, or fallback. V1 values must not change: future
corrections or additions require a new catalog version. Schema 31 and earlier
documents migrate without changing their existing dimensions.

## Carr Lane V1 tap drills and socket-head recesses

Schema 45 adds two more frozen catalogs from the Carr Lane Manufacturing
reference booklet, Rev. 9/2021
([PDF](https://www.carrlane.com/Portals/0/PDFs/CLM-Trig%20Booklet-ENG-v4-PM.pdf)),
pages 3 and 9. Every value was checked against that PDF on 2026-10-03.

- **Tap drills** (`ScalarExpr::CarrLaneTapDrillV1`, `carr_lane_tap_drill_v1`):
  the manufacturer's closest drill for 75% theoretical cut thread. Metric
  covers M1.6x0.35 through M27x2 (28 coarse and fine pairs). Inch covers #0-80
  through 1-1/8-12 (36 UNC/UNF pairs). Pitch is a length in both systems: for
  1/4-20 pass `Quantity::length(1.0 / 20.0, LengthUnit::Inch)`.
- **Socket-head recesses** (`ScalarExpr::CarrLaneSocketHeadV1`,
  `carr_lane_socket_head_v1`, `carr_lane_socket_dimension_v1`): counterbore
  diameter and depth plus normal- and close-fit clearance diameters for
  socket-head cap screws. Metric covers M1.6 through M48 (18 sizes); inch
  covers #0 through 2 in (20 sizes).

`HoleCatalogSystem::{Metric, Inch}` selects the table. Inch rows are keyed by
basic major diameter (#10 is 0.190 in). Fractions are exact rational inches;
number and letter drills use the decimal values the booklet prints for them
(#7 is 0.201 in), and the 5/16 in counterbore depth keeps its printed 0.312 in
rather than 0.3125 in. Results are millimeter `Quantity` values.

A hole driven entirely from a nominal size:

```rust
let socket = |dimension| ScalarExpr::CarrLaneSocketHeadV1 {
    nominal_diameter: Box::new(ScalarExpr::Parameter("screw".into())),
    system: HoleCatalogSystem::Metric,
    dimension,
};
let finish = HoleFinish::Counterbore {
    diameter: socket(SocketHeadDimension::CounterboreDiameter),
    depth: socket(SocketHeadDimension::CounterboreDepth),
};
let diameter = socket(SocketHeadDimension::NormalClearance);
```

Lookups follow the same contract as ISO 273 V1: exact keys within 1e-9 mm, no
interpolation or nearest size, unsupported pairs fail regeneration and keep the
previous accepted output, and V1 values never change. The booklet's optional
60-degree countersink relief (column C) is not generated.

## Limits

These catalogs choose nominal diameters and depths only. They do not select
manufacturing tolerances, thread classes, or engagement lengths, and do not
validate fit, strength, or machinability. A tap-drill expression does not
create `Hole.thread` metadata; record the thread separately. Explicit custom
dimensions remain available.
