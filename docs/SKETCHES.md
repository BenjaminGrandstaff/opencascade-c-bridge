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

Current authoring examples use schema 110. Older model documents migrate
without adding constraints or changing their entities. Current native ABI is 56.

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
Open-profile corner edits follow in schema 110 below. General spline/ellipse
corner neighbors remain unsupported by this editor.

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
schema 110 without adding operations. Spline/ellipse neighbors remain future
work.
