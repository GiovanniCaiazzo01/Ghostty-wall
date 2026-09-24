# 05: Plan local explicit-color Profile

**What to build:** Resolve a local-directory Profile with path or seeded-random wallpaper and explicit colors into complete public Plan JSON without persistent mutation.

**Blocked by:** 02 — Candidate Selection; 03 — validated Intent; 04 — Managed Root initialization.

**Status:** done

- [x] Linux recursive Candidate enumeration stays anchored to the SourceRoot directory descriptor, skips non-UTF-8 and symlink entries, checks Candidate Set drift, and safely reopens the selected file with `openat2`.
- [x] Selected bytes are bounded, decoded as PNG/JPEG, and hashed into the resolved Asset.
- [x] Plan embeds the exact Environment Manifest and Environment ID.
- [x] Full RFC 0005 Operations, Diagnostics, and Error Responses, including RFC 0007 Recovery Inspection on non-empty History and platform reload observations.
- [x] Planning performs no filesystem mutation, cache write, Projection write, or reload.

## Comments

- Added read-only Source diagnostics, registered `source.empty-candidate-set` Error Response, path/seed validation, and content-verified Asset/Environment reuse (including duplicate noncanonical Asset detection).
- Complete Plan API now performs synchronized Recovery Inspection, reports canonically ordered Projection Diagnostics, validates the effective Integration Hook, and consumes explicit reload capability observations. Reload execution remains issue 11 scope.
