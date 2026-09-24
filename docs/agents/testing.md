# Testing

Test behavioral contracts, not every function or type.

## Oracles

- Use RFC test vectors for canonicalization, IDs, and Selection Algorithms.
- Exercise CLI behavior against temporary `HOME` and `XDG_CONFIG_HOME` directories.
- Use recording adapters for filesystem plans, systemd, AppleScript, clock, entropy, and network boundaries.
- Verify atomicity, idempotence, replay, corruption detection, and failure preservation.
- Use a real Ghostty `+show-config` probe as an integration oracle when available; report its absence instead of faking success.

## Test shape

- Prefer tests through public domain or application interfaces.
- Use focused fixtures for config precedence, images, themes, and malformed persisted records.
- Use property tests for parsers, canonical forms, fixed-point conversion, and round trips when they expose broad invariants.
- Keep snapshots small and stable. Assert semantic fields instead of entire large JSON documents.
- Test the TUI state machine separately from rendering; keep visual snapshots few.

Every bug fix needs an oracle that fails for the observed behavior and passes for the correction.
