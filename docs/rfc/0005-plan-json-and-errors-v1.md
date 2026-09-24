# RFC 0005: Plan JSON and Error Response v1

Status: Accepted
Date: 2026-09-23

## Purpose

This RFC defines the public JSON contract for `plan --json`, including a complete successful Plan, non-fatal Diagnostics, and fatal Error Responses.

A Plan is a fully resolved informational snapshot of one invocation against the state observed during planning. It is not persistent identity, an executable artifact, or reusable optimistic-locking state.

## JSON transport

In JSON mode:

```text
exit 0
→ stdout contains exactly one complete Plan JSON value

exit != 0
→ stdout contains exactly one complete ErrorResponse JSON value
```

Standard output MUST NOT contain banners, progress, warnings, logs, or additional JSON values. Standard error remains empty unless JSON serialization itself is impossible or the process terminates abnormally. Tracing MUST be silenced or routed away from both protocol output and normal standard error in JSON mode.

A planning failure MUST NOT emit a partial Plan.

## Planning purity

Planning may read, fetch, decode, hash, validate, and probe. It may:

- read authoritative Intent and durable state;
- enumerate local Sources;
- query GitHub;
- acquire a selected image into bounded memory;
- validate and decode that image;
- resolve or generate colors;
- calculate asset, Candidate Set, and Environment identities;
- inspect Ghostty configuration and reload capabilities.

Planning MUST NOT write cache or temporary files, persist records, mutate Intent or durable state, materialize Projection, append history, or reload Ghostty.

Fetch, decode, validation, or hashing failure terminates planning and produces an Error Response.

## Plan shape

A v1 Plan contains exactly:

```text
schema_version
profile
source?
selection?
asset?
color_resolution?
environment
operations
diagnostics
```

It contains no Plan identifier, timestamp, hostname, process identifier, transport credential, or ephemeral path.

`schema_version` MUST equal `1`.

## Profile

```json
{
  "id": "night",
  "schema_version": 1
}
```

`id` follows the Profile slug contract in RFC 0003. The Plan does not copy Profile Intent.

## Wallpaper provenance

`source`, `selection`, and `asset` are present together if and only if the resolved Profile has `wallpaper.mode = "source"`. Otherwise all three are absent. `null` is forbidden.

### GitHub Source

Configured ref:

```json
{
  "id": "anime",
  "kind": "github",
  "repository": "ThePrimeagen/anime",
  "ref": {
    "kind": "configured",
    "value": "master"
  },
  "resolved_commit": "0123456789abcdef0123456789abcdef01234567",
  "path": "wallpapers"
}
```

Default branch:

```json
{
  "id": "anime",
  "kind": "github",
  "repository": "ThePrimeagen/anime",
  "ref": {
    "kind": "default-branch",
    "value": "main"
  },
  "resolved_commit": "0123456789abcdef0123456789abcdef01234567"
}
```

`path` is absent for repository root. `resolved_commit` MUST contain exactly 40 lowercase hexadecimal characters.

### Local-directory Source

```json
{
  "id": "local",
  "kind": "local-directory",
  "configured_path": "~/Pictures/wallpapers",
  "resolved_root": "/home/gio/Pictures/wallpapers"
}
```

`resolved_root` is the absolute canonical SourceRoot for this machine. Local Sources have no invented revision.

## Selection

### Random

```json
{
  "kind": "random",
  "algorithm": "random-v1",
  "seed": "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f",
  "candidate_set_fingerprint": "cset-v1-e91de812a09b8d2daa3b7162a024f97cdbb8d182dde293ca357beff816e55548",
  "candidate_count": 42,
  "selected_index": 7,
  "candidate": "tokyo/night.jpg"
}
```

Rules:

- `algorithm` MUST equal `random-v1`;
- `seed` MUST contain 64 lowercase hexadecimal characters;
- Candidate Set Fingerprint follows RFC 0004;
- `candidate_count` is `1..=9_007_199_254_740_991`;
- `selected_index` is `0..candidate_count`;
- `selected_index` MUST be strictly less than `candidate_count`;
- `candidate` MUST equal the Candidate identity at `selected_index` in the canonical Candidate Set.

### Direct path

```json
{
  "kind": "path",
  "candidate": "tokyo/night.jpg"
}
```

Fingerprint, count, index, seed, and algorithm are forbidden for direct path Selection.

## Asset

```json
{
  "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "media_type": "image/jpeg",
  "byte_length": 481923
}
```

Rules:

- `sha256` contains 64 lowercase hexadecimal characters;
- `media_type` is `image/png` or `image/jpeg`;
- `byte_length` is `0..=9_007_199_254_740_991`;
- filename, local path, URL, cache path, and transport metadata are forbidden.

## Color Resolution

`color_resolution` is present if and only if `environment.manifest.colors` is present.

### Generated

```json
{
  "kind": "generated",
  "algorithm": "kmeans-v1"
}
```

Generated Color Resolution requires `source`, `selection`, and `asset`, and requires `environment.manifest.wallpaper.mode = "image"`. The asset digest is not duplicated.

#### `kmeans-v1`

`kmeans-v1` is the only v1 generated-color algorithm. It operates on selected Asset bytes and produces the complete RFC 0001 colors object, including cursor and selection colors.

Input is decoded PNG or JPEG. Decoded width and height MUST each be at most 16,384, decoded pixel count MUST be `1..=16_777_216`, decoder allocation MUST be bounded at 256 MiB, and decoded channels MUST be 8-bit grayscale, grayscale-alpha, RGB, or RGBA. Grayscale channels are replicated into RGB; alpha, color profiles, and orientation metadata do not affect colors. A limit violation or decode failure is `asset.unsupported-image`.

Pixels are RGB triples in row-major order. Let `N` be pixel count and `S = min(N, 65_536)`. Samples are pixels at zero-based indices `floor(i * N / S)` for every `i` in `0..S`. Clustering uses eight RGB centers and squared Euclidean distance:

1. Center 0 is the per-channel sample mean, rounded to nearest integer with half values rounded up.
2. Each remaining center is the sampled RGB value whose distance to its nearest existing center is greatest. Ties choose lexicographically greatest RGB.
3. Perform exactly 16 Lloyd assignment/update rounds. Assignment ties choose the lowest center index. Updated per-channel means use the same rounding as center 0. An empty center retains its prior value.

Background is the center minimizing `2126*r + 7152*g + 722*b`; ties choose lexicographically smallest RGB. Foreground is whichever of `000000` and `ffffff` has greater WCAG 2 relative luminance contrast against background; a tie chooses `000000`. Cursor equals foreground. Selection background is the per-channel mix `(2*background + foreground) / 3`, rounded to nearest integer with half values rounded up. Selection foreground is chosen from black and white by the same contrast rule. These choices guarantee at least 4.5:1 contrast for foreground, cursor, and selection foreground against their respective backgrounds.

ANSI palette entries 0, 7, 8, and 15 are background, foreground, selection background, and foreground. Entries 1–6 use these RGB targets in order:

```text
cd3131 0dbc79 e5e510 2472c8 bc3fbc 11a8cd
```

Entries 9–14 use:

```text
f14c4c 23d18b f5f543 3b8eea d65cd6 29b8db
```

For each target, choose the nearest final center by squared Euclidean distance, ties by lowest center index, then output the equal per-channel mix of center and target rounded to nearest integer with half values rounded up.

Only generated colors enter the Environment Manifest and Environment identity. Algorithm name remains Plan and Activation provenance, so another resolution producing identical managed colors deduplicates to the same Environment.

### Explicit

```json
{
  "kind": "explicit"
}
```

### Named theme

```json
{
  "kind": "theme",
  "theme": "TokyoNight",
  "content_sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
}
```

Theme Resolution Content is exactly the resolved managed colors object that becomes `environment.manifest.colors`. It excludes theme name, raw file bytes, comments, whitespace, unmanaged Ghostty properties, source path, and Ghostty version.

Its digest is:

```text
canonical = JCS(ThemeResolutionContent)

theme_content_digest = SHA-256(
    ASCII("ghostty-wall.theme-resolution.v1")
    || 0x00
    || canonical
)

content_sha256 = lowercase_hex(theme_content_digest)
```

Two differently named themes that resolve to the same managed colors have the same `content_sha256` and may produce the same Environment while retaining distinct theme-name provenance.

## Environment

```json
{
  "environment_id": "env-v1-<64 lowercase hexadecimal characters>",
  "manifest": {
    "schema_version": 1
  }
}
```

The Environment Manifest and ID follow RFC 0001. The persistent Environment record's `record_schema_version` is not part of Plan.

## Operations

Operations are semantic, informative, and normatively ordered:

```text
1. ensure_asset          present only when asset is present
2. ensure_environment    always present
3. activate_environment  always present
4. reload_ghostty        always present
```

Example:

```json
[
  {
    "kind": "ensure_asset",
    "asset_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    "disposition": "create"
  },
  {
    "kind": "ensure_environment",
    "environment_id": "env-v1-...",
    "disposition": "create"
  },
  {
    "kind": "activate_environment",
    "environment_id": "env-v1-...",
    "disposition": "apply"
  },
  {
    "kind": "reload_ghostty",
    "required": false,
    "adapter": "systemd"
  }
]
```

`ensure_asset` and `ensure_environment` dispositions are `create` or `reuse`. Existing valid content is reused. Existing corrupt or inconsistent content is a planning error; `replace`, overwrite, repair, and migration are not dispositions.

`activate_environment.disposition` MUST equal `apply`. Reapplying the current Environment still represents a future Activation.

`reload_ghostty.required` MUST be `false`. Adapter is `systemd`, `applescript`, or `unavailable`. With `unavailable`, `reason` is required and is one of:

```text
unsupported-platform
adapter-command-unavailable
ghostty-integration-unavailable
```

With another adapter, `reason` is forbidden. Adapter availability does not assert that Ghostty is running.

Operations describe state observed during this invocation. They are not reusable preconditions. The executor remains responsible for race-safe idempotent behavior.

Infrastructure operations such as directory creation, locking, temporary writes, fsync, rename, history append, and symlink update MUST NOT appear.

## Diagnostics

`diagnostics` is always present and is an array. Diagnostics are non-fatal observations from this invocation. They do not alter Environment, Selection, Operations, or any identity.

Severity is the closed enum:

```text
info
warning
```

`error` and `fatal` are forbidden.

Diagnostics contain no free-form message, context, details, data, or metadata field. Each registered code defines its exact fields.

### Registered v1 diagnostics

Skipped non-UTF-8 Source entries:

```json
{
  "code": "source.skipped-non-utf8-entries",
  "severity": "warning",
  "source_id": "local",
  "count": 3
}
```

Rules:

- aggregation key is `(code, source_id)`;
- observations with that key are summed;
- `count` is `1..=9_007_199_254_740_991`;
- zero count omits the Diagnostic;
- aggregation overflow is a planning error.

Projection drift Diagnostics contain exactly `code` and `severity`:

```json
{
  "code": "projection.missing",
  "severity": "warning"
}
```

The registered codes are:

```text
projection.missing
  History is non-empty and current.ghostty is absent.

projection.unexpected
  History is empty and current.ghostty is present.

projection.out-of-sync
  History is non-empty and current.ghostty is present but is not an
  equivalent regular-file Projection.
```

Each has severity exactly `warning`. They contain no other fields, do not aggregate counts, and appear at most once. Projection Diagnostics do not alter Operations.

### Ordering

Diagnostic production is:

1. aggregate equivalent observations;
2. validate registered-code invariants;
3. reject exact duplicates;
4. JCS-canonicalize each Diagnostic independently;
5. sort ascending by canonical UTF-8 bytes.

Traversal, API, task, and thread order MUST NOT affect the array.

### Compatibility

A producer emits only codes registered by its version.

A consumer validates a known code strictly and rejects unknown fields or invalid invariants for that code. For an unknown code, it verifies that the entry is an object, reads `code`, MAY validate the v1 severity, and ignores the remainder of that Diagnostic.

Unknown Diagnostics are ignorable observations. Unknown Errors are not ignorable failures.

Human CLI rendering is derived from code and typed fields. Rendered wording and localization are not JSON API.

## Error Response

An Error Response contains exactly:

```json
{
  "schema_version": 1,
  "error": {
    "category": "resolution",
    "code": "source.empty-candidate-set",
    "source_id": "anime"
  }
}
```

`schema_version` versions the Error Response protocol and MUST equal `1`.

The `error` object is a strict tagged union by `code`. It contains no human message, generic context, details, recursive cause, debug data, stack trace, or raw external payload.

An Error Response contains exactly one primary error.

## Error categories and exit status

| Exit | Category | Meaning |
| ---: | --- | --- |
| `0` | none | success |
| `2` | `usage` | invalid invocation or required invocation input |
| `3` | `intent` | invalid authoritative user Intent |
| `4` | `resolution` | Source, Selection, acquisition, theme, or integration resolution failure |
| `5` | `corruption` | invalid Ghostty Wall-owned durable state |
| `6` | `apply` | failure preventing requested durable mutation |
| `70` | `internal` | violated internal invariant or unexpected software failure |

Reload failure after successful durable activation is best-effort runtime outcome, not category `apply`, and does not change success to exit `6`.

## Fatal phase ordering

The first error in the first failing phase wins:

```text
1. parse and validate usage
2. load and validate authoritative Intent
3. validate durable state consulted by the invocation
4. resolve external and local inputs
5. perform durable apply, when requested
6. report internal invariant failure
```

Later phases MUST NOT begin after an earlier phase fails. Parallelism within a phase MUST preserve a deterministic primary-error ordering defined by that phase.

Malformed `config.toml` or Profile TOML is category `intent`. Invalid Environment, Asset, or future history data owned by Ghostty Wall is category `corruption`.

## Error compatibility

Every registered code defines:

- exactly one category;
- exact required and optional fields;
- field invariants;
- security and redaction behavior.

For a known code, consumers validate fields strictly and reject unknown fields.

For an unknown code in a known category, consumers preserve failure, retain category and code, and ignore the remaining code-specific payload. An unknown category is incompatible with Error Response schema v1.

The error registry is additive within schema v1 when a new code uses an existing category and does not change existing code contracts.

The v1 planning registry includes:

```text
usage.resolution-seed-required
usage.resolution-seed-forbidden
  category: usage
  fields: none

intent.unknown-source
  category: intent
  fields: source_id

integration.not-initialized
  category: intent
  fields: path

resolution.unsupported-input
source.changed-during-planning
asset.unsupported-image
integration.unsupported-platform
  category: resolution
  fields: none

source.empty-candidate-set
  category: resolution
  fields: source_id

resolution.filesystem-unavailable
integration.inspection-failed
integration.hook-drift
  category: resolution
  fields: path

durable-state.corrupt
durable-state.unreadable
  category: corruption
  fields: path

internal.invariant
  category: internal
  fields: none
```

`path` is the normalized user-actionable local path whose inspection failed or whose owned state is invalid. These errors never include file contents or a raw operating-system error.

Additional codes MUST be registered by the RFC governing the behavior that emits them.

## Security

Error fields describe identifiers and normalized user-actionable locations only.

Error Responses MUST NOT contain credentials, authorization headers, signed URLs, raw HTTP bodies, raw file contents, stack traces, or unredacted external payloads. Absolute local paths appear only when required for remediation; Source-relative Candidate identity is preferred.

## Out of scope

This RFC does not define:

- persisted or executable Plans;
- `apply --plan`;
- Activation storage;
- transaction implementation details;
- machine-readable successful `apply` outcome;
- representation of best-effort reload failure after successful activation.
