# Sketch studio (plan)

Status: approved 2026-10-15; being built phase by phase (see the table
under "Building it").

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
│ Toolbar: Select │ Line Rect Circle Arc Spline Ellipse Slot Polygon    │
│          Point │ Dimension │ Constrain ▾ │ Trim … │ Undo Redo │ Apply │
├───────────────┬──────────────────────────────────────┬────────────────┤
│ Layers        │                                      │ Inspector      │
│ ● profile  XY │      3D model view                   │ (selection)    │
│ ○ rib     YZ  │      active sketch plane gridded,    │  Line L3       │
│ ○ boss  face7 │      other sketches dimmed           │  length 20 mm  │
│ + New sketch  │                                      │  [Horizontal]  │
│               │      ┌ contextual constraint bar ┐   │  [Fix]         │
│               │      │ ∥  ⊥  =  ◎  tangent  …    │   │                │
├───────────────┴──────────────────────────────────────┴────────────────┤
│ Status: 3 free degrees · solved · residual 2e-15 · snap: endpoint P4  │
└───────────────────────────────────────────────────────────────────────┘
```

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
| Rectangle | R | Two corners (Shift: from center) |
| Circle | C | Center, then radius |
| Arc | A | Center, start, end (or 3-point with Shift) |
| Spline | S | Points, double-click to finish; clicking the first point closes it |
| Ellipse | E | Center, major-axis end, minor width |
| Slot | O | Two cap centers, then width |
| Polygon | G | Center, then a vertex; side count in the toolbar |
| Point | P | Click |
| Dimension | D | Click one or two pieces of geometry, place the label, type a value |

While a tool is active, a small field next to the cursor accepts typed
lengths and angles (for example `20` then Tab, then `45` for a line), so
exact values don't need a separate step.

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
| starts an arc or spline at the end of a line or arc | the tangent direction | tangent |
| makes a length or radius equal (within 1%) to a hovered one | that size | equal length or equal radius |
| lines up with a point it hovered, horizontally or vertically | the alignment | horizontal or vertical (point to point) |

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
