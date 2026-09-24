# 06: Apply and replay local Environments

**What to build:** Let a Linux user durably apply a planned local Profile, recover Projection from committed History, and navigate backward with `previous` without consulting Profile or Source state.

**Blocked by:** 05 — local resolution core already available; full public Plan completion depends on this issue's Recovery Inspection and issue 11's reload observations. Do not mark 05 complete before those integrations.

**Status:** in-progress

## Progress

- Activation ID encodes validated monotonic sequence; regression tests cover limits and canonical spelling.
- Read-only History inspection validates contiguous records, replay cursor/cause, strict schema/timestamps and referenced Environment/Asset bytes under shared lock. `previous_target` resolves cursor; no replay commit yet. Tests include truncated PNG despite matching digest and magic.
- Apply publication, Projection reconciliation, `previous` commit, and reload remain open; do not claim issue complete. Storage traversal still needs race-safe directory pinning; duplicate assets outside canonical shard are not yet checked.

- [ ] Asset and Environment stores distinguish missing, valid, and corrupt state.
- [ ] Apply uses exclusive locking, atomic durable primitives, and Activation as the commit point.
- [ ] Recovery Inspection is read-only and Reconciliation changes only derived Projection.
- [ ] History validation, sequence identity, cursor semantics, and replay invariants follow RFC 0006.
- [ ] `previous` works across replay-of-replay and repeated Environment cases without network or regeneration.
- [ ] Reload failure does not invalidate durable activation.
