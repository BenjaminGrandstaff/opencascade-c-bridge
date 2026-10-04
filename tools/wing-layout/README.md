# Wing layout workshop

An isolated wing station editor and OCCT CAD handoff for a Ho 229-inspired RC workflow. It does not modify the core assembly, motion or meshing code.

## Open the editor

From the repository root:

```sh
python3 -m http.server 8790 --bind 127.0.0.1 --directory tools/wing-layout
```

Open http://127.0.0.1:8790. The browser tool has no package dependencies or external assets.

Edit full span and spanwise stations: chord, leading-edge offset, vertical offset, geometric twist and chordwise twist pivot. Select a station to highlight it, import a Selig `.dat` airfoil, insert intermediate stations, and save/reopen the project. All dimensions are millimeters. Fractions run from centerline to tip. Both halves are mirrored about Y=0. X points aft, Y right, Z up; positive incidence raises the leading edge.

`example.json` is the same editable illustrative example offered by Reset. Its span, planform, airfoil profiles and twist are **assumed**. It is not a verified historical Ho 229 reconstruction. Generated symmetric profiles are placeholders; no airfoil zero-lift angles are supplied. The Smithsonian's [Ho 229 V3 preservation project](https://airandspace.si.edu/explore/researchers/projects/horten-ho-229-v3-preservation) is a historical reference, not the numerical source for these station values.

Geometric twist and airfoil shape are separate inputs. Where the user supplies a section zero-lift angle α₀, the chart also shows geometric twist minus α₀. This is a section incidence quantity. It does not account for induced angle, section Reynolds number, pitching moment, center of gravity, trim or stability. The area, aspect ratio and mean aerodynamic chord use the untwisted planform with linear chord interpolation. No CG recommendation is implied.

## Export actual CAD

Click **CAD sections JSON** and save `wing-sections.json`. The export contains both wing halves with corresponding 80-point polygon sections, using 40 cosine intervals per surface. Profiles are closed at the trailing-edge midpoint, so a finite trailing-edge gap is approximated. Leading-edge endpoints slightly off x=0 and trailing-edge endpoints slightly off x=1 are extended to normalized chord endpoints. Import normalized Selig coordinates in upper-then-lower surface order; Lednicer files with point count headers are rejected.

Build the C bridge as usual, then run this separate CLI from the repository root (adjust the input/output paths):

```sh
OCCT_BRIDGE_LIB_DIR="$PWD/build" \
LD_LIBRARY_PATH="$PWD/build" \
CARGO_TARGET_DIR=/tmp/occb-wing-layout-target \
cargo run --manifest-path tools/wing-layout/cad/Cargo.toml -- \
  /path/to/wing-sections.json /path/to/wing.step
```

### Parametric model from the project file

Pass the saved project (`occb-wing-layout-v1`, from **Save project**) instead of the sections file:

```sh
OCCT_BRIDGE_LIB_DIR="$PWD/build" \
LD_LIBRARY_PATH="$PWD/build" \
CARGO_TARGET_DIR=/tmp/occb-wing-layout-target \
cargo run --manifest-path tools/wing-layout/cad/Cargo.toml -- \
  /path/to/wing-project.json /path/to/wing.step
```

The CLI then builds a parametric family in `occt-parametric` and writes `wing.model.json` beside the STEP and BREP files. Its parameters are `span` and, for each station `i`, `chord_i`, `leading_edge_i`, `height_i`, and `twist_i` (radians), so a change to one value regenerates the wing from the document instead of re-exporting sections. Each half is a `Loft` feature through **smooth** sections: every airfoil is one B-spline interpolated through the same 80 resampled points, with a sharp corner only at the trailing edge. Spanwise panels stay ruled, as above. Stored requirements check that each half is a valid single solid, and the CLI prints their results. On the starter layout, smooth sections enclose about 0.1% more volume than the inscribed polygons, and the STEP file is about a sixth of the size.

### Polygon sections

The CLI creates a **ruled solid loft** for each half and a compound holding both touching solids. It writes STEP and BREP, verifies each half has a valid positive-volume solid, and reopens both exported files to check validity and volume. Sections use polygon edges, so the output approximates curved airfoils. Between stations the loft is ruled; intermediate sections need not reproduce a rotation with linearly interpolated twist. This version creates the outer wing solid, not ribs, spar channels, control surfaces, print segmentation, or the Ho 229 center-body/engine geometry.

The separate Cargo manifest depends on the existing `occt-bridge` and `occt-parametric` crates and does not change their APIs. Build artifacts are directed to `/tmp` in the commands above.

## Verification

```sh
node tools/wing-layout/model.test.mjs
LD_LIBRARY_PATH="$PWD/build" node tools/wing-layout/cad.test.mjs \
  /tmp/occb-wing-layout-target/debug/occb-wing-cad
```

The model tests check analytic rectangular/trapezoidal planform measurements, twist sign and mirroring, Selig ordering, common section sampling, project serialization and invalid inputs. CAD checks verify exact prismatic volumes, valid starter lofts, STEP/BREP roundtrips and rejection of nonplanar sections. With the local server running, `browser.test.html` exercises rendering, span/twist editing, station insertion, reset and invalid-input recovery and displays PASS or FAIL.

## Next aerodynamic milestone

Obtain attributable Ho 229 spanwise section coordinates and twist/incidence definitions, enter those values with provenance, and compare the geometry against the source. Then connect a swept-wing aerodynamic solver to assess span loading, trim, static pitch stability and tip stall over an RC Reynolds-number envelope. A prescribed twist curve alone cannot establish those results. Structural/material design can follow a verified aerodynamic layout.
