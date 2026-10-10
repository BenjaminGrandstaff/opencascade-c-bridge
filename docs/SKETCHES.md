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

Current authoring examples use schema 98. Older model documents migrate
without adding constraints or changing their entities. Native ABI remains 52.

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

Entity trim/extend operations must precede whole-profile offsets; multiple
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
Ellipse tangency and general curve contact remain future work. Native ABI
remains 52.

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
