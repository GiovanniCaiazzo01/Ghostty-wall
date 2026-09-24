# RFC 0008: Managed Filesystem Layout and Initialization v1

Status: Accepted
Date: 2026-09-23

## Purpose

This RFC defines the v1 Managed Root, filesystem layout, ownership, initialization, integration hook, repair, legacy migration, and safe uninstall behavior.

V1 deliberately uses one Managed Root. Intent, durable state, derived state, and coordination are semantically distinct but physically co-located to simplify backup, recovery, and ownership.

## Managed Root

The platform paths are:

```text
Linux:
${XDG_CONFIG_HOME:-$HOME/.config}/ghostty/ghostty-wall

macOS:
$HOME/Library/Application Support/com.mitchellh.ghostty/ghostty-wall
```

macOS always uses the Application Support path for the Managed Root. Ghostty root-config discovery remains independent and considers both XDG and macOS-specific locations.

V1 has no public `GHOSTTY_WALL_HOME` or equivalent override. Tests inject a PathResolver.

## Layout

The published layout is:

```text
ghostty-wall/
├── state.lock
├── config.toml
├── profiles/
├── assets/
│   └── sha256/
│       └── ab/
│           └── <sha256>.{png|jpg}
├── environments/
├── history/
│   └── activations/
├── cache/
└── current.ghostty
```

`cache/` and `current.ghostty` are optional after initialization. Every other displayed component through `history/activations/` is required after successful init.

There is no `generated/` directory in v1.

## Data classes

```text
AUTHORITATIVE INTENT
  config.toml
  profiles/

DURABLE APPLICATION STATE
  assets/sha256/
  environments/
  history/activations/

DERIVED / DISPOSABLE
  cache/
  current.ghostty

COORDINATION / PUBLICATION
  state.lock
```

Deleting cache is always semantically safe. Deleting authoritative or durable state is not.

## AssetStore paths

For digest `d`:

```text
image/png:
assets/sha256/<d[0..2]>/<d>.png

image/jpeg:
assets/sha256/<d[0..2]>/<d>.jpg
```

Rules:

- digest is exactly 64 lowercase hexadecimal characters;
- shard is exactly the first two digest characters;
- `image/png` maps to `.png`;
- `image/jpeg` maps to `.jpg`;
- identity remains SHA-256 of the exact bytes.

On read or reuse, shard, filename digest, extension, validated media type, and SHA-256 of actual bytes MUST agree.

At most one canonical file may exist for one digest. Both `.png` and `.jpg`, duplicate noncanonical locations, or content/media mismatch are AssetStore corruption.

## Filesystem boundary

User-controlled ancestors of Managed Root may resolve through symbolic links, mount points, or bind mounts. Init resolves them to a canonical parent.

The final `ghostty-wall` component and all managed structural components below it MUST NOT be symbolic links.

Sensitive components are:

```text
state.lock
config.toml
profiles/
assets/
assets/sha256/
environments/
history/
history/activations/
cache/
current.ghostty
```

Expected directories are real directories. Expected files are regular files when present. `current.ghostty` and `cache/` may be absent. Init never follows, removes, replaces, or repairs a symlink at a sensitive component.

## Ownership and permissions

New directories are created with mode `0700`. New files are created with mode `0600`; creation MUST set permissions intentionally rather than relying on process umask.

Existing managed components:

- are owned by the current effective user;
- are not writable by group or other users.

Group or other readability is non-fatal in v1. Normal `plan`, `apply`, and `previous` never change permissions. `doctor` reports unsafe permissions. `init --repair` may only tighten permissions and MUST NOT change ownership.

Wrong ownership is fatal and is never repaired with `chown`.

## Default Intent

First init writes:

```toml
schema_version = 1

[sources.welcome]
kind = "local-directory"
path = "profiles"
```

First init also writes `profiles/welcome.toml` and bundled `profiles/welcome.png`, plus a local `welcome` Source in `config.toml`. `plan welcome` works without extra files or a Resolution Seed. Existing published installations keep their Intent unchanged; init never backfills or replaces example files.

## Eager and optional creation

Init eagerly creates:

```text
Managed Root
profiles/
assets/
assets/sha256/
environments/
history/
history/activations/
cache/
config.toml
profiles/welcome.toml (first init only)
profiles/welcome.png (first init only)
state.lock
```

It does not create `current.ghostty` before the first committed Activation.

After init, missing cache remains valid. Read-only commands do not recreate it. Mutating commands and `init --repair` may recreate it when useful.

## Publication marker

`state.lock` is both the lock target defined by RFC 0007 and the durable publication marker for a complete required layout.

Successful publication is:

```text
create state.lock
fsync state.lock
fsync Managed Root
```

Only after the Managed Root fsync is the layout published.

Init uses a reserved temporary in-progress marker before publication. The marker is not part of the published layout or domain state. It allows `init --repair` to distinguish a provably interrupted first init from unexplained loss. A stale marker beside a valid published `state.lock` is implementation debris and may be removed safely.

## Required filesystem capabilities

Real init probes the actual Managed Root filesystem for:

- advisory locking;
- file fsync;
- directory fsync;
- atomic replacement;
- atomic publication without replacement.

Probe objects belong exclusively to that init invocation. They are removed after probing.

Missing required capability is fatal. Init MUST NOT substitute a weaker protocol. The Integration Hook is not installed after a failed capability probe.

`--dry-run` performs no capability mutation and reports each capability as `requires-runtime-probe`, never `verified`.

## Init ordering

First init is:

```text
canonicalize and validate parent
create Managed Root safely
create reserved in-progress marker
create required directories
write default config.toml atomically
create bundled welcome Profile and image without replacing existing files
create optional cache/
probe required filesystem capabilities
create and fsync state.lock
fsync Managed Root
remove stale in-progress marker when safe
install Integration Hook
```

The Integration Hook is always last. A crash before `state.lock` publication leaves an incomplete unpublished layout. A crash after publication but before hook installation leaves a valid layout with integration drift.

Init tracks every object created by its invocation. On ordinary failure it removes only an object it created and only when that object remains unchanged and empty where required. It MUST NOT recursively delete Managed Root as generic rollback.

## Ghostty root config resolution

Ghostty root config candidates are evaluated in Ghostty load order:

```text
all platforms:
  $XDG_CONFIG_HOME/ghostty/config.ghostty
  $XDG_CONFIG_HOME/ghostty/config

macOS additionally:
  $HOME/Library/Application Support/com.mitchellh.ghostty/config.ghostty
  $HOME/Library/Application Support/com.mitchellh.ghostty/config
```

When `XDG_CONFIG_HOME` is unset, it defaults to `$HOME/.config`.

Init selects the last existing candidate. If none exists, it creates:

```text
Linux:
$XDG_CONFIG_HOME/ghostty/config.ghostty

macOS:
$HOME/Library/Application Support/com.mitchellh.ghostty/config.ghostty
```

The selected config and any created parent directory are user-owned integration files, not Managed Root.

## Root config symbolic links

A user-owned Ghostty root config may be a symbolic link.

Init resolves the complete link chain, verifies that the final target is a regular file owned by the current user with safe permissions, and edits that target atomically in its own directory. It preserves the root-config symlink entry.

Before commit, Init revalidates the verified target identity and MUST NOT accidentally replace or follow a differently retargeted symlink.

## Integration Hook

Ghostty Wall owns exactly one semantic `config-file` directive whose optional target resolves to the canonical absolute path of:

```text
<Managed Root>/current.ghostty
```

The rendered directive follows Ghostty's `config-file` grammar and uses the optional `?` prefix. Semantic ownership is independent of whitespace or textual path spelling.

Hook comparison:

1. parses each `config-file` directive;
2. identifies optionality;
3. resolves relative paths against the containing config;
4. normalizes the resolved target;
5. compares it with the canonical Projection path.

Unrelated `config-file` directives are never modified.

## Normal init

Normal init behavior is:

| State | Result |
| --- | --- |
| Managed Root absent | create installation |
| published required layout valid | preserve |
| cache absent | valid; may recreate |
| current Projection absent | valid |
| one equivalent hook in effective root | no-op |
| no equivalent hook | install canonical hook |
| duplicate equivalent hooks | require `--repair` |
| equivalent hook only in stale/non-effective root | require `--repair` |
| stale managed target | require `--repair` |
| managed component symlink | fatal |
| invalid Intent | category `intent`; preserve |
| missing required authoritative/durable structure | fatal or require explicit repair as allowed below |

Idempotent no-op init does not rewrite files or change mtimes.

## Dry run

`init --dry-run` performs no mutation. It reports:

- resolved Managed Root;
- Ghostty root candidates and selected effective target;
- required and optional layout state;
- hook state and planned edits;
- permission or ownership failures;
- legacy-state detection;
- filesystem capability probes as `requires-runtime-probe`.

Dry run never claims a mutation-dependent property was verified.

## Repair

`init --repair` is explicit and conservative.

It may:

- recreate missing cache;
- recreate `state.lock` after validating the remaining layout;
- tighten permissions without changing ownership;
- complete a provably interrupted first init;
- normalize equivalent Integration Hooks.

Completing interrupted first init is allowed only when a reserved in-progress marker proves the attempt and no user Profile, Asset, Environment, or Activation record exists. Exact bundled welcome Profile and image bytes are recognized as init-owned; altered or unexpected files stop repair. A valid existing `config.toml` is preserved. A missing `config.toml` may be replaced with default only in that proven pristine interrupted-init state.

When any user Profile, Asset, Environment, or Activation exists, Repair MUST NOT synthesize a missing authoritative or durable component. Examples such as missing `config.toml`, `profiles/`, `assets/`, `environments/`, or `history/activations/` are preserved as evidence of possible data loss and reported as failure.

Repair of hooks:

1. determines the currently effective root config;
2. ensures exactly one equivalent hook there;
3. removes equivalent hooks from stale or other root configs;
4. leaves unrelated directives untouched;
5. modifies each real config target atomically in its own directory.

Repair does not move Managed Root and does not perform legacy migration.

## Legacy v0 migration

Legacy migration is explicit:

```text
ghostty-wall init --migrate-legacy
ghostty-wall init --migrate-legacy --dry-run
```

Normal init never imports legacy state automatically.

Recognized legacy inputs are:

- `wallpaper_repos.txt` at the known v0 Ghostty config location;
- the known v0 `wallpaper.conf`;
- a `config-file` directive whose resolved target exactly equals that known legacy `wallpaper.conf`.

A matching basename elsewhere is not recognized ownership.

### Migration preflight

Before any mutation, migration:

1. parses every non-comment legacy Source entry;
2. validates every legacy name against the v1 Source slug;
3. normalizes every Source;
4. validates repository, branch, and path;
5. checks collisions among imported entries;
6. checks collisions against current `config.toml`;
7. constructs complete resulting Intent in memory;
8. identifies only the strictly recognized legacy hook;
9. validates every target edit.

Any validation failure produces zero Source imports and zero hook changes.

Mapping rules:

- a legacy name must already be a valid v1 Source identifier;
- empty legacy branch becomes `ref = "main"` to preserve v0 behavior;
- identical existing Source is a no-op;
- same Source ID with different content is an error;
- automatic slug invention and partial import are forbidden.

The legacy `wallpaper_repos.txt` and `wallpaper.conf` files are always preserved.

### Migration commit and crash behavior

Migration cannot promise one atomic transaction across Managed Root and user root configs. It guarantees:

- complete preflight before mutation;
- each modified file is replaced atomically in its own directory;
- deterministic, idempotent continuation after crash.

Commit progression is:

```text
atomically update config.toml
publish or validate v1 layout
install v1 Integration Hook
atomically remove recognized legacy hook
complete
```

If interrupted, rerunning the same migration recognizes already-identical Sources and completed steps, then continues. While imported Intent and hook state show a recognizable incomplete migration, `plan`, `apply`, and `previous` MUST fail rather than pretend migration completed.

No persistent migration journal is required when artifact state determines the next idempotent step unambiguously.

## Safe recursive cache removal

Cache is owned disposable state and may be removed recursively only with bounded no-follow traversal:

- traversal never follows symbolic links;
- traversal never escapes canonical cache root;
- a symlink entry is removed as an entry, not followed;
- unexpected ownership or unsafe type transitions stop deletion.

This permission does not apply to `current.ghostty` or any authoritative or durable directory.

## Uninstall

V1 uninstall removes integration and disposable state while preserving recoverable user history.

Preflight identifies and validates:

- owned equivalent Integration Hooks;
- every root config target to edit;
- Managed Root and state lock;
- Projection and cache removal safety;
- binary ownership evidence, when binary removal is requested.

If hook removal cannot be performed safely, uninstall stops before any mutation.

Commit order is:

```text
acquire exclusive state lock
revalidate hook and layout
atomically remove owned Integration Hooks
fsync every affected root-config directory
remove current.ghostty safely under RFC 0007
remove cache safely
fsync Managed Root
release state lock
remove owned binary last, when ownership is proven
```

Uninstall preserves:

```text
state.lock
config.toml
profiles/
assets/
environments/
history/
legacy files
```

There is no destructive `--purge` in v1.

Binary lifecycle remains with an external package manager unless an official installer can prove that it owns the exact binary path. Running executable location alone is not ownership proof. Failure to prove ownership leaves the binary installed and reports that outcome.

A future reinstall recognizes the preserved published layout, reinstalls the hook, and reconstructs Projection from the latest Activation.

## Prohibited behavior

Init, Repair, migration, and uninstall MUST NOT:

- follow a symlink inside Managed Root;
- recursively delete Managed Root;
- silently weaken locking, fsync, or atomic-publish requirements;
- overwrite invalid Intent;
- synthesize missing durable state where loss may have occurred;
- delete legacy files;
- modify unrelated Ghostty includes;
- claim cross-filesystem multi-file atomicity;
- remove a binary without ownership proof.
