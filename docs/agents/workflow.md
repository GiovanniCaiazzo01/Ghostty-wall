# Development Workflow

## Before implementation

1. Read `CONTEXT.md` and the governing RFCs or ADRs.
2. Identify the behavioral oracle for the change.
3. Update an RFC or ADR first when the intended behavior changes its contract.

## Checks

For the current Bash v0 implementation:

```sh
bash scripts/test.sh
shellcheck -x bin/ghostty-wall scripts/install.sh scripts/mac/install-mac.sh scripts/linux/install-linux.sh scripts/uninstall.sh scripts/test.sh scripts/integration-test.sh
```

For Rust changes after `Cargo.toml` exists:

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
```

Run the smallest relevant check during development, then every applicable gate before handoff. Live GitHub and real Ghostty probes are integration checks; report when the environment cannot run them.

## Migrations

Exercise dry-run, successful migration, rollback or recovery, unknown future versions, and interrupted writes. Ordinary apply commands must not perform implicit schema migration.
