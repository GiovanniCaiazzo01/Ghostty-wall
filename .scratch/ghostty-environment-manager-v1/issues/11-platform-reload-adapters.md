# 11: Integrate supported Ghostty runtime adapters

**What to build:** Let durable apply attempt Linux systemd reload and experimental macOS AppleScript reload while reporting unavailable or failed runtime outcomes separately from Activation success.

**Blocked by:** 06 — Apply and replay local Environments.

**Status:** done

- [x] Linux uses the documented Ghostty user-service reload interface.
- [x] macOS uses the Ghostty AppleScript API and remains labeled experimental.
- [x] Recording-adapter tests verify exact actions and failure classification.
- [x] Unavailable runtime remains non-fatal after durable activation.
- [x] Real-system smoke procedures exist for every claimed support level.

## Comments

- Added typed reload outcomes for success, runtime unavailability, probe failure, and reload failure without changing durable Activation success.
- Linux records and executes `systemctl --user is-active --quiet` before the documented user-service reload. Experimental macOS uses Ghostty's `perform action "reload_config"` AppleScript API.
- Added Linux stable and macOS experimental real-system smoke procedures to `docs/release-checklist.md`.
