# 07: Diagnose and maintain the installation lifecycle

**What to build:** Let users inspect installation health, repair only safe structure and hook drift, migrate recognized v0 configuration idempotently, and uninstall without losing durable History.

**Blocked by:** 06 — Apply and replay local Environments.

**Status:** done

- [x] Doctor reports verified, failed, and unavailable checks without mutation.
- [x] Repair never synthesizes missing authoritative or durable state where loss may have occurred.
- [x] Legacy migration preflights every entry, updates each file atomically, resumes idempotently, and preserves legacy files.
- [x] Uninstall removes owned integration and disposable state while preserving Intent, Assets, Environments, and History.
- [x] Unsafe ownership, symlinks, permissions, or filesystem capabilities fail closed.
