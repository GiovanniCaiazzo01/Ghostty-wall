# Release Checklist

## Automated gates

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
bash scripts/test.sh
shellcheck -x bin/ghostty-wall scripts/install.sh scripts/install-v1.sh scripts/mac/install-mac.sh scripts/linux/install-linux.sh scripts/uninstall.sh scripts/test.sh scripts/integration-test.sh
```

Run live GitHub integration when network and token are available:

```bash
cargo test --test github_live -- --ignored
bash scripts/integration-test.sh
```

Confirm CI and Integration workflows pass exact commit to tag.

## Linux v1 smoke — stable

1. Build `cargo build --locked --release` on clean x86_64 GNU/Linux runner.
2. Run `GHOSTTY_WALL_BINARY="$PWD/target/release/ghostty-wall" INSTALL_PREFIX="$tmp/prefix" ./scripts/install-v1.sh`.
3. Confirm installed binary reports `ghostty-wall 1.0.1`.
4. With temporary `HOME` and `XDG_CONFIG_HOME`, run `init --dry-run`, `init`, and `doctor`.
5. Create local Source plus generated-colors Profile; run `plan PROFILE --json` and `apply PROFILE`.
6. Apply second time, run `previous`, and verify contiguous Activation records plus `current.ghostty`.
7. If Ghostty systemd user service is active, confirm `systemctl --user reload app-com.mitchellh.ghostty.service` succeeds.
8. Otherwise confirm apply reports reload unavailable while Activation remains committed.
9. Run `ghostty-wall uninstall`; confirm hook, Projection, and cache removed while Intent and History remain.
10. Remove installer-owned binary manually.

`tests/cli_release.rs` automates steps 2–6 against temporary roots. Runtime adapter tests record exact systemd command behavior.

## macOS v1 smoke — experimental

Automated evidence: `tests/runtime_reload.rs` verifies AppleScript running, unavailable, probe-failure, and reload-failure classifications. Apply tests verify reload failure does not roll back durable Activation.

Real-system procedure, still required before promotion from experimental:

1. Build and run `./scripts/install-v1.sh`; confirm experimental warning.
2. Run `ghostty-wall init` and inspect Application Support Managed Root plus effective Ghostty root config hook.
3. Launch Ghostty and run `ghostty-wall apply PROFILE`; confirm Activation and `current.ghostty` commit before reload.
4. Confirm Ghostty updates through `perform action "reload_config"`.
5. Quit Ghostty and apply again; confirm reload unavailable while Activation remains committed.
6. Deny Automation on disposable setup and apply; confirm reload failed while Activation remains committed.
7. Run migration dry-run and safe uninstall; verify legacy files, Intent, and History remain.
8. Record OS version, Ghostty version, install source, and outcomes in issue 14.

Do not call macOS stable until issue 14 has repeatable real-system evidence.

## Artifact verification

1. Tag exactly `v1.0.1` after all gates pass.
2. Release workflow must publish:
   - `ghostty-wall-v1.0.1-x86_64-unknown-linux-gnu.tar.gz`;
   - matching `.sha256` file.
3. Download both from GitHub Release and verify `sha256sum --check`.
4. Extract archive and run bundled `./install-v1.sh` in clean temporary home.
5. Verify README migration and uninstall commands against artifact.
6. Confirm Bash v0 remains reachable at tag `v0.2.2`.
7. Reconcile release notes with `CHANGELOG.md`; publish only after artifact smoke passes.
