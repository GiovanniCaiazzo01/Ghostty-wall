# Ghostty Environment Manager v1

**Status:** ready-for-agent

## Problem Statement

Ghostty Wall is currently a Bash utility that chooses a random wallpaper from configured GitHub repositories, rewrites a small Ghostty include, and attempts a reload. It cannot describe reproducible visual environments, replay previous states, generate matching colors, validate durable state, or safely evolve its configuration.

Users need a reliable Ghostty companion that manages only its own visual layer, preserves user configuration, explains every planned change, and can recover its derived Projection without pretending corrupt durable state is healthy.

## Solution

Build Ghostty Wall v1 as a single Rust crate and visual Environment manager.

Users define Wallpaper Sources and named Profiles. Ghostty Wall resolves a Profile into a deterministic Plan, acquires and validates one wallpaper, materializes an immutable content-addressed Environment, commits an Activation to append-only local History, projects the active Environment into one managed Ghostty include, and attempts runtime reload separately.

The implementation follows accepted RFC 0001–0008. Linux is stable in v1; macOS remains experimental until it has repeatable real-system verification.

## User Stories

1. As a Ghostty user, I want initialization to create a complete managed layout, so that later commands can rely on explicit filesystem invariants.
2. As a Ghostty user, I want initialization to add exactly one semantic include to my effective Ghostty config, so that Ghostty Wall controls only its own Projection.
3. As a cautious user, I want an init dry run, so that I can inspect every intended mutation before files change.
4. As a user with a damaged installation, I want explicit conservative repair, so that reconstructible structure is restored without hiding possible data loss.
5. As a v0 user, I want an explicit idempotent legacy migration, so that existing GitHub Sources move to v1 without deleting legacy files.
6. As a user with dotfile-managed Ghostty config, I want root-config symlinks preserved, so that initialization does not break my dotfile workflow.
7. As a user, I want Source and Profile identifiers validated once, so that invalid names cannot leak into later operations.
8. As a user, I want local-directory Sources, so that I can manage wallpapers without network access.
9. As a user, I want GitHub repository Sources pinned to one resolved commit per invocation, so that one resolution cannot mix branch revisions.
10. As a user, I want recursive Candidate discovery with deterministic membership and ordering, so that seeded random selection is reproducible.
11. As a user, I want invalid image bytes rejected after selection, so that extension alone never creates an unusable Environment.
12. As a user, I want a Profile to leave wallpaper unmanaged, explicitly disable it, or resolve it from a Source, so that ownership is unambiguous.
13. As a user, I want Profiles to select a Candidate by path or deterministic random Selection, so that both fixed and rotating environments are expressible.
14. As an automation author, I want random planning to require an explicit Resolution Seed, so that Plan output is reproducible.
15. As an automation author, I want stable Plan JSON, so that integrations can inspect Profile, Source, Selection, Asset, Environment, Operations, and Diagnostics.
16. As an automation author, I want structured Error Responses and stable exit categories, so that scripts never parse human prose.
17. As a user, I want explicit colors in a Profile, so that a completely local vertical slice works before theme or generation adapters exist.
18. As a user, I want a named Ghostty theme resolved into managed colors, so that replay does not depend on the theme remaining installed.
19. As a user, I want matching colors generated from the selected wallpaper, so that wallpaper-to-theme is the product's distinguishing feature.
20. As a user, I want only the supported visual Ghostty subset managed, so that Ghostty Wall never becomes a general configuration passthrough.
21. As a user, I want Environment identity derived from canonical managed content, so that semantically equal results deduplicate across Profiles and algorithms.
22. As a user, I want wallpaper Assets retained by content digest, so that History replay survives Source deletion or network loss.
23. As a user, I want applying the same Environment twice to create two Activations, so that History records user actions rather than distinct states.
24. As a user, I want `previous` to navigate logical History without bouncing through replay events, so that repeated backward navigation behaves predictably.
25. As a user, I want current Ghostty configuration regenerated from the latest committed Activation, so that crashes cannot make derived Projection authoritative.
26. As a user, I want committed durable corruption to stop recovery, so that the tool never fabricates replacement history.
27. As a user, I want Projection drift reported during planning and repaired only by mutating commands, so that Plan remains read-only.
28. As a user, I want apply to serialize writers while leaving network and image work outside the lock, so that commits are safe without unnecessary lock duration.
29. As a user, I want runtime reload failure separated from durable activation success, so that a valid applied Environment is not rolled back because Ghostty is not running.
30. As a user, I want `doctor` to distinguish verified, failed, and unavailable checks, so that integration problems are actionable.
31. As a Linux user, I want documented systemd reload support, so that active Ghostty sessions update through the supported interface.
32. As a macOS user, I want experimental AppleScript integration clearly labeled, so that support claims match available verification.
33. As a terminal user, I want an interactive TUI over the same core services, so that browsing and preview do not introduce separate business logic.
34. As a user, I want safe uninstall to remove integration and disposable state while preserving Intent and History, so that uninstall cannot destroy recoverable environments.
35. As a maintainer, I want accepted RFC test vectors executable in Rust tests, so that protocol compatibility survives refactors.
36. As a maintainer, I want domain invariants represented by validated types, so that invalid primitives cannot circulate through the core.
37. As a maintainer, I want filesystem durability primitives tested against real temporary filesystems, so that mocks do not hide rename, fsync, or locking failures.
38. As a maintainer, I want the Bash v0 regression suite to remain green during the rewrite, so that the existing release remains usable until v1 replaces it.

## Implementation Decisions

- Use one Rust crate. Add modules only when their contracts are implemented; do not pre-create networking, theme, palette, or TUI subsystems.
- Treat RFC 0001–0008 as normative. Change an RFC before intentionally changing its contract.
- Keep domain types independent of filesystem, network, clock, entropy, and Ghostty adapters.
- Decode persisted or user-authored formats into raw representations, then perform semantic validation into domain types.
- Keep canonicalization, hashing, fixed-point conversion, and versioned identifiers implementation-independent.
- Keep Plan informational and non-executable. Apply always creates and revalidates its own Plan.
- Keep Environment semantic content separate from Activation provenance.
- Keep Recovery Inspection read-only and Reconciliation limited to derived Projection.
- Use one platform-specific Managed Root with explicit ownership, no-follow, permission, locking, fsync, and atomic-publication guarantees.
- Preserve the Bash implementation until the Rust CLI reaches an accepted replacement milestone.
- Implement the local-directory plus explicit-colors path before GitHub, named themes, generated colors, macOS, or TUI.
- Keep direct `random` and `set` commands outside v1; Profiles express random and path selection.
- Keep successful apply runtime outcome separate from Plan JSON and Error Response until its own contract is needed.

## Testing Decisions

- Primary acceptance seam: invoke the public CLI against temporary home/config roots and observe JSON, exit status, managed files, History, and Projection.
- Protocol seam: exercise public codec/domain interfaces with accepted RFC test vectors for canonical JSON, identifiers, Candidate Sets, and `random-v1`.
- Storage seam: run filesystem integration tests using real temporary directories and files; use adapters only for platform facilities unavailable in test.
- Adapter seam: use recording adapters for network, clock, entropy, reload, and platform integration while keeping core resolution real.
- Real integration seam: run live GitHub and real Ghostty probes separately; report unavailable environments instead of replacing them with fake success.
- Tests assert behavioral contracts and semantic state, not private functions or giant snapshots.
- Every corruption class has a fixture and proves fail-closed behavior.
- Every atomic mutation test proves failure preservation and idempotence.
- TUI tests target state transitions; visual snapshots stay few and stable.
- Continue running the Bash v0 suite until replacement is explicit.

## Out of Scope

- Windows support.
- Custom shaders.
- Scheduler and filesystem watching.
- Automatic light/dark switching.
- Remote Profile sharing or marketplace.
- Arbitrary Ghostty configuration passthrough.
- Direct `random` and `set` commands.
- Executable or persisted Plans.
- History sync or merge across machines.
- History pruning, compaction, Asset garbage collection, or destructive purge.
- Silent schema migration, repair, clamping, or durable-state reconstruction.
- General plugin architecture.

## Further Notes

- The current working tree is authoritative; the GitHub repository remains the earlier baseline until an explicit commit/push request.
- RFC 0001 domain types and Environment Manifest codec are already implemented with passing compatibility tests.
- The next unimplemented protocol slice is Candidate Set identity plus `random-v1`.
