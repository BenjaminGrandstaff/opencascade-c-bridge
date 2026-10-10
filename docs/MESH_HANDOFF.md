# Mesh hand-off and manufacturing screening

ABI 35 exposes non-destructive surface tessellation and indexed subshape lookup.
Schema 44 stores `MeshExportDefinition` records and material appearances.
Older documents load with no mesh exports and ordinary default appearances.

## Tagged surface meshes

```rust,ignore
let definition = MeshExportDefinition {
    id: "bracket-analysis".into(),
    output: InstanceOutputRef { instance: "bracket".into(), output: "body".into() },
    settings: MeshSettings::default(),
    face_tags: vec![MeshFaceTag { id: "fixed".into(), faces: vec![fixed_face_selector] }],
    manufacturing: Some(ManufacturingSettings::default()),
};
let mesh = definition.generate(&graph, &session)?;
let surface_msh = mesh.to_msh()?;
```

The output selects explicit model geometry at its current instance/frame/joint
pose. Face selectors use resolved parameters and existing operation history.
Names must be unique, nonempty, at most 128 UTF-8 bytes, and contain no controls,
quotes, or backslashes. `__unassigned_boundary` is reserved. Different physical
groups cannot overlap; repeating a selection within one group is harmless.

Every triangle retains its source face index. Those indices belong to the
current output's unique face map; semantic tag names carry identity between
regenerations. A bulk topology lookup assigns selected faces without repeatedly
walking the face map. Exact area, surface centroid, and bounding descriptors are
included for the external volume mesher.

`Session::surface_mesh` copies geometry before meshing, preserving source BREP
and triangulation data. Count and fill calls each tessellate once. Linear
deflection is an absolute distance; the default is 0.1 mm. The bounded resolution
floor is max(1e-7 model units, largest bounding span * 1e-5). Angular deflection is
0.01 through pi radians, default 0.3. The triangle budget is 1 through 1,000,000.
Numerically degenerate pole/sliver triangles are omitted. The returned-data
budget does not bound OCCT's internal meshing workspace.

The Gmsh 2.2 ASCII surface export uses mm coordinates and physical face names.
Near-coincident seam vertices weld when each coordinate differs by at most
max(1e-9 mm, span * 1e-12), using a spatial hash. Welding that collapses a triangle fails. Meshing and welding are
approximations; use appropriate model scale and inspect solver mesh quality.

## Tetrahedral volume meshes

```rust,ignore
definition.write_fea_bundle(&graph, &session, Path::new("analysis/bracket"))?;
```

This creates a new directory containing `body.brep`, `surface.msh`, and
`volume.json`. Existing directories are preserved. A single valid solid is
required. The BREP retains exact CAD geometry; the manifest stores boundary
names and geometric face descriptors.

```bash
tools/mesh/run.sh analysis/bracket analysis/bracket-volume.msh \
  --size-mm 2 --maximum-elements 100000
```

The runner creates an isolated `build/mesh-python` environment and installs
pinned Gmsh 4.15.2 on first use. Python with `venv` and Gmsh's platform libraries
(including libGLU on Linux) are required. An existing Gmsh environment can instead
run `python tools/mesh/volume.py ...` directly.

The size is a maximum target edge length in mm. Curvature refinement requests
24 elements around a full circle; small curved boundaries can therefore produce
more elements than a uniform-size estimate. The runner uses one meshing thread,
first-order tetrahedra, and Gmsh 4.1 ASCII output. It checks the initial element
estimate, final element count, and positive inverse-condition quality. The
maximum element budget is 1 through 1,000,000 and applies to output, not Gmsh's
workspace. It prints element count, matched faces, and minimum quality.

Imported faces match by area, centroid, and bounds with spatial/area buckets.
No assumption is made about imported topology numbering. The default positional
matching tolerance is 1e-5 mm; `--face-tolerance-mm` changes it. Area matching uses
max(1e-8 mm², largest face area * 1e-7). Unmatched or ambiguous faces fail before
boundary assignment. Every surface belongs to an explicit physical group or
`__unassigned_boundary`; the solid has the export's physical name. Matching and
meshing failures publish no output. A completed mesh is published atomically
without overwriting an existing output file.

Independent Meshio parsing verified named boundaries, positive signed tetra
volumes, and analytic volumes for a box and a drilled block. The refined drilled
block has 18,568 tetrahedra and volume 5626.166 mm³ against 5623.009 mm³ analytically
(0.06% difference). The geometric matcher handles 10,000 reordered descriptors
within its three-second budget. Its tests run with:

```bash
python3 -m unittest discover -s tools/mesh/tests -v
```

## glTF rendering

```rust,ignore
graph.set_material_appearance("steel", Some(MaterialAppearance {
    base_color: [0.2, 0.3, 0.4, 1.0], // linear RGBA
    metallic: 0.9,
    roughness: 0.2,
    double_sided: false,
}))?;
let gltf = graph.export_gltf(&session, &[definition])?;
// Or the named output of every instance that has it, nodes named by instance:
let scene = graph.export_gltf_output(&session, "body", MeshSettings::default())?;
// Or from a regeneration you keep, e.g. for its verification results:
let generation = graph.regenerate_all(&session)?;
let scene = graph.export_gltf_generated(&session, &generation, "body", MeshSettings::default())?;
```

Appearance keys identify existing materials. Color, metallic, and roughness
components must be finite in [0, 1]. Clones inherit appearance through their
material assignment. Unspecified appearances use a neutral default. Rendering performs no mass
calculation. Alpha below one selects blending.

The self-contained glTF 2.0 JSON uses embedded binary buffers, flat triangle
normals, and meters. Model Z-up coordinates become glTF Y-up via `[x, z, -y]`.
Each component rebases vertices near its center before converting them to float32;
its world offset remains in the node translation. Overflow or collapsed float32
triangles fail. Identical rebased geometry and appearances share meshes and
materials. Graph generation runs once per distinct parameter variant, and so
does tessellation: untagged instances sharing a generated variant, output,
settings and appearance reuse one mesh, each node carrying the rigid transform
from the tessellated instance's placement (frames and joint motion included)
to its own, as a translation or, when rotated, a matrix. Face-tagged
definitions are tessellated per instance so their tag names stay their own. Temporary
geometry is released on success and failure.

Khronos's glTF validator reports zero errors and warnings for a box, a drilled
block, and a posed assembly with inherited appearance. A 1,000-part scene exports
in 0.009 seconds against a ten-second budget (1.393 seconds before shared
tessellation), and the 10,000-part viewer export in 0.253 seconds, with one generated variant and one
shared mesh. Mesh definitions and material appearances participate in semantic
comparison/merge; change impact lists affected mesh exports.

## Sampled manufacturing checks

```rust,ignore
let report = mesh.check_manufacturability(definition.manufacturing.unwrap_or_default())?;
```

The report contains three screens:

- Signed draft from triangle normals relative to a supplied pull direction.
  Pull-normal cap facets are excluded; side faces report their minimum sampled
  angle and whether it meets the requested minimum.
- Downward overhang beyond a supplied maximum inclination from vertical.
  Facets touching the lowest build plane are excluded. Bridging, supports,
  printer material, and process settings are not modeled.
- Inward normal rays from deterministic triangle-centroid samples. A median
  triangle BVH finds the nearest exit. Wall checks require a closed, consistently
  oriented welded mesh. Missing exits remain explicit unresolved samples.

Directions are dimensionless; wall lengths use units. Draft minima are in
[0, pi/2), overhang limits in [0, pi/2], and wall thickness is positive. The sample
budget is 1 through 20,000, default 1,000. The report labels the minimum as
`minimum_sampled_wall_mm`. Neither that value nor a passing sample set proves a
global minimum wall thickness. Facet normals can differ from exact curved
surface normals; finer tessellation improves screening resolution.

Analytic box checks, exact directional sphere-ray comparisons, bore thickness,
and tapered cone draft verify the calculations. Open or inconsistently oriented
meshes are rejected. Screening 10,000 indexed wall rays on 100,518 triangles takes
3.284 seconds against a ten-second budget. Meshing adds geometry-dependent OCCT
cost; indexing is O(T log T), with O(T) storage and ray cost depending on candidate
facets. Dense overlapping bounds can degrade ray traversal. These cases and the
1,000-part export run through `tools/bench/run.sh`.

The same screens back the `MinimumWall`, `DraftAngle`, and `Overhang` part
requirements (schema 48), which store their settings in the family and run on
every regeneration; the requirement's draft check uses each face's smallest
draft magnitude rather than the signed minimum reported here. See
[Requirement rules](REQUIREMENTS.md).

Primary references: [OCCT meshing](https://github.com/Open-Cascade-SAS/OCCT/wiki/mesh),
[Gmsh API and formats](https://gmsh.info/doc/texinfo/), and
[glTF 2.0 specification](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html).
