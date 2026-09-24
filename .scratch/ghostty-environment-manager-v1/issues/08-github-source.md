# 08: Resolve GitHub wallpaper Sources

**What to build:** Let users plan and apply Profiles backed by GitHub repositories while pinning every invocation to one commit and enumerating complete Candidate membership.

**Blocked by:** 06 — Apply and replay local Environments.

**Status:** ready-for-agent

- [ ] Requested ref resolves once to one commit used for enumeration and acquisition.
- [ ] Regular blobs are included recursively; symlinks and submodules are excluded.
- [ ] Truncated tree responses trigger complete subtree traversal or explicit failure.
- [ ] GitHub rate-limit/auth failures produce safe structured errors without credential leakage.
- [ ] Live integration remains separate from deterministic adapter tests.
