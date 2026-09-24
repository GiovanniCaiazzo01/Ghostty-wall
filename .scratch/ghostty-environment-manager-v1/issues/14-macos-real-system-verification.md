# 14: Verify experimental macOS support on a real system

**What to verify:** Exercise v1 Managed Root, Ghostty Integration Hook, apply/reload, and safe lifecycle on a real macOS installation. This is human-run evidence, not a blocker for Linux-stable v1; macOS remains experimental until verified.

**Blocked by:** 04 — Managed Root initialization; 11 — macOS reload adapter. Run after the relevant CLI is available.

**Status:** ready-for-human

- [ ] Record macOS and Ghostty versions, installation method, and tested Ghostty configuration locations.
- [ ] In an isolated user/config setup, verify `init --dry-run` writes nothing; real `init` publishes layout under `$HOME/Library/Application Support/com.mitchellh.ghostty/ghostty-wall` and installs one effective optional include.
- [ ] Verify Ghostty root config precedence across XDG and Application Support; preserve root-config symlinks; confirm `init` no-op, `init --repair`, and safe failure behavior.
- [ ] Verify `plan` is read-only; `apply`, `previous`, Projection regeneration, and experimental AppleScript reload with Ghostty running and unavailable.
- [ ] Record actual Ghostty `+show-config` result and reproducible commands/logs, redacting user paths or credentials as needed. Record unavailable probes explicitly; do not claim success from mocks.
- [ ] Document defects as new linked issues; update platform support claim only after evidence passes.

## Comments

- Requires access to a macOS host with Ghostty. Linux CI and recording-adapter tests cannot satisfy this issue.
