# Model history and change impact

Schema 43 adds explicit revision records to `ModelDocument`. Older documents
load with an empty ledger. Generation audit records remain separate. Creating
another document with `from_graph` starts a new ledger; preserve the existing
document when editing an established model.

## Recording a revision

```rust,ignore
let previous = document.clone();
// Edit document parameters, definitions, instances, drawings, or assembly data.
document.record_revision(&previous, RevisionMetadata {
    id: "revision-002".into(),
    author: "Ben".into(),
    recorded_at: "2026-10-03T20:00:00Z".into(),
    message: "Increase bracket width".into(),
})?;
```

Both documents must validate and carry identical existing histories. IDs and
metadata strings must be nonempty; IDs must be unique. Each parent identifies
the preceding entry. No-op edits, including declaration reordering, fail without
modifying the document. Each record contains typed semantic changes with
before/after payloads. The ledger itself is excluded from those payloads, so
recording another edit does not recursively embed previous history.

The timestamp is caller supplied and treated as an opaque string. These are
review records, not snapshots or JSON Patch operations. Loading validates ledger
structure and change paths, but does not reconstruct or authenticate historical
model states. Preserve your baseline and version-control history if restoration
is required. Independent branches that append different histories produce a
three-way merge conflict; neither branch's history is silently discarded.

Recording costs document validation, semantic comparison, and linear history
scanning/copying. Appending after 10,000 entries in a 10,000-instance document
takes 0.185 seconds against a 10-second scale budget.

## Reporting impact

```rust,ignore
let report = previous.change_impact(&document)?;
for instance in report.instances {
    println!("{}: {:?}", instance.instance, instance.features);
}
```

The report resolves inherited and active-configuration overrides and derived
parameters. Direct feature changes use the same signatures as incremental
regeneration. A queue follows the union of old and new dependency edges to
include downstream features. Pinning an inherited parameter prevents unrelated
source edits from rebuilding that clone's geometry. Added and removed instances
list their complete parameter and feature sets.

Separate flags report placement, inherited material, suppression, and persisted
intent changes. A placement or material edit can affect a component without
rebuilding its local geometry. Enclosing frame and joint motion are included.
Placement comparisons preserve exact serialized representation, so two
geometrically equivalent representations may still be reported as changed.
Family declaration order and override-map order are ignored.

Referenced drawings are listed when a drawing definition or one of its model
inputs changes. Conservative flags request pattern refresh and assembly
verification. The report uses current stored poses and does not generate shapes,
refresh driven patterns, solve relationships, prove manufacturability, or predict
future collision results. Regeneration performs those applicable checks.

Shared parameter variants, clone material ancestry, and frame chains are cached.
Dependency propagation runs once per pair of variants; repeated instances copy
only their emitted report lists. Reporting 10,000 inherited instances with 100
dependent features takes 0.144 seconds against a 10-second budget, without kernel
handles. Revision and impact scale checks run through `tools/bench/run.sh`.
