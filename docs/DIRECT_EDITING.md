# Direct editing

Schema 183 adds three features that change a solid's faces directly instead
of editing the features that made them. They work on any solid, including an
imported STEP body with no history, and they are ordinary timeline features:
parameters drive them, later selectors follow their faces, and
[feature history](FEATURE_HISTORY.md) can check them out, suppress them or
move them. Native support is ABI 86 (`occt_bridge_remove_faces`,
`occt_bridge_offset_faces`).

| Operation | What it does | Kernel |
|---|---|---|
| `remove_faces` | Deletes the selected faces and closes the gap from the faces around them: a hole, pocket, boss, fillet or chamfer disappears as if it was never modeled. | OCCT defeaturing (`BRepAlgoAPI_Defeaturing`) |
| `offset_faces` | Moves the selected faces along their outward normals by a signed `distance`; the faces around them extend or trim to meet, keeping their own surfaces (drafted sides stay drafted). | Per-face skin offset (`BRepOffset_MakeOffset`) |
| `move_faces` | Moves the material the selected faces bound, such as a hole, pocket, boss or rib, by an `offset` vector. | Removal, then booleans |

Each takes the `input` solid and a list of face selectors (the
`FaceSelector` schema from `occt_get_schema`).
Selectors that pick the same face count it once. The input must be one valid
solid, and the result must be one valid solid with positive volume.

```json
{ "id": "raised", "operation": { "offset_faces": {
    "input": "drilled",
    "faces": [{ "at_extreme": { "axis": "z", "extremum": "maximum",
                "tolerance": { "literal": { "value": 1e-6, "dimension": "length", "unit": "millimeter" } } } }],
    "distance": { "parameter": "lift" },
    "tolerance": { "literal": { "value": 1e-6, "dimension": "length", "unit": "millimeter" } } } } }
```

## `remove_faces`

Select every face of the detail: the wall of a through hole, the side and
cap of a boss, all faces of a fillet chain. If the kernel cannot remove the
selection completely, the feature fails. It never returns a partly edited
solid. Removing an outer face whose neighbours never meet again, such as the
top of a plain block, also fails. The error names the OCCT alert, for example
`BOPAlgo_AlertUnableToRemoveTheFeature`, and the face selector at fault.

## `offset_faces`

`distance` is a signed, nonzero length. Positive adds material: a top face
rises and an end face moves out. On a hole's wall the outward normal points
at the axis, so a negative distance widens the hole and a positive one
narrows it. `tolerance` is a positive length, as for `offset`. All selected
faces move by the same distance; chain features for different distances.

History records every face, edge and vertex as modified, not generated. A
`persistent` selector on an earlier feature therefore still finds a face after
the offset, and so does any later edit.

## `move_faces`

`offset` is a nonzero length vector, and may use parameters per component.
The feature first removes the selected faces, as `remove_faces` does. The
region that removal filled in (a hole) is then cut at the moved place, and the
region it took away (a boss) is fused there. The history of every step is
composed, so faces that did not move keep their identity. The moved detail
should stay inside the body: a hole moved past an edge becomes a notch, and a
boss moved off the body becomes a separate solid, which is an error.

## Example

[`direct-edit`](../tools/model/direct-edit.request.json) drills a Ø8 hole in
a 60 × 40 × 20 plate, raises the top by `lift` with `offset_faces`, and moves
the hole by `shift` along x with `move_faces`. The hole is selected with a
`persistent` selector on `drilled`, which follows its wall through the
offset. A required volume range checks the result. With `lift` = 8 the plate
is built exactly, then rejected by that check.

## Studio

The solid editor lists **Delete faces**, **Offset faces** and **Move faces**
as direct edits. **Pick a face to edit** accepts curved faces too, such as
hole walls and fillets. A curved face is anchored at its centre: if a later
change moves that face, the selector fails instead of choosing a neighbour.
Repick it in that case. Preview checks every instance before Apply.

## Tests and scale

- Bridge: exact volumes for removing a hole and a pair of fillets, pushing and
  pulling faces, widening a hole, and offsetting a top face between drafted
  sides. Bad selections, zero or non-finite distances, duplicate faces, and
  removals that cannot heal are all refused.
- Kernel: the same edits through feature definitions, with a parameter edit,
  a moved hole found again at its new centre, a moved boss, and errors that
  name their selector.
- Studio: picking a curved face, and previewing each edit.
- Command: the example under parameter edits.

## Limits and scale

One `remove_faces` or `move_faces` feature selects at most 100 faces, and one
`offset_faces` feature at most 10,000. A larger selection is refused before
the kernel runs; split it into several features. OCCT defeaturing reworks the
whole solid for each detail it removes, so its time grows faster than
linearly. The scale suite (`--direct-edit-only`, release build) measures, on
perforated plates:

| Edit | Time |
|---|---|
| remove 100 hole walls (one feature) | 12.4 s |
| move 100 hole walls (one feature) | 12.8 s |
| offset 400 hole walls (one feature) | 2.2 s |

The suite's budget is 80 s for all three. A faster removal would work on each
detail inside a box around it, then apply the changes to the solid with one
boolean. A prototype was four times faster at 100 holes, but still quadratic,
and it left seam faces that needed unifying. It is tracked on the roadmap.
