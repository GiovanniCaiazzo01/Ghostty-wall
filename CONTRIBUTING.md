# Contributing to Ghostty Wall

Bug reports, fixes, documentation and focused improvements are welcome. Start with the [open issues](https://github.com/GiovanniCaiazzo01/Ghostty-wall/issues) or the [current release plan](https://github.com/GiovanniCaiazzo01/Ghostty-wall/issues/7).

## Choose a change

Search existing issues and pull requests first. Small fixes and documentation corrections can go straight to a PR. For substantial features, new commands, persisted-format changes or platform support, open an issue to agree on the scope before implementation.

For a **bug report**, include Ghostty Wall and Ghostty versions, OS, steps to reproduce, expected behavior and actual behavior. Include a minimal configuration or relevant output when useful; remove private paths and terminal content.

For a **feature proposal**, explain the user problem, a concrete example and what a successful result would look like. Check whether somebody is already working on an issue before starting.

## Name your branch and PR

Create a branch from an up-to-date `main`. Contributors without write access should work in a fork and open a PR against this repository's `main`.

Use `<type>/<short-description>`: lowercase words separated by hyphens. An existing issue number is useful but optional, for example `feat/8-source-maintenance` or `docs/clarify-installation`.

| Type | Use |
| --- | --- |
| `feat` | New user-facing behavior |
| `fix` | A bug fix |
| `docs` | Documentation |
| `refactor` | Restructuring without intended behavior changes |
| `test` | Test coverage |
| `chore` | Tooling, dependencies or CI |

For example, from an up-to-date checkout:

```sh
git switch -c feat/8-source-maintenance main
# Make and commit the change.
git push -u origin feat/8-source-maintenance
```

Keep one goal per branch and PR. Existing branches do not need renaming to follow this convention.

PR titles use `<type>: <change>`, with an optional scope: `fix(profiles): preserve custom colors`. Use descriptive commit messages; temporary review commits do not need to follow the PR title format.

## Build and verify

Use Rust **1.88+** with Cargo, rustfmt and Clippy. Python **3.11+** is used for documentation and PTY checks. Run commands from the repository root.

```sh
cargo build --locked --release
./target/release/ghostty-wall --version
```

For Rust changes, match the [CI checks](.github/workflows/ci.yml):

```sh
cargo fmt --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
```

For CLI/TUI changes, also exercise the affected flows and the existing PTY suite:

```sh
GHOSTTY_WALL_BIN="$PWD/target/release/ghostty-wall" python3 -B -m unittest discover -s tests -p '*pty.py' -v
```

Use the built binary and a disposable HOME for checks that change configuration. A passing PTY test or accepted reload request does not prove that a real Ghostty window visibly changed; record real-window observations separately.

For documentation:

```sh
python3 website/generate.py
python3 website/test_site.py
```

Check Markdown rendering and links. If site content changes, include the generated `website/dist/` updates; see [website editing](website/README.md). Markdown-only changes do not need new Rust tests. Shell and installer changes should also run the relevant checks listed in CI.

Add focused regression coverage for behavior changes. Preserve existing Profiles and History when changing persisted data, and update affected CLI help and user guides. Use [CONTEXT.md](CONTEXT.md) for domain terminology; coding agents must also follow [AGENTS.md](AGENTS.md).

## Write the PR description

The [PR template](.github/pull_request_template.md) asks for:

- **Why:** the problem and current behavior.
- **What changes:** the resulting behavior and meaningful compatibility or default changes.
- **Validation:** checks actually run, their results and anything not verified.

Link the relevant issue. Use `Closes #8` only if the PR fully resolves it; use `Related to #8` for partial work. Do not close the release-plan issue through an implementation PR.

Use short before/after output or screenshots when they clarify a visible change. Explain migrations or recovery steps when needed. Understand the changes you submit and be able to explain the evidence, regardless of the tools used.

## Review and release

Review your own diff, then open the PR against `main`. Draft PRs are welcome for implementation feedback. Push follow-up commits to the same branch and address review comments; avoid unrelated cleanup.

Maintainers review the change and CI results before merging. Keep the linked issue and release plan updated: **in progress**, **in review**, **merged** and **released** are separate states. Publishing a release is a separate maintainer decision.
