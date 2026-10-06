# Spec 04: assess readability on the composed wallpaper

- Branch: `feat/spec-04-wallpaper-readability`
- Depends on: [01](01-editor-actions.md) and [03](03-preview-apply.md) for draft acceptance and prepared-result consistency
- Baseline: v1.2.0 at `3ee4f21`
- Non-goals and shared constraints: [spec index](README.md)

## Problem statement

Generated colors are corrected against a uniform background, without wallpaper opacity. A color that passes the generator's contrast check can be difficult to read after composition. Users need an honest assessment of the combined result and a small, explicit adjustment they can preview.

This first increment adds **model-based assessment and an opt-in opacity suggestion**, not a new palette generator or a guarantee about every real Ghostty window.

## Current baseline

- `src/palette.rs`: `readable` checks a 4.5:1 ratio against the generated solid background. `generate_kmeans_v3` does not receive wallpaper opacity.
- `src/tui/sample/graphics.rs`: the sample already blends linear-light sRGB using wallpaper opacity multiplied by PNG pixel alpha.
- `src/profile_editor.rs`: opacity can already be changed in the draft while customized colors remain distinct from automatic colors.
- `tests/generated_palette.rs`: includes a white image and expects background `555555` plus versioned deterministic colors.

Reference case, with an opaque solid background and white image:

| Model inputs | Contrast of text `#c9c9c9` |
| --- | --- |
| Background `#555555`, image opacity 0 | Approximately 4.502:1 |
| Same background, white wallpaper at opacity 0.05, linear-light sRGB | Approximately 3.404:1 |

These are mathematical model values, not measurements of a live Ghostty window.

## Solution and user stories

Assess the same prepared content shown in the simulated preview. Report which managed text roles are at risk, and let the user accept a suggested lower wallpaper opacity before saving or applying.

1. As a user importing a bright photograph, I want warnings to account for the photograph and its chosen opacity.
2. As a user looking at a dark or monochrome image, I want the same assessment model rather than a brightness heuristic.
3. As a user finding an unreadable result, I want an opacity proposal I can preview before accepting.
4. As a user with customized colors, I do not want them overwritten automatically.
5. As a user inheriting unknown Ghostty settings, I want an unavailable/qualified assessment rather than invented certainty.
6. As a user who prefers a deliberately low-contrast look, I want to save it without a forced correction.
7. As a user comparing before and after, I want ordinary terminal content, not only colored bars.

## Implementation decisions

- Keep all existing palette algorithms, their output bytes, and provenance identifiers unchanged. The palette seam has **CRITICAL** graph risk across creation, planning, and local/GitHub apply; assessment is not permission to change it in place.
- Reuse the existing linear-light sRGB composition model and managed opacity, including PNG pixel alpha. Assess an opaque solid background plus the image; do not silently fold desktop transparency into this model.
- Evaluate managed foreground and each managed ANSI slot against a conservative composed-background luminance range, including the solid background for potentially uncovered areas. Do not report contrast against an average image pixel as a worst-case result.
- Use the bounded decoded source image, not a small thumbnail that can erase bright/dark regions. The calculation must conservatively cover possible image regions and interpolation; it need not predict real window geometry or glyph placement.
- Use 4.5:1 as the model target for those text roles. Report the lowest ratio and affected role names. Assess explicit managed selection foreground/background separately; opacity cannot fix their mutual contrast. Cursor and dim-text appearance are represented but are not covered by a blanket 4.5:1 guarantee.
- If required colors, wallpaper opacity, or effective background behavior are unmanaged/inherited, mark the relevant assessment unavailable or conditional. Illustrative preview defaults are not evidence of the user's effective configuration. Managed terminal transparency below 1 also makes the opaque-background result conditional.
- Offer an explicit **Suggest opacity** action in the visual editor. Suggest a deterministic lower opacity, representable in the existing field, that reaches the target for the evaluated wallpaper-backed text roles under this model. Show old/new strength and before/after assessment.
- If no lower opacity can meet the target, including opacity 0, explain that colors also need editing. Do not invent a successful adjustment, rewrite customized colors, or present selection issues as solved by reducing wallpaper strength.
- Accepting a proposal changes only the in-memory wallpaper opacity and refreshes the prepared sample. Declining changes nothing. Spec 01 still controls Save / Save and use / Cancel; warnings are advisory, not a new ban on deliberate manual colors.
- Reuse the shared sample to show normal text, dim text, success/error lines, a short diff, selection, and cursor. Clearly distinguish represented appearance from the roles actually assessed. Keep one simulated-preview label with model limitations available in details.
- Run assessment with the existing asynchronous preview work and resource limits. Inspecting or suggesting must not write a Profile, publish assets, append History, or request reload.
- Do not add a new dependency, persisted diagnostic, or machine-readable Plan field merely to expose this UI assessment. A future generator that incorporates opacity requires its own versioned design and compatibility tests.

## Acceptance criteria

- [ ] The reference case above reproduces 4.502:1 and 3.404:1 within 0.01 and flags the latter below target.
- [ ] Opacity 0 reduces the wallpaper-backed assessment to the solid-background case; opacity 1 and PNG alpha are covered by reference tests.
- [ ] Bright, dark, monochrome, and mixed-luminance images are assessed with image opacity; a small high-risk region cannot disappear through thumbnail averaging.
- [ ] All managed foreground/ANSI roles are reported individually, without conflating normal/bright slots or recoloring them.
- [ ] A suggested opacity is representable, does not exceed the current value, and meets the text-role target when the model says a solution exists.
- [ ] If no opacity-only solution exists, the UI says so and preserves all colors and settings.
- [ ] Accepting the proposal updates the draft and sample only; decline/Cancel leave saved and durable state unchanged.
- [ ] Unknown inherited values and desktop transparency cannot produce an unqualified readability claim.
- [ ] Selection diagnostics are separate; cursor/dim samples do not imply that their real rendering passed an unavailable check.
- [ ] The final applied content equals the approved prepared result from Spec 03, including any accepted opacity change.
- [ ] Existing versioned palette fixtures, Environment identity fixtures, and historical replay remain byte-compatible.
- [ ] Invalid or oversized images fail within existing resource bounds; navigation and terminal restoration remain responsive.

## Testing decisions

Use deterministic numerical cases and tiny synthetic PNGs for opaque/transparent, monochrome, bright, dark, and mixed regions. Verify the composed luminance/contrast and every proposed opacity against the target. Add one integration path through draft suggestion, Save and use, and captured-result apply; assert the committed opacity and unchanged manual colors.

Prior art: `tests/generated_palette.rs`, `tests/profile_editor.rs`, `tests/intent_extensions.rs`, sample compositing tests, and `tests/responsive_preview_pty.py`. Reuse the installed image library and existing test harnesses. Preserve the old palette fixtures as compatibility evidence; do not update them to hide a changed generator.

Focused starting commands; new assessment and proposal assertions are required in addition to these baseline suites:

```bash
cargo test --locked --test generated_palette --test profile_editor --test intent_extensions
cargo test --locked --lib tui::sample
cargo build --locked
python3 -m unittest discover -s tests -p responsive_preview_pty.py -v
```

Document the opaque-background/linear-light model, unavailable/conditional cases, advisory target, and accepted adjustment in advanced usage and website color/preview reference. Do not advertise WCAG compliance or measured real-window readability.

## Out of scope and stop condition

No Focus/Balanced/Expressive variants, light/dark generation, new perceptual color space, automatic recoloring, blur/font/desktop simulation, or live-preview integration. Stop when users can assess the composed modeled result, preview an explicit opacity-only adjustment, and retain full control with honest limitations.
