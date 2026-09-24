//! Best-effort Ghostty runtime reload adapters.

use std::{
    env, io,
    path::{Path, PathBuf},
    process::Command,
};

use crate::plan::{ReloadObservation, ReloadUnavailableReason};

/// Ghostty's documented Linux user service.
pub const GHOSTTY_SYSTEMD_SERVICE: &str = "app-com.mitchellh.ghostty.service";
const GHOSTTY_DBUS_NAME: &str = "com.mitchellh.ghostty";

const GHOSTTY_RUNNING_SCRIPT: &str = "application \"Ghostty\" is running";
const GHOSTTY_RELOAD_SCRIPT: &str =
    "tell application \"Ghostty\" to perform action \"reload_config\" on terminal 1";

/// Stage at which an available runtime adapter failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReloadFailure {
    /// Runtime availability probe could not complete.
    Probe,
    /// Runtime accepted a reload attempt but it failed.
    Reload,
}

/// Best-effort runtime result, separate from durable Activation success.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReloadOutcome {
    /// Running Ghostty accepted the reload action.
    Succeeded,
    /// Reload could not be attempted on the observed runtime.
    Unavailable(ReloadUnavailableReason),
    /// An available adapter or runtime failed.
    Failed(ReloadFailure),
}

/// Adapter used by planning and after durable Activation commit.
pub trait ReloadAdapter {
    /// Reports adapter capability without mutating runtime state.
    fn observation(&self) -> ReloadObservation;

    /// Attempts one best-effort runtime reload.
    fn reload(&self) -> ReloadOutcome;
}

impl<F, E> ReloadAdapter for F
where
    F: Fn() -> Result<(), E>,
{
    fn observation(&self) -> ReloadObservation {
        ReloadObservation::Unavailable(ReloadUnavailableReason::AdapterCommandUnavailable)
    }

    fn reload(&self) -> ReloadOutcome {
        if self().is_ok() {
            ReloadOutcome::Succeeded
        } else {
            ReloadOutcome::Failed(ReloadFailure::Reload)
        }
    }
}

/// Captured external command result used by runtime adapters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandResult {
    success: bool,
    stdout: Vec<u8>,
}

impl CommandResult {
    /// Constructs a command observation.
    pub fn new(success: bool, stdout: impl AsRef<[u8]>) -> Self {
        Self {
            success,
            stdout: stdout.as_ref().to_vec(),
        }
    }
}

/// Narrow process boundary for recording exact runtime actions in tests.
pub trait CommandRunner {
    /// Returns whether command can be resolved without executing it.
    fn available(&self, program: &Path) -> bool;

    /// Executes command directly, without shell interpolation.
    fn run(&self, program: &Path, args: &[&str]) -> io::Result<CommandResult>;
}

/// Standard-library external command runner.
#[derive(Clone, Copy, Debug, Default)]
pub struct ProcessCommandRunner;

impl CommandRunner for ProcessCommandRunner {
    fn available(&self, program: &Path) -> bool {
        command_available(program)
    }

    fn run(&self, program: &Path, args: &[&str]) -> io::Result<CommandResult> {
        let output = Command::new(program).args(args).output()?;
        Ok(CommandResult::new(output.status.success(), output.stdout))
    }
}

/// Linux adapter using systemd or the running GTK application's D-Bus action.
pub struct SystemdReload<R> {
    runner: R,
    program: PathBuf,
}

impl<R> SystemdReload<R> {
    /// Constructs adapter with explicit command path or name.
    pub fn new(runner: R, program: impl Into<PathBuf>) -> Self {
        Self {
            runner,
            program: program.into(),
        }
    }
}

impl<R: CommandRunner> ReloadAdapter for SystemdReload<R> {
    fn observation(&self) -> ReloadObservation {
        if self.runner.available(&self.program) || self.runner.available(Path::new("busctl")) {
            ReloadObservation::Systemd
        } else {
            ReloadObservation::Unavailable(ReloadUnavailableReason::AdapterCommandUnavailable)
        }
    }

    fn reload(&self) -> ReloadOutcome {
        if self.runner.available(&self.program) {
            match self.runner.run(
                &self.program,
                &["--user", "is-active", "--quiet", GHOSTTY_SYSTEMD_SERVICE],
            ) {
                Ok(result) if result.success => {
                    return match self.runner.run(
                        &self.program,
                        &["--user", "reload", GHOSTTY_SYSTEMD_SERVICE],
                    ) {
                        Ok(result) if result.success => ReloadOutcome::Succeeded,
                        Ok(_) | Err(_) => ReloadOutcome::Failed(ReloadFailure::Reload),
                    };
                }
                Ok(_) => {}
                Err(_) => return ReloadOutcome::Failed(ReloadFailure::Probe),
            }
        }

        let busctl = Path::new("busctl");
        if !self.runner.available(busctl) {
            return ReloadOutcome::Unavailable(ReloadUnavailableReason::AdapterCommandUnavailable);
        }
        match self
            .runner
            .run(busctl, &["--user", "--quiet", "status", GHOSTTY_DBUS_NAME])
        {
            Ok(result) if result.success => {}
            Ok(_) => {
                return ReloadOutcome::Unavailable(
                    ReloadUnavailableReason::GhosttyIntegrationUnavailable,
                );
            }
            Err(_) => return ReloadOutcome::Failed(ReloadFailure::Probe),
        }
        match self.runner.run(
            busctl,
            &[
                "--user",
                "call",
                GHOSTTY_DBUS_NAME,
                "/com/mitchellh/ghostty",
                "org.gtk.Actions",
                "Activate",
                "sava{sv}",
                "reload-config",
                "0",
                "0",
            ],
        ) {
            Ok(result) if result.success => ReloadOutcome::Succeeded,
            Ok(_) | Err(_) => ReloadOutcome::Failed(ReloadFailure::Reload),
        }
    }
}

/// Experimental macOS adapter using Ghostty's AppleScript action API.
pub struct AppleScriptReload<R> {
    runner: R,
    program: PathBuf,
}

impl<R> AppleScriptReload<R> {
    /// Constructs adapter with explicit command path or name.
    pub fn new(runner: R, program: impl Into<PathBuf>) -> Self {
        Self {
            runner,
            program: program.into(),
        }
    }
}

impl<R: CommandRunner> ReloadAdapter for AppleScriptReload<R> {
    fn observation(&self) -> ReloadObservation {
        if self.runner.available(&self.program) {
            ReloadObservation::Applescript
        } else {
            ReloadObservation::Unavailable(ReloadUnavailableReason::AdapterCommandUnavailable)
        }
    }

    fn reload(&self) -> ReloadOutcome {
        if !self.runner.available(&self.program) {
            return ReloadOutcome::Unavailable(ReloadUnavailableReason::AdapterCommandUnavailable);
        }

        match self
            .runner
            .run(&self.program, &["-e", GHOSTTY_RUNNING_SCRIPT])
        {
            Ok(result) if applescript_boolean(&result) == Some(true) => {}
            Ok(result) if applescript_boolean(&result) == Some(false) => {
                return ReloadOutcome::Unavailable(
                    ReloadUnavailableReason::GhosttyIntegrationUnavailable,
                );
            }
            Ok(_) | Err(_) => return ReloadOutcome::Failed(ReloadFailure::Probe),
        }

        match self
            .runner
            .run(&self.program, &["-e", GHOSTTY_RELOAD_SCRIPT])
        {
            Ok(result) if applescript_boolean(&result) == Some(true) => ReloadOutcome::Succeeded,
            Ok(_) | Err(_) => ReloadOutcome::Failed(ReloadFailure::Reload),
        }
    }
}

fn applescript_boolean(result: &CommandResult) -> Option<bool> {
    if !result.success {
        return None;
    }
    match std::str::from_utf8(&result.stdout).ok()?.trim() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

/// Adapter for platforms without a supported Ghostty runtime integration.
pub struct UnavailableReload {
    reason: ReloadUnavailableReason,
}

impl UnavailableReload {
    /// Constructs an unavailable adapter.
    pub const fn new(reason: ReloadUnavailableReason) -> Self {
        Self { reason }
    }
}

impl ReloadAdapter for UnavailableReload {
    fn observation(&self) -> ReloadObservation {
        ReloadObservation::Unavailable(self.reason)
    }

    fn reload(&self) -> ReloadOutcome {
        ReloadOutcome::Unavailable(self.reason)
    }
}

/// Selects the supported adapter for the current platform.
#[cfg(target_os = "linux")]
pub fn platform_reload_adapter() -> impl ReloadAdapter {
    SystemdReload::new(ProcessCommandRunner, "systemctl")
}

/// Selects the experimental adapter for the current platform.
#[cfg(target_os = "macos")]
pub fn platform_reload_adapter() -> impl ReloadAdapter {
    AppleScriptReload::new(ProcessCommandRunner, "osascript")
}

/// Reports unsupported platforms without attempting a command.
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn platform_reload_adapter() -> impl ReloadAdapter {
    UnavailableReload::new(ReloadUnavailableReason::UnsupportedPlatform)
}

fn command_available(program: &Path) -> bool {
    if program.components().count() > 1 {
        return executable(program);
    }
    env::var_os("PATH").is_some_and(|path| {
        env::split_paths(&path).any(|directory| executable(&directory.join(program)))
    })
}

#[cfg(unix)]
fn executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    path.metadata()
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn executable(path: &Path) -> bool {
    path.is_file()
}
