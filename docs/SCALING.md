# Uniform geometry scaling

Schema 83 adds `scale` using the existing native scale API. Native ABI remains
52. The [scaled-part example](../tools/model/scaled-part.request.json) scales an
asymmetric fused bracket with parameter-driven `center` and `factor` expressions.

`input` names an earlier feature. `center` is a length-valued vector in family
coordinates; `factor` is a finite, positive dimensionless scalar. Each point P
becomes C + factor × (P − C). Negative factors and zero are rejected; use
[mirror](MIRRORS.md) for reflection. Distances scale by the factor, areas by its
square, and volumes by its cube. Family datums and assembly frames remain
separately authored.

Nonunit scaling copies native geometry, leaving its source unchanged. The
result preserves source modification history and follows the session's result
validation/healing policy. Extreme factors may exceed kernel tolerances and
fail. Exactly unit scaling may share geometry through a rigid location.
Rebuild the native library with this change: factors close to one previously
fell into a tolerance-based rigid-transform path and now scale correctly.

Centre and factor parameter dependencies enter incremental regeneration.
Changing either reuses the input features and rebuilds the scale output and
its dependents. A failed edit retains accepted geometry without leaking
handles. Saved documents round-trip and older schemas migrate to 83.

The solid viewer shows measured result spans and a linked `scale × factor`
label. The label is anchored at the result bounds centre; its dimensionless
value does not use a length arrow. Metadata includes the evaluated scale
centre, factor and source expressions. AI authoring schemas expose `scale`,
and MCP provides the complete `scaled-part` request.

Native cost follows topology and transformed curve/surface data. Shared linked
instances reuse one generated variant. Release gates check 500 resized parts
in 2.433 seconds and 10,000 linked copies in 0.426 seconds, each within a
10-second budget, verifying validity, volume, bounds, ancestry and cleanup.
Tests cover shifted centres, factors 0.001 and 1,000, factors 1 ± 1e-8,
dimensional errors, selective rebuilding and failed-edit rollback.
