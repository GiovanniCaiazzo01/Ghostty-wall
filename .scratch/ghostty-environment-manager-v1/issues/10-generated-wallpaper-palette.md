# 10: Generate managed colors from wallpaper

**What to build:** Let a Profile generate a readable managed color palette from its resolved wallpaper and record algorithm provenance without changing Environment identity semantics.

**Blocked by:** 05 — Plan a local explicit-color Profile.

**Status:** ready-for-agent

- [ ] One versioned deterministic palette algorithm is specified and implemented.
- [ ] Generated output always satisfies the complete RFC 0001 color model.
- [ ] Identical output colors deduplicate regardless of algorithm provenance.
- [ ] Image limits and malformed inputs fail safely.
- [ ] Curated fixtures verify determinism, contrast invariants, and useful output without brittle giant snapshots.
