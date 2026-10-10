# Modeled threads

`FeatureOperation::Thread` cuts a 60° screw thread with the ISO 68-1 basic
profile into an earlier solid:

```json
{ "id": "threaded", "operation": { "thread": {
    "input": "rod",
    "origin": { "literal": { "x": {"value": 0, "dimension": "length", "unit": "millimeter"},
                             "y": {"value": 0, "dimension": "length", "unit": "millimeter"},
                             "z": {"value": 5, "dimension": "length", "unit": "millimeter"} } },
    "axis": { "literal": { "x": {"value": 0, "dimension": "scalar", "unit": null},
                           "y": {"value": 0, "dimension": "scalar", "unit": null},
                           "z": {"value": 1, "dimension": "scalar", "unit": null} } },
    "major_diameter": { "literal": {"value": 10, "dimension": "length", "unit": "millimeter"} },
    "pitch": { "literal": {"value": 1.5, "dimension": "length", "unit": "millimeter"} },
    "length": { "literal": {"value": 15, "dimension": "length", "unit": "millimeter"} },
    "internal": false } } }
```

- The thread runs `length` along `axis` from `origin`, on the axis through
  `origin`, for `length / pitch` turns (at most 10,000). It is right-handed
  unless `left_handed` is true. All values are expressions.
- **External** threads cut a rod of `major_diameter`: the groove reaches
  5H/8 deep (H = √3/2 · pitch) and leaves a P/8 crest flat and a P/4 root flat.
- **Internal** threads cut a hole drilled at the minor diameter,
  `major_diameter − 2 · 5H/8` (8.376 mm for M10×1.5), out to the major
  diameter, leaving a P/4 crest flat and a P/8 root flat.

## How it is built

The groove section, a trapezoid in a plane through the axis, is swept along a
[helix](HELIX.md) at its centroid radius with the axis as binormal. That is a
screw motion, so the removed volume is exactly the section's in-material
area × 2π × centroid radius × turns; tests match it within 1e-4 (1e-7 to 1e-9
in practice for M10×1.5) for external and internal, right- and left-handed
threads. The section extends P/16 past the part surface along its flanks, so
the cutting tool never shares a face with the part, while staying narrower than
the pitch so successive turns of the tool never overlap. The result is checked
as one valid solid; crest flats keep the original diameter.

No ABI change: the feature combines the helix, polyline face, binormal sweep
and boolean cut. Schema 84 adds it.

## Not yet included

- Thread runout or chamfered starts: grooves begin and end abruptly.
- Tolerance classes, truncated or rounded roots (ISO 965, ASME B1.1 classes)
  and fit checks; the profile is the basic profile.
- Tapered (pipe) threads, multi-start threads and non-60° forms (ACME,
  buttress).
- Generating geometry from a hole's thread callout; holes still record thread
  intent only.


## AI example and viewer dimensions

The [threaded-rod example](../tools/model/threaded-rod.request.json) exposes major
diameter, pitch, thread length and start margin. The rod radius follows half
the major diameter, and its height follows thread length plus two margins.
Changing those parameters rebuilds compatible source geometry and the modeled
thread. The example uses an external right-hand thread with unthreaded ends;
its dimensions are illustrative and do not specify tolerance class or fit.

Direct thread outputs show linked nominal major diameter, axial pitch and run
length. The run starts at the evaluated thread origin and follows its axis.
Derived turns are run/pitch; metadata records internal/external mode and hand.
These labels describe driving geometry, not measured thread fit or compliance.
The feature's major-diameter glyph is a nominal diameter reference at the
starting plane. The shared collector uses bounded annotations independent of
turn count; native geometry/mesh budgets still apply.

Core/model/MCP checks cover parameter edits, native validity, analytical removed
volume, exact bounds, anchors, linked controls, failed-edit retention and
cleanup. A release-scale MCP gate checks 10 threaded-rod scenes within 30
seconds. No geometry schema or ABI change is required; the current catalog
uses model schema 91 and native ABI 52.

The modeled-thread scene gate passed in 2.451 seconds for 10 native views.
