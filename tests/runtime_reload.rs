use std::{cell::RefCell, collections::VecDeque, io, path::Path, rc::Rc};

use ghostty_wall::{
    plan::{ReloadObservation, ReloadUnavailableReason},
    runtime::{
        AppleScriptReload, CommandResult, CommandRunner, ReloadAdapter, ReloadFailure,
        ReloadOutcome, SystemdReload,
    },
};

#[derive(Clone)]
struct RecordingRunner {
    available: bool,
    calls: Rc<RefCell<Vec<Vec<String>>>>,
    results: Rc<RefCell<VecDeque<Result<CommandResult, io::ErrorKind>>>>,
}

impl RecordingRunner {
    fn new(
        available: bool,
        results: impl IntoIterator<Item = Result<CommandResult, io::ErrorKind>>,
    ) -> Self {
        Self {
            available,
            calls: Rc::new(RefCell::new(Vec::new())),
            results: Rc::new(RefCell::new(results.into_iter().collect())),
        }
    }

    fn calls(&self) -> Vec<Vec<String>> {
        self.calls.borrow().clone()
    }
}

impl CommandRunner for RecordingRunner {
    fn available(&self, _program: &Path) -> bool {
        self.available
    }

    fn run(&self, program: &Path, args: &[&str]) -> io::Result<CommandResult> {
        let mut call = vec![program.display().to_string()];
        call.extend(args.iter().map(|argument| (*argument).to_owned()));
        self.calls.borrow_mut().push(call);
        self.results
            .borrow_mut()
            .pop_front()
            .expect("recorded command result")
            .map_err(io::Error::from)
    }
}

#[test]
fn systemd_adapter_uses_documented_user_service_reload() {
    let runner = RecordingRunner::new(
        true,
        [
            Ok(CommandResult::new(true, [])),
            Ok(CommandResult::new(true, [])),
        ],
    );
    let adapter = SystemdReload::new(runner.clone(), "systemctl");

    assert_eq!(adapter.observation(), ReloadObservation::Systemd);
    assert_eq!(adapter.reload(), ReloadOutcome::Succeeded);
    assert_eq!(
        runner.calls(),
        vec![
            vec![
                "systemctl".to_owned(),
                "--user".to_owned(),
                "is-active".to_owned(),
                "--quiet".to_owned(),
                "app-com.mitchellh.ghostty.service".to_owned(),
            ],
            vec![
                "systemctl".to_owned(),
                "--user".to_owned(),
                "reload".to_owned(),
                "app-com.mitchellh.ghostty.service".to_owned(),
            ],
        ]
    );
}

#[test]
fn systemd_adapter_classifies_unavailable_and_failed_runtime() {
    let missing = RecordingRunner::new(false, []);
    let adapter = SystemdReload::new(missing.clone(), "systemctl");
    assert_eq!(
        adapter.observation(),
        ReloadObservation::Unavailable(ReloadUnavailableReason::AdapterCommandUnavailable)
    );
    assert_eq!(
        adapter.reload(),
        ReloadOutcome::Unavailable(ReloadUnavailableReason::AdapterCommandUnavailable)
    );
    assert!(missing.calls().is_empty());

    let inactive = RecordingRunner::new(
        true,
        [
            Ok(CommandResult::new(false, [])),
            Ok(CommandResult::new(false, [])),
        ],
    );
    let adapter = SystemdReload::new(inactive.clone(), "systemctl");
    assert_eq!(
        adapter.reload(),
        ReloadOutcome::Unavailable(ReloadUnavailableReason::GhosttyIntegrationUnavailable)
    );

    let failed = RecordingRunner::new(
        true,
        [
            Ok(CommandResult::new(true, [])),
            Ok(CommandResult::new(false, [])),
        ],
    );
    let adapter = SystemdReload::new(failed, "systemctl");
    assert_eq!(
        adapter.reload(),
        ReloadOutcome::Failed(ReloadFailure::Reload)
    );
}

#[test]
fn systemd_adapter_reloads_desktop_launched_ghostty_via_dbus() {
    let runner = RecordingRunner::new(
        true,
        [
            Ok(CommandResult::new(false, [])),
            Ok(CommandResult::new(true, [])),
            Ok(CommandResult::new(true, [])),
        ],
    );
    let adapter = SystemdReload::new(runner.clone(), "systemctl");
    assert_eq!(adapter.reload(), ReloadOutcome::Succeeded);
    assert_eq!(
        runner.calls(),
        vec![
            vec![
                "systemctl",
                "--user",
                "is-active",
                "--quiet",
                "app-com.mitchellh.ghostty.service"
            ],
            vec![
                "busctl",
                "--user",
                "--quiet",
                "status",
                "com.mitchellh.ghostty"
            ],
            vec![
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
                "0"
            ],
        ]
        .into_iter()
        .map(|args| args.into_iter().map(str::to_owned).collect())
        .collect::<Vec<Vec<String>>>()
    );
}

#[test]
fn applescript_adapter_uses_ghostty_action_api() {
    let runner = RecordingRunner::new(
        true,
        [
            Ok(CommandResult::new(true, b"true\n")),
            Ok(CommandResult::new(true, b"true\n")),
        ],
    );
    let adapter = AppleScriptReload::new(runner.clone(), "osascript");

    assert_eq!(adapter.observation(), ReloadObservation::Applescript);
    assert_eq!(adapter.reload(), ReloadOutcome::Succeeded);
    assert_eq!(
        runner.calls(),
        vec![
            vec![
                "osascript".to_owned(),
                "-e".to_owned(),
                "application \"Ghostty\" is running".to_owned(),
            ],
            vec![
                "osascript".to_owned(),
                "-e".to_owned(),
                "tell application \"Ghostty\" to perform action \"reload_config\" on terminal 1"
                    .to_owned(),
            ],
        ]
    );
}

#[test]
fn applescript_adapter_classifies_stopped_probe_and_reload_failure() {
    let stopped = RecordingRunner::new(true, [Ok(CommandResult::new(true, b"false\n"))]);
    let adapter = AppleScriptReload::new(stopped, "osascript");
    assert_eq!(
        adapter.reload(),
        ReloadOutcome::Unavailable(ReloadUnavailableReason::GhosttyIntegrationUnavailable)
    );

    let probe_failed = RecordingRunner::new(true, [Err(io::ErrorKind::PermissionDenied)]);
    let adapter = AppleScriptReload::new(probe_failed, "osascript");
    assert_eq!(
        adapter.reload(),
        ReloadOutcome::Failed(ReloadFailure::Probe)
    );

    let reload_failed = RecordingRunner::new(
        true,
        [
            Ok(CommandResult::new(true, b"true\n")),
            Ok(CommandResult::new(true, b"false\n")),
        ],
    );
    let adapter = AppleScriptReload::new(reload_failed, "osascript");
    assert_eq!(
        adapter.reload(),
        ReloadOutcome::Failed(ReloadFailure::Reload)
    );
}
