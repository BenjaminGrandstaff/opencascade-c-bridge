# Using the engine from an AI design system

An external AI design system can use this project as its geometry and
verification backend through [MCP](../tools/model/MCP.md). The external system
supplies design intent, references, construction knowledge and a plan. This
engine builds editable geometry, evaluates declared requirements and returns
measurements, diagnostics, views and exports.

This is a usage pattern for consumers of the existing APIs. This repository
does not implement a general agent harness, domain knowledge graph or research
service. Those systems can evolve independently of the CAD engine.

## Responsibility boundary

| Responsibility | External design system | This project |
| --- | --- | --- |
| Design intent | Understand the brief, style and intended use; resolve ambiguity | Represent supplied dimensions, parameters and requirements |
| Domain context | Gather references and construction knowledge; retain sources and uncertainty | Expose typed geometry, native catalogs and generated schemas |
| Planning | Decompose the object, choose component recipes and coordinate optional specialist agents | Execute feature graphs and report dependency or geometry failures |
| Design relationships | Explain why parts exist and how they should fit or move | Represent actual instances, placements, joints, constraints and named references |
| Review | Judge composition, realism and fidelity to references; decide what to change | Return rendered views, sections, measurements and verification evidence |
| Iteration | Maintain the design plan and resolve conflicting proposals | Preview guarded edits, preserve accepted builds and support guarded studio updates |

The engine's [material](MATERIALS.md) and [standard-part](STANDARD_PARTS.md)
catalogs remain part of its typed modeling interface. An external knowledge
graph should refer to their identifiers and sourced dimensions where applicable,
rather than maintain a conflicting copy of those facts.

## Context that helps an AI build a detailed object

A useful external context package contains more than an object name:

- A design brief with scale, dimensions, units, intended use and acceptance criteria.
- References with provenance, including which details are observed, inferred or invented.
- A component graph describing parts, their functions and their relationships.
- Construction recipes with parameters, connection details and applicable limits.
- A record linking design components to actual model feature and instance IDs.
- Review findings and unresolved assumptions, tied to the model version inspected.

For example, a furniture graph might express that a frame supports a tabletop,
an axle rotates in bearings, a crank drives the axle and brackets fasten the
bearings to the frame. Its records can include dimensions, materials, clearances
and source references. The AI uses those relationships to select and parameterize
geometry operations. The graph is external context; it is not an additional
`ModelDocument` field or a request format accepted by this engine.

Specialist agents are optional. A caller might separate reference research,
construction planning, geometry authoring and review, but a single agent can
perform the same workflow. Coordination and context retrieval belong to the
caller. Agents should share a specific design brief and accepted model state,
rather than independently invent incompatible dimensions or assemblies.

## Example: detailed tabletop dungeon furniture

A basic rack or workbench can look simplistic even when the geometry engine
already supports the required details. An external design system can improve
the result by supplying a more complete construction plan:

| Component | Context supplied by the caller | Existing geometry operations |
| --- | --- | --- |
| Timber frame | Beam sizes, leg positions, joints and load-bearing relationships | Sketches, extrusions, Boolean cuts, fillets and chamfers |
| Planked surface | Board dimensions, spacing, edge treatment and selected damage | Repeated instances or patterns, shaped profiles and local cuts |
| Rollers and crank | Axle diameter, bearing positions, crank offset and desired movement | Cylinders, revolved profiles, placements and assembly joints |
| Brackets and fasteners | Attachment locations, thicknesses and selected fastener sizes | Extrusions, holes, patterns and catalog standard parts |
| Shackles and restraints | Loop shapes, attachment points and minimum feature dimensions | Sketches, sweeps, revolutions, cuts and separate components |

The caller chooses a level of detail appropriate to the output. A tabletop print
needs readable forms and durable details at miniature scale; a larger display
model can support different proportions and finer features. Decorative damage
should be intentional and distinguishable from a geometry error.

Detailed constructed furniture does not inherently require a new sculpting
backend. Before identifying an engine gap, check whether the missing detail is
absent from the brief, construction plan or authored feature graph. The same
pattern applies to architectural props, fixtures and other assemblies built
from recognizable components.

## Build, inspect and revise through MCP

1. Read `occt://catalog`, the applicable schemas and examples to discover the
   running installation's capabilities. Keep research and construction context
   in the external system, and record relevant assumptions in the model.
2. Translate the component plan into named features, instances, constraints and
   declared requirements. Use native catalog identifiers when appropriate.
3. Call `occt_build` with selected outputs and explicit units. Inspect the
   verification evidence, including failed checks, before accepting a candidate.
4. Request `renders` for visual review, including useful standard views and
   sections. Use `occt_inspect_model` or `occt_inspect_build` for measured geometry
   and selector queries. Visual fidelity and declared geometric correctness
   are separate review criteria.
5. Revise with `occt_preview_edit_build` and `occt_edit_build`, using the inspected
   source model hash and stable feature IDs. Keep the external graph's model
   references synchronized with accepted revisions.
6. When collaborating with a person, use `occt_session_read` and
   `occt_session_apply` as described in [shared studio sessions](../tools/model/MCP.md#shared-studio-sessions).
   Re-read and reconcile the design after a hash conflict.
7. Return the accepted editable model and requested exports, together with
   assumptions and remaining review findings.

See [the model contract](../tools/model/README.md),
[rendered views and sections](../tools/model/README.md#rendered-views-and-sections)
and [design command workflows](../tools/model/MCP.md#drawing-inspection-and-mechanism-workflow).
The engine verifies the requirements actually declared and supported by its
checks; it does not automatically certify historical accuracy, visual quality,
printability or structural suitability from a natural-language brief.
