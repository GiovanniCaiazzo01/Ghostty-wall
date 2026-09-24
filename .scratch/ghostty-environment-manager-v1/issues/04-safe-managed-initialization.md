# 04: Initialize the Managed Root safely

**What to build:** Preview and create the v1 Managed Root with one semantic Ghostty Integration Hook, without following managed symlinks or weakening durability.

**Blocked by:** 01 — Environment Manifest compatibility core.

**Status:** done

- [x] Dry run reports candidates, selected config, layout, hook, legacy indicators, unsafe state, and five unprobed capabilities without writing.
- [x] Linux init creates eager layout with restrictive creation modes, default Intent, publication marker, capability probes, and final hook.
- [x] Repeated init is a no-op preserving file and directory mtimes.
- [x] Linux root config precedence, semantic hook equivalence, stale/duplicate detection, and explicit repair follow RFC 0008 in tested cases.
- [x] Safety failure does not install a new hook; rollback touches only invocation-owned objects, checking inode and file content.
- [x] Linux fault injection covers failed capability probe, Managed Root publication fsync, post-rename Hook fsync, and concurrent root-config edits. Real macOS verification tracked separately in 14.

## Comments

- Added conservative `init_repair` for valid published layout, permissions, missing cache/lock, hooks, and pristine interrupted init. Existing Intent and durable data preserved; missing authoritative/durable structure without proven pristine marker fails closed.
- Linux tests cover symlink ancestors, root-config symlinks, hook precedence/repair, published locking, dry run, interrupted init, rollback, permissions, and idempotence. macOS paths implemented but no target or real-system verification available here; see human issue 14. Pre-commit root-config identity and content are revalidated; edits by an uncooperative concurrent writer remain inherently racy without shared coordination.
