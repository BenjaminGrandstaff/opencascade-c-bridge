# Fastened joints

Schema 186 adds two features. Together they replace placing a screw, its
washers, a nut and every hole by hand:
- **`fastened_joint`** puts ISO 4762 socket head cap screws through a stack
  of parts, held by nuts or by a tapped hole in the last part. It measures the
  stack along each screw, picks the screw length, places the hardware and
  checks that the joint can be assembled.
- **`joint_holes`** drills one of the stack's parts to match.

```json
{ "id": "joint", "operation": { "fastened_joint": {
    "stack": ["lid", "base"],
    "positions": [ { "...": "a point on each screw's axis line" } ],
    "axis": { "...": "from the head toward the nut or tapped part" },
    "nominal_diameter": { "parameter": "bolt" },
    "fastening": "tapped",
    "head": "counterbored" } } }
{ "id": "lid_drilled",  "operation": { "joint_holes": { "joint": "joint", "input": "lid" } } }
{ "id": "base_drilled", "operation": { "joint_holes": { "joint": "joint", "input": "base" } } }
```

The joint's result is the hardware, one compound with every screw, washer
and nut. Group it with the drilled parts (for example with a `compound`) to
output the assembled joint.

## Settings

| Field | Meaning | Default |
|---|---|---|
| `stack` | 1 to 8 distinct part feature ids, head side first | required |
| `positions` | 1 to 1,000 points, one on each screw's axis line (anywhere along it) | required |
| `axis` | Direction from the head into the stack | required |
| `nominal_diameter` | Screw size: M3, M4, M5, M6, M8, M10, M12, M14, M16 or M20 | required |
| `fastening` | `nut` (ISO 4032 behind the last part) or `tapped` (the last part is tapped and not clamped) | `nut` |
| `head` | `surface` (on the first part, on a washer if the joint has washers) or `counterbored` (sunk flush, with no washer) | `surface` |
| `washers` | ISO 7089 plain washers under the head and the nut | `true` |
| `clearance` | ISO 273 clearance series: `fine`, `medium` or `coarse` | `medium` |
| `length` | Screw length under the head | the shortest standard length that fits |
| `engagement` | Least thread engagement in a tapped part | 1.5 × diameter |

All are expressions, so parameters can drive them.

## How a joint is planned

Along each screw's axis line, the kernel finds where each stack part's
material starts and ends. Then, for each screw:
- **Contact:** neighboring parts must touch (within 0.001 mm) in stack order.
  A gap, an overlap, parts out of order, or a line that misses a part is
  refused, naming the parts and the position.
- **Seat:** the screw head sits on a washer on the first part, on the part
  itself, or at the floor of a counterbore. The counterbore is the
  [Carr Lane socket-head recess](HOLE_SIZE_CATALOG.md#carr-lane-v1-tap-drills-and-socket-head-recesses):
  as deep as the head is tall, and narrower than the part is thick.
- **Least length:**
  - nut: through the stack, the nut washer and the nut, plus two thread
    pitches past the nut;
  - tapped: to the tapped part's face plus the engagement.

One screw length serves every position: the longest of the least lengths,
rounded up to the next ISO 4762 standard length, or the given `length` if it
is long enough. The finished joint is then checked:
- The thread must reach the nut or the tapped part. A long screw's unthreaded
  shank (length minus the thread length b) cannot clamp.
- A tapped hole is drilled two pitches deeper than the screw reaches, plus a
  118° drill point. It must end at least 0.5 mm short of the part's far side.
- Screws must be at least the hardware's footprint apart: the widest of the
  washer, nut corners, head, counterbore and hole.

Messages give the numbers. For example: "the M5 × 16 screw's tapped hole in
'base' needs 12.36 mm but the part is 12 mm thick at joint position 1; use a
shorter screw or a nut".

## Holes

`joint_holes` cuts every hole the joint needs in `input`, which must be in
its stack, in one boolean:
- **Clearance holes** (ISO 273 at the chosen series) through each clamped
  part, with the counterbore in the first part when the head is sunk.
- **Tap-drill holes** in a tapped joint's last part: the Carr Lane 75 % drill
  for the coarse pitch, to the depth above, with a 118° point.

The screw is modeled at its nominal diameter, so it overlaps the tap-drill
hole over the engaged length. That overlap stands for the thread, as in most
CAD programs. Holes of the same shape are built once and copied, so 1,000
screws cost one cut per part.

## Reports

A build lists, under each output, the joints it is built from, as
`fastened_joints`:

```json
{ "feature": "joint", "stack": ["lid", "base"], "positions": 4,
  "screw": { "nominal_diameter_mm": 5, "pitch_mm": 0.8, "length_mm": 12,
             "chosen": true, "minimum_length_mm": 10.5 },
  "grip_mm": { "min": 3, "max": 3 },
  "thread_engagement_mm": { "min": 9, "max": 9 },
  "hardware": [ { "part": "socket_head_cap_screw_iso_4762",
                  "designation": "ISO 4762 M5 × 12", "quantity": 4 } ] }
```

The hardware list has one line per part with its total quantity. Placed
instances are measured where they are. The library entry point is
`PartInstanceFasteners::fastened_joints`; the planner is the kernel-free
`occt_parametric::fasteners::plan_joint`. Section renders now cut each solid
of a compound separately, so a screw in its tapped hole is drawn correctly.

## Sources

- Screw, nut and washer dimensions: the
  [standard-part catalog](STANDARD_PARTS.md).
- Clearance holes: ISO 273 in the
  [hole-size catalog](HOLE_SIZE_CATALOG.md).
- Tap drills, pitches and counterbores: Carr Lane V1 in the
  [hole-size catalog](HOLE_SIZE_CATALOG.md).
- Standard screw lengths: ISO 4762 as tabulated by Engineers Edge
  (<https://engineersedge.com/iso_socket_head_screw.htm>, read 2026-10-23).
  That table has no M14 row, so an M14 joint needs a `length`.

## Limits

- Only socket head cap screws. Other screw types are not supported.
- Screws are straight lines through parts that touch. Spacers, gaps and
  slotted holes are not modeled.
- Threads are not modeled, and neither is preload or joint strength.
- The joint and its holes are measured on the stack parts as they are
  generated, so drill the parts with `joint_holes`, not with separate hole
  features that would change the measured material.

## Example and tests

[`fastened-lid`](../tools/model/fastened-lid.request.json) fixes an 8 mm lid
to a 20 mm base with four M5 screws. The heads are counterbored flush and
the screws are tapped into the base. The `bolt` and `lid` parameters
re-plan it: M6 gives 12 mm screws, and a 4 mm lid is refused because the
counterbore does not fit.

Tests cover:
- **Core planner:** the standard-length table; nut, tapped and counterbored
  layouts against hand-worked lengths, seats, grips and engagements; the
  parts list; and every refusal (gap, overlap, too short, plain shank in the
  nut, no standard length, M14 without a length, tapped hole breaking
  through, counterbore too deep, colliding screws).
- **Kernel:** hardware volumes equal the catalog parts', bounds match the
  seats, hole volumes match closed forms, and the hardware does not overlap
  the clearance-drilled parts. A tapped joint along +x has a flush head and a
  drill-pointed blind hole. Parameter edits re-plan the screw and redrill.
- **Model command and MCP:** the report, size edits, refusals, and a joint in
  a rotated, moved frame.

Scale: 1,000 M5 screws through a 960 × 600 mm lid into a tapped base are
measured, placed and drilled in 4.8 s (release build; budget 20 s).
