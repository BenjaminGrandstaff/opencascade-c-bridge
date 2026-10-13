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

## Runout (schema 114)

An optional `runout` (a length, at most ten pitches) ends the thread with a
fading groove instead of an abrupt step:

```json
"thread": { "...": "...", "length": {"parameter": "thread_length"},
            "runout": {"literal": {"value": 3, "dimension": "length", "unit": "millimeter"}} }
```

After the full-depth `length`, the groove continues for `runout` along the
axis while its path moves radially by the basic depth plus the P/16
overshoot: outward for external threads, inward into the bore for internal
ones. The groove therefore leaves the material and its depth fades linearly
to zero. The path is the helix joined to an interpolated spiral (16 points
per turn, at least 4) that starts at the helix's native end point and
tangent; a mismatch between the analytic and native helix end is rejected.
One binormal sweep follows both, so there is no seam in the groove.

Tests use the M10 fixture at both hands and sides. A two-pitch runout removes
37% (external) or 34% (internal) of what two more full-depth turns would, with
left- and right-handed runouts equal to 1e-4. In the last of three runout
pitches the fade removes under a quarter of what it removes in the first (8%
measured), and nothing beyond the runout. Negative and over-ten-pitch runouts
fail without leaking handles. A 100-turn M10×1.5 thread with a ten-pitch
runout is a required scale case (12 s budget). Modeled tapped holes take the
same `runout`; see [modeled tapped holes](HOLES.md#modeled-tapped-holes-schema-113).

## Unified inch classes (ASME B1.1, schema 152)

A modeled thread or a tapped hole's `thread` callout may name the Unified
series and class it represents:

```json
"unified": {"series": "unc", "class": "2B"}
```

- `series`: `unc`, `unf` or `unef` (the diameter and pitch must be a
  standard size of that series), `un` (any whole or half pitch on a
  standard diameter), or `uns` (any diameter, whole or half threads per
  inch).
- `class`: `1A`, `2A` or `3A` for external threads and `1B` or `2B` for
  internal ones; it must match the thread's side. Class 3B is not
  supported.
- `rounded_root` (external only) names the UNR root in the designation.
- `length_of_engagement` (optional) widens the pitch-diameter tolerance for
  long engagements; the standard tables use the basic major diameter.

On a tapped hole, the callout's `designation` must be the standard one,
such as `1/4-20 UNC-2B`, or `1/4-20 UNC-2B-LH` when left-handed. The
drilled bore, which becomes the thread's minor diameter, must lie within
the class's minor-diameter limits. A #7 tap drill (0.201 in) passes for
1/4-20 UNC-2B (0.196–0.207 in); a 0.190 in bore fails. Modeled threads keep
the basic profile, which the class limits bound.

`UnifiedThread::limits` gives the limits of size in inches, each as minimum
and maximum: the major diameter (external, or the internal minimum), the
pitch diameter, the internal minor diameter and the allowance.
`unified_fit` pairs an external and an internal thread and returns the
least and greatest pitch-diameter clearance and the least major-diameter
clearance; `assembles()` holds when neither least clearance is negative.

The limits follow B1.1's tolerance formulas as published by calculators
that reproduce its tables. Listed in `screw_threads.rs`, they are checked
against published 2A and 2B tables for ten UNC and UNF sizes from #0 to
1 in, and against 1A and 3A values for 1/4-20. Four values lie one
rounding step from the tables, on rounding boundaries:

- #4-48 2A maximum pitch diameter: 0.0978 (table 0.0979);
- #2-64 2B maximum minor diameter: 0.0752 (table 0.0753);
- #10-24 2B maximum minor diameter: 0.155 (table 0.156);
- 1/4-20 2B maximum pitch diameter: 0.2223 (table 0.2224).

The standard sizes come from the Unified Thread Standard size table:
#0 to 4 in coarse, #0 to 1-1/2 in fine and #12 to 1 in extra-fine. Other
UNEF sizes must use `uns`. External minor diameters, which B1.1 gives only
for reference, are not computed. A check costs O(standard sizes), at most
34 comparisons.

### Inspecting threads against the limits

An `InspectionRecord` may list measured `threads`: each names an `instance`
and its `feature` (a `thread` feature or a threaded `hole` with a Unified
class), with the measured `pitch_diameter_mm` (for example by the three-wire
method) and optional `major_diameter_mm` and `minor_diameter_mm`:

```json
"threads": [{"instance": "part", "feature": "tapped", "pitch_diameter_mm": 5.588,
             "minor_diameter_mm": 5.105}]
```

The report's `threads` gives each thread's standard designation and, per
measured diameter, its limits in millimetres (absent where B1.1 sets none)
and whether it conforms. Measurements are decided under the record's
`uncertainty` rule with its `expanded_mm`, as for GD&T controls: guarded
acceptance needs the value U inside its limits, guarded rejection rejects
only beyond U outside them. `InspectionReport::conforms()` needs every
measured thread to conform. Unknown features, features without a Unified
class, repeated threads and nonpositive or nonfinite values fail the record.
The `occt-inspection-report` command accepts the same list in a group's
`points` and counts `threads_conforming` and `threads_nonconforming`.

Each measured thread resolves its instance's parameters and finds its
feature, so a record costs O(threads · (parameters + features)). A required scale case judges 10,000
measured tapped holes, 100 on each of 100 instances.

Gauging (GO/NO-GO, B1.3 gauging systems) and functional-size checks are not
modeled; the checks are of measured diameters against the limits.

## Not yet included

- Chamfered thread starts on external threads (holes can use a countersink).
- Metric (ISO 965) tolerance classes and class 3B. Their fundamental
  deviations, tolerances and 3B minor diameters are tabulated in the
  standards with rounding the published formulas do not reproduce (for
  example the g deviation for a 2 mm pitch is 38 µm against the formula's
  37), and no public source of the full tables was found.
- Truncated or rounded roots in the modeled geometry; the profile is the
  basic profile.
- Tapered (pipe) threads, multi-start threads and non-60° forms (ACME,
  buttress).
- Runout at the thread start; runout only fades the end.


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
