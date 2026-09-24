use std::{
    fs,
    path::PathBuf,
    str::FromStr,
    sync::atomic::{AtomicU64, Ordering},
};

use ghostty_wall::{
    apply::{ApplyError, apply_local_profile, previous},
    codec::intent::{parse_config_toml, parse_profile_toml},
    domain::IntentId,
    history::{HistoryError, inspect_history},
    plan::ReloadUnavailableReason,
    recovery::{ProjectionState, inspect_recovery_state, reconcile_recovery_state},
    runtime::{ReloadFailure, ReloadOutcome, UnavailableReload},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Root {
    base: PathBuf,
    managed: PathBuf,
    root_config: PathBuf,
}

impl Root {
    fn new() -> Self {
        let base = std::env::temp_dir().join(format!(
            "ghostty-wall-apply-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let managed = base.join("ghostty-wall");
        fs::create_dir_all(managed.join("history/activations")).unwrap();
        fs::create_dir(managed.join("environments")).unwrap();
        fs::create_dir_all(managed.join("assets/sha256")).unwrap();
        fs::write(managed.join("state.lock"), []).unwrap();
        let root_config = base.join("config.ghostty");
        fs::write(
            &root_config,
            format!(
                "config-file = ?{}\n",
                managed.join("current.ghostty").display()
            ),
        )
        .unwrap();
        Self {
            base,
            managed,
            root_config,
        }
    }

    fn apply_profile(&self, profile_id: &str, profile_toml: &str, timestamp: &str) {
        let config = parse_config_toml("schema_version = 1\n[sources]\n").unwrap();
        let profile = parse_profile_toml(profile_toml).unwrap();
        let outcome = apply_local_profile(
            &self.base,
            &self.base,
            &self.managed,
            &self.root_config,
            &IntentId::from_str(profile_id).unwrap(),
            &config,
            &profile,
            None,
            timestamp,
            UnavailableReload::new(ReloadUnavailableReason::GhosttyIntegrationUnavailable),
        )
        .unwrap();
        assert_eq!(
            outcome.reload_outcome(),
            ReloadOutcome::Unavailable(ReloadUnavailableReason::GhosttyIntegrationUnavailable)
        );
    }
}

impl Drop for Root {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.base).unwrap();
    }
}

#[test]
fn apply_commits_each_profile_event_before_best_effort_reload() {
    let root = Root::new();
    root.apply_profile("one", "schema_version = 1\n", "2026-09-23T08:31:15.123456Z");
    root.apply_profile("one", "schema_version = 1\n", "2026-09-23T08:31:16.123456Z");

    let history = inspect_history(&root.managed).unwrap();
    assert_eq!(history.latest().unwrap().sequence(), 2);
    assert_eq!(history.latest().unwrap().history_cursor(), 2);
    assert!(root.managed.join("current.ghostty").is_file());
    assert_eq!(
        inspect_recovery_state(&root.managed, &root.root_config)
            .unwrap()
            .projection(),
        ProjectionState::Consistent
    );
}

#[test]
fn failed_reload_is_reported_after_activation_commit() {
    let root = Root::new();
    let config = parse_config_toml("schema_version = 1\n[sources]\n").unwrap();
    let profile = parse_profile_toml("schema_version = 1\n").unwrap();

    let outcome = apply_local_profile(
        &root.base,
        &root.base,
        &root.managed,
        &root.root_config,
        &IntentId::from_str("one").unwrap(),
        &config,
        &profile,
        None,
        "2026-09-23T08:31:15.123456Z",
        || Err::<(), _>("reload failed"),
    )
    .unwrap();

    assert_eq!(
        outcome.reload_outcome(),
        ReloadOutcome::Failed(ReloadFailure::Reload)
    );
    assert_eq!(
        inspect_history(&root.managed)
            .unwrap()
            .latest()
            .unwrap()
            .sequence(),
        1
    );
}

#[test]
fn previous_replays_cursor_events_without_profile_or_source_resolution() {
    let root = Root::new();
    root.apply_profile("one", "schema_version = 1\n", "2026-09-23T08:31:15.123456Z");
    root.apply_profile(
        "two",
        "schema_version = 1\n[wallpaper]\nmode = \"none\"\n",
        "2026-09-23T08:31:16.123456Z",
    );
    root.apply_profile(
        "three",
        "schema_version = 1\n",
        "2026-09-23T08:31:17.123456Z",
    );

    previous(
        &root.managed,
        &root.root_config,
        "2026-09-23T08:31:18.123456Z",
        || Ok::<(), ()>(()),
    )
    .unwrap();
    let history = inspect_history(&root.managed).unwrap();
    assert_eq!(history.latest().unwrap().sequence(), 4);
    assert_eq!(history.latest().unwrap().history_cursor(), 2);

    previous(
        &root.managed,
        &root.root_config,
        "2026-09-23T08:31:19.123456Z",
        || Ok::<(), ()>(()),
    )
    .unwrap();
    let history = inspect_history(&root.managed).unwrap();
    assert_eq!(history.latest().unwrap().sequence(), 5);
    assert_eq!(history.latest().unwrap().history_cursor(), 1);
    fs::write(
        root.managed.join("current.ghostty"),
        "foreground = ffffff\n",
    )
    .unwrap();
    assert!(
        previous(
            &root.managed,
            &root.root_config,
            "2026-09-23T08:31:20.123456Z",
            || Ok::<(), ()>(())
        )
        .is_err()
    );
    assert_eq!(
        inspect_recovery_state(&root.managed, &root.root_config)
            .unwrap()
            .projection(),
        ProjectionState::Consistent
    );
}

#[test]
fn previous_uses_durable_asset_after_local_source_is_deleted() {
    let root = Root::new();
    let source = root.base.join("wallpapers");
    fs::create_dir(&source).unwrap();
    fs::write(
        source.join("white.png"),
        include_bytes!("fixtures/white.png"),
    )
    .unwrap();
    let config = parse_config_toml(&format!(
        "schema_version = 1\n[sources.local]\nkind = \"local-directory\"\npath = \"{}\"\n",
        source.display()
    ))
    .unwrap();
    let profile = parse_profile_toml(
        "schema_version = 1\n[wallpaper]\nmode = \"source\"\nsource = \"local\"\nselection = \"path\"\npath = \"white.png\"\nopacity = 0.5\n",
    )
    .unwrap();
    apply_local_profile(
        &root.base,
        &root.base,
        &root.managed,
        &root.root_config,
        &IntentId::from_str("image").unwrap(),
        &config,
        &profile,
        None,
        "2026-09-23T08:31:15.123456Z",
        || Ok::<(), ()>(()),
    )
    .unwrap();
    let shard = fs::read_dir(root.managed.join("assets/sha256"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let asset = fs::read_dir(&shard)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let digest = asset.file_stem().unwrap().to_str().unwrap();
    let duplicate = shard.join(format!("{digest}.copy"));
    fs::copy(&asset, &duplicate).unwrap();
    assert!(
        apply_local_profile(
            &root.base,
            &root.base,
            &root.managed,
            &root.root_config,
            &IntentId::from_str("image").unwrap(),
            &config,
            &profile,
            None,
            "2026-09-23T08:31:16.000000Z",
            || Ok::<(), ()>(()),
        )
        .is_err()
    );
    fs::remove_file(duplicate).unwrap();
    let wrong_shard = root.managed.join("assets/sha256/ff");
    fs::create_dir(&wrong_shard).unwrap();
    let duplicate = wrong_shard.join(asset.file_name().unwrap());
    fs::copy(&asset, &duplicate).unwrap();
    assert!(matches!(
        previous(
            &root.managed,
            &root.root_config,
            "2026-09-23T08:31:16.000000Z",
            || Ok::<(), ()>(())
        ),
        Err(ApplyError::History(HistoryError::Corrupt(path))) if path == duplicate
    ));
    fs::remove_dir_all(wrong_shard).unwrap();
    assert_eq!(
        inspect_history(&root.managed)
            .unwrap()
            .latest()
            .unwrap()
            .sequence(),
        1
    );
    root.apply_profile(
        "off",
        "schema_version = 1\n[wallpaper]\nmode = \"none\"\n",
        "2026-09-23T08:31:16.123456Z",
    );
    fs::remove_dir_all(source).unwrap();

    previous(
        &root.managed,
        &root.root_config,
        "2026-09-23T08:31:17.123456Z",
        || Ok::<(), ()>(()),
    )
    .unwrap();

    let projection = fs::read_to_string(root.managed.join("current.ghostty")).unwrap();
    assert!(projection.contains("assets/sha256/"));
    assert!(projection.contains(".png"));
    assert_eq!(
        inspect_recovery_state(&root.managed, &root.root_config)
            .unwrap()
            .projection(),
        ProjectionState::Consistent
    );
    assert_eq!(
        inspect_history(&root.managed)
            .unwrap()
            .latest()
            .unwrap()
            .history_cursor(),
        1
    );
}

#[cfg(unix)]
#[test]
fn reconciliation_replaces_projection_symlink_without_following_it() {
    use std::os::unix::fs::symlink;

    let root = Root::new();
    root.apply_profile("one", "schema_version = 1\n", "2026-09-23T08:31:15.123456Z");
    let external = root.base.join("external");
    fs::write(&external, "untouched").unwrap();
    fs::remove_file(root.managed.join("current.ghostty")).unwrap();
    symlink(&external, root.managed.join("current.ghostty")).unwrap();

    reconcile_recovery_state(&root.managed, &root.root_config).unwrap();

    assert_eq!(fs::read_to_string(external).unwrap(), "untouched");
    assert!(root.managed.join("current.ghostty").is_file());
    assert!(!root.managed.join("current.ghostty").is_symlink());
}

#[test]
fn previous_reconciles_stale_projection_even_when_history_is_empty() {
    let root = Root::new();
    fs::write(
        root.managed.join("current.ghostty"),
        "foreground = ffffff\n",
    )
    .unwrap();

    assert!(
        previous(
            &root.managed,
            &root.root_config,
            "2026-09-23T08:31:15.123456Z",
            || Ok::<(), ()>(())
        )
        .is_err()
    );

    assert!(!root.managed.join("current.ghostty").exists());
}

#[test]
fn reconciliation_only_restores_projection_from_committed_history() {
    let root = Root::new();
    root.apply_profile(
        "wallpaper-off",
        "schema_version = 1\n[wallpaper]\nmode = \"none\"\n",
        "2026-09-23T08:31:15.123456Z",
    );
    let record = root
        .managed
        .join("history/activations/act-v1-0000000000000001.json");
    let before = fs::read(&record).unwrap();
    fs::write(
        root.managed.join("current.ghostty"),
        "foreground = ffffff\n",
    )
    .unwrap();

    reconcile_recovery_state(&root.managed, &root.root_config).unwrap();

    assert_eq!(fs::read(record).unwrap(), before);
    assert_eq!(
        inspect_recovery_state(&root.managed, &root.root_config)
            .unwrap()
            .projection(),
        ProjectionState::Consistent
    );
}
