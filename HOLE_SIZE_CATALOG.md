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

## Limits

This catalog chooses only nominal cylindrical clearance diameters. It does not
select tap drills, thread pitch, manufacturing tolerances, counterbore or
countersink dimensions, or inch sizes; nor does it validate fit, engagement,
strength, or machinability. Explicit custom dimensions remain available.
Thread metadata remains independent and is not inferred from a clearance size.
