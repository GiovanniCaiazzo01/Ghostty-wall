# RFC 0007: Recovery and Reconciliation v1

Status: Accepted
Date: 2026-09-23

## Purpose

This RFC defines synchronized read-only Recovery Inspection and mutating Reconciliation of derived Projection state.

The central rule is:

> Recovery restores reconstructible derived state to the state implied by the latest committed Activation. It never synthesizes, replaces, or repairs committed durable History.

Derived Projection drift is repairable when reconciliation is safe. Durable committed-state corruption is fatal.

## Operations

Recovery has two conceptual operations:

```text
inspect_recovery_state()
reconcile_recovery_state()
```

Recovery Inspection is read-only. It is used by `plan`, `doctor`, and the planning phase of `apply`.

Reconciliation mutates only derived state. It runs under the exclusive state lock for `apply` and `previous`. A future explicit `recover` command may invoke it.

`plan` and `doctor` MUST NOT reconcile.

## Authority

Recovery authority is:

```text
History
→ latest Activation
→ Environment
→ required Durable Asset
→ expected Projection
```

History determines the durable current Environment. `current.ghostty`, current Profile contents, cache, and Source state are not authorities.

## State lock

`init` creates an empty managed lock target:

```text
ghostty-wall/state.lock
```

The file has no domain semantics and is not authoritative state.

Read-only inspection acquires a shared state lock. Reconciliation and durable mutation acquire the exclusive state lock. An implementation MAY use the exclusive lock for inspection when reliable shared locking is unavailable.

A reader MUST NOT inspect History concurrently with Activation publication. The writer retains the exclusive lock through the Activation directory fsync defined by RFC 0006.

Opening and locking `state.lock` MUST NOT create, truncate, rewrite, or otherwise mutate it. Missing managed layout or lock target means integration was not initialized; planning stops before durable inspection with a registered category `intent` error.

## Recovery Inspection

While holding the state lock, inspection:

1. validates complete History under RFC 0006;
2. determines the latest Activation, if any;
3. validates its Environment under RFC 0001;
4. validates every Durable Asset required by that Environment;
5. derives the expected Projection;
6. inspects `current.ghostty` without following symbolic links;
7. compares actual and expected managed semantics;
8. inspects the effective Ghostty root include.

Inspection does not modify stale temporary files, Projection, Intent, History, Environments, Assets, or the integration hook.

Any failure required to validate committed durable state is category `corruption` and terminates inspection.

## Projection inspection

Projection state is classified as:

```text
consistent
missing
unexpected
out-of-sync
```

The public Plan Diagnostics are defined in RFC 0005:

- `projection.missing`;
- `projection.unexpected`;
- `projection.out-of-sync`.

They are warnings and do not modify Plan Operations.

Inspection of the Projection path MUST be race-safe. It MUST open or inspect the directory entry without following symbolic links and verify that the opened object is still the expected regular file. A separate `lstat` followed by an unprotected path-based open is insufficient.

## Empty History

With empty History, expected managed Projection is absent.

| `current.ghostty` | Inspection |
| --- | --- |
| absent | consistent |
| present in any form | `projection.unexpected` |

Orphan Environments and Durable Assets are not inspected as current state.

## Non-empty History

For latest Activation `A` referencing Environment `E`, inspection:

1. validates `A`;
2. validates `E`;
3. validates Assets required by `E`;
4. projects `E` in memory;
5. compares the expected managed semantics with `current.ghostty`.

| `current.ghostty` | Inspection |
| --- | --- |
| absent | `projection.missing` |
| equivalent regular file | consistent |
| stale or malformed regular file | `projection.out-of-sync` |
| symlink, directory, or special entry | `projection.out-of-sync` |

## Semantic equivalence

Projection comparison is semantic, not byte-for-byte.

Inspection parses only the Ghostty configuration grammar emitted by the v1 projector, normalizes supported values, and compares the resulting managed model with the expected Environment Projection.

Comments, whitespace, and property ordering do not affect equivalence. Unknown properties, unsupported syntax, duplicate values that cannot be normalized unambiguously, malformed values, and extra managed state produce `projection.out-of-sync`.

Ghostty Wall does not preserve edits to `current.ghostty`; the file is wholly managed derived state.

## Durable corruption

Reconciliation MUST stop without durable mutation when any committed dependency is missing or invalid, including:

- malformed, missing, duplicate, or non-contiguous Activation records;
- Activation identifier, sequence, cause, or cursor mismatch;
- unsupported durable record version;
- missing or invalid referenced Environment;
- Environment ID or Manifest digest mismatch;
- missing required Durable Asset;
- Asset digest mismatch.

Recovery MUST NOT rerun a Profile, consult a Source, redownload content, regenerate colors, substitute another Environment, skip a corrupt Activation, or rewrite committed History.

## Orphans and temporary files

Valid unreferenced Environments and Durable Assets are legal orphans. Recovery neither deletes nor adopts them.

Files using the atomic writer's reserved temporary naming are unpublished implementation debris, not durable records, History gaps, or durable orphans. Validation ignores them.

Under the exclusive lock, Reconciliation MAY remove stale reserved temporary files. Their cleanup is not required for semantic recovery.

## Ghostty integration drift

The managed Projection and root configuration include are separate:

```text
managed Projection:
  ghostty-wall/current.ghostty

integration hook:
  config-file = ?ghostty-wall/current.ghostty
```

Inspection verifies that the include is installed in the effective highest-precedence Ghostty root configuration.

Missing include, moved effective root, or a newly higher-precedence root is integration drift. Recovery MUST NOT edit user-owned root configuration. Remediation is the explicit `init --repair` lifecycle.

`plan`, `apply`, and `previous` require an effective managed include. Integration drift produces a category `resolution` Error Response and no Plan or Activation. `doctor` reports the drift without mutation.

Reload unavailability remains non-fatal and distinct from integration drift.

## Reconciliation

Reconciliation requires the exclusive state lock and a fully valid durable state.

It MUST NOT:

- create an Activation;
- change History;
- create, replace, or delete an Environment or Durable Asset;
- edit the Ghostty root include;
- consult Profile or Source state;
- reload Ghostty.

Reconciliation is idempotent.

## Atomic Projection replacement

When a regular-file Projection is required:

```text
render complete current.ghostty
write a temporary file in the managed directory
fsync the temporary file
atomically replace current.ghostty
fsync the managed directory
```

Temporary and destination entries MUST reside on the same filesystem.

When no Projection is expected:

```text
remove the current.ghostty directory entry without following it
fsync the managed directory
```

An absent destination is a no-op.

## Non-regular Projection entries

Reconciliation treats a symbolic link as the directory entry itself and never follows its target.

A special non-directory entry MAY be unlinked without following it when the operation is safe. Otherwise reconciliation fails.

A directory at `current.ghostty` MUST NEVER be deleted recursively. An implementation MAY remove an empty directory using no-follow, directory-relative semantics. A non-empty directory or any directory state that cannot be removed safely causes reconciliation failure.

After safely removing an unexpected entry, Reconciliation may create the expected regular-file Projection. Failure to reconcile derived state is an apply failure, not durable corruption.

Directory-entry inspection, removal, and replacement MUST be race-safe against type changes and symbolic-link substitution.

## Reconciliation matrix

| Durable History | `current.ghostty` | Inspection | Reconciliation |
| --- | --- | --- | --- |
| empty | absent | consistent | no-op |
| empty | regular file | `projection.unexpected` | remove safely |
| non-empty | absent | `projection.missing` | regenerate |
| non-empty | equivalent regular file | consistent | no-op |
| non-empty | stale or malformed regular file | `projection.out-of-sync` | atomic replace |
| empty | symlink, special entry, or directory | `projection.unexpected` | remove safely or fail |
| non-empty | symlink, special entry, or directory | `projection.out-of-sync` | replace safely or fail |

## Apply protocol

The read phase is:

```text
acquire shared state lock
inspect durable and integration state
release lock
resolve Profile and Source
fetch, decode, and validate
build complete Plan
```

The write phase is:

```text
acquire exclusive state lock
revalidate complete durable History
revalidate effective Ghostty integration hook
reconcile Projection to latest committed Activation
revalidate Plan create/reuse observations
ensure Durable Asset
ensure Environment
materialize target Projection
commit Activation under RFC 0006
release lock
attempt best-effort reload
```

The exclusive lock is not held during network access, source enumeration, image decode, or color generation.

The root config is externally owned and cannot be serialized by the state lock. Apply guarantees only that it does not ignore integration drift observable during locked revalidation.

## `previous` protocol

`previous` runs under the exclusive state lock:

```text
validate complete durable state
revalidate effective Ghostty integration hook
reconcile Projection to latest committed Activation
derive target from current History Cursor
validate target Activation, Environment, and Asset
materialize target Projection
commit History-replay Activation
release lock
attempt best-effort reload
```

`previous` performs no network access, Source resolution, Profile resolution, image generation, or palette generation.

## Recovery and History

Reconciliation of derived state MUST NOT create an Activation. Restoring Projection to the latest committed Environment is not a new user choice.

History remains byte-for-record unchanged by recovery. Cause `recovery` does not exist in Activation schema v1.
