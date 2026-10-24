# Standard parts

Schema 180 adds a frozen catalog of metric hardware, `occb_standard_parts_v1`,
and a `standard_part` feature that places a part at its nominal (maximum)
dimensions. Threads are not modeled: shanks and nut bores are plain cylinders
at the nominal diameter, so they are for fit, clearance, mass and visual
review, not for thread engagement (use [threads](THREADS.md) for that).

| `part` | Standard | Sizes (`nominal_diameter`, mm) | Needs `length` |
|---|---|---|---|
| `socket_head_cap_screw_iso_4762` | ISO 4762 hexagon socket head cap screw | 3, 4, 5, 6, 8, 10, 12, 14, 16, 20 | yes, under the head |
| `hex_nut_iso_4032` | ISO 4032 hexagon nut, style 1 | 3, 4, 5, 6, 8, 10, 12, 14, 16, 20 | no |
| `plain_washer_iso_7089` | ISO 7089 plain washer, normal series, grade A | 3, 4, 5, 6, 8, 10, 12, 14, 16, 20 | no |
| `dowel_pin_iso_8734` | ISO 8734 hardened parallel dowel pin | 1, 1.5, 2, 2.5, 3, 4, 5, 6, 8, 10, 12, 16, 20 | yes, overall |

Source: the dimension tables of fasten.it, read 2026-10-20
(<https://fasten.it/norms/norm/din_en_iso_4762>, `din_en_iso_4032`,
`din_en_iso_7089`, `din_en_iso_8734`), which cite DIN EN ISO 4762:2004,
4032:2013, 7089:2000 and 8734:1998. Two entries in those tables are
inconsistent and are not used: the M2.5 screw's k min (above its k max) and
the M24 nut's m max (below M22's). A test checks that every frozen column
grows with size.

## The feature

```json
{ "standard_part": {
    "part": "socket_head_cap_screw_iso_4762",
    "nominal_diameter": { "parameter": "bolt" },
    "length": { "literal": { "value": 30, "dimension": "length", "unit": "millimeter" } },
    "origin": { "...": "the seat, in millimeters" },
    "axis": { "...": "dimensionless direction the part extends" } } }
```

`origin` is the seat: the underside of a screw head (the head sits behind
it, the shank runs along `axis`), or the bearing face of a nut, washer or
pin end. Screws have a hex socket of across-flats s and depth t min; nuts
are hexagonal prisms with a nominal bore; washers are annuli; dowel pins have
45° end chamfers of the catalog c. An unknown size, a missing or unexpected
length, a pin too short for its chamfers, or a zero axis is an error.

## In expressions

`standard_part_dimension_v1` returns a catalog dimension as a length, for
sizing counterbores, wrench clearance or stack heights:

```json
{ "standard_part_dimension_v1": {
    "part": "socket_head_cap_screw_iso_4762",
    "nominal_diameter": { "parameter": "bolt" },
    "dimension": "head_diameter" } }
```

Dimensions: `head_diameter`, `head_height`, `socket_across_flats`,
`socket_depth` and `thread_length` (screws); `across_flats`,
`across_corners` and `height` (nuts); `inner_diameter`, `outer_diameter` and
`height` (washers); `chamfer` (dowel pins). Asking a part for a dimension it
does not have is an error.

## Example and tests

[`bolted-joint`](../tools/model/bolted-joint.request.json) stacks two plates
with an ISO 273 medium clearance hole, an M6 × 30 socket head cap screw, a
washer under its head, and a washer and nut below, all driven by `bolt`,
`plate` and `screw_length`. Tests match each part's volume to its closed form to
1e-9 (head, shank and socket; hexagon minus bore; annulus; cylinder plus two
frustums), check seats and bounds for any axis, and check errors and the
catalog expression.

To place screws, washers and nuts through a stack of parts with the screw
length chosen and matching holes cut, use a
[fastened joint](FASTENED_JOINTS.md).

Not yet included: rolling bearings (no public, machine-readable manufacturer
table was reachable) and inch hardware.
