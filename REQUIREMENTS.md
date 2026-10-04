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
  `Millimeter`, `CubicMillimeter`, `Kilogram`). `NoInterference` reports the
  largest overlap volume; `MinimumClearance` reports the closest violating pair
  and omits a value when nothing is closer than the minimum, because pruned
  pairs are not measured;
- `evidence`: `Exact`, or `Sampled { samples, unresolved }` for rules that
  screen finitely many samples, where a failure is a real violation but a pass
  is not a proof;
- `witness`: for collision failures, the two outputs (`instance:output`) and one
  closest or overlapping point on each, in millimeters.

## Planned

Sampled manufacturing rules (wall thickness, draft, overhang) follow; see the
[Roadmap](ROADMAP.md).
