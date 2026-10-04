# Hinge and slider assembly studies

`occt-motion-study` turns selected outputs in an existing parametric model into
separate fixed, revolute or prismatic components. It accepts any model family, including
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
  frames and bounded revolute/prismatic joints at their start coordinates;
- `study.json`: the selected outputs and coordinated joint positions in the
  engine's `MotionStudy` format;
- `motion.report.json`: sampled collisions and relationship checks, plus
  the explicit excluded pairs and continuous status, witness points, overlap volumes and unresolved intervals.

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
fixed components omit both `hinge` and `slider`; a moving component supplies
exactly one of them. Clones retain inherited parameters and materials,
the source's local placement, and its enclosing frame hierarchy. Original
instances remain as prototypes in the resulting document. The study selects
only the new components; use those same selections for mass reports and exports
to avoid counting prototypes or intermediate outputs as extra parts.

Hinge origins and axes are expressed in the source instance's enclosing frame,
after its local placement. Origins use millimeters, axes are dimensionless
nonzero vectors, and setup angles use degrees. All hinge and slider coordinates advance
together at evenly spaced fractions from their independent start to end angles.
Travel is unwrapped: 0 to 360 degrees is a full turn. Both endpoints must satisfy
the supplied limits. Setup files reject unknown fields.

A slider uses the same component fields and a `slider` block:

```json
{
  "id": "carriage", "source": "prototype", "output": "body",
  "slider": {
    "axis": [1, 0, 0],
    "minimum_mm": -50,
    "maximum_mm": 50,
    "start_mm": 20,
    "end_mm": -20
  }
}
```

The axis is a dimensionless nonzero direction in the source's enclosing frame;
the engine normalizes it. Travel is an offset in millimeters from the cloned
source placement. Reverse travel is supported. Parent frame rotations rotate
the sliding direction as well as the part. Both endpoints must satisfy finite
limits. Hinge and slider coordinates advance at the same sample fractions, with
radian and millimeter values stored in `study.json`. Components specifying both
kinds reject. Existing hinge setups retain their frame IDs and behavior.

[`slider-example.json`](slider-example.json) compares a sliding clone with a
fixed clone of `prototype:body` over +20 to -20 mm using two samples. For a
10 mm wide box, both endpoints are clear and the continuous check detects the
crossing between them. Change source IDs, outputs, axes and travel for your model.

Optional `collision_options` and `continuous_options` use the engine's serialized
types; omit them for defaults. Contact is a nonclear result. All selected
components are checked against each other, including fixed components. Optional
`excluded_pairs` names unordered pairs of component IDs:

```json
"excluded_pairs": [["fixed", "bearing"]]
```

Use this for intentional contact between assembled components. Both IDs must
appear in `components`; self-pairs, duplicates (including reversed duplicates)
and unknown IDs reject. A pair exclusion skips all interactions between those
components for the entire study, including interference and clearance failures.
Other pairs and all relationship checks remain active. Up to one million pair
exclusions are supported. `study.json` stores the corresponding exact output
references, and `motion.report.json` always lists the exclusions, including an
empty list when all pairs were checked. A passing report applies to this scope.

No force, aerodynamic or constrained-linkage
simulation is performed. Continuous checks use bounded floating-point BREP
queries; inspect unresolved intervals as described in
[Assembly motion](../../ASSEMBLY_MOTION.md).

Limits are 2–10,000 samples, 1–10,000 components and one million sampled joint
coordinates. Joint insertion is batched; preparing 10,000 hinges or a mixed set of hinges and sliders is
checked against a 10 s budget. Sample report storage grows with sampled
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
