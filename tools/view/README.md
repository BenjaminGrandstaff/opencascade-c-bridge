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

Set `OCCT_VIEW_DRAWEXE` to use a DRAW executable that is not on `PATH`. If the
viewer cannot start, the view is still written and the error shows the command
to open it.

Colors come from the document's material appearances. Set one in code with
`InstanceGraph::set_material_appearance(material, Some(appearance))`; parts
without a material use DRAW's default color.

In DRAW, `vfit` refits the view and `vdisplay -dispMode 0 NAME` switches a part
to wireframe. The view is read-only: edit the model document and run the
command again to see changes.
