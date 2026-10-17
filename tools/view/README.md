# Model viewer

`occt-view` regenerates a saved model document and shows every part, named
after its instance and colored by its material appearance: in OCCT's DRAW
viewer, or with `--serve` in a browser page that also edits the family's
parameters and sketch definitions.

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

### Undo and redo

**Undo** and **Redo** restore applied sketch, solid, parameter, placement and
instance edits together with the selected solid output and requirement checks.
Use Ctrl/Cmd+Z to undo, Ctrl/Cmd+Shift+Z or Ctrl+Y to redo. Focused text fields,
including sketch fields, keep their normal text undo. History navigation waits
for queued parameter edits, and stale requests cannot overwrite a newer model.

**Save** keeps session history. Undoing a saved edit makes the document unsaved;
returning to the saved content clears that status. A new accepted edit after
Undo discards redo; previews, rejected edits and unchanged parameter submissions
do not. **Revert** and successful external file reloads clear history. Failed
reloads retain the accepted model and history.

History is temporary and bounded to 50 snapshots and 32 MB of serialized
document/output data; older entries are dropped when either limit is reached. Meshes and native handles are
regenerated on navigation rather than retained. Local sketch and solid drafts
are separate: reload a draft after navigating history before applying it.

### Sketch studio

The palette includes elliptical arcs (U), conics (Q), text (T), three-point
arcs (J), center rectangles (B), and control splines (K). Control splines
support degree 1–5 and optional positive rational weights; text uses installed
fonts validated by native Preview. See the
[primitive gestures](../../docs/SKETCH_STUDIO.md#additional-primitive-tools).


`/studio` (also linked as "Sketch studio" from the viewer) sketches in the 3D
view instead of a flat overlay:

- **Layers.** Every sketch is drawn on its plane next to the solid and
  listed with its plane and free degrees. Click a layer to make it active;
  `N` turns the camera square to it. New sketch starts one on XY, XZ or YZ
  (with an offset) or on a face you click.
- **Drawing.** The tool palette on the left (an icon and name per tool;
  keys `L R C A S E O G P`) draws lines, rectangles,
  circles, arcs, splines, ellipses, slots, polygons and points. Type a value
  during a gesture for an exact length, radius or size. Snapping adds the
  constraints it implies (coincident, midpoint, on curve, horizontal,
  vertical, parallel, perpendicular, equal, tangent); hold Alt to add none.
- **Constraining.** With Select (`V`), click or Shift-click geometry; a bar
  offers the constraints it can take and Enter applies the first. Dimension
  (`D`) measures what you click and asks for a value or parameter. Free
  geometry is blue, conflicting geometry red. Drag free points. Delete or
  Backspace removes the selection and what depends on it (constraints,
  unused endpoints, loop members); redrawing a deleted outline edge closes
  the outline again.
- **Modify.** Fillet (`I`) and Chamfer (`H`) take a corner of the outline
  and a typed value or parameter; Extend (`W`) runs a curve's end on to the
  next curve it meets, previewed while you hover. They record the model's
  profile and curve edits, so the edited outline draws solid over its
  dashed sources. Trim (`M`) cuts away the piece of a line, arc or circle
  between the curves crossing it, and Split (`Y`) cuts a line or arc in
  two; both edit the geometry, holding new ends on the crossing curves.
  Offset (`Z`) grows or shrinks the whole outline by a typed distance or
  parameter with round or sharp corners, drawing a guide as you type.
- **Instances.** With several instances, a header menu chooses the one
  whose parameter values and placement the sketches are shown with;
  sketch edits apply to every instance.
- **Pattern.** Mirror, Linear and Circular take the selected or clicked
  curves, then the mirror line, direction line or center point, with a
  dashed guide, and record linked sketch patterns. A mirror of half an
  outline closes it through the axis, and patterned holes stay holes.
- **Solid features.** **Extrude, revolve, sweep or loft** uses the active applied sketch
  and its plane axes, previews the native result in 3D, then applies one model
  history step. Existing extrusions, revolutions, sweeps and lofts retain their parameter/formula drivers.
  Close or changing values restores the accepted model; all variants must pass.
- **Planes.** New plane builds a datum plane offset or angled from a base
  plane, on a picked face, midway between two picked faces, or through three
  picked or typed points, with a live preview. Its values become parameters
  that its inspector edits; Sketch on this plane starts a sketch on it. See
  [Datum planes](../../docs/DATUM_PLANES.md).
- **Drafts.** Every change previews natively at once; Undo, Redo, Discard and
  Apply act on the draft, then Save writes the file. `?` lists shortcuts, and
  Advanced opens the classic panels (point links, patterns, projections,
  curve and region edits, Sketch to solid).

See [Sketch studio](../../docs/SKETCH_STUDIO.md) for the design and what still
needs the classic editor.

### Create a sketch

Choose **New sketch** in **Dimensions & sketches**. Give the new feature a
unique name, choose a centered rectangle or circle, and enter its dimensions
in millimeters. XY, XZ and YZ planes support a signed offset along their
displayed normal. Starter points are fixed; use **Edit sketch geometry** to
change coordinates, unfix points, draw more geometry or add dimensions afterward.

Choose **Pick support face in 3D**, orbit to the desired surface, then click
it. The picked face is highlighted, and the panel shows its native normal and
area center. A click picks the nearest visible triangle; orbit drags do not
pick, and Escape cancels even while the accepted view is loading. Picking works
on tilted planar faces as well as primary faces. Curved surfaces are refused.

The face index is a version-local mesh hint and is never saved in the sketch.
The server derives a unique normal selector where possible, or follows a
unique ancestral face through native feature history. This distinguishes
parallel faces such as a pocket floor and the outer top surface, while allowing
source dimensions to change. If neither query uniquely identifies the picked
face, a strict center anchor is used and clearly shown in the panel: moving
that support requires picking again. Stale or preview geometry cannot bind a
new creation draft. Preview/Apply still verify the attachment in all variants.

Alternatively, for **Planar face**, choose a support feature and one of the six
family-axis normal directions. The selector must resolve exactly one planar
face across every instance/variant. Local zero follows the face area center,
and the signed offset follows its oriented normal. The profile is not clipped
to the support boundary. Datum planes remain available through the model APIs.

**Preview new sketch** leaves the accepted model and file unchanged.
**Apply verified new sketch** regenerates all variants, verifies valid, nonempty
face geometry, retains all existing requirements and keeps the current solid
output visible. The new profile then
appears in **Edit sketch geometry** and **Sketch to solid**, where it can become
an extrusion, boss or pocket. Undo/redo includes creation and downstream edits;
**Save** writes the feature graph. New sketches append to the feature list:
without an explicit `--output`, a restart or Revert uses the latest saved feature
as the default output, including a sketch that has not yet been extruded.

### Sketch geometry drafts

In **Dimensions & sketches**, choose **Edit sketch geometry**, select a sketch
feature, and click **Load sketch draft**. Loading switches to that sketch view.
The editor changes the primary family's definition, shared by every instance;
instance selection chooses the preview's parameter values, not an independent
sketch definition.

Edit point coordinates and fixed/free status, or add named points, lines and
circles. Lines use two existing points; circles use center and rim points.
Click visible sketch points or curves to select them for dimension and constraint controls. Literal coordinate
fields use millimeters; parameter/formula coordinates retain their expressions.
Free-point coordinates are solver starting positions, so constraints may move
them. Edit the outer profile's curve names to include new boundary geometry.
Removing a referenced point requires repairing its references in the same draft.

The **Support & projected references** section changes an existing draft's
attachment or adds linked geometry. Choose a **Source feature**, then **Reattach
to picked face** or **Project picked edge**. The viewer temporarily shows that
accepted source output so you can orbit and click it; source inspection does not
apply a model edit. Known downstream features and the edited sketch are excluded
from the list, and native validation rejects remaining dependency cycles.

Reattachment replaces the face support and frame while preserving sketch-local
point coordinates, geometry, constraints and the existing offset expression.
It clears a previous datum-plane attachment. Preview checks downstream solids
and requirements before Apply, and Undo restores the previous attachment.

Edge clicks select visible outlines and highlight the chosen edge. Saved links
use semantic extrema, authored curve ancestry or a clearly reported strict
center anchor; topology indices are not stored. Straight edges become linked
lines. Curved edges use exact rational B-spline import, not sampled sketch
geometry. Preview rejects collapsed or unsupported projections. Source dimension
changes regenerate projected geometry together with constraints and solids.

Projection names appear in curve selectors, and stable generated endpoints such
as `reference:start` and `reference:end` appear in point selectors. The references
can drive typed constraints and remain fixed to their source. To use a projection
as boundary geometry, include its name in **Outer profile**. Adding construction
references requires an explicit profile or discovery curve list so an implicit
boundary is not changed by the new geometry. **Remove projection** only removes
the link from the draft; repair any referencing constraints or boundaries before
applying. Local drafts, Preview/Apply, Save and Undo/Redo retain their usual
behavior. Escape cancels pending or active source picks.

The **Dimensions & constraints** panel creates and edits relationships through
geometry selectors and numeric fields. Choose a constraint type, then click
points or curves in the view (click again to deselect), or use the geometry lists.
For two-entity relationships, selection order determines **First** and **Second**.
Selected geometry is highlighted; **Clear geometry selection** or Escape clears
it. Picks are limited to authored points and curves in the loaded sketch.

Tools include horizontal/vertical, coincident, parallel/perpendicular/collinear,
equal line lengths or radii, concentric, midpoint, point-on-curve, and tangency
at a shared point or between independent lines/circles/arcs. Dimensions include
point distance, signed horizontal/vertical and point-to-line distance, radius,
diameter, curve length, line/line angle, line-to-axis angle, and arc sweep.
Lengths use millimeters and angles use degrees; angle expressions remain radians
in the model. Choose **Driving value** to enter a number or link a compatible
family scalar parameter. Existing formulas remain intact unless explicitly
replaced. These are driving dimensions; reference measurements remain read-only.

Click a listed constraint or its dimension/constraint annotation to edit its
supported authored definition, then **Update selected constraint**. **Remove
constraint** deletes it from the draft. Generated constraint annotations cannot
be edited as authored constraints. Preview recomputes native geometry, dimensions
and residuals; conflicts remain diagnostic and cannot be applied.

**Advanced constraint definitions** and **Full definition** retain JSON controls
for other constraint kinds, generated/projected geometry, holes, regions, and
profile edits.

### Canvas drawing and constrained dragging

After loading a sketch draft, choose a **Canvas tool**: Point uses one click;
Line, Circle (center then rim), and Rectangle (opposite corners) use two clicks.
The cursor snaps to existing authored points before using the configurable
millimeter grid; set the grid to zero to disable grid snapping. Named point IDs
are reused when snapped. New entities receive unused stable IDs. Rectangles add
horizontal/vertical constraints. **Fix new points** defaults on; turn it off to
create free geometry that you will constrain later.

New geometry appears as a dashed local draft overlay. Native solving and profile
validation occur on **Preview draft**, followed by guarded **Apply** and **Save**.
Drawing normally adds construction geometry. **Use new circle / rectangle as
outer profile** explicitly replaces a simple outer boundary; sketches with holes
or region/profile edits require their existing profile controls instead.

The Drag tool moves authored free points with literal coordinates. Fixed,
generated, and formula-driven points require their coordinate or driving controls.
During movement a dashed guide shows the target; on release, a read-only native
preview temporarily pins the point, solves its connected geometry, releases the
pin, and verifies the original acceptance policy and all required checks. It
returns a draft only if the released solve still reaches the target. Existing
constraints and fixed positions are retained. Unsupported/conflicting or stale
moves are rejected; the accepted model and file remain unchanged. Solving occurs
on release, rather than on every pointer movement.

Shift-drag empty space to pan while a drawing tool is active. Escape or pointer
cancellation cancels an unfinished gesture; completed local drawings remain in
the draft. Escape during a pending drag discards its eventual reply. Drawing tools
are limited to the loaded sketch; other views keep their normal navigation.

**Preview draft** shows candidate geometry and solver evidence without changing
the accepted model or its file. **Apply verified draft** checks the model version
and complete expected sketch, validates the definition, regenerates all variants,
and enforces required checks before replacing the studio state. Rejected or stale
drafts remain editable. **Save** writes an applied edit; **Revert** reloads the
saved model. **Discard draft** closes the local draft and restores the accepted
annotation view. No schema or native ABI change is needed.

### Sketch-to-solid controls

In the studio, choose **Extrude, revolve, sweep or loft** in the side panel; the classic
viewer uses **Sketch to solid**. These controls create an extrusion, revolution
or path sweep, add material by union, or remove material by cut. Select an existing **Sketch / planar profile**, enter a
result feature name, and choose a target solid for union/cut. The profile's current
material regions and holes are used together; individual region picking and new
sketch creation are not part of these controls. Apply pending sketch drafts first;
saving them is unnecessary before creating a solid draft.

Enter positive depth in millimeters or link a length parameter. Direction X/Y/Z
is a finite, nonzero vector in family coordinates, normalized so depth sets the
travel distance. Use negative components to reverse travel. **Symmetric** splits
the total depth equally about the profile plane. A profile on the top of a plate
can cut a pocket by extruding toward the plate, for example direction `(0,0,-1)`.
**Show source sketch** switches the annotated view to its authored geometry.

The **Feature list** opens existing distance/symmetric extrusions, revolutions
and sweeps for editing.
Their IDs stay stable. Known direction/depth expressions retain their authored
form when unchanged; other formulas have a **Keep existing direction and formula**
choice and are replaced only when you explicitly choose entered values or a
parameter. Geometric cutoff extents and editing other operation types remain
outside this panel.

Choose **Solid feature → Revolution** to turn a profile about a family-coordinate
axis. Set its origin in millimetres and a finite, nonzero direction. In the studio
the initial axis follows the sketch's local Y axis through its origin. The signed
angle is entered in degrees (nonzero, at most one full turn); scalar parameter
links use radians. **Symmetric** splits the total angle about the profile plane.
**Edit revolution …** retains existing axis/origin and angle formulas until you
explicitly replace them. Revolutions can also supply the tool for union or cut.

Choose **Solid feature → Sweep along path**, then a separate **Sweep path**
from the applied open/closed sketch wires or helices. Position the profile at the
path start, usually across the initial tangent; holes are retained. The active
open sketch becomes the initial path choice. **Show sweep path** activates its
sketch in the studio. **Least twist**, **Frenet**, **Fixed** and **Binormal direction**
control how the section turns; binormal uses a normalized family-coordinate
vector. **Edit sweep …** keeps authored binormal formulas by default, with an
explicit option to replace them. Sweeps can create new solids or union/cut tools.

Choose **Solid feature → Loft through profiles** to join ordered, distinct
closed sections on different planes. Each section needs one boundary.
Choose **Smooth** or **Ruled** surfaces. **Add loft station**, Remove and the
Earlier/Later controls edit the section order. **Add loft hole track** adds an
inner profile at every station; reordering/removing stations moves all tracks
together. All choices must be filled and every saved profile reference distinct.
**Show loft station …** activates its sketch. **Edit loft …** retains ordered
sections and holes; source sketch parameters remain linked. Lofts can also
supply union/cut tools. Native limits are 2–1000 stations and at most 100 hole
tracks; validation rejects invalid sections, crossing holes and non-solid output.

**Preview solid** displays candidate 3D geometry and per-instance solid count and
volume while the accepted document, glTF, and file remain unchanged. Valid
`/api/features` previews return their native `gltf` mesh plus `version`, `instance`
and `output`; rejected results have no verified preview mesh. Apply repeats
validation against the expected feature graph and model version. All variants
regenerate and existing required checks remain enforced. The result must contain
valid, nonempty solid material; unions/cuts must make an actual material change.
A cut that erases a thinner variant is rejected even if another variant survives.
Required validity and positive-volume checks for the result are stored in the
model, so later parameter edits and CLI/MCP regeneration enforce them too. Existing
requirements are retained; identical result guarantees are reused.

**Apply verified solid** updates the studio and displays the result output;
**Save** persists the feature graph. **Revert** restores the saved document and
original CLI output selection. **Close solid draft** restores the accepted view.
Union/cut creation adds a named extrusion tool plus a boolean result, preserving
the source profile and target as editable dependencies. No schema or ABI change
is required.

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
or any other edit can be discarded. Undo/redo remains available after **Save**.
The page needs no external files. The server listens only on
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
and rejected-constraint previews without acceptance. The real page logic and shared renderer also run against minimal DOM
hosts in the JavaScript tests. The sketch draft workflow was verified in headless
Chrome through canvas curve selection, equal-length creation, annotated dimension
editing, coordinate editing, pocket-cut preview/apply/revert, existing extrusion
editing, and constrained dragging/drawing, with no
page errors.
