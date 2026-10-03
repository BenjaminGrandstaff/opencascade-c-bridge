# Semantic document comparisons and merges

`ModelDocument::semantic_diff(&other)` compares persisted model intent without
creating geometry. It returns deterministic `DocumentChange` records containing
typed field/entity paths and optional before/after JSON values.

```rust
let changes = accepted.semantic_diff(&edited)?;
for change in changes {
    println!("{:?}: {:?} -> {:?}", change.path, change.before, change.after);
}
```

Family declarations and top-level instances, frames, patterns, additional
families, relationships, requirements, configurations, and materials are matched
by stable IDs. Moving an instance in the serialized list produces no edit;
changing its override reports the instance ID, variant, override name, and
changed value. Duplicate IDs fail rather than silently dropping entities.
IDs containing slashes, tildes, or brackets remain distinct typed path segments.

Ordered lists, including sketch profiles, expression operands, pattern members,
and generation audit records, retain their order. Changes to those lists are
reported as whole values. Numbers and units use exact serialized comparisons:
equivalent geometry or equivalent quantities in different units can still
produce changes. Load older documents through `ModelDocument::from_json` before
comparing them so migration defaults are applied consistently.

An absent before/after value means an addition/removal; a present JSON null means
an actual null field. Change records preserve that distinction when serialized.
Added and removed entities use canonical payloads, where nested declaration
lists are ID maps. Records describe changes for review; they are not JSON Patch
operations. No model schema or C ABI change is needed.

For S serialized bytes and at most N entries per object or collection, comparison costs
O(S log N + P + C) time and O(S + P + C) memory, including output path bytes P and
copied changed payload bytes C. It creates no kernel handles. The dedicated
benchmark compares 10,000 instances ten times, including reordered declarations
and one precisely located override edit, with a five-second budget:

```bash
LD_LIBRARY_PATH="$PWD/build" \
  cargo bench --manifest-path rust/occt-parametric/Cargo.toml --bench document_diff
```

This case also runs through `tools/bench/run.sh`.

## Three-way merge

`base.three_way_merge(&left, &right)` combines documents edited from a common
base. It returns `DocumentMerge::Merged(Box<ModelDocument>)` for a validated
result or `DocumentMerge::Conflicts(Vec<DocumentConflict>)` for incompatible
edits. Invalid inputs or an invalid combined model return `ModelError` with
the input side or merged-document context.

```rust
match base.three_way_merge(&left, &right)? {
    DocumentMerge::Merged(document) => {
        let json = document.to_json_pretty()?;
        // Review or persist json using the application's normal workflow.
    }
    DocumentMerge::Conflicts(conflicts) => {
        for conflict in conflicts {
            println!("{:?}: {:?} / {:?}", conflict.path, conflict.left, conflict.right);
        }
    }
}
```

Independent IDs, fields, and override-map entries combine. Identical edits
coalesce. Delete/edit conflicts, different additions of the same entity ID,
incompatible enum replacements, and different edits to ordered arrays require
resolution. Conflict records contain the typed path and canonical base, left,
and right values, preserving null versus absence across serialization. Empty
dictionaries omitted by serialization behave like empty maps; deleting their
last entry can combine with an unrelated entry added on the other branch.

The merge leaves inputs unchanged and sorts declaration collections by ID.
It returns no partial document when conflicts exist. After combining edits it
checks document references, dependency graphs, parameter bounds, and existing
model constraints. For example, removing an instance on one branch and adding
a clone of that instance on the other branch fails combined-model validation,
even though each branch is valid. Geometry is checked during regeneration.

The tree merge costs O(S log N + S D + P + C) time and O(S + P + C) memory, where
D is document nesting depth, P conflict path bytes, and C conflict payload
bytes. Subtree equivalence checks account for the S D term and skip unchanged
branches. Input and output document validation adds its existing graph and
parameter costs. The dedicated scale case merges 10,000 reordered instances ten
times, checking two independent overrides on one instance against an
eight-second budget:

```bash
LD_LIBRARY_PATH="$PWD/build" \
  cargo bench --manifest-path rust/occt-parametric/Cargo.toml --bench document_merge
```

Both scale cases run through `tools/bench/run.sh`.

## Git merge driver

The `occt-document-merge` binary takes `BASE CURRENT OTHER`, migrates and validates
all three JSON documents, and uses the semantic three-way merge above. Success
atomically replaces `CURRENT` with the validated merged document and preserves
its file permissions. Conflicts, malformed documents, validation failures, and
write errors exit nonzero; conflicts leave `CURRENT` byte-for-byte unchanged and
print typed paths and payloads to stderr. Exclusive temporary creation and a
same-directory rename prevent partial writes. Input files must be separate.

Build it using the same library path as the other Rust tools:

```bash
OCCT_BRIDGE_LIB_DIR="$PWD/build" \
  cargo build --release --manifest-path rust/occt-parametric/Cargo.toml \
  --bin occt-document-merge
```

Install the driver in the repository whose model documents you want to merge:

```bash
git config --local merge.occt-document.name 'OCCT semantic model merge'
git config --local merge.occt-document.driver \
  'env LD_LIBRARY_PATH=/absolute/path/to/bridge/build /absolute/path/to/occt-document-merge "%O" "%A" "%B"'
```

Add an appropriately scoped rule to that repository's `.gitattributes`:

```gitattributes
models/*.json merge=occt-document
```

Git retains unmerged stages after a conflict. Resolve the model and its revision
history deliberately, validate it, then stage it normally; the driver does not
insert text conflict markers into JSON. Actual Git merges were checked for
independent edits and incompatible edits, including preservation of current
bytes and Git's unmerged stages. Installation is explicit: the bridge repository
does not modify another repository's Git settings automatically.

Revision records and change impact are described in [Model history](MODEL_HISTORY.md).
