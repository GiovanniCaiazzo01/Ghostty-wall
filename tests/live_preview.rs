#![cfg(target_os = "linux")]

use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex, mpsc},
    thread,
    time::Duration,
};

use ghostty_wall::{
    apply::apply_local_profile,
    codec::{
        intent::{parse_config_toml, parse_profile_toml},
        manifest,
    },
    domain::IntentId,
    history::inspect_history,
    plan::ReloadUnavailableReason,
    preview::{PreviewError, PreviewSession},
    recovery::{ProjectionState, inspect_recovery_state, reconcile_recovery_state},
    runtime::{ReloadFailure, ReloadOutcome, UnavailableReload},
};

const TIME: &str = "2026-09-23T08:31:15.123456Z";

static SERIAL: Mutex<()> = Mutex::new(());

fn fixture(root: &Path) -> (PathBuf, PathBuf) {
    let managed = root.join("ghostty-wall");
    fs::create_dir_all(managed.join("history/activations")).unwrap();
    fs::create_dir(managed.join("environments")).unwrap();
    fs::create_dir_all(managed.join("assets/sha256")).unwrap();
    fs::write(managed.join("state.lock"), []).unwrap();
    let config = root.join("config.ghostty");
    fs::write(
        &config,
        format!(
            "config-file = ?{}\n",
            managed.join("current.ghostty").display()
        ),
    )
    .unwrap();
    (managed, config)
}

fn apply(root: &Path, managed: &Path, config_path: &Path, id: &str) {
    let config = parse_config_toml("schema_version = 1\n[sources]\n").unwrap();
    let profile = parse_profile_toml("schema_version = 1\n").unwrap();
    apply_local_profile(
        root,
        root,
        managed,
        config_path,
        &id.parse::<IntentId>().unwrap(),
        &config,
        &profile,
        None,
        TIME,
        || Ok::<(), ()>(()),
    )
    .unwrap();
}

fn draft() -> ghostty_wall::domain::EnvironmentManifest {
    manifest::decode(br#"{"schema_version":1,"terminal":{"font_size_millipoints":15000}}"#).unwrap()
}

#[test]
fn cancel_restores_other_active_profile_without_history_events() {
    let _serial = SERIAL.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let (managed, config) = fixture(temp.path());
    apply(temp.path(), &managed, &config, "welcome");
    let committed = fs::read(managed.join("current.ghostty")).unwrap();
    let actions = Arc::new(Mutex::new(Vec::new()));
    let record = actions.clone();
    let mut session = PreviewSession::begin(&managed, &config, move || {
        record.lock().unwrap().push("reload");
        Ok::<(), ()>(())
    })
    .unwrap();
    assert_eq!(session.update(&draft()).unwrap(), ReloadOutcome::Succeeded);
    assert_ne!(
        fs::read(managed.join("current.ghostty")).unwrap(),
        committed
    );
    assert_eq!(session.cancel().unwrap(), ReloadOutcome::Succeeded);
    assert_eq!(
        fs::read(managed.join("current.ghostty")).unwrap(),
        committed
    );
    assert_eq!(actions.lock().unwrap().len(), 2);
    assert!(!managed.join("preview.session").exists());
    assert_eq!(inspect_history(&managed).unwrap().activations().len(), 1);
}

#[test]
fn dropping_editor_restores_projection_and_requests_reload() {
    let _serial = SERIAL.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let (managed, config) = fixture(temp.path());
    apply(temp.path(), &managed, &config, "other");
    let committed = fs::read(managed.join("current.ghostty")).unwrap();
    let calls = Arc::new(Mutex::new(0));
    let recorded = calls.clone();
    {
        let mut session = PreviewSession::begin(&managed, &config, move || {
            *recorded.lock().unwrap() += 1;
            Ok::<(), ()>(())
        })
        .unwrap();
        session.update(&draft()).unwrap();
        assert_ne!(
            fs::read(managed.join("current.ghostty")).unwrap(),
            committed
        );
    }
    assert_eq!(
        fs::read(managed.join("current.ghostty")).unwrap(),
        committed
    );
    assert_eq!(*calls.lock().unwrap(), 2);
    assert!(!managed.join("preview.session").exists());
    assert_eq!(inspect_history(&managed).unwrap().activations().len(), 1);
}

#[test]
fn failed_projection_update_does_not_reload_or_commit_and_cancel_recovers() {
    let _serial = SERIAL.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let (managed, config) = fixture(temp.path());
    apply(temp.path(), &managed, &config, "legacy");
    let committed = fs::read(managed.join("current.ghostty")).unwrap();
    let record =
        fs::read(managed.join("history/activations/act-v1-0000000000000001.json")).unwrap();
    let calls = Arc::new(Mutex::new(0));
    let recorded = calls.clone();
    let mut session = PreviewSession::begin(&managed, &config, move || {
        *recorded.lock().unwrap() += 1;
        Ok::<(), ()>(())
    })
    .unwrap();
    fs::remove_file(managed.join("current.ghostty")).unwrap();
    fs::create_dir(managed.join("current.ghostty")).unwrap();
    fs::write(managed.join("current.ghostty/keep"), "untouched").unwrap();
    assert!(session.update(&draft()).is_err());
    assert_eq!(*calls.lock().unwrap(), 0);
    assert!(managed.join("preview.session").exists());
    assert_eq!(
        fs::read(managed.join("current.ghostty/keep")).unwrap(),
        b"untouched"
    );
    assert_eq!(
        fs::read(managed.join("history/activations/act-v1-0000000000000001.json")).unwrap(),
        record
    );
    fs::remove_file(managed.join("current.ghostty/keep")).unwrap();
    fs::remove_dir(managed.join("current.ghostty")).unwrap();
    assert_eq!(session.cancel().unwrap(), ReloadOutcome::Succeeded);
    assert_eq!(*calls.lock().unwrap(), 1);
    assert!(!managed.join("preview.session").exists());
    assert_eq!(
        fs::read(managed.join("current.ghostty")).unwrap(),
        committed
    );
}

#[test]
fn failed_reload_does_not_commit() {
    let _serial = SERIAL.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let (managed, config) = fixture(temp.path());
    apply(temp.path(), &managed, &config, "welcome");
    let mut session =
        PreviewSession::begin(&managed, &config, || Err::<(), _>("reload failed")).unwrap();
    assert_eq!(
        session.update(&draft()).unwrap(),
        ReloadOutcome::Failed(ReloadFailure::Reload)
    );
    assert_eq!(
        session.finish().unwrap(),
        ReloadOutcome::Failed(ReloadFailure::Reload)
    );
    assert_eq!(inspect_history(&managed).unwrap().activations().len(), 1);
    assert_eq!(
        inspect_recovery_state(&managed, &config)
            .unwrap()
            .projection(),
        ProjectionState::Consistent
    );
}

#[test]
fn unavailable_runtime_is_not_a_successful_preview_or_restoration() {
    let _serial = SERIAL.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let (managed, config) = fixture(temp.path());
    apply(temp.path(), &managed, &config, "other");
    let committed = fs::read(managed.join("current.ghostty")).unwrap();
    let history_file = managed.join("history/activations/act-v1-0000000000000001.json");
    let record = fs::read(&history_file).unwrap();
    let mut session = PreviewSession::begin(
        &managed,
        &config,
        UnavailableReload::new(ReloadUnavailableReason::GhosttyIntegrationUnavailable),
    )
    .unwrap();
    assert_eq!(
        session.update(&draft()).unwrap(),
        ReloadOutcome::Unavailable(ReloadUnavailableReason::GhosttyIntegrationUnavailable)
    );
    assert_ne!(
        fs::read(managed.join("current.ghostty")).unwrap(),
        committed
    );
    assert_eq!(
        session.cancel().unwrap(),
        ReloadOutcome::Unavailable(ReloadUnavailableReason::GhosttyIntegrationUnavailable)
    );
    assert_eq!(
        fs::read(managed.join("current.ghostty")).unwrap(),
        committed
    );
    assert_eq!(fs::read(history_file).unwrap(), record);
    assert!(!managed.join("preview.session").exists());
}

#[test]
fn cancel_reports_failed_restore_reload_without_rewriting_committed_history() {
    let _serial = SERIAL.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let (managed, config) = fixture(temp.path());
    apply(temp.path(), &managed, &config, "welcome");
    let committed = fs::read(managed.join("current.ghostty")).unwrap();
    let history_file = managed.join("history/activations/act-v1-0000000000000001.json");
    let record = fs::read(&history_file).unwrap();
    let mut session = PreviewSession::begin(&managed, &config, || Err::<(), _>("no bus")).unwrap();
    assert_eq!(
        session.update(&draft()).unwrap(),
        ReloadOutcome::Failed(ReloadFailure::Reload)
    );
    assert_eq!(
        session.cancel().unwrap(),
        ReloadOutcome::Failed(ReloadFailure::Reload)
    );
    assert_eq!(
        fs::read(managed.join("current.ghostty")).unwrap(),
        committed
    );
    assert_eq!(fs::read(history_file).unwrap(), record);
    assert!(!managed.join("preview.session").exists());
}

#[test]
fn failed_restoration_retains_marker_and_history_until_recovery() {
    let _serial = SERIAL.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let (managed, config) = fixture(temp.path());
    apply(temp.path(), &managed, &config, "legacy");
    let record_path = managed.join("history/activations/act-v1-0000000000000001.json");
    let record = fs::read(&record_path).unwrap();
    let committed = fs::read(managed.join("current.ghostty")).unwrap();
    let mut session = PreviewSession::begin(&managed, &config, || Ok::<(), ()>(())).unwrap();
    session.update(&draft()).unwrap();
    fs::remove_file(managed.join("current.ghostty")).unwrap();
    fs::create_dir(managed.join("current.ghostty")).unwrap();
    fs::write(managed.join("current.ghostty/obstruction"), b"keep").unwrap();
    assert!(session.cancel().is_err());
    assert!(managed.join("preview.session").exists());
    assert_eq!(fs::read(&record_path).unwrap(), record);
    assert_eq!(
        fs::read(managed.join("current.ghostty/obstruction")).unwrap(),
        b"keep"
    );
    fs::remove_file(managed.join("current.ghostty/obstruction")).unwrap();
    fs::remove_dir(managed.join("current.ghostty")).unwrap();
    reconcile_recovery_state(&managed, &config).unwrap();
    assert!(!managed.join("preview.session").exists());
    assert_eq!(
        fs::read(managed.join("current.ghostty")).unwrap(),
        committed
    );
    assert_eq!(fs::read(record_path).unwrap(), record);
}

#[test]
fn draft_cannot_point_at_unretained_image() {
    let _serial = SERIAL.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let (managed, config) = fixture(temp.path());
    apply(temp.path(), &managed, &config, "welcome");
    let mut session = PreviewSession::begin(&managed, &config, || Ok::<(), ()>(())).unwrap();
    let image = manifest::decode(br#"{"schema_version":1,"wallpaper":{"mode":"image","asset_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","media_type":"image/jpeg"}}"#).unwrap();
    assert!(matches!(
        session.update(&image),
        Err(PreviewError::AssetUnavailable)
    ));
    session.cancel().unwrap();
    assert_eq!(inspect_history(&managed).unwrap().activations().len(), 1);
}

#[test]
fn rejected_second_draft_does_not_reload_or_discard_first_draft_and_cancel_restores_legacy_state() {
    let _serial = SERIAL.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let (managed, config) = fixture(temp.path());
    apply(temp.path(), &managed, &config, "legacy");
    let committed = fs::read(managed.join("current.ghostty")).unwrap();
    let history_file = managed.join("history/activations/act-v1-0000000000000001.json");
    let original_record = fs::read(&history_file).unwrap();
    let calls = Arc::new(Mutex::new(0));
    let recorded = calls.clone();
    let mut session = PreviewSession::begin(&managed, &config, move || {
        *recorded.lock().unwrap() += 1;
        Ok::<(), ()>(())
    })
    .unwrap();
    session.update(&draft()).unwrap();
    let first_draft = fs::read(managed.join("current.ghostty")).unwrap();
    assert_ne!(first_draft, committed);
    let unretained_image = manifest::decode(br#"{"schema_version":1,"wallpaper":{"mode":"image","asset_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","media_type":"image/jpeg"}}"#).unwrap();
    assert!(matches!(
        session.update(&unretained_image),
        Err(PreviewError::AssetUnavailable)
    ));
    assert_eq!(
        fs::read(managed.join("current.ghostty")).unwrap(),
        first_draft
    );
    assert_eq!(*calls.lock().unwrap(), 1);
    assert_eq!(fs::read(&history_file).unwrap(), original_record);
    session.cancel().unwrap();
    assert_eq!(*calls.lock().unwrap(), 2);
    assert_eq!(
        fs::read(managed.join("current.ghostty")).unwrap(),
        committed
    );
    assert_eq!(fs::read(history_file).unwrap(), original_record);
    assert!(!managed.join("preview.session").exists());
}

#[test]
fn marker_symlink_refuses_begin_without_touching_target_or_history() {
    use std::os::unix::fs::symlink;

    let _serial = SERIAL.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let (managed, config) = fixture(temp.path());
    apply(temp.path(), &managed, &config, "legacy");
    let committed = fs::read(managed.join("current.ghostty")).unwrap();
    let history_file = managed.join("history/activations/act-v1-0000000000000001.json");
    let record = fs::read(&history_file).unwrap();
    let outside = temp.path().join("outside");
    fs::write(&outside, b"do not modify").unwrap();
    symlink(&outside, managed.join("preview.session")).unwrap();
    assert!(PreviewSession::begin(&managed, &config, || Ok::<(), ()>(())).is_err());
    assert_eq!(fs::read(outside).unwrap(), b"do not modify");
    assert_eq!(
        fs::read(managed.join("current.ghostty")).unwrap(),
        committed
    );
    assert_eq!(fs::read(history_file).unwrap(), record);
    assert!(
        fs::symlink_metadata(managed.join("preview.session"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[test]
fn confirmation_uses_one_normal_profile_activation_not_one_per_update() {
    let _serial = SERIAL.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let (managed, config) = fixture(temp.path());
    apply(temp.path(), &managed, &config, "welcome");
    let mut session = PreviewSession::begin(&managed, &config, || Ok::<(), ()>(())).unwrap();
    for _ in 0..3 {
        session.update(&draft()).unwrap();
    }
    session.finish().unwrap();
    apply(temp.path(), &managed, &config, "boy");
    assert_eq!(inspect_history(&managed).unwrap().activations().len(), 2);
}

#[test]
fn second_editor_busy_and_apply_waits_then_commits_once() {
    let _serial = SERIAL.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let (managed, config) = fixture(temp.path());
    apply(temp.path(), &managed, &config, "welcome");
    let mut session = PreviewSession::begin(&managed, &config, || Ok::<(), ()>(())).unwrap();
    session.update(&draft()).unwrap();
    assert!(matches!(
        PreviewSession::begin(&managed, &config, || Ok::<(), ()>(())),
        Err(PreviewError::Busy)
    ));
    let (tx, rx) = mpsc::channel();
    let root = temp.path().to_owned();
    let managed_thread = managed.clone();
    let config_thread = config.clone();
    let worker = thread::spawn(move || {
        apply(&root, &managed_thread, &config_thread, "boy");
        tx.send(()).unwrap();
    });
    assert!(rx.recv_timeout(Duration::from_millis(100)).is_err());
    session.cancel().unwrap();
    rx.recv_timeout(Duration::from_secs(5)).unwrap();
    worker.join().unwrap();
    let history = inspect_history(&managed).unwrap();
    assert_eq!(history.activations().len(), 2);
    // The waiting apply is a newer durable choice; cancel must not roll it
    // back to the editor's starting Activation after releasing the lock.
    assert_eq!(history.latest().unwrap().sequence(), 2);
    let projection_after_apply = fs::read(managed.join("current.ghostty")).unwrap();
    reconcile_recovery_state(&managed, &config).unwrap();
    assert_eq!(
        fs::read(managed.join("current.ghostty")).unwrap(),
        projection_after_apply
    );
    assert_eq!(
        inspect_recovery_state(&managed, &config)
            .unwrap()
            .projection(),
        ProjectionState::Consistent
    );
}

#[test]
fn read_only_inspection_waits_for_editor_and_never_observes_draft_as_committed() {
    let _serial = SERIAL.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let (managed, config) = fixture(temp.path());
    apply(temp.path(), &managed, &config, "legacy");
    let committed = fs::read(managed.join("current.ghostty")).unwrap();
    let mut session = PreviewSession::begin(&managed, &config, || Ok::<(), ()>(())).unwrap();
    session.update(&draft()).unwrap();
    assert_ne!(
        fs::read(managed.join("current.ghostty")).unwrap(),
        committed
    );
    let (started_tx, started_rx) = mpsc::channel();
    let (result_tx, result_rx) = mpsc::channel();
    let reader_managed = managed.clone();
    let reader_config = config.clone();
    let reader = thread::spawn(move || {
        started_tx.send(()).unwrap();
        let result = inspect_recovery_state(&reader_managed, &reader_config);
        result_tx.send(result).unwrap();
    });
    started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(result_rx.recv_timeout(Duration::from_millis(100)).is_err());
    assert!(managed.join("preview.session").exists());
    session.cancel().unwrap();
    assert_eq!(
        result_rx
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap()
            .projection(),
        ProjectionState::Consistent
    );
    reader.join().unwrap();
    assert_eq!(
        fs::read(managed.join("current.ghostty")).unwrap(),
        committed
    );
    assert_eq!(inspect_history(&managed).unwrap().activations().len(), 1);
}

#[test]
fn save_and_use_invokes_real_intent_save_before_one_activation() {
    let _serial = SERIAL.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let xdg = temp.path().join("xdg");
    let ghostty = xdg.join("ghostty");
    fs::create_dir_all(&ghostty).unwrap();
    let (managed, config) = fixture(&ghostty);
    fs::create_dir(managed.join("profiles")).unwrap();
    fs::write(
        managed.join("config.toml"),
        "schema_version = 1\n[sources]\n",
    )
    .unwrap();
    let profile_path = managed.join("profiles/other.toml");
    fs::write(&profile_path, "schema_version = 1\n").unwrap();
    apply(temp.path(), &managed, &config, "welcome");
    let before = fs::read(managed.join("current.ghostty")).unwrap();
    let reloads = Arc::new(Mutex::new(0));
    let recorded = reloads.clone();
    let mut session = PreviewSession::begin(&managed, &config, move || {
        *recorded.lock().unwrap() += 1;
        Ok::<(), ()>(())
    })
    .unwrap();
    session.update(&draft()).unwrap();
    // Synchronized History readers wait for the preview lock; check the
    // published records directly while this test owns that lock.
    assert_eq!(
        fs::read_dir(managed.join("history/activations"))
            .unwrap()
            .count(),
        1
    );
    assert_eq!(session.finish().unwrap(), ReloadOutcome::Succeeded);
    assert_eq!(*reloads.lock().unwrap(), 2);
    assert!(!managed.join("preview.session").exists());
    assert_eq!(fs::read(managed.join("current.ghostty")).unwrap(), before);

    // Exercise the public Intent editor, not a synthetic save Result. An unsafe
    // file mode makes the actual atomic Intent writer reject the publication.
    let edit = || {
        Command::new(env!("CARGO_BIN_EXE_ghostty-wall"))
            .args(["edit", "other", "terminal.font_size", "15"])
            .env("HOME", temp.path())
            .env("XDG_CONFIG_HOME", &xdg)
            .output()
            .unwrap()
    };
    fs::set_permissions(&profile_path, fs::Permissions::from_mode(0o622)).unwrap();
    let failed = edit();
    assert!(!failed.status.success(), "{:?}", failed);
    let failure = String::from_utf8_lossy(&failed.stderr);
    assert!(
        failure.contains("managed component has wrong kind"),
        "{failure}"
    );
    assert!(failure.contains("other.toml"), "{failure}");
    assert_eq!(
        fs::read_to_string(&profile_path).unwrap(),
        "schema_version = 1\n"
    );
    assert_eq!(inspect_history(&managed).unwrap().activations().len(), 1);
    fs::set_permissions(&profile_path, fs::Permissions::from_mode(0o600)).unwrap();
    let saved = edit();
    assert!(saved.status.success(), "{:?}", saved);
    let saved_text = fs::read_to_string(&profile_path).unwrap();
    assert!(saved_text.contains("font_size = 15"));
    assert_eq!(inspect_history(&managed).unwrap().activations().len(), 1);
    let config_intent = parse_config_toml("schema_version = 1\n[sources]\n").unwrap();
    let profile = parse_profile_toml(&saved_text).unwrap();
    let outcome = apply_local_profile(
        &managed,
        temp.path(),
        &managed,
        &config,
        &"other".parse::<IntentId>().unwrap(),
        &config_intent,
        &profile,
        None,
        TIME,
        || Err::<(), _>("reload failed"),
    )
    .unwrap();
    assert_eq!(
        outcome.reload_outcome(),
        ReloadOutcome::Failed(ReloadFailure::Reload)
    );
    assert_eq!(inspect_history(&managed).unwrap().activations().len(), 2);
    assert_eq!(
        inspect_recovery_state(&managed, &config)
            .unwrap()
            .projection(),
        ProjectionState::Consistent
    );
}

// Child exits without Drop to simulate SIGKILL after durable Projection publication.
#[test]
fn preview_child() {
    let Ok(root) = std::env::var("GHOSTTY_WALL_PREVIEW_TEST_CHILD") else {
        return;
    };
    let root = Path::new(&root);
    let (managed, config) = (root.join("ghostty-wall"), root.join("config.ghostty"));
    let mut session = PreviewSession::begin(&managed, &config, || Ok::<(), ()>(())).unwrap();
    let stage = std::env::var("GHOSTTY_WALL_PREVIEW_TEST_STAGE");
    if stage.as_deref() != Ok("marker-only") {
        session.update(&draft()).unwrap();
    }
    if stage.as_deref() == Ok("wait-for-signal") {
        fs::write(root.join("preview-ready"), []).unwrap();
        loop {
            thread::sleep(Duration::from_secs(1));
        }
    }
    std::process::exit(0);
}

#[test]
fn sigterm_during_preview_recovers_on_next_writer_without_history_change() {
    let _serial = SERIAL.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let (managed, config) = fixture(temp.path());
    apply(temp.path(), &managed, &config, "other");
    let committed = fs::read(managed.join("current.ghostty")).unwrap();
    let record =
        fs::read(managed.join("history/activations/act-v1-0000000000000001.json")).unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "preview_child"])
        .env("GHOSTTY_WALL_PREVIEW_TEST_CHILD", temp.path())
        .env("GHOSTTY_WALL_PREVIEW_TEST_STAGE", "wait-for-signal")
        .spawn()
        .unwrap();
    for _ in 0..500 {
        if temp.path().join("preview-ready").exists() {
            break;
        }
        if let Some(status) = child.try_wait().unwrap() {
            panic!("preview child exited before readiness: {status}");
        }
        thread::sleep(Duration::from_millis(10));
    }
    if !temp.path().join("preview-ready").exists() {
        child.kill().unwrap();
        child.wait().unwrap();
        panic!("preview child did not reach the draft publish point");
    }
    assert_ne!(
        fs::read(managed.join("current.ghostty")).unwrap(),
        committed
    );
    // SIGTERM has no cleanup guarantee: the marker, not an exit handler,
    // enables the next writer to reconstruct the committed Projection.
    assert_eq!(unsafe { libc::kill(child.id() as i32, libc::SIGTERM) }, 0);
    assert!(!child.wait().unwrap().success());
    assert!(managed.join("preview.session").exists());
    assert_eq!(
        inspect_recovery_state(&managed, &config)
            .unwrap()
            .projection(),
        ProjectionState::OutOfSync
    );
    reconcile_recovery_state(&managed, &config).unwrap();
    assert_eq!(
        fs::read(managed.join("current.ghostty")).unwrap(),
        committed
    );
    assert!(!managed.join("preview.session").exists());
    assert_eq!(
        fs::read(managed.join("history/activations/act-v1-0000000000000001.json")).unwrap(),
        record
    );
}

#[test]
fn interrupted_preview_before_projection_publish_is_recovered() {
    let _serial = SERIAL.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let (managed, config) = fixture(temp.path());
    apply(temp.path(), &managed, &config, "welcome");
    let before = fs::read(managed.join("current.ghostty")).unwrap();
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "preview_child"])
        .env("GHOSTTY_WALL_PREVIEW_TEST_CHILD", temp.path())
        .env("GHOSTTY_WALL_PREVIEW_TEST_STAGE", "marker-only")
        .status()
        .unwrap();
    assert!(status.success());
    assert_eq!(fs::read(managed.join("current.ghostty")).unwrap(), before);
    assert_eq!(
        inspect_recovery_state(&managed, &config)
            .unwrap()
            .projection(),
        ProjectionState::OutOfSync
    );
    reconcile_recovery_state(&managed, &config).unwrap();
    assert!(!managed.join("preview.session").exists());
    assert_eq!(inspect_history(&managed).unwrap().activations().len(), 1);
}

#[test]
fn zz_mismatched_interrupted_marker_must_not_be_silently_recovered() {
    let _serial = SERIAL.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let (managed, config) = fixture(temp.path());
    apply(temp.path(), &managed, &config, "welcome");
    let before = fs::read(managed.join("current.ghostty")).unwrap();
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "preview_child"])
        .env("GHOSTTY_WALL_PREVIEW_TEST_CHILD", temp.path())
        .env("GHOSTTY_WALL_PREVIEW_TEST_STAGE", "marker-only")
        .status()
        .unwrap();
    assert!(status.success());
    let marker_path = managed.join("preview.session");
    let mut marker: serde_json::Value =
        serde_json::from_slice(&fs::read(&marker_path).unwrap()).unwrap();
    marker["sequence"] = serde_json::json!(2);
    let tampered = serde_json::to_vec(&marker).unwrap();
    fs::write(&marker_path, &tampered).unwrap();

    // A syntactically valid marker with a false starting Activation is not
    // evidence of an interrupted editor; recovery must not silently erase it.
    assert!(reconcile_recovery_state(&managed, &config).is_err());
    assert_eq!(fs::read(&marker_path).unwrap(), tampered);
    assert_eq!(fs::read(managed.join("current.ghostty")).unwrap(), before);
    assert_eq!(inspect_history(&managed).unwrap().activations().len(), 1);

    // A wrong Environment ID with the correct sequence is equally invalid.
    marker["sequence"] = serde_json::json!(1);
    marker["environment_id"] = serde_json::json!(format!("env-v1-{}", "0".repeat(64)));
    let tampered = serde_json::to_vec(&marker).unwrap();
    fs::write(&marker_path, &tampered).unwrap();
    assert!(reconcile_recovery_state(&managed, &config).is_err());
    assert!(PreviewSession::begin(&managed, &config, || Ok::<(), ()>(())).is_err());
    assert_eq!(fs::read(&marker_path).unwrap(), tampered);
    assert_eq!(fs::read(managed.join("current.ghostty")).unwrap(), before);
}

#[test]
fn new_editor_recovers_interrupted_legacy_preview_before_publishing_its_own_draft() {
    let _serial = SERIAL.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let (managed, config) = fixture(temp.path());
    apply(temp.path(), &managed, &config, "legacy");
    let committed = fs::read(managed.join("current.ghostty")).unwrap();
    let record_path = managed.join("history/activations/act-v1-0000000000000001.json");
    let record = fs::read(&record_path).unwrap();
    let status = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "preview_child"])
        .env("GHOSTTY_WALL_PREVIEW_TEST_CHILD", temp.path())
        .status()
        .unwrap();
    assert!(status.success());
    let old_marker = fs::read(managed.join("preview.session")).unwrap();
    assert_ne!(
        fs::read(managed.join("current.ghostty")).unwrap(),
        committed
    );

    let mut editor = PreviewSession::begin(&managed, &config, || Ok::<(), ()>(())).unwrap();
    assert_ne!(
        fs::read(managed.join("preview.session")).unwrap(),
        old_marker
    );
    assert_eq!(
        fs::read(managed.join("current.ghostty")).unwrap(),
        committed
    );
    assert_eq!(fs::read(&record_path).unwrap(), record);
    editor.update(&draft()).unwrap();
    editor.cancel().unwrap();
    assert_eq!(
        fs::read(managed.join("current.ghostty")).unwrap(),
        committed
    );
    assert!(!managed.join("preview.session").exists());
    assert_eq!(fs::read(record_path).unwrap(), record);
    assert_eq!(inspect_history(&managed).unwrap().activations().len(), 1);
}

#[test]
fn invalid_interrupted_marker_is_not_discarded() {
    let _serial = SERIAL.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let (managed, config) = fixture(temp.path());
    apply(temp.path(), &managed, &config, "welcome");
    fs::write(managed.join("preview.session"), "invalid").unwrap();
    assert!(reconcile_recovery_state(&managed, &config).is_err());
    assert_eq!(
        fs::read_to_string(managed.join("preview.session")).unwrap(),
        "invalid"
    );
    assert_eq!(inspect_history(&managed).unwrap().activations().len(), 1);
}

#[test]
fn begin_rejects_invalid_committed_history_without_a_marker_or_projection_write() {
    let _serial = SERIAL.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let (managed, config) = fixture(temp.path());
    apply(temp.path(), &managed, &config, "legacy");
    let projection = managed.join("current.ghostty");
    fs::write(&projection, b"# do not overwrite on corruption\n").unwrap();
    let corrupt = managed.join("history/activations/act-v1-0000000000000001.json");
    fs::write(&corrupt, b"not an Activation").unwrap();
    assert!(PreviewSession::begin(&managed, &config, || Ok::<(), ()>(())).is_err());
    assert!(!managed.join("preview.session").exists());
    assert_eq!(
        fs::read(&projection).unwrap(),
        b"# do not overwrite on corruption\n"
    );
    assert_eq!(fs::read(&corrupt).unwrap(), b"not an Activation");
}

#[test]
fn interrupted_empty_history_recovers_to_absent_projection_without_activation() {
    let _serial = SERIAL.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let (managed, config) = fixture(temp.path());
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "preview_child"])
        .env("GHOSTTY_WALL_PREVIEW_TEST_CHILD", temp.path())
        .status()
        .unwrap();
    assert!(status.success());
    assert!(managed.join("preview.session").exists());
    assert_eq!(
        inspect_recovery_state(&managed, &config)
            .unwrap()
            .projection(),
        ProjectionState::Unexpected
    );
    reconcile_recovery_state(&managed, &config).unwrap();
    assert!(!managed.join("preview.session").exists());
    assert!(!managed.join("current.ghostty").exists());
    assert!(inspect_history(&managed).unwrap().activations().is_empty());
}

#[test]
fn legacy_profile_apply_recovers_interrupted_preview_without_adopting_draft() {
    let _serial = SERIAL.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let (managed, config) = fixture(temp.path());
    // A pre-preview v1 installation need not have a Welcome Profile.
    apply(temp.path(), &managed, &config, "legacy");
    let original =
        fs::read(managed.join("history/activations/act-v1-0000000000000001.json")).unwrap();
    let status = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "preview_child"])
        .env("GHOSTTY_WALL_PREVIEW_TEST_CHILD", temp.path())
        .status()
        .unwrap();
    assert!(status.success());
    let projection_before = fs::read(managed.join("current.ghostty")).unwrap();
    let marker_before = fs::read(managed.join("preview.session")).unwrap();
    assert_eq!(
        inspect_recovery_state(&managed, &config)
            .unwrap()
            .projection(),
        ProjectionState::OutOfSync
    );
    assert_eq!(
        fs::read(managed.join("current.ghostty")).unwrap(),
        projection_before
    );
    assert_eq!(
        fs::read(managed.join("preview.session")).unwrap(),
        marker_before
    );
    apply(temp.path(), &managed, &config, "other");
    assert!(!managed.join("preview.session").exists());
    let history = inspect_history(&managed).unwrap();
    assert_eq!(history.activations().len(), 2);
    assert_eq!(
        fs::read(managed.join("history/activations/act-v1-0000000000000001.json")).unwrap(),
        original
    );
    assert_eq!(
        inspect_recovery_state(&managed, &config)
            .unwrap()
            .projection(),
        ProjectionState::Consistent
    );
}

#[test]
fn interrupted_preview_recovers_without_new_activation() {
    let _serial = SERIAL.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let (managed, config) = fixture(temp.path());
    apply(temp.path(), &managed, &config, "welcome");
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "preview_child"])
        .env("GHOSTTY_WALL_PREVIEW_TEST_CHILD", temp.path())
        .status()
        .unwrap();
    assert!(status.success());
    assert_eq!(
        inspect_recovery_state(&managed, &config)
            .unwrap()
            .projection(),
        ProjectionState::OutOfSync
    );
    reconcile_recovery_state(&managed, &config).unwrap();
    assert!(!managed.join("preview.session").exists());
    assert_eq!(inspect_history(&managed).unwrap().activations().len(), 1);
    assert_eq!(
        inspect_recovery_state(&managed, &config)
            .unwrap()
            .projection(),
        ProjectionState::Consistent
    );
}
