# 06: Apply and replay local Environments

**What to build:** Let a Linux user durably apply a planned local Profile, recover Projection from committed History, and navigate backward with `previous` without consulting Profile or Source state.

**Blocked by:** 05 — local resolution core already available; full public Plan completion depends on this issue's Recovery Inspection and issue 11's reload observations. Do not mark 05 complete before those integrations.

**Status:** done

## Progress

- Activation ID and validated History enforce contiguous monotonic sequence, strict records, cursor/cause invariants, and referenced Environment/Asset integrity.
- Local Profile apply revalidates under the exclusive lock, reconciles Projection, ensures immutable Asset/Environment records, and publishes the Activation at the durable no-replace commit point.
- `previous` replays durable Environments by History Cursor without Profile or Source resolution; the best-effort reload outcome remains separate from the committed Activation.

- [x] Asset and Environment stores distinguish missing, valid, and corrupt state.
- [x] Apply uses exclusive locking, atomic durable primitives, and Activation as the commit point.
- [x] Recovery Inspection is read-only and Reconciliation changes only derived Projection.
- [x] History validation, sequence identity, cursor semantics, and replay invariants follow RFC 0006.
- [x] `previous` works across replay-of-replay and repeated Environment cases without network or regeneration.
- [x] Reload failure does not invalidate durable activation.
