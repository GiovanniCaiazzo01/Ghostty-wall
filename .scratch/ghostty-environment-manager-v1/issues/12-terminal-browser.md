# 12: Browse and apply Profiles in the terminal

**What to build:** Let users browse Sources, preview wallpapers and resolved colors, inspect contrast, and apply Profiles through a terminal UI backed only by existing application services.

**Blocked by:** 06 — Apply and replay local Environments; 08 — Resolve GitHub wallpaper Sources; 09 — Materialize named Ghostty themes; 10 — Generate managed colors from wallpaper; 11 — Integrate supported Ghostty runtime adapters.

**Status:** done

- [x] TUI contains no independent resolution or persistence logic.
- [x] Preview uses Ghostty-compatible terminal image rendering with graceful unsupported fallback.
- [x] Cancel leaves durable and derived state unchanged.
- [x] Apply invokes the same Plan/apply path as CLI commands.
- [x] State-machine tests cover navigation and actions; visual snapshots remain minimal.

## Comments

- Added a terminal browser state machine over a narrow application-service boundary; preview delegates to Plan and apply delegates to the normal durable apply service.
- Added resolved Source, Candidate, colors, WCAG contrast presentation, and bounded PNG/JPEG wallpaper previews through Ghostty's Kitty graphics protocol.
- Unsupported terminals receive a text fallback. Recording-service tests prove cancel never invokes apply.
