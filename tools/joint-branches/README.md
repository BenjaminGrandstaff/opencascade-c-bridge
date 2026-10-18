# Alternative linkage pose discovery

`occt-joint-branches` searches closed-linkage assembly poses and writes one
reloadable model per discovered pose, plus a detailed report. It performs no
geometry generation or collision checks, and preserves the source model.

```sh
OCCT_BRIDGE_LIB_DIR="$PWD/build/bench" cargo build --release \
  --manifest-path rust/occt-parametric/Cargo.toml --bin occt-joint-branches
LD_LIBRARY_PATH="$PWD/build/bench" \
  rust/occt-parametric/target/release/occt-joint-branches \
  tools/joint-branches/example.model.json \
  tools/joint-branches/example.setup.json new-branches
```

The example is an illustrative crank-slider: 2 mm crank and 3 mm rod, driven
at 60 degrees. Its slider closes at 1 ± sqrt(6) mm. The grid searches rod angle
and slider translation, discovering both assembly poses. All quantities are
explicitly dimensioned; angles are dimensionless radians.

The setup schema is `occb-joint-branches-v1`. `axes` selects unique free joint
coordinates with explicit `seeds`; their Cartesian product supplies starting
poses. Omitted `options` fields use library defaults. Supported options are
`joint_solver` (`maximum_iterations`, `characteristic_length`),
`maximum_total_iterations`, `maximum_branches`, `include_current_pose`,
`distinct_normalized_distance`, and `equivalence` (`periodic_angles` or
`coordinates`). Unknown setup or option fields are rejected. Seed ranges affect
starts only; closure may converge outside them while respecting physical limits.

The new destination contains `report.json` and safe numbered files such as
`0001.model.json`. The report schema is `occb-joint-branch-report-v1`; its
`models` list corresponds to `search.branches` in first-discovery order. Models
retain source family definitions, frames, declarations and metadata, changing
only solved free coordinates. Validation and search complete before creating the
directory. Existing destinations are rejected; disk errors may leave partial
files. A report with zero branches or a partial search is still a successful
command execution; inspect `search.status` and the result counters.

Each returned pose passes the recorded assembly relationships. Budget stops
are explicit, and unsuccessful searches include the best failed candidate.
`seeds_exhausted` means all requested starts were attempted, not that all possible
branches were found. Underconstrained solutions may belong to a continuous family.
Branch discovery does not certify continuous motion connectivity, interference
freedom or a force equilibrium. Reload a selected branch for geometry/collision
checks and use its pose as the seed for a closed motion study.

See [the API and resource limits](../../docs/ASSEMBLY_MOTION.md#alternative-linkage-poses).

## Use through MCP

The [local MCP server](../model/MCP.md#launch-and-connect) exposes `occt_joint_branches`
when this executable is installed beside `occt-model` or in `--commands-dir`.
Retrieve the complete [linkage-branches example](../model/linkage-branches.request.json)
with `occt_get_example`, then pass it to the tool. Its request contains `schema`,
`model` and `setup`; `occt_get_schema` exposes the `joint_branches` schema.
The executable also prints that complete MCP request schema with `--schema`;
the positional CLI above continues to take separate model and setup files.

Returns the search report and reloadable branch models. Inspect termination status and bounds; a completed search does not prove exhaustive discovery. Build a selected branch separately to verify geometry and requirements.
Reports and named files are readable through returned `occt://analysis/...`
resources. Model inputs and accepted builds remain unchanged. See the
[shared workflow and resource limits](../model/MCP.md#drawing-inspection-and-mechanism-workflow).
