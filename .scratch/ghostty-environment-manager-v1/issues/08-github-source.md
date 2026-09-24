# 08: Resolve GitHub wallpaper Sources

**What to build:** Let users plan and apply Profiles backed by GitHub repositories while pinning every invocation to one commit and enumerating complete Candidate membership.

**Blocked by:** 06 — Apply and replay local Environments.

**Status:** done

- [x] Requested ref resolves once to one commit used for enumeration and acquisition.
- [x] Regular blobs are included recursively; symlinks and submodules are excluded.
- [x] Truncated tree responses trigger complete subtree traversal or explicit failure.
- [x] GitHub rate-limit/auth failures produce safe structured errors without credential leakage.
- [x] Live integration remains separate from deterministic adapter tests.
