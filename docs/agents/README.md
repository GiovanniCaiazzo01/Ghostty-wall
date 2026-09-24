# Agent Engineering Guide

These documents contain project-specific engineering rules. Read the relevant rows plus every RFC or ADR governing the code being changed.

| When the task changes... | Read |
| --- | --- |
| any production behavior | [Core principles](./principles.md) |
| Rust code or public Rust types | [Rust conventions](./rust.md) |
| code comments or non-obvious local invariants | [Code comments](./comments.md) |
| module boundaries, persistence, resolution, or adapters | [Architecture](./architecture.md) |
| diagnostics or failure behavior | [Error handling](./errors.md) |
| tests, fixtures, or verification | [Testing](./testing.md) |
| caching, image work, networking, or concurrency | [Performance](./performance.md) |
| Cargo dependencies or feature flags | [Dependencies](./dependencies.md) |
| paths, remote input, assets, tokens, or file mutation | [Security](./security.md) |
| local checks, migrations, or release preparation | [Development workflow](./workflow.md) |

Do not copy RFC or ADR requirements into these guides. Link to the normative document.
