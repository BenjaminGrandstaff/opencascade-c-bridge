# Requirement rules

Requirements record why geometry exists and check it on every regeneration.
Each has an id, version, kind, priority, statement, provenance, and one rule.
A **required** failure rejects the regeneration, releases its handles, and keeps
the previous accepted result; **preferred** and **advisory** failures are
reported and the result is accepted.

Part requirements live on a family and run once per distinct resolved
parameter set, so 10,000 clones that differ only in placement are checked once,
in family coordinates. Assembly requirements live on the instance graph and run
after `regenerate_all` on placed geometry. Partial regeneration and motion
samples do not evaluate them.

## Part rules (`VerificationRule`)

| Rule | Passes when | Evidence |
|---|---|---|
| `ShapeValid { output }` | The output is a valid BREP. | Exact |
| `VolumeRange { output, minimum, maximum }` | Volume lies in the inclusive range. | Exact |
| `Connectivity { output, solids, allow_voids }` (schema 46) | The output has exactly `solids` solids, no face, edge, or vertex outside them, and, unless `allow_voids`, one shell per solid. | Exact |
| `MinimumRadius { output, minimum, side, sharp_edges, samples_per_direction }` (schema 47) | Every face radius on `side` is at least `minimum`. | Exact on planes, cylinders, cones, spheres, and tori; sampled on other surfaces |
| `MinimumWall { output, minimum, mesh, maximum_samples }` (schema 48) | No inward ray from a sampled facet crosses less than `minimum` of material. | Sampled |
| `DraftAngle { output, pull_direction, minimum_radians, mesh }` (schema 48) | No face runs closer than `minimum_radians` to parallel with the pull. | Sampled |
| `Overhang { output, build_direction, maximum_radians, mesh }` (schema 48) | No downward facet above the build plate leans more than `maximum_radians` from vertical. | Sampled |

Connectivity catches booleans that leave disjoint pieces, sewing that never
closes into a solid, and hollow results with sealed internal voids. It maps each
solid's faces, edges, and vertices onto the output's topology once: O(topology)
time, with transient handles released before returning. `allow_voids` is
omitted from documents when false.

### Minimum radius

Each face's principal curvatures are signed by the part's outward normal.
**Convex** radii curve away from it: the outside of a cylinder, a sphere, or a
fillet on an outside corner. **Concave** radii curve toward it: a bore or a
fillet in an inside corner. `side` selects `Convex`, `Concave`, or `Both`.

- Planes have no curvature. Cylinders and spheres have one radius. Cones use the
  smallest circumferential radius over the face, which is zero when the face
  includes the apex. Tori check their minor radius and the stationary latitudes
  of the other principal radius. These are exact.
- Other surfaces (B-splines, blends, swept and offset surfaces) are evaluated on
  a `samples_per_direction`² grid over the face's parameter range, keeping only
  points inside the face. A smaller radius between samples can be missed, so the
  result reports `Sampled` evidence. The default is 17 per direction (omitted
  from documents); the allowed range is 2 to 1024.
- `sharp_edges: Ignore` measures curved faces only, such as checking fillet
  sizes. `sharp_edges: ZeroRadius { tangency_radians }` also treats every
  sharp edge on the measured side as radius zero, such as an inside corner a
  round cutter cannot reach. Edges are classified by OCCT's offset analysis;
  faces meeting within `tangency_radians` are smooth.

Radii within 1e-12 relative of the minimum pass, because radii are computed
from curvature. The measured value is the smallest radius found, and a failing
result's witness names the face or edge (`face 3`, `edge 7`, indexed in
`Session::subshapes` order) and a point on it. A part with no curvature on the
measured side passes and reports no measured value. Cost is O(faces) for
analytic faces plus O(samples²) face classifications per sampled face, and one
edge analysis pass when sharp edges count.

### Manufacturing screens

The three manufacturing rules tessellate the output once, within `mesh`
(`MeshSettings`: linear and angular deflection and a triangle budget, default
0.1 mm, 0.3 rad, 1,000,000 triangles; omitted from documents when default), and
screen the facets. Directions are dimensionless and in family coordinates,
because part rules run before placement. All three report `Sampled` evidence:
facet normals approximate curved surfaces to within the mesh deflection.

- **`MinimumWall`** casts an inward normal ray from the centroid of up to
  `maximum_samples` facets (1 to 20,000, default 1,000, omitted when default)
  and measures the distance to where it leaves the material. The output's mesh
  must be closed and consistently oriented. A ray's length is at least the
  local wall thickness, so a sample below `minimum` is a real thin wall, up to
  the mesh deflection; a pass does not prove a global minimum, because thin
  regions between samples can be missed. The witness gives the ray's entry and
  exit points. Samples whose ray finds no exit are counted as `unresolved`, and
  a screen where no ray finds an exit is an error rather than a pass.
- **`DraftAngle`** measures each face's smallest facet draft magnitude: the
  angle between the facet and the pull direction, whichever way it leans. Faces
  closer to parallel with the pull than `minimum_radians`, in [0, pi/2), "require
  draft". Facets normal to the pull (caps) are skipped, and faces leaning either
  way pass because they release from one mold half or the other. This is the
  usual CAD draft analysis; it does not detect undercuts, which depend on the
  parting line. (The mesh hand-off report keeps its signed per-face minimum.)
- **`Overhang`** counts downward-facing facets, above the lowest build plane,
  that lean more than `maximum_radians`, in [0, pi/2], from vertical. Bridging,
  supports, and process settings are not modeled. The measured value is the
  count, with a maximum of zero.

Cost is one tessellation, O(T) for draft and overhang, and O(T log T) indexing
plus one ray per wall sample, for T facets. Like other part rules they run once
per shared parameter set.

## Assembly rules (`AssemblyVerificationRule`)

| Rule | Passes when | Evidence |
|---|---|---|
| `MassRange { instance, output, .. }` | Volume times material density lies in range. | Exact |
| `DatumClearance { first, second, minimum, maximum }` | Datum separation lies in range. | Exact |
| `RelationshipSatisfied { relationship }` | The relationship holds within model tolerances. | Exact |
| `NoInterference { outputs }` (schema 46) | No two outputs overlap by volume. Touching within the model's linear tolerance is allowed. | Exact |
| `MinimumClearance { first, second, minimum }` (schema 46) | Every pair is at least `minimum` apart; interference and contact fail when `minimum` is positive. | Exact |

`MinimumClearance` checks pairs within `first`, or, when `second` is given,
only pairs with one output from each set. Pairs inside a set are then never
inspected, an instance in both sets is never paired with itself, and each
unordered pair is checked once.

### Output sets

Collision rules name outputs with an `OutputSet`:

- `Explicit(Vec<InstanceOutputRef>)`: listed outputs, at most one per instance.
  Each must be a declared feature of its instance's family. Instances suppressed
  by the active configuration are skipped. Removing a pattern member that a set
  names explicitly is rejected.
- `AllWithOutput(name)`: that output of every generated, unsuppressed instance
  that has it. The document stores one name whatever the instance count.

A set that matches no generated output is an evaluation error, not a pass, so a
misspelled output name cannot succeed vacuously.

Collision checks are exact BREP distance and overlap-volume queries behind a
bounding-volume index: expected O(n log n) broad phase for n outputs, or
O((m + n) log n) between sets, and one exact query per candidate pair. Dense
layouts where every bound overlaps can still need O(n²) exact queries. Validity
and volume are measured once per shared variant, not per instance. See
[Assembly motion](ASSEMBLY_MOTION.md) for the underlying checks.

## Results

`VerificationResult` carries:

- `status`: `Passed` or `Failed`, and a readable `message`;
- `measured`: the value and its limits in normalized units (`Count`,
  `Millimeter`, `CubicMillimeter`, `Kilogram`, `Radian`). `NoInterference` reports the
  largest overlap volume; `MinimumClearance` reports the closest violating pair
  and omits a value when nothing is closer than the minimum, because pruned
  pairs are not measured;
- `evidence`: `Exact`, or `Sampled { samples, unresolved }` for rules that
  screen finitely many samples or facets, where a failure is a real violation
  (up to the mesh deflection) but a pass is not a proof;
- `witness`: on failure, what was found and where, in millimeters: the two
  outputs (`instance:output`) and a point on each for collisions; the face or
  edge (`face 3`, `edge 7`, in `Session::subshapes` order) and a point on it for
  part rules, with a wall ray's entry and exit points.

## Limits

Sampled rules cannot prove the absence of violations between samples, and the
draft rule does not find undercuts. Interference, minimum-clearance, and
manufacturing rules evaluate one configuration and pose: the active
configuration at `regenerate_all`, not every motion sample. See the
[Roadmap](ROADMAP.md) for later extensions.
