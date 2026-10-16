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
- **Advanced drawer.** Projections, patterns, crossing and spline-sample
  picking, curve and region edits stay in the existing modules. The
  studio mounts them in one collapsible drawer instead of re-creating
  them, so nothing is lost on day one.

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
| Remove constraints | Yes | Inspector |
| Delete geometry | No | Not in either editor's canvas; the classic JSON fields can |
| Crossing points, spline samples, weighted point links | Partly | Forms in the Advanced drawer; canvas picking needs the classic viewer |
| Patterns, point links, projections, curve and region edits, profiles, dependencies | Yes | Advanced drawer hosts the classic panels |
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
