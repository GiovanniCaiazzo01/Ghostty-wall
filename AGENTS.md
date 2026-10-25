# Agent Guide

## Documentation

- Read [CONTEXT.md](./CONTEXT.md) when changing domain language or model boundaries.
- For user-facing behavior changes, update relevant [README](./README.md), examples, CLI help and [website content](./website/README.md).
- When changing persisted formats or identifiers, preserve existing compatibility and add regression coverage in `tests/`.
- Public work is tracked in [GitHub issues](https://github.com/GiovanniCaiazzo01/Ghostty-wall/issues). `.scratch/` is ignored local working material, not published documentation.
- Documentation-only changes do not require a version bump or release tag. Pages deploys independently; its version label follows `Cargo.toml`.
