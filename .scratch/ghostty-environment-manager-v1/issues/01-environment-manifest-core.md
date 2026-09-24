# 01: Establish Environment Manifest compatibility core

**What to build:** Provide validated Environment domain types, strict JSON codec, RFC 8785 canonicalization, and stable Environment identity so later slices can persist and compare managed visual state safely.

**Blocked by:** None (can start immediately).

**Status:** completed

- [x] RFC 0001 test vector produces the exact canonical JSON and Environment ID.
- [x] Wallpaper unmanaged, managed-none, and managed-image remain distinct.
- [x] Unknown fields, nulls, invalid colors, invalid palette length, empty terminal, and numeric range violations are rejected.
- [x] Formatting, Clippy with warnings denied, Rust tests, and Bash regression tests pass.
