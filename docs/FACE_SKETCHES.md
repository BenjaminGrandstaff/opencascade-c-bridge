# Sketches attached to planar faces

Schema 93 adds optional `SketchDefinition.face_support`. Native ABI remains 52;
attachment uses the existing semantic selectors, planarity, oriented normal
and area-centre queries. Older sketches keep `face_support: None` after load
and retain their existing explicit or datum-plane placement.

```json
"face_support": {
  "input": "block",
  "face": {
    "normal_aligned": {
      "direction": {"literal": {
        "x": {"value": 0, "dimension": "scalar", "unit": null},
        "y": {"value": 0, "dimension": "scalar", "unit": null},
        "z": {"value": 1, "dimension": "scalar", "unit": null}
      }},
      "minimum_dot": {"literal": {"value": 0.999999, "dimension": "scalar", "unit": null}}
    }
  },
  "offset": {"parameter": "support_offset"}
}
```

## Frame and dependency rules

`input` names another feature output. The face selector must resolve exactly
one planar face; named and persistent face references are supported. Missing,
ambiguous and nonplanar supports fail rather than choosing another face.
Source and selector-history dependencies participate in feature ordering and
cycle detection. Selector expressions, named-reference definitions and the
length-valued offset participate in signatures; source edits rebuild attached
sketches and downstream features.

Local sketch zero is the selected face's area centre plus `offset` along its
oriented normal. Offset defaults to zero millimetres and may be signed.
`x_axis` remains an explicit dimensionless direction in family coordinates and
must lie in the support plane; `y_axis` becomes `normal × x_axis`, normalized.
`origin` and the supplied `y_axis` are replaced by that resolved frame.
`datum_plane` and `face_support` are mutually exclusive.

The origin follows the face centre, which can move when its trimmed area
changes. This is not a persistent UV coordinate or a corner anchor. The
attachment defines an infinite sketch plane; it does not clip or constrain
the profile to the face boundary. Extrusion directions remain explicit and
do not automatically follow the support normal. Changing support orientation
may require an updated X axis and downstream sweep direction.

Constraints solve in sketch-local XY, then native wires/faces are constructed
in the resolved plane. SketchFace, SketchWire and SketchOpenWire share the
attachment path. Failed builds release temporary handles and preserve prior
accepted results.

## Example and viewer

The [face-pocket example](../tools/model/face-pocket.request.json) attaches a
20 × 12 mm rectangular sketch to a 60 × 40 × 20 mm block's top face, extrudes
a cutter 5 mm down and cuts the block. The sketch centre is `[30,20,20]` mm;
the pocketed block's volume is 46800 mm³. Increasing block height to 30 mm
moves the sketch to Z=30 and preserves the 5 mm cut depth.

```sh
LD_LIBRARY_PATH="$PWD/build" rust/occt-parametric/target/release/occt-model \
  --visualize tools/model/face-pocket.request.json /tmp/face-pocket
LD_LIBRARY_PATH="$PWD/build" rust/occt-parametric/target/release/occt-view \
  /tmp/face-pocket/model.json --serve --output body --port 0 --no-open
```

The standalone example shows the final body, the native attached profile face
in 3D, and its local source sketch. Source sketch scenes remain in local XY;
`face_support` metadata records the family-space origin, normal, source
definition and resolution status. A side-panel support annotation links
selector/offset expression controls. Failed diagnostic builds keep source
equations visible and mark attachment metadata unavailable with the error.
The live viewer verifies height edits, rejected edits and revert.

For STEP/STL exports use `occb-model-request-v1`, remove `sketches`, enable
`step`/`stl`, and select just `body` (one output per instance). Source sketches
and definitions remain in the saved model.

## API and scaling

`SketchFaceSupport` and `SketchSupportPlanes` are exported Rust types.
`PartInstance::sketch_support_planes(session, generated)` resolves the attached
planes/errors for an already generated local result. It builds feature and
parameter indexes once, costing O(features + parameter resolution + total
selector/native query cost), with one value/error per attachment and bounded
query temporaries. It does not retain native face handles. Feature execution
uses the existing build indexes; the viewer reuses one diagnostic generation
per instance for solid scenes and support metadata.

Tests cover height and offset edits, analytical pocket volume, named persistent
references through translation/rotation, invalid X axes, support ambiguity,
missing/self references, nonplanar faces, unit errors, rejected-edit retention
and handle cleanup. The 1,000-profile generation/plane-query/height-edit gate
passes in 0.694 s (10 s budget). MCP generates 20 pocket bodies, 20 attached
profile faces and 20 local sketch scenes in 0.231 s (10 s budget).

Linked analytic external geometry is now available in schema 94; see
[sketch projections](SKETCH_PROJECTIONS.md). Face-local UV/corner anchors and
automatic normal-relative extrusion definitions remain future work.
