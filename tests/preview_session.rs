#![cfg(target_os = "linux")]

use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::{Path, PathBuf},
    process::{Child, Command},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use ghostty_wall::{
    apply::{ApplyOutcome, apply_local_profile},
    codec::{intent::parse_named_profile_toml, manifest},
    domain::{EnvironmentManifest, IntentId, Sha256Digest},
    history::inspect_history,
    init::{InitPaths, init},
    plan::plan_local_profile_json_uninspected,
    preview::{PreviewError, PreviewSession},
    profile_workflow::{ProfileDraft, ProfileOutcome, ProfileWorkflows, WorkflowError},
    recovery::{ProjectionState, inspect_recovery_state, reconcile_recovery_state},
    runtime::{ReloadFailure, ReloadOutcome},
};
use sha2::{Digest, Sha256};

const TIME: &str = "2026-09-23T08:31:15.123456Z";
const JPEG: &[u8] = include_bytes!("fixtures/white.jpg");
const PNG: &[u8] = include_bytes!("fixtures/palette.png");

struct Fixture {
    _temp: tempfile::TempDir,
    paths: InitPaths,
    root: PathBuf,
    hook: PathBuf,
    workflow: ProfileWorkflows,
}

impl Fixture {
    fn new(active: bool) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let paths = InitPaths {
            home: temp.path().to_owned(),
            xdg_config_home: Some(temp.path().join("xdg")),
        };
        init(&paths).unwrap();
        let root = paths.managed_root();
        let hook = paths.home.join("xdg/ghostty/config.ghostty");
        let workflow = ProfileWorkflows::load(paths.clone()).unwrap();
        let original = paths.home.join("user-original.jpg");
        fs::write(&original, JPEG).unwrap();
        let mut boy = workflow.create("boy").unwrap();
        workflow.import_image(&mut boy, &original).unwrap();
        workflow.save(boy).unwrap();
        let fixture = Self {
            _temp: temp,
            paths,
            root,
            hook,
            workflow,
        };
        if active {
            fixture.apply("welcome", false);
        }
        fixture
    }

    fn apply(&self, name: &str, fail_reload: bool) -> ApplyOutcome {
        apply_saved(&self.paths, &self.root, &self.hook, name, fail_reload)
    }

    fn resolve(&self, draft: &ProfileDraft) -> EnvironmentManifest {
        let (_, profile) = parse_named_profile_toml(
            draft.id().as_str(),
            self.workflow.config(),
            draft.document(),
        )
        .unwrap();
        // Preview owns the state lock; a normal Plan would wait on our own editor.
        let plan = plan_local_profile_json_uninspected(
            &self.root,
            &self.paths.home,
            &self.root,
            draft.id(),
            self.workflow.config(),
            &profile,
            None,
        )
        .unwrap();
        manifest::decode(&serde_json::to_vec(&plan["environment"]["manifest"]).unwrap()).unwrap()
    }
}

fn apply_saved(
    paths: &InitPaths,
    root: &Path,
    hook: &Path,
    name: &str,
    fail_reload: bool,
) -> ApplyOutcome {
    let workflow = ProfileWorkflows::load(paths.clone()).unwrap();
    let draft = workflow.edit(name).unwrap();
    let (_, profile) = parse_named_profile_toml(name, workflow.config(), draft.document()).unwrap();
    apply_local_profile(
        root,
        &paths.home,
        root,
        hook,
        draft.id(),
        workflow.config(),
        &profile,
        None,
        TIME,
        || if fail_reload { Err(()) } else { Ok(()) },
    )
    .unwrap()
}

fn image_manifest(bytes: &[u8], media: &str, font: u32) -> EnvironmentManifest {
    let digest = Sha256Digest::from_bytes(Sha256::digest(bytes).into()).to_string();
    manifest::decode(
        &serde_json::to_vec(&serde_json::json!({
            "schema_version": 1,
            "wallpaper": { "mode": "image", "asset_sha256": digest, "media_type": media },
            "terminal": { "font_size_millipoints": font }
        }))
        .unwrap(),
    )
    .unwrap()
}

fn files(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn visit(root: &Path, directory: &Path, result: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            let kind = fs::symlink_metadata(&path).unwrap().file_type();
            if kind.is_dir() {
                visit(root, &path, result);
            } else {
                assert!(kind.is_file(), "unexpected entry {}", path.display());
                result.insert(
                    path.strip_prefix(root).unwrap().to_owned(),
                    fs::read(&path).unwrap(),
                );
            }
        }
    }
    let mut result = BTreeMap::new();
    visit(root, root, &mut result);
    result
}

fn durable_files(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    files(root)
        .into_iter()
        .filter(|(path, _)| {
            let name = path.to_string_lossy();
            name != "current.ghostty" && name != "preview.session" && !name.starts_with(".tmp-")
        })
        .collect()
}

fn image_paths(root: &Path) -> Vec<PathBuf> {
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

fn font_draft(workflow: &ProfileWorkflows, draft: &mut ProfileDraft, font: u32) {
    let original = draft.saved_document().unwrap();
    draft
        .set_document(
            workflow.config(),
            format!("{original}\n[terminal]\nfont_size = {font}\n"),
        )
        .unwrap();
}

#[test]
fn cli_help_distinguishes_library_sessions_from_read_only_previews() {
    let home = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_ghostty-wall"))
        .arg("--help")
        .env_clear()
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join("xdg"))
        .env("PATH", "/nonexistent")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let help = String::from_utf8(output.stdout).unwrap();
    for distinction in [
        "read-only; not live Ghostty reload",
        "saves immediately; no live draft",
        "Live draft sessions are library-only; CLI/TUI previews do not reload Ghostty.",
        "apply/previous restore from History before committing; doctor is read-only",
    ] {
        assert!(help.contains(distinction), "missing help: {distinction}");
    }
    assert_eq!(fs::read_dir(home.path()).unwrap().count(), 0);
}

#[test]
fn welcome_active_edit_boy_cancel_restores_welcome_without_saving_or_history_events() {
    let f = Fixture::new(true);
    let before = files(&f.root);
    let durable = durable_files(&f.root);
    let root = f.root.clone();
    let reloads = Arc::new(Mutex::new(Vec::new()));
    let record = reloads.clone();
    let mut session = PreviewSession::begin(&f.root, &f.hook, move || {
        record
            .lock()
            .unwrap()
            .push(fs::read(root.join("current.ghostty")).unwrap());
        Ok::<(), ()>(())
    })
    .unwrap();
    assert_eq!(
        session
            .starting_activation()
            .unwrap()
            .profile_id()
            .unwrap()
            .as_str(),
        "welcome"
    );
    assert_eq!(session.starting_activation().unwrap().sequence(), 1);
    let mut boy = f.workflow.edit("boy").unwrap();
    let saved = boy.saved_document().unwrap().to_owned();
    for font in [14, 15, 16] {
        font_draft(&f.workflow, &mut boy, font);
        let draft = f.resolve(&boy);
        assert_ne!(draft, *session.starting_activation().unwrap().environment());
        assert_eq!(
            session.update_with_image(&draft, JPEG).unwrap(),
            ReloadOutcome::Succeeded
        );
        assert_eq!(boy.saved_document(), Some(saved.as_str()));
        assert_eq!(durable_files(&f.root), durable);
    }
    let images = image_paths(&f.root);
    assert_eq!(images.len(), 1, "slider movements reuse one staged image");
    assert_eq!(fs::read(&images[0]).unwrap(), JPEG);
    assert_eq!(
        fs::metadata(&images[0]).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let last_draft = fs::read(f.root.join("current.ghostty")).unwrap();
    assert!(String::from_utf8_lossy(&last_draft).contains(".tmp-preview-image-"));
    assert_ne!(last_draft, before[Path::new("current.ghostty")]);
    assert_eq!(session.cancel().unwrap(), ReloadOutcome::Succeeded);
    assert_eq!(files(&f.root), before);
    assert_eq!(reloads.lock().unwrap().len(), 4);
    assert_eq!(
        reloads.lock().unwrap().last().unwrap(),
        &before[Path::new("current.ghostty")]
    );
    assert_eq!(
        fs::read(f.paths.home.join("user-original.jpg")).unwrap(),
        JPEG
    );
}

#[test]
fn finish_save_and_use_has_one_activation_and_separate_restoration_and_apply_reload_results() {
    let f = Fixture::new(true);
    let before = files(&f.root);
    let mut session = PreviewSession::begin(&f.root, &f.hook, || Err::<(), ()>(())).unwrap();
    let mut boy = f.workflow.edit("boy").unwrap();
    for font in [14, 15] {
        font_draft(&f.workflow, &mut boy, font);
        assert_eq!(
            session.update_with_image(&f.resolve(&boy), JPEG).unwrap(),
            ReloadOutcome::Failed(ReloadFailure::Reload)
        );
    }
    let restoration = session.finish().unwrap();
    assert_eq!(restoration, ReloadOutcome::Failed(ReloadFailure::Reload));
    assert_eq!(files(&f.root), before);
    let id = f.workflow.save_draft(&boy).unwrap();
    assert_eq!(inspect_history(&f.root).unwrap().activations().len(), 1);
    let applied = f
        .workflow
        .use_saved(&id, |id| Ok::<_, String>(f.apply(id.as_str(), true)))
        .unwrap();
    assert!(matches!(
        applied,
        ProfileOutcome::SavedAndApplied {
            reload: ReloadOutcome::Failed(ReloadFailure::Reload),
            ..
        }
    ));
    assert_eq!(inspect_history(&f.root).unwrap().activations().len(), 2);
    assert!(image_paths(&f.root).is_empty());
    assert!(!f.root.join("preview.session").exists());
    let projection = fs::read_to_string(f.root.join("current.ghostty")).unwrap();
    assert!(projection.contains("font-size = 15.000"));
    assert!(projection.contains("assets/sha256/"));
    assert!(!projection.contains(".tmp-preview"));
}

#[test]
fn failed_edit_save_retains_draft_rolls_back_only_new_image_and_is_not_an_apply_or_reload_error() {
    let f = Fixture::new(true);
    let before = files(&f.root);
    let mut session = PreviewSession::begin(&f.root, &f.hook, || Err::<(), ()>(())).unwrap();
    let mut boy = f.workflow.edit("boy").unwrap();
    font_draft(&f.workflow, &mut boy, 17);
    session.update_with_image(&f.resolve(&boy), JPEG).unwrap();
    let replacement = f.paths.home.join("replacement.png");
    fs::write(&replacement, PNG).unwrap();
    f.workflow.import_image(&mut boy, &replacement).unwrap();
    let draft_document = boy.document().to_owned();
    let restoration = session.finish().unwrap();
    assert_eq!(restoration, ReloadOutcome::Failed(ReloadFailure::Reload));
    let profile = f.root.join("profiles/boy.toml");
    fs::set_permissions(&profile, fs::Permissions::from_mode(0o622)).unwrap();
    let error = f.workflow.save_draft(&boy).unwrap_err();
    assert!(matches!(error, WorkflowError::SaveFailed { .. }), "{error}");
    assert!(error.to_string().contains("was not saved"));
    assert!(!error.to_string().contains("apply failed"));
    assert_eq!(boy.document(), draft_document);
    assert_eq!(files(&f.root), before);
    assert_eq!(fs::read(&replacement).unwrap(), PNG);
    fs::set_permissions(&profile, fs::Permissions::from_mode(0o600)).unwrap();
    let saved = f.workflow.save_draft(&boy).unwrap();
    assert_eq!(fs::read_to_string(profile).unwrap(), draft_document);
    assert!(matches!(
        f.workflow
            .use_saved(&saved, |_| Err::<ApplyOutcome, _>("offline")),
        Err(WorkflowError::Apply(_))
    ));
    assert_eq!(inspect_history(&f.root).unwrap().activations().len(), 1);
    assert_eq!(
        fs::read(f.root.join("current.ghostty")).unwrap(),
        before[Path::new("current.ghostty")]
    );
}

#[test]
fn rejected_images_never_publish_reload_or_create_files() {
    let f = Fixture::new(true);
    let calls = Arc::new(Mutex::new(0));
    let recorded = calls.clone();
    let mut session = PreviewSession::begin(&f.root, &f.hook, move || {
        *recorded.lock().unwrap() += 1;
        Ok::<(), ()>(())
    })
    .unwrap();
    let before = files(&f.root);
    let oversized = vec![0; 32 * 1024 * 1024 + 1];
    let mut wide = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(16_385, 1)
        .write_to(&mut wide, image::ImageFormat::Png)
        .unwrap();
    let cases = [
        (image_manifest(PNG, "image/png", 14000), JPEG),
        (image_manifest(JPEG, "image/png", 14000), JPEG),
        (
            image_manifest(b"\x89PNG\r\n\x1a\ninvalid", "image/png", 14000),
            b"\x89PNG\r\n\x1a\ninvalid".as_slice(),
        ),
        (
            image_manifest(&oversized, "image/jpeg", 14000),
            oversized.as_slice(),
        ),
        (
            image_manifest(wide.get_ref(), "image/png", 14000),
            wide.get_ref().as_slice(),
        ),
        (EnvironmentManifest::new(None, None, None), JPEG),
    ];
    for (draft, bytes) in cases {
        assert!(matches!(
            session.update_with_image(&draft, bytes),
            Err(PreviewError::InvalidImage)
        ));
        assert_eq!(files(&f.root), before);
    }
    assert_eq!(*calls.lock().unwrap(), 0);
    session.cancel().unwrap();
}

#[test]
fn later_publication_failure_preserves_first_frame_and_cleanup_does_not_remove_unowned_debris() {
    let f = Fixture::new(true);
    let before = files(&f.root);
    let calls = Arc::new(Mutex::new(0));
    let recorded = calls.clone();
    let mut session = PreviewSession::begin(&f.root, &f.hook, move || {
        *recorded.lock().unwrap() += 1;
        Ok::<(), ()>(())
    })
    .unwrap();
    session
        .update_with_image(&image_manifest(JPEG, "image/jpeg", 14000), JPEG)
        .unwrap();
    let first_projection = fs::read(f.root.join("current.ghostty")).unwrap();
    let first_image = image_paths(&f.root).pop().unwrap();
    let collision = f
        .root
        .join(format!(".tmp-projection-{}", std::process::id()));
    fs::write(&collision, b"unowned debris").unwrap();
    assert!(
        session
            .update_with_image(&image_manifest(PNG, "image/png", 15000), PNG)
            .is_err()
    );
    assert_eq!(fs::read(&collision).unwrap(), b"unowned debris");
    assert_eq!(
        fs::read(f.root.join("current.ghostty")).unwrap(),
        first_projection
    );
    assert_eq!(fs::read(first_image).unwrap(), JPEG);
    assert_eq!(*calls.lock().unwrap(), 1);
    assert_eq!(image_paths(&f.root).len(), 2);
    fs::remove_file(collision).unwrap();
    session.cancel().unwrap();
    assert_eq!(files(&f.root), before);
}

#[test]
fn temporary_image_substitution_is_rejected_and_cleanup_does_not_follow_symlinks_or_other_tokens() {
    let f = Fixture::new(true);
    let mut session = PreviewSession::begin(&f.root, &f.hook, || Ok::<(), ()>(())).unwrap();
    let draft = image_manifest(JPEG, "image/jpeg", 14000);
    session.update_with_image(&draft, JPEG).unwrap();
    let path = image_paths(&f.root).pop().unwrap();
    let outside = f.paths.home.join("keep.jpg");
    fs::write(&outside, JPEG).unwrap();
    fs::remove_file(&path).unwrap();
    symlink(&outside, &path).unwrap();
    let unrelated = f.root.join(format!(
        ".tmp-preview-image-{}-{}.jpg",
        "0".repeat(32),
        "a".repeat(64)
    ));
    fs::write(&unrelated, b"other token").unwrap();
    let projection = fs::read(f.root.join("current.ghostty")).unwrap();
    assert!(session.update_with_image(&draft, JPEG).is_err());
    assert_eq!(
        fs::read(f.root.join("current.ghostty")).unwrap(),
        projection
    );
    session.cancel().unwrap();
    assert_eq!(fs::read(outside).unwrap(), JPEG);
    assert_eq!(fs::read(unrelated).unwrap(), b"other token");
    assert!(!path.exists());
}

#[test]
fn unsafe_image_cleanup_keeps_marker_and_recovery_retries_without_deleting_directory_contents() {
    let f = Fixture::new(true);
    let before = files(&f.root);
    let mut session = PreviewSession::begin(&f.root, &f.hook, || Ok::<(), ()>(())).unwrap();
    session
        .update_with_image(&image_manifest(JPEG, "image/jpeg", 14000), JPEG)
        .unwrap();
    let path = image_paths(&f.root).pop().unwrap();
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    fs::write(path.join("keep"), b"do not recursively delete").unwrap();
    assert!(session.cancel().is_err());
    assert!(f.root.join("preview.session").exists());
    assert_eq!(
        fs::read(f.root.join("current.ghostty")).unwrap(),
        before[Path::new("current.ghostty")]
    );
    assert!(reconcile_recovery_state(&f.root, &f.hook).is_err());
    assert_eq!(
        fs::read(path.join("keep")).unwrap(),
        b"do not recursively delete"
    );
    fs::remove_file(path.join("keep")).unwrap();
    fs::remove_dir(path).unwrap();
    reconcile_recovery_state(&f.root, &f.hook).unwrap();
    assert_eq!(files(&f.root), before);
}

#[test]
fn drop_cleans_images_and_empty_history_cancel_restores_absence() {
    for active in [false, true] {
        let f = Fixture::new(active);
        let before = files(&f.root);
        let calls = Arc::new(Mutex::new(0));
        let recorded = calls.clone();
        let mut session = PreviewSession::begin(&f.root, &f.hook, move || {
            *recorded.lock().unwrap() += 1;
            Ok::<(), ()>(())
        })
        .unwrap();
        assert_eq!(session.starting_activation().is_some(), active);
        session
            .update_with_image(&image_manifest(PNG, "image/png", 14000), PNG)
            .unwrap();
        if active {
            drop(session);
        } else {
            session.cancel().unwrap();
        }
        assert_eq!(*calls.lock().unwrap(), 2);
        assert_eq!(files(&f.root), before);
    }
}

// Subprocesses use explicit disposable paths and recording-only adapters, never platform reload.
#[test]
fn session_child() {
    let Ok(home) = std::env::var("GHOSTTY_WALL_SESSION_TEST_HOME") else {
        return;
    };
    let home = PathBuf::from(home);
    let paths = InitPaths {
        home: home.clone(),
        xdg_config_home: Some(home.join("xdg")),
    };
    let root = paths.managed_root();
    let hook = home.join("xdg/ghostty/config.ghostty");
    if std::env::var("GHOSTTY_WALL_SESSION_TEST_ACTION").as_deref() == Ok("apply") {
        fs::write(home.join("applying"), []).unwrap();
        apply_saved(&paths, &root, &hook, "newer", false);
        return;
    }
    let mut session = PreviewSession::begin(&root, &hook, || Ok::<(), ()>(())).unwrap();
    if std::env::var("GHOSTTY_WALL_SESSION_TEST_ACTION").as_deref() == Ok("partial-image") {
        // Fault only this disposable child after marker publication, leaving a
        // partial image that recovery must delete without decoding or adopting.
        unsafe {
            libc::signal(libc::SIGXFSZ, libc::SIG_IGN);
            let limit = libc::rlimit {
                rlim_cur: 64,
                rlim_max: 64,
            };
            assert_eq!(libc::setrlimit(libc::RLIMIT_FSIZE, &limit), 0);
        }
        assert!(matches!(
            session.update_with_image(&image_manifest(JPEG, "image/jpeg", 18000), JPEG),
            Err(PreviewError::Io { .. })
        ));
    } else {
        session
            .update_with_image(&image_manifest(JPEG, "image/jpeg", 18000), JPEG)
            .unwrap();
    }
    std::process::exit(0);
}

fn child(f: &Fixture, action: &str) -> Child {
    Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "session_child"])
        .env_clear()
        .env("HOME", &f.paths.home)
        .env("XDG_CONFIG_HOME", f.paths.home.join("xdg"))
        .env("PATH", "/nonexistent")
        .env("GHOSTTY_WALL_SESSION_TEST_HOME", &f.paths.home)
        .env("GHOSTTY_WALL_SESSION_TEST_ACTION", action)
        .spawn()
        .unwrap()
}

fn wait(mut child: Child) {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success());
            return;
        }
        if Instant::now() > deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("isolated child timed out");
        }
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn interrupted_image_preview_is_read_only_drift_then_recovered_without_adopting_image_or_draft() {
    let f = Fixture::new(true);
    let before = files(&f.root);
    for stage in ["crash", "partial-image"] {
        wait(child(&f, stage));
        let interrupted = files(&f.root);
        let images = image_paths(&f.root);
        assert_eq!(images.len(), 1);
        if stage == "partial-image" {
            assert_eq!(fs::metadata(&images[0]).unwrap().len(), 64);
        }
        assert_eq!(
            inspect_recovery_state(&f.root, &f.hook)
                .unwrap()
                .projection(),
            ProjectionState::OutOfSync
        );
        assert_eq!(files(&f.root), interrupted);
        reconcile_recovery_state(&f.root, &f.hook).unwrap();
        assert_eq!(files(&f.root), before);
        reconcile_recovery_state(&f.root, &f.hook).unwrap();
        assert_eq!(files(&f.root), before);
    }
}

#[test]
fn process_apply_waits_and_cancel_never_rolls_back_the_newer_activation() {
    let f = Fixture::new(true);
    fs::write(
        f.root.join("profiles/newer.toml"),
        "schema_version = 1\n[terminal]\nfont_size = 23\n",
    )
    .unwrap();
    let mut session = PreviewSession::begin(&f.root, &f.hook, || Ok::<(), ()>(())).unwrap();
    session
        .update_with_image(&image_manifest(JPEG, "image/jpeg", 14000), JPEG)
        .unwrap();
    let mut applying = child(&f, "apply");
    let deadline = Instant::now() + Duration::from_secs(5);
    while !f.paths.home.join("applying").exists() {
        if Instant::now() > deadline {
            applying.kill().unwrap();
            applying.wait().unwrap();
            panic!("apply child did not start");
        }
        thread::sleep(Duration::from_millis(10));
    }
    thread::sleep(Duration::from_millis(100));
    assert!(applying.try_wait().unwrap().is_none());
    session.cancel().unwrap();
    wait(applying);
    let history = inspect_history(&f.root).unwrap();
    assert_eq!(history.activations().len(), 2);
    assert_eq!(
        history.latest().unwrap().profile_id(),
        Some(&"newer".parse::<IntentId>().unwrap())
    );
    assert_eq!(
        fs::read_to_string(f.root.join("current.ghostty")).unwrap(),
        "font-size = 23.000\n"
    );
    assert!(image_paths(&f.root).is_empty());
    assert!(!f.root.join("preview.session").exists());
}
