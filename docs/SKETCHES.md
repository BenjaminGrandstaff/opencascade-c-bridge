# Sketch geometry, constraints and profile edits

Schema **72** adds the initial advanced sketch capabilities: ellipses, signed line
angles, circular radius/diameter dimensions, point symmetry, point-on-curve
constraints, and saved trim/extend/offset operations. Native editing and curve
projection use C ABI **47**. Older documents migrate with empty `ellipses` and
`profile_operations`, preserving their previous geometry and constraints.

A sketch contains named points, lines, circles, arcs, interpolated splines and
ellipses in a typed 3D plane. `profile` chooses the ordered boundary used by
`SketchFace`, `SketchWire` or `SketchOpenWire`; omitted entities are construction
geometry. A sole ellipse, circle or closed spline can supply a closed profile.
Mixed geometry needs an explicit profile. Length expressions normalize to mm;
angle expressions are dimensionless **radians**.

## Added constraints

| JSON variant | Meaning |
|---|---|
| `angle: {first, second, value}` | Signed angle from the first directed line to the second, in `[-pi, pi]` radians; line direction follows its start/end IDs. |
| `radius: {curve, value}` | Positive radius of a circle or arc. |
| `diameter: {curve, value}` | Positive diameter of the circle supporting a circle or arc. |
| `symmetric: {first, second, axis}` | Two points reflected across the infinite named line. |
| `point_on_curve: {point, curve}` | Point on a supporting line, full circle, bounded arc, full ellipse, or bounded/periodic interpolated spline. |

These share the sparse solver and its native residual diagnostics. Arc
membership also enforces the directed arc span. Ellipse membership uses its
implicit equation, scaled in mm; spline membership projects onto the native
interpolated curve, including specified endpoint tangency, with two coordinate
residuals. Splines are never replaced by a sampled polyline for this constraint.
Analytic constraints touch at most eight coordinates; spline constraints also
touch their defining/tangent-neighbor points. Local finite differences and
normal-matrix fill depend on those references. A small relative damped step
allows underconstrained curves to reach their targets; free degrees and
redundancy are calculated from the undamped Jacobian.

An ellipse is `{"id":"e","center":"c","major":"a","minor":"b"}`.
Its defining points must be distinct; radii must be positive with major >=
minor. An implicit solver equation keeps its axis vectors perpendicular.
Axis lengths can be driven with point-distance constraints or fixed parameter
coordinates. Radius/diameter constraints describe circular geometry, rather
than an ellipse's varying curvature radius. Tangency retains the existing
shared-endpoint/rim convention; spline endpoint tangency is imposed during
native construction and shown as constructed, rather than a measured zero.

## Equal circular radii (schema 92)

`equal_radius: {first, second}` relates two distinct named circles or arcs.
For arcs it compares supporting-circle radii; arc span and orientation do not
change the equality. Lines, ellipses, splines, missing IDs and self-relations
are rejected. The constraint has no target value: drive one radius with a
parameter or other constraints, then solve the other. Conflicting fixed radii
produce an unsolved sketch with a real millimetre residual. It does not equate
arc lengths, spans or centres.

The solver contributes one radius-difference equation, touching at most four
points/eight coordinates. Each lookup uses the existing entity index; sparse
finite differences and normal-matrix fill stay local to those references.
Arc endpoints retain their implicit equal-distance equation. Diagnostics use
the same residual, rather than reporting a constructed equality. The shared
viewer shows `=R`, highlights both curves, anchors the label at their centres
and links parameters referenced by their centre/rim source expressions.

The [equal-radius plate example](../tools/model/equal-radius-plate.request.json)
uses fixed parameter-driven geometry for the left circle and a solved right
rim with horizontal/equal-radius constraints. Two saved sketch definitions
select the left and right boundaries independently; their shared constraint
definition is copied explicitly in JSON. A rectangular outer sketch and both
hole wires feed `planar_region`, then `extrude` creates the solid. The sketches
keep their points, equations and identities, and remain inspectable beside
the solid.

Changing `hole_radius` rebuilds both hole sketches, the holed face and the
extrusion while reusing the outer sketch. For the default 60 × 30 × 8 mm plate
with two 3 mm radius holes, volume is
`(60 × 30 − 2 × π × 3²) × 8 = 13947.610658 mm³`. Native validity, both bore
radii, analytical volume, rejected-edit retention, live edits and STEP/STL
export are verified. A 1,000-pair solve and residual-diagnostic gate passes in
0.009 s (10 s budget); 20 solids plus 60 source sketch scenes and matched-radius
annotations pass through MCP in 0.316 s (10 s budget).

Current authoring examples use schema 119. Older model documents migrate
without adding constraints or changing their entities. Current native ABI is 60.

## Saved profile operations

`SketchDefinition.profile_operations` derives the generated boundary **after**
solving source constraints. The source point/entity identities remain intact.
Constraints and dimensions refer to that source geometry, not silently moved
trim/extend endpoints. The derived profile is what subsequent solid features
use and what the viewer draws in purple alongside faded source curves.

```json
"profile_operations": [
  {"trim": {
    "entity": "circle",
    "first": {"literal": {"value": 0, "dimension": "scalar", "unit": null}},
    "last": {"literal": {"value": 0.25, "dimension": "scalar", "unit": null}}
  }},
  {"extend": {
    "entity": "circle",
    "start": {"literal": {"value": 0, "dimension": "length", "unit": "millimeter"}},
    "end": {"parameter": "extension"}
  }},
  {"offset": {"distance": {"parameter": "offset"}, "join": "arc"}}
]
```

- **Trim** retains `first..last` of the entity's oriented **native parameter
  interval**, with `0 <= first < last <= 1`. Fractions are not arc-length
  fractions. Circle/ellipse zero follows its rim/major-axis direction. Lines,
  circular/elliptic curves and interpolated splines keep their native geometry.
- **Extend** takes nonnegative lengths, at least one positive. Lines and conics
  continue naturally by arc length. Splines use OCCT tangent-continuous
  extension to targets that far along the original endpoint tangents; that
  distance is not the resulting spline's arc length. Trim a full closed curve
  before extending it; periodic overlaps and degenerate tangents fail.
- **Offset** edits the entire assembled planar profile. Positive grows a closed
  profile; for an open profile it selects the right side of traversal relative
  to the sketch's plane normal. Negative selects the opposite direction.
  `join` is `arc` (round corners, default) or `intersection` (extended edges).
  Single open curves use native offset curves; multi-edge/closed profiles use
  native joining/offset algorithms. A split or disconnected result fails
  explicitly rather than selecting a region arbitrarily.

Entity trim/extend operations must precede corner edits and whole-profile offsets; multiple
successive offsets are allowed. Entities must belong to the selected profile.
At most 1,024 operations are accepted. Edited segments must connect; closed
features require closure, and `SketchOpenWire` requires distinct ends. Native
precision and validity checks still apply, including downstream face/solid
validation. Each operation allocates a new handle and releases superseded
intermediate handles; the source definition and input shapes stay available.

## AI tools and viewer

Generated `sketch`, `feature`, `model`, `edit`, and `view` schemas include these
fields. Use guarded feature edits to change an embedded sketch or add a sketch
feature; parameter edits regenerate its equations and profile operations.
`occt_get_example` now includes **sketch-advanced**, a diagnostic view request
with an ellipse extrusion, construction dimensions/constraints and an edited
circular path. `occt_visualize_model` shows it without accepting failed geometry.

```sh
LD_LIBRARY_PATH="$PWD/build" cargo run \
  --manifest-path rust/occt-parametric/Cargo.toml --bin occt-model -- \
  --visualize tools/model/sketch-advanced.request.json /tmp/advanced-sketch
LD_LIBRARY_PATH="$PWD/build" cargo run \
  --manifest-path rust/occt-parametric/Cargo.toml --bin occt-view -- \
  /tmp/advanced-sketch/model.json --serve --output body
```

The combined studio exposes linked controls, radial/full-diameter labels,
angular arcs and radian residuals, symmetry/on-curve markers, and profile
operation controls. A native profile failure remains visible with its source
geometry; failed edits remain diagnostic and cannot replace accepted geometry.
Interactive point drawing/dragging and adding constraints through canvas tools
remain separate viewer work; these capabilities are defined in the model/API.

## Verification and scale

Tests cover solved/conflicting constraints, units and references, native spline
projection, exact rotated-plane ellipse areas, trim/extend geometry, signed
open offsets, closed corner joins, source preservation, schema migration,
geometry acceptance, and handle cleanup. C error-path tests exercise each added
entry point. Viewer/API tests inspect the same constraint and derived-profile
data used for native regeneration.

`cargo bench --manifest-path rust/occt-parametric/Cargo.toml --bench sketch_advanced`
(with the native library environment above) enforces three 10-second budgets:
1,000 coupled advanced components, 1,000 point-on-spline components, and 1,000
trim/extend/offset profile regenerations. Measured release times were 0.063 s,
0.115 s and 0.088 s. The benchmark is included in `tools/bench/run.sh`.

Separate saved closed sketch boundaries can now form a single holed face using
schema-79 `planar_region`. That face feeds extrusion and revolution while the
source sketches keep their own constraints and identities. See
[hollow profiles](PLANAR_REGIONS.md).


Schema 93 adds semantic attachment to a selected planar solid face for all
three sketch output kinds. The face centre/normal define the plane, and source
edits rebuild dependent sketches and solids. See
[face-attached sketches](FACE_SKETCHES.md) for frame rules and the pocket example.


Schema 94 adds named linked projections of source lines and conics into the
sketch frame. Fixed imported points/curves can participate in ordinary
constraints or profile boundaries. See [linked projections](SKETCH_PROJECTIONS.md).

## Midpoint and concentric relations (schema 95)

`midpoint: {point, line}` puts a named point at the arithmetic midpoint of a
named line segment. Its two equations compare X and Y in millimetres and
reference only the point and line endpoints. Endpoints may move; the point
follows their solved coordinates. All references are checked before solving.

`concentric: {first, second}` gives two distinct circles, arcs, or ellipses a
shared centre. Its two millimetre equations reference only their centre
points. It preserves separate radii, ellipse axes and arc spans. Missing
curves, lines, splines and self-relations are rejected. Arc and ellipse
geometry still obeys its existing implicit equations.

Both relations use the sparse solver, rank diagnostics and measured residuals.
Conflicting fixed coordinates leave a sketch unsolved. The viewer labels them
`MID` and `CONC`, highlights their targets, shows solved point/midpoint or centre
anchors, and links parameters in the relevant coordinate expressions. These
are constraint annotations with actual millimetre residuals.

The [projected pocket](../tools/model/projected-pocket.request.json) now uses
midpoint to place its top edge relative to the projected source edge. The
[concentric bushing](../tools/model/concentric-bushing.request.json) solves an
initially offset inner centre and bore rim against a fixed parameter-driven
outer circle. Two explicit copies of the sketch select the outer and inner
wires; `planar_region` and `extrude` make the annular body. Its default volume
is `π × (12² − 5²) × 8 = 2990.796206 mm³`. Centre or bore edits rebuild both
wires and the solid; height edits reuse both wires and the region. Native
validity, bore radius, moved bounds and analytical volume are tested.

Older documents migrate to schema 95 without adding relations. Native ABI
remains 52. Independent full-circle tangencies are described below; arbitrary curve tangency remains future work.

## Signed point-to-line distance (schema 96)

`point_line_distance: {point, line, value}` measures perpendicular distance to
the infinite supporting line of a named segment. `value` is a length expression:
positive is left of the segment's start-to-end direction, negative is right,
and zero puts the point on the line. Reversing the endpoints reverses the sign.
The perpendicular foot may lie beyond the segment endpoints. This relation
constrains one degree of freedom; other relations can set position along the
line. It also works when the line endpoints move.

For line endpoints A and B, unit direction U = (B − A)/|B − A| and point P,
the millimetre residual is `U.x × (P.y − A.y) − U.y × (P.x − A.x) − value`.
The sparse Jacobian touches only P, A and B. Rank and residual diagnostics
use this actual equation. Wrong units, unknown point/line references and
collapsed line coordinates are rejected; conflicting fixed geometry remains
unsolved. Negative targets and zero targets are valid.

The viewer shows a driving `⊥` dimension with a signed millimetre label,
anchors from the perpendicular foot to the solved point, highlights the point
and reference line, and links the target expression's parameters. The
[projected pocket](../tools/model/projected-pocket.request.json) uses `mid`
and the projected `front-edge` with `value: {negate: {parameter: margin}}`.
For this example the edge runs left to right, so the negative value puts the
pocket below it. Increasing margin from 4 to 6 mm moves the profile from
family-space Y=24..36 to Y=22..34. Incremental regeneration reuses the source
block and rebuilds the profile, cut tool and body. Native volume stays
46800 mm³; changing block depth still moves the projected reference and cut.

Model documents migrate to schema 96; native ABI remains 52. This is an
oriented supporting-line dimension, rather than shortest distance to a bounded
segment. Tangency to general curves remains a sketch relation gap.

## Independent line and circle tangency (schema 97)

`line_circle_tangent: {line, circle, side}` constrains a full circle tangent to
the infinite supporting line of a named segment. `side` is required: `left`
or `right` of the line's start-to-end direction. The contact may be beyond
the segment endpoints. No shared point or circle-rim identity is required.
Reversing the line direction reverses the meaning of the side.

`circle_circle_tangent: {first, second, mode}` constrains two distinct full
circles. Required `mode` is `external` or `internal`. External tangency makes
centre distance equal the sum of radii. Internal tangency makes it equal the
first radius minus the second radius: the first circle contains the second
and must have a strictly larger radius. Reorder the circle references when
the intended containing circle is second.

Each relation contributes one measured millimetre residual, touching at most
four points/eight coordinates. Line tangency uses signed centre-to-line
distance minus the chosen signed radius. Circle tangency uses centre distance
minus the sum or ordered difference of radii. Sparse finite differences,
rank diagnostics and underconstrained solving use these equations. Conflicting
fixed geometry remains unsolved, with a real residual. Missing references,
wrong entity types and self-circle relations are rejected. Initialize with
positive radii, a nonzero line and distinct circle centres; internal tangency
also requires the first circle to be larger during solving. Coincident equal
circles are not treated as an internal tangent pair.

The viewer labels these `T LEFT`, `T RIGHT`, `T EXT` and `T INT`. Calculated
contact anchors coincide when the relation is satisfied and separate for
conflicting geometry. Targets highlight both entities; controls link source
coordinate expressions, and radius dimensions retain their driving controls.
These annotations carry measured millimetre residuals, rather than constructed
status or the dimensionless angular residual of endpoint tangency.

The [tangent boss](../tools/model/tangent-boss.request.json) has a fixed reference
circle of radius R, a horizontal reference line at Y=−R, and a solved boss
circle of radius r. Left-side line tangency and external circle tangency place
the boss at `(sqrt(4 × R × r), r − R)` on the positive-X solution branch. The
source centre and rim coordinates are initial guesses; they do not contain
this position formula. A radius equation and horizontal radius axis complete
the four equations over four free coordinates. `sketch_face` and `extrude`
produce the boss alone; reference geometry remains visible in the sketch.
For R=5, r=3 and height=8 mm, native volume is `π × 3² × 8 = 226.194671 mm³`.
Changing either radius rebuilds the profile and body; height edits reuse the
profile. Native validity, moved bounds, analytical volume and rejected-edit
retention are tested, along with external and internal contact markers.

Existing `tangent: {first, second, point}` retains its shared endpoint/rim
semantics, including constructed spline endpoint tangency. The independent
relations require full circles; arcs, ellipses and splines retain that older
relation. Schema 98 extends these relations to directed arcs, as described below.
Line–ellipse tangency follows in schema 102 below; other ellipse and general
curve contacts remain future work. Native ABI at this increment remains 52.

## Directed arc tangency (schema 98)

`line_circle_tangent` now accepts a circle or arc in its `circle` role;
`circle_circle_tangent` accepts circles or arcs in either circular role. The
saved field names, required side/mode choices and full-circle behavior remain
the same. Arc contacts are checked against their start/end points and
`clockwise` direction, including span endpoints and angle wraparound.

The supporting-circle tangency residual is accompanied by one span residual
for each arc. A contact inside its directed span contributes zero. An excluded
contact contributes the shortest angular distance to a span endpoint times
the arc radius, in millimetres. Line/arc contact points follow the selected
line side. External circular contacts face each other; ordered internal
contacts point from the containing centre toward the contained centre.
Both arcs must contain their respective contacts. Supporting circles that
touch outside an arc's span do not satisfy the relation.

The sparse Jacobian includes the arc's end point as well as its centre and
start point, allowing free endpoints to move their span into contact. Each
line/arc relation touches at most five points/ten coordinates; an arc/arc
relation touches at most six points/twelve coordinates. Existing implicit arc
radius equality still applies. Inactive span rows are zero and do not reduce
free degrees; the existing redundancy count can include these zero rows.
The viewer keeps calculated supporting-circle contact markers and reports
actual span-inclusive residuals. Markers can coincide on supporting circles
while the relation is failed because an arc excludes the contact.

The [arc tangent boss](../tools/model/arc-tangent-boss.request.json) uses a
clockwise reference semicircle and a solved 270-degree boss arc. Its line
and circular tangencies position the boss without shared point identities.
The boss arc and its closing chord form an exact native face, then extrusion
makes a circular-segment solid. The selected reference arc and boss arc both
contain their circular contact; the boss arc also contains its line contact.
At reference radius 5 mm, boss radius 3 mm and height 8 mm, volume is
`(3π/4 + 1/2) × 3² × 8 = 205.646003 mm³`. Radius/reference edits rebuild the
profile and body; height edits reuse the profile. Native validity, exact arc
radius, bounds, analytical volume and failed-span handle cleanup are tested.

Model documents migrate to schema 98 and native ABI remains 52. These remain
supporting-line/circular tangencies; ellipse and general spline contacts are
future work. Named `tangent` endpoint relations keep their existing semantics.

## Explicit B-spline bases (schema 99 / ABI 53)

A spline entry may provide `basis`. Without it, `points` remain named
interpolation points. With it, `points` are named **control poles**, and the
supplied basis goes directly to `Geom_BSplineCurve`:

```json
{
  "id": "curve",
  "points": ["a", "b", "c"],
  "basis": {
    "degree": 2,
    "knots": [0, 1],
    "multiplicities": [3, 3],
    "weights": [
      {"literal": {"value": 1, "dimension": "scalar", "unit": null}},
      {"parameter": "middle_weight"},
      {"literal": {"value": 1, "dimension": "scalar", "unit": null}}
    ]
  }
}
```

The current basis is clamped and non-periodic. Degree is 1..25, there must be
at least degree+1 poles, and distinct knots are finite and strictly increasing.
Knot and multiplicity arrays have matching lengths. Endpoint multiplicities
are degree+1, interior multiplicities are 1..degree, and the sum is
`pole_count + degree + 1`. At most 10,000 poles and 10,002 distinct knots are
accepted. Interior knots are supported and retain their specified continuity.
Repeated control poles are permitted; a wholly collapsed control polygon is
rejected. Repeat the first named pole at the end for a closed clamped curve;
this joins the ends without promising a smooth periodic seam.

Weights may be omitted/empty for all ones, or supplied as positive finite
**dimensionless** expressions, one per pole. Their parameter dependencies
enter incremental regeneration. `SketchSolution.spline_weights` stores their
evaluated values for previews and profile construction. Knot values, degree
and multiplicities are explicit constants editable in the feature definition.
Periodic bases are added in schema 100 below; non-clamped bases follow in schema 101.

Point-on-curve constraints project onto the exact native B-spline, including
rational weights, rather than onto its control polygon. Control poles retain
the usual point constraints and solver variables. A named endpoint `tangent`
relation with a line or arc uses the adjacent control-pole direction as a
measured angular residual; it moves free poles and preserves the supplied
basis. Adjacent endpoint poles must define a nonzero direction for this
relation. Existing interpolated splines still use constructed endpoint
tangency. Independent spline contacts follow in schemas 103–106;
ellipse-circle/arc contacts follow in schema 107. Ellipse-to-ellipse tangency
follows in schema 108.

The viewer exposes the basis, pole IDs and evaluated weights, draws a dashed
control polygon, and links a `B-spline degree …` annotation to coordinate and
weight controls. Sampled display curves are previews of the native curve;
profiles and solids use the supplied B-spline itself.

The [rational spline cap](../tools/model/rational-spline-cap.request.json) uses
poles `(R,0)`, `(R,R)`, `(0,R)`, degree 2, endpoint multiplicities 3 and weights
`[1, sqrt(1/2), 1]`. This is a quarter-circle arc, closed with a chord and
extruded. At R=10 mm and height=8 mm, volume is
`(π/4 − 1/2) × 10² × 8 = 228.318531 mm³`. Setting the middle weight to 1 makes
a polynomial quadratic cap with volume `10²/3 × 8 = 266.666667 mm³`.
Weight edits rebuild profile and body; height edits reuse the profile. Native
weights, knot spans, point membership, endpoint tangency, validity, volume,
failed-edit handle retention and viewer controls are tested.

The native segment ABI now accepts kind 3 with borrowed knot/multiplicity and
optional weight buffers. Existing line, arc and interpolation kinds keep
their behavior. ABI 53 changes the segment struct layout, so rebuild all
native and Rust consumers together; model documents migrate to schema 99
without changing existing interpolated spline definitions.

## Periodic B-spline bases (schema 100 / ABI 54)

Set `basis.periodic: true` to build a periodic native B-spline from its supplied
control poles, knots, multiplicities and positive weights. The field defaults
to false, preserving clamped schema-99 curves and older interpolation defaults.
List the periodic control poles directly; the engine does not append a duplicate
seam pole. Repeated poles remain legal, but adding the first again adds a real
control pole and changes the basis rather than simply closing it.

A periodic basis requires first and last knot multiplicities to match, with
all multiplicities between 1 and degree. Its pole-count equation is
`pole_count = sum(multiplicities) − last_multiplicity`. Degree and size limits,
finite increasing knots, known poles and positive dimensionless weight rules
are unchanged. Continuity at a knot, including the seam, follows degree minus
multiplicity; uniform cubic multiplicity-one bases have C² continuity. Higher
multiplicities may deliberately reduce continuity.

Periodic curves are standalone closed profiles. They cannot be concatenated
with lines/arcs or used as an untrimmed open profile, and named endpoint
`tangent` constraints are rejected because the loop has no free ends. Ordinary
point constraints on control poles and point-on-curve constraints still work;
point membership projects onto the native periodic curve, not its polygon.

The curve's actual seam generally lies between poles. Native wire construction
therefore verifies kernel closure instead of comparing the first and last
control-pole coordinates. The renderer closes the dashed control polygon,
charges its extra vertex to the shared display budget, and marks the basis
annotation as periodic. Source pole IDs remain distinct from the curve seam.

The [periodic spline pad](../tools/model/periodic-spline-pad.request.json) uses
four poles at `(±a, ±b)`, degree 3, knots `[0,1,2,3,4]` and multiplicities all 1.
Unit weights produce a rounded rectangle with area `122/45 × a × b`, and bounds
`±11/12 × a` in X and `±11/12 × b` in Y. For a=10, b=8 and height=8 mm, native
volume is `1735.111111 mm³`. A positive `corner_weight` parameter changes one
pole's rational weight while maintaining seam continuity. Size/weight edits
rebuild the profile and solid; height edits reuse the profile. Native validity,
analytical area/volume, exact bounds, shared seam position and tangent, weight
edits, failed-edit retention and closed viewer polygons are tested.

ABI 54 interprets the existing explicit-segment flag bit 2 as periodic. Rebuild
the native bridge and Rust consumers together so older libraries cannot ignore
the new semantics. Documents migrate to schema 100; omitted periodic flags
remain false. Non-clamped bases follow below; independent general-curve
contact remains future work.


## Non-clamped B-spline bases (schema 101 / ABI 55)

Set `basis.non_clamped: true` for an explicit non-periodic basis whose outer
knot multiplicities need not equal degree+1. Supply `endpoints` beside `basis`
to name the sketch points at the actual ends of the native active curve domain:

```json
{
  "id": "curve",
  "points": ["a", "b", "c"],
  "endpoints": {"start": "start", "end": "end"},
  "basis": {
    "non_clamped": true,
    "degree": 2,
    "knots": [0, 1, 2, 3, 4, 5],
    "multiplicities": [1, 1, 1, 1, 1, 1]
  }
}
```

The named endpoints must be existing sketch points distinct from all control
pole IDs. Use free points to let the solver place them; fixed points impose
additional geometry requirements and can conflict. Four implicit millimetre
residuals tie their X/Y coordinates to the native endpoints. A closed
non-periodic spline may name the same point for both ends. Its poles must
produce a native closed curve after solving. Non-clamped and periodic modes
are mutually exclusive; other spline modes must omit `endpoints`.

Degree remains 1..25, with at most 10,000 poles and 10,002 distinct knots.
Knots must be finite and strictly increasing. Outer multiplicities are
1..degree+1, interior multiplicities 1..degree, and their sum is
pole count+degree+1. Weights remain positive finite dimensionless expressions,
or omitted for all ones. OCCT determines the active interval; it can lie
inside the supplied knot range. Neither outer knots nor first/last poles are
assumed to be the geometric endpoints.

The [non-clamped cap](../tools/model/non-clamped-spline-cap.request.json)
uses poles `(0,0)`, `(span/2,rise)`, `(span,0)` with the basis above. At unit
weights, the active interval is `[2,3]`, its ends are `(span/4,rise/2)` and
`(3 span/4,rise/2)`, and its maximum Y is `3 rise/4`. A chord joining those
endpoints encloses area `span × rise / 12`. At default dimensions its valid
extruded solid has volume `133.333333 mm³`. Changing span, rise or weight
rebuilds the profile and solid; changing only height reuses the profile.

Shared-endpoint `tangent` equations use the native first derivative for this
mode, including rational weights and interior knot structure. Their sparse
Jacobian depends on all defining poles; endpoint equations also include the
named endpoint coordinates. Zero tangents fail tangent constraints. Point
membership continues to project onto the native curve. Independent tangency
between curves without a shared named endpoint remains future work.

Mixed native wires check actual segment vertices, so these curves can connect
to lines and arcs without confusing poles with endpoints. Accepted profiles
still enforce closure. The viewer preserves dashed control polygons, adds
endpoint IDs to curve selection, and exposes the endpoint declaration. A
“Native spline endpoints” constraint annotation shows measured millimetre
residuals and passed/failed status. Unsolved previews retain the native curve
so fixed endpoint conflicts can be inspected.

ABI 55 assigns explicit-segment flag bit 3 (8) to non-clamped bases, mutually
exclusive with periodic bit 2 (4), and adds `occt_bridge_curve_end_data` /
`Session::curve_end_data`. The query accepts an edge or one-edge wire and
returns active-domain start/end positions and unnormalized first derivatives
in located world coordinates, following edge orientation. It stores no new
shape handles. Rebuild the native bridge and Rust consumers together. Older
model documents migrate to schema 101 with `non_clamped` false and no endpoint
declaration, preserving existing interpolation and clamped/periodic behavior.


## Line–ellipse tangency (schema 102)

Use `line_ellipse_tangent` to constrain a full ellipse tangent to a line without
requiring a shared point ID:

```json
{
  "line_ellipse_tangent": {
    "line": "baseline",
    "ellipse": "ellipse",
    "side": "left"
  }
}
```

The line is oriented from its start to end. `left` places the ellipse centre
on its left side; `right` selects the opposite side. The contact is on the
infinite supporting line and may lie outside the finite line segment. The
constraint requires a known line and a full ellipse. Zero-length lines and
zero axes fail. Existing ellipse axis orthogonality equations and native
major/minor axis rules still apply.

Let `n` be the line's unit left normal, `u` the ellipse's major-axis vector,
and `v` its perpendicular minor-axis vector. Its exact support in that
normal direction is `h = hypot(u·n, v·n)`. The signed millimetre residual is
`n·(centre − line_start) − s h`, with `s=+1` for left and `s=−1` for right.
This equation accounts for ellipse rotation and reduces to the existing
circle/line distance equation when the two axes are equal.

The ellipse contact is
`centre − s (u (u·n) + v (v·n)) / h`. For a rotated ellipse this point generally
has a different coordinate along the line from the centre's projection.
The solver and diagnostic viewer share this analytic calculation. The sparse
Jacobian includes both line endpoints and the ellipse's centre, major and
minor defining points. This tangency is solved and measured; it is not imposed
by curve construction.

The viewer displays the contact and its projection onto the line as two
anchors. They coincide when satisfied and separate in a conflicting sketch.
The annotation reports millimetre residuals with passed/failed status and
links coordinate controls plus distance/angle parameters directly driving
its defining points. Contact queries index line and ellipse IDs once, keeping
batch diagnostics linear in the number of entities and constraints.

The [ellipse tangent pad](../tools/model/ellipse-tangent-pad.request.json)
solves all six free coordinates of a rotated ellipse above a horizontal
baseline. Its `semi_major`, `semi_minor` and `tilt` (radians) parameters control
axes and angle. The default a=5 mm, b=3 mm, tilt=π/6 and height=8 mm yield
centre Y=√13 mm, contact X=−4√3/√13 mm, minimum Y=0 and native volume=120π mm³.
Axis or tilt edits rebuild the profile and solid; height-only edits reuse the
profile. Fixed-geometry conflicts, signed sides, rotated lines, the circular
limit, native validity/bounds/volume and failed-edit retention are tested.

Older documents migrate to schema 102 without adding constraints. Native ABI
remains 55. Independent line–spline tangency follows below; curve–curve contact remains
future work.


## Line-spline tangency (schema 103 / ABI 56)

Use a seeded sketch point to locate an independent tangent contact between a
line and any native spline mode:

```json
{
  "line_spline_tangent": {
    "line": "tangent-line",
    "spline": "curve",
    "point": "contact"
  }
}
```

The contact point must exist but need not be a shared endpoint or control pole.
A free point slides during solving; its initial coordinates select the local
contact branch. A fixed point imposes a specific location and may conflict.
The line is its infinite supporting line; the spline is its bounded native
active interval. Endpoints can be contacts if their one-sided tangents align.
This is a local nonlinear solve, not an exhaustive search for every contact.

Four residuals enforce contact membership in X/Y (millimetres), signed
point-to-line distance (millimetres), and normalized tangent cross product
(dimensionless). Native projection considers both interval endpoints and all
projection candidates. Its first derivative supplies the tangent direction,
including rational weights and the actual active domain. Direction is scaled
before normalization so knot parameter units do not change the alignment
condition. Zero tangents and internal C0 B-spline knots, including C0 periodic
seams, fail instead of choosing an ambiguous tangent. Smooth spans elsewhere
on a globally C0 spline remain usable.

The sparse Jacobian includes both line endpoints, the contact point and all
control/interpolation points. Interpolated splines also include any line/arc
neighbours already imposing endpoint tangency. This new relation never changes
how the curve is constructed and is reported as a measured constraint.
Existing shared-endpoint `tangent` and point-membership behavior are preserved.
Explicit rational, non-clamped, periodic and interpolated modes are tested,
as are free control-pole movement and contacts excluded by the bounded domain.

Viewer anchors show the seeded point, nearest native spline contact and the seeded point’s
supporting-line projection. They coincide when satisfied and separate during
ordinary fixed-geometry conflicts. The detail reports `position_residual_mm`
and `tangent_sine`, with residual units
`["mm", "mm", "mm", "dimensionless"]`; the overall residual summary is marked
`mixed`. Coordinate and rational-weight controls are linked to the relation.

The [spline tangent cap](../tools/model/spline-tangent-cap.request.json)
uses a degree-2 clamped rational curve with poles `(-span/2,0)`, `(0,rise)` and
`(span/2,0)`, and weights `[1,w,1]`. The horizontal tangent touches at
`(0, rise × w / (1+w))`. At unit weights, the chord-bounded profile has area
`span × rise / 3`, giving default volume `533.333333 mm³` at height=8 mm.
Weight and size edits regenerate the profile and solid while moving the
contact; height-only edits reuse the profile. Native validity, analytical
volume, active bounds, incremental reuse and failed-edit retention are tested.

ABI 56 adds `occt_bridge_curve_closest_data` / `Session::curve_closest_data`.
The query accepts an edge or one-edge wire, returning a bounded closest point
and unnormalized oriented derivative in located world coordinates. Equal
nearest-distance ties choose the first candidate deterministically. It stores
no shape handles and rejects multi-edge inputs, zero tangents and C0 knot
contacts. Rebuild the native bridge and Rust consumers together. Older model
documents migrate to schema 103 without adding relations. Spline–circle
contact follows below; spline–ellipse and spline–spline contact remain future work.


## Spline-circle tangency (schema 104)

Use `spline_circle_tangent` to solve a contact between a bounded native spline
and a full circle or directed circular arc, without shared endpoint IDs:

```json
{
  "spline_circle_tangent": {
    "spline": "curve",
    "circle": "boss",
    "point": "contact"
  }
}
```

The point must exist. A free point slides during solving; fixed points impose
an exact location and can conflict. Its initial coordinates and the initial
circular geometry select the local branch. This relation has no external or
internal containment mode for the spline. It solves a local tangent contact
rather than searching for all contacts or proving whole-profile containment.
The circle role accepts full circles and arcs. Unknown roles, zero radius and
a contact at the circular centre fail.

Four residuals enforce bounded native spline membership in X/Y (millimetres),
`distance(contact, centre) − radius` (millimetres), and tangent alignment
(dimensionless). The spline derivative comes from the native bounded closest
point query. The circular tangent is perpendicular to the contact's radial
direction. Arc roles add a fifth millimetre residual measuring the distance
along the supporting circle to the nearest allowed directed endpoint when
the contact lies outside the span. Clockwise, counterclockwise, wrapped spans
and included endpoints use the same domain rules as existing circular tangency.
A zero tangent or C0 spline knot still fails the native contact query.

The sparse Jacobian includes the contact point, all spline poles/interpolation
points, existing endpoint tangent neighbours, and the circle's centre/rim or
arc's centre/start/end. All four spline modes are supported: clamped rational,
non-clamped, periodic and interpolated. The new relation is solved and measured,
including when an interpolated spline is used; it does not impose the contact
by curve construction. Free poles can move to satisfy a fixed circular target.

Viewer anchors show the solved seeded point, bounded native spline point and
radial projection onto the supporting circle. Details report
`position_residual_mm`, `tangent_sine` and `arc_span_residual_mm` (null for full
circles). Residual units are `["mm", "mm", "mm", "dimensionless"]`, with a final
`"mm"` entry for arc domains. The overall residual summary is `mixed`. A
contact may have zero position/tangent residuals and still fail because its
arc-domain residual is positive. Radius/diameter, direct distance/angle and
coordinate parameters, plus rational weights, are linked through indexed
references to the defining points.

The [spline tangent boss](../tools/model/spline-tangent-boss.request.json)
places a circular boss above the rational quadratic spline from the previous
example. At weight w, the contact is `(0, rise × w / (1+w))`, and the boss's
centre Y is contact Y plus `boss_radius`. With default rise=10 mm, w=1,
radius=3 mm and height=8 mm, its minimum Y is 5 mm and native volume is
`72π = 226.194671 mm³`. Weight/rise edits move the boss; radius edits change
its size while retaining contact. These edits rebuild profile and solid;
height-only edits reuse the profile. Native validity, volume/bounds, incremental
reuse and failed-edit retention are tested.

`SketchDefinition::spline_circle_tangent_contacts` exposes the measured
`SketchSplineCircleContact` records indexed by constraint number. Older
documents migrate to schema 104 without adding constraints. The implementation
uses the existing ABI 56 closest-point/derivative query; native ABI is unchanged.
Spline–spline tangency follows below; independent spline–ellipse tangency remains future work.


## Spline-spline tangency (schema 105)

Use a named contact point to constrain two distinct bounded native splines:

```json
{
  "spline_spline_tangent": {
    "first": "curve",
    "second": "upper-curve",
    "point": "contact"
  }
}
```

Both entity roles must name splines and must have different IDs. The point
must exist but need not be a shared endpoint or pole. Free points slide;
fixed points specify a location and may conflict. Initial point and pole
positions select the local contact branch. This is a local nonlinear solve;
coincident curves may leave the contact underconstrained. The relation does
not join wires or change their endpoint IDs or profile connectivity rules.

Five residuals enforce X/Y membership on each bounded native curve
(millimetres) and normalized tangent cross product (dimensionless). Tangency
is unoriented: parallel and opposite derivatives satisfy it. Both curves use
the native closest-point/derivative query, including interval endpoints,
rational weights and actual active domains. Zero tangents and C0 knot
contacts retain the native query's rejection policy. A shared position with
nonparallel derivatives fails rather than being mistaken for tangency.

The sparse Jacobian includes the contact point, both pole/interpolation sets,
and any line/arc neighbours already imposing interpolated endpoint tangency.
Clamped rational, interpolated, non-clamped and periodic splines are supported.
The relation is solved and measured rather than imposed by construction.
Viewer anchors show the solved point and both native closest points; details
separate `position_residual_mm` and `tangent_sine`, with units
`["mm", "mm", "mm", "mm", "dimensionless"]` and an overall `mixed` summary.
Coordinate, direct dimension and weight controls are linked to both curves.

The [spline tangent pair](../tools/model/spline-tangent-pair.request.json)
uses a lower rational quadratic with contact peak
`lower_rise × w / (1+w)`. A second polynomial quadratic has span `span` and
pole depth `upper_rise`, while all three poles are free. Tangency determines
its vertical position: its minimum Y equals the lower peak and its chord Y
is peak + upper_rise/2. Its chord-bounded native cap has area
`span × upper_rise / 3`, yielding default volume `426.666667 mm³` at height=8 mm.
Parameter-dependent initial guesses stay near the intended contact over the
example's size ranges; the native residuals still determine the solution.

Reference-weight and size edits rebuild the profile and solid. Height-only
edits reuse the profile. Native validity, analytical volume, active bounds,
reversed directions, endpoint contacts, rejected intersections, narrow/steep
parameter edits and failed-edit retention are tested.
`SketchDefinition::spline_spline_tangent_contacts` returns measured contacts
indexed by constraint number. Older documents migrate to schema 105 without
adding relations; native ABI remains 56. Independent spline–ellipse and other
ellipse contacts remain future work.

## Spline-ellipse tangency (schema 106)

`spline_ellipse_tangent` names a `spline`, a full `ellipse`, and a seeded
contact `point`. It measures bounded native spline and full native ellipse
closest points and derivatives through OpenCascade. All existing spline modes
use the same contact query. Tangency is unoriented: opposite derivatives are
allowed. The seed selects a local contact; this is not a global search for
all possible tangencies. Native zero derivatives and ambiguous C0 contacts
remain rejected.

The solver adds four positional residuals in millimeters (seed minus each
native contact's x/y coordinates) and a normalized tangent cross product.
The ellipse retains its intrinsic perpendicular-axis equation and positive,
ordered axis validation. Temporary axis swaps during nonlinear iterations
preserve the ellipse locus for native queries. A contact outside the spline's
bounded domain cannot pass merely by lying on its supporting extension.

The viewer shows the seed and both measured contacts, tangent sine, separate
residual units, and linked spline weight, point and ellipse dimension controls.
Fixed conflicts remain visible. `spline_ellipse_tangent_contacts` returns the
three contact positions and sine, indexed by constraint number.

The [spline tangent ellipse](../tools/model/spline-tangent-ellipse.request.json)
example extrudes the ellipse into a solid. Editing `middle_weight` changes its
placement; `semi_major`, `semi_minor`, and `tilt` change its shape and contact.
Editing `height` reuses the profile. Older documents migrate to schema 106
without adding constraints; native ABI remains 56.

## Ellipse-circle tangency (schema 107)

`ellipse_circle_tangent` names a full `ellipse`, a `circle` (or directed circular
arc), and a seeded contact `point`. OpenCascade measures the nearest native
ellipse point and derivative. A radial circle projection supplies the circular
contact and tangent. The residuals are seed minus ellipse contact x/y in
millimeters, radial circle gap in millimeters, and normalized tangent cross
product. An arc adds its directed span violation in millimeters.

Tangency permits parallel or opposite directions. External and internal
contacts use the same relation; the initial point and coordinates select a
local branch. This is a local contact solve, not a containment test or global
search for all tangencies. The ellipse keeps its intrinsic perpendicular-axis
constraint and positive, ordered axis validation. A zero circular radius or
contact at the circle center is rejected. Contacts outside an arc span fail
independently of supporting-circle tangency; endpoints are allowed.

The viewer shows the seed and both measured contacts, separate positional,
tangent and arc-span diagnostics, and linked ellipse dimension, circle radius
and coordinate controls. Fixed conflicts remain visible. The public
`ellipse_circle_tangent_contacts` method returns `SketchEllipseCircleContact`,
sharing the measured contact representation with spline-circle tangency.

The [ellipse tangent boss](../tools/model/ellipse-tangent-boss.request.json)
example extrudes the positioned circle. Editing `semi_major`, `semi_minor` and
`tilt` changes its contact and placement. `boss_radius` changes the profile and
solid; `height` reuses the profile. Older documents migrate to schema 107
without adding constraints; native ABI remains 56.

## Ellipse-ellipse tangency (schema 108)

`ellipse_ellipse_tangent` names two distinct full ellipses (`first`, `second`)
and a seeded contact `point`. OpenCascade measures the closest point and
native derivative on each ellipse. The residuals are seed minus each contact's
x/y coordinates in millimeters and normalized tangent cross product. Opposite
native derivative directions are allowed.

External and internal contacts use the same local relation; initial coordinates
and seed select a branch. A passing constraint establishes contact and tangent
alignment, not global containment or nonintersection. Separate named curves
may occupy the same geometry. Native seams and equal-axis ellipses are supported.
Each ellipse retains its intrinsic perpendicular-axis equation and positive,
ordered axis validation. Unknown curves, repeated IDs and nonfinite diagnostic
coordinates are rejected.

The viewer shows the seed and both measured contacts, four positional units
and one dimensionless tangent unit, plus links to dimensions and coordinates
of both ellipses. Fixed conflicts remain inspectable. The public
`ellipse_ellipse_tangent_contacts` method returns the three contact positions
and measured tangent sine, indexed by constraint number.

The [ellipse tangent pair](../tools/model/ellipse-tangent-pair.request.json)
example extrudes the upper ellipse. `semi_major`, `semi_minor`, and `tilt` edit
the reference; `upper_major`, `upper_minor`, and `upper_tilt` edit the solid's
profile. `height` reuses the profile. Older documents migrate to schema 108
without adding constraints; native ABI remains 56.

## Profile fillets and chamfers (schema 109 / ABI 57)

Saved `profile_operations` now support `fillet` and `chamfer` at a named
remaining profile corner:

```json
[
  {"fillet": {"point": "C", "radius": {"parameter": "fillet_radius"}}},
  {"chamfer": {
    "point": "A",
    "incoming": {"parameter": "chamfer_incoming"},
    "outgoing": {"parameter": "chamfer_outgoing"}
  }}
]
```

These are native OpenCascade planar edits on a closed profile. The two adjacent
edges must be straight lines or circular arcs. Radius and both chamfer distances
are positive lengths above native linear tolerance. Chamfer distances follow
incoming/outgoing profile traversal; distances on arcs use curvilinear abscissa.
Reversing traversal swaps which neighbor receives each distance.

The source sketch is solved first. A corner edit changes its derived boundary,
while dimensions and constraints remain attached to source points and entities.
Concave corners and rotated sketch planes are supported. The native result must
form a valid closed planar face. Too-large sizes, missing remaining vertices,
unsupported neighbors and invalid results fail without mutating the source.
Open-profile corner edits follow in schema 110 below; spline and ellipse
neighbors follow in schema 111.

Order operations as entity trims/extensions, corner edits, then offsets. Each
named corner may be edited once; offsets cannot precede a named corner edit.
A trim/extension that removes the selected corner fails when the native vertex
lookup runs. Each operation traverses the current boundary; work scales with
its edge count, and the existing 1,024-operation limit applies. Native handles
are released after a failed edit or completed regeneration.

The viewer draws the native derived boundary and fades the source geometry.
Fillet/chamfer labels anchor to their named source corners, link size controls,
and expose native errors when an edit cannot be built. The
[corner treated plate](../tools/model/corner-treated-plate.request.json)
extrudes a constrained rectangle with a fillet at C and an asymmetric chamfer
at A. Its area is `width*height - radius²*(1-π/4) - incoming*outgoing/2`.
Size edits rebuild the profile and solid; a depth-only edit reuses the profile.

The bridge adds immutable `fillet_wire_corner` and `chamfer_wire_corner`
wrappers. Rebuild the native library and Rust tools together for ABI 57.
Older documents migrate to schema 109 without adding profile operations.

## Open-profile corner edits (schema 110 / ABI 58)

The existing `fillet` and `chamfer` profile operations now also edit internal
corners on open line/arc profiles. No new operation tags are introduced.
Named end points cannot be corner selections because two adjacent edges are
required. Open endpoints and remote neighbor endpoints remain fixed.

Open fillets use OpenCascade's analytical planar edge-pair constructor. Native
result edges are reoriented to the source traversal and checked for shrinkage
within their original bounded curves. Supporting-curve extensions and consumed
neighbors are rejected. Open chamfers trim each neighbor by native arc length
and insert a straight connecting edge. Incoming/outgoing distances retain their
traversal meaning, including on circular arcs. Straight/tangent joins are
rejected for chamfers. The entire source must be a valid planar wire, and the
result must remain a connected valid open wire with one added edge per edit.

Size and operation-order validation matches closed profiles: trims/extensions,
then corner edits, then offsets. The viewer retains source constraints, anchors
edit labels at named corners and draws the native edited path. Errors remain
inspectable without converting the open profile into a closed face.

The [open corner pipe](../tools/model/open-corner-pipe.request.json) sweeps a
circular section along a path with a fillet at B and asymmetric chamfer at C.
For its right-angle corners, path length is
`width + height + end_run - 2*fillet_radius + π*fillet_radius/2
 - chamfer_incoming - chamfer_outgoing
 + sqrt(chamfer_incoming² + chamfer_outgoing²)`.
Tests compare native pipe volume to `π*radius²*path_length`. Path size edits
reuse the section; section radius edits reuse the path. Failed corner edits
preserve accepted solids. Native tests also cover reversed traversal, circular
arc neighbors, rotated planes and small/large profiles far from the origin.

Rebuild native and Rust tools together for ABI 58. Older documents migrate to
schema 110 without adding operations. Spline/ellipse neighbors follow in
schema 111 below.

## Spline and ellipse corner neighbors (schema 111 / ABI 59)

The same `fillet` and `chamfer` operations accept corners where either
neighbor is a spline or a trimmed ellipse, on open and closed profiles. No new
operation tags are introduced. Corners with only line/arc neighbors keep the
schema 109/110 algorithms and produce identical geometry.

Curved fillets use OpenCascade's iterative planar fillet solver. Only its
circular arc is kept; both neighbors are cut from their source curves at the
arc's tangent points, so remote ends and curve definitions are unchanged.
The solver chooses its own shared vertex and walks one edge while projecting
onto the other. When the two neighbors also meet at their far ends (a two-edge
loop such as a lens), only the halves nearest the selected corner are passed
to it. Both edge orders and all native solutions are tried; a solution is
accepted only if it:

- is a minor arc (under π) of the requested radius,
- meets both trimmed neighbors tangentially (within 1e-8 in cosine), and
- shrinks each neighbor within its bounded source curve, keeping its remote end.

Major arcs, wrong-tip solutions on lenses and mid-span inscribed circles are
rejected. A radius that only fits as such a circle fails. Curved chamfers trim
each neighbor by native arc length (1e-7 abscissa tolerance) and insert a
straight chord. Tangent-continuous joins are rejected for both operations.
Closed results must still form a valid planar face.

The [spline corner pad](../tools/model/spline-corner-pad.request.json) extrudes
a rectangle whose top is a three-point spline crown. It has a fillet at C
(side into spline) and an asymmetric chamfer at D (3 mm along the spline, 4 mm
down the side). Tests check:

- exactly one fillet arc of the requested radius,
- the chamfered side's exact remaining length,
- chord bounds and prism volume against face area,
- profile/body rebuilds for radius, crown and height edits,
- the narrowest/flattest parameter extremes, and
- an oversized fillet that fails without leaking handles while the accepted
  result stays usable.

Native tests cover open spline/line paths, closed spline lenses at both tips,
ellipse/line half-discs, tangent joins, and 0.01×, 1× and 1000× paths on a
distant XZ plane.

The iterative solver is slower than the analytical one. 1,000 pad profile
regenerations take 1.29 s and the MCP renders 20 annotated pads in 0.27 s
(both within 10 s budgets). Rebuild native and Rust tools together for ABI 59.
Older documents migrate to schema 111 without adding operations.

## Sketch patterns and holes (schema 112 / ABI 60)

`patterns` makes linked copies of sketch entities; `holes` cuts inner loops
from a sketch's face output. Together they draw a symmetric outline once and
pattern features such as bolt circles inside one sketch.

```json
"patterns": [
  {"mirror": {"id": "m", "entities": ["AB", "BC", "crown"], "axis": "axis",
              "on_axis": ["A", "K", "T"]}},
  {"linear": {"id": "row", "entities": ["slot"], "direction": "base",
              "spacing": {"parameter": "pitch"}, "count": 4}},
  {"circular": {"id": "bolts", "entities": ["bolt"], "center": "K", "count": 6}}
],
"holes": [["bore"], ["bolt"], ["bolts:1:bolt"], ["bolts:2:bolt"]]
```

**Copies.** Instance k of pattern `p` copies entity `e` as `p:k:e`, and each of
its points as `p:k:point` (a mirror has one instance, k = 1). Patterns expand
in order, so later patterns, the profile, holes and profile operations can name
earlier copies. Copy points are solver unknowns linked to their sources:

- **Mirror:** reflection across the infinite line `axis`. Points listed in
  `on_axis` are shared instead of copied and held on the axis. Axis endpoints
  used by the mirrored entities must be listed there.
- **Linear:** `count` instances (source included), offset by `spacing` along the
  start-to-end direction of `direction`. Negative spacing reverses it.
- **Circular:** `count` instances rotated counterclockwise about `center` by
  `angle` radians per step. Without `angle` they divide a full turn equally.
  Steps that bring an instance back onto an earlier one are rejected.

Each copied point adds two unknowns and two link equations (one for a shared
mirror point). Copies are rigid images: they carry no constraints of their own,
and free degrees are unchanged. Dimensions and constraints stay on the source;
editing a source, the reference line, the center, the spacing or the angle
moves every copy. Pattern spacing and angle parameters are sketch dependencies
for incremental rebuilds.

A reflection reverses traversal. Mirrored lines and arcs swap their ends (arcs
keep their turning flag). Mirrored splines reverse their points; explicit bases
also reverse weights and multiplicities and reflect knots. A half profile
followed by its mirrored copies in reverse order therefore closes into one
loop through the shared on-axis points.

**Holes.** Each hole is an ordered closed loop like `profile`: a single circle,
ellipse or closed spline, or connected lines, arcs and splines. Hole entities
cannot repeat across the profile or other holes. Holes need an explicit
profile and apply only to face outputs; wire outputs reject them.

The native `occt_bridge_create_face_with_holes` builds the face directly,
without booleans, and reorients inner wires as holes. It requires each hole to
be coplanar, strictly inside the outer loop, not nested, and more than 1e-7 mm
from the boundary and the other holes. Pairwise checks are bounding-box
filtered. The face must be valid, with area equal to the outer area minus the
hole areas (relative 1e-7). Circle holes use exact circles; equal-radius
ellipses would force slow numeric extrema in these checks.

**Validation and limits.** Pattern IDs must be nonempty, unique and free of
`:`. Each pattern copies 1–1000 distinct entities, counts are 2–1000, and a
sketch allows 100 patterns, 20,000 copied points, 10,000 copied entities and
1,000 holes. A pattern may not copy its own reference line or center. Copy IDs
that partly collide with existing geometry are rejected. Linear spacing must
be nonzero.

The viewer draws copies as ordinary sketch geometry. It adds one group
annotation per pattern, anchored at the axis or direction midpoint or at the
center, targeting sources and copies, and linked to spacing or angle controls.
Sketch scenes list `holes` and `patterns`.

The [mirrored flange](../tools/model/mirrored-flange.request.json) mirrors a
base, side and crown arc about a vertical axis, with a central bore and a
six-hole circular bolt pattern cut as holes. Tests check:

- the face area against
  `width*height + π*width²/8 - π*(bore² + 6*bolt²)`, and the prism volume,
- symmetric bounds and rebuilds for width, bolt and bore edits,
- the tightest in-range layout, and
- a bolt circle outside the outline, which fails natively without leaking
  handles while the accepted result stays usable.

Unit tests also cover:

- a mirrored spline half outline whose face is exactly twice the half,
- mirrored arcs and weighted splines,
- linear rows that follow spacing and source radius,
- circular patterns with equal and explicit steps,
- chained patterns,
- every validation error, and
- native hole layouts: outside, crossing, touching, overlapping, nested,
  off-plane and open.

1,000 flange profile regenerations take 2.17 s. One sketch with a
1,000-instance circular pattern solves and cuts its 1,000 holes in 2.07 s.
The MCP renders 20 annotated flanges in 0.90 s (all within 10 s budgets).
Rebuild native and Rust tools together for ABI 60. Older documents migrate to
schema 112 without patterns or holes.

## Local solver explanations

Every `SketchSolution` now includes `diagnosis`, also exposed in the annotated
viewer and MCP visualization. This is runtime output; authoring schema 114 and
native ABI 60 remain unchanged. The `sketch-diagnosis` example starts with one
free motion and duplicate width/horizontal equations. Increase `extra_width`
to reveal conflicting dimensions; change `lean` to inspect the coupled motion
of the top-left corner.

The final Jacobian supplies locally redundant constraint indices, satisfied
constraints involving fixed geometry, related conflicting equation groups,
and representative directions in its nullspace. Constraint annotations link
to their groups; group selection highlights related geometry and parameter
controls. Free-motion annotations identify affected points and relative `dx`
and `dy` components. Independent coordinate choices parameterize these coupled
directions; they cannot generally move alone. The viewer calls out solved
sketches with remaining free motion.

These are first-order explanations at the final iterate. Redundancy depends on
equation order and numerical tolerance; a conflict group is neither a minimal
conflict set nor proof of global nonlinear infeasibility. A zero derivative
with structurally free points is reported as `stationary_linearization`, not
fixed geometry. `local_rank` and `rank_agrees_with_solver` expose possible
numerical differences from the existing solver's rank calculation. Sketch
acceptance and solver iterations retain their existing behavior.

Analysis reuses the final Jacobian without additional native geometry queries.
Sparse elimination stops at 2,000,000 work units or 200,000 stored entries.
Dependency traces retain at most 64 scalar equation rows; exceeding this drops
only the trace, preserving rank and free-motion analysis. A conflicting row
with a dropped trace reports `trace_complete: false` and a partial source list.
`complete` and `limitation` identify interrupted analysis; an interrupted rank
calculation leaves `local_rank` empty. Output includes at most 128 conflict
groups, 16 motion directions, and 64 moving points per direction, with explicit
omitted counts. All independent free-coordinate choices remain available.

## Sketch acceptance policy (schema 115)

Set `"acceptance": "fully_constrained"` on a sketch to require satisfied
solver equations and zero local free degrees before constructing a face or
closed/open wire. The default, `"solved"`, permits remaining free motion;
older documents migrate to this default, and it is omitted when serialized.
All points count, including construction points and linked pattern copies.
Duplicate satisfied equations are permitted. The policy adds no native ABI
change and no extra solver pass or geometry query.

`SketchDefinition::solve` still returns the diagnostic solution. Call
`check_acceptance` to check it explicitly; native profile creation applies
that check automatically. Viewer scenes expose `acceptance`,
`acceptance_error`, and `solver.accepted` separately from `solver.solved`.
A failed policy annotation links to locally free points. Builds reject strict
failures before publishing artifacts; a rejected live edit preserves the
accepted geometry and previews the diagnostic sketch.

This policy uses the existing solver's numerical Jacobian rank. It is a local
criterion, not a proof of global rigidity or uniqueness. At singular positions,
equations can converge yet retain a first-order free direction even when
nonlinear constraints prevent finite motion. Such sketches fail this policy.
The diagnosis rank and its agreement flag remain available for inspection.

The `strict-sketch-block` example starts with a fully constrained rectangle
and a construction point constrained by two distances. Set `lift` to zero:
the two distances become tangent at the point, giving one local free degree.
The equations still solve, but strict acceptance rejects the edit. Restore
`lift` to 4 mm to recover. For AI part authoring, use this policy on each
sketch that must pass the local full-constraint check, then verify the native
shape and engineering requirements as usual.

## Exact spline and trimmed-conic projections (schema 116)

A sketch projection with `"kind": "spline"` imports a finite standard edge as
an exact rational B-spline. Native B-spline, Bézier, periodic and non-clamped
curves are supported through their actual bounded edge domains. Full ellipses
remain available through `"kind": "ellipse"`; trimmed elliptic edges and other
finite convertible conics can use `spline`. Existing analytic projection kinds
retain their type checks. Unsupported/offset curves and point-collapsed
projections fail explicitly. Native ABI remains 60.

The existing semantic selector must resolve exactly one source edge. Its located
geometry is converted to rational Bézier spans, poles are projected into the
resolved sketch frame, and adjacent spans are combined into a clamped rational
B-spline. The curve trace and edge traversal are preserved without sampling.
Each span receives a unit parameter interval, so the source parameterization
and parametric continuity order are not preserved; this does not change the
geometric trace. Shared endpoint weights are normalized per span. Native
metric queries still have their usual numerical integration tolerances.

The projected entity is named by the projection ID. Stable fixed endpoint IDs
are `id:start` and `id:end`; use these in authored lines and constraints.
Interior fixed `id:pole:N` points are runtime inspection data whose count can
change with source knot/trim edits. Their names cannot be relied on in saved
constraints. Closed imports reuse the start point at the spline's last pole
(within 1e-9 mm closure tolerance), while retaining the fixed end alias. As with
other closed splines, a closed import must be the sole entity in its profile.

Each import permits up to 10,000 returned native poles and degree 25; a sketch
permits up to 100,000 imported spline poles in total. Disconnected spans,
nonfinite coordinates/weights and exhausted bounds reject regeneration. Native
conversion uses count/fill queries; Rust copy/storage scale with returned poles.
Sources remain unchanged and query handles are released. Imported points are
fixed, so strict sketch acceptance permits fully fixed references.

The `projected-spline-pad` example projects a multi-span rational arch into a
tilted plane, closes its endpoints with a line and extrudes the face. Editing
`weight`, `width` or `height` updates both source and projected geometry.
The viewer shows the exact basis/control polygon, distinguishes the external
curve and links its projection annotation to source controls. Live failed edits
preserve accepted geometry; the pad exports valid STEP and STL.

## Connected multi-edge projection (schema 117)

Set `"kind": "chain"` on a projection to import a connected selection of
1–1000 edges as one rational B-spline. The existing semantic selector can use
unions, persistent or named references, and type/position filters to select a
whole open chain or closed boundary. Analytic and spline edge segments can be
mixed. Exact rational degree elevation brings all spans to a common degree;
projection and source-control linking then use the existing spline path.

Connectivity is checked in located source-space coordinates before projection,
so coincident projected ends cannot join unrelated 3D edges. Endpoint groups
use a 1e-7 mm tolerance, with ambiguous/nontransitive matches rejected. Each
node must have degree two, except an open chain's two terminal nodes.
Disconnected selections, separate loops and branches reject regeneration.
Returned poles, promoted poles and storage remain bounded by the 10,000-pole
per-projection and 100,000-pole per-sketch limits; degree remains at most 25.
Endpoint clustering takes bounded O(E²) work for E selected edges.

An open chain starts at its lexicographically least terminal endpoint in
source-space XYZ. A closed chain starts at its least source edge endpoint;
incident-edge ties use oriented control poles and normalized weights. Edge
orientation is reversed when needed. This makes ordering independent of
selector enumeration, but the seam can move when source edits change those
geometric ordering keys. Individual source edge labels are not retained.

The geometry uses spline endpoint names `id:start/end`; internal poles are
runtime data. Closed chains reuse the start pole and retain a fixed end alias,
using the 1e-7 mm join/closure tolerance. Adjacent endpoint discrepancies within
that tolerance are joined at the retained pole. Curve trace is preserved up
to this joint tolerance without sampling; span parameterization is rebuilt.

A chain becomes one native boundary edge: junctions are spline knots, rather
than independently addressable topology vertices/edges. Native profile
construction, extrusion and STEP/STL exchange are supported. Downstream edge
selectors see this composite edge; individual junction topology and stable
junction references remain separate authoring extensions.

The `projected-chain-pad` example imports a closed rational arch plus its line
segment into a tilted sketch, with strict acceptance. Editing source `weight`,
`height` or `width` updates the entire boundary. Degenerate live edits preserve
accepted geometry. Validation includes mixed degrees, reversed/permuted
selection, ambiguous endpoints, branch/disconnect rejection and handle cleanup.
A 1,000-edge closed import plus a 1,001-edge rejection takes 0.332 s;
1,000 mixed-degree profile edits take 0.432 s (10 s budgets).

## Named chain junction references (schema 118)

A chain projection may declare `junctions`, with user-chosen names and bounded
source-space matches. Each entry has `id`, length-vector expression `near`, and
positive length expression `maximum_distance`. For example:

```json
{
  "id": "right",
  "near": {"components": {
    "x": {"parameter": "width"},
    "y": {"literal": {"value": 0, "dimension": "length", "unit": "millimeter"}},
    "z": {"literal": {"value": 0, "dimension": "length", "unit": "millimeter"}}
  }},
  "maximum_distance": {"literal": {"value": 0.000001, "dimension": "length", "unit": "millimeter"}}
}
```

The entry creates fixed point `projection:junction:right` in sketch coordinates.
Use it in constraints, dimensions and authored profile entities. The name
retains its meaning through source and control-pole changes by matching a
unique geometric junction, rather than selecting a pole or traversal index.
Hint expressions use located family coordinates before sketch-plane projection;
instance/assembly placement is outside that coordinate system. Source edits
must keep the intended junction within the authored bound. This is a guarded
geometric reference, not persistent topological identity.

Candidates are connections between two distinct selected source edges.
Open-chain terminals use the existing `start/end` names; a standalone closed
edge's seam and internal Bézier boundaries are not junction candidates. Both
smooth and corner connections qualify. A closed multi-edge seam qualifies.
Exactly one candidate must lie within the bound: zero or multiple candidates
reject regeneration, even if one ambiguous candidate is closer. Aliases use
retained curve endpoint coordinates at the join, so they agree with the
composite curve after the existing 1e-7 mm connection/closure tolerance.

Names must be nonempty and unique within the projection. At most 1,000 entries
are allowed per chain and 10,000 per sketch. Non-chain projections reject
junction entries. Matching reuses the chain's existing endpoint graph and
performs bounded O(references × junctions) distance checks without extra native
queries. Hint and radius parameters participate in incremental invalidation;
failed hint-only edits preserve the accepted build. Older documents migrate
with an empty list, omitted on serialization. Native ABI remains 60.

Viewer/MCP group annotations expose the name, fixed point, coordinate, match
rule and source controls. Point constraints and dimension annotations link to
the same fixed references. The `projected-junction-pad` example constrains a
marker to the right-hand junction and checks the width between left/right
references. Change `width` to move the marker; set `junction_shift` to 0.1 mm
to trigger a missing match, then restore zero. Valid STEP/STL export is verified.

The composite remains one native edge. Named reference points do not restore
separate junction vertices, split edges or change tangent/corner-edit semantics.
A 1,000-reference/1,000-edge loop takes 0.360 s, and 1,000 driven profile edits
take 0.307 s (10 s budgets). MCP renders 20 annotated pads in 1.308 s / 10 s.


## Named projected intersections (schema 119)

An analytic projection may declare `intersections`, each with `id`, `other`
(the ID of another projection in the same sketch), length-vector expression
`near`, and positive length expression `maximum_distance`. It creates a fixed
point named `projection:intersection:id`, usable by constraints, dimensions,
and authored geometry. The hint uses **sketch-local coordinates**, with z=0,
including on tilted or attached sketch planes. This differs from junction hints,
which use source family coordinates before projection.

Supported pairs are line–line, line–circle, line–arc, circle–circle,
circle–arc, arc–arc, and line–ellipse, in either order. Lines are finite
segments and arcs obey their directed span; circles and ellipses are full curves.
The calculation uses analytic equations on the materialized projected curves,
without sampled intersections or extra native queries. Curves can cross in the
sketch plane even when their source edges do not meet in 3D. Intersections
refer to the projected entities before profile trim, extend, offset, or corner
operations. ABI 61 extends this to all projected curve pairs, including
splines/chains and the remaining ellipse pairs, using the native path below.

Exactly one crossing must fall within the hint radius. Zero matches or multiple
matches reject regeneration; proximity does not silently choose the closest.
An isolated tangent contact is one candidate. Overlapping collinear segments
and equal coincident circular supports are rejected; coincident circular
supports are rejected even when their arc spans are disjoint. A shared endpoint
of otherwise disjoint collinear segments is an isolated candidate. Calculations
use floating-point analytic equations with bounded roundoff checks; ill-conditioned
or degenerate input can fail rather than produce an arbitrary reference.

Names must be nonempty and unique within each projection, and `other` must name
a distinct declared projection. Generated point IDs obey the usual uniqueness
checks. Limits are 1,000 references per projection and 10,000 per sketch.
Curve data and up to two crossings are cached once per referenced pair, so many
names sharing a pair do not repeat the calculation. Storage is O(sketch size +
references + pairs), and matching is O(references); entity lookup is bounded by
the existing 1,000-projection limit. Hint and radius expressions participate in
parameter invalidation. Older documents migrate with an empty list, omitted
on serialization. This format remains schema 119; current native ABI is 61.

The viewer and MCP expose a selectable `Intersection name` annotation with the
fixed point, sketch coordinates, both curve targets, and controls from both
source projections. Reference points do not split curves or introduce native
vertices. Authored free geometry can be driven by coincident/distance relations
to these fixed points under `fully_constrained` acceptance.

See `tools/model/projected-intersection-pad.request.json`: a horizontal projected
line crosses a projected circle, and the right crossing drives the centre of a
2 mm radius extruded pad. Change `radius` from 10 to 12 mm to move the centre
from x=8 to x=sqrt(108) mm while y=6 mm and volume=16π mm³ remain unchanged.
The left crossing has its own fixed alias. Setting `match_radius` to 30 mm
rejects an ambiguous selection; setting `intersection_shift` to 10 mm rejects
a missing match. Restore the defaults to recover. Hint-only failures retain the
accepted part; hint-only successful branch changes rebuild the profile and body
while reusing the source features.

Performance gates: 1,000 aliases sharing a curve pair resolve, build, and solve
in 0.005 s; 1,000 driven profile regenerations take 0.236 s (10 s budgets).
MCP renders 20 annotated pads in 0.188 s / 10 s. All 38 advanced sketch and
41 MCP scale gates pass. Native curve-distance checks independently confirm
that the selected point lies on both sources in the planar example.


## Native projected intersections (ABI 61)

Every pair of distinct declared projections now supports isolated finite
crossings, including rational splines, connected chains, ellipse–ellipse,
ellipse–circle and ellipse–arc. Existing line/circle/arc and line–ellipse pairs
retain their analytic fast path. Native pairs reconstruct each exact materialized
curve once in sketch-local XY, then use OCCT's bounded 2D curve intersection
algorithm. They use explicit poles, knots, multiplicities and weights, without
sampled polylines. Analytic arcs retain their directed finite span. Closed
spline/chain profiles remain a single edge; aliases do not split topology.

The hint rules, fixed alias IDs, constraints, dimensions, source controls,
incremental invalidation, and accepted-build preservation are unchanged.
Tilted and attached sketches use the same local coordinates as the solver;
intersections are computed after projection, rather than on the original 3D
source edges. The model format remains **schema 119**, and old examples still
migrate as before. Native and Rust binaries require **ABI 61** and must be
rebuilt together.

The new C/Rust `planar_curve_intersections` query accepts an edge or one-edge
wire in the XY plane. It returns sorted finite crossing coordinates, stores no
shapes, and leaves inputs unchanged. Located shapes are resolved in world
coordinates before checking planarity. Kernel intersection precision is 1e-9 mm;
repeated reports within 1e-12 mm are merged. Distinct nearby candidates retain
the usual ambiguity rule. A null/zero query returns the required count, and a
short buffer fails without partial writes. The Rust wrapper validates both
sessions, queries the count, then fills the bounded buffer.

OCCT can return intersection regions for overlapping or tangential curves.
Outside line–spline pairs, this query rejects any reported region and an
incomplete native solve; it does not substitute an endpoint, midpoint, or
approximate nearest contact. The line–spline path below resolves stable isolated
touches algebraically. Other isolated spline tangencies can still fail explicitly. Existing
analytic pairs retain their isolated-tangency behavior. See
[OCCT's intersection guide](https://github.com/Open-Cascade-SAS/OCCT/blob/master/dox/user_guides/modeling_algos/modeling_algos.md).

Budgets apply before kernel pair work: degree <=25, <=10,000 spline poles,
<=10,001 knots, and <=10,000 spline span pairs per curve pair. The model limits
aggregate native span-pair work to 100,000 and cached native candidates to
10,000 per sketch, with at most 1,000 candidates per native pair. Existing
1,000 references/projection and 10,000 references/sketch caps remain. These are
input/output work bounds, not wall-time deadlines; kernel cost depends on curve
complexity. Results are cached per unordered projection pair, and temporary
native wires are released on success and failure.

`projected-spline-intersection-pad` crosses a rational quadratic arch with a
horizontal line. The selected right crossing drives a fully constrained 2 mm
radius pad and its dimension. At width=40, rise=20, weight=1, offset=6 mm, its
centre is x=20+sqrt(160), y=6 mm. Change `width` or the rational `weight` to move
the pad; a broad `match_radius=50` rejects ambiguity. Restore 12 mm to recover.
`projected-ellipse-intersection-pad` crosses two translated ellipses. Its upper
crossing (named `right`) starts at x=5, y=sqrt(27) mm; edit `radius` or
`minor_radius` to move it. Both examples keep native volume=16π mm³ and expose
both curve/source controls in the viewer and MCP. Model tests also cover exact
closed-chain intersections and tilted-plane coordinates.

Performance gates pass: 1,000 aliases sharing a native spline pair build and
solve in 0.005 s; 1,000 rational spline profile edits varying width and weight
take 0.291 s; 1,000 ellipse-pair profile edits take 0.263 s (10 s budgets).
MCP renders 20 native spline pads in 0.184 s and 20 ellipse pads in 0.179 s.
All 41 advanced sketch and 43 MCP scale gates pass. Both examples export valid
STEP/STL solids, including a weight=1.25 rational spline source. Live source
edits and ambiguity rejection retain the accepted viewer result on failure.


## Isolated line-spline touches (schema 119 / ABI 61)

Finite line–B-spline/Bézier pairs now use a rational Bernstein distance polynomial
for each exact finite Bézier span. Positive weights allow the homogeneous signed
line-distance numerator to identify the same roots as the rational curve. No
sampled polyline or nearest-point approximation is used. This path handles both
crossings and stable isolated touches, including even-multiplicity roots that
do not change sign, fourth-order flat contacts, and non-collinear endpoint
contacts. Connected-chain and projected rational spans use the same path.

The root solver builds a bounded derivative chain and uses its critical roots
to partition [0,1] into monotone intervals. Sign-changing intervals use at most
80 bisection steps. Exact zero critical values retain repeated roots; nonzero
critical values keep their signs, so nearby crossings are not silently merged
into a single touch. Long-double coefficients/evaluation reduce cancellation.
Coefficient uncertainty, roots indistinguishable at the native double parameter
precision, or a final line-distance residual over 1e-9 mm reject explicitly.
Ill-conditioned contacts are not claimed as exact real-arithmetic certificates.

At most 1,000,000 degree-squared evaluation work units are allowed per native
query, in addition to existing degree/pole/span/candidate budgets. Dense
high-degree input is rejected when this work bound is exhausted. Geometry
conversion and coefficient setup remain bounded by the existing input limits.
The model's unordered-pair cache and alias limits remain unchanged. Degree-
chain storage is bounded by degree <=25; there is no recursive call stack.

Collinear or numerically flat spans whose projected control hull overlaps the
finite guide are rejected as overlap/unresolved collinear contact. Completely
disjoint finite collinear hulls are ignored. This deliberately leaves some
collinear endpoint-only cases unresolved. Non-collinear contacts respect the
actual finite line and spline ranges. Repeated point reports at adjacent span
joins use the existing 1e-12 mm merge threshold; other nearby candidates remain
subject to unique-hint matching. Conic–spline and spline–spline tangential regions
still follow the native region-rejection rule.

`projected-tangent-intersection-pad` uses a quadratic arch with unit weights
and a horizontal guide at `rise/2 + offset`. With offset=0, its sole contact is
`reference:intersection:touch`, at (width/2, rise/2) in sketch coordinates. The
reference drives a fully constrained 2 mm radius pad and a direct dimension.
Default centre: (20,10) mm; width=50 and rise=24 move it to (25,12) mm. The
extruded solid retains volume=16π mm³. A positive offset of 0.000001 mm rejects
a missing match, and a negative offset of -0.000001 mm rejects two crossings.
Failed edits retain accepted geometry. Restore offset=0 to recover.

No saved fields, C signatures, or output structures change: model schema remains
119 and native ABI remains 61. The viewer/MCP uses the existing fixed reference
annotation and links controls from both curves. Tests independently project the
returned points onto both native inputs, check trimmed/short domains, rational
and higher-order touches, near misses, nearby distinct crossings, overlap
rejection, work-budget exhaustion, and handle cleanup.

Performance: 1,000 cached touch aliases build/solve in 0.006 s, and 1,000
width/height profile edits take 0.297 s (10 s budgets). MCP renders 20 annotated
touch pads in 0.183 s. All 43 advanced sketch and 44 MCP scale gates pass.
Native STEP/STL export and live failed-edit preservation are verified.

## Axis distances, curve lengths and reference dimensions (schema 122)

Schema 122 adds eight constraints and driven (reference) dimensions:

| JSON variant | Meaning |
|---|---|
| `horizontal_distance: {first, second, value}` | Signed sketch-x offset `x(second) − x(first)`, a length. |
| `vertical_distance: {first, second, value}` | Signed sketch-y offset `y(second) − y(first)`, a length. |
| `line_angle: {line, value}` | Signed angle of a directed line from the sketch +x axis, in `[−π, π]` radians. |
| `arc_angle: {arc, value}` | Sweep of an arc along its direction, in `(0, 2π)` radians. |
| `curve_length: {curve, value}` | Positive length of a line, arc, circle, full ellipse or spline. |
| `equal_curve_length: {first, second}` | Two distinct curves of any of those kinds with equal lengths. |
| `collinear: {first, second}` | Both ends of the second line on the infinite supporting line of the first. |
| `coradial: {first, second}` | Two distinct circles or arcs with a shared centre and equal radii. |

Line, arc and circle lengths are closed form. Ellipse and spline lengths are
measured on the native curve with adaptive arc-length integration, so they
include interpolation tangents, rational weights and non-clamped bases. A
spline length constraint touches the spline's defining and tangent-neighbour
points, like point membership.

A sketch may list `measurements`, each `{"id": ..., "measure": {...}}`, where
`measure` is one of `distance`, `horizontal_distance`, `vertical_distance`,
`point_line_distance`, `angle`, `line_angle`, `arc_angle`, `radius`,
`diameter` or `curve_length`, with the same fields as the matching
constraint minus `value`. Measurements add no equations: the solution, free
degrees and redundancy are unchanged. Each value is the matching
constraint's residual at a zero target, so it uses the solver's own geometry
and sign conventions. `SketchDefinition::measure` returns them in order
(millimetres, or radians for angles), and the viewer shows them as
parenthesised `measured` dimensions with `driving: false`. Ids must be
unique; there are at most 10,000.

The bridge's `occt_bridge_shape_edge_length` now integrates adaptively
(`GCPnts_AbscissaPoint`, 1e-12) instead of with fixed-order Gauss points,
which were off by up to 1.6% on full ellipses (48.563 mm read as 49.327 mm
for semi-axes 12 and 1). The C signature and ABI number are unchanged.

The [dimensioned plate](../tools/model/dimensioned-plate.request.json) is
fully constrained by these relations: a notched bottom edge kept straight by
`collinear`, a slope set by `line_angle` and `curve_length`, axis distances
for every other point, and a hole. Five reference dimensions report the
height, the slope's run, the top length, the shoulder angle and the hole
perimeter. Tests check exact positions for every new constraint, driven
native spline and ellipse lengths, unchanged solutions with measurements,
and rejected references and values. A 1,000-segment chain driven only by
the new dimensions solves with 1,000 reference dimensions in 0.016 s, and
200 splines driven by native lengths solve in 0.046 s (10 s budgets). The
MCP renders 20 dimensioned plates in 0.49 s (10 s budget).

## Rectangles, slots and regular polygons (schema 123)

A sketch may list `shapes`. Each expands, before patterns, into ordinary
named points, lines, arcs and constraints; the solver, profile builder and
viewer see nothing new. Generated IDs are `{id}:{part}`, so constraints,
dimensions, measurements and patterns can refer to them. Listing a shape's
`id` in `profile` or a `holes` loop stands for its boundary in traversal
order. Shape IDs must be unique and free of `:`; all shapes together
generate at most 20,000 points.

| JSON variant | Generates |
|---|---|
| `rectangle: {id, first, opposite, angle?}` | Corners `{id}:corner1` and `{id}:corner3`, sides `{id}:side0`..`{id}:side3` from `first`. Sides follow the sketch axes, or `side0` has the signed `angle` (radians) from +x, with the others perpendicular and parallel. |
| `slot: {id, start, end, radius}` | Side points `{id}:start_right`, `{id}:end_right`, `{id}:end_left`, `{id}:start_left`; sides `{id}:right`, `{id}:left`; caps `{id}:end_cap`, `{id}:start_cap` about the named end centres. Both caps have `radius` and are tangent to both sides. |
| `polygon: {id, center, vertex, sides}` | Vertices `{id}:vertex1`.. after `vertex`, sides `{id}:side0`.. from `vertex`, and the construction circle `{id}:circle`. 3 to 1,000 sides. |

Each generated point adds exactly two equations from existing constraint
kinds, so a shape adds no free degrees or redundancy: a rectangle uses
horizontal/vertical (or line-angle, perpendicular and parallel) sides; a
slot uses two cap radii and four endpoint tangencies; a polygon puts every
other vertex on its circle and makes every side equal to the first.
Generated points are seeded at the exact shape for the current parameters.

The [slotted plate](../tools/model/slotted-plate.request.json) is a
rectangle with two slot holes and a hexagonal hole, all driven by
parameters, with reference dimensions for the slot travel and hexagon side.
The viewer groups each shape's boundary under a `shape-{id}` annotation that
links its radius or angle control. Tests check corners, tangent points and
exact face areas for axis and rotated rectangles, slots in any direction,
and polygons of 3 to 200 sides; shapes used as holes, constrained through
generated IDs and copied by a linear pattern; and rejected IDs, references,
side counts and radii. 500 slot holes and a 1,000-sided polygon expand,
solve and build one face in 1.13 s (10 s budget). The MCP renders 20 slotted
plates in 0.49 s (10 s budget).

## Authored elliptical arcs (schema 124 / ABI 63)

An ellipse may name `start` and `end` points (both or neither) to become an
elliptical arc. It runs from `start` to `end` counterclockwise in the sketch
plane, or clockwise with `"clockwise": true`:

```json
{"id": "arch", "center": "c", "major": "tr", "minor": "m",
 "start": "tr", "end": "tl"}
```

- The solver holds each end on the ellipse with its implicit equation,
  scaled in millimetres. An end that is the major or minor vertex is on the
  ellipse by definition and adds no equation.
- Arcs connect into profiles and holes through their named ends like lines,
  circular arcs and splines.
- `tangent` at a shared end uses the ellipse's true tangent there, not the
  circle approximation.
- `point_on_curve` also enforces the arc's span, measured by eccentric
  angle and scaled by the minor radius.
- `curve_length` and reference dimensions measure the native arc.
- Mirror patterns swap a copied arc's ends, as for circular arcs, and
  native corner fillets and chamfers accept elliptical-arc neighbours.
- Tangency constraints that solve independent contacts
  (`line_ellipse_tangent`, `ellipse_ellipse_tangent`,
  `spline_ellipse_tangent`, `ellipse_circle_tangent`) still require full
  ellipses.

ABI 63 adds curve-wire segment kind 4 (`CurveSegment::EllipseArc` in Rust):
an exact trimmed `Geom_Ellipse` from the centre, both axis vertices (either
may be the longer) and two ends on the ellipse within 1e-7 mm. It turns
counterclockwise about (major − centre) × (minor − centre) unless flagged
clockwise; the sketch maps its own sense onto that frame.

The [arched panel](../tools/model/arched-panel.request.json) tops two
vertical sides with an elliptical arch, fully constrained with no redundant
equations, and measures the arch length. Tests check exact half-ellipse
areas in both directions, a line tangent to an arc at the analytic tangent
point with a native arc length matching numerical integration, the span
check on a rotated ellipse, a mirrored quarter arc closing a half ellipse,
native fillets at both arc corners, and rejected definitions. 500 rise
edits of the panel regenerate with exact volumes in 0.62 s (10 s budget).

## Conic arcs (schema 125)

The `conic` shape draws a conic arc from `start` to `end` whose end tangents
meet at `control`:

```json
{"conic": {"id": "skin", "start": "r", "control": "c", "end": "tip",
           "rho": {"parameter": "rho"}}}
```

`rho` is a dimensionless expression in (0, 1): the shoulder point (the
curve's midpoint) lies `rho` of the way from the chord's midpoint to
`control`. Below 0.5 the arc is elliptic (`rho = w/(1 + w)` with `w = cos θ`
gives a circular arc of half-angle θ), 0.5 (the default) a parabola, and
above 0.5 a hyperbola.

A conic expands into an explicit rational quadratic spline with the shape's
own `id`: poles `start`, `control`, `end`, clamped knots and weights
`[1, rho/(1 − rho), 1]`. It is exact, not an approximation, and inherits
every spline capability: end tangency through the control point, point
membership, native length, intersections, profile and corner edits. The
viewer labels it `conic ρ … (elliptic | parabola | hyperbolic)` and links
`rho`.

The [conic nose](../tools/model/conic-nose.request.json) revolves a conic
half profile into a nose cone; `rho` moves it from elliptic (0.3) through
parabolic to hyperbolic (0.7). Tests check the parabolic segment's
two-thirds area, the shoulder position for all three types, an exact
quarter circle (area 25π and length 5π to 1e-9), a control point solved as
the intersection of two end tangents, revolved volumes against numerical
integration of the exact curve (1e-6), and rejected `rho` and points. 500
`rho` edits regenerate the revolved nose in 0.92 s (10 s budget).

## Trimming between points and crossings (schema 126 / ABI 64)

`trim_between` keeps the part of a profile entity from one bound to another
along the entity's direction:

```json
{"trim_between": {"entity": "head",
  "from": {"crossing": {"curve": "right", "near": "hint_r"}},
  "to": {"crossing": {"curve": "left", "near": "hint_l"}}}}
```

Each bound is one of:

| Bound | Meaning |
|---|---|
| `"start"`, `"end"` | The entity's own start or end, so the other end can stay. |
| `{"point": {"point": id}}` | A sketch point on the entity: its solved position, which must lie within 1e-6 mm of the curve. Constrain it there, for example with `point_on_curve`. |
| `{"crossing": {"curve": id, "near": id}}` | The native crossing of the entity with another sketch curve (in the profile or construction geometry) nearest to the hint point `near`. |

Full circles, ellipses and periodic splines wrap through their seam, so a
trimmed closed curve can be one segment of a larger profile; open curves
need `from` before `to`. Like `trim`, it must precede corner edits and
offsets, which then act on the joined boundary. Crossings come from the
bridge's native planar intersection in sketch-local coordinates, so a trim
follows the curves through every parameter edit without sampled geometry.

ABI 64 adds `occt_bridge_trim_curve_between` (`Session::trim_curve_between`):
it projects both points onto a one-edge curve, requires them on it within
1e-6, and keeps the span between them along its traversal.

The [keyhole tag](../tools/model/keyhole-tag.request.json) cuts a full
circle back to the arc between its crossings with two legs, trims the legs
at the same crossings, and fillets the feet. Tests check the keyhole area
against its closed form (1e-9), a full ellipse trimmed between named points
in both senses, a spline trimmed from its start to a line crossing, and
rejected bounds (identical, self-crossing, unknown, non-crossing, off the
curve, or after a corner edit). 500 neck-width edits regenerate in 1.52 s
(10 s budget).

## Several regions in one sketch (schema 127)

A sketch's face output can hold more than one outer boundary. `regions`
lists further loops, each with its own holes:

```json
"profile": ["shim"],
"regions": [{"profile": ["washer"], "holes": [["bore"]]},
            {"profile": ["slot_shim"]}]
```

- The face output becomes one compound: the profile's face, then one face
  per region in order. Shape IDs expand in region boundaries and holes as in
  `profile`.
- Every entity belongs to at most one loop across the profile, holes and
  regions. Region boundaries and hole loops must be nonempty; at most 1,000
  regions, each with at most 1,000 holes.
- Regions must not overlap each other or the profile's face (touching is
  allowed). Candidates come from a sweep over face bounds, then a native
  common must have no area, so disjoint layouts cost O(n log n). The error
  names the overlapping pair.
- Profile operations (trims, fillets, offsets) apply to `profile` only.
- Wire outputs reject regions, as they reject holes.
- Extrude and revolve sweep each face of a compound into one solid of a
  compound of disjoint solids.

The [spacer set](../tools/model/spacer-set.request.json) draws a shim, a
washer with its bore and a slot spacer in one sketch and extrudes all three
with one feature. Tests check face counts and exact areas with holes and
slots, overlap and touch cases, wire rejection, reused and empty loops,
profile fillets beside regions, and the extruded volume. A negative gap
that pushes the washer into the shim reports the overlap in the viewer.
1,000 rectangle regions build and extrude into 1,001 solids in 0.79 s (10 s
budget).

## Curvature (G2) continuity (schema 128 / ABI 65)

`curvature_continuous: {first, second, point}` makes two curves that share
the end `point` tangent there with equal curvature vectors (curvature
times principal normal). It adds two equations and replaces a `tangent` at
the same point; adding both makes the tangency redundant.

- Lines have zero curvature, circles and arcs `1/r` toward their centre,
  ellipses and elliptical arcs their closed-form curvature, and splines
  their native curvature at the end. Curvature vectors do not depend on
  direction, so the curves may run either way through the joint.
- A cubic spline can be G2 at one end only where its first three poles
  allow it; independent G2 at both ends of one spline needs degree 5 or
  more (six poles), since each end uses three poles.
- Curvatures that cannot match, such as a line and an arc, leave the
  sketch unsolved.
- Seeds must not put a spline pole on its end point, where the tangent is
  undefined.

ABI 65 adds `occt_bridge_curve_curvature_at`
(`Session::curve_curvature_at`): the curvature vector of a one-edge curve
at its point nearest a query, from the native first and second derivatives
(zero where straight).

The [G2 paddle](../tools/model/g2-paddle.request.json) blends two straight
sides into a circular crown with quintic splines that are curvature
continuous at all four joints. Tests check arcs continuing onto the same
circle, a line flattening a cubic's first poles, an arc bending a cubic to
curvature 1/r (confirmed natively), a circle osculating an ellipse at its
vertex (radius b²/a), infeasible and invalid joints, and equal native
curvature on both sides of every joint of the built paddle face (1e-8). 500
rise edits solve all four joints in 2.0 s (10 s budget).

## Text (schema 129 / ABI 66)

A sketch may list `texts`, single lines whose glyph outlines become faces of
the face output:

```json
{"id": "marking", "text": "PART 42", "font": "Liberation Sans",
 "style": "bold", "height": {"parameter": "text_height"},
 "anchor": "anchor", "align": "center"}
```

- `text` is printable UTF-8 on one line, 1 to 1,000 characters; a sketch
  has at most 100 texts.
- `font` is an installed family, resolved strictly through fontconfig in
  `style` (`regular`, `bold`, `italic` or `bold_italic`). A missing family
  or style is an error, never a substitute. Reproducing identical outlines
  across machines also requires the same font file and version. The viewer
  reports the resolved font file.
- `height` is the em size; `anchor` is the sketch point at the baseline's
  start, centre or end (`align`: `start`, `center`, `end`); `angle` turns
  the baseline from the sketch +x axis.
- Glyph outlines are exact: lines and quadratic or cubic Béziers straight
  from the font, with kerning. Contours are sorted into outer boundaries
  and holes by nesting, so TrueType and CFF fonts both work. Missing glyphs
  are errors.
- A sketch of only texts (no other curves) has just the glyph faces. With a
  profile or regions, each text joins the face compound and must not
  overlap them; engrave or emboss with a separate text sketch.
- Wire outputs reject texts.

ABI 66 adds `occt_bridge_font_file` and `occt_bridge_create_text`
(`Session::font_file`, `Session::create_text`). The bridge now links
FreeType and fontconfig directly; it does not use OCCT's visualization
toolkit for text.

The [engraved nameplate](../tools/model/engraved-nameplate.request.json)
cuts bold "PART 42" 1 mm into a plate from a text-only sketch on its top
face. It needs Liberation Sans installed. Tests check glyph topology
(holes in `8`, separate dots and `%` rings), exact area scaling with height,
alignment, rotation, overlap with a profile, the engraved volume against
the letters' area, and errors for missing families and glyphs, bad text,
heights, anchors and wire outputs; geometry tests use the first installed
of several common sans families and skip when none is. A 1,000-character
text builds, extrudes and validates in 5.1 s (10 s budget, checked alongside
all 52 sketch scale gates).
