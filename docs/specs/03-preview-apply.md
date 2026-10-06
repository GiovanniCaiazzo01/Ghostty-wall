# Spec 03: apply the prepared preview result

- Branch: `fix/spec-03-preview-apply`
- Depends on: [01](01-editor-actions.md) for the shared draft completion path
- Integration case: Source changes from [02](02-source-management.md)
- Baseline: v1.2.0 at `3ee4f21`
- Non-goals and shared constraints: [spec index](README.md)

## Problem statement

The user chooses a particular simulated result, but Use resolves the recipe again. Reusing a Resolution Seed does not freeze a changing Candidate Set or image bytes. A different image can therefore be committed from the one the user approved.

The promise here is exact **managed content**, not pixel-identical rendering of a real Ghostty window.

## Current baseline

- `src/cli/management/preview.rs`: the background worker prepares a sample and watches file versions, but delivers only the sample to the view.
- `src/cli/management.rs`: Use calls Application apply with a Profile identifier and the session seed, resolving again.
- `src/cli/editor.rs`: Save and use also calls apply after saving, rather than committing the prepared draft result.
- `src/plan.rs` and `src/apply.rs`: own resolution, durable asset materialization, Activation commit, recovery, and Projection publication.
- `docs/advanced-usage.md`: explicitly documents that changed Sources can change the applied image.

## Solution and user stories

Retain one bounded, in-memory prepared result for the currently approved preview. Apply its validated Environment Manifest and original image bytes through the normal durable commit path.

1. As a user previewing a random Profile, I want Use to apply exactly that image and palette.
2. As a user whose Source gained or lost candidates, I want the approved captured image used instead of a new random selection.
3. As a user navigating quickly, I do not want a late preview for another Profile to authorize Use.
4. As a user with a stale recipe/configuration, I want a fresh preview and a new explicit choice rather than a silently substituted result.
5. As a user editing wallpaper or colors, I want Save and use to commit the last complete sample I reviewed.
6. As a user whose apply failed, I want normal recovery and outcome reporting, not a special unsafe preview-only apply path.
7. As a user of scripts, I want direct `apply` to retain its existing fresh-resolution behavior.

## Implementation decisions

- Keep the prepared Environment Manifest, original bounded asset bytes, expected content hashes, resolution provenance, and semantic dependencies together. A thumbnail or rendered PNG is not a substitute for the approved original asset.
- Attach readiness to the selected Profile and latest draft/selection generation. A resize can rebuild presentation without selecting another Candidate. Ignore late worker results for superseded requests.
- Browsing and preparation stay read-only. Retaining in-memory bytes must not persist a cache, create an Environment on disk, publish an image, or append an Activation before explicit Use.
- Use is unavailable while the selected result is loading, failed, or invalidated. Explain what the user must do; do not quietly resolve and apply another result.
- Candidate Set additions/removals or original image-file changes alone need not invalidate captured bytes: if the prepared result remains valid, commit those approved bytes, not newly fetched bytes. Applying a valid captured remote result must not need a second network resolution.
- A changed Profile recipe, Source configuration, or resolved theme dependency invalidates approval. Missing/mutated prepared bytes, failed hash verification, or unsafe publication boundaries also prevent commit. Refresh the preview and require another explicit Use; never apply the refreshed result automatically.
- For editing, recompute the prepared result after any visual setting/image change. Saving publishes the approved recipe first; apply then verifies that the saved recipe corresponds to that draft, without mistaking its own save for an unrelated concurrent change.
- Validate dependencies and content again at the existing synchronized commit boundary. Reuse normal recovery, integration checks, Durable Asset retention, immutable Environment identity, Activation provenance, Projection publication, and best-effort reload.
- The committed Environment ID and managed image hash must match the approved result. Keep its Selection/Resolution Seed provenance; do not fabricate a new resolution or put presentation/session metadata into the Environment Manifest.
- Save and apply remain separate transactions, with Spec 01 outcomes. Saving can succeed even if subsequent apply validation fails.
- Bound retention to the current result and the existing bounded in-flight/pending preview work. Do not retain an unbounded catalogue of full-size images or block terminal restoration on remote I/O.
- Direct non-preview CLI `apply`, `plan`, and `preview` keep their existing contracts. There is no cross-process preview token or new persistent format in this spec.

## Acceptance criteria

- [ ] With an unchanged Source, committed Environment ID, managed image bytes/hash, colors, and wallpaper settings match the approved sample inputs.
- [ ] Adding/removing candidates after a random preview cannot change the image applied by Use.
- [ ] Modifying/removing the original image after preparation still applies the captured approved bytes when all semantic dependencies remain valid.
- [ ] A captured GitHub result can be used after the server becomes unavailable, without silently selecting/fetching a different image.
- [ ] Changing a Profile, Source definition, or theme after preview blocks the old approval; refreshed content requires another explicit choice.
- [ ] Navigating away, editing the draft, or receiving a late worker result cannot authorize a stale result.
- [ ] Use during loading/error does not create an Activation or change Projection; a resize alone does not trigger a new random choice.
- [ ] Save and use in visual Create/Edit commits the same final managed content that was reviewed, while preserving separate save/apply failure semantics.
- [ ] Corrupt prepared bytes or a commit conflict fail through normal safeguards, without rewriting History or bypassing ownership checks.
- [ ] Success records exactly one Activation; `previous` can replay the captured asset after its original Source disappears.
- [ ] Preparation is still responsive, cancellable from the UI, bounded, and free of durable writes.
- [ ] Direct CLI apply and existing historical identity/replay fixtures remain compatible.

## Testing decisions

Exercise the highest shared preparation-to-commit seam and verify the resulting immutable manifest, asset hash, Activation, and Projection. Use a disposable local Source and fake GitHub responses. Mutate the Candidate Set, original bytes, recipe, configuration, and theme between preparation and Use; this must expose the old re-resolution bug.

Prior art: `tests/terminal_browser.rs`, `tests/plan_local.rs`, `tests/apply_history.rs`, `tests/preview_session.rs`, `tests/github_source.rs`, `tests/responsive_preview_pty.py`, and `tests/qa_management_pty.py`. Add a focused prepared-result integration test if existing public seams cannot express the full transition; do not create a parallel apply subsystem for testing.

Focused starting commands; extend or add the preparation-to-commit regression before these are considered evidence of completion:

```bash
cargo test --locked --test terminal_browser --test plan_local --test apply_history --test preview_session --test github_source
cargo build --locked
python3 -m unittest discover -s tests -p responsive_preview_pty.py -v
python3 -m unittest discover -s tests -p qa_management_pty.py -v
```

Update preview guidance, CLI/TUI help, advanced usage, and website preview limitations. Remove the changed-Source mismatch warning only when the captured-result contract is implemented; retain all real-rendering limitations.

## Out of scope and stop condition

No live Ghostty preview, persistent preview cache, cross-process approval format, exact font/desktop simulation, or new selection algorithm. Stop when interactive approval reliably commits the same managed result and stale approval is rejected with a fresh-choice path.
