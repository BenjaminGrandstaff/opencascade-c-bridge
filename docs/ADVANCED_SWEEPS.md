# Advanced sweeps

Schema 175 and ABI 82 extend the `sweep` feature. A profile can now twist
about its path, scale along it, follow a guide rail, or blend through
further sections. A new `path_plane` datum lets profile and section sketches
sit on planes normal to the path, so they follow the path's constraints.
Sweeps without the new fields keep their earlier native operation and
results.

```json
"sweep": {
  "profile": "inlet_section",
  "path": "path",
  "orientation": "corrected_frenet",
  "twist": {"angle_radians": {"parameter": "twist"}, "law": "smooth"},
  "sections": [{"profile": "outlet_section",
                "position": {"literal": {"value": 1, "dimension": "scalar", "unit": null}}}]
}
```

The [advanced-sweep example](../tools/model/advanced-sweep.request.json) is a
duct along a constrained route: a horizontal run, then a tangent arc. Run
length, bend radius and bend angle are solver dimensions. The duct's square
inlet and outlet sit on `path_plane` datums at the route's two ends, and it
twists a quarter turn and flares between them.

## Options

| Field | Meaning | Limits |
|---|---|---|
| `twist` | `angle_radians` turned about the path from start to end, relative to least-twist transport, following `law` | Corrected-Frenet orientation; a path without sharp corners; no guide |
| `scale` | Size from 1 at the start to `end` at the end, about the path, following `law` | `end` positive; no sections, no scaling guide |
| `guide` | `input`, an edge or wire beside the path, which the profile turns to keep facing; with `scales`, it is also resized about the path in proportion to the rail's distance | Corrected-Frenet orientation; no twist; the rail must cross every plane normal to the path, away from the path |
| `sections` | Further profiles at `position`s, fractions of the path's length, blended smoothly | At most 100, increasing, the last at 1; single-boundary profiles; no scale law or scaling guide |

`law` is `linear` (in proportion to the length swept) or `smooth`
(3t² − 2t³, level at both ends). Every profile must be closed, or every
profile open. Holed profiles sweep each boundary with the same twist,
scale and guide, then subtract the inner volumes, as plain
[hollow sweeps](HOLLOW_SWEEPS.md) do.

## Following constrained paths

```json
{"id": "outlet", "kind": {"path_plane": {
  "input": "path",
  "position": {"literal": {"value": 1, "dimension": "scalar", "unit": null}}}}}
```

A `path_plane` is normal to the path output `input` (an edge or wire) at
`position`, a fraction of its length from the start. It faces along the
path. A sketch with `datum_plane` set to it is placed where the path
crosses the plane, and the sketch's x axis is projected into the plane. For
a route in one plane, an x axis along that plane's normal stays in every
path plane. The sketch depends on the path. When constraint dimensions or
parameters move or turn the path, its profile or section moves and turns
with it, so the sweep stays valid. Profiles placed by fixed coordinates do
not follow edits like this.

## Studio

- **Sweep editor:** the solid editor's sweep form (*Sketch to solid*,
  *Sweep along path*, or *Edit sweep*) adds rows for:
  - twist: an entered angle in degrees, a scalar parameter in radians, or a
    kept formula, with its law;
  - scale along the path;
  - a guide rail that turns, or turns and resizes, the profile;
  - sections, each a profile and a position (add and remove rows).
- **Editing:** an edit keeps every saved option, parameter link and formula.
- **Checks before preview:** the form refuses the same conflicts as the
  kernel, with plain messages. Native preview then checks the solid as for
  other solid edits.
- **Plane tool:** the sketch studio's *New plane* tool has a *Normal to
  path* type, which picks a sketch wire or helix and a position from 0 to 1.
  Its position becomes a parameter in [0, 1], editable under *Driven by*.
- **Placement:** *Sketch on this plane* starts a profile that the path's
  constraints carry with it.

## Native behaviour

ABI 82 adds `occt_bridge_sweep_advanced` and `occt_bridge_path_station`.
Each sweep is one OCCT pipe-shell build:

- **Twist:** an auxiliary spine wound about a rotation-minimizing frame
  (double reflection), sampled at least 64 times and 64 times per turn (at
  most 8,193 points). The spine sits within half the tightest bend radius,
  so each plane normal to the path meets it once. Twisted faces are
  approximated to 1e-5.
- **Scale:** an OCCT linear or S law over the path's normalized length.
- **Scaling guide:** the bridge samples the rail's distance from the path
  in each normal plane (64–1,024 samples) and interpolates a scale law. A
  scaling guide therefore resizes about the path, keeping the end centred.
  OCCT 7.9's own contact mode resizes about another point.
- **Sections:** placed at path stations. A final section is required,
  because OCCT extrapolates past the last one.

The folding check compares each profile's reach from its own path point,
times the largest scale, with the path's tightest bend.

## Verification and scale

The checks below compare against exact volumes:

- **Bridge tests:**
  - twists and multi-turn twists on lines and arcs;
  - linear and smooth scale laws;
  - scaling guides, including the centred end;
  - a helical guide setting the end angle;
  - flared and bulged sections (quadratic interpolation);
  - path stations;
  - conflicting options, rejected with no partial result.
- **Feature tests:**
  - the example duct, re-dimensioned through its constraints;
  - a scale-law duct;
  - a twisted square tube with a hole;
  - JSON round trips and dependencies.

The scale bench builds 250 twisted, sectioned ducts on one constrained route
and rebuilds them after a route edit in 5.9 s (10 s budget). That is about
11 ms per duct, about 7 ms of it the twist's auxiliary spine.
