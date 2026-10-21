# Feature documentation

One page per modeling capability: its API, persisted schema, limits, and
examples. Project-wide status lives in [the roadmap](../ROADMAP.md) and
[the parametric architecture](../PARAMETRIC_ARCHITECTURE.md).

## Sketches and profiles

- [Sketch geometry, constraints and profile edits](SKETCHES.md)
- [Hollow sketch profiles](PLANAR_REGIONS.md)
- [Sketches attached to planar faces](FACE_SKETCHES.md)
- [Linked edge projections into sketches](SKETCH_PROJECTIONS.md)

## Features

- [Extrusion end conditions](EXTRUSIONS.md)
- [Symmetric sketch revolutions](REVOLUTIONS.md)
- [Lofts from saved sketch profiles](LOFTS.md)
- [Sweeping hollow sketch profiles](HOLLOW_SWEEPS.md)
- [Advanced sweeps: twist, scale, guides, sections](ADVANCED_SWEEPS.md)
- [Hole bottoms and drill points](HOLES.md)
- [Hole-size catalog](HOLE_SIZE_CATALOG.md)
- [Modeled threads](THREADS.md)
- [Helices, springs and coils](HELIX.md)
- [Fillets and chamfers](EDGE_TREATMENTS.md)
- [Signed skin offsets](OFFSETS.md)
- [Uniform geometry scaling](SCALING.md)
- [Mirrored parts](MIRRORS.md)
- [Split bodies](SPLIT_BODY.md)
- [Thickened sheets](THICKEN.md)
- [Compound tool groups](COMPOUNDS.md)
- [Linear patterns inside a part](PART_PATTERNS.md)
- [Sheet metal](SHEET_METAL.md)

## Assemblies and verification

- [Joints, collision checks, and sampled motion](ASSEMBLY_MOTION.md)
- [Requirement rules](REQUIREMENTS.md)
- [Mesh hand-off and manufacturing screening](MESH_HANDOFF.md)

## Drawings and documents

- [Generated drawings](DRAWINGS.md)
- [Model history and change impact](MODEL_HISTORY.md)
- [Feature timeline: checkout, suppression and reordering](FEATURE_HISTORY.md)
- [Semantic document comparisons and merges](DOCUMENT_DIFF.md)

## AI authoring and local tools

- [Using the engine from an external AI design system](AI_DESIGN_SYSTEMS.md)
- [Five printable dungeon maps through MCP](PRINTABLE_MAPS.md)
- [Model build, edit and repair requests](../tools/model/README.md)
- [MCP setup, generated schemas and live capability catalog](../tools/model/MCP.md#launch-and-connect)
- [Drawing, inspection and mechanism workflows](../tools/model/MCP.md#drawing-inspection-and-mechanism-workflow)
- [Interactive annotated sketch and solid viewer](../tools/model/VIEWER.md)
- [Drawing export CLI](../tools/drawing-export/README.md)
- [Motion study CLI](../tools/motion-study/README.md)
- [Mass and balance CLI](../tools/balance-report/README.md)
- [Alternative linkage branches CLI](../tools/joint-branches/README.md)

MCP tools use the existing engine and command implementations. Generated request
schemas and complete examples are discoverable at runtime through `occt://catalog`.
Analysis reports remain separate from accepted part builds; a merged document or
alternative pose needs a subsequent build to verify geometry and requirements.
