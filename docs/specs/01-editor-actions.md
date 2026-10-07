# Spec 01: consistent draft completion and clear outcomes

- Branch: `feat/spec-01-editor-actions`
- Depends on: none
- Baseline: v1.2.0 at `3ee4f21`
- Non-goals and shared constraints: [spec index](README.md)

## Problem statement

A user can create a Profile for later, but the visual editor only offers Save and use. Editing a future Profile therefore requires a different, field-by-field workflow. Completion messages also expose Activation details before clearly explaining whether saving, applying, or reloading succeeded.

## Current baseline

- `src/cli/editor.rs`: confirmed visual completion always saves and then calls `use_saved`.
- `src/cli/create_tui.rs` and `src/cli.rs`: creation saves first, then asks Use now / Not now.
- `src/profile_workflow.rs`: `save_draft` already saves without applying; `use_saved` invokes the separate durable apply path.
- `docs/advanced-usage.md`: documents the current distinction between visual and field edits.

## Solution and user stories

Creation and visual editing expose the same three completion intentions: **Save**, **Save and use**, and **Cancel**.

1. As a user editing a future Profile, I want to save without changing Ghostty.
2. As a user creating a Profile, I want the same completion choices that I see when editing.
3. As a user trying changes, I want Cancel to discard only my unsaved draft.
4. As a user who declined Save and use confirmation, I want my draft retained.
5. As a user encountering a definite save failure, I want to correct it without losing the draft.
6. As a user whose apply failed after saving, I want to know the Profile was still saved.
7. As a user whose reload is unavailable, I want an explicit manual-reload instruction.
8. As a user investigating an error, I want technical details without having them dominate the main status.

## Implementation decisions

- Reuse the shared in-memory editor and existing save/apply services. Return an explicit completion choice instead of treating successful save as permission to apply.
- Full-screen Create and Edit offer Save / Save and use / Cancel with visible keyboard guidance. Save and use retains its confirmation, defaulting to Back to editor. Cancel remains available through the existing escape/quit controls.
- Save validates and publishes the Profile and any staged owned image, then finishes without applying. For an unchanged Profile it may be a no-op, but still must not activate or reload.
- Cancel before publication writes no Profile, managed image, Projection, or History. Declining Save and use confirmation is not Cancel: it returns to the same draft.
- A definite save failure retains the draft and existing ownership-aware rollback. Uncertain publication or incomplete rollback preserves the existing inspection-required behavior; do not offer an unsafe retry or claim that cancellation undoes it.
- Save and use remains two operations: save first, then apply. Apply failure does not delete the saved Profile or pretend it is unsaved.
- Keep existing command arguments and immediate, non-applying field edits unchanged. Standalone line-based creation retains its existing accepted answers and safe defaults; its Save then Use now / Not now sequence may remain as a compatibility presentation of the same intentions.
- Do not connect the experimental live preview API.

### Main outcome messages

| Known result | Main message |
| --- | --- |
| Save succeeded; apply not requested | `Profile saved.` |
| Save succeeded; apply failed | `Profile saved; applying failed. See details.` |
| Configuration committed; reload action accepted | `Configuration updated; reload requested.` |
| Configuration committed; reload unavailable or failed | `Configuration updated; reload Ghostty manually.` |
| Publication or rollback uncertain | An inspection-required warning; no success message or automatic retry. |

The Profile name may accompany a message. Activation identifiers and reload failure reasons remain in the existing detailed report. Never say a visible change was verified merely because a reload action was accepted. Apply errors with uncertain effects must not be described as leaving everything unchanged.

## Acceptance criteria

- [ ] Create and visual Edit expose all three intentions without requiring TOML or field commands.
- [ ] Save on a changed existing Profile updates the recipe and necessary staged image only; current Projection, History contents/cursor, integration, and reload call count are unchanged.
- [ ] Save on a new Profile makes it available in the browser without activating it.
- [ ] Cancel preserves original Profile and image bytes and leaves no newly published image.
- [ ] Declining Save and use preserves all draft edits and allows a later Save.
- [ ] Successful Save and use records exactly one normal Activation and requests reload only after durable commit and lock release.
- [ ] A definite save failure retains the draft; a retry cannot publish a duplicate image or overwrite a concurrent edit.
- [ ] Apply failure after successful save reports the partial success accurately; the Profile remains available.
- [ ] Accepted, failed, and unavailable reload outcomes have the messages above and accessible technical details.
- [ ] The same behavior works standalone and embedded in the management TUI, including compact layouts and terminal restoration.
- [ ] Existing line-based creation aliases/defaults and immediate field edits remain covered by regressions.

## Testing decisions

Use the existing workflow/editor seam for filesystem and Activation assertions, then exercise the real standalone and embedded forms through disposable-HOME PTYs. Observe persisted state and reload calls, not internal widget layout alone.

Prior art: `tests/profile_workflow.rs`, `tests/profile_editor.rs`, `tests/preview_session.rs`, `tests/create_command.rs`, `tests/edit_command.rs`, `tests/edit_pty.py`, and `tests/qa_management_pty.py`. Extend those tests rather than introducing a new harness. Include existing fault-injection and concurrent-edit coverage because the save seam has **HIGH** graph risk.

Focused starting commands; extend their assertions to cover this spec before declaring completion:

```bash
cargo test --locked --test profile_workflow --test profile_editor --test preview_session --test create_command --test edit_command
cargo build --locked
python3 -m unittest discover -s tests -p edit_pty.py -v
python3 -m unittest discover -s tests -p qa_management_pty.py -v
```

Update CLI help, advanced usage, and corresponding website/editor content with the actual key bindings and outcomes. Do not alter machine-readable plan output.

## Out of scope and stop condition

No new Profile identifier model, palette algorithm, persistent preview cache, live reload preview, or combined save/apply transaction. Stop when all completion paths and failure distinctions above are proven and documented; do not redesign the rest of the browser.
