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

The page renders the model's glTF with WebGL: drag to orbit, Shift-drag or
right-drag to pan, wheel to zoom, and **Fit view** to reframe. The side panel
lists the family's parameters with their units: scalars and integers get a
number field and a slider between their limits, booleans a checkbox and
choices a list; vector parameters are shown but edited in the file.

- An edit changes the parameter's default in the family, regenerates and
  redraws within a moment, keeping the camera. Edits to several parameters
  merge, and one request is in flight at a time, so none is lost.
- An edit that fails validation or regeneration (outside a parameter's limits,
  for example) is rejected as a whole: the field reverts, the message stays
  in the panel, and the model is unchanged.
- **Save** writes the edited document over the model file, through a
  temporary file and a rename so other readers never see a partial file.
  Nothing is written until then.
- A save made elsewhere, by an editor or another tool, reloads the page within
  about a second once two checks agree. It replaces unsaved browser edits. A
  file that does not load is reported and the last model stays.

Edits change family defaults, so they apply to every instance without its own
override. The page needs no external files. The server listens only on
127.0.0.1, accepts only `127.0.0.1` or `localhost` Host headers, and accepts
edits only with the `X-OCCT-View` header that the page sends, which other
sites' pages cannot add; it is a local editing tool, not something to expose.

Each edit costs one full regeneration and glTF export. Instances sharing a
generated variant are tessellated once, so 10,000 pattern members export in
about a quarter second. The renderer's tests run with
`node --test rust/occt-parametric/src/bin/view/web/viewer.test.mjs`.
