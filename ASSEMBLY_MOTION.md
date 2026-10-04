# Joints, collision checks, and sampled motion

Schema 41 persists `AssemblyJoint` records by frame. Each frame can carry one
fixed, revolute, prismatic, cylindrical, or planar joint. The frame tree defines
parentage and the existing rest placement defines attachment geometry. Joint
motion acts after the frame's rest placement, in its parent's coordinates;
the parent placement and joint then move everything in the child frame.
Axes are dimensionless nonzero vectors, origins and translations are lengths,
and angles are dimensionless radians. Planar joints need an in-plane x axis
perpendicular to their normal; y is normal cross x.

`InstanceGraph::add_joint` and `add_joints` validate unique IDs, frame bindings,
axes, coordinates, and optional limits before accepting changes. Batch insertion
is atomic. `set_joint_coordinate(frame, JointDof, Quantity)` changes one legal
coordinate in O(log joints); invalid units, nonfinite values, and limit violations
preserve the previous state. Limits reject values rather than clamping them.
Earlier documents load with no joints and preserve their placements.

These are driven kinematic joints. They use named frames for attachment and
do not automatically infer mating faces or solve closed mechanical linkages.
Existing datum relationships remain available for checking their poses.

## Geometry checks

Regenerate the participating instances, then call
`GraphRegeneration::check_pair` or `check_collisions` with explicit
`InstanceOutputRef` values. Select one final solid output per distinct instance
to avoid counting intermediate feature geometry as additional components.
`CollisionOptions` specifies length-valued minimum clearance and contact
tolerance plus an optional absolute overlap-volume tolerance in mm³.

Results distinguish clear pairs, touching pairs, positive-volume interference,
and insufficient clearance. They report separation in mm, overlap in mm³, and
one pair of witness points. Interference takes precedence over clearance;
touching pairs with a positive required clearance are clearance violations.
The overlap threshold combines the requested absolute tolerance with a small
operand-volume-relative roundoff margin. Bounding boxes only choose candidates;
BREP minimum-distance and non-destructive common-volume queries determine the
reported result. Inputs remain unchanged and queries create no persistent
shape handles.

Median bounding-volume hierarchy construction takes expected O(n log n) time
and O(n) storage. Sparse assemblies avoid testing every pair. Dense or
degenerate bounds can require O(n²) candidates when every pair may intersect.
Validity and volume are measured once per shared parameter variant and output.

## Motion studies

`MotionStudy::linear` creates 2–10,000 evenly spaced samples including both
endpoints. Translation endpoints normalize to mm; angular endpoints are radians.
For coordinated movement, construct `MotionSample` values containing several
`JointPosition` values. Each sample applies to the same original graph, so an
omitted coordinate retains its original value rather than the preceding sample.
Duplicate coordinates within a sample are rejected.

`InstanceGraph::run_motion_study` validates every sample and joint limit before
generating geometry. It generates each participating parameter variant once,
then makes location-only placed copies per sample. Results contain collision
reports and datum-relationship checks at each sample. The source graph and any
accepted generation remain unchanged. Temporary geometry is released on both
success and failure, with handle storage bounded by the local variants plus
one sample's outputs rather than the number of samples.

The study reports sampled positions. It does not prove that the path between
samples is collision-free, integrate forces, or solve linkage constraints.
Increase sampling density for narrow obstacles and fast angular changes.

For a runnable workflow, `occt-motion-study` reads a model document and a setup
file selecting separate component outputs and bounded hinges. It writes a
reloadable assembly, a coordinated study, and sampled plus continuous reports.
It preserves the source model and supports general hinges as well as wing
elevons. See the [command and wing examples](tools/motion-study/README.md).

## Kernel measurements

`InstanceGraph::mass_properties(session, outputs)` measures one solid output per
distinct unsuppressed component at its current placement and joint pose. It
applies the assigned or inherited material density and returns each component's
mass in kg, volume in mm³, center in mm, and central inertia tensor in kg mm²,
plus the assembly totals. Parameter variants share generation. Total inertia
uses the parallel-axis theorem with incremental weighted centers, retaining
accuracy far from the world origin. Components contribute their full masses
even if they overlap; the sum describes parts, rather than a geometric union.
The query preserves the graph and releases all generated handles on success
or failure. Empty selections, duplicate instances, missing materials/outputs,
and unrepresentable physical values are rejected.

An existing mass report can group measurements with `material_totals()` and
project its CG with `balance(ChordReference)`, without another kernel query.
`SymmetricWingPlanform` supplies a MAC reference from exact linear-panel
integrals. The [`occt-balance-report` command](tools/balance-report/README.md)
writes component, material, inertia and geometric CG data from selected outputs.
It supports an explicit world-space chord or a matching wing station layout.

ABI 33 adds `Session::mass_properties`, `distance`, and `overlap_volume`.

Mass properties contain volume, center, the row-major central inertia tensor
in model XYZ axes at unit density, and an adaptive relative volume error
estimate. For model length unit u, inertia has units u⁵; multiply by a compatible
density to obtain physical inertia. Parametric model geometry uses mm.
The central tensor is evaluated about a nearby reference to preserve accuracy
when geometry is far from the world origin. Distance accepts general BREP
geometry; mass properties and overlap require valid solid geometry.

The scale suite checks 10,000 joint insertions and edits, 10,000 separated solid
collision participants sharing one generation, and 1,000 slider samples crossing
an obstacle with one generated variant and no leaked handles.

## Continuous translation paths

`InstanceGraph::check_translation_motion(session, study, options)` checks the
paths between the samples of a `MotionStudy`. Joint translations interpolate
linearly in normalized millimeters; every sample still applies to the original
graph. Angular coordinates must stay constant across the entire study, even
when a full turn would have identical endpoint poses. Prismatic motion,
cylindrical axial motion at a fixed angle, planar translation at a fixed angle,
and nested frames with fixed rotations are supported. Changing geometry,
rotating paths, and solved closed linkages are outside this API.

```rust
let report = graph.check_translation_motion(
    &session,
    &study,
    ContinuousCollisionOptions::default(),
)?;
assert_eq!(report.status, ContinuousStatus::Clear);
assert_eq!(report.unresolved_pairs, 0);
```

A swept bounding-box BVH first rejects pairs whose paths stay separated. For a
candidate pair, exact BREP distance at an interval midpoint supplies a lower
separation bound throughout that interval:

```text
interval separation >= midpoint distance - relative translation * half interval
```

Intervals whose bound exceeds the required clearance/contact threshold and
numeric guard are clear. Other intervals subdivide adaptively. The report
contains only witnessed violations and unresolved pairs; a witnessed violation
can be touching, interference, or insufficient clearance, using the existing
`PairCheck` convention. `Collision` takes precedence in the aggregate status;
`unresolved_pairs` must also be checked when completeness for other pairs matters.
A witness fraction lies in [0, 1] within its zero-based segment. It is an observed
violating position, **not** the first time of contact.

The default extra distance guard is 1e-6 mm. Frame/world-coordinate roundoff
adds a scale-dependent margin. Bounds use native floating-point BREP queries,
not a formally certified error enclosure for OCCT. Caller-supplied guards should
reflect the precision of the input geometry. Near-contact or grazing paths can
remain unresolved without producing an observed violation.

`ContinuousCollisionOptions` bounds exact queries globally (default 100,000),
subdivision depth (32), and the smallest interval fraction (1e-8). Reaching these
limits returns `Unresolved`, never a clear path. The separate candidate-pair
budget (100,000 per segment) fails the query if exceeded. Invalid units,
coordinates, limits, angular motion, outputs, and budgets reject the operation;
no failure changes the graph or leaves temporary handles behind.

Local parameter variants generate once for the entire study. Each interval
query places two shared copies and releases them immediately. Swept BVH storage
is O(selected bodies); subdivision storage is O(depth); reports store at most
one nonclear record per candidate pair per segment. Sparse assemblies avoid
O(n²) pair queries, while dense swept bounds can require that many candidates.
The scale suite covers 10,000 translating bodies and 1,000 independently checked
obstacle crossings. This distance-and-motion-bound approach is related to
[controlled conservative advancement](https://gamma-web.iacs.umd.edu/papers/documents/articles/2009/tang09.pdf);
this implementation uses adaptive interval subdivision for translation-only
BREP motion.

## Closed-linkage solving

`InstanceGraph::solve_joint_coordinates(&free, options)` adjusts explicitly
selected `JointVariable { frame, coordinate }` entries until all recorded
assembly relationships hold. Coordinates omitted from `free` stay driven;
instance placements and frame rest placements stay fixed. A crank-slider can
select its connecting-rod angle and slider distance while leaving the input
crank angle driven. A four-bar can select its coupler and rocker angles.

```rust,ignore
let free = [
    JointVariable { frame: "rod".into(), coordinate: JointDof::Angle },
    JointVariable { frame: "slider".into(), coordinate: JointDof::Axial },
];
graph.set_joint_coordinate("crank", JointDof::Angle, Quantity::scalar(0.8))?;
let result = graph.solve_joint_coordinates(&free, JointSolveOptions::default())?;
if result.solved {
    // Regenerate or check collisions at the accepted closed pose.
}
```

The local solver reuses the placement solver's relationship residuals and
Marquardt-damped sparse normal equations. Finite differences respect bounds;
trial steps project onto the declared coordinate limits. Set
`characteristic_length` to a representative link length when angular and
linear coordinates differ greatly in scale (default 1 mm). Iteration offsets
use millimeters or radians internally. Reported values retain original units;
an exact active bound retains its declared limit unit to avoid rounding beyond
that bound.

Only a candidate passing every authoritative relationship check replaces the
graph. An unsuccessful result includes best-fit positions, relationship checks,
active limits, Jacobian nullity, redundant equation count, and the largest
normalized residual. Failure leaves the original graph unchanged. Nullity is a
local numerical rank estimate before active bounds restrict motion. No BREP
geometry is generated or native handles allocated by this operation.

Limits are 32 selected coordinates, 256 relationships, and 1–1,000 iterations
(default 200). Each Jacobian evaluates the relationships twice per selected
coordinate, using temporary graph copies. This is intended for bounded
mechanisms; it does not claim large assembly solver scalability. All recorded
relationships participate, including fixed endpoint checks. Empty selections,
duplicate coordinates, unsupported freedoms, fixed-range bounds, and invalid
options reject the operation.

The initial pose selects a local assembly branch. Singular seeds, incompatible
relationships, exhausted budgets, or travel limits can prevent closure. There
is no global branch search or force simulation. For an individual pose, drive a
coordinate, solve closure, and then regenerate or query collisions. Closed
motion studies below automate this sequence for sampled poses. Tests compare
crank-slider and four-bar closure against
independent geometry and cover scales, distant origins, planar alignment,
limits, conflicts, unit preservation, persistence, and atomic failures. The
scale suite also checks 1,000 driven crank-slider poses against an analytic
position oracle.

## Continuous rotating joint paths

`check_continuous_motion(session, study, options)` also accepts changing revolute,
cylindrical, and planar angles. Every coordinate interpolates linearly between
adjacent samples in its joint's parent coordinates. Angles stay unwrapped:
0 to 2π is a full turn, and negative or multiple turns retain their direction
and travel. Omitted sample coordinates use the original graph's value, matching
sampled motion semantics. Interpolation does not re-solve linkage closure.

The checker propagates an enclosing sphere and point-speed bound through each
body's frame chain. For a rotating frame, the speed bound adds angular travel
in radians times the greatest possible distance to its pivot, plus linear
travel. Child-frame speed is preserved by the parent's rigid transform. The
sum of both bodies' speed bounds limits how much their separation can change
within an interval. Exact midpoint BREP separation greater than this bound and
the configured margins rejects the whole interval; otherwise it subdivides.
Swept enclosing spheres feed the BVH. Their bounds can be loose for long or
nested mechanisms, increasing candidate pairs and exact queries.

Each exact query applies interpolated frame transforms to shared geometry,
releasing intermediate handles immediately. Unchanged angular paths retain the
translation checker's relative-translation bound and swept boxes. The graph,
accepted geometry, query budgets, and `Clear`/`Collision`/`Unresolved` semantics
are the same as for translation checks. `check_translation_motion` remains an
explicit translation-only API and rejects changing angles.

Coordinate, angular interpolation, and transform roundoff contribute to the
separation guard. Large angle magnitudes can make that guard large and produce
unresolved results; nonrepresentable bounds reject the operation. These are
floating-point kernel queries with numeric margins, not a certified error
enclosure. Near contact, grazing, fast multi-turn motion, or dense swept bounds
can exhaust the query/subdivision budgets. A witnessed collision identifies an
observed violating pose, not the first time of contact. The checker does not
simulate forces, changing geometry, or time-varying joint axes and pivots.

Six tests cover full and reverse multiple turns missed by endpoint samples,
counter-rotating bodies, nested planar/cylindrical paths against an independent
pose oracle, conservative
bounds over sampled corner trajectories, clear and budget-limited paths, large
angles, invalid inputs, overflow, accepted-state preservation, and cleanup.
Scale cases exercise 10,000 sparse independently rotating bodies and 1,000
obstacle crossings checked against independent planar box geometry. These take
0.180 s and 10.640 s respectively (10 s and 30 s budgets).

## Closed-linkage motion studies

`solve_motion_study(study, free, options)` closes each driven sample without
creating geometry. `run_closed_motion_study(session, study, free, options)`
then runs the sampled collision checks, generating each local parameter
variant once. For a crank-slider, `study` drives the crank while `free` selects
the connecting-rod angle and slider distance.

```rust,ignore
let result = graph.run_closed_motion_study(
    &session, &study, &free, ClosedMotionOptions::default(),
)?;
match result.closure.status {
    JointMotionStatus::Complete => {
        let motion = result.motion.as_ref().unwrap();
        // Each sample contains closed relationships and sampled collision checks.
    }
    JointMotionStatus::ClosureFailed | JointMotionStatus::BudgetExceeded => {
        // Inspect failed_sample and solutions; collision checks have not run.
    }
}
```

The first pose starts from the source graph's free coordinates. Each later pose
starts from the previous successful solution. Driver edits remain independent:
a driver omitted from a sample uses the original graph's value, rather than
inheriting the previous driver's value. A sample cannot both drive a coordinate
and select it as free. All driver edits and collision options validate before
solving begins; kernel-free solving does not validate the selected BREP outputs.

`JointMotionSolution` reports each attempted closure, total reported iterations,
status, and the first failed or unattempted sample index. `closed_study` is
available only after every sample closes and contains the explicit driver
edits plus solved free coordinates. Omitted drivers still refer to the original
graph, so reuse that study with the same source graph. A closure or budget
failure returns no partial study and no collision result; successful earlier
closures and the last attempted best fit remain available for inspection.

`ClosedMotionOptions` defaults to the ordinary 200-iteration per-pose solver
and 100,000 total reported iterations. The total budget must be between 1 and
1,000,000; each pose's iteration allowance is capped by the remaining budget.
When it is exhausted, later samples are not attempted. The existing limits of
10,000 samples, 32 free coordinates, and 256 relationships also apply. Report
storage grows with sample count and its coordinates/checks; temporary native
geometry stays bounded by the local variants and one sample's placed outputs.
The graph and accepted geometry remain unchanged on success, closure failure,
invalid input, and kernel errors.

This is local pose continuation. Start from an assembled pose near the first
driver sample and use suitably spaced samples. A large driver jump or singular
configuration can reach another valid assembly branch; there is no global
branch guarantee. Collision results are sampled. Even a returned closed study
fed to `check_continuous_motion` describes independent linear interpolation of
its coordinates, which can leave linkage closure between samples. It does not
certify the continuous path of the constrained mechanism.

Six tests compare driven poses against analytic slider positions, exercise both
seeded assembly branches, check sampled interference after closure, and cover
late travel-limit failures, omission semantics, budgets, invalid driver/free
roles, generation failures, accepted-state preservation, and cleanup. The
10,000-pose closure case takes 0.445 s; 1,000 closed poses with shared geometry
and collision checks take 0.534 s (5 s budgets).
