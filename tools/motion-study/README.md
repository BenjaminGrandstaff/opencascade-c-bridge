# Hinge assemblies and interference studies

`occt-motion-study` turns selected outputs in an existing parametric model into
separate fixed or revolute components. It accepts any model family, including
the printable wing export, and writes sampled collision reports plus continuous
checks between samples. The source model is preserved.

Build against the repository's native library:

```sh
OCCT_BRIDGE_LIB_DIR="$PWD/build/bench" \
  cargo build --release --manifest-path rust/occt-parametric/Cargo.toml \
  --bin occt-motion-study
LD_LIBRARY_PATH="$PWD/build/bench" \
  rust/occt-parametric/target/release/occt-motion-study \
  wing.model.json tools/motion-study/elevon-right.json /tmp/right-elevon-study
```

The output directory must be new. The command writes:

- `assembly.model.json`: a reloadable model with cloned components, enclosing
  frames and bounded revolute joints, initially at each hinge's start angle;
- `study.json`: the selected outputs and coordinated joint positions in the
  engine's `MotionStudy` format;
- `motion.report.json`: sampled collisions and relationship checks, plus
  continuous status, witness points, overlap volumes and unresolved intervals.

Exit codes are **0** for clear travel with satisfied sampled relationships,
**2** for a completed report containing contact, interference, inadequate
clearance, unresolved paths or failed relationships, and **1** for invalid input
or an execution error. A completed nonclear report is still written. Invalid
inputs and geometry failures create no output directory. Existing artifacts
are never overwritten; disk errors during publication may leave partial output.

## Setup format

```json
{
  "schema": "occb-motion-study-v1",
  "samples": 11,
  "components": [
    { "id": "fixed", "source": "prototype", "output": "body" },
    {
      "id": "moving", "source": "prototype", "output": "flap",
      "hinge": {
        "origin_mm": [100, 0, 0],
        "axis": [0, 1, 0],
        "minimum_deg": -25,
        "maximum_deg": 25,
        "start_deg": -25,
        "end_deg": 25
      }
    }
  ]
}
```

`source` names an instance in the input document. `output` names one of its
final solid feature outputs. Each component gets a new unique instance ID;
fixed components omit `hinge`. Clones retain inherited parameters and materials,
the source's local placement, and its enclosing frame hierarchy. Original
instances remain as prototypes in the resulting document. The study selects
only the new components; use those same selections for mass reports and exports
to avoid counting prototypes or intermediate outputs as extra parts.

Hinge origins and axes are expressed in the source instance's enclosing frame,
after its local placement. Origins use millimeters, axes are dimensionless
nonzero vectors, and setup angles use degrees. All hinge coordinates advance
together at evenly spaced fractions from their independent start to end angles.
Travel is unwrapped: 0 to 360 degrees is a full turn. Both endpoints must satisfy
the supplied limits. Setup files reject unknown fields.

Optional `collision_options` and `continuous_options` use the engine's serialized
types; omit them for defaults. Contact is a nonclear result. All selected
components are checked against each other, including fixed components. Choose
separate studies for independent mechanisms when intentional fixed-to-fixed
contact would obscure the result. No force, aerodynamic or constrained-linkage
simulation is performed. Continuous checks use bounded floating-point BREP
queries; inspect unresolved intervals as described in
[Assembly motion](../../ASSEMBLY_MOTION.md).

Limits are 2–10,000 samples, 1–10,000 components and one million sampled hinge
coordinates. Joint insertion is batched; preparing 10,000 hinges takes about
0.15 s in the test build (10 s budget). Sample report storage grows with sampled
positions and collisions. Geometry is shared per parameter variant within each
of the sampled and continuous passes; these two passes generate independently.

## Wing examples

`elevon-right.json` and `elevon-left.json` use the starter layout in
[`tools/wing-layout/example.json`](../wing-layout/example.json) and the structure
settings shown in the [wing README](../wing-layout/README.md): elevon span
fractions 0.5–0.95, hinge fraction 0.75, and a 1 mm chordwise gap. First export
that structure with `occb-wing-cad --build`; supply its `.model.json` here.
Each study compares one elevon with its own fixed wing half through ±25 degrees.
The mirrored axes and angle signs give mirrored deflections.

These are explicit assumed straight hinge lines joining the two chord-line
points, not inferred hinge hardware. The current wing cutout has no spanwise
end gaps, so contact or interference is an expected finding to inspect. The
examples report that geometry; they do not certify a buildable hinge. Both
11-sample starter studies returned interference at nonzero deflections and
contact at neutral, with no unresolved pairs, during verification. Update
origins, axes and output selections when station geometry or structure settings
change. Hinge definitions are stored values, not parameter-driven expressions.
