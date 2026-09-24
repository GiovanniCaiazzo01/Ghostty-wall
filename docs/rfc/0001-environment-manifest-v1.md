# RFC 0001: Environment Manifest v1

Status: Accepted
Date: 2026-09-22

## Purpose

This RFC defines the portable, canonical representation and identity of a Ghostty Wall Environment.

An Environment is an immutable snapshot of only the managed Projection produced by Ghostty Wall. It does not represent Ghostty's complete effective configuration. Replaying an Environment guarantees semantic equivalence of the managed Projection, not byte stability of `current.ghostty` across Ghostty Wall versions.

The key words **MUST**, **MUST NOT**, **SHOULD**, and **MAY** are normative.

## Data boundaries

- A Profile is user-authored intent and does not participate in Environment identity.
- An Environment Manifest is the semantic managed result and fully determines Environment identity.
- An Activation records that an Environment became current. Cause, provenance, and timestamps belong to the Activation, not the Environment.
- A Projection is derived, machine-local output such as `current.ghostty`. Paths in a Projection do not participate in Environment identity.
- A Durable Asset is addressed by the SHA-256 digest of its content. Discovery cache and generated intermediates do not participate in Environment identity.

## Environment record

Each Environment MUST be stored at:

```text
environments/<environment-id>.json
```

The file MUST contain this envelope:

```json
{
  "record_schema_version": 1,
  "environment_id": "env-v1-<64 lowercase hexadecimal characters>",
  "manifest": {
    "schema_version": 1
  }
}
```

`record_schema_version` versions the storage envelope. `manifest.schema_version` versions the semantic Manifest. Only the canonicalized `manifest` object participates in the Environment digest.

The envelope is immutable after creation. It MUST NOT contain Profile names, provenance, timestamps, local paths, or other Activation data.

## Manifest schema

The complete v1 shape is:

```json
{
  "schema_version": 1,
  "wallpaper": {
    "mode": "image",
    "asset_sha256": "<64 lowercase hexadecimal characters>",
    "media_type": "image/png | image/jpeg",
    "fit": "contain | cover | stretch | none",
    "position": "top-left | top-center | top-right | center-left | center | center-right | bottom-left | bottom-center | bottom-right",
    "opacity_millionths": 0,
    "repeat": false
  },
  "colors": {
    "background": "rrggbb",
    "foreground": "rrggbb",
    "palette": [
      "rrggbb", "rrggbb", "rrggbb", "rrggbb",
      "rrggbb", "rrggbb", "rrggbb", "rrggbb",
      "rrggbb", "rrggbb", "rrggbb", "rrggbb",
      "rrggbb", "rrggbb", "rrggbb", "rrggbb"
    ],
    "cursor": "rrggbb",
    "selection_background": "rrggbb",
    "selection_foreground": "rrggbb"
  },
  "terminal": {
    "font_size_millipoints": 13500,
    "background_opacity_millionths": 920000,
    "background_blur_intensity": 20,
    "cursor_style": "block | bar | underline | block_hollow"
  }
}
```

`schema_version` is required. `wallpaper`, `colors`, and `terminal` are independently optional. A Manifest containing only `schema_version` is valid and represents an empty managed Projection.

Within an optional section:

- `wallpaper.mode` is required when `wallpaper` is present and MUST be either `none` or `image`.
- With `wallpaper.mode = none`, no other wallpaper property is permitted.
- With `wallpaper.mode = image`, `asset_sha256` and `media_type` are required; `fit`, `position`, `opacity_millionths`, and `repeat` are independently optional.
- `colors`, when present, MUST contain `background`, `foreground`, and exactly 16 `palette` entries in ANSI index order 0 through 15. The cursor and selection properties are optional.
- `terminal`, when present, MUST contain at least one property.

A missing property means unmanaged. It MUST NOT be interpreted as a Ghostty default.

The wallpaper field is therefore tri-state:

| Representation | Meaning |
| --- | --- |
| `wallpaper` absent | wallpaper configuration is unmanaged |
| `wallpaper.mode = none` | wallpaper is managed and explicitly disabled |
| `wallpaper.mode = image` | the declared wallpaper properties are managed |

## Scalar rules

### Integers

All Manifest integers MUST be nonnegative and no greater than `9_007_199_254_740_991` (`2^53 - 1`). Field-specific ranges are:

| Field | Inclusive range | Unit |
| --- | ---: | --- |
| `schema_version` | exactly `1` | none |
| `wallpaper.opacity_millionths` | `0..=1_000_000` | one millionth |
| `terminal.background_opacity_millionths` | `0..=1_000_000` | one millionth |
| `terminal.font_size_millipoints` | `1_000..=1_000_000` | one thousandth of a point |
| `terminal.background_blur_intensity` | `0..=255` | Ghostty blur intensity |

Ghostty Wall intentionally limits wallpaper opacity to `0..=1`, although Ghostty accepts `background-image-opacity` values greater than `1`.

Profile decimals MUST be parsed as decimal values and converted exactly to fixed-point integers. Excess precision MUST be rejected. Implementations MUST NOT parse through binary floating point, silently round, or clamp.

### Colors

Every color MUST match:

```text
^[0-9a-f]{6}$
```

Colors are 8-bit sRGB values. A leading `#`, uppercase hexadecimal, named color, alpha channel, or alternate color space is invalid.

### Digests

When `wallpaper.mode = image`, `wallpaper.asset_sha256` MUST match:

```text
^[0-9a-f]{64}$
```

It identifies the exact durable asset bytes. The Manifest MUST NOT contain an asset path. The Projection resolves the digest through the machine-local Asset Store.

### Strings and enums

Every Manifest string MUST contain only ASCII characters permitted by its field grammar. Enums are closed and lowercase. Unknown enum values MUST be rejected.

`terminal.cursor_style`, when present, MUST be one of `block`, `bar`, `underline`, or `block_hollow`.

## Structural validity

A v1 reader:

- MUST reject `null`;
- MUST reject duplicate object member names;
- MUST reject unknown properties;
- MUST reject empty optional section objects;
- MUST reject missing required section properties;
- MUST reject arrays with an invalid length or order;
- MUST reject unsupported media types;
- MUST reject invalid strings and out-of-range integers;
- MUST NOT infer omitted values;
- MUST NOT insert Ghostty defaults;
- MUST NOT normalize unknown values;
- MUST NOT use Ghostty's clamping behavior as validation;
- MUST reject an unknown future schema version without modifying files.

## Canonical representation

The Manifest MUST first pass schema and semantic validation. It is then canonicalized using the JSON Canonicalization Scheme defined by [RFC 8785](https://www.rfc-editor.org/rfc/rfc8785.html).

The canonical representation is the UTF-8 byte sequence emitted by JCS. Implementations MUST NOT use ordinary JSON serialization, source property order, pretty printing, or a serializer-specific representation as digest input.

No floating-point values are permitted in a v1 Manifest.

## Environment identity

Given canonical Manifest bytes `canonical`, the digest preimage is:

```text
ASCII("ghostty-wall.environment-manifest.v1")
|| 0x00
|| canonical
```

Identity is:

```text
digest = SHA-256(preimage)
environment_id = "env-v1-" || lowercase_hex(digest)
```

The domain separator, NUL byte, hash algorithm, lowercase hexadecimal encoding, and ID prefix are all part of the v1 contract.

## Load invariants

When loading an Environment, all three values MUST agree:

1. the filename without `.json`;
2. the envelope's `environment_id`;
3. the ID recomputed from the canonical Manifest.

Any mismatch means the Environment is corrupt. A reader MUST report the corruption and MUST NOT silently rename, rewrite, or repair the record.

When `wallpaper.mode = image`, the referenced Durable Asset MUST exist and its bytes MUST hash to `wallpaper.asset_sha256`. A missing or mismatched asset makes the Environment non-replayable and verification MUST fail.

## Projection

Projection generation starts from an empty managed configuration every time. It MUST NOT merge with the previously active Environment.

The v1 mapping is:

| Manifest field | Ghostty option |
| --- | --- |
| `wallpaper.mode = none` | `background-image =` |
| resolved durable asset path for `wallpaper.mode = image` | `background-image` |
| `wallpaper.fit` | `background-image-fit` |
| `wallpaper.position` | `background-image-position` |
| `wallpaper.opacity_millionths` | `background-image-opacity` |
| `wallpaper.repeat` | `background-image-repeat` |
| `colors.background` | `background` |
| `colors.foreground` | `foreground` |
| `colors.palette[0..15]` | `palette = 0..15=<color>` |
| `colors.cursor` | `cursor-color` |
| `colors.selection_background` | `selection-background` |
| `colors.selection_foreground` | `selection-foreground` |
| `terminal.font_size_millipoints` | `font-size` |
| `terminal.background_opacity_millionths` | `background-opacity` |
| `terminal.background_blur_intensity` | `background-blur` |
| `terminal.cursor_style` | `cursor-style` |

Fixed-point values MUST be projected as exact decimal values without loss. Projection ordering, whitespace, comments, and decimal formatting are not stable across Ghostty Wall versions.

## Canonical test vector

Input Manifest:

```json
{
  "schema_version": 1,
  "wallpaper": {
    "mode": "image",
    "asset_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    "media_type": "image/jpeg",
    "fit": "cover",
    "position": "center",
    "opacity_millionths": 110000,
    "repeat": false
  },
  "colors": {
    "background": "11151c",
    "foreground": "e7e2dc",
    "cursor": "e7e2dc",
    "selection_background": "303846",
    "palette": [
      "11151c", "d35f72", "86b77d", "d3ae6f",
      "6f8faf", "a17cb8", "70b7b1", "c5c8c6",
      "4b5263", "e06c75", "98c379", "e5c07b",
      "61afef", "c678dd", "56b6c2", "e7e2dc"
    ]
  },
  "terminal": {
    "font_size_millipoints": 13500,
    "background_opacity_millionths": 920000,
    "background_blur_intensity": 20
  }
}
```

Canonical JCS bytes interpreted as UTF-8:

```text
{"colors":{"background":"11151c","cursor":"e7e2dc","foreground":"e7e2dc","palette":["11151c","d35f72","86b77d","d3ae6f","6f8faf","a17cb8","70b7b1","c5c8c6","4b5263","e06c75","98c379","e5c07b","61afef","c678dd","56b6c2","e7e2dc"],"selection_background":"303846"},"schema_version":1,"terminal":{"background_blur_intensity":20,"background_opacity_millionths":920000,"font_size_millipoints":13500},"wallpaper":{"asset_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","fit":"cover","media_type":"image/jpeg","mode":"image","opacity_millionths":110000,"position":"center","repeat":false}}
```

Expected digest:

```text
73d59a8d7c8c3470a27402ab2541c421eedb8377f5a414685b34d326f3c32163
```

Expected Environment ID:

```text
env-v1-73d59a8d7c8c3470a27402ab2541c421eedb8377f5a414685b34d326f3c32163
```

An implementation MUST pass this vector before creating public v1 Environment IDs.

## Explicit exclusions

The following MUST NOT appear in a v1 Manifest:

- Profile or Source identity;
- source URL, repository, branch, or local path;
- creation or activation timestamps;
- Ghostty Wall or Ghostty versions;
- operating system, hostname, or username;
- generator or algorithm identity;
- original filename, download URL, cache path, or Asset Store path;
- named Ghostty themes;
- arbitrary Ghostty configuration properties;
- macOS-specific blur values.

These values belong to Activation provenance, another explicitly versioned record, or nowhere.

## Verification oracles

Verification SHOULD be layered:

1. schema validation proves structural and range validity;
2. the canonical test vector proves canonicalization and digest compatibility;
3. recomputation proves filename, declared ID, and Manifest agreement;
4. asset hashing proves Durable Asset integrity;
5. projection parsing proves semantic agreement with the Manifest;
6. an optional real Ghostty `+show-config` probe checks integration with Ghostty's parser and loader.

The real Ghostty probe is an integration oracle, not a substitute for Manifest validation.
