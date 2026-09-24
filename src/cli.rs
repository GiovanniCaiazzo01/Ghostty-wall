//! Command-line interface joining Ghostty Wall's application services.

use std::{
    env, fs,
    fs::OpenOptions,
    io::{self, BufRead, Read, Write},
    path::{Path, PathBuf},
    str::FromStr,
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::Value;

use crate::{
    apply::{
        ApplyOutcome, apply_github_profile, apply_github_profile_with_theme, apply_local_profile,
        apply_local_profile_with_theme, previous,
    },
    codec::intent::{parse_config_toml, parse_named_profile_toml},
    domain::{
        ColorsIntent, ConfigIntent, IntentId, ProfileIntent, ResolutionSeed, SourceIntent,
        WallpaperIntent,
    },
    github::GithubHttpClient,
    init::{InitPaths, dry_run, init, init_repair},
    lifecycle::{CheckStatus, doctor, migrate_legacy, uninstall},
    plan::{
        PlanError, PlanPlatform, plan_github_profile_json, plan_github_profile_with_theme_json,
        plan_local_profile_json, plan_local_profile_with_theme_json, planned_github_asset_bytes,
        planned_local_asset_bytes,
    },
    runtime::{ReloadAdapter, platform_reload_adapter},
    terminal_browser::{
        BrowserAction, BrowserApplication, BrowserMode, PlannedProfile, TerminalBrowser,
        TerminalGraphics,
    },
    theme::ThemeFileResolver,
};

const HELP: &str = concat!(
    "Ghostty Wall ",
    env!("CARGO_PKG_VERSION"),
    "\n\nUsage:\n",
    "  ghostty-wall init [--dry-run | --repair | --migrate-legacy]\n",
    "  ghostty-wall plan PROFILE [--seed HEX] [--json]\n",
    "  ghostty-wall apply PROFILE [--seed HEX]\n",
    "  ghostty-wall previous\n",
    "  ghostty-wall doctor\n",
    "  ghostty-wall tui [--seed HEX]\n",
    "  ghostty-wall uninstall\n"
);
const MAX_INTENT_BYTES: u64 = 1024 * 1024;

/// Runs CLI using process arguments and environment, returning process exit status.
pub fn run() -> i32 {
    let args: Vec<String> = env::args().skip(1).collect();
    let stdout = io::stdout();
    let stderr = io::stderr();
    match execute(&args, &mut stdout.lock(), &mut stderr.lock()) {
        Ok(()) => 0,
        Err(CliError::JsonPlan(code)) => code,
        Err(error) => {
            let _ = writeln!(stderr.lock(), "ghostty-wall: {error}");
            error.exit_code()
        }
    }
}

fn execute(
    args: &[String],
    output: &mut impl Write,
    input_errors: &mut impl Write,
) -> Result<(), CliError> {
    match args {
        [] => Err(CliError::Usage("missing command".into())),
        [flag] if flag == "--help" || flag == "-h" => write_text(output, HELP),
        [flag] if flag == "--version" || flag == "-V" => {
            writeln!(output, "ghostty-wall {}", env!("CARGO_PKG_VERSION"))?;
            Ok(())
        }
        [command, rest @ ..] if command == "init" => command_init(rest, output),
        [command, rest @ ..] if command == "plan" => command_plan(rest, output),
        [command, rest @ ..] if command == "apply" => command_apply(rest, output),
        [command] if command == "previous" => command_previous(output),
        [command] if command == "doctor" => command_doctor(output),
        [command, rest @ ..] if command == "tui" => command_tui(rest, output, input_errors),
        [command] if command == "uninstall" => command_uninstall(output),
        _ => Err(CliError::Usage("unknown command or option".into())),
    }
}

fn command_init(args: &[String], output: &mut impl Write) -> Result<(), CliError> {
    let paths = process_paths()?;
    match args {
        [] => print_init_report(init(&paths)?, output),
        [flag] if flag == "--dry-run" => print_init_report(dry_run(&paths)?, output),
        [flag] if flag == "--repair" => print_init_report(init_repair(&paths)?, output),
        [flag] if flag == "--migrate-legacy" => {
            let report = migrate_legacy(&paths, false)?;
            writeln!(
                output,
                "Migrated {} Source(s); removed {} legacy hook(s).",
                report.imported_sources, report.removed_legacy_hooks
            )?;
            Ok(())
        }
        [first, second] if first == "--migrate-legacy" && second == "--dry-run" => {
            let report = migrate_legacy(&paths, true)?;
            writeln!(
                output,
                "Would import {} Source(s) and remove {} legacy hook(s).",
                report.imported_sources, report.removed_legacy_hooks
            )?;
            Ok(())
        }
        _ => Err(CliError::Usage("invalid init options".into())),
    }
}

fn command_plan(args: &[String], output: &mut impl Write) -> Result<(), CliError> {
    let options = ProfileOptions::parse(args, true)?;
    let application = Application::load(options.seed)?;
    match application.plan(&options.profile) {
        Ok(plan) => {
            if options.json {
                serde_json::to_writer(&mut *output, &plan)?;
            } else {
                serde_json::to_writer_pretty(&mut *output, &plan)?;
            }
            writeln!(output)?;
            Ok(())
        }
        Err(CliError::Plan(error)) if options.json => {
            serde_json::to_writer(&mut *output, &error.error_response())?;
            writeln!(output)?;
            Err(CliError::JsonPlan(error.exit_status()))
        }
        Err(error) => Err(error),
    }
}

fn command_apply(args: &[String], output: &mut impl Write) -> Result<(), CliError> {
    let options = ProfileOptions::parse(args, false)?;
    let application = Application::load(options.seed)?;
    print_apply_outcome(application.apply(&options.profile)?, output)
}

fn command_previous(output: &mut impl Write) -> Result<(), CliError> {
    let paths = process_paths()?;
    let outcome = previous(
        &paths.managed_root(),
        &paths.ghostty_root_config(),
        &timestamp()?,
        platform_reload_adapter(),
    )?;
    print_apply_outcome(outcome, output)
}

fn command_doctor(output: &mut impl Write) -> Result<(), CliError> {
    let report = doctor(&process_paths()?);
    let mut failed = false;
    writeln!(output, "Managed Root: {}", report.managed_root.display())?;
    for check in report.checks {
        let status = match check.status {
            CheckStatus::Verified => "verified",
            CheckStatus::Failed => {
                failed = true;
                "failed"
            }
            CheckStatus::Unavailable => "unavailable",
        };
        writeln!(output, "{}: {} — {}", check.name, status, check.detail)?;
    }
    if failed {
        Err(CliError::DoctorFailed)
    } else {
        Ok(())
    }
}

fn command_uninstall(output: &mut impl Write) -> Result<(), CliError> {
    let report = uninstall(&process_paths()?)?;
    writeln!(
        output,
        "Removed {} hook(s); Projection: {}; cache: {}. Intent and History preserved.",
        report.removed_hooks,
        removed(report.removed_projection),
        removed(report.removed_cache)
    )?;
    Ok(())
}

fn command_tui(
    args: &[String],
    output: &mut impl Write,
    input_errors: &mut impl Write,
) -> Result<(), CliError> {
    let seed = parse_seed_only(args)?;
    let mut application = Application::load(seed)?;
    let profiles = profile_ids(&application.paths.managed_root().join("profiles"))?;
    let sources = application
        .config
        .sources
        .iter()
        .map(|(id, _)| id.clone())
        .collect();
    let mut browser = TerminalBrowser::new(sources, profiles);
    let stdin = io::stdin();
    let mut lines = stdin.lock().lines();

    loop {
        write_text(output, &browser.render())?;
        if browser.mode() == BrowserMode::Preview {
            browser.render_preview_image(output, TerminalGraphics::from_environment())?;
        }
        if matches!(
            browser.mode(),
            BrowserMode::Applied | BrowserMode::Cancelled
        ) {
            return Ok(());
        }
        output.flush()?;
        let action = match lines.next().transpose()? {
            Some(line) => match line.trim() {
                "j" => BrowserAction::Down,
                "k" => BrowserAction::Up,
                "tab" => BrowserAction::NextPane,
                "" | "enter" => BrowserAction::Preview,
                "a" => BrowserAction::Apply,
                "b" | "esc" => BrowserAction::Back,
                "q" => BrowserAction::Cancel,
                _ => {
                    writeln!(input_errors, "keys: j k tab enter a b q")?;
                    continue;
                }
            },
            None => BrowserAction::Cancel,
        };
        browser.dispatch(action, &mut application)?;
    }
}

struct Application {
    paths: InitPaths,
    config: ConfigIntent,
    seed: Option<ResolutionSeed>,
    github: GithubHttpClient,
    themes: ThemeFileResolver,
}

impl Application {
    fn load(seed: Option<ResolutionSeed>) -> Result<Self, CliError> {
        let paths = process_paths()?;
        let config = parse_config_toml(&read_text(&paths.managed_root().join("config.toml"))?)?;
        let themes = ThemeFileResolver::new(theme_roots(&paths));
        Ok(Self {
            paths,
            config,
            seed,
            github: GithubHttpClient::from_env(),
            themes,
        })
    }

    fn load_profile(&self, profile: &IntentId) -> Result<ProfileIntent, CliError> {
        let path = self
            .paths
            .managed_root()
            .join("profiles")
            .join(format!("{profile}.toml"));
        let text = read_text(&path)?;
        let (_, profile) = parse_named_profile_toml(profile.as_str(), &self.config, &text)?;
        Ok(profile)
    }

    fn plan(&self, profile_id: &IntentId) -> Result<Value, CliError> {
        let profile = self.load_profile(profile_id)?;
        let root = self.paths.managed_root();
        let reload = platform_reload_adapter();
        let platform = PlanPlatform::new(self.paths.ghostty_root_config(), reload.observation());
        let github = uses_github(&self.config, &profile);
        let theme = matches!(profile.colors, Some(ColorsIntent::Theme { .. }));
        let seed = self.seed.as_ref();
        let result = match (github, theme) {
            (true, true) => plan_github_profile_with_theme_json(
                &root,
                &self.paths.home,
                &root,
                profile_id,
                &self.config,
                &profile,
                seed,
                &platform,
                &self.github,
                &self.themes,
            ),
            (true, false) => plan_github_profile_json(
                &root,
                &self.paths.home,
                &root,
                profile_id,
                &self.config,
                &profile,
                seed,
                &platform,
                &self.github,
            ),
            (false, true) => plan_local_profile_with_theme_json(
                &root,
                &self.paths.home,
                &root,
                profile_id,
                &self.config,
                &profile,
                seed,
                &platform,
                &self.themes,
            ),
            (false, false) => plan_local_profile_json(
                &root,
                &self.paths.home,
                &root,
                profile_id,
                &self.config,
                &profile,
                seed,
                &platform,
            ),
        };
        result.map_err(CliError::Plan)
    }

    fn apply(&self, profile_id: &IntentId) -> Result<ApplyOutcome, CliError> {
        let profile = self.load_profile(profile_id)?;
        let root = self.paths.managed_root();
        let root_config = self.paths.ghostty_root_config();
        let activated_at = timestamp()?;
        let github = uses_github(&self.config, &profile);
        let theme = matches!(profile.colors, Some(ColorsIntent::Theme { .. }));
        let seed = self.seed.as_ref();
        let result = match (github, theme) {
            (true, true) => apply_github_profile_with_theme(
                &root,
                &self.paths.home,
                &root,
                &root_config,
                profile_id,
                &self.config,
                &profile,
                seed,
                &self.themes,
                &activated_at,
                &self.github,
                platform_reload_adapter(),
            ),
            (true, false) => apply_github_profile(
                &root,
                &self.paths.home,
                &root,
                &root_config,
                profile_id,
                &self.config,
                &profile,
                seed,
                &activated_at,
                &self.github,
                platform_reload_adapter(),
            ),
            (false, true) => apply_local_profile_with_theme(
                &root,
                &self.paths.home,
                &root,
                &root_config,
                profile_id,
                &self.config,
                &profile,
                seed,
                &self.themes,
                &activated_at,
                platform_reload_adapter(),
            ),
            (false, false) => apply_local_profile(
                &root,
                &self.paths.home,
                &root,
                &root_config,
                profile_id,
                &self.config,
                &profile,
                seed,
                &activated_at,
                platform_reload_adapter(),
            ),
        };
        result.map_err(CliError::Apply)
    }
}

impl BrowserApplication for Application {
    fn plan_profile(&mut self, profile: &IntentId) -> Result<PlannedProfile, String> {
        let plan = self.plan(profile).map_err(|error| error.to_string())?;
        let image = if uses_github_plan(&plan) {
            planned_github_asset_bytes(&plan, &self.github)
        } else {
            planned_local_asset_bytes(&plan)
        }
        .map_err(|error| error.to_string())?;
        Ok(PlannedProfile::new(plan, image))
    }

    fn apply_profile(&mut self, profile: &IntentId) -> Result<(), String> {
        self.apply(profile)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}

struct ProfileOptions {
    profile: IntentId,
    seed: Option<ResolutionSeed>,
    json: bool,
}

impl ProfileOptions {
    fn parse(args: &[String], allow_json: bool) -> Result<Self, CliError> {
        let Some(profile) = args.first() else {
            return Err(CliError::Usage("PROFILE is required".into()));
        };
        let profile = IntentId::from_str(profile)?;
        let mut seed = None;
        let mut json = false;
        let mut index = 1;
        while index < args.len() {
            match args[index].as_str() {
                "--seed" if seed.is_none() => {
                    let value = args
                        .get(index + 1)
                        .ok_or_else(|| CliError::Usage("--seed requires HEX".into()))?;
                    seed = Some(ResolutionSeed::from_str(value)?);
                    index += 2;
                }
                "--json" if allow_json && !json => {
                    json = true;
                    index += 1;
                }
                _ => return Err(CliError::Usage("invalid profile command options".into())),
            }
        }
        Ok(Self {
            profile,
            seed,
            json,
        })
    }
}

fn process_paths() -> Result<InitPaths, CliError> {
    let home = env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| CliError::Usage("HOME must be set".into()))?;
    let xdg_config_home = env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);
    Ok(InitPaths {
        home,
        xdg_config_home,
    })
}

fn parse_seed_only(args: &[String]) -> Result<Option<ResolutionSeed>, CliError> {
    match args {
        [] => Ok(None),
        [flag, value] if flag == "--seed" => Ok(Some(ResolutionSeed::from_str(value)?)),
        _ => Err(CliError::Usage("invalid tui options".into())),
    }
}

fn uses_github(config: &ConfigIntent, profile: &ProfileIntent) -> bool {
    let Some(WallpaperIntent::Source { source, .. }) = &profile.wallpaper else {
        return false;
    };
    config
        .sources
        .iter()
        .find(|(id, _)| id == source)
        .is_some_and(|(_, source)| matches!(source, SourceIntent::Github { .. }))
}

fn uses_github_plan(plan: &Value) -> bool {
    plan.pointer("/source/kind").and_then(Value::as_str) == Some("github")
}

fn profile_ids(directory: &Path) -> Result<Vec<IntentId>, CliError> {
    let mut ids = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        if entry.file_type()?.is_file()
            && entry
                .path()
                .extension()
                .and_then(|extension| extension.to_str())
                == Some("toml")
        {
            let stem = entry
                .path()
                .file_stem()
                .and_then(|stem| stem.to_str())
                .ok_or_else(|| CliError::Intent("Profile filename is not UTF-8".into()))?
                .parse()?;
            ids.push(stem);
        }
    }
    Ok(ids)
}

fn theme_roots(paths: &InitPaths) -> Vec<PathBuf> {
    vec![
        paths.xdg_config_home().join("ghostty/themes"),
        paths
            .home
            .join("Library/Application Support/com.mitchellh.ghostty/themes"),
        PathBuf::from("/usr/local/share/ghostty/themes"),
        PathBuf::from("/usr/share/ghostty/themes"),
        PathBuf::from("/Applications/Ghostty.app/Contents/Resources/ghostty/themes"),
    ]
}

fn read_text(path: &Path) -> Result<String, CliError> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    let file = options.open(path)?;
    let length = file.metadata()?.len();
    if length > MAX_INTENT_BYTES {
        return Err(CliError::Intent(format!(
            "Intent file exceeds {MAX_INTENT_BYTES} bytes: {}",
            path.display()
        )));
    }
    let mut bytes = Vec::with_capacity(length as usize);
    file.take(MAX_INTENT_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_INTENT_BYTES {
        return Err(CliError::Intent(format!(
            "Intent file exceeds {MAX_INTENT_BYTES} bytes: {}",
            path.display()
        )));
    }
    String::from_utf8(bytes)
        .map_err(|_| CliError::Intent(format!("Intent is not UTF-8: {}", path.display())))
}

fn timestamp() -> Result<String, CliError> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| CliError::Internal("system clock predates Unix epoch".into()))?;
    #[cfg(unix)]
    {
        let seconds: libc::time_t = elapsed
            .as_secs()
            .try_into()
            .map_err(|_| CliError::Internal("system clock is out of range".into()))?;
        let mut broken_down = std::mem::MaybeUninit::<libc::tm>::uninit();
        // SAFETY: `seconds` and writable `tm` storage remain valid for this call.
        let result = unsafe { libc::gmtime_r(&seconds, broken_down.as_mut_ptr()) };
        if result.is_null() {
            return Err(CliError::Internal("cannot convert system clock".into()));
        }
        // SAFETY: successful `gmtime_r` initialized `broken_down`.
        let broken_down = unsafe { broken_down.assume_init() };
        Ok(format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:06}Z",
            broken_down.tm_year + 1900,
            broken_down.tm_mon + 1,
            broken_down.tm_mday,
            broken_down.tm_hour,
            broken_down.tm_min,
            broken_down.tm_sec,
            elapsed.subsec_micros()
        ))
    }
    #[cfg(not(unix))]
    Err(CliError::Internal("unsupported platform clock".into()))
}

fn print_init_report(
    report: crate::init::InitReport,
    output: &mut impl Write,
) -> Result<(), CliError> {
    writeln!(output, "Managed Root: {}", report.managed_root.display())?;
    writeln!(output, "Ghostty config: {}", report.root_config.display())?;
    writeln!(output, "Capabilities: {}", report.capabilities)?;
    for mutation in report.mutations {
        writeln!(output, "- {mutation}")?;
    }
    Ok(())
}

fn print_apply_outcome(outcome: ApplyOutcome, output: &mut impl Write) -> Result<(), CliError> {
    writeln!(output, "Activated {}.", outcome.activation_id())?;
    writeln!(
        output,
        "Ghostty reload: {}.",
        if outcome.reload_succeeded() {
            "succeeded"
        } else {
            "unavailable or failed; Activation remains committed"
        }
    )?;
    Ok(())
}

fn removed(value: bool) -> &'static str {
    if value { "removed" } else { "absent" }
}

fn write_text(output: &mut impl Write, text: &str) -> Result<(), CliError> {
    output.write_all(text.as_bytes())?;
    Ok(())
}

#[derive(Debug)]
enum CliError {
    Usage(String),
    Intent(String),
    Plan(PlanError),
    JsonPlan(i32),
    Apply(crate::apply::ApplyError),
    DoctorFailed,
    Internal(String),
    Io(io::Error),
    Init(crate::init::InitError),
    Lifecycle(crate::lifecycle::LifecycleError),
    Browser(crate::terminal_browser::BrowserError),
    Preview(crate::terminal_browser::ImagePreviewError),
    Json(serde_json::Error),
}

impl CliError {
    fn exit_code(&self) -> i32 {
        match self {
            Self::Usage(_) => 2,
            Self::Intent(_) => 3,
            Self::Plan(error) => error.exit_status(),
            Self::JsonPlan(code) => *code,
            Self::Apply(_) | Self::DoctorFailed | Self::Init(_) | Self::Lifecycle(_) => 6,
            Self::Internal(_)
            | Self::Io(_)
            | Self::Browser(_)
            | Self::Preview(_)
            | Self::Json(_) => 70,
        }
    }
}

impl std::fmt::Display for CliError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Usage(message) => write!(formatter, "{message}\n\n{HELP}"),
            Self::Intent(message) | Self::Internal(message) => formatter.write_str(message),
            Self::Plan(error) => error.fmt(formatter),
            Self::JsonPlan(_) => Ok(()),
            Self::Apply(error) => error.fmt(formatter),
            Self::DoctorFailed => formatter.write_str("one or more Doctor checks failed"),
            Self::Io(error) => error.fmt(formatter),
            Self::Init(error) => error.fmt(formatter),
            Self::Lifecycle(error) => error.fmt(formatter),
            Self::Browser(error) => error.fmt(formatter),
            Self::Preview(error) => error.fmt(formatter),
            Self::Json(error) => error.fmt(formatter),
        }
    }
}

impl From<io::Error> for CliError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}
impl From<crate::domain::ValidationError> for CliError {
    fn from(error: crate::domain::ValidationError) -> Self {
        Self::Usage(error.to_string())
    }
}
impl From<crate::codec::intent::IntentTomlError> for CliError {
    fn from(error: crate::codec::intent::IntentTomlError) -> Self {
        Self::Intent(error.to_string())
    }
}
impl From<crate::apply::ApplyError> for CliError {
    fn from(error: crate::apply::ApplyError) -> Self {
        Self::Apply(error)
    }
}
impl From<crate::init::InitError> for CliError {
    fn from(error: crate::init::InitError) -> Self {
        Self::Init(error)
    }
}
impl From<crate::lifecycle::LifecycleError> for CliError {
    fn from(error: crate::lifecycle::LifecycleError) -> Self {
        Self::Lifecycle(error)
    }
}
impl From<crate::terminal_browser::BrowserError> for CliError {
    fn from(error: crate::terminal_browser::BrowserError) -> Self {
        Self::Browser(error)
    }
}
impl From<crate::terminal_browser::ImagePreviewError> for CliError {
    fn from(error: crate::terminal_browser::ImagePreviewError) -> Self {
        Self::Preview(error)
    }
}
impl From<serde_json::Error> for CliError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}
