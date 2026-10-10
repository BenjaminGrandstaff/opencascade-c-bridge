# Linked edge projections in sketches

Schema 94 adds `SketchDefinition.projections`. Each entry links one named
analytic entity to a source feature and semantic edge selector. Native ABI
remains 52. Projection uses located native line/conic coefficients and an
orthogonal sketch frame, without sampled or polygonal replacements.

```json
"projections": [
  {
    "id": "front-edge",
    "input": "block",
    "kind": "line",
    "edge": {"named": "front-reference"}
  }
]
```

The named reference must be declared as an edge reference in the owning family.
Inline selectors also work; the complete
[projected-pocket example](../tools/model/projected-pocket.request.json)
selects the block edge at maximum Y and maximum Z. Source references stay in
the saved model and participate in dependencies, named-reference signatures
and selector-parameter tracking. No topology index is persisted.

## Named geometry and constraints

Every entry must select exactly one edge, and its declared `kind` must match
the projected geometry. IDs and generated point names must not collide with
existing entities/points. At most 1,000 projections are accepted per sketch.
Sources are feature outputs within the same part family.

| Kind | Projected geometry | Generated fixed points |
|---|---|---|
| `line` | Nonzero straight segment | `id:start`, `id:end` |
| `circle` | Full circular projection | `id:center`, `id:rim` |
| `arc` | Partial circular projection | `id:center`, `id:start`, `id:end` |
| `ellipse` | Full ellipse or circle projection | `id:center`, `id:major`, `id:minor` |

The imported curve entity is named `id`. Existing constraints can reference
that entity and its generated points. Imported points are fixed by the source
geometry; solve other sketch points relative to them. Existing `profile`
selection chooses which entities form the generated boundary, so projections
can be construction references or actual profile edges. Use an explicit
profile when construction lines should be omitted.

Line/arc endpoints follow the native edge's oriented parameter interval;
they are not persistent corner identities. Circle rim points use sketch +X.
Ellipse major/minor points describe the projected principal axes, rather than
source vertices or seams. Arc sense follows the projected basis and oriented
interval, including mirrored sketch frames.

A tilted source circle can project to an ellipse; declare `ellipse` for that
case. Degenerate projections, mismatched kinds, multiple/missing edges, general
splines and partial elliptic projections fail explicitly. Conic type checks
use floating-point-scale tolerances rather than a modelling-length tolerance.
Native reconstruction and validity/precision checks still apply. Full imported
ellipses can subsequently use the existing saved trim/extend profile operations.

## Frame and runtime API

Projection uses the same orthonormal frame as native sketch construction,
including explicit, datum-plane and face-attached placement. Source points
are transformed into local XY by subtracting the frame origin and taking dot
products with X/Y. The plane-normal component is discarded. This is
orthogonal reference projection, not hidden-line removal or an intersection.

Source definitions retain their projection declarations and initial guesses.
Feature execution creates a temporary resolved sketch with fixed projected
coordinates before solving and building native profiles.
`PartInstance::resolved_sketches(session, generated)` returns `ResolvedSketch`
snapshots (definition plus resolved plane), indexed by feature ID, with
per-feature errors. `SketchProjection`, `SketchProjectionKind`, `ResolvedSketch`
and `ResolvedSketches` are exported Rust types. Runtime snapshots are inspection
values; original definitions are the linked model to save and edit.

Calling standalone solve/diagnostic/preview methods on an unresolved projected
sketch returns a resolution error. Use the resolved snapshot for those local
operations. Source definitions are structurally checked against declared
entity/point types; placeholder coordinates used for that check are never
used as solved or exported geometry.

Expansion and point-index storage are O(sketch size + projections). Native
query cost follows semantic selection. Each analytic projection takes O(1)
work, including normalized 2×2 principal-axis extraction for conics. Bulk
snapshot inspection builds feature, datum and parameter indexes once. No
native query handles are retained. Solver work follows existing sparse row
widths and elimination fill.

## Pocket example and viewer

The example attaches a sketch to the block's top face and projects its
maximum-Y edge as `front-edge`. A guide point lies on the projected line and
a fixed construction axis. A midpoint relation puts `mid` at the centre of
the top profile edge; a vertical construction line and distance constraint
place that edge 4 mm below the projected reference, with a 20 × 12 mm rectangular profile. The initial point
coordinates are solver guesses; they contain no block-depth expression.

Changing block depth from 40 to 50 mm moves the projected reference from
local Y=20 to Y=25 and the pocket's family-space range from Y=24..36 to
Y=34..46. The rectangle remains 20 × 12 mm and the downward cut remains 5 mm.
The final volume changes from 46800 to 58800 mm³.

```sh
LD_LIBRARY_PATH="$PWD/build" rust/occt-parametric/target/release/occt-model \
  --visualize tools/model/projected-pocket.request.json /tmp/projected-pocket
LD_LIBRARY_PATH="$PWD/build" rust/occt-parametric/target/release/occt-view \
  /tmp/projected-pocket/model.json --serve --output body --port 0 --no-open
```

The viewer marks imported curves as `external` and draws them in teal.
Projection annotations identify the source and link selector parameters plus
parameters directly referenced by the source feature's operation. Source
constraints remain inspectable beside the native profile and final solid.
Rejected references report errors and preserve previously accepted geometry.
For STEP/STL export select only `body` in an `occb-model-request-v1` request.

Tests cover native lines/arcs/circles, tilted ellipses, imported endpoints in
profiles, source-driven pocket movement, named persistent references through
placement, conflicts, degeneracy, unsupported splines, large-scale conic type
checks, migration and handle cleanup. The 1,000-sketch generation/snapshot/
source-edit gate passes in 0.936 s (10 s budget). MCP generates 20 pocket bodies,
20 native profile faces and 20 source sketch scenes in 0.242 s (10 s budget).
Live depth edits, failed-edit retention, revert and STEP/STL exports pass.

Exact spline projection needs an explicit spline representation beyond the
current interpolated-point sketch primitive. Partial elliptic imports and
references across assembly instances remain future work.
