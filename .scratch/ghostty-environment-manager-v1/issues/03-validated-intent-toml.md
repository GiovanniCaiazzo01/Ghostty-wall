# 03: Load validated Source and Profile Intent

**What to build:** Let users author the complete v1 Source registry and Profile recipes in strict TOML while preserving decimal precision and unmanaged-field semantics.

**Blocked by:** 01 — Establish Environment Manifest compatibility core.

**Status:** done

Review regression fixes: lexical TOML numeric representations now come from `toml_edit` (quoted keys and inline tables preserved); `parse_named_profile_toml` validates Profile slug and referenced Source.

- [x] Config and Profile TOML accept only RFC 0003 fields and tagged variants.
- [x] Source and Profile identifiers use validated slug types.
- [x] Decimal font size and opacity convert lexically to exact fixed-point values.
- [x] Excess precision, unknown fields, null-equivalent invalid forms, and cross-section invariant violations fail clearly.
- [x] A minimal Profile resolves to empty managed intent.
