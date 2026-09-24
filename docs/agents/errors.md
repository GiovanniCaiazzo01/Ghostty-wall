# Error Handling

## Diagnostics

Every user-facing failure states:

1. what failed;
2. the relevant Profile, Source, Environment, or path;
3. whether files changed;
4. the next safe action when one exists.

Distinguish invalid Intent, unavailable integration, transient external failure, and durable-state corruption. `doctor` reports `verified`, `failed`, and `unavailable` separately.

## Safety

- Validate before mutation.
- Preserve the prior visible state when an operation fails.
- Treat digest, filename, or declared-ID disagreement as corruption.
- Require an explicit repair or migration command for durable data changes.
- Preserve source errors; add domain context without replacing the cause.

Never silently clamp, migrate, rewrite, rename, or repair persisted data.
