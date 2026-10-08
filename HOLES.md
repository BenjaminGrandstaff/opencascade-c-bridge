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
recesses and combine with the tip. Thread metadata retains its existing intent
semantics and does not create a helix.

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

The [example request](tools/model/drill-point.request.json) is available through
MCP as `drill-point`. Change `point_angle`, `diameter` or `depth` to regenerate
the hole. Increasing depth far enough to expose the point rejects the edit.

The implementation adds one cone, one containment intersection, one cutter
fusion and one cut, with native cost governed by input and cutter topology.
There is no additional global graph scan. The `drill_points` scale gate checks
500 regenerations, analytical volume and validity, and zero retained handles
against a 10-second budget.
