# Feature timeline: checkout, suppress and move

Schema 182 adds a timeline to each family. Feature declaration order is its
history order; dependency order still controls geometry execution. The native
ABI stays at 85. Older documents migrate to a full, unsuppressed timeline.

```json
"history": {
  "head": "moved",
  "suppressed": ["notched", "spare"]
}
```

`head` checks out the timeline through that feature, inclusively. Later features
are rolled back. Omit `head` (or use null) to restore the full timeline. Omit
`history` entirely for the original behavior. Checkout does not delete features
or their parameters, requirements and references.

Suppression turns a feature off without removing it. A modifier such as a hole,
fillet, cut or transform passes its primary input through under its own feature
ID. Later operations rebuild on that substituted body. A new-body feature such
as a primitive or sketch has no input to pass through; its dependent features
are blocked. Suppressing a boolean passes through its left/object body, not its
tool. Restoring the feature replays its normal operation.

## States in build reports

Accepted build reports, edit previews, and reports rejected by required
verification include `feature_states`, grouped by family, in timeline order:

```json
"feature_states": [{
  "family": "history-part",
  "head": "moved",
  "features": [
    {"feature": "base", "state": "active"},
    {"feature": "notched", "state": "suppressed", "passes": "base"},
    {"feature": "spare", "state": "suppressed"},
    {"feature": "spare-moved", "state": "blocked", "by": "spare"},
    {"feature": "later", "state": "rolled_back"}
  ]
}]
```

`active` features execute normally. A suppressed modifier can still provide an
output through `passes`; suppressed new bodies, blocked features and rolled-back
features provide none. Explicitly selecting an unavailable output rejects the
build. Family requirements on unavailable outputs are retained but not evaluated;
checks on substituted outputs still run and required failures still reject.
A checked-out part therefore does not establish that its unfinished features
satisfy their requirements. Assembly checks retain their normal behavior.

The [complete feature-history example](../tools/model/feature-history.request.json)
builds a translated cube with a suppressed notch, an independently suppressed
body and blocked dependent, and a rolled-back final translation.

## Guarded edits through CLI and MCP

The existing `occb-model-edit-v1` request accepts three additional `changes`:

```json
{"action":"checkout_feature", "family":"history-part",
 "expected":{"head":"moved","suppressed":["notched","spare"]}, "head":null}
```

```json
{"action":"suppress_feature", "family":"history-part", "feature":"notched",
 "expected":{"head":"moved","suppressed":["notched","spare"]}, "suppressed":false}
```

```json
{"action":"move_feature", "family":"history-part", "feature":"tool",
 "expected":["base","tool","notched","spare","spare-moved","moved","later"],
 "after":null}
```

For checkout and suppression, `expected` is the original family's full history
object (`{}` for the default). For moves, it is the original ordered list of
feature IDs. Guards in a transaction compare against the same starting snapshot.
`after: null` moves first; otherwise it names the feature to follow. Moves that
cross an input or a dependent are rejected before publication. Feature IDs and
operations stay unchanged. An individual feature may have distinct suppression
and move changes in one transaction; duplicate changes of the same kind reject.

Supply the usual outputs and revision metadata. The CLI runs these through
`occt-model --edit REQUEST.json NEW_DIRECTORY`; MCP exposes the same generated
change schema through `occt_edit_build` and `occt_preview_edit_build`, with the
existing accepted-build hash guards. Retrieve `feature-history` using
`occt_get_example`. See [guarded edits](../tools/model/MCP.md#guarded-edits-and-revision-history)
for the complete request envelope and fingerprint workflow.

Successful edits bump the family's version and record actual semantic changes.
Reordering surviving features adds a `feature_order` change alongside per-ID
operation changes. Three-way merges preserve timeline order, combine compatible
insertions, and report conflicting ordering constraints instead of sorting the
features by ID. History state changes enter change-impact signatures and
propagate to downstream users. Failed edits leave accepted source builds intact.

## Studio controls

The studio's **Solid features → Feature history** list shows every feature and
its state, including a blocked dependency or substituted input. Each row has
**Checkout**, **Suppress/Restore**, and earlier/later move controls.
**Restore latest** removes the checkout boundary; it preserves explicit
suppressions. Dependency-invalid moves show an error and retain the accepted
model. Apply or discard a sketch draft and finish source picking before changing
history.

Checkout displays the last available output at the selected timeline position.
Suppression keeps the selected output when it remains available, otherwise it
shows the last available feature. An edit leaving no output is rejected. Inactive
sketches are excluded from the studio's sketch layers. Edits regenerate before
acceptance, join the existing **Undo/Redo** stack, and remain unsaved until
**Save**. Reopening a saved model selects an available history output by default;
an explicit unavailable `--output` still rejects. Shared-session changes from MCP
also switch to an available history output when they roll back the studio's
previous selection.

This feature timeline is separate from the append-only
[document revision ledger](MODEL_HISTORY.md), studio undo snapshots, and native
face/edge provenance. It does not add feature branches, named timeline bookmarks,
or checkout before the first feature. State propagation uses a queue in
O(features + dependency edges); the scale gate exercises 100 state walks through
10,000 reverse-ordered features.
