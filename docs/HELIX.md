# Helices, springs and coils

`FeatureOperation::Helix` makes an open helical wire to sweep profiles along:

```json
{ "id": "coil", "operation": { "helix": {
    "origin": { "literal": { "x": {"value": 0, "dimension": "length", "unit": "millimeter"},
                             "y": {"value": 0, "dimension": "length", "unit": "millimeter"},
                             "z": {"value": 0, "dimension": "length", "unit": "millimeter"} } },
    "axis":  { "literal": { "x": {"value": 0, "dimension": "scalar", "unit": null},
                            "y": {"value": 0, "dimension": "scalar", "unit": null},
                            "z": {"value": 1, "dimension": "scalar", "unit": null} } },
    "start": { "literal": { "x": {"value": 1, "dimension": "scalar", "unit": null},
                            "y": {"value": 0, "dimension": "scalar", "unit": null},
                            "z": {"value": 0, "dimension": "scalar", "unit": null} } },
    "radius": { "literal": {"value": 10, "dimension": "length", "unit": "millimeter"} },
    "pitch":  { "literal": {"value": 4, "dimension": "length", "unit": "millimeter"} },
    "turns":  { "parameter": "turns" } } } }
```

- The helix winds about the axis through `origin`, starting at
  `origin + radius * start` (`start` is made perpendicular to the axis), and
  rises `pitch` along the axis per turn. `turns` may be fractional, up to
  10,000. It is right-handed (counterclockwise looking against the axis as it
  climbs) unless `left_handed` is true.
- All values are expressions, so radius, pitch and turns can be family
  parameters; a spring regenerates when they change, reusing features that do
  not depend on them.

## Making a spring

Sweep a closed profile placed at the helix start along it with
`SweepOrientation::Binormal` set to the helix axis, which keeps the section
from twisting as it climbs. For a round wire, sketch a circle centered on the
start point in the plane perpendicular to the start tangent
`(0, 2π·radius, pitch)` (for an axis along z and a start along x; the y
component is negative for left-handed helices). A section larger than the
coil's tightest bend fails, as for any sweep.

## Kernel

`occt_bridge_create_helix_wire` (ABI 52; `Session::create_helix_wire` with
`HelixOptions` in Rust) builds the helix as a straight line on its exact
cylinder, one edge per turn (a fractional count is split into equal parts, so
no sliver edge is left over), each with a 3D B-spline within
`Precision::Confusion()`. Per-turn edges keep each swept face small, so a
boolean against a swept helix grows linearly with the turns: cutting a
100-turn thread groove took 14.6 s with one long edge and 3.8 s per turn-edge. Tests check its length against
`turns · √((2π·radius)² + pitch²)` to 1e-6, that sampled points lie on the
cylinder, its end points for whole and fractional turns and both hands, and
that a circle swept along it is a valid solid whose volume matches the tube
formula (area × centerline length) within 0.1%. Zero or negative radius,
pitch or turns, more than 10,000 turns, a start direction along the axis and
non-finite values are rejected.

## Not yet included

Modeled screw threads (a thread form swept along a helix and cut from a
cylinder) and variable-pitch or tapered (conical) helices. Hole thread
callouts still record intent without geometry.


## AI example and linked viewer dimensions

The [spring example](../tools/model/spring.request.json) is available through MCP
as `spring`. It uses schema 83 / native ABI 52 and exposes `coil_radius`,
`pitch`, `turns` and `wire_radius`. Derived expressions orient the section
perpendicular to the helix starting tangent, including after radius/pitch
edits. Conservative illustrative guards keep adjacent turns separated and
wire radius smaller than coil radius. They do not verify spring stiffness,
stress, fatigue, tolerances or manufacturing standards. The example has open
ends and right-handed winding; opposite winding also needs the matching
section orientation, or reflection of the complete solid.

Wire outputs display native outlines without surface triangulation.
A direct helix or a sweep whose immediate route is a helix shows linked coil
radius, axial pitch per turn, dimensionless turns and axial rise. Rise is
pitch times turns, excluding wire thickness and end treatments. The radius
anchor uses the native route start. Pitch is a complete-turn reference even
for fractional-turn helices. Sweep route length still comes from native edge
measurement.

Helix routes and their directly swept edge outlines use at least 32 intervals
per turn (at least 33 points total). Sampling is O(turns) per edge and consumes
the global vertex budget before native allocation. More than 100,000 points
per edge is rejected explicitly, rather than displaying an aliased coil.
Other sweep routes keep their existing sampling. Transformed helix routes
retain generic route displays; direct helix controls apply to the original
feature frame.

Tests cover radius/pitch/fractional-turn edits, native solid volume, correct
anchors, route continuity, oversized-wire rejection, accepted-part retention
and display limits. Dense spring batches can exceed MCP's 32 MiB resource read
limit; the returned local artifact path remains available for those views.

Release MCP gates verify 10 complete spring scenes in 6.773 seconds (30-second
budget) and 100 helix-wire scenes in 0.278 seconds (10-second budget). The dense
spring batch also checks bounded resource-read rejection and local-artifact
retrieval; wire scenes stay within the normal MCP read limit.
