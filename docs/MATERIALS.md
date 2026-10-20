# Materials

Schema 179 adds a frozen material catalog, `occb_materials_v1`, that
expressions and requirements can read. It holds typical values from public
manufacturer data sheets, read on 2026-10-20 and kept in the units each sheet
prints (converted in code). Where a sheet gives a range, the catalog keeps the
conservative end: the lower strength, stiffness and temperature. A property a
sheet does not print is absent, and asking for it is an error that names the
source, never a filled-in guess.

Typical values are not design allowables. Real stock is governed by the
supplier's certificate, and printed parts vary with printer, settings and
orientation.

## Catalog

Values are in kg/m³, MPa and °C. "HDT" is heat deflection temperature (ASTM
D648 / ISO 75) at 0.45 or 1.80 MPa; "service" is continuous service in air;
"layers" is the bond strength between printed layers.

| `material` | Density | Modulus | Yield | Ultimate | HDT 0.45 | HDT 1.80 | Service | Layers | Source |
|---|---|---|---|---|---|---|---|---|---|
| `aluminum_6061_t6` | 2,700 | 68,300 | 276 | 310 | | | | | Proto Labs aluminum-6061-t651_t6 |
| `stainless_steel_304` | 7,889 | 199,948 | 331 | 683 | | | | | Proto Labs stainless-steel-304 |
| `steel_alloy_4140` | 7,750 | 227,527 | | | | | | | Proto Labs steel-alloy-4140 |
| `titanium_cnc` | 4,470 | | 880 | 950 | | | | | Proto Labs titanium (no grade named; matches Ti-6Al-4V) |
| `brass_c360` | 8,498 | 96,527 | 310 | 386 | | | | | Proto Labs brass-c360 (yield at 0.5% extension) |
| `abs_cnc` | 1,030 | 2,075 | 39 | | | | | | Proto Labs abs (yield printed as ">5,600 psi") |
| `polycarbonate_cnc` | 1,200 | | 62 | 68 | 135 | 124 | | | Proto Labs polycarbonate-pc-black |
| `pom_acetal_copolymer` | 1,410 | 2,620 | 61 | | 158 | 104 | 82 | | Proto Labs pom |
| `nylon_6_6` | 1,140 | 2,413 | 83 | | 199 | 90 | | | Proto Labs nylon-6-6 |
| `peek` | 1,310 | 3,447 | 110 | | | 160 | 249 | | Proto Labs peek |
| `pei_ultem_1000` | 1,270 | 2,965 | 115 | 117 | 207 | 201 | 170 | | Proto Labs pei-ultem-1000 |
| `pla_printed` | 1,240 | 2,300 | 51 | | 55 | 55 | | 17 | Prusament PLA TDS v1.1 (2022), horizontal prints |
| `petg_printed` | 1,270 | 1,500 | 47 | | 68 | 68 | | 18 | Prusament PETG TDS v1.1 (2022), horizontal prints |

Rounded here; the code keeps full precision. Proto Labs sheets are the 2026
CNC material data sheets linked from
<https://www.protolabs.com/materials/cnc-machining/>. Aluminum 7075 and 1018
steel are left out because their sheets print no density.

## In expressions

`material_property_v1` returns a dimensionless number in the unit the
property names:

```json
{ "material_property_v1": { "material": "pla_printed", "property": "interlayer_strength_mpa" } }
```

Properties: `density_kg_per_cubic_meter`, `youngs_modulus_mpa`,
`yield_strength_mpa`, `ultimate_tensile_strength_mpa`,
`heat_deflection_0_45_mpa_c`, `heat_deflection_1_80_mpa_c`,
`continuous_service_c` and `interlayer_strength_mpa`. Use it in derived
parameters, for example to size a wall from an allowable stress.

## Requirements

- `mass_range` (exact): the output's exact volume times the catalog density
  lies in `minimum_kilograms..=maximum_kilograms`.
- `temperature_limit` (exact): the material's `limit`
  (`continuous_service`, `heat_deflection_0_45_mpa` or
  `heat_deflection_1_80_mpa`) exceeds `service_temperature_c` by at least
  `minimum_margin_c`.
- `bending_stress` (sampled): Euler–Bernoulli bending at one section. The
  output is cut by the plane through `section_origin` normal to `beam_axis`.
  The section's area moment about its centroidal axis along `bending_axis`
  (projected into the plane) comes from the kernel's exact central inertia
  of a slab 10⁻⁴ of the part's size thick, corrected for the slab's own
  thickness. The extreme-fiber distance comes from 64 samples on each section edge,
  so curved sections can read up to about 0.12% low. The peak stress M·c/I
  under `moment_newton_millimeters` must leave `minimum_safety_factor`
  against the material's `strength`: `yield` (default), `ultimate_tensile`,
  or `interlayer` for bending across printed layers. It ignores stress
  concentrations, shear, torsion and buckling. Its result reports the
  section area, I, c, stress and safety factor, and its witness is the
  extreme-fiber point.

Tests check rectangles, rods and tubes against bh³/12, πd⁴/64 and the hollow
tube formula to 1e-6, masses and temperature margins exactly, and missing
catalog values as errors. [`printed-spar`](../tools/model/printed-spar.request.json)
is a complete example: a 300 mm printed PLA box spar with mass, heat and
root-bending requirements (safety factor 2.73 at a 1.6 mm wall). Setting
`wall` to 0.8 mm drops the factor to 1.64 and the build is rejected with the
full verification table.
