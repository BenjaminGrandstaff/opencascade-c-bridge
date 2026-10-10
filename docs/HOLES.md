# Hole bottoms and drill points

Schema 75 adds optional `bottom` to `FeatureOperation::Hole`. It defaults to
`"flat"`, preserving existing blind and through-all holes. Native ABI stays 48.

```json
"bottom": {
  "drill_point": {
    "angle_radians": {"parameter": "point_angle"}
  }
}
```

The included point angle is a scalar in radians, strictly between zero and pi.
For a bore diameter `d`, the cone adds `d / (2 * tan(angle / 2))` to the blind
depth. The existing `extent.blind.depth` is the full-diameter cylindrical depth;
it does not include the point. A 6 mm bore at an included angle of 120 degrees
adds approximately 1.732 mm of tip depth. Angle values are supplied geometry,
not a standards lookup or a recommendation for a particular cutting tool.

Drill points require `blind` extent. Native overlap volume checks that the
complete conical tip lies in the original input material within Boolean
tolerance; a tip that breaks through a face or enters an existing void is
rejected. This is containment, not a specified minimum tip clearance or a
manufacturing-strength check. Counterbores and countersinks remain entry
recesses and combine with the tip. Thread metadata only records intent unless
it is modeled; see [modeled tapped holes](#modeled-tapped-holes-schema-113).

The cone, barrel and optional entry recess are fused before one cut from the
original body. Native cut history remains attached to that original input.
Changing the included angle invalidates the hole and its dependents while
reusing an unchanged blank. Invalid angles, incompatible units, through-all
points and breakout leave the accepted geometry intact.

Drawing callouts distinguish `FULL DIA DEPTH` and append `DRILL POINT` with the
included angle. This records the supplied geometry; it does not assert a
standards-conforming hole symbol. The viewer shows full-diameter bore depth,
an included-angle arc, computed tip depth and total depth metadata. Labels link
to diameter, bore-depth and point-angle controls. Family-coordinate anchors also
follow placed parts in the assembly viewer.

The [example request](../tools/model/drill-point.request.json) is available through
MCP as `drill-point`. Change `point_angle`, `diameter` or `depth` to regenerate
the hole. Increasing depth far enough to expose the point rejects the edit.

The implementation adds one cone, one containment intersection, one cutter
fusion and one cut, with native cost governed by input and cutter topology.
There is no additional global graph scan. The `drill_points` scale gate checks
500 regenerations, analytical volume and validity, and zero retained handles
against a 10-second budget.


## Geometry-driven depth (schema 76)

`extent` also accepts `"up_to_next"` or
`{"up_to_face":{"face":FACE_SELECTOR}}`. Both resolve against the hole's
`input` output and travel forward from the supplied position along its axis.
The position is the cutter start plane; it is not automatically moved to an
entry surface. Through-all keeps its existing two-direction behavior.

A selected limit must resolve to exactly one bounded face and terminate the
whole circular bore profile strictly forward. Next-face mode searches the input
faces for the earliest complete cutoff; crossing competing limits are ambiguous
and require explicit selection. Parallel planar, inclined and curved limits use
the same exact native coverage/splitting operations as extrusion. Coincident,
backward, ambiguous and partially covering limits fail. Surface boundaries are
respected. Moving the input geometry or selector parameters rebuilds the hole.
Named, persistent and history selectors participate in dependencies and normal
reference validation.

Counterbores and countersinks remain supported. The entry recess must stay
strictly before the same selected limiting face; native surface-distance checks
reject contact or extension beyond it, including a face wholly inside the
recess. A shallow wide recess can terminate at a smaller internal cavity face.
Drill points remain specific to numeric blind depth.

Drawings record `UP TO FACE` or `UP TO NEXT FACE`. The viewer displays the actual
bore-centre ray distance and end point, with links to upstream geometry and
selector parameters. For a nonuniform cap, this is the travel along the bore
centre, not a statement that all radial positions have the same depth.
`PartInstance::hole_limit_measurement` exposes this derived witness without
retaining temporary handles; it expects family-local geometry generated from
that instance's current definition and parameters.

The [hole-limits example](../tools/model/hole-limits.request.json), also available
as `hole-limits` in MCP, cuts through a spherical cap with both limit modes.
Changing `depth` moves the cap and updates both holes and their measured depths.
Its selected-face rule follows a parameterized location instead of relying on
face area ordering.

The shared cutoff helpers index input faces once, retaining only bounded native
candidates. Next-face selection adds at most one containment comparison per
candidate. A single-output witness query also indexes the family definition.
The `hole_limits` gate checks 100 two-hole curved-cap regenerations, analytical
volume within body-scale integration tolerance, exact ray witnesses, validity
and handle cleanup against a 10-second budget.

## Modeled tapped holes (schema 113)

A hole's `thread` callout can cut its thread. Add `modeled`, optionally with a
`length`:

```json
"thread": {"designation": "M6x1", "nominal_diameter": {"literal": {"value": 6, "dimension": "length", "unit": "millimeter"}},
           "pitch": {"literal": {"value": 1, "dimension": "length", "unit": "millimeter"}},
           "handedness": "right", "modeled": {"length": {"parameter": "thread_length"}}}
```

The hole cuts an ISO 68-1 basic-profile internal thread of the callout's
nominal diameter, pitch and handedness, from the hole entry along its axis.
The groove and helix are those of the [thread feature](THREADS.md) with
`internal: true`. The groove is fused into the hole cutter (bore, tip and
entry recess), so one boolean cut removes everything and face history stays
attached to the original input.

- **Bore.** The hole diameter is the tap drill. It must be at least the basic
  minor diameter (`nominal − 2·5H/8`, 8.376 mm for M10×1.5) and less than the
  nominal diameter. A larger tap drill leaves a wider crest flat.
- **Extent.** Blind and through-all holes only; up-to-face and up-to-next
  limits are rejected. Through holes thread their whole span by default.
  Blind holes stop at least half a pitch above the full-diameter bore depth,
  so the groove (at most about 0.41 P each side of its helix) never cuts below
  the bore floor. Omitted `length` threads that whole usable span. A drill
  point lies beyond the bore depth and is unaffected.
- **Entry.** The thread starts at the hole position. A countersink finish acts
  as an entry chamfer; a counterbore wider than the nominal diameter leaves
  the groove in free space over its depth.
- **Runout.** `modeled.runout` (schema 114) fades the groove out over that
  axial length after the thread length, into the bore (see
  [thread runout](THREADS.md#runout-schema-114)). Blind holes must fit length
  plus runout in the usable bore; a through hole's runout may pass its exit.
  Without runout the groove ends abruptly.
- **Not included.** Tolerance classes and fit checks.

The viewer labels the hole that produces the output with its callout, thread
length and turns (for example `M6x1 RH tapped · 9.000 mm · 9.000 turns`),
linked to the length and size controls, or `full bore` when the length is
omitted. Thread length, nominal diameter and pitch parameters invalidate the
hole and its dependents.

The [tapped block](../tools/model/tapped-block.request.json) has an M10×1.5
through tap (8.5 mm tap drill) and an M6×1 blind tap 9 mm long with a 2 mm
runout in a 14 mm bore, with a 90° countersink and a 118° drill point. Tests check:

- the cut against drilling plain holes and cutting the same threads with the
  thread feature (equal to 1e-7 relative),
- removed volume against the screw-motion (Pappus) volume of the in-material
  groove, within the partial turns at the entry and exit,
- that blind grooves stay between the entry and the bore floor, within the
  nominal radius, across depth and length edits that rebuild the hole,
- that left- and right-handed threads remove equal volume, and their entry
  vertices mirror each other across the block's symmetry plane, and
- that a bore below the basic minor diameter, an overlong or zero length, an
  up-to-next limit, and a nominal size at or below the bore all fail without
  leaking handles.

Tapped holes cost about 52 ms per turn (1.15 s for this block's 22 turns);
fusing the groove into the cutter roughly doubles the boolean work of a
separate thread cut, in exchange for keeping input history. A 100-turn
M10×1.5 tap through a 150 mm block is a required scale case (12 s budget).
The MCP renders 10 tapped blocks in 14.3 s (30 s budget). Older documents
migrate to schema 113 without modeled threads; there is no ABI change.
