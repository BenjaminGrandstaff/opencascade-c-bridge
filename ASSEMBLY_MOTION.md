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
