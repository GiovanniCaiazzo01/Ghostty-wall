# 09: Materialize named Ghostty themes

**What to build:** Let a Profile resolve a named Ghostty theme into the managed color model so the resulting Environment replays without depending on the theme remaining installed.

**Blocked by:** 05 — Plan a local explicit-color Profile.

**Status:** ready-for-agent

- [ ] Built-in and supported local named themes resolve through one adapter.
- [ ] Only managed colors enter the Environment Manifest.
- [ ] Theme Resolution Content digest and provenance follow RFC 0005.
- [ ] Equal managed colors deduplicate Environment identity while preserving distinct theme names in Activation provenance.
- [ ] Missing or malformed themes fail planning without partial Plan.
