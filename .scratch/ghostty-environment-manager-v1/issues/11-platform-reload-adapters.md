# 11: Integrate supported Ghostty runtime adapters

**What to build:** Let durable apply attempt Linux systemd reload and experimental macOS AppleScript reload while reporting unavailable or failed runtime outcomes separately from Activation success.

**Blocked by:** 06 — Apply and replay local Environments.

**Status:** ready-for-agent

- [ ] Linux uses the documented Ghostty user-service reload interface.
- [ ] macOS uses the Ghostty AppleScript API and remains labeled experimental.
- [ ] Recording-adapter tests verify exact actions and failure classification.
- [ ] Unavailable runtime remains non-fatal after durable activation.
- [ ] Real-system smoke procedures exist for every claimed support level.
