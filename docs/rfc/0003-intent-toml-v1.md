# RFC 0003: Intent TOML v1

Status: Accepted
Date: 2026-09-23

## Purpose

This RFC defines the complete authoritative TOML intent model for Ghostty Wall v1.

Only two locations contain authoritative TOML:

```text
config.toml
profiles/*.toml
```

`config.toml` owns the Source registry. Each file in `profiles/` owns one Profile recipe. Runtime state, history, Environment records, generated output, cache configuration, and current selections MUST NOT be stored in these TOML files.

Readers MUST reject duplicate keys, unknown properties, invalid unions, and unknown future schema versions.

## Identifiers

Source and Profile identifiers MUST match:

```text
^[a-z0-9]+(?:-[a-z0-9]+)*$
```

Their encoded length MUST be between 1 and 64 ASCII bytes.

A Source identifier is the key below `sources` in `config.toml`. A Profile identifier is its filename without `.toml`. Neither identifier is duplicated as a field.

## `config.toml`

The only permitted top-level properties are:

```text
schema_version
sources
```

`schema_version` is required and MUST equal `1`. `sources` is required and MAY be empty.

Example:

```toml
schema_version = 1

[sources.anime]
kind = "github"
repository = "ThePrimeagen/anime"
ref = "master"

[sources.landscapes]
kind = "github"
repository = "somebody/wallpapers"
path = "landscapes"

[sources.local]
kind = "local-directory"
path = "~/Pictures/wallpapers"
```

There is no authoritative `sources/` directory in v1.

## Source union

Every Source has a required `kind`. The v1 enum is closed:

```text
github
local-directory
```

### GitHub Source

Schema:

```toml
[sources.anime]
kind = "github"
repository = "ThePrimeagen/anime"
ref = "master"
path = "wallpapers"
```

Rules:

- `repository` is required and contains exactly one `/` separating non-empty owner and repository components.
- `ref` is optional. When absent, resolution discovers the repository's default branch.
- `path` is optional. When absent, the repository root is used.
- `path` is a slash-separated relative path.
- `path` MUST NOT start with `/`, contain `\`, or contain empty, `.`, or `..` segments.

A requested ref is not an immutable revision. Resolution records the requested ref and resolved commit in Activation provenance. Neither belongs in an Environment Manifest.

### Local-directory Source

Schema:

```toml
[sources.local]
kind = "local-directory"
path = "~/Pictures/wallpapers"
```

`path` is required.

Path resolution is:

- a `~/` prefix expands to the current user's home directory;
- an absolute path is used directly;
- a relative path is resolved against the directory containing `config.toml`;
- no other tilde form, environment variable, or shell expansion is performed.

Recursive discovery, globbing, include/exclude filters, extension configuration, and symlink policy are outside this intent schema and require a future Source contract.

## Profile files

A Profile is stored at:

```text
profiles/<profile-id>.toml
```

Subdirectories are not part of v1. The only permitted top-level properties are:

```text
schema_version
wallpaper
colors
terminal
```

`schema_version` is required and MUST equal `1`. The other sections are independently optional. Inheritance and implicit global defaults are not supported.

This is a valid Profile:

```toml
schema_version = 1
```

It resolves to an Environment with an empty managed Projection.

## Wallpaper intent

The `wallpaper` section is a tagged union.

Section absent:

```text
wallpaper is unmanaged
```

Managed disabled:

```toml
[wallpaper]
mode = "none"
```

No other wallpaper property is permitted with `mode = "none"`.

Resolved from a Source:

```toml
[wallpaper]
mode = "source"
source = "anime"
selection = "random"
fit = "cover"
position = "center"
opacity = 0.11
repeat = false
```

With `mode = "source"`:

- `source` is required and MUST reference an existing Source identifier;
- `selection` is required and MUST be `random` or `path`;
- `path` is required with `selection = "path"` and forbidden with `selection = "random"`;
- `fit`, `position`, `opacity`, and `repeat` are independently optional.

A selection path is relative to the Source's logical root and follows the GitHub Source path safety rules.

Wallpaper enums are:

```text
fit:
  contain | cover | stretch | none

position:
  top-left | top-center | top-right |
  center-left | center | center-right |
  bottom-left | bottom-center | bottom-right
```

An omitted visual property remains unmanaged in the resolved Environment. The resolver MUST NOT insert Ghostty defaults.

Random selection follows [RFC 0002](./0002-random-selection-v1.md). A Plan requires a Resolution Seed when and only when Profile resolution requires randomness.

## Colors intent

The `colors` section is a tagged union with a closed `mode` enum.

### Generated

```toml
[colors]
mode = "generated"
```

Generated colors require `wallpaper.mode = "source"`. An unmanaged or disabled wallpaper is a static validation error.

The current palette algorithm is selected by the resolver. Its identity and version are Activation provenance, not Profile intent or Environment identity.

### Named Ghostty theme

```toml
[colors]
mode = "theme"
theme = "TokyoNight"
```

`theme` is required and MUST be non-empty. Resolution materializes only the colors represented by the managed color model in RFC 0001. The theme name is Activation provenance and MUST NOT appear in the Environment Manifest.

### Explicit

```toml
[colors]
mode = "explicit"
background = "1a1b26"
foreground = "c0caf5"
palette = [
  "15161e", "f7768e", "9ece6a", "e0af68",
  "7aa2f7", "bb9af7", "7dcfff", "a9b1d6",
  "414868", "f7768e", "9ece6a", "e0af68",
  "7aa2f7", "bb9af7", "7dcfff", "c0caf5",
]
cursor = "c0caf5"
selection_background = "33467c"
selection_foreground = "c0caf5"
```

`background`, `foreground`, and exactly 16 palette entries are required. Cursor and selection properties are optional. Every color MUST match `^[0-9a-f]{6}$`.

Properties belonging to another colors mode are forbidden.

## Terminal intent

Example:

```toml
[terminal]
font_size = 13.5
background_opacity = 0.92
background_blur_intensity = 20
cursor_style = "bar"
```

Every property is optional, but an empty `terminal` table is invalid. An omitted property remains unmanaged.

| Property | Contract |
| --- | --- |
| `font_size` | decimal `1..=1000`, at most 3 fractional digits |
| `background_opacity` | decimal `0..=1`, at most 6 fractional digits |
| `background_blur_intensity` | integer `0..=255` |
| `cursor_style` | `block`, `bar`, `underline`, or `block_hollow` |

Resolution maps decimal values to the fixed-point fields defined by RFC 0001.

## Decimal parsing

Profile decimals are lexical decimal input. They MUST be validated and converted without passing through binary floating point.

Examples:

```text
13.5       -> 13500 millipoints
0.92       -> 920000 millionths
0.1234567  -> error
```

Implementations MUST reject excessive precision, exponent notation, non-finite values, silent rounding, and clamping.

## Rename semantics

Renames are explicit operations.

Renaming a Source atomically updates:

- its key in `config.toml`;
- every live Profile reference to that Source.

Renaming a Profile atomically renames its file.

Existing Activation records are historical provenance and MUST NOT be rewritten by either rename.

## Migration and mutation

Intent migrations are explicit operations. Ordinary resolution or apply commands MUST NOT rewrite intent files to a newer schema.

Every intent mutation MUST be atomic and idempotent. A failed multi-file mutation, including Source rename, MUST leave the complete prior intent valid and visible.
