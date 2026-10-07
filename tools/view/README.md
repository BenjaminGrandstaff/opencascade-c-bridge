# Model viewer

`occt-view` regenerates a saved model document and shows every part, named
after its instance and colored by its material appearance: in OCCT's DRAW
viewer, or with `--serve` in a browser page that also edits the family's
parameters.

```sh
OCCT_BRIDGE_LIB_DIR="$PWD/build" LD_LIBRARY_PATH="$PWD/build" \
  cargo run --manifest-path rust/occt-parametric/Cargo.toml --bin occt-view -- MODEL.json
```

The command prints the generated `view.tcl` path and starts
`DRAWEXE -i -f view.tcl` without waiting, so the window stays open after the
command exits. DRAW prints which DRAW name belongs to which instance.

| Option | Effect |
|---|---|
| `--output NAME` | Show this feature output of every instance that has it. Default: the primary family's last feature. |
| `--dir NEW_DIRECTORY` | Write `model.brep` and `view.tcl` here; the directory must not exist. Default: a fresh directory under the system temporary directory. |
| `--no-open` | Write the view without starting DRAW. |
| `--watch` | Keep running: after each save of the model file, regenerate and reload the open viewer, keeping the camera. Close the viewer to stop. |

Set `OCCT_VIEW_DRAWEXE` to use a DRAW executable that is not on `PATH`. If the
viewer cannot start, the view is still written and the error shows the command
to open it.

Colors come from the document's material appearances. Set one in code with
`InstanceGraph::set_material_appearance(material, Some(appearance))`; parts
without a material use DRAW's default color.

## Live editing

With `--watch`, edit the model document in any editor and save: the command
regenerates it and sends `source reload.tcl` to the viewer, which swaps in the
new parts (added and removed instances included) without moving the camera.
A change is read once two polls 250 ms apart agree, so a save in progress is
not read. A save that does not load or regenerate prints the error and keeps
the previous view; the next good save reloads. With `--no-open --watch` the
view files are rewritten on each save for another viewer to reload.

Each export also writes `reload.tcl` next to `view.tcl`; it is what `--watch`
sends, and any DRAW session showing that directory can run it after the files
are rewritten.

In DRAW, `vfit` refits the view and `vdisplay -dispMode 0 NAME` switches a part
to wireframe.

## Browser viewer and parameter editing

```sh
OCCT_BRIDGE_LIB_DIR="$PWD/build" LD_LIBRARY_PATH="$PWD/build" \
  cargo run --manifest-path rust/occt-parametric/Cargo.toml --bin occt-view -- MODEL.json --serve
```

The command prints `http://127.0.0.1:8791/` (choose another port with
`--port`, or `--port 0` for any free one) and opens it with `xdg-open` unless
`--no-open` is given. It serves until stopped with Ctrl-C. `--output` works as
above; `--watch` and `--dir` do not apply.

The page opens with **Dimensions & sketches**: annotated 3D geometry and sketch
views of the instance selected under **Editing**, or the first instance when
editing family defaults. Choose its solid or a sketch from the inner **View**
list. Click a dimension or constraint label to highlight related geometry,
inspect native measurements/residuals, and edit its linked parameters directly
in the selection panel. Corresponding fields in the main parameter panel are
highlighted. Values without a declared driving parameter remain measurements.

An edit regenerates the geometry, dimensions and constraint checks together,
keeping the selected annotation and camera when its feature remains available.
Linked parameter fields use the parameter's actual unit, including for
multi-parameter formulas; they do not silently invert a dimension expression.
Native checks and geometry are shared with the standalone
[dimension viewer](../model/VIEWER.md).

Choose **Assembly** to see every placed part and material color in the WebGL
glTF view: click a part to select it, drag to orbit, Shift-drag or right-drag to
pan, and wheel to zoom. The annotated views use **family-local coordinates**;
Assembly shows placements and frames. The selected part (or the first editable
part under Family defaults) now carries selectable dimension and check labels.
Click a driving label to edit its linked controls directly in the side panel,
without leaving Assembly; measured spans remain read-only. Dimensions and
Checks toggles hide/show the overlays, and Escape clears the annotation
selection. Labels follow orbit/pan/zoom and the part's placement, frame rotation
and offset, while their native measurement values remain unchanged by placement.
A rejected edit opens its marked diagnostic view instead of drawing failed
candidate annotations on accepted assembly geometry.

**Fit view** applies to the current view. Native glTF nodes include a
`extras.familyLocalMatrix` transform for annotations. It differs from the render
matrix: shared tessellations are recentered on a placed representative, so
native family-local anchors need their own full placement/frames transform.
The JavaScript projector clips anchors behind the camera or outside its depth
range; labels render over the geometry.
The side panel still edits defaults, instance overrides and placements, and
supports Add copy, Delete, Save and Revert. Scalars and integers get number
fields/sliders, booleans a checkbox, and choices a list; vectors are edited in
the file.

- An edit changes the parameter's default in the family, regenerates and
  redraws within a moment, keeping the camera. Edits to several parameters
  merge, and one request is in flight at a time, so none is lost.
- An edit that fails validation or regeneration (outside a parameter's limits,
  for example) is rejected as a whole: the field reverts, the message stays
  in the panel, and the model is unchanged. When the attempted geometry can be
  visualized, the annotated view shows a **Rejected edit preview** with failed
  constraints/checks in red. An unavailable solid switches to the failing
  sketch when possible. The preview is never accepted or saved; Revert or a
  successful edit restores the accepted view. Invalid parameter/model data
  that cannot produce a diagnostic preview retains the prior accepted view.
- **Save** writes the edited document over the model file, through a
  temporary file and a rename so other readers never see a partial file.
  Nothing is written until then.
- A save made elsewhere, by an editor or another tool, reloads the page within
  about a second once two checks agree. It replaces unsaved browser edits. A
  file that does not load is reported and the last model stays.

### Editing one instance

The **Editing** list chooses what the fields change: *Family defaults* (every
instance without its own value) or one instance of the primary family.
Clicking a part selects its instance and fades the others; clicking empty
space returns to the family defaults.

For an instance, each field shows its effective value, tagged **override**
when the instance sets it itself (with **Reset** to remove it) or
**inherited** when it comes from a clone source; untagged values are the
family default. Editing a field sets the instance's own override, keeping the
default's unit. The list shows how many overrides each instance has.
Instances with overrides become their own generated variants, so they no
longer share a mesh with their siblings.

Below the parameters, **Placement** shows the instance's translation and
rotation (axis, angle in degrees, and the point the axis passes through) in
millimeters, relative to its assembly frame when it has one. Changing a field
moves the instance; an angle of 0 removes the rotation. Pattern members are
placed by their pattern's rule, so a placement edit on a member is refused
with a message instead of being silently undone.

**Add copy** adds a clone of the selected instance: it inherits the
instance's parameters and material, sits in the same frame, and is placed
one part-width (plus a quarter) further along X; the copy is selected. Its
id is the source's id with `-copy` (then `-copy2`, …). **Delete** removes the
selected instance with its own material assignment. An instance that others
are cloned from, or that belongs to a pattern, cannot be deleted; the message
names what depends on it. Other references, such as relationships or
drawing annotations, are caught by the model's validation.

### Requirement results

The panel's **Requirements** section shows the model's requirement checks
from the same regeneration that drew it, failures first, with each
requirement's priority when it is not *required* and its statement on hover.

- With *Family defaults*, it lists every assembly check, and for each part
  requirement whether it passes on all instances or which ones fail.
- With an instance selected, it lists that instance's own checks and the
  assembly checks that name it, noting failing assembly checks elsewhere.
- Failing assembly checks that report where they fail (clearance and
  interference witnesses, for example) are marked with red dots drawn over the
  parts.

A *required* failure rejects the edit or file that caused it, as loading the
model would. Assembly and saved geometry always meet required checks; an
explicitly marked rejected-edit preview may show the failed candidate. Results are shared by instances with identical parameters, so a
10,000-instance report is about 250 KB.

**Revert** discards every unsaved change and reloads the file, so a deletion
or any other edit can be undone until **Save**. The page needs no external files. The server listens only on
127.0.0.1, accepts only `127.0.0.1` or `localhost` Host headers, and accepts
edits only with the `X-OCCT-View` header that the page sends, which other
sites' pages cannot add; it is a local editing tool, not something to expose.

Each edit costs one full regeneration and glTF export. On a 10,000-instance
model, startup takes 0.8 s, listing instances about 26 ms, one instance's
values 33 ms, the requirement report 2 ms, and an override edit with
regeneration and assembly checks 0.74 s. Instances sharing a
generated variant are tessellated once, so 10,000 pattern members export in
about a quarter second. The renderer's tests run with
`node --test rust/occt-parametric/src/bin/view/web/*.test.mjs`.
Annotation work is lazy and bounded per selected instance, with one cached
result keyed by model version. Late requests cannot replace a newer instance
or pair new geometry with older controls. Save waits for queued edits to finish.
Native tests cover live sketch/solid updates, override isolation, save/revert,
and rejected-constraint previews without acceptance. Browser automation was
unavailable; the real page logic and shared renderer run against minimal DOM
hosts in the JavaScript tests.
