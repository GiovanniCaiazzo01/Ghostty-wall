# 02: Select wallpaper Candidates deterministically

**What to build:** Turn unordered Candidate paths into a canonical Candidate Set with stable fingerprinting and select one Candidate through reproducible `random-v1`.

**Blocked by:** 01 — Establish Environment Manifest compatibility core.

**Status:** done

- [x] Candidate paths enforce RFC 0004 path and UTF-8 invariants.
- [x] Candidate Set construction deduplicates and sorts by raw UTF-8 bytes.
- [x] Empty and non-empty Candidate Set test vectors match RFC 0004.
- [x] `random-v1` ordinary and rejection-sampling vectors match RFC 0002.
- [x] Empty Candidate Set returns its specific selection error.
