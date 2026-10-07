#![cfg(target_os = "linux")]

// These are protocol/reporting regressions, not ticket 10's owned-window visual proof.
// Linux commands are recorded or disposable stubs; CLI children cannot run real runtime commands.
use std::{
    cell::RefCell,
    collections::{BTreeMap, VecDeque},
    fs, io,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Output},
    rc::Rc,
};

use ghostty_wall::{
    codec::manifest,
    history::inspect_history,
    plan::{ReloadObservation, ReloadUnavailableReason},
    preview::{PreviewError, PreviewSession},
    recovery::{inspect_recovery_state, reconcile_recovery_state},
    runtime::{
        CommandResult, CommandRunner, ReloadAdapter, ReloadFailure, ReloadOutcome, SystemdReload,
    },
};

type Reply = Result<CommandResult, io::ErrorKind>;

#[derive(Clone)]
struct ScriptedCommands {
    systemctl: bool,
    replies: Rc<RefCell<VecDeque<Reply>>>,
    calls: Rc<RefCell<Vec<Vec<String>>>>,
}

impl ScriptedCommands {
    fn new(systemctl: bool, replies: impl IntoIterator<Item = Reply>) -> Self {
        Self {
            systemctl,
            replies: Rc::new(RefCell::new(replies.into_iter().collect())),
            calls: Rc::new(RefCell::new(Vec::new())),
        }
    }

    fn assert_calls(&self, expected: &[&[&str]]) {
        let expected: Vec<Vec<String>> = expected
            .iter()
            .map(|call| call.iter().map(|arg| (*arg).to_owned()).collect())
            .collect();
        assert_eq!(*self.calls.borrow(), expected);
        assert!(self.replies.borrow().is_empty(), "unconsumed command reply");
    }
}

impl CommandRunner for ScriptedCommands {
    fn available(&self, program: &Path) -> bool {
        program == Path::new("busctl") || (self.systemctl && program == Path::new("systemctl"))
    }

    fn run(&self, program: &Path, args: &[&str]) -> io::Result<CommandResult> {
        assert!(self.available(program));
        let mut call = vec![program.to_str().unwrap().to_owned()];
        call.extend(args.iter().map(|arg| (*arg).to_owned()));
        self.calls.borrow_mut().push(call);
        self.replies
            .borrow_mut()
            .pop_front()
            .expect("unexpected runtime command")
            .map_err(io::Error::from)
    }
}

fn reply(success: bool) -> Reply {
    Ok(CommandResult::new(success, []))
}

const BUS_PROBE: &[&str] = &[
    "busctl",
    "--user",
    "--quiet",
    "status",
    "com.mitchellh.ghostty",
];
const BUS_RELOAD: &[&str] = &[
    "busctl",
    "--user",
    "call",
    "com.mitchellh.ghostty",
    "/com/mitchellh/ghostty",
    "org.gtk.Actions",
    "Activate",
    "sava{sv}",
    "reload-config",
    "0",
    "0",
];

#[test]
fn linux_bus_only_adapter_reports_probe_and_action_failures_without_false_success() {
    let cases = [
        (
            vec![reply(false)],
            ReloadOutcome::Unavailable(ReloadUnavailableReason::GhosttyIntegrationUnavailable),
        ),
        (
            vec![Err(io::ErrorKind::PermissionDenied)],
            ReloadOutcome::Failed(ReloadFailure::Probe),
        ),
        (
            vec![reply(true), reply(false)],
            ReloadOutcome::Failed(ReloadFailure::Reload),
        ),
        (
            vec![reply(true), Err(io::ErrorKind::ConnectionReset)],
            ReloadOutcome::Failed(ReloadFailure::Reload),
        ),
        (vec![reply(true), reply(true)], ReloadOutcome::Succeeded),
    ];
    for (replies, expected) in cases {
        let count = replies.len();
        let commands = ScriptedCommands::new(false, replies);
        let adapter = SystemdReload::new(commands.clone(), "systemctl");
        assert_eq!(adapter.observation(), ReloadObservation::Systemd);
        assert!(
            commands.calls.borrow().is_empty(),
            "observation must not reload"
        );
        assert_eq!(adapter.reload(), expected);
        let calls = [BUS_PROBE, BUS_RELOAD];
        commands.assert_calls(&calls[..count]);
    }
}

#[test]
fn failed_active_service_reload_does_not_fall_through_to_a_different_bus_instance() {
    for failure in [reply(false), Err(io::ErrorKind::BrokenPipe)] {
        let commands = ScriptedCommands::new(true, [reply(true), failure]);
        let adapter = SystemdReload::new(commands.clone(), "systemctl");
        assert_eq!(
            adapter.reload(),
            ReloadOutcome::Failed(ReloadFailure::Reload)
        );
        commands.assert_calls(&[
            &[
                "systemctl",
                "--user",
                "is-active",
                "--quiet",
                "app-com.mitchellh.ghostty.service",
            ],
            &[
                "systemctl",
                "--user",
                "reload",
                "app-com.mitchellh.ghostty.service",
            ],
        ]);
    }
}

#[test]
fn failed_systemd_probe_does_not_fall_through_to_another_bus_instance() {
    for failure in [io::ErrorKind::PermissionDenied, io::ErrorKind::NotFound] {
        let commands = ScriptedCommands::new(true, [Err(failure)]);
        let adapter = SystemdReload::new(commands.clone(), "systemctl");
        assert_eq!(adapter.observation(), ReloadObservation::Systemd);
        assert!(commands.calls.borrow().is_empty());
        assert_eq!(
            adapter.reload(),
            ReloadOutcome::Failed(ReloadFailure::Probe)
        );
        commands.assert_calls(&[&[
            "systemctl",
            "--user",
            "is-active",
            "--quiet",
            "app-com.mitchellh.ghostty.service",
        ]]);
    }
}

struct LegacyCli {
    home: tempfile::TempDir,
    root: PathBuf,
    hook: PathBuf,
}

impl LegacyCli {
    fn new() -> Self {
        let home = tempfile::tempdir().unwrap();
        let root = home.path().join("xdg/ghostty/ghostty-wall");
        let hook = home.path().join("xdg/ghostty/config.ghostty");
        let fixture = Self { home, root, hook };
        fixture.ok(&["init"]);
        fs::remove_file(fixture.root.join("profiles/welcome.toml")).unwrap();
        fs::remove_file(fixture.root.join("profiles/welcome.png")).unwrap();
        fs::write(
            fixture.root.join("config.toml"),
            "schema_version = 1\n[sources.local]\nkind = \"local-directory\"\npath = \"profiles\"\n",
        )
        .unwrap();
        for (id, file, bytes, font) in [
            (
                "legacy",
                "legacy.png",
                include_bytes!("fixtures/white.png").as_slice(),
                11,
            ),
            (
                "boy",
                "boy.jpg",
                include_bytes!("fixtures/white.jpg").as_slice(),
                13,
            ),
        ] {
            fs::write(fixture.root.join("profiles").join(file), bytes).unwrap();
            fs::write(
                fixture.root.join(format!("profiles/{id}.toml")),
                format!(
                    "schema_version = 1\n[wallpaper]\nmode = \"source\"\nsource = \"local\"\nselection = \"path\"\npath = \"{file}\"\n[terminal]\nfont_size = {font}\n"
                ),
            )
            .unwrap();
        }
        fixture
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_ghostty-wall"))
            .args(args)
            .env_clear()
            .env("HOME", self.home.path())
            .env("XDG_CONFIG_HOME", self.home.path().join("xdg"))
            .env("XDG_RUNTIME_DIR", self.home.path().join("runtime"))
            .env("PATH", "/nonexistent")
            .current_dir(self.home.path())
            .output()
            .unwrap()
    }

    fn ok(&self, args: &[&str]) -> Output {
        let output = self.run(args);
        assert!(output.status.success(), "{args:?}: {output:?}");
        output
    }
}

fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn visit(root: &Path, directory: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(directory).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                visit(root, &entry.path(), files);
            } else {
                assert!(entry.file_type().unwrap().is_file());
                files.insert(
                    entry.path().strip_prefix(root).unwrap().to_owned(),
                    fs::read(entry.path()).unwrap(),
                );
            }
        }
    }
    let mut files = BTreeMap::new();
    visit(root, root, &mut files);
    files
}

#[test]
fn unavailable_draft_retry_then_failed_finish_restores_legacy_or_empty_history() {
    for active in [false, true] {
        let f = LegacyCli::new();
        if active {
            f.ok(&["apply", "legacy"]);
        }
        let plan: serde_json::Value =
            serde_json::from_slice(&f.ok(&["plan", "boy", "--json"]).stdout).unwrap();
        assert_eq!(plan["profile"]["schema_version"], 1);
        let mut json = plan["environment"]["manifest"].clone();
        json["terminal"]["font_size_millipoints"] = 19000.into();
        let draft = manifest::decode(&serde_json::to_vec(&json).unwrap()).unwrap();
        let before = snapshot(&f.root);
        let hook_before = fs::read(&f.hook).unwrap();
        let commands = ScriptedCommands::new(
            false,
            [
                reply(false),
                reply(true),
                reply(true),
                reply(true),
                reply(false),
            ],
        );
        let mut session = PreviewSession::begin(
            &f.root,
            &f.hook,
            SystemdReload::new(commands.clone(), "systemctl"),
        )
        .unwrap();
        assert_eq!(session.starting_activation().is_some(), active);
        if active {
            assert_eq!(
                session
                    .starting_activation()
                    .unwrap()
                    .profile_id()
                    .unwrap()
                    .as_str(),
                "legacy"
            );
        }
        assert!(matches!(
            session.update_with_image(&draft, include_bytes!("fixtures/white.png")),
            Err(PreviewError::InvalidImage)
        ));
        assert!(commands.calls.borrow().is_empty());
        let mut after_rejection = snapshot(&f.root);
        assert!(
            after_rejection
                .remove(Path::new("preview.session"))
                .is_some()
        );
        assert_eq!(after_rejection, before);

        assert_eq!(
            session
                .update_with_image(&draft, include_bytes!("fixtures/white.jpg"))
                .unwrap(),
            ReloadOutcome::Unavailable(ReloadUnavailableReason::GhosttyIntegrationUnavailable)
        );
        let provisional = snapshot(&f.root);
        assert!(provisional.contains_key(Path::new("preview.session")));
        assert!(
            String::from_utf8(provisional[Path::new("current.ghostty")].clone())
                .unwrap()
                .contains(".tmp-preview-image-")
        );
        assert_eq!(
            provisional
                .keys()
                .filter(|path| {
                    path.file_name()
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .starts_with(".tmp-preview-image-")
                })
                .count(),
            1
        );
        for (path, bytes) in &before {
            if path != Path::new("current.ghostty") {
                assert_eq!(provisional[path], *bytes);
            }
        }
        assert_eq!(
            session
                .update_with_image(&draft, include_bytes!("fixtures/white.jpg"))
                .unwrap(),
            ReloadOutcome::Succeeded
        );
        assert_eq!(
            snapshot(&f.root),
            provisional,
            "retry must reuse staged bytes"
        );
        assert_eq!(
            session.finish().unwrap(),
            ReloadOutcome::Failed(ReloadFailure::Reload)
        );
        commands.assert_calls(&[BUS_PROBE, BUS_PROBE, BUS_RELOAD, BUS_PROBE, BUS_RELOAD]);
        assert_eq!(
            snapshot(&f.root),
            before,
            "finish restores files, not verified pixels"
        );
        assert_eq!(fs::read(&f.hook).unwrap(), hook_before);

        let commands = ScriptedCommands::new(false, [reply(false)]);
        let next = PreviewSession::begin(
            &f.root,
            &f.hook,
            SystemdReload::new(commands.clone(), "systemctl"),
        )
        .unwrap();
        assert_eq!(
            next.cancel().unwrap(),
            ReloadOutcome::Unavailable(ReloadUnavailableReason::GhosttyIntegrationUnavailable)
        );
        commands.assert_calls(&[BUS_PROBE]);
        assert_eq!(snapshot(&f.root), before);
        assert!(!f.root.join("profiles/welcome.toml").exists());
        assert_eq!(
            inspect_history(&f.root).unwrap().activations().len(),
            usize::from(active)
        );
    }
}

#[test]
fn failed_second_draft_reload_leaves_provisional_files_until_explicit_cancel() {
    for systemctl in [false, true] {
        let f = LegacyCli::new();
        f.ok(&["apply", "legacy"]);
        let plan: serde_json::Value =
            serde_json::from_slice(&f.ok(&["plan", "boy", "--json"]).stdout).unwrap();
        let mut json = plan["environment"]["manifest"].clone();
        let before = snapshot(&f.root);
        let hook_before = fs::read(&f.hook).unwrap();
        let commands = ScriptedCommands::new(
            systemctl,
            [
                reply(true),
                reply(true),
                reply(true),
                reply(false),
                reply(true),
                reply(true),
            ],
        );
        let mut session = PreviewSession::begin(
            &f.root,
            &f.hook,
            SystemdReload::new(commands.clone(), "systemctl"),
        )
        .unwrap();
        assert_eq!(
            session
                .starting_activation()
                .unwrap()
                .profile_id()
                .unwrap()
                .as_str(),
            "legacy"
        );
        json["terminal"]["font_size_millipoints"] = 17000.into();
        let first = manifest::decode(&serde_json::to_vec(&json).unwrap()).unwrap();
        assert_eq!(
            session
                .update_with_image(&first, include_bytes!("fixtures/white.jpg"))
                .unwrap(),
            ReloadOutcome::Succeeded
        );
        let first_files = snapshot(&f.root);
        json["terminal"]["font_size_millipoints"] = 19000.into();
        let second = manifest::decode(&serde_json::to_vec(&json).unwrap()).unwrap();
        assert_eq!(
            session
                .update_with_image(&second, include_bytes!("fixtures/white.jpg"))
                .unwrap(),
            ReloadOutcome::Failed(ReloadFailure::Reload)
        );

        // Failed action delivery is not a file rollback or evidence of the window's pixels.
        let after_failure = snapshot(&f.root);
        assert_eq!(
            after_failure.keys().collect::<Vec<_>>(),
            first_files.keys().collect::<Vec<_>>()
        );
        for (path, bytes) in &first_files {
            if path != Path::new("current.ghostty") {
                assert_eq!(after_failure[path], *bytes);
            }
        }
        let projection = &after_failure[Path::new("current.ghostty")];
        assert_ne!(*projection, first_files[Path::new("current.ghostty")]);
        assert_ne!(*projection, before[Path::new("current.ghostty")]);
        assert!(String::from_utf8_lossy(projection).contains("font-size = 19.000"));
        for (path, bytes) in &before {
            if path != Path::new("current.ghostty") {
                assert_eq!(after_failure[path], *bytes);
            }
        }
        assert!(after_failure.contains_key(Path::new("preview.session")));
        assert_eq!(session.cancel().unwrap(), ReloadOutcome::Succeeded);
        assert_eq!(snapshot(&f.root), before);
        assert_eq!(fs::read(&f.hook).unwrap(), hook_before);
        assert_eq!(inspect_history(&f.root).unwrap().activations().len(), 1);
        assert!(!f.root.join("profiles/welcome.toml").exists());
        let (probe, reload): (&[&str], &[&str]) = if systemctl {
            (
                &[
                    "systemctl",
                    "--user",
                    "is-active",
                    "--quiet",
                    "app-com.mitchellh.ghostty.service",
                ],
                &[
                    "systemctl",
                    "--user",
                    "reload",
                    "app-com.mitchellh.ghostty.service",
                ],
            )
        } else {
            (BUS_PROBE, BUS_RELOAD)
        };
        commands.assert_calls(&[probe, reload, probe, reload, probe, reload]);
    }
}

#[test]
fn failed_cancel_after_replay_and_source_loss_restores_history_not_saved_profile() {
    let f = LegacyCli::new();
    f.ok(&["apply", "legacy"]);
    let original_projection = fs::read(f.root.join("current.ghostty")).unwrap();
    f.ok(&["apply", "boy"]);
    f.ok(&["previous"]);
    let replay = inspect_history(&f.root).unwrap().latest().unwrap().clone();
    assert_eq!(replay.sequence(), 3);
    assert_eq!(replay.history_cursor(), 1);
    assert!(replay.profile_id().is_none());
    assert_eq!(
        fs::read(f.root.join("current.ghostty")).unwrap(),
        original_projection
    );
    let plan: serde_json::Value =
        serde_json::from_slice(&f.ok(&["plan", "boy", "--json"]).stdout).unwrap();
    assert_eq!(plan["profile"]["schema_version"], 1);
    let mut json = plan["environment"]["manifest"].clone();
    json["terminal"]["font_size_millipoints"] = 19000.into();
    let draft = manifest::decode(&serde_json::to_vec(&json).unwrap()).unwrap();
    assert_ne!(draft.wallpaper(), replay.environment().wallpaper());
    fs::remove_file(f.root.join("profiles/legacy.toml")).unwrap();
    fs::remove_file(f.root.join("profiles/legacy.png")).unwrap();
    let before = snapshot(&f.root);
    let hook_before = fs::read(&f.hook).unwrap();
    let commands =
        ScriptedCommands::new(false, [reply(true), reply(true), reply(true), reply(false)]);
    let mut session = PreviewSession::begin(
        &f.root,
        &f.hook,
        SystemdReload::new(commands.clone(), "systemctl"),
    )
    .unwrap();
    let starting = session.starting_activation().unwrap();
    assert_eq!(starting.sequence(), replay.sequence());
    assert_eq!(starting.environment_id(), replay.environment_id());
    assert!(starting.profile_id().is_none());
    assert_eq!(
        session
            .update_with_image(&draft, include_bytes!("fixtures/white.jpg"))
            .unwrap(),
        ReloadOutcome::Succeeded
    );
    let provisional = snapshot(&f.root);
    assert_ne!(
        provisional[Path::new("current.ghostty")],
        original_projection
    );
    assert!(provisional.contains_key(Path::new("preview.session")));
    for (path, bytes) in &before {
        if path != Path::new("current.ghostty") {
            assert_eq!(provisional[path], *bytes);
        }
    }
    assert_eq!(
        session.cancel().unwrap(),
        ReloadOutcome::Failed(ReloadFailure::Reload)
    );
    commands.assert_calls(&[BUS_PROBE, BUS_RELOAD, BUS_PROBE, BUS_RELOAD]);
    assert_eq!(snapshot(&f.root), before);
    assert_eq!(fs::read(&f.hook).unwrap(), hook_before);
    assert!(!f.root.join("profiles/welcome.toml").exists());
    assert!(!f.root.join("profiles/legacy.toml").exists());
    assert!(!f.root.join("profiles/legacy.png").exists());
    let history = inspect_history(&f.root).unwrap();
    assert_eq!(history.activations().len(), 3);
    assert_eq!(
        history.latest().unwrap().environment_id(),
        replay.environment_id()
    );
    assert!(history.latest().unwrap().profile_id().is_none());
    f.ok(&["preview", "boy"]);
    assert_eq!(snapshot(&f.root), before);
}

#[test]
fn older_dependency_loss_during_preview_preserves_evidence_and_prevents_restore_reload() {
    for (ending, damaged_kind) in ["cancel", "finish", "drop"]
        .into_iter()
        .flat_map(|ending| ["asset", "environment"].map(|kind| (ending, kind)))
    {
        let f = LegacyCli::new();
        f.ok(&["apply", "legacy"]);
        f.ok(&["apply", "boy"]);
        let plan: serde_json::Value =
            serde_json::from_slice(&f.ok(&["plan", "legacy", "--json"]).stdout).unwrap();
        let draft =
            manifest::decode(&serde_json::to_vec(&plan["environment"]["manifest"]).unwrap())
                .unwrap();
        let before = snapshot(&f.root);
        let hook_before = fs::read(&f.hook).unwrap();
        let history = inspect_history(&f.root).unwrap();
        let active = history.latest().unwrap();
        assert_eq!(active.profile_id().unwrap().as_str(), "boy");
        assert_ne!(active.environment(), &draft);
        let older: serde_json::Value = serde_json::from_slice(
            &before[Path::new("history/activations/act-v1-0000000000000001.json")],
        )
        .unwrap();
        assert_ne!(older["environment_id"], active.environment_id().to_string());
        let dependency = if damaged_kind == "asset" {
            let digest = older["asset"]["sha256"].as_str().unwrap();
            PathBuf::from(format!("assets/sha256/{}/{digest}.png", &digest[..2]))
        } else {
            PathBuf::from(format!(
                "environments/{}.json",
                older["environment_id"].as_str().unwrap()
            ))
        };
        let commands = ScriptedCommands::new(false, [reply(true), reply(true)]);
        let mut session = PreviewSession::begin(
            &f.root,
            &f.hook,
            SystemdReload::new(commands.clone(), "systemctl"),
        )
        .unwrap();
        assert_eq!(
            session
                .update_with_image(&draft, include_bytes!("fixtures/white.png"))
                .unwrap(),
            ReloadOutcome::Succeeded
        );
        assert!(
            fs::read_to_string(f.root.join("current.ghostty"))
                .unwrap()
                .contains(".tmp-preview-image-")
        );

        // Simulate external damage, not a cooperating writer: even a non-latest
        // dependency must validate before restoration or cleanup (RFCs 0006/0009).
        if damaged_kind == "asset" {
            fs::remove_file(f.root.join(&dependency)).unwrap();
        } else {
            fs::write(f.root.join(&dependency), b"{").unwrap();
        }
        let damaged = snapshot(&f.root);
        assert!(matches!(
            session.update_with_image(&draft, include_bytes!("fixtures/white.png")),
            Err(PreviewError::History(_))
        ));
        match ending {
            "cancel" => assert!(matches!(session.cancel(), Err(PreviewError::History(_)))),
            "finish" => assert!(matches!(session.finish(), Err(PreviewError::History(_)))),
            "drop" => drop(session),
            _ => unreachable!(),
        }
        commands.assert_calls(&[BUS_PROBE, BUS_RELOAD]);
        assert_eq!(snapshot(&f.root), damaged, "{ending}, {damaged_kind}");
        assert!(f.root.join("preview.session").exists());
        assert!(inspect_recovery_state(&f.root, &f.hook).is_err());
        assert!(reconcile_recovery_state(&f.root, &f.hook).is_err());
        let retry_commands = ScriptedCommands::new(false, []);
        assert!(
            PreviewSession::begin(
                &f.root,
                &f.hook,
                SystemdReload::new(retry_commands.clone(), "systemctl"),
            )
            .is_err()
        );
        retry_commands.assert_calls(&[]);
        assert_eq!(snapshot(&f.root), damaged);
        assert_eq!(fs::read(&f.hook).unwrap(), hook_before);

        // Restore only known fixture bytes; recovery must never adopt the draft's
        // duplicate PNG as a repair for the lost committed dependency.
        fs::write(f.root.join(&dependency), &before[&dependency]).unwrap();
        reconcile_recovery_state(&f.root, &f.hook).unwrap();
        assert_eq!(snapshot(&f.root), before);
        assert_eq!(inspect_history(&f.root).unwrap().activations().len(), 2);
        commands.assert_calls(&[BUS_PROBE, BUS_RELOAD]);
        assert!(!f.root.join("profiles/welcome.toml").exists());
        assert!(!f.root.join("preview.session").exists());
    }
}

#[test]
fn blocked_image_cleanup_never_reloads_or_claims_completed_restoration() {
    for (active, ending) in [false, true]
        .into_iter()
        .flat_map(|active| ["cancel", "finish", "drop"].map(|ending| (active, ending)))
    {
        let f = LegacyCli::new();
        if active {
            f.ok(&["apply", "legacy"]);
        }
        let plan: serde_json::Value =
            serde_json::from_slice(&f.ok(&["plan", "boy", "--json"]).stdout).unwrap();
        let draft =
            manifest::decode(&serde_json::to_vec(&plan["environment"]["manifest"]).unwrap())
                .unwrap();
        let before = snapshot(&f.root);
        let hook_before = fs::read(&f.hook).unwrap();
        let commands = ScriptedCommands::new(false, [reply(true), reply(true)]);
        let mut session = PreviewSession::begin(
            &f.root,
            &f.hook,
            SystemdReload::new(commands.clone(), "systemctl"),
        )
        .unwrap();
        assert_eq!(session.starting_activation().is_some(), active);
        assert_eq!(
            session
                .update_with_image(&draft, include_bytes!("fixtures/white.jpg"))
                .unwrap(),
            ReloadOutcome::Succeeded
        );
        let provisional = snapshot(&f.root);
        let image = provisional
            .keys()
            .find(|path| path.to_str().unwrap().starts_with(".tmp-preview-image-"))
            .unwrap();
        let path = f.root.join(image);
        // A replaced entry is not authority to recursively delete its contents.
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        fs::write(path.join("keep"), b"preserve obstruction").unwrap();
        match ending {
            "cancel" => assert!(matches!(session.cancel(), Err(PreviewError::Recovery(_)))),
            "finish" => assert!(matches!(session.finish(), Err(PreviewError::Recovery(_)))),
            "drop" => drop(session),
            _ => unreachable!(),
        }
        let mut expected = before.clone();
        expected.insert(
            PathBuf::from("preview.session"),
            provisional[Path::new("preview.session")].clone(),
        );
        expected.insert(image.join("keep"), b"preserve obstruction".to_vec());
        assert_eq!(snapshot(&f.root), expected, "{active}, {ending}");
        commands.assert_calls(&[BUS_PROBE, BUS_RELOAD]);
        inspect_recovery_state(&f.root, &f.hook).unwrap();
        assert!(reconcile_recovery_state(&f.root, &f.hook).is_err());
        let retry_commands = ScriptedCommands::new(false, []);
        assert!(
            PreviewSession::begin(
                &f.root,
                &f.hook,
                SystemdReload::new(retry_commands.clone(), "systemctl"),
            )
            .is_err()
        );
        retry_commands.assert_calls(&[]);
        assert_eq!(snapshot(&f.root), expected);
        assert_eq!(fs::read(&f.hook).unwrap(), hook_before);
        assert!(path.is_dir());

        fs::remove_file(path.join("keep")).unwrap();
        fs::remove_dir(path).unwrap();
        for _ in 0..2 {
            reconcile_recovery_state(&f.root, &f.hook).unwrap();
            assert_eq!(snapshot(&f.root), before);
        }
        commands.assert_calls(&[BUS_PROBE, BUS_RELOAD]);
        assert_eq!(
            inspect_history(&f.root).unwrap().activations().len(),
            usize::from(active)
        );
        assert!(!f.root.join("profiles/welcome.toml").exists());
    }
}

#[test]
fn linux_cli_preview_is_non_live_and_unavailable_or_failed_reload_preserves_activation() {
    for route in [
        "no-commands",
        "unavailable",
        "systemd",
        "dbus",
        "systemd-spawn-failure",
        "dbus-spawn-failure",
    ] {
        assert_cli_reload_failure_preserves_activation(route);
    }
}

fn assert_cli_reload_failure_preserves_activation(route: &str) {
    let f = LegacyCli::new();
    f.ok(&["apply", "legacy"]);
    let before = snapshot(&f.root);
    let hook_before = fs::read(&f.hook).unwrap();
    let bin = f.home.path().join("recording-bin");
    let calls = f.home.path().join("runtime-calls");
    fs::create_dir(&bin).unwrap();
    // These stubs cannot contact Ghostty or a service manager. A missing interpreter
    // exercises a real spawn error despite a positive executable-availability check;
    // unlike an inactive service, this must not fall through to a different instance.
    for program in ["systemctl", "busctl"] {
        if route == "no-commands" {
            continue;
        }
        let path = bin.join(program);
        let spawn_failure = matches!(
            (route, program),
            ("systemd-spawn-failure", "systemctl") | ("dbus-spawn-failure", "busctl")
        );
        let script = if spawn_failure {
            format!("#!{}\nexit 99\n", bin.join("missing-interpreter").display())
        } else {
            "#!/bin/sh\nprintf '%s %s\\n' \"${0##*/}\" \"$*\" >> \"$GW_QA_RUNTIME_CALLS\"\ncase \"$GW_QA_RELOAD_ROUTE:${0##*/}:$2\" in\n  systemd:systemctl:is-active|dbus:busctl:--quiet) exit 0 ;;\n  *) exit 1 ;;\nesac\n".to_owned()
        };
        fs::write(&path, script).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let invoke = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_ghostty-wall"))
            .args(args)
            .env_clear()
            .env("HOME", f.home.path())
            .env("XDG_CONFIG_HOME", f.home.path().join("xdg"))
            .env("XDG_RUNTIME_DIR", f.home.path().join("runtime"))
            .env("PATH", &bin)
            .env("GW_QA_RUNTIME_CALLS", &calls)
            .env("GW_QA_RELOAD_ROUTE", route)
            .current_dir(f.home.path())
            .output()
            .unwrap()
    };
    let run = |args: &[&str]| {
        let output = invoke(args);
        assert!(output.status.success(), "{args:?}: {output:?}");
        assert!(output.stderr.is_empty(), "{args:?}: {output:?}");
        output
    };
    let plan: serde_json::Value =
        serde_json::from_slice(&run(&["plan", "boy", "--json"]).stdout).unwrap();
    let reload = plan["operations"].as_array().unwrap().last().unwrap();
    assert_eq!(reload["kind"], "reload_ghostty");
    if route == "no-commands" {
        assert_eq!(reload["adapter"], "unavailable");
        assert_eq!(reload["reason"], "adapter-command-unavailable");
    } else {
        assert_eq!(reload["adapter"], "systemd");
        assert!(reload.get("reason").is_none());
    }
    assert_eq!(reload["required"], false);
    assert_eq!(plan["profile"]["schema_version"], 1);
    run(&["preview", "boy"]);
    for args in [&["--help"][..], &["edit", "--help"][..]] {
        let output = run(args);
        assert!(
            String::from_utf8_lossy(&output.stdout)
                .to_ascii_lowercase()
                .contains("not live ghostty reload")
        );
    }
    assert!(
        !calls.exists(),
        "read-only commands must not execute runtime actions"
    );
    assert_eq!(snapshot(&f.root), before);

    for (args, sequence) in [(&["apply", "boy"][..], 2), (&["previous"][..], 3)] {
        let committed = snapshot(&f.root.join("history/activations"));
        let output = run(args);
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.starts_with("Configuration updated; reload Ghostty manually."));
        assert!(text.contains("Activation remains committed"));
        assert!(text.contains("Reload details:"));
        assert!(!text.contains("reload: succeeded"));
        let history = inspect_history(&f.root).unwrap();
        assert_eq!(history.latest().unwrap().sequence(), sequence);
        assert_eq!(history.activations().len() as u64, sequence);
        for (path, bytes) in committed {
            assert_eq!(
                fs::read(f.root.join("history/activations").join(path)).unwrap(),
                bytes
            );
        }
    }
    let mut expected =
        "systemctl --user is-active --quiet app-com.mitchellh.ghostty.service\n".to_owned();
    match route {
        "systemd" => {
            expected.push_str("systemctl --user reload app-com.mitchellh.ghostty.service\n")
        }
        "unavailable" | "dbus" => {
            expected.push_str("busctl --user --quiet status com.mitchellh.ghostty\n");
            if route == "dbus" {
                expected.push_str(&BUS_RELOAD.join(" "));
                expected.push('\n');
            }
        }
        "no-commands" | "systemd-spawn-failure" => expected.clear(),
        "dbus-spawn-failure" => (),
        _ => unreachable!(),
    }
    let recorded = if calls.exists() {
        fs::read_to_string(&calls).unwrap()
    } else {
        String::new()
    };
    assert_eq!(recorded, expected.repeat(2), "{route}");
    for (path, bytes) in before {
        assert_eq!(fs::read(f.root.join(path)).unwrap(), bytes);
    }
    assert_eq!(fs::read(&f.hook).unwrap(), hook_before);
    assert!(!f.root.join("profiles/welcome.toml").exists());
    assert!(!f.root.join("preview.session").exists());

    // A runnable adapter is not a substitute for the effective integration hook.
    // On drift, even a runtime availability probe must not execute (RFC 0007).
    let committed = snapshot(&f.root);
    for drift in ["font-size = 9\n", "config-file = ?other.ghostty\n"] {
        fs::write(&f.hook, drift).unwrap();
        for args in [
            &["plan", "boy", "--json"][..],
            &["preview", "boy"][..],
            &["apply", "boy"][..],
            &["previous"][..],
        ] {
            let output = invoke(args);
            assert!(!output.status.success(), "{route} {args:?}: {output:?}");
            if args[0] == "plan" {
                assert_eq!(output.status.code(), Some(4));
                assert!(output.stderr.is_empty());
                let error: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
                assert_eq!(error["error"]["category"], "resolution");
                assert_eq!(error["error"]["code"], "integration.hook-drift");
            } else {
                assert!(output.stdout.is_empty());
                assert!(String::from_utf8_lossy(&output.stderr).contains("integration drift"));
            }
            assert_eq!(snapshot(&f.root), committed);
            assert_eq!(fs::read_to_string(&f.hook).unwrap(), drift);
            let after_calls = if calls.exists() {
                fs::read_to_string(&calls).unwrap()
            } else {
                String::new()
            };
            assert_eq!(after_calls, recorded, "{route} {args:?}");
        }
    }

    fs::write(&f.hook, &hook_before).unwrap();
    let record = Path::new("history/activations/act-v1-0000000000000001.json");
    let mut unsupported: serde_json::Value = serde_json::from_slice(&committed[record]).unwrap();
    unsupported["record_schema_version"] = 99.into();
    // Even an old, non-latest Activation must validate before any reload (RFCs 0006/0007).
    for invalid in [b"{".to_vec(), serde_json::to_vec(&unsupported).unwrap()] {
        fs::write(f.root.join(record), invalid).unwrap();
        let corrupt_state = snapshot(&f.root);
        for args in [
            &["plan", "boy", "--json"][..],
            &["preview", "boy"][..],
            &["apply", "boy"][..],
            &["previous"][..],
        ] {
            let output = invoke(args);
            assert!(!output.status.success(), "{route} {args:?}: {output:?}");
            if args[0] == "plan" {
                assert_eq!(output.status.code(), Some(5));
                assert!(output.stderr.is_empty());
                let error: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
                assert_eq!(error["error"]["category"], "corruption");
                assert_eq!(error["error"]["code"], "durable-state.corrupt");
                assert!(error.get("environment").is_none(), "no partial Plan");
            } else {
                assert!(output.stdout.is_empty());
                assert!(!output.stderr.is_empty());
            }
            assert_eq!(snapshot(&f.root), corrupt_state);
            assert_eq!(fs::read(&f.hook).unwrap(), hook_before);
            let after_calls = if calls.exists() {
                fs::read_to_string(&calls).unwrap()
            } else {
                String::new()
            };
            assert_eq!(after_calls, recorded, "{route} {args:?}");
        }
        let commands = ScriptedCommands::new(false, []);
        assert!(
            PreviewSession::begin(
                &f.root,
                &f.hook,
                SystemdReload::new(commands.clone(), "systemctl"),
            )
            .is_err()
        );
        commands.assert_calls(&[]);
        assert_eq!(snapshot(&f.root), corrupt_state);
        assert_eq!(fs::read(&f.hook).unwrap(), hook_before);
        assert!(!f.root.join("preview.session").exists());
    }
}

#[test]
fn delivered_other_profile_draft_then_failed_cancel_restores_legacy_files_not_verified_pixels() {
    let cases = [
        (
            vec![reply(false)],
            ReloadOutcome::Unavailable(ReloadUnavailableReason::GhosttyIntegrationUnavailable),
        ),
        (
            vec![Err(io::ErrorKind::ConnectionRefused)],
            ReloadOutcome::Failed(ReloadFailure::Probe),
        ),
        (
            vec![reply(true), reply(false)],
            ReloadOutcome::Failed(ReloadFailure::Reload),
        ),
    ];
    for (cancel_replies, expected) in cases {
        let f = LegacyCli::new();
        let applied = f.ok(&["apply", "legacy"]);
        let text = String::from_utf8(applied.stdout).unwrap();
        assert!(text.contains("Activated act-v1-0000000000000001"));
        assert!(text.starts_with("Configuration updated; reload Ghostty manually."));
        assert!(text.contains("unavailable; Activation remains committed"));
        let plan: serde_json::Value =
            serde_json::from_slice(&f.ok(&["plan", "boy", "--json"]).stdout).unwrap();
        assert_eq!(plan["profile"]["schema_version"], 1);
        let mut json = plan["environment"]["manifest"].clone();
        json["terminal"]["font_size_millipoints"] = 17000.into();
        let draft = manifest::decode(&serde_json::to_vec(&json).unwrap()).unwrap();
        let before = snapshot(&f.root);
        let hook_before = fs::read(&f.hook).unwrap();
        let count = cancel_replies.len();
        let commands = ScriptedCommands::new(
            false,
            [reply(true), reply(true)].into_iter().chain(cancel_replies),
        );
        let adapter = SystemdReload::new(commands.clone(), "systemctl");
        let mut session = PreviewSession::begin(&f.root, &f.hook, adapter).unwrap();
        assert_eq!(
            session
                .starting_activation()
                .unwrap()
                .profile_id()
                .unwrap()
                .as_str(),
            "legacy"
        );
        assert_eq!(
            session
                .update_with_image(&draft, include_bytes!("fixtures/white.jpg"))
                .unwrap(),
            ReloadOutcome::Succeeded
        );
        assert_ne!(
            fs::read(f.root.join("current.ghostty")).unwrap(),
            before[Path::new("current.ghostty")]
        );
        for (path, bytes) in &before {
            if path != Path::new("current.ghostty") {
                assert_eq!(fs::read(f.root.join(path)).unwrap(), *bytes);
            }
        }
        assert_eq!(session.cancel().unwrap(), expected);
        commands.assert_calls(&[BUS_PROBE, BUS_RELOAD, BUS_PROBE, BUS_RELOAD][..2 + count]);
        assert_eq!(
            snapshot(&f.root),
            before,
            "file restoration is not pixel restoration"
        );
        assert_eq!(fs::read(&f.hook).unwrap(), hook_before);

        f.ok(&["preview", "boy"]);
        assert_eq!(snapshot(&f.root), before);
        let profile = f.root.join("profiles/boy.toml");
        fs::set_permissions(&profile, fs::Permissions::from_mode(0o622)).unwrap();
        let failed = f.run(&["edit", "boy", "terminal.font_size", "17"]);
        assert!(!failed.status.success());
        assert_eq!(
            snapshot(&f.root),
            before,
            "failed save must preserve committed state"
        );
        fs::set_permissions(&profile, fs::Permissions::from_mode(0o600)).unwrap();
        f.ok(&["edit", "boy", "terminal.font_size", "17"]);
        assert_eq!(inspect_history(&f.root).unwrap().activations().len(), 1);
        assert!(
            fs::read_to_string(&profile)
                .unwrap()
                .contains("schema_version = 1")
        );
        f.ok(&["apply", "boy"]);
        assert_eq!(inspect_history(&f.root).unwrap().activations().len(), 2);
        f.ok(&["previous"]);
        assert_eq!(inspect_history(&f.root).unwrap().activations().len(), 3);
        assert_eq!(
            fs::read(f.root.join("current.ghostty")).unwrap(),
            before[Path::new("current.ghostty")]
        );
        assert_eq!(
            snapshot(&f.root.join("history/activations"))
                [Path::new("act-v1-0000000000000001.json")],
            before[Path::new("history/activations/act-v1-0000000000000001.json")]
        );
        assert!(!f.root.join("profiles/welcome.toml").exists());
        assert!(!f.root.join("preview.session").exists());
        assert!(
            !fs::read_to_string(f.root.join("current.ghostty"))
                .unwrap()
                .contains(".tmp-preview")
        );
    }
}
