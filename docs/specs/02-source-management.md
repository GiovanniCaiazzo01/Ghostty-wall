# Spec 02: edit, remove, and check Wallpaper Sources

- Branch: `feat/spec-02-source-management`
- Depends on: none
- Baseline: v1.2.0 at `3ee4f21`
- Non-goals and shared constraints: [spec index](README.md)

## Problem statement

A moved directory or incorrectly configured Wallpaper Source sends the user back to manual TOML editing. The product can add and inspect Sources, but cannot safely edit or remove them through ordinary controls, and does not explain which Profiles depend on them before a change.

## Current baseline

- `src/cli.rs`: `command_source` accepts only `source add`; an identical definition is a no-op and a differing existing definition is rejected.
- `src/cli/maintenance.rs` and `src/cli/profile_forms.rs`: source administration is presented through the existing management forms/reports.
- `src/codec/intent.rs`: validates configuration and source references in Profile recipes.
- `src/selection.rs` and `src/github.rs`: own existing local/GitHub candidate resolution.

## Solution and user stories

Expose Source administration in both CLI and the management TUI, using existing local-directory and GitHub Source kinds.

1. As a user, I want to inspect a Source definition and the Profiles that use it.
2. As a user moving a wallpaper directory, I want to update its location without changing its identifier.
3. As a user correcting a GitHub Source, I want to edit repository, ref, and subpath without opening TOML.
4. As a user cleaning up unused Sources, I want removal to be explicit and safe.
5. As a user removing an in-use Source, I want to see the blocking Profiles rather than break their recipes.
6. As a user investigating a failed preview, I want a read-only check that distinguishes an unavailable Source from an empty Candidate Set.
7. As a user whose configuration changed concurrently, I want my stale form to refuse overwriting the new version.

## Implementation decisions

### Planned CLI surface

| Command | Contract |
| --- | --- |
| `source list` | List configured identifiers and kinds in canonical order. |
| `source show SOURCE` | Show definition and dependent saved Profiles, without resolving or writing. |
| `source edit SOURCE local DIRECTORY` | Explicitly replace an existing Source definition, retaining its identifier. |
| `source edit SOURCE github OWNER/REPO [--ref REF] [--path PATH]` | Explicitly replace an existing GitHub definition, retaining its identifier. |
| `source check SOURCE` | Resolve read-only and report availability, candidate count, or an actionable error. |
| `source remove SOURCE` | Show removal summary and request explicit confirmation, default Cancel. |

These are planned commands; existing `source add` arguments and duplicate-definition behavior remain unchanged. Edit specifies a complete replacement definition, so omitted GitHub ref/subpath fields use the existing Source defaults rather than silently retaining hidden old fields. The TUI pre-fills the current definition and shows the proposed change before saving.

- Reuse existing parsing, candidate resolution, forms, reports, and state coordination. Preserve unrelated configuration and comments when editing one Source.
- Editing a Source does not rename it, rewrite dependent Profiles, apply an Environment, request reload, or modify the current Projection/History. Show dependent Profile identifiers before confirmation in the TUI and in the CLI result. Future resolution may select a different wallpaper; say so.
- Validate the complete revised configuration and that existing Profile references remain structurally valid. A syntactically valid but unreachable Source can be saved explicitly; reachability is assessed by Check, not promised by Save.
- Removal is refused if any saved Profile references the Source, including protected or inactive Profiles. List the blocking identifiers. No force/cascade option in this spec.
- If a Profile cannot be inspected safely, dependency analysis is incomplete: refuse removal and report the problem rather than assume it is unreferenced.
- Removing an unused Source removes only its configuration entry. Never remove source directories/files, original images, owned images, Durable Assets, Environments, or History.
- Check reuses the existing bounded resolver and credentials. It does not activate, reload, create a persistent cache, or decode/download every candidate image. State clearly that a reachable listing is not proof every image will decode.
- Missing/invalid/inaccessible Sources and empty Candidate Sets receive distinct diagnostics. Check returns a failure status when the Source is not usable, without changing durable state.
- Hold the existing state lock for mutations and verify the configuration version used by the form before publication. Recompute removal dependencies under the lock; a concurrently added reference must block deletion.
- Cancellation/EOF preserves the definition. Read-only checks remain dismissible through existing maintenance behavior, with late results prevented from overwriting a different selection.

## Acceptance criteria

- [ ] The user can edit local and GitHub definitions and remove an unused Source through CLI and TUI without opening TOML.
- [ ] Source identity stays fixed; dependent Profile recipes are byte-preserved.
- [ ] Updating a Source leaves the active Projection, History/cursor, integration, images, and reload call count unchanged.
- [ ] Identical `source add` remains a no-op; differing `source add` still refuses silent replacement.
- [ ] Show/edit/remove report dependent Profiles in deterministic order.
- [ ] An in-use Source cannot be removed; missing/corrupt Profile data cannot turn into authorization to remove it.
- [ ] Cancel and EOF do not publish any configuration change.
- [ ] A concurrent configuration edit or new Profile reference is detected before publication; unrelated data is not overwritten.
- [ ] Check distinguishes a working Source, no candidates, missing/unreadable local path, malformed GitHub definition, and remote failure using fake HTTP responses in tests.
- [ ] Replaying an existing Environment after Source removal or relocation still works from its Durable Assets.
- [ ] Successful Source changes refresh the browser and invalidate previews based on the old configuration, without automatically applying anything.

## Testing decisions

Test through public CLI commands and the existing management PTY seam using disposable Managed Roots. Assert configuration bytes, source/original files, durable History, and Projection; do not depend on live GitHub.

Prior art: `tests/management_cli.rs`, `tests/cli_release.rs`, `tests/github_source.rs`, `tests/apply_history.rs`, `tests/maintenance_pty.py`, and `tests/qa_management_pty.py`. Add focused source-command regressions using those conventions, including dependency refusal and concurrent changes.

Focused starting commands; new Source behavior must have runnable coverage in these suites or a narrowly scoped source-command test target:

```bash
cargo test --locked --test management_cli --test cli_release --test github_source --test apply_history
cargo build --locked
python3 -m unittest discover -s tests -p maintenance_pty.py -v
python3 -m unittest discover -s tests -p qa_management_pty.py -v
```

Update CLI help, Source examples, advanced usage, and website Source/management reference. Document complete-definition edit semantics and removal refusal.

## Out of scope and stop condition

No Source renaming, identifier migration, additional Source providers, background synchronization, force deletion, or image cleanup. Stop once supported Sources can be inspected, edited, checked, and safely removed with the dependency and preservation guarantees above.
