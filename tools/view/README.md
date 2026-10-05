# Model viewer

`occt-view` regenerates a saved model document and opens every part in OCCT's
DRAW viewer, shaded, named after its instance and colored by its material
appearance.

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
