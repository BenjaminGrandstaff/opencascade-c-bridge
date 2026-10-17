# Sketch studio (plan)

Status: built (phases 0–5, 2026-10-15). Open it with `occt-view --serve`
at `/studio`, or from the "Sketch studio" link in the classic viewer. The
classic sketch editor is unchanged and stays available.

The sketch studio replaces the panel-driven sketch editor with sketching
directly in the 3D model view. Each sketch is a layer on its own plane.
You draw with the mouse from a toolbar, the studio adds the obvious
constraints as you draw, and you add the rest by selecting geometry. It is
built beside the current editor, not into it, so the two can run side by
side until the studio reaches parity.

## What exists today

- **One 2D overlay at a time.** Choosing "Sketch · part/profile" swaps the
  3D view for a flat SVG drawing of that sketch alone. You can't see the
  sketch on its plane, next to the solid or the other sketches.
- **Tools in a side panel.** The drawing tools (point, line, circle,
  rectangle, arc, spline, ellipse, slot, polygon) are a dropdown, with
  their options as checkboxes and number fields beneath. Each tool takes
  clicks in a fixed order described in a paragraph of help text.
- **Constraints through forms.** The core solver has 38 constraint kinds,
  but you add them through typed forms in the panel. While drawing, only
  rectangles constrain themselves (horizontal and vertical sides); points
  snapped onto existing points are shared, which makes them coincident.
- **A crowded panel and canvas.** More than ten collapsible sections
  (dependencies, point links, crossing and spline-sample picking, saved
  crossing windows, projections, curve and region edits, patterns) are
  stacked under the tools. On the canvas, labels for mirrors, patterns
  and constraints are drawn on top of each other where geometry meets.

The server side is sound and stays as it is: `/api/sketches` lists the
definitions, `/api/sketch` previews and applies a draft against the whole
model, `/api/sketch-create` adds a sketch, `/api/sketch-drag` solves a
drag, and `/api/sketch-face-pick` attaches a sketch to a face. Drafts are
previewed natively before they are applied, and applying is guarded by
the model version.

## Layout

```
┌───────────────────────────────────────────────────────────────────────┐
│ Sketch studio  Look at  Fit          Advanced ?  Undo Redo … Apply    │
├───────────────┬──────────────────────────────────────┬────────────────┤
│ EDIT          │                                      │ Inspector      │
│ Select  Dim   │      3D model view                   │ (selection)    │
│ DRAW          │      active sketch plane gridded,    │  Line L3       │
│ Line Rect Circ│      other sketches dimmed           │  length 20 mm  │
│ Arc Spline …  │                                      │  [Horizontal]  │
│ (tool options)│      ┌ contextual constraint bar ┐   │  [Fix]         │
│ Layers        │      │ ∥  ⊥  =  ◎  tangent  …    │   │                │
│ ● profile  XY │      └───────────────────────────┘   │                │
│ + New sketch  │                                      │                │
├───────────────┴──────────────────────────────────────┴────────────────┤
│ Status: 3 free degrees · solved · residual 2e-15 · snap: endpoint P4  │
└───────────────────────────────────────────────────────────────────────┘
```

- **Tools (left).** A palette of tools, each a button with the shape's
  icon over its name and its key in the corner, in Edit (Select,
  Dimension) and Draw groups; the chosen tool's options (polygon sides,
  closed spline, arc flip) sit below it.

- **Layers (left).** Every sketch in the model, with its plane (XY, XZ,
  YZ, a datum or a face), a visibility toggle and a color. One sketch is
  active at a time. "New sketch" offers the three base planes, the datum
  planes, or a face picked in the view.
- **View (center).** The solid stays visible. Every visible sketch is
  drawn on its plane in 3D; the active one is full strength over a light
  grid, the others are dimmed. "Look at" turns the camera square to the
  active plane, and you can still orbit while drawing.
- **Inspector (right).** Shows only the selection: its values,
  constraints and actions. Nothing is shown when nothing is selected.
- **Status bar (bottom).** Free degrees, solver state, the current snap
  and short hints for the active tool.
- **Curve and region edits (left).** Opens the native operation controls with
  canvas picking and draft Undo/Redo. Named point picking has its own panel.
- **Linked patterns (left).** Mirror, linear and circular patterns use native
  controls with source, axis/direction and center picking on the canvas.
- **Advanced drawer.** Hosts the classic panels for projections, explicit
  boundary definitions, dependencies and sketch-to-solid features.

## Drawing

Clicks are cast from the camera onto the active plane, so drawing works
at any view angle. Every tool shows a live preview, and Escape cancels.

| Tool | Key | Mouse |
|---|---|---|
| Line | L | Click, click… chains lines; double-click or Escape ends the chain |
| Rectangle | R | Two corners (B selects the center-rectangle tool) |
| Circle | C | Center, then radius |
| Arc | A | Center, start, end (J selects the three-point tool) |
| Spline | S | Points, double-click to finish; clicking the first point closes it |
| Ellipse | E | Center, major-axis end, minor width |
| Slot | O | Two cap centers, then width |
| Polygon | G | Center, then a vertex; side count under the tool palette |
| Point | P | Click |
| Dimension | D | Click one or two pieces of geometry, place the label, type a value |

While a tool is active, a small field next to the cursor accepts typed
lengths and angles (for example `20, 45` then Enter for a line), so
exact values don't need a separate step.

## Draft Undo and Redo

Undoing the first studio edit returns to the starting sketch and keeps Redo
available. The local draft remains open, including for an empty new sketch;
no empty or unchanged baseline is submitted for native Preview. Its status
reads **Draft · at starting sketch · Redo available**. Undo is disabled at
that boundary even if accepted model history exists. Redo restores the edit
and previews it again. A new edit after Undo clears only the local redo branch.

**Discard** explicitly clears the local draft and both history stacks, leaving
the accepted model unchanged. **Apply** verifies and accepts the edited draft
and ends its local history; Undo/Redo then navigate accepted model changes
using the current model version. Successful model navigation drops any idle
clean draft from before that change.
Save and the Advanced drawer require applying or discarding local history,
including when it is at the starting sketch with Redo held. Browsing another
layer does not replace that held history; restoring a snapshot selects its
own layer again. Focused text fields retain their normal text Undo behavior.

Undo/Redo cancels scheduled previews and invalidates outstanding preview
responses before restoring a snapshot. At the starting sketch, accepted
geometry and solver feedback return immediately. Late success/error replies
from undone edits cannot restore old geometry or mark the baseline invalid.
Snapshot navigation reuses immutable definition references in O(1) stack work;
restoring solver presentation scans the sketch layers/scenes, and rendering
and native Preview retain their own costs. The persistent studio benchmark
checks 10,000 Undo plus 10,000 Redo transitions on 10,000-curve snapshots within
1 s, excluding browser layout and native regeneration.

## Pending edits and solver replies

Apply locks sketch editing, Discard, Undo/Redo and Save until its request and
model reload finish. Camera navigation still works. Repeated clicks or keyboard
shortcuts cannot submit another Apply or change the definition being accepted.
A rejected Apply keeps the draft and history for correction or retry. If Apply
succeeds but refreshing fails, the draft is cleared and the status reports that
the model was applied and the page needs reloading.

Solved point drags are read-only requests until their result becomes a local
edit. They are accepted only for the same draft revision, model version,
instance and active layer. A later edit, Undo/Redo, Discard, Apply, layer or tool
change invalidates the old reply, including errors. Of overlapping drags, only
the newest may finish; a successful drag creates one Undo entry. A new edit
invalidates older Preview replies immediately, before the preview debounce
expires. Older polling/reload results cannot replace the model loaded after a
studio Apply, model Undo/Redo or Save.

## Generated geometry references

The studio retains the native API's verified generated point names and curve
kinds when loading a sketch or receiving a current, successful Preview. This
covers shape boundaries, pattern copies and projected geometry. Catalogues are
inspection data; saved definitions contain only the references you use.

Select a generated side or arc to get its supported constraints. The Dimension
tool measures a generated line between its native defining endpoints, or an arc
by its center and start point. Shared-endpoint tangency suggestions use those
same roles, including explicit spline endpoints. Drawing can reuse a generated
corner by name; generated lines also supply midpoint, parallel/equal-length and
endpoint-tangency inference, while generated circles/arcs supply radius
inference. Generated points retain their owning shape/pattern/projection and
are not independently draggable or editable as authored coordinates.

Compatible draft edits and Undo/Redo retain verified catalogues. Changing a
generator or an existing source curve invalidates its catalogue until a new
successful Preview. The suggestion list refreshes when Preview supplies new
names. Unverified scene points, edited-curve display names and glyph outlines
cannot create snapping constraints. Curve kinds come from native metadata and
defining point roles from the solved scene, never from sampled shape guesses.

The studio scale benchmark includes 10,000 generated pattern lines with 20,000
named endpoints and 330,000 curve samples, with the same 500 ms index-build and
16 ms subsequent-move budgets as authored geometry.

## Editing driving dimensions

Double-click a driving dimension label to edit its value, or focus it with Tab
and press Enter or Space. Selecting geometry also exposes an **Edit** button
beside its driving dimensions in the inspector; a curve's defining points are
included, so selecting a line reveals its endpoint distance. Inline values use
millimetres or degrees. Enter a number or parameter name and press Enter or
**Update**. Parameter names change this dimension's binding; they do not edit
the parameter's global default. Escape cancels.

Restoring the original value/expression skips an unchanged native Preview and
keeps the local Undo history; Apply stays disabled until there is a model change.
Updates replace the value of the existing constraint, retaining its geometry
roles and position in the constraint list. They use the normal draft Preview,
Apply and Undo/Redo workflow. Invalid values keep the editor open for correction.
An unchanged value creates no Undo entry and preserves the original units or
parameter expression exactly. Compound formulas show an empty field with a
preservation hint: leave it empty to keep the formula, or type a replacement.
If the draft, history, layer or model version changes while a value is open,
its old editor cannot write to the changed sketch.

Reference dimensions are drawn as parenthesized measured values with dashed
label borders. They update with the solved geometry and remain read-only; they
do not become driving constraints. Dimensions generated internally by shapes
can be displayed, but only a label matched to an authored driving constraint
opens this editor. Use the shape's controls to edit its own definition.

## Picking named points on the canvas

**Named points** in the side panel opens **Intersection point**,
**Spline sample** or **Weighted point**. Both work on the active applied sketch, including a draft
whose current Preview succeeded. Apply a new sketch first. Picks use the
studio's sketch plane at any camera angle and native geometry queries.

For an intersection, choose a new point name or an existing authored target and
a search radius in millimetres. Start the pick, click two different source
curves, then move near the intended crossing. The canvas shows the search ring,
hint and finite native candidate markers. Click to stage only when exactly one
candidate lies inside the window; missing or ambiguous windows leave the draft
unchanged. Change the radius without reselecting the curves. The saved link
retains the literal hint and radius for regeneration.

For a spline sample, choose a name or existing target, start the pick, then
click the source spline near the intended location. Review the native point and
parameter fraction. Enter an exact fraction and update its marker, or use Start
(0), Midpoint (0.5) or End (1), then **Stage spline sample**. These are parameter
fractions, not arc-length fractions. Preview verifies the staged relation.

Each staged link is one draft edit with Undo/Redo and Apply. Close, Escape,
pointer cancellation, another tool, a layer change, reload or draft edit cancels
the pick; late replies cannot restore markers or stage a point. Starting a new
sketch, plane workflow or Advanced editing also ends the pick. Finish or cancel
an active face/plane pick before starting a sketch point pick. Right-drag and
Shift-drag remain camera navigation. Generated source curves use the verified
catalogue; display-only glyphs and edited-curve names cannot be sources.

## Picking linked patterns

Choose **Mirror and pattern** in the left panel. Choose Linear, Circular or
Mirror, enter a pattern name, then **Pick pattern sources on canvas**. Click
source curves to add them to the form; repeated clicks keep one source entry.
The source pick stays active for more clicks. **Finish canvas pick** returns to
Select; the form's multi-select list also lets you change the source set.

Pick a direction line for linear copies, an axis line for a mirror or a named
center point for circular copies. Reference picks finish after one valid click.
Mirrors also allow repeated picks of shared points on the axis. Preview checks
that shared points lie on that axis. Source curves cannot include the reference
line. Verified generated curves from earlier patterns are valid sources; a
pattern cannot depend on itself or a later pattern. Source picks use the native
source geometry, rather than display aliases created by later curve edits.

Linear spacing accepts positive or negative millimetres or a length parameter.
Circular spacing defaults to an equal full turn; an entered step is in degrees,
or it can use a scalar parameter. Counts include the source and range from 2 to
1,000. Existing patterns retain their names/types and untouched formulas or
parameter drivers when you edit the count or picked sources. Select an existing
pattern in **Pattern to edit** to update or remove it.

Picks and unfinished form changes leave the draft unchanged. Add, update or
remove stages one Undo entry. Native Preview generates the copies and checks
dependent profiles and solids; Apply commits the result. Generated copies do
not automatically enter explicit profile or hole loops. Those boundary choices
remain in Advanced. A rejected draft can reopen this panel for repair, and a
previewed new sketch can stage patterns before its first Apply. Context changes,
Escape and the same stale-form guards as curve edits cancel pending picks.

The scale benchmark checks 1,000 repeated source picks from 20,000 curves within
one second, excluding browser layout. It reuses the source catalogue and option
index during the pick; no draft snapshots are created until submission.

## Editing curves and regions

Choose **Edit curves and regions** in the left panel. The studio reuses the
native controls for exact trim to named points or crossings, parameter-fraction
trim, source split, extension to a crossing, corner fillet/chamfer and signed
profile offset. Corner and offset operations can target the outer profile or
the material region containing a named seed. Existing discovery definitions
also expose cell selection, seed selection and adjacent-cell merging here.

Use the form's **Pick … on canvas** buttons to fill a curve, endpoint, crossing
hint or region seed. Picks and unfinished form edits leave the draft unchanged.
Submitting, updating or removing an operation stages one Undo entry; updating
discovery selection does the same. Splitting can replace explicit boundary
references with the two saved piece names in that same transaction. Native
edited-piece display aliases resolve to their saved curve names for subsequent
operations, including after Apply.

Preview checks the edited boundaries and all dependent solids. A trim must
belong to its selected profile, crossing hints must identify a valid crossing,
and region seeds must lie strictly inside their material. The editor can reopen
a rejected draft to repair or remove an operation. Removing the last rejected
edit back to the accepted definition retains Undo/Redo and leaves Apply disabled.
Previewed new local sketches also support operations before their first Apply.

Close or Escape cancels the pending pick. Changing sketches, tools, history,
plane workflows or drafts invalidates the open form, so old callbacks cannot
stage edits in a newer context. Right-drag and Shift-drag retain camera navigation.
Initial discovery setup and explicit boundary authoring remain in Advanced.

## Picking weighted points

Choose **Weighted point** under Named points. Choose a new name or an existing
authored target, then **Start weighted point pick**. Click authored, projected
or verified generated source points on the canvas. Each click adds a source
with weight 1; a repeated click asks you to edit the existing weight. You can
also add/remove source rows and type source names in the form.

Review up to 256 positive scalar weights. Enter a literal or choose a scalar
family parameter. Relinking an existing weighted point loads its saved terms,
including parameter and compound-formula drivers; a formula remains unchanged
until explicitly replaced. Equal weights make an average or midpoint. The
source catalogue is shared by the rows, so adding picked sources does not
rebuild thousands of source options for every row.

With literal weights, the canvas shows source markers and the weighted position
computed from their current solved coordinates. Parameter/formula weights show
source markers and defer the result to native Preview. **Stage weighted point**
creates one draft edit, then Preview evaluates the link and all model variants.
Apply, Undo/Redo and Save work as for other named points. Changing a referenced
weight parameter regenerates the linked point.

Relinking unfixes the authored target and replaces its saved point links;
other dimensions remain active. The target cannot be one of its own sources.
Invalid literal weights disable staging; native Preview checks parameter values,
cycles and dependent sources. Close, Escape, a draft edit or a context change
cancels the un-staged pick. Source markers are review data, never saved geometry.

## Additional primitive tools

The toolbar also authors these existing native definitions (schema 164 and ABI
76 unchanged):

| Tool | Key | Gesture and options |
|---|---|---|
| Center rect | B | Center, then corner. Typed width/height are full sizes. Four sides retain H/V constraints; a construction diagonal and midpoint keep the center relationship. |
| 3-point arc | J | Start, a point the arc passes through, then end. The span includes the middle point, retained with native point-on-curve membership. Collinear clicks reject locally. |
| Ellipse arc | U | Center, major-axis end, minor width, then start/end directions. End directions project onto the ellipse; snapped endpoints must already lie on it. X/Flip switches the directed span. |
| Conic | Q | Start, intersection of the end tangents, then end. Set rho in (0,1): below 0.5 elliptic, 0.5 parabolic, above 0.5 hyperbolic. |
| Control spline | K | Click poles, then Enter/Finish. Degree 1–5, at least degree+1 poles, maximum 256 including a closing pole. Optional positive weights, one per clicked pole, author a rational spline. Closed repeats the first pole with a C0 seam; it does not promise smooth periodicity. |
| Text | T | Enter one-line text, installed font family/style, em height, baseline angle and start/center/end alignment; click the baseline anchor. Native Preview builds exact font outlines and rejects missing fonts. |

Open arcs/conics remain construction geometry until selected into a profile;
they do not replace an existing closed outline. New closed center rectangles
and closed control splines can start a local sketch's outer profile. Text
outlines participate in the sketch face output. Center rectangles' diagonal
is excluded from their profile. Newly authored points stay free.

Curve ghosts use at most 65 samples, with control-polygon guides bounded by 256 poles; control-spline ghosts evaluate the
rational basis instead of connecting poles. Text shows an anchor until native
Preview supplies glyph outlines. Ghosts are visual approximations; persisted
curves retain exact native definitions. General knot/basis editing remains in
the advanced spline controls. A gallery generated through the same commit paths
is [studio-primitives.request.json](../tools/model/studio-primitives.request.json).

Source names are reserved once per gesture, with per-prefix counters. Additional
record authoring is O(G + N) time/space for existing geometry data G and at most
256 new poles N; center rectangles also use the shared constant-size drawing
helper. Ghost evaluation is O(N + 65·degree²). The persistent studio benchmark
checks 20 degree-5, 256-pole commits and previews against 10,000 existing curves
within a 2 s budget; it excludes native regeneration and browser layout.

## Automatic constraints

As you draw, the studio infers constraints from where the cursor snaps.
The snap and the constraint it will add are shown before you click, and
holding Alt suppresses them.

| When the cursor… | It snaps to… | And adds |
|---|---|---|
| lands on an existing point | that point | shared point (coincident) |
| lands on a curve | the nearest point on it | point on curve |
| lands on a line's midpoint | the midpoint | midpoint |
| lands on a circle or arc center | the center | coincident (concentric for circles) |
| draws a line within 3° of horizontal or vertical | the axis direction | horizontal or vertical |
| draws a line within 3° of parallel or perpendicular to a line it hovered | that direction | parallel or perpendicular |
| starts an arc at the end of a line, centre on the line's normal | the tangent direction | tangent |
| makes a length or radius equal (within 1%) to a hovered one | that size | equal length or equal radius |
| lines up with a point it hovered, horizontally or vertically | the alignment | nothing (the schema has no point-to-point horizontal) |

Rectangles keep their horizontal and vertical sides. Inference considers
only geometry near the cursor through a spatial grid, so it stays fast in
sketches with many thousands of entities.

## Adding constraints

- **From the selection.** Select one or more pieces of geometry, and a
  small bar beside them offers only the constraints that apply. Two lines
  give parallel, perpendicular, equal, collinear and angle; a line and a
  circle give tangent and distance; a point and a curve give on-curve and
  distance; two circles give concentric, equal and tangent; and so on,
  for each of the 38 kinds the solver supports. Kinds that need more than
  a selection (crossing points, spline samples, weighted point links)
  start their existing picking flow. One key press applies the first
  suggestion.
- **Dimensions.** The Dimension tool picks the measurement from what was
  clicked (length, distance, radius, diameter or angle). It places the
  label where you release, then opens an inline value field that also
  accepts a parameter name or expression.
- **Feedback.** Fully constrained geometry is drawn dark, geometry that
  can still move is blue, using the solver's free motions per point, and
  conflicting constraints are red, with the conflict group listed in the
  inspector. Free points can be dragged; the drag is solved natively, as
  today.

## Clean presentation

- **Labels never overlap.** Constraint glyphs and dimension labels are
  laid out with collision avoidance: each moves along its leader to the
  nearest free spot, and groups that still collide fold into one badge
  that expands on hover.
- **Glyphs instead of words.** Constraints are small icons (∥, ⊥, =, ◎, a
  tangent mark) at the geometry, with the full name on hover, and only for
  the active sketch.
- **One place for each thing.** Tools in the toolbar, selection in the
  inspector, solver state in the status bar, advanced features in the
  drawer.

## Built so far

- **Phase 0.** Every sketch scene in the view data has a `frame`: the
  sketch plane's `origin_mm`, `x_axis` and `y_axis` in family-local
  millimetres. It comes from `SketchDefinition::resolved_frame`, with
  datum planes and face supports resolved as the kernel resolves them, so
  a client can place sketch-local points in 3D as
  `origin + x·x_axis + y·y_axis`. Tests cover a sketch on its own axes, one
  on a datum plane, and one attached to a face.
- **Phase 1.** `occt-view --serve` serves the studio at `/studio`, with a
  "Classic viewer" link back. The model is drawn in 3D and every sketch
  on its plane, with the active sketch's points and a grid whose spacing
  (1, 2 or 5 × 10ⁿ mm) follows the zoom. The Layers panel lists each
  sketch with its plane ("XY", "XZ @ y = -4", "Face", "Datum side"), its
  free degrees and a visibility switch. Clicking a layer makes it active;
  Look at (N) turns the camera square to it with its x axis to the right.
  New sketch starts a local layer on XY, XZ or YZ with an offset, or on a
  face picked in the view and resolved by `/api/sketch-face-pick`. The
  layer reaches the model when its first closed shape is applied (phase 2).
  The inspector shows the plane's origin and axes and the solver state.
  The page follows edits made elsewhere by polling the model version.
  Node tests cover plane math (ray hits, projection, look-at for all three
  planes, face picks), layer labels and the grid budget; a Rust test checks
  that the page and every module it imports are served.
- **Phase 2.** The toolbar has every current shape (Line, Rectangle,
  Circle, Arc, Spline, Ellipse, Slot, Polygon, Point) with keys, live
  previews and per-tool options (polygon sides, closed spline and Finish,
  arc Flip). Lines chain until a double-click or Escape. Typing a value
  with a gesture open places the next click exactly: a line's length (and
  angle), a circle's or polygon's radius, a rectangle's width and height.
  While drawing, the cursor snaps to points (shared, so coincident), line
  midpoints (midpoint) and curves (point on curve). Lines within 3° snap
  horizontal, vertical, or parallel or perpendicular to the last line
  hovered, and within 1% snap equal in length to it; circles and arcs snap
  equal in radius to a hovered one; an arc started at a line's end with
  its centre on the line's normal becomes tangent to it. A free end moves
  onto these rules; an end snapped to a point keeps only rules that
  already hold, so snapping never fights fixed geometry. Point alignment
  snaps without a constraint, since the schema has no point-to-point
  horizontal. Alt suppresses inference. New points are free, not fixed.
  Each shape is added to a draft that is previewed natively at once (the
  layer shows its solved free degrees); Undo and Redo work within the
  draft; Apply sends it through `/api/sketch`, or `/api/sketch-create` for
  a new layer, whose first closed shape becomes its profile.
  Inference uses a uniform grid built once per draft, and drawing skips
  samples on straight runs; `tools/bench/sketch-studio.mjs` measures 0.1 ms
  per mouse move and 27 ms per overlay redraw at 10,000 lines.
- **Phase 3.** The Select tool (V) picks points and curves on the active
  sketch; Shift-click adds or removes. A bar beside the selection offers
  the constraints it can take, generated from the classic editor's
  constraint table (36 kinds, with options such as tangent side as
  separate choices), natural ones first and dimensions last; Enter applies
  the first. Two curves sharing one end get tangency at that end filled in.
  The Dimension tool (D) measures what is clicked (a line's length, a
  circle's diameter, an arc's radius, two points' distance, a point's
  signed distance to a line, two lines' angle) and opens a value box at
  the click, prefilled with the current value; it takes a number or a
  parameter name. The active sketch is drawn by solve state: free geometry
  (moved by a free motion of the solver) in blue, conflicting geometry
  (touched by a conflicting or failed constraint) in red, the rest in the
  ink color, with the selection highlighted. Free points with literal
  coordinates drag, solved natively through `/api/sketch-drag`. The
  inspector lists the selection's state, the draft constraints on it with
  Remove buttons, and every constraint it can take. Kinds that need a
  picking flow (crossing points, spline samples, weighted links) stay in
  the classic editor until phase 4's Advanced drawer.
- **Phase 4.** Constraints on the active sketch are drawn as glyphs (H, V,
  ∥, ⊥, T, =, ◎, M, ∈ and so on) and dimensions as their values, each in a
  small box laid out so none overlap. A label tries a table of spots
  around its geometry one label-size apart, nearest first, with a leader
  when moved; dimensions are placed first; labels with no room fold into a
  "+n" badge that lists them on hover. Conflicting labels are red and
  labels of the selection are outlined. Layout uses a spatial hash of
  placed boxes: 5,000 labels lay out in under 100 ms (a Node test). The
  active sketch has an outline in the view color, so light geometry stays
  visible over light faces. Look at now also fits the sketch. `?` shows
  every shortcut. The Advanced button opens a drawer hosting the classic
  sketch editor's panels unchanged (point links, patterns, projections,
  curve and region edits, profiles, dependencies); its previews show in the
  studio and its Apply reloads it. It needs the studio draft applied or
  discarded first, and its canvas picking (crossings, spline samples,
  faces, edges) still needs the classic viewer, linked from the drawer.
- **Phase 5.** The studio gains model-level Undo and Redo (when no draft is
  open), Save, a point inspector (x, y and fixed for one selected point),
  and Sketch to solid in the Advanced drawer. The classic viewer links to
  the studio. The parity review is below.

## Planes

The Planes panel, between the tool palette and the layers, lists the
model's datum planes, each drawn in the view as a dashed outline with its
name (the selected one in the accent color). New plane builds one of the
five [constructed kinds](DATUM_PLANES.md), chosen from a row of icons:

- **Offset** and **Angle**: a base plane (XY, XZ, YZ or another plane) and
  a distance, or an angle about the base's x or y axis.
- **On face**: click a flat face; the plane follows it at a distance.
- **Midplane**: click two parallel faces.
- **3 points**: click three points on the model (corners within 16 pixels
  snap to the corner) or type them.

The plane previews live (in the ghost color) through `POST /api/datum`, and
Create plane adds it with the parameters that drive it. Selecting a plane
shows how it is defined, where it resolves, and its driving parameters,
which edit like any other parameter (angles in degrees); the plane and
every sketch on it follow. From there, Sketch on this plane starts a sketch
on it, Offset from it starts a new plane based on it, and Delete removes it
when no sketch uses it. A plane that fails (its face is gone) is listed in
red with the reason, and so is each sketch on it.

## Deleting

With Select, Delete or Backspace removes the selection, and the inspector
shows a Delete selection button with what it will remove ("Removes 1 point
and 2 lines"). The deletion goes into the draft like any edit: it previews
at once, Undo restores it, and Apply keeps it. It follows the classic
editor's authored reference graph (`sketch-dependencies.mjs`):

- Curves, shapes and texts built on a deleted point go with it.
- Points left unused by remaining geometry go with the curve, shape or text
  that used them; shared endpoints stay.
- Constraints, reference dimensions, and curve, profile and region edits
  that name anything deleted go too, with whatever used their results
  (a split's pieces, for example). Parameters, text and selectors that only
  share a name are not references.
- Profile, hole and region loops and pattern entity lists drop the deleted
  members; an emptied hole, region or pattern goes, and so does a pattern
  whose axis, direction or center is deleted.
- A generated name stands for its record: selecting a rectangle's side
  deletes the rectangle, a pattern copy deletes the pattern, and a
  projection's endpoint deletes the projection. The status line says so.

A loop that loses one run of curves is turned to start after the gap, and
lines, arcs or splines drawn afterwards that meet its free ends join it,
turned round if they run the other way (an arc also flips its sense). So
deleting an edge of an outline and drawing its replacement closes the
outline again; until then the preview says the profile is not a continuous
boundary and Apply is refused. Deletion is linear in the sketch's size: the
Node tests delete from a 10,000-line chain in well under a second.

## Parity with the classic editor

| Capability | Studio | Notes |
|---|---|---|
| New sketch on XY, XZ or YZ with an offset | Yes | Layers panel |
| New sketch on a picked face | Yes | Picked in the 3D view |
| New sketch on a datum plane | Yes | Planes panel or New sketch; the studio also creates the planes (below) |
| Create datum planes | Studio only | Offset, angle, on face, midplane, three points; driven by parameters |
| Point, line, rectangle, circle, arc, spline, ellipse, slot, polygon | Yes | Tool palette, with typed values and automatic constraints |
| Elliptical arcs, conics, text, three-point arcs, center rectangles, control splines | Yes | Native records, toolbar gestures and options |
| Drag free points (solved) | Yes | Select tool |
| Edit a point's coordinates and fixed flag | Yes | Inspector |
| 36 typed constraints and dimensions with parameters | Yes | Selection bar, inspector, Dimension tool |
| Edit driving dimensions | Studio inline | Double-click a label or Edit in the inspector; reference dimensions are read-only |
| Remove constraints | Yes | Inspector |
| Delete geometry | Studio only | Delete key or inspector, with dependents; see [Deleting](#deleting) |
| Crossing points and spline samples | Yes | Named points panel, native candidates, on-canvas markers and draft history |
| Weighted point links | Yes | Named points panel, canvas sources, literal/parameter weights and preserved formulas |
| Curve and region operations | Yes | Studio panel with canvas picking, native Preview and draft Undo/Redo |
| Linked mirror, linear and circular patterns | Yes | Studio panel with canvas sources/references, native Preview and draft Undo/Redo |
| Projections, explicit profiles, dependencies | Yes | Advanced drawer hosts the classic panels |
| Sketch to solid | Yes | Advanced drawer |
| Preview, Apply, Undo, Redo, Save | Yes | Toolbar; Revert stays in the classic viewer |
| Several instances | Partly | The studio edits the first editable instance's family sketches |

Retiring the classic editor is a separate decision, to make with Codex,
which maintains it.

## Building it

The studio is a separate page, `/studio`, made of new modules. It imports
the existing pure helpers (`drawPrimitive`, `primitiveGhost`,
`snapPosition`, the renderer's `ray` and `viewProjection`) without changing
them, and calls the same endpoints. Codex's editor files are not edited.
The only shared changes are one route in `serve.rs` and one server
addition (phase 0).

| Phase | Delivers | Checks |
|---|---|---|
| 0 | Sketch scenes include their resolved plane (origin and axes in world space), from the existing `resolved_frame`. | A Rust test for base-plane, datum and face-attached sketches. |
| 1 | `/studio` page: 3D view, sketches drawn on their planes, layers with visibility and the active sketch, Look at, New sketch on a base plane or picked face. | Node tests for plane math and projection; a browser check of a model with three sketches on three planes. |
| 2 | Toolbar with every current shape, live previews, typed values, snapping, and automatic constraints; Preview and Apply through `/api/sketch`. | Node tests for each inference rule and tool; a browser walkthrough drawing each shape. |
| 3 | Selection, the contextual constraint bar for every kind, the Dimension tool, free and conflict coloring, solved drags. | Node tests for which constraints each selection offers; solver round trips through the existing endpoints. |
| 4 | Label layout without overlaps, constraint glyphs, keyboard shortcuts, and the Advanced drawer hosting the existing modules. | A layout test that no two labels overlap on the crowded demo sketch. |
| 5 | Parity review against the current editor, then a link from the main viewer. Retiring the old panel is a separate decision with Codex. | A checklist of every current editor capability. |

Each phase is its own commit with its own tests. Phases 2 and 3 are the
largest.

**Scale.** Sketches are drawn and hit-tested through a spatial grid, so
redrawing and snapping stay well under a frame for 10,000 entities. A new
scale case checks that.

## Open questions

- Should automatic constraints be on by default? This plan says yes, with
  Alt to suppress them and each one removable afterwards.
- Should the studio become the default sketch editor after phase 5, or
  stay an alternative?
