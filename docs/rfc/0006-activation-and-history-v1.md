# RFC 0006: Activation and History v1

Status: Accepted
Date: 2026-09-23

## Purpose

This RFC defines durable Activation records, local append-only History, the apply commit point, and backward navigation.

An Activation asserts that one Environment became the durable managed state. It is not an Environment, Plan, Projection, or runtime reload result. Applying the same Environment twice creates two distinct Activations.

Ghostty reload occurs after the durable commit and is outside Activation semantics.

## v1 command scope

Durable activation in v1 is initiated only by:

```text
apply PROFILE
previous
```

Planning is available through:

```text
plan PROFILE
```

Direct `random` and `set` commands are outside v1. Random and fixed-path wallpaper behavior remains expressible through Profiles.

## Identity and sequence

Each Activation has a local monotonic `sequence` in:

```text
1..=9_007_199_254_740_991
```

Its identifier is:

```text
activation_id =
    "act-v1-" || sequence encoded as exactly 16 lowercase hexadecimal digits
```

Example:

```text
sequence = 42
activation_id = act-v1-000000000000002a
```

Committed sequences are strictly increasing, contiguous, never reused, and never renumbered. History v1 is local to one managed directory and is not mergeable across machines.

Activation identity does not depend on timestamp, Environment, Profile, or record bytes.

## Storage

History is stored as one immutable record per Activation:

```text
history/
└── activations/
    ├── act-v1-0000000000000001.json
    ├── act-v1-0000000000000002.json
    └── act-v1-0000000000000003.json
```

There is no authoritative `history.json`, current-Environment pointer, or mutable sequence counter in v1.

Filename, declared `activation_id`, and encoded `sequence` MUST agree.

## Record shape

A Profile Activation:

```json
{
  "record_schema_version": 1,
  "activation_id": "act-v1-000000000000002a",
  "sequence": 42,
  "history_cursor": 42,
  "activated_at": "2026-09-23T08:31:15.123456Z",
  "environment_id": "env-v1-<64 lowercase hexadecimal characters>",
  "cause": {
    "kind": "profile"
  },
  "profile": {
    "id": "night",
    "schema_version": 1
  }
}
```

When present, `source`, `selection`, `asset`, and `color_resolution` use the exact corresponding value contracts from RFC 0005.

A History replay Activation:

```json
{
  "record_schema_version": 1,
  "activation_id": "act-v1-000000000000002b",
  "sequence": 43,
  "history_cursor": 37,
  "activated_at": "2026-09-23T08:32:02.000000Z",
  "environment_id": "env-v1-<64 lowercase hexadecimal characters>",
  "cause": {
    "kind": "history-replay",
    "activation_id": "act-v1-0000000000000025"
  }
}
```

`record_schema_version` MUST equal `1`. Unknown properties and `null` are forbidden.

Operations, Diagnostics, reload adapter, Plan dispositions, and execution internals MUST NOT be persisted in an Activation.

## Timestamp

`activated_at` is valid UTC RFC 3339 in exactly this shape:

```text
YYYY-MM-DDTHH:MM:SS.ffffffZ
```

It uses literal `Z`, exactly six fractional digits, and no numeric offset. Clock regressions are allowed.

Timestamp does not determine identity, ordering, current state, or backward navigation. Sequence is the sole ordering authority.

## Profile cause

With `cause.kind = "profile"`:

- `profile` is required;
- `history_cursor` equals `sequence`;
- `source`, `selection`, and `asset` are all present or all absent;
- those three fields are present if and only if the resolved Profile used `wallpaper.mode = "source"`;
- `color_resolution` is present if and only if the referenced Environment Manifest contains `colors`;
- provenance values match the Plan used by apply.

Cause contains no additional fields.

## History-replay cause

With `cause.kind = "history-replay"`:

- `cause.activation_id` names one valid Activation in the same validated local History;
- the target sequence is strictly less than the new sequence;
- the new `environment_id` equals the target's `environment_id`;
- the new `history_cursor` equals the target's `history_cursor`;
- `profile`, `source`, `selection`, `asset`, and `color_resolution` are absent.

The backwards-only target requirement prevents reference cycles. Cause preserves actual genealogy even when History Cursor preserves a different logical navigation position.

## History Cursor

Every Activation satisfies:

```text
1 <= history_cursor <= sequence
```

For a Profile Activation:

```text
history_cursor = sequence
```

For a History replay of target `X`:

```text
history_cursor = X.history_cursor
```

Persisting the cursor avoids recursive replay-chain traversal.

## `previous`

Given the current Activation:

```text
target_sequence = current.history_cursor - 1
```

If `history_cursor` is greater than `1`, `previous`:

1. loads and validates the target Activation by `target_sequence`;
2. validates the target Environment and required Durable Asset;
3. applies that immutable Environment without resolving its old Profile;
4. commits a new Activation whose cause is `history-replay` of the target;
5. copies the target's `history_cursor`.

Repeated Environment values are preserved as distinct events. `previous` navigates Activations, not distinct Environment IDs.

Empty History and `history_cursor = 1` are distinct no-predecessor failures and MAY use distinct registered Error codes.

## Current durable state

An empty History is valid and has no current Activation.

For non-empty valid History, the Activation with maximum sequence is the durable current Activation. Its Environment is the durable current Environment.

No second authoritative current-state file exists.

`current.ghostty` remains a derived Projection:

```text
latest Activation
→ Environment
→ Projection
→ current.ghostty
```

With empty History, `current.ghostty` MUST be absent after recovery.

## History validation

For empty History, no final Activation records exist.

For non-empty History with maximum sequence `N`, final records MUST represent every sequence exactly once:

```text
1, 2, 3, ..., N
```

For every Activation:

- filename, `activation_id`, and `sequence` agree;
- `record_schema_version` is supported;
- timestamp is structurally valid;
- referenced Environment exists and validates under RFC 0001;
- every Durable Asset required by that Environment exists and validates;
- `1 <= history_cursor <= sequence`;
- cause-specific invariants hold.

Malformed records, duplicate or missing sequences, identifier mismatch, unsupported versions, invalid references, and cause violations are durable-state corruption. Validation MUST NOT skip them.

Valid unreferenced Environments and Durable Assets are orphans, not History corruption.

## Single-writer protocol

Every durable apply acquires one exclusive writer lock in the managed directory.

The lock covers:

```text
validate complete durable History
determine current Activation
choose next sequence
revalidate Plan create/reuse observations
ensure Asset and Environment
materialize Projection
publish Activation durably
```

Reload occurs after releasing the writer lock.

A Plan created before lock acquisition is informational. Under the lock, `create` may have become `reuse`. Existing corruption aborts apply and is never converted into overwrite or repair.

The next sequence is `1` for empty History or `N + 1` otherwise. Exhausting the v1 sequence range is an apply failure.

## Activation commit point

Asset and Environment are ensured before Projection and Activation. Projection is materialized atomically before Activation publication.

Activation publication is:

```text
write a temporary record in history/activations/
fsync the temporary record
atomically publish to the final filename without replacement
fsync history/activations/
```

Temporary and final files MUST reside on the same filesystem. Publication MUST fail rather than replace an existing final record.

The final directory fsync is the durable Activation commit point. Before it, no new Activation exists. After it, the new Activation is the durable current state.

Best-effort reload occurs only after the commit point. Reload failure does not invalidate the Activation or turn durable apply success into exit status `6`.

## Crash behavior

A crash before Activation commit may leave:

- a valid unreferenced Durable Asset;
- a valid unreferenced Environment;
- a stale or advanced `current.ghostty`;
- unpublished temporary files.

These conditions are not History corruption.

On recovery, Projection is reconciled from the latest durable Activation. If History is empty, stale `current.ghostty` may be removed. Derived-state reconciliation is not durable-state repair.

Unpublished temporary records are not Activations and do not participate in contiguity.

## Lifetime

Every Environment referenced by History is live durable data. Every Durable Asset required by such an Environment is also live durable data.

V1 performs no automatic History deletion, retention, compaction, pruning, Environment garbage collection, or Asset garbage collection. Cache remains disposable.
