# 10: Generate managed colors from wallpaper

**What to build:** Let a Profile generate a readable managed color palette from its resolved wallpaper and record algorithm provenance without changing Environment identity semantics.

**Blocked by:** 05 — Plan a local explicit-color Profile.

**Status:** done

- [x] One versioned deterministic palette algorithm is specified and implemented.
- [x] Generated output always satisfies the complete RFC 0001 color model.
- [x] Identical output colors deduplicate regardless of algorithm provenance.
- [x] Image limits and malformed inputs fail safely.
- [x] Curated fixtures verify determinism, contrast invariants, and useful output without brittle giant snapshots.

## Comments

- Specified `kmeans-v1` in RFC 0005 with bounded decoding, deterministic sampling and clustering, ANSI derivation, and WCAG contrast rules.
- Planning now materializes complete generated colors and records algorithm provenance outside Environment identity; fixture tests cover determinism, monochrome usefulness, provenance-independent deduplication, malformed input, and dimension limits.
