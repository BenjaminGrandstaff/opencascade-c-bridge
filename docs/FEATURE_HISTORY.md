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
face/edge provenance. It does not check out before the first feature. State propagation uses a queue in
O(features + dependency edges); the scale gate exercises 100 state walks through
10,000 reverse-ordered features.

## Bookmarks and branches (schema 185)

Bookmarks and branches work like git tags and branches over the timeline.

```json
"history": {
  "head": "moved",
  "suppressed": ["notched", "spare"],
  "bookmarks": [{"name": "notch reverted", "head": "moved", "suppressed": ["notched", "spare"]}],
  "branches": [{"name": "further", "base": "spare-moved", "features": ["..."], "head": "moved", "suppressed": ["notched", "spare"]}]
}
```

**Bookmarks.** A bookmark names a timeline state: a checkout `head` and its
suppressions. Checking out a bookmark restores both. Bookmarks belong to the
branch they were made on and travel with it.

**Branches.** A branch is an alternative continuation of the timeline: after
feature `base` (or from the start, when there is no base), the features go on
as the branch lists them instead.
- **The active branch** is the family's own `features`. It is named by
  `branch`; omitted means `main`.
- **Stored branches** are in `branches`. Each keeps only its features after
  its base, with its own head, suppressions and bookmarks. Features up to the
  base are shared, so an edit there reaches every branch that continues
  after it.
- **Creating a branch** stores a copy of the active branch's continuation
  after the chosen base, which you then change on its own.
- **Switching** stores the active branch's continuation and timeline state
  under its name, and makes the chosen branch active.
- **Merging** appends the branch's features that the active branch lacks,
  in the branch's order, after the active ones, so all their inputs come
  first. A feature both define differently is a conflict. The merge is then
  refused, names the features, and changes nothing. The merged branch stays
  stored.

Parameters, requirements, references and datums are shared by all branches.
The document is refused when:
- a stored branch redeclares a shared feature;
- a bookmark or branch state names a missing feature;
- the feature a branch continues after is gone. Deleting that feature
  requires deleting or merging the branch first.

Names are 1 to 64 letters, digits, spaces, `-`, `_` or `.`. A branch keeps at
most 1,000 bookmarks, a family 100 stored branches, and a branch 10,000
features. A switch is linear in features: 100 switches of a 5,000-feature tail
of a 10,000-feature family take 58 ms with validation (5 s budget).

### Editing

One guarded `timeline` change does each action:

```json
{"action": "timeline", "family": "history-part",
 "expected": {"head": "moved", "suppressed": ["notched", "spare"]},
 "expected_order": ["base", "tool", "notched", "spare", "spare-moved", "moved", "later"],
 "timeline": {"kind": "create_branch", "name": "further", "base": "spare-moved"}}
```

`kind` is `bookmark`, `delete_bookmark`, `checkout_bookmark`, `create_branch`
(with an optional `base`), `switch_branch`, `delete_branch` or
`merge_branch`. `expected` is the family's whole history object and
`expected_order` its feature IDs, both as read. A timeline change must be the
family's only change in its transaction. Merge conflicts fail with stage
`conflict` and list the features in `branch_conflicts`. Build reports add
`branch`, `branches` and `bookmarks` to `feature_states` when a family has
any.

### Studio

**Feature history** shows the active branch and lists the other branches with
**Switch**, **Merge** and **Delete**. **New branch** takes a name and the
feature it continues after (the last one by default). **Bookmark this
timeline** saves the current checkout and suppressions under a name, and each
bookmark has **Checkout** and **Delete**. Every action joins Undo/Redo, and
Save writes the branches into the model.

### Tests

- **Core:** bookmarks, branch switching, edits before the base reaching
  other branches, merges and conflicts, validation, and a JSON round trip.
- **Command line:** branch geometry differs and switches back exactly; a
  bookmark travels with its branch; conflicting merges, stale guards and
  mixed transactions are refused.
- **Studio:** switch, Undo/Redo, bookmarks and save/reopen; the panel's
  controls; and the
  [feature-history example](../tools/model/feature-history.request.json),
  which now carries a bookmark and a stored branch.
