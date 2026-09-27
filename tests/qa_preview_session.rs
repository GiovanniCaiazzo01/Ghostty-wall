#![cfg(target_os = "linux")]

use std::{
    collections::BTreeMap,
    fs,
    os::unix::process::{CommandExt, ExitStatusExt},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use ghostty_wall::{
    apply::{ApplyError, ApplyOutcome, apply_local_profile, previous},
    codec::{intent::parse_named_profile_toml, manifest},
    domain::{EnvironmentManifest, Sha256Digest},
    history::inspect_history,
    init::{InitPaths, init},
    plan::{ReloadUnavailableReason, plan_local_profile_json_uninspected},
    preview::{PreviewError, PreviewSession},
    profile_workflow::{ProfileDraft, ProfileOutcome, ProfileWorkflows, WorkflowError},
    recovery::{ProjectionState, inspect_recovery_state, reconcile_recovery_state},
    runtime::{ReloadFailure, ReloadOutcome, UnavailableReload},
};
use sha2::{Digest, Sha256};

const TIME: &str = "2026-09-23T08:31:15.123456Z";
const PNG: &[u8] = include_bytes!("fixtures/white.png");
const JPEG: &[u8] = include_bytes!("fixtures/white.jpg");

struct Fixture {
    _temp: tempfile::TempDir,
    paths: InitPaths,
    root: PathBuf,
    hook: PathBuf,
}

impl Fixture {
    fn legacy() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let paths = paths(temp.path());
        init(&paths).unwrap();
        let root = paths.managed_root();
        let hook = hook(temp.path());
        // A customized pre-preview installation has v1 Intent and no Welcome.
        fs::remove_file(root.join("profiles/welcome.toml")).unwrap();
        fs::remove_file(root.join("profiles/welcome.png")).unwrap();
        fs::write(
            root.join("config.toml"),
            "schema_version = 1\n[sources.local]\nkind = \"local-directory\"\npath = \"profiles\"\n",
        )
        .unwrap();
        fs::write(root.join("profiles/legacy.png"), PNG).unwrap();
        fs::write(root.join("profiles/legacy.toml"), legacy_profile(11)).unwrap();
        fs::write(
            root.join("profiles/boy.toml"),
            "schema_version = 1\n[wallpaper]\nmode = \"none\"\n[terminal]\nfont_size = 13\n",
        )
        .unwrap();
        Self {
            _temp: temp,
            paths,
            root,
            hook,
        }
    }

    fn workflow(&self) -> ProfileWorkflows {
        ProfileWorkflows::load(self.paths.clone()).unwrap()
    }

    fn apply(&self, name: &str) -> ApplyOutcome {
        apply_saved(&self.paths.home, name).unwrap()
    }
}

fn paths(home: &Path) -> InitPaths {
    InitPaths {
        home: home.to_owned(),
        xdg_config_home: Some(home.join("xdg")),
    }
}

fn hook(home: &Path) -> PathBuf {
    home.join("xdg/ghostty/config.ghostty")
}

fn legacy_profile(font: u32) -> String {
    format!(
        "schema_version = 1\n[wallpaper]\nmode = \"source\"\nsource = \"local\"\nselection = \"path\"\npath = \"legacy.png\"\nfit = \"contain\"\nopacity = 0.2\n[terminal]\nfont_size = {font}\n"
    )
}

fn apply_saved(home: &Path, name: &str) -> Result<ApplyOutcome, ApplyError> {
    let paths = paths(home);
    let root = paths.managed_root();
    let workflow = ProfileWorkflows::load(paths).unwrap();
    let draft = workflow.edit(name).unwrap();
    let (_, profile) = parse_named_profile_toml(name, workflow.config(), draft.document()).unwrap();
    apply_local_profile(
        &root,
        home,
        &root,
        &hook(home),
        draft.id(),
        workflow.config(),
        &profile,
        None,
        TIME,
        UnavailableReload::new(ReloadUnavailableReason::AdapterCommandUnavailable),
    )
}

fn resolve(f: &Fixture, workflow: &ProfileWorkflows, draft: &ProfileDraft) -> EnvironmentManifest {
    let (_, profile) =
        parse_named_profile_toml(draft.id().as_str(), workflow.config(), draft.document()).unwrap();
    // The session owns the lock, so its caller must not use a lock-taking Plan reader.
    let plan = plan_local_profile_json_uninspected(
        &f.root,
        &f.paths.home,
        &f.root,
        draft.id(),
        workflow.config(),
        &profile,
        None,
    )
    .unwrap();
    manifest::decode(&serde_json::to_vec(&plan["environment"]["manifest"]).unwrap()).unwrap()
}

fn image_draft() -> EnvironmentManifest {
    let digest = Sha256Digest::from_bytes(Sha256::digest(JPEG).into()).to_string();
    manifest::decode(
        &serde_json::to_vec(&serde_json::json!({
            "schema_version": 1,
            "wallpaper": {"mode": "image", "asset_sha256": digest, "media_type": "image/jpeg"},
            "terminal": {"font_size_millipoints": 19000}
        }))
        .unwrap(),
    )
    .unwrap()
}

fn files(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn visit(root: &Path, path: &Path, result: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            let kind = fs::symlink_metadata(&path).unwrap().file_type();
            if kind.is_dir() {
                visit(root, &path, result);
            } else {
                assert!(kind.is_file(), "unexpected entry {}", path.display());
                result.insert(
                    path.strip_prefix(root).unwrap().to_owned(),
                    fs::read(path).unwrap(),
                );
            }
        }
    }
    let mut result = BTreeMap::new();
    visit(root, root, &mut result);
    result
}

fn staged_images(root: &Path) -> Vec<PathBuf> {
    fs::read_dir(root)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(".tmp-preview-image-")
        })
        .collect()
}

fn assert_clean(root: &Path, hook: &Path) {
    assert!(!root.join("preview.session").exists());
    assert!(staged_images(root).is_empty());
    assert_eq!(
        inspect_recovery_state(root, hook).unwrap().projection(),
        ProjectionState::Consistent
    );
}

#[test]
fn replay_active_without_welcome_cancels_to_immutable_environment_even_after_source_loss() {
    let f = Fixture::legacy();
    f.apply("legacy");
    f.apply("boy");
    previous(&f.root, &f.hook, TIME, || Ok::<(), ()>(())).unwrap();
    let starting = inspect_history(&f.root).unwrap().latest().unwrap().clone();
    assert_eq!(starting.sequence(), 3);
    assert!(starting.profile_id().is_none());
    fs::remove_file(f.root.join("profiles/legacy.png")).unwrap();
    fs::remove_file(f.root.join("profiles/legacy.toml")).unwrap();
    fs::write(f.root.join("config.toml"), b"unavailable source registry").unwrap();
    let before = files(&f.root);
    let mut session = PreviewSession::begin(&f.root, &f.hook, || Ok::<(), ()>(())).unwrap();
    assert_eq!(
        session.starting_activation().unwrap().environment(),
        starting.environment()
    );
    let mut json: serde_json::Value =
        serde_json::from_slice(&manifest::encode_canonical(starting.environment()).unwrap())
            .unwrap();
    json["terminal"]["font_size_millipoints"] = 27500.into();
    let draft = manifest::decode(&serde_json::to_vec(&json).unwrap()).unwrap();
    session.update(&draft).unwrap();
    assert!(
        staged_images(&f.root).is_empty(),
        "reuse the starting Durable Asset"
    );
    assert!(
        fs::read_to_string(f.root.join("current.ghostty"))
            .unwrap()
            .contains("font-size = 27.500")
    );
    session.cancel().unwrap();
    assert_eq!(files(&f.root), before);
    assert_clean(&f.root, &f.hook);
    assert!(!f.root.join("profiles/welcome.toml").exists());
}

#[test]
fn full_library_edit_cancel_then_save_use_and_previous_preserves_v1_data() {
    let f = Fixture::legacy();
    f.apply("legacy");
    let workflow = f.workflow();
    let before = files(&f.root);
    let mut draft = workflow.edit("boy").unwrap();
    let saved = draft.document().to_owned();
    let root = f.root.clone();
    let reload_frames = Arc::new(Mutex::new(Vec::new()));
    let frames = reload_frames.clone();
    let mut session = PreviewSession::begin(&f.root, &f.hook, move || {
        frames
            .lock()
            .unwrap()
            .push(fs::read(root.join("current.ghostty")).unwrap());
        Ok::<(), ()>(())
    })
    .unwrap();
    for font in [14, 15, 16] {
        draft
            .set_document(
                workflow.config(),
                saved.replace("font_size = 13", &format!("font_size = {font}")),
            )
            .unwrap();
        session.update(&resolve(&f, &workflow, &draft)).unwrap();
        assert_eq!(
            fs::read(f.root.join("profiles/boy.toml")).unwrap(),
            saved.as_bytes()
        );
        assert_eq!(
            files(&f.root.join("history")),
            files_from(&before, "history")
        );
    }
    session.cancel().unwrap();
    assert_eq!(files(&f.root), before);
    assert_eq!(reload_frames.lock().unwrap().len(), 4);
    assert_eq!(
        reload_frames.lock().unwrap().last().unwrap(),
        &before[Path::new("current.ghostty")]
    );
    assert_eq!(draft.saved_document(), Some(saved.as_str()));

    let mut session = PreviewSession::begin(&f.root, &f.hook, || Err::<(), ()>(())).unwrap();
    assert_eq!(
        session.update(&resolve(&f, &workflow, &draft)).unwrap(),
        ReloadOutcome::Failed(ReloadFailure::Reload)
    );
    assert_eq!(
        session.finish().unwrap(),
        ReloadOutcome::Failed(ReloadFailure::Reload)
    );
    let id = workflow.save_draft(&draft).unwrap();
    assert_eq!(inspect_history(&f.root).unwrap().activations().len(), 1);
    let outcome = workflow
        .use_saved(&id, |id| apply_saved(&f.paths.home, id.as_str()))
        .unwrap();
    assert!(matches!(
        outcome,
        ProfileOutcome::SavedAndApplied {
            reload: ReloadOutcome::Unavailable(_),
            ..
        }
    ));
    assert_eq!(inspect_history(&f.root).unwrap().activations().len(), 2);
    assert_eq!(
        fs::read_to_string(f.root.join("profiles/boy.toml")).unwrap(),
        draft.document()
    );
    assert!(draft.document().starts_with("schema_version = 1"));
    assert!(
        fs::read_to_string(f.root.join("current.ghostty"))
            .unwrap()
            .contains("font-size = 16.000")
    );
    previous(&f.root, &f.hook, TIME, || Ok::<(), ()>(())).unwrap();
    assert_eq!(
        fs::read(f.root.join("current.ghostty")).unwrap(),
        before[Path::new("current.ghostty")]
    );
    assert_eq!(inspect_history(&f.root).unwrap().activations().len(), 3);
    assert_clean(&f.root, &f.hook);
}

fn files_from(files: &BTreeMap<PathBuf, Vec<u8>>, prefix: &str) -> BTreeMap<PathBuf, Vec<u8>> {
    files
        .iter()
        .filter_map(|(p, b)| {
            p.strip_prefix(prefix)
                .ok()
                .map(|p| (p.to_owned(), b.clone()))
        })
        .collect()
}

#[test]
fn restore_reload_releases_lock_and_cannot_undo_an_apply_during_that_reload() {
    for (ending, fail_restore) in ["cancel", "finish", "drop"]
        .into_iter()
        .flat_map(|ending| [false, true].map(|fail| (ending, fail)))
    {
        let f = Fixture::legacy();
        f.apply("legacy");
        let before = files(&f.root);
        let hook_before = fs::read(&f.hook).unwrap();
        let home = f.paths.home.clone();
        let called = Arc::new(Mutex::new(0));
        let calls = called.clone();
        let mut session = PreviewSession::begin(&f.root, &f.hook, move || {
            let mut count = calls.lock().unwrap();
            *count += 1;
            if *count == 2 {
                // A failed restoration reload must not re-arm Drop's rollback after
                // another writer has committed under the released lock (RFC 0009).
                apply_saved(&home, "boy").unwrap();
                if fail_restore {
                    return Err(());
                }
            }
            Ok::<(), ()>(())
        })
        .unwrap();
        assert_eq!(
            session.update_with_image(&image_draft(), JPEG).unwrap(),
            ReloadOutcome::Succeeded
        );
        let expected = if fail_restore {
            ReloadOutcome::Failed(ReloadFailure::Reload)
        } else {
            ReloadOutcome::Succeeded
        };
        match ending {
            "cancel" => assert_eq!(session.cancel().unwrap(), expected),
            "finish" => assert_eq!(session.finish().unwrap(), expected),
            "drop" => drop(session),
            _ => unreachable!(),
        }
        assert_eq!(*called.lock().unwrap(), 2, "{ending}, fail={fail_restore}");
        let history = inspect_history(&f.root).unwrap();
        assert_eq!(history.activations().len(), 2);
        let latest = history.latest().unwrap();
        assert_eq!(latest.sequence(), 2);
        assert_eq!(latest.profile_id().unwrap().as_str(), "boy");
        assert_eq!(
            fs::read_to_string(f.root.join("current.ghostty")).unwrap(),
            "background-image = \nfont-size = 13.000\n",
            "{ending}, fail={fail_restore}"
        );
        for (path, bytes) in before {
            if path != Path::new("current.ghostty") {
                assert_eq!(fs::read(f.root.join(path)).unwrap(), bytes);
            }
        }
        assert_eq!(fs::read(&f.hook).unwrap(), hook_before);
        assert!(!f.root.join("profiles/welcome.toml").exists());
        assert_clean(&f.root, &f.hook);
    }
}

#[test]
fn confirmation_conflict_preserves_competing_saved_profile_and_retains_unsaved_draft() {
    let f = Fixture::legacy();
    f.apply("legacy");
    let workflow = f.workflow();
    let mut draft = workflow.edit("boy").unwrap();
    let saved = draft.document().to_owned();
    draft
        .set_document(workflow.config(), saved.replace("13", "17"))
        .unwrap();
    let mut session = PreviewSession::begin(&f.root, &f.hook, || Ok::<(), ()>(())).unwrap();
    session.update(&resolve(&f, &workflow, &draft)).unwrap();
    session.finish().unwrap();
    let competitor = f.workflow();
    let mut newer = competitor.edit("boy").unwrap();
    newer
        .set_document(competitor.config(), saved.replace("13", "21"))
        .unwrap();
    competitor.save_draft(&newer).unwrap();
    let before = files(&f.root);
    assert!(matches!(
        workflow.save_draft(&draft),
        Err(WorkflowError::Changed(_))
    ));
    assert_eq!(files(&f.root), before);
    assert!(draft.document().contains("font_size = 17"));
    assert_eq!(draft.saved_document(), Some(saved.as_str()));
    assert_eq!(inspect_history(&f.root).unwrap().activations().len(), 1);
    assert_clean(&f.root, &f.hook);
}

#[test]
fn actual_apply_failure_after_confirm_is_separate_from_successful_intent_save() {
    let f = Fixture::legacy();
    f.apply("legacy");
    let workflow = f.workflow();
    let mut draft = workflow.edit("boy").unwrap();
    let saved = draft.document().replace("13", "17");
    draft
        .set_document(workflow.config(), saved.clone())
        .unwrap();
    let mut session = PreviewSession::begin(&f.root, &f.hook, || Ok::<(), ()>(())).unwrap();
    session.update(&resolve(&f, &workflow, &draft)).unwrap();
    session.finish().unwrap();
    let id = workflow.save_draft(&draft).unwrap();
    let committed = fs::read(f.root.join("current.ghostty")).unwrap();
    let history = files(&f.root.join("history"));
    let obstruction = f
        .root
        .join(format!(".tmp-projection-{}", std::process::id()));
    fs::write(&obstruction, b"pre-existing publication debris").unwrap();
    let error = workflow
        .use_saved(&id, |id| apply_saved(&f.paths.home, id.as_str()))
        .unwrap_err();
    assert!(matches!(error, WorkflowError::Apply(_)), "{error}");
    assert!(error.to_string().contains("Profile saved but apply failed"));
    assert_eq!(
        fs::read_to_string(f.root.join("profiles/boy.toml")).unwrap(),
        saved
    );
    assert_eq!(files(&f.root.join("history")), history);
    assert_eq!(fs::read(f.root.join("current.ghostty")).unwrap(), committed);
    assert_eq!(
        fs::read(&obstruction).unwrap(),
        b"pre-existing publication debris"
    );
    fs::remove_file(obstruction).unwrap();
    reconcile_recovery_state(&f.root, &f.hook).unwrap();
    workflow
        .use_saved(&id, |id| apply_saved(&f.paths.home, id.as_str()))
        .unwrap();
    assert_eq!(inspect_history(&f.root).unwrap().activations().len(), 2);
    assert_clean(&f.root, &f.hook);
}

// Invoked by qa_preview_publication.py with a child-only, path-scoped fsync fault.
#[test]
fn publication_fault_child() {
    let Ok(mode) = std::env::var("GW_QA_FAULT_MODE") else {
        return;
    };
    let home = PathBuf::from(std::env::var("GW_QA_PREVIEW_HOME").unwrap());
    let paths = paths(&home);
    init(&paths).unwrap();
    let root = paths.managed_root();
    let hook = hook(&home);
    let profile_path = root.join("profiles/boy.toml");
    let workflow = ProfileWorkflows::load(paths).unwrap();
    let first_image = home.join("first.png");
    fs::write(&first_image, PNG).unwrap();
    let mut created = workflow.create("boy").unwrap();
    workflow.import_image(&mut created, &first_image).unwrap();
    workflow.save(created).unwrap();
    apply_saved(&home, "boy").unwrap();
    let before = files(&root);
    let armed = home.join("fault-armed");
    let mut draft = workflow.edit("boy").unwrap();
    let original = home.join("original.jpg");
    fs::write(&original, JPEG).unwrap();
    workflow.import_image(&mut draft, &original).unwrap();
    let calls = Arc::new(Mutex::new(0));
    let recorded = calls.clone();
    let reload = move || {
        *recorded.lock().unwrap() += 1;
        Err::<(), ()>(())
    };
    if mode == "begin-sync" {
        fs::write(&armed, []).unwrap();
        assert!(matches!(
            PreviewSession::begin(&root, &hook, reload),
            Err(PreviewError::Io { .. })
        ));
        assert!(root.join("preview.session").exists());
        assert_eq!(*calls.lock().unwrap(), 0);
    } else {
        let mut session = PreviewSession::begin(&root, &hook, reload).unwrap();
        if mode == "image-sync" || mode == "update-sync" {
            fs::write(&armed, []).unwrap();
            if mode == "image-sync" {
                assert!(matches!(
                    session.update_with_image(&image_draft(), JPEG),
                    Err(PreviewError::Io { .. })
                ));
                assert_eq!(staged_images(&root).len(), 1);
                assert_eq!(
                    fs::read(root.join("current.ghostty")).unwrap(),
                    before[Path::new("current.ghostty")]
                );
            } else {
                assert!(matches!(
                    session.update(&EnvironmentManifest::new(None, None, None)),
                    Err(PreviewError::Recovery(_))
                ));
                // Rename succeeded before directory fsync failed: recover from the marker,
                // not from an assumption that a failed update left the old frame intact.
                assert!(fs::read(root.join("current.ghostty")).unwrap().is_empty());
                assert!(staged_images(&root).is_empty());
            }
            assert_eq!(*calls.lock().unwrap(), 0);
            assert!(session.cancel().is_err());
            assert!(root.join("preview.session").exists());
        } else {
            assert_eq!(
                session.update_with_image(&image_draft(), JPEG).unwrap(),
                ReloadOutcome::Failed(ReloadFailure::Reload)
            );
            if mode.starts_with("save-") {
                assert_eq!(
                    session.finish().unwrap(),
                    ReloadOutcome::Failed(ReloadFailure::Reload)
                );
                assert_eq!(files(&root), before);
                fs::write(&armed, []).unwrap();
                let error = workflow.save_draft(&draft).unwrap_err();
                assert!(
                    matches!(error, WorkflowError::PublicationUncertain { .. }),
                    "{error}"
                );
                assert!(error.to_string().contains("durability is uncertain"));
                assert!(!error.to_string().contains("apply failed"));
                if mode == "save-profile-sync" {
                    assert_eq!(fs::read_to_string(&profile_path).unwrap(), draft.document());
                } else {
                    assert_eq!(
                        fs::read(&profile_path).unwrap(),
                        before[Path::new("profiles/boy.toml")]
                    );
                }
                assert_eq!(files(&root.join("history")), files_from(&before, "history"));
                assert_clean(&root, &hook);
                assert_eq!(*calls.lock().unwrap(), 2);
                fs::remove_file(&armed).unwrap();
                if mode == "save-image-sync" {
                    // Inspect the uncertain artifact before explicitly retrying this same draft.
                    let images: Vec<_> = fs::read_dir(root.join("profiles"))
                        .unwrap()
                        .map(|e| e.unwrap().path())
                        .filter(|p| p.file_name().unwrap().to_string_lossy().starts_with("boy-"))
                        .collect();
                    assert_eq!(images.len(), 1);
                    assert_eq!(fs::read(&images[0]).unwrap(), JPEG);
                    workflow.save_draft(&draft).unwrap();
                }
                workflow
                    .use_saved(draft.id(), |id| apply_saved(&home, id.as_str()))
                    .unwrap();
                assert_eq!(inspect_history(&root).unwrap().activations().len(), 2);
                assert_eq!(fs::read(&original).unwrap(), JPEG);
                assert_clean(&root, &hook);
                return;
            }
            fs::write(&armed, []).unwrap();
            assert!(session.cancel().is_err());
            assert!(
                root.join("preview.session").exists(),
                "failed restoration/cleanup retains the recovery marker"
            );
            assert_eq!(
                *calls.lock().unwrap(),
                1,
                "failed restoration must not claim a reload"
            );
        }
    }
    assert_eq!(files(&root.join("history")), files_from(&before, "history"));
    fs::remove_file(armed).unwrap();
    reconcile_recovery_state(&root, &hook).unwrap();
    assert_eq!(files(&root), before);
    assert_clean(&root, &hook);
    assert_eq!(fs::read(original).unwrap(), JPEG);
}

// This child has no platform runtime adapter; even a successful reload is recording-only.
#[test]
fn isolated_preview_child() {
    let Ok(home) = std::env::var("GW_QA_PREVIEW_HOME") else {
        return;
    };
    let home = PathBuf::from(home);
    let root = paths(&home).managed_root();
    let mut session = PreviewSession::begin(&root, &hook(&home), || Ok::<(), ()>(())).unwrap();
    session.update_with_image(&image_draft(), JPEG).unwrap();
    fs::write(home.join("ready"), []).unwrap();
    loop {
        thread::sleep(Duration::from_millis(10));
    }
}

struct OwnedChild(Child);

impl OwnedChild {
    fn wait_for_exit(&mut self, timeout: Duration) -> ExitStatus {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(status) = self.0.try_wait().unwrap() {
                return status;
            }
            assert!(
                Instant::now() < deadline,
                "preview child did not exit in time"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn interrupt(f: &Fixture, signal: i32) {
    let ready = f.paths.home.join("ready");
    if ready.exists() {
        fs::remove_file(&ready).unwrap();
    }
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "isolated_preview_child", "--test-threads=1"])
        .env_clear()
        .env("GW_QA_PREVIEW_HOME", &f.paths.home);
    // Background test runners may pass SIGINT as ignored; restore the child's disposition.
    unsafe {
        command.pre_exec(|| {
            if libc::signal(libc::SIGINT, libc::SIG_DFL) == libc::SIG_ERR {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = OwnedChild(command.spawn().unwrap());
    let deadline = Instant::now() + Duration::from_secs(10);
    while !ready.exists() {
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "preview child exited early"
        );
        assert!(Instant::now() < deadline, "preview child timed out");
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(unsafe { libc::kill(child.0.id() as i32, signal) }, 0);
    assert_eq!(
        child.wait_for_exit(Duration::from_secs(10)).signal(),
        Some(signal)
    );
}

#[test]
fn background_runner_ignoring_sigint_still_recovers_interrupted_preview() {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "sigkill_and_sigint_image_sessions_recover_on_begin_apply_and_previous_without_sources",
            "--test-threads=1",
        ])
        .env_clear();
    // Model a background shell without changing this test runner's signal disposition.
    unsafe {
        command.pre_exec(|| {
            if libc::signal(libc::SIGINT, libc::SIG_IGN) == libc::SIG_ERR {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = OwnedChild(command.spawn().unwrap());
    assert!(child.wait_for_exit(Duration::from_secs(30)).success());
}

#[test]
fn sigkill_and_sigint_image_sessions_recover_on_begin_apply_and_previous_without_sources() {
    for (action, signal) in [
        ("begin", libc::SIGKILL),
        ("apply", libc::SIGINT),
        ("previous", libc::SIGKILL),
    ] {
        let f = Fixture::legacy();
        f.apply("legacy");
        f.apply("boy");
        let history = files(&f.root.join("history"));
        let environments = files(&f.root.join("environments"));
        let assets = files(&f.root.join("assets"));
        interrupt(&f, signal);
        assert_eq!(staged_images(&f.root).len(), 1);
        fs::remove_file(f.root.join("profiles/legacy.toml")).unwrap();
        fs::remove_file(f.root.join("profiles/legacy.png")).unwrap();
        let interrupted = files(&f.root);
        assert_eq!(
            inspect_recovery_state(&f.root, &f.hook)
                .unwrap()
                .projection(),
            ProjectionState::OutOfSync
        );
        assert_eq!(files(&f.root), interrupted, "inspection is read-only");
        match action {
            "begin" => {
                PreviewSession::begin(&f.root, &f.hook, || Ok::<(), ()>(()))
                    .unwrap()
                    .cancel()
                    .unwrap();
            }
            "apply" => {
                f.apply("boy");
            }
            "previous" => {
                previous(&f.root, &f.hook, TIME, || Ok::<(), ()>(())).unwrap();
            }
            _ => unreachable!(),
        }
        assert_clean(&f.root, &f.hook);
        assert_eq!(files(&f.root.join("environments")), environments);
        assert_eq!(files(&f.root.join("assets")), assets);
        let after = files(&f.root.join("history"));
        for (name, bytes) in &history {
            assert_eq!(&after[name], bytes);
        }
        assert_eq!(after.len(), history.len() + usize::from(action != "begin"));
        assert!(!f.root.join("profiles/welcome.toml").exists());
        let projection = fs::read_to_string(f.root.join("current.ghostty")).unwrap();
        assert!(projection.contains(if action == "previous" {
            "font-size = 11.000"
        } else {
            "font-size = 13.000"
        }));
    }
}

#[test]
fn cleanup_keeps_near_match_names_and_other_token_images() {
    let f = Fixture::legacy();
    f.apply("legacy");
    let before = files(&f.root);
    let mut session = PreviewSession::begin(&f.root, &f.hook, || Ok::<(), ()>(())).unwrap();
    session.update_with_image(&image_draft(), JPEG).unwrap();
    let marker: serde_json::Value =
        serde_json::from_slice(&fs::read(f.root.join("preview.session")).unwrap()).unwrap();
    let token = marker["token"].as_str().unwrap();
    let names = [
        format!(".tmp-preview-image-{token}-{}.png.bak", "a".repeat(64)),
        format!(".tmp-preview-image-{token}-{}.jpg", "A".repeat(64)),
        format!(".tmp-preview-image-{token}-{}.png", "a".repeat(63)),
        format!(
            ".tmp-preview-image-{}-{}.png",
            "f".repeat(32),
            "a".repeat(64)
        ),
    ];
    for name in &names {
        fs::write(f.root.join(name), b"not owned by this preview").unwrap();
    }
    session.cancel().unwrap();
    let mut after = files(&f.root);
    for name in names {
        assert_eq!(
            after.remove(Path::new(&name)).unwrap(),
            b"not owned by this preview"
        );
    }
    assert_eq!(after, before);
}

#[test]
fn editor_image_replacement_supports_legacy_profile_without_colors() {
    let f = Fixture::legacy();
    f.apply("legacy");
    let before = files(&f.root);
    let workflow = f.workflow();
    let mut draft = workflow.edit("boy").unwrap();
    let original = f.paths.home.join("original.jpg");
    fs::write(&original, JPEG).unwrap();
    workflow.import_image(&mut draft, &original).unwrap();
    assert!(draft.document().contains("schema_version = 2"));
    assert_eq!(files(&f.root), before);
    assert_eq!(fs::read(original).unwrap(), JPEG);
}

#[test]
fn interrupted_preview_recovery_preserves_evidence_when_committed_asset_is_missing() {
    let f = Fixture::legacy();
    f.apply("legacy");
    let before = files(&f.root);
    interrupt(&f, libc::SIGKILL);
    let digest = Sha256Digest::from_bytes(Sha256::digest(PNG).into()).to_string();
    let asset = f
        .root
        .join("assets/sha256")
        .join(&digest[..2])
        .join(format!("{digest}.png"));
    let retained = fs::read(&asset).unwrap();
    fs::remove_file(&asset).unwrap();
    let damaged = files(&f.root);
    let calls = Arc::new(Mutex::new(0));
    let recorded = calls.clone();
    assert!(
        PreviewSession::begin(&f.root, &f.hook, move || {
            *recorded.lock().unwrap() += 1;
            Ok::<(), ()>(())
        })
        .is_err()
    );
    assert!(inspect_recovery_state(&f.root, &f.hook).is_err());
    assert!(reconcile_recovery_state(&f.root, &f.hook).is_err());
    assert_eq!(
        files(&f.root),
        damaged,
        "corruption must not be repaired from draft bytes"
    );
    assert_eq!(*calls.lock().unwrap(), 0);
    assert!(f.root.join("preview.session").exists());
    assert_eq!(staged_images(&f.root).len(), 1);

    // Restore the known fixture bytes, not a production repair or Source re-resolution.
    fs::write(asset, retained).unwrap();
    reconcile_recovery_state(&f.root, &f.hook).unwrap();
    assert_eq!(files(&f.root), before);
    assert_clean(&f.root, &f.hook);
}

#[test]
fn empty_history_image_crash_waits_for_explicit_hook_repair_then_restores_absence() {
    let f = Fixture::legacy();
    let before = files(&f.root);
    let installed_hook = fs::read(&f.hook).unwrap();
    interrupt(&f, libc::SIGKILL);
    let interrupted = files(&f.root);
    assert_eq!(
        inspect_recovery_state(&f.root, &f.hook)
            .unwrap()
            .projection(),
        ProjectionState::Unexpected
    );
    assert_eq!(files(&f.root), interrupted);
    fs::write(&f.hook, b"font-size = 9\n").unwrap();
    assert!(PreviewSession::begin(&f.root, &f.hook, || Ok::<(), ()>(())).is_err());
    assert!(reconcile_recovery_state(&f.root, &f.hook).is_err());
    assert_eq!(files(&f.root), interrupted);
    assert_eq!(fs::read(&f.hook).unwrap(), b"font-size = 9\n");

    fs::write(&f.hook, installed_hook).unwrap();
    reconcile_recovery_state(&f.root, &f.hook).unwrap();
    assert_eq!(files(&f.root), before);
    assert!(inspect_history(&f.root).unwrap().activations().is_empty());
    assert!(!f.root.join("current.ghostty").exists());
    assert_clean(&f.root, &f.hook);
}

#[test]
fn editor_panic_unwinds_to_starting_projection_and_cleans_image_without_saving() {
    let f = Fixture::legacy();
    f.apply("legacy");
    let before = files(&f.root);
    let root = f.root.clone();
    let frames = Arc::new(Mutex::new(Vec::new()));
    let recorded = frames.clone();
    let mut session = PreviewSession::begin(&f.root, &f.hook, move || {
        recorded
            .lock()
            .unwrap()
            .push(fs::read(root.join("current.ghostty")).unwrap());
        Ok::<(), ()>(())
    })
    .unwrap();
    session.update_with_image(&image_draft(), JPEG).unwrap();
    assert_ne!(
        fs::read(f.root.join("current.ghostty")).unwrap(),
        before[Path::new("current.ghostty")]
    );
    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        let _editor = session;
        panic!("isolated editor failure after publishing draft");
    }));
    assert!(panicked.is_err());
    assert_eq!(files(&f.root), before);
    let frames = frames.lock().unwrap();
    assert_eq!(frames.len(), 2);
    assert_eq!(
        frames.last().unwrap(),
        &before[Path::new("current.ghostty")]
    );
    assert_clean(&f.root, &f.hook);
}

#[test]
fn modified_owner_token_fails_closed_and_does_not_restore_over_unknown_session() {
    let f = Fixture::legacy();
    f.apply("legacy");
    let mut session = PreviewSession::begin(&f.root, &f.hook, || Ok::<(), ()>(())).unwrap();
    session.update_with_image(&image_draft(), JPEG).unwrap();
    let marker_path = f.root.join("preview.session");
    let original = fs::read(&marker_path).unwrap();
    let mut marker: serde_json::Value = serde_json::from_slice(&original).unwrap();
    marker["token"] = "e".repeat(32).into();
    fs::write(&marker_path, serde_json::to_vec(&marker).unwrap()).unwrap();
    let before = files(&f.root);
    assert!(matches!(
        session.update(&EnvironmentManifest::new(None, None, None)),
        Err(PreviewError::InvalidMarker(_))
    ));
    assert!(matches!(
        session.cancel(),
        Err(PreviewError::InvalidMarker(_))
    ));
    assert_eq!(files(&f.root), before);
    fs::write(&marker_path, original).unwrap();
    reconcile_recovery_state(&f.root, &f.hook).unwrap();
    assert_clean(&f.root, &f.hook);
}
