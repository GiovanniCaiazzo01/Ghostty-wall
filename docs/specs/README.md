# Product specs: first tranche

Implementation contracts for the four verified problems selected from the product audit. These documents describe **planned behavior**, not features already available in a release.

Baseline: [v1.2.0 source at `3ee4f21`](https://github.com/GiovanniCaiazzo01/Ghostty-wall/tree/3ee4f214f335be6648c67d1107705580ad01dfcc). Use the vocabulary in [CONTEXT.md](../../CONTEXT.md): a Profile is a recipe, an Environment is an immutable resolved snapshot, and History records Activations.

## Specs and branches

| Order | Spec | Implementation branch | Dependencies |
| --- | --- | --- | --- |
| 1 | [Consistent Save / Save and use / Cancel](01-editor-actions.md) | `feat/spec-01-editor-actions` | None |
| 2 | [Edit, remove, and check Wallpaper Sources](02-source-management.md) | `feat/spec-02-source-management` | None |
| 3 | [Apply the prepared preview result](03-preview-apply.md) | `fix/spec-03-preview-apply` | 01 for the shared completion flow |
| 4 | [Assess readability on the composed wallpaper](04-wallpaper-readability.md) | `feat/spec-04-wallpaper-readability` | 01 and 03 for the draft/confirmation path |

Use the [release plan (#7)](https://github.com/GiovanniCaiazzo01/Ghostty-wall/issues/7) and its linked issues for current implementation and review status. The branch names above are working names; create a missing branch from an up-to-date `main`. The shared specification documents are included in [PR #13](https://github.com/GiovanniCaiazzo01/Ghostty-wall/pull/13), alongside the Spec 01 implementation.

## Work one spec at a time

1. Read the selected spec and check the release plan for current scope, dependencies and existing work.
2. Open or link a [GitHub issue](https://github.com/GiovanniCaiazzo01/Ghostty-wall/issues) for the selected spec. Issues remain the public execution tracker; these documents define scope, not a second status board.
3. Create its implementation branch from an up-to-date `main`, or switch to an existing branch and merge the latest `main`. Do this for each subsequent spec so completed dependencies are present. Do not reset or overwrite work already on a branch.
4. Recheck the baseline and run upstream GitNexus impact analysis before source edits. The risk notes below are planning evidence, not authorization to skip a fresh check.
5. Add the smallest regression that fails for the stated problem. Reuse the existing Rust tests and disposable-HOME PTY tests; do not add a test framework.
6. Implement only that spec. Update relevant CLI help, user documentation, examples, and website content for shipped behavior.
7. Run its focused checks, then the applicable CI gates. Run complete GitNexus change analysis before committing. A partial/truncated graph result is not a clean check.
8. Submit one implementation PR for that spec and link its issue and acceptance evidence. Start the next dependent spec after the previous work is merged.

For example, to start Spec 02 with a clean, up-to-date checkout when its branch does not yet exist:

```bash
git switch -c feat/spec-02-source-management main
```

## Shared constraints

- Preserve existing Profile formats, identifiers, Environment identities, historical replay, and versioned palette provenance.
- Save, apply, and reload remain separate outcomes. An accepted reload request is not proof of a visible Ghostty change.
- Do not change unmanaged Ghostty settings or original images. Previewing, inspecting, and cancelling must not create Activations.
- Preserve state locking, conflict checks, bounded reads, atomic publication, and ownership-aware rollback. Never turn an uncertain publication into a blind retry.
- Keep terminal restoration, small-window behavior, `NO_COLOR`, and non-terminal interaction working.
- No desktop theming, separate GUI, marketplace, new palette backend, or automatic migration in this tranche.
- Editing these specification documents alone needs no version bump or release tag.

## Planning risk

This table records the initial planning analysis against the v1.2.0 baseline. It is historical context; rerun impact analysis before changing source.

| Seam | Graph risk | Callers / affected paths | Planning constraint |
| --- | --- | --- | --- |
| `save_draft` | **HIGH** | Visual `edit_loop`, workflow `save`, publication/rollback and editor tests; create/edit and deletion-fallback flows | Reuse its contract; do not rewrite publication to add a UI action. |
| `command_source` | **LOW** | CLI `execute`, line-browser `tui_command` | Preserve existing add behavior and use the same state coordination. |
| Application `apply` | **LOW**, lower-bound graph | Management browser, `apply_profile`; textual confirmation also finds creation, visual edit, and direct CLI apply | Cover all interactive callers; graph omitted unresolved receivers. |
| `generate_kmeans_v3` | **CRITICAL** | Plan resolution, draft generation, editor color generation, image staging; local/GitHub apply paths | Spec 04 leaves this algorithm unchanged. Future generation changes require separately versioned provenance and replay regressions. |

## Deferred audit backlog

These are candidates for later specs, **not additional acceptance requirements for the four branches above**.

| Candidate | Outcome to specify later |
| --- | --- |
| Browser discovery and layout | Search, recent/favorite Profiles, useful preview space, and one clear simulated-preview label with accessible details. |
| Readable History | Recognizable name/date/image summaries and explicit replay of an Environment; retain immutable Activation records and `previous` semantics. |
| Display names and active-profile rename | A friendly editable name separate from stable identity, with legacy compatibility and no change to the current appearance. |
| Automatic palette variants | Focus / Balanced / Expressive and light/dark generation, compared on a fixed image corpus while preserving useful ANSI distinctions. |
| Curated starter Profiles and showcase | Licensed bright, dark, monochrome, and colorful examples; honest screenshots and a clear distinction between simulation and real Ghostty output. |
| Complete Profile import/export | Portable managed appearance without external paths; distinguish a source-backed recipe from a frozen resolved result and validate untrusted bundles. |
| Reversible live preview | Confirm/cancel plus real-window restoration proof, including multiple windows and unavailable reload. Keep the existing API library-only until that evidence exists. |
| Image normalization | Opt-in bounded resize/conversion/orientation handling, preserving originals and offering actionable errors. |
| Shell completions | Fish first, then Bash and Zsh; complete commands and local Profile/Source identifiers without side effects or network access. |
| Distribution | Separately scoped maintained Arch packaging and verified macOS support; do not imply a target is supported before its install/update/reload gates pass. |

After the first tranche, validate the golden path with five new users: install, import their image, obtain a result they would keep, save a second Profile, and return to the previous result. Record time to a satisfactory result, requests for help, manual configuration edits, repeated adjustments, and use after a few days. This is a manual product study, not a request to add telemetry.
