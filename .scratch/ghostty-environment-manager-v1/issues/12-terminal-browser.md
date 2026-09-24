# 12: Browse and apply Profiles in the terminal

**What to build:** Let users browse Sources, preview wallpapers and resolved colors, inspect contrast, and apply Profiles through a terminal UI backed only by existing application services.

**Blocked by:** 06 — Apply and replay local Environments; 08 — Resolve GitHub wallpaper Sources; 09 — Materialize named Ghostty themes; 10 — Generate managed colors from wallpaper; 11 — Integrate supported Ghostty runtime adapters.

**Status:** ready-for-agent

- [ ] TUI contains no independent resolution or persistence logic.
- [ ] Preview uses Ghostty-compatible terminal image rendering with graceful unsupported fallback.
- [ ] Cancel leaves durable and derived state unchanged.
- [ ] Apply invokes the same Plan/apply path as CLI commands.
- [ ] State-machine tests cover navigation and actions; visual snapshots remain minimal.
