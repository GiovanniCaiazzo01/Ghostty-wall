//! Command-line interface joining Ghostty Wall's application services.

use std::{
    env, fs,
    fs::OpenOptions,
    io::{self, BufRead, IsTerminal, Read, Write},
    path::{Path, PathBuf},
    str::FromStr,
    time::{SystemTime, UNIX_EPOCH},
};

use crossterm::{
    event::{self, Event, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use serde_json::Value;
use sha2::{Digest, Sha256};

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
    init::{InitPaths, dry_run, init, init_repair, init_welcome},
    lifecycle::{CheckStatus, doctor, migrate_legacy, uninstall},
    plan::{
        PlanError, PlanPlatform, plan_github_profile_json, plan_github_profile_with_theme_json,
        plan_local_profile_json, plan_local_profile_with_theme_json, planned_github_asset_bytes,
        planned_local_asset_bytes,
    },
    profile_workflow::NEW_PROFILE_WALLPAPER_OPACITY,
    runtime::{ReloadAdapter, platform_reload_adapter},
    terminal_browser::{
        BrowserAction, BrowserApplication, BrowserFocus, BrowserMode, PlannedProfile,
        TerminalBrowser, TerminalGraphics, format_plan_preview,
    },
    theme::ThemeFileResolver,
    tui::{self, Input},
    update::{self, UpdateError},
};

mod create_tui;
mod delete;
mod editor;
mod forms;
mod maintenance;
mod management;
mod presentation;
mod profile_forms;
mod sources;

const HELP: &str = concat!(
    "Ghostty Wall ",
    env!("CARGO_PKG_VERSION"),
    "\n\nUsage:\n",
    "  ghostty-wall                           # Profile management center\n",
    "  ghostty-wall init [--dry-run | --repair | --migrate-legacy | --welcome]\n",
    "  ghostty-wall plan PROFILE [--seed HEX] [--json]\n",
    "  ghostty-wall preview PROFILE [--seed HEX]  # read-only; not live Ghostty reload\n",
    "  ghostty-wall create [PROFILE]     # guided image or stable generated wallpaper\n",
    "  ghostty-wall new PROFILE [IMAGE [--apply]]\n",
    "  ghostty-wall new PROFILE --generate SEED_HEX  # stable gradient-v1 PNG\n",
    "  ghostty-wall new PROFILE --source SOURCE --path CANDIDATE [--apply]\n",
    "  ghostty-wall source list | show SOURCE | check SOURCE\n",
    "  ghostty-wall source edit SOURCE local DIRECTORY\n",
    "  ghostty-wall source edit SOURCE github OWNER/REPO [--ref REF] [--path PATH]\n",
    "  ghostty-wall source remove SOURCE  # confirmed; unused Sources only\n",
    "  ghostty-wall source add SOURCE local DIRECTORY\n",
    "  ghostty-wall source add SOURCE github OWNER/REPO [--ref REF] [--path PATH]\n",
    "  ghostty-wall edit [PROFILE]     # visual draft; Save / Save and use / Cancel\n",
    "  ghostty-wall edit PROFILE FIELD VALUE    # advanced; saves immediately; no live draft\n",
    "  ghostty-wall duplicate PROFILE NEW\n",
    "  ghostty-wall rename PROFILE NEW  # inactive, non-Welcome Profile only\n",
    "  ghostty-wall delete [PROFILE]   # confirmed; active deletion falls back to Welcome\n",
    "  ghostty-wall list\n",
    "  ghostty-wall history\n",
    "  ghostty-wall apply PROFILE [--seed HEX]\n",
    "  ghostty-wall previous\n",
    "  ghostty-wall doctor\n",
    "  ghostty-wall tui [--seed HEX]       # Create/Edit/Delete/Use; ? Actions, --help\n",
    "  ghostty-wall uninstall\n",
    "  ghostty-wall update [--check]  # release-installer or Cargo; --check is read-only\n",
    "\nNew image Profiles start at wallpaper opacity 0.05; terminal transparency is unchanged.\n",
    "Live draft sessions are library-only; CLI/TUI previews do not reload Ghostty.\n",
    "After an interrupted session, apply/previous restore from History before committing; doctor is read-only.\n"
);
const CREATE_HELP: &str = concat!(
    "Usage: ghostty-wall create [PROFILE]\n\n",
    "Create a complete Profile with a generated wallpaper or your own PNG/JPEG.\n",
    "New wallpaper opacity is 0.05 (subdued image), not terminal background opacity.\n",
    "Light themes may look lighter, not darker. Existing Profiles are unchanged.\n",
    "After saving, edit PROFILE wallpaper.opacity VALUE to customize (0..1).\n",
    "Omit PROFILE to choose an id; supplied ids are not asked for again.\n",
    "Ids: 1..64 lowercase letters/digits with single internal hyphens. Existing ids are errors.\n\n",
    "Answer each prompt then press Enter. Type cancel (or send EOF) before Save\n",
    "to discard the draft without writing files. Generated wallpapers change only\n",
    "when you choose Another variant before Save; plan/apply never regenerate them.\n\n",
    "The image picker starts at system Downloads/Pictures locations. Use a number\n",
    "or relative path, .. for parent, d/p to switch roots, /text to search, or\n",
    "path:/absolute/path to open an absolute path. Images are decoded before import;\n",
    "the original is untouched. Save copies the image into managed storage; an identical\n",
    "existing image may be reused but never gains deletion ownership.\n\n",
    "A failed Save before Profile publication rolls back only its new unchanged image;\n",
    "existing files are preserved. Fix the reported error before retrying create.\n",
    "Uncertain publication or incomplete rollback requires inspection before retry.\n\n",
    "After Save choose Use now or Not now (default). Use now commits an Activation\n",
    "via normal apply; reload is best-effort, not proof of visible Ghostty change.\n",
    "Not now keeps the Profile saved and leaves the terminal unchanged.\n",
    "Choices are listed separately; defaults appear in the input question.\n",
    "NO_COLOR disables inline styling; piped input/output stays plain.\n",
    "Run ghostty-wall init first. This command does not provide live draft preview.\n",
    "TUI: n opens an embedded form and approximate sample. s Save finishes without\n",
    "applying; u Save and use confirms (default: Back to editor); Esc/q Cancel\n",
    "discards the unsaved draft. Declining confirmation or a definite save failure\n",
    "retains the draft. v opens details; uncertain publication requires inspection.\n"
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
        [] => command_tui(&[], output, input_errors),
        [flag] if flag == "--help" || flag == "-h" => write_text(output, HELP),
        [flag] if flag == "--version" || flag == "-V" => {
            writeln!(output, "ghostty-wall {}", env!("CARGO_PKG_VERSION"))?;
            Ok(())
        }
        [command, rest @ ..] if command == "init" => command_init(rest, output),
        [command, rest @ ..] if command == "plan" => command_plan(rest, output),
        [command, rest @ ..] if command == "preview" => command_preview(rest, output),
        [command, rest @ ..] if command == "create" => command_create(rest, output),
        [command, rest @ ..] if command == "new" => command_new(rest, output),
        [command, rest @ ..] if command == "source" => command_source(rest, output),
        [command, rest @ ..] if command == "edit" => command_edit(rest, output),
        [command, rest @ ..] if command == "duplicate" => {
            command_profile_file(rest, "duplicate", output)
        }
        [command, rest @ ..] if command == "rename" => command_profile_file(rest, "rename", output),
        [command, rest @ ..] if command == "delete" => delete::command(rest, output),
        [command] if command == "list" => command_list(output),
        [command] if command == "history" => command_history(output),
        [command, rest @ ..] if command == "apply" => command_apply(rest, output),
        [command] if command == "previous" => command_previous(output),
        [command] if command == "doctor" => command_doctor(output),
        [command, rest @ ..] if command == "tui" => command_tui(rest, output, input_errors),
        [command] if command == "uninstall" => command_uninstall(output),
        [command, rest @ ..] if command == "update" => {
            if matches!(rest, [flag] if flag == "--help" || flag == "-h") {
                return write_text(
                    output,
                    "Usage: ghostty-wall update [--check]\n\nExplicitly update Ghostty Wall, never Ghostty or Profile data.\n--check only checks the latest stable release; it never installs.\nRelease-installer binaries use verified Linux x86_64 release archives.\nCargo installations build the stable GitHub tag in temporary staging,\nthen update the invoked binary and Cargo metadata at the original prefix.\nSource builds require Cargo/Rust (1.88+), a native linker and network.\nLinux/macOS and atomic-exchange-capable writable install directories are\nrequired. Missing/mismatched ownership or substituted paths are rejected;\nmanual binaries are not overwritten. No privilege escalation.\nKeep other installers idle. Failed publication rolls back; if rollback\nalso fails, inspect the recovery paths in the error before retrying.\n",
                );
            }
            update::run(parse_update_options(rest)?, output).map_err(CliError::Update)
        }
        _ => Err(CliError::Usage("unknown command or option".into())),
    }
}

fn parse_update_options(args: &[String]) -> Result<bool, CliError> {
    match args {
        [] => Ok(false),
        [flag] if flag == "--check" => Ok(true),
        _ => Err(CliError::Usage("invalid update options".into())),
    }
}

fn command_init(args: &[String], output: &mut impl Write) -> Result<(), CliError> {
    let paths = process_paths()?;
    match args {
        [] => print_init_report(init(&paths)?, output),
        [flag] if flag == "--dry-run" => print_init_report(dry_run(&paths)?, output),
        [flag] if flag == "--repair" => print_init_report(init_repair(&paths)?, output),
        [flag] if flag == "--welcome" => print_init_report(init_welcome(&paths)?, output),
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

fn command_preview(args: &[String], output: &mut impl Write) -> Result<(), CliError> {
    let options = ProfileOptions::parse(args, false)?;
    let application = Application::load(options.seed)?;
    let plan = application.plan(&options.profile)?;
    write_text(output, &format_plan_preview(&plan))
}

/// Guided creation keeps all tentative image bytes and Intent in memory until explicit save.
fn command_create(args: &[String], output: &mut impl Write) -> Result<(), CliError> {
    let supplied = match args {
        [] => None,
        [flag] if flag == "--help" || flag == "-h" => return write_text(output, CREATE_HELP),
        [name] => {
            IntentId::from_str(name).map_err(|error| {
                CliError::Input(format!(
                    "Cannot create Profile {name:?}: {}; no files changed. Choose another id.",
                    CliError::from(error)
                ))
            })?;
            Some(name.as_str())
        }
        _ => {
            return Err(CliError::Input(
                "expected create [PROFILE]; see create --help".into(),
            ));
        }
    };
    let paths = process_paths()?;
    let workflow = crate::profile_workflow::ProfileWorkflows::load(paths.clone()).map_err(|error| {
        if matches!(&error, crate::profile_workflow::WorkflowError::Io { source, .. } if source.kind() == io::ErrorKind::NotFound) {
            CliError::Intent(format!("Cannot start creation: {error}; no files changed. Run ghostty-wall init for an uninitialized installation; otherwise inspect the missing path first."))
        } else {
            error.into()
        }
    })?;
    let stdin = io::stdin();
    let mut lines = stdin.lock().lines();
    let mut name = supplied.map(str::to_owned);
    let mut draft = loop {
        let candidate = match name.take() {
            Some(value) => value,
            None => match create_prompt(&mut lines, output, "Profile ID (or cancel): ")? {
                Some(value) => value,
                None => return create_cancelled(output),
            },
        };
        match workflow.create(&candidate) {
            Ok(draft) => break draft,
            Err(
                error @ (crate::profile_workflow::WorkflowError::Id(_)
                | crate::profile_workflow::WorkflowError::Collision(_)),
            ) if supplied.is_none() => {
                presentation::error(
                    output,
                    &format!(
                        "Invalid or existing Profile ID: {}. Try another; nothing saved.",
                        CliError::from(error)
                    ),
                )?;
            }
            Err(error) => return Err(error.into()),
        }
    };
    presentation::heading(output, &format!("Create Profile {}.", draft.id()))?;
    presentation::line(
        output,
        presentation::Role::Warning,
        "Nothing is saved until you choose Save.",
    )?;
    let generated = loop {
        presentation::choices(
            output,
            "Wallpaper:",
            &[
                "[g] Generate wallpaper",
                "[i] Image from Downloads/Pictures",
                "cancel  Discard draft",
            ],
        )?;
        let Some(choice) = create_prompt(&mut lines, output, "Choice (g/i/cancel; no default): ")?
        else {
            return create_cancelled(output);
        };
        match choice.to_ascii_lowercase().as_str() {
            "g" | "generate" => {
                generate_create_image(&workflow, &mut draft, output)?;
                break true;
            }
            "i" | "image" => {
                if !pick_create_image(&workflow, &mut draft, &paths, &mut lines, output)? {
                    return create_cancelled(output);
                }
                break false;
            }
            _ => presentation::error(output, "Choose g or i; nothing saved.")?,
        }
    };
    let mut colors = draft.generated_colors()?;
    loop {
        writeln!(
            output,
            "Draft {}: wallpaper and generated colors (background #{}, foreground #{}, 16 ANSI colors). This is not a live Ghostty reload.",
            draft.id(),
            colors.background(),
            colors.foreground()
        )?;
        let label = if generated {
            presentation::choices(
                output,
                "Save draft:",
                &[
                    "[s]ave wallpaper and colors",
                    "[a]nother generated variant (before save only)",
                    "cancel  Discard draft",
                ],
            )?;
            "Choice (s/a/cancel; no default): "
        } else {
            presentation::choices(
                output,
                "Save draft:",
                &["[s]ave copied image and colors", "cancel  Discard draft"],
            )?;
            "Choice (s/cancel; no default): "
        };
        let Some(choice) = create_prompt(&mut lines, output, label)? else {
            return create_cancelled(output);
        };
        match choice.to_ascii_lowercase().as_str() {
            "s" | "save" => break,
            "a" | "another" if generated => {
                generate_create_image(&workflow, &mut draft, output)?;
                colors = draft.generated_colors()?;
            }
            _ => presentation::error(
                output,
                "Choose one of the listed actions; draft unchanged, nothing saved.",
            )?,
        }
    }
    let id = workflow.save(draft)?;
    presentation::line(
        output,
        presentation::Role::Success,
        &format!("Profile saved.\nSaved Profile {id}. No Activation yet."),
    )?;
    loop {
        presentation::choices(
            output,
            "Use saved Profile:",
            &["[y] Use now", "[n] Not now (default)"],
        )?;
        let Some(choice) = create_prompt(&mut lines, output, "Choice (y/n; default: n): ")? else {
            writeln!(
                output,
                "Not now; saved Profile remains, terminal unchanged."
            )?;
            return Ok(());
        };
        match choice.to_ascii_lowercase().as_str() {
            "" | "n" | "no" | "not now" => {
                presentation::line(
                    output,
                    presentation::Role::Success,
                    &format!("Saved Profile {id}; terminal unchanged."),
                )?;
                return Ok(());
            }
            "y" | "yes" | "use now" => {
                // Loading and applying can fail independently of the already completed save.
                let outcome = workflow.use_saved(&id, |id| Application::load(None)?.apply(id))?;
                write_text(output, &completion_report(&id, outcome))?;
                return Ok(());
            }
            _ => presentation::error(output, "Choose y or n; saved Profile remains.")?,
        }
    }
}

fn create_prompt(
    input: &mut impl Iterator<Item = io::Result<String>>,
    output: &mut impl Write,
    label: &str,
) -> Result<Option<String>, CliError> {
    presentation::prompt(output, label)?;
    output.flush()?;
    Ok(input
        .next()
        .transpose()?
        .map(|line| line.trim().to_owned())
        .filter(|line| !matches!(line.as_str(), "cancel" | "esc" | "b" | "\x1b")))
}

fn create_cancelled(output: &mut impl Write) -> Result<(), CliError> {
    writeln!(output, "Cancelled; no Profile saved.")?;
    Ok(())
}

fn generate_create_image(
    workflow: &crate::profile_workflow::ProfileWorkflows,
    draft: &mut crate::profile_workflow::ProfileDraft,
    output: &mut impl Write,
) -> Result<(), CliError> {
    let mut bytes = [0; 32];
    fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    let seed = crate::domain::Sha256Digest::from_bytes(bytes);
    workflow.generate_image(draft, seed)?;
    writeln!(
        output,
        "Generated variant {seed} (256x256 PNG); held in memory until Save."
    )?;
    Ok(())
}

// XDG user-dirs are quoted paths with $HOME expansion, not shell expressions.
fn create_image_roots(paths: &InitPaths) -> [PathBuf; 2] {
    let config_home = paths
        .xdg_config_home
        .as_ref()
        .filter(|path| path.is_absolute())
        .cloned()
        .unwrap_or_else(|| paths.home.join(".config"));
    let config = fs::read_to_string(config_home.join("user-dirs.dirs")).unwrap_or_default();
    let directory = |key: &str, fallback: &str| {
        let value = config
            .lines()
            .filter_map(|line| line.trim().split_once('='))
            .find(|(name, _)| name.trim() == key)
            .map(|(_, value)| value.trim());
        value
            .and_then(|value| parse_user_directory(value, &paths.home))
            .unwrap_or_else(|| paths.home.join(fallback))
    };
    [
        directory("XDG_DOWNLOAD_DIR", "Downloads"),
        directory("XDG_PICTURES_DIR", "Pictures"),
    ]
}

fn parse_user_directory(value: &str, home: &Path) -> Option<PathBuf> {
    let quoted = value.strip_prefix('"')?;
    let home_relative = quoted.starts_with("$HOME/") || quoted.starts_with("$HOME\"");
    let mut chars = quoted
        .strip_prefix("$HOME")
        .filter(|_| home_relative)
        .unwrap_or(quoted)
        .chars();
    let mut decoded = String::new();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                let rest = chars.as_str().trim();
                if !rest.is_empty() && !rest.starts_with('#') {
                    return None;
                }
                let path = if home_relative {
                    home.join(decoded.trim_start_matches('/'))
                } else {
                    PathBuf::from(decoded)
                };
                return path.is_absolute().then_some(path);
            }
            '\\' => {
                let escaped = chars.next()?;
                if !matches!(escaped, '$' | '`' | '"' | '\\') {
                    decoded.push('\\');
                }
                decoded.push(escaped);
            }
            '$' | '`' => return None,
            _ => decoded.push(c),
        }
    }
    None
}

fn pick_create_image(
    workflow: &crate::profile_workflow::ProfileWorkflows,
    draft: &mut crate::profile_workflow::ProfileDraft,
    paths: &InitPaths,
    input: &mut impl Iterator<Item = io::Result<String>>,
    output: &mut impl Write,
) -> Result<bool, CliError> {
    pick_image_with(paths, input, output, |path| {
        workflow.import_image(draft, path)
    })
}

fn pick_image_with(
    paths: &InitPaths,
    input: &mut impl Iterator<Item = io::Result<String>>,
    output: &mut impl Write,
    mut import: impl FnMut(&Path) -> Result<(), crate::profile_workflow::WorkflowError>,
) -> Result<bool, CliError> {
    let roots = create_image_roots(paths);
    let mut current = roots
        .iter()
        .find(|path| path.is_dir())
        .cloned()
        .unwrap_or_else(|| paths.home.clone());
    let mut search = String::new();
    loop {
        presentation::heading(output, &format!("Images in {}", current.display()))?;
        writeln!(output)?;
        presentation::line(
            output,
            presentation::Role::Choice,
            &format!("  d  Downloads: {}", roots[0].display()),
        )?;
        presentation::line(
            output,
            presentation::Role::Choice,
            &format!("  p  Pictures: {}", roots[1].display()),
        )?;
        writeln!(output)?;
        let entries = match fs::read_dir(&current) {
            Ok(entries) => {
                let mut found = Vec::new();
                for entry in entries.flatten() {
                    let path = entry.path();
                    let Ok(name) = entry.file_name().into_string() else {
                        continue;
                    };
                    let supported = path.extension().and_then(|s| s.to_str()).is_some_and(|s| {
                        matches!(s.to_ascii_lowercase().as_str(), "png" | "jpg" | "jpeg")
                    });
                    if entry
                        .file_type()
                        .is_ok_and(|kind| kind.is_dir() || (kind.is_file() && supported))
                        && name.to_lowercase().contains(&search.to_lowercase())
                    {
                        found.push(name);
                    }
                }
                found.sort();
                found
            }
            Err(error) => {
                presentation::error(
                    output,
                    &format!(
                        "Cannot browse {}: {error}. Choose another location.",
                        current.display()
                    ),
                )?;
                Vec::new()
            }
        };
        for (index, name) in entries.iter().take(100).enumerate() {
            presentation::line(
                output,
                presentation::Role::Choice,
                &format!("  {}: {}", index + 1, name),
            )?;
        }
        if entries.len() > 100 {
            writeln!(
                output,
                "More results: use /text to narrow the list, or type a path."
            )?;
        }
        let Some(choice) = create_prompt(
            input,
            output,
            "Number/relative path\npath:/absolute/path\n.. parent; d Downloads; p Pictures\n/text search; cancel\nChoice (no default): ",
        )?
        else {
            return Ok(false);
        };
        if let Some(query) = choice.strip_prefix('/') {
            // Absolute paths can also be entered using an explicit 'path:' prefix.
            search = query.to_owned();
            continue;
        }
        if choice == "d" || choice == "p" {
            let destination = &roots[usize::from(choice == "p")];
            if destination.is_dir() {
                current = destination.clone();
                search.clear();
            } else {
                presentation::error(
                    output,
                    &format!(
                        "{} is not available; choose another location.",
                        destination.display()
                    ),
                )?;
            }
            continue;
        }
        if choice == ".." {
            if let Some(parent) = current.parent() {
                current = parent.to_owned();
                search.clear();
            }
            continue;
        }
        let selected = choice
            .parse::<usize>()
            .ok()
            .and_then(|index| index.checked_sub(1))
            .and_then(|index| entries.get(index).filter(|_| index < 100))
            .map(|name| current.join(name))
            .unwrap_or_else(|| {
                let value = choice.strip_prefix("path:").unwrap_or(&choice);
                let path = PathBuf::from(value);
                if path.is_absolute() {
                    path
                } else {
                    current.join(path)
                }
            });
        if selected.is_dir() {
            current = selected;
            search.clear();
            continue;
        }
        match import(&selected) {
            Ok(()) => return Ok(true),
            Err(error) => presentation::error(
                output,
                &format!(
                    "Cannot use {}: {error}. Choose another PNG/JPEG; nothing saved.",
                    selected.display()
                ),
            )?,
        }
    }
}

fn command_new(args: &[String], output: &mut impl Write) -> Result<(), CliError> {
    if let [name, flag, seed] = args
        && flag == "--generate"
    {
        return new_generated(name, seed, output);
    }
    if let [name] = args {
        let stdin = io::stdin();
        let mut lines = stdin.lock().lines();
        let Some(image) = prompt_tui(&mut lines, output, "PNG/JPEG image path (b to cancel): ")?
        else {
            return Ok(());
        };
        return command_new(&[name.clone(), image], output);
    }
    match args {
        [name, flag, source, path_flag, path] if flag == "--source" && path_flag == "--path" => {
            return new_from_source(name, source, path, false, output);
        }
        [name, flag, source, path_flag, path, apply]
            if flag == "--source" && path_flag == "--path" && apply == "--apply" =>
        {
            return new_from_source(name, source, path, true, output);
        }
        _ => {}
    }
    let (name, image, apply) = match args {
        [name, image] => (name, image, false),
        [name, image, flag] if flag == "--apply" => (name, image, true),
        _ => {
            return Err(CliError::Usage(
                "expected new PROFILE [IMAGE [--apply]] or new PROFILE --source SOURCE --path CANDIDATE [--apply]".into(),
            ));
        }
    };
    let id = IntentId::from_str(name)?;
    let application = Application::load(None)?;
    let source = application
        .config
        .sources
        .iter()
        .filter(|(_, source)| matches!(source, SourceIntent::LocalDirectory { path } if path == "profiles"))
        .min_by_key(|(id, _)| (id.as_str() != "welcome", id.as_str()))
        .map(|(id, _)| id)
        .ok_or_else(|| CliError::Intent("new needs a local Source pointing to profiles/; use source add SOURCE local DIRECTORY first".into()))?;
    let root = application.paths.managed_root();
    let profiles = profile_directory(&root)?;
    let input = Path::new(image);
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    let file = options.open(input)?;
    if !file.metadata()?.is_file() || file.metadata()?.len() > 32 * 1024 * 1024 {
        return Err(CliError::Intent(
            "image must be regular PNG/JPEG under 32 MiB".into(),
        ));
    }
    let mut bytes = Vec::new();
    file.take(32 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 32 * 1024 * 1024 {
        return Err(CliError::Intent("image exceeds 32 MiB".into()));
    }
    let format = image::guess_format(&bytes)
        .map_err(|_| CliError::Intent("image must be PNG or JPEG".into()))?;
    let extension = match format {
        image::ImageFormat::Png => "png",
        image::ImageFormat::Jpeg => "jpg",
        _ => return Err(CliError::Intent("image must be PNG or JPEG".into())),
    };
    let mut reader = image::ImageReader::with_format(io::Cursor::new(&bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16_384);
    limits.max_image_height = Some(16_384);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    let decoded = reader
        .decode()
        .map_err(|error| CliError::Intent(error.to_string()))?;
    if u64::from(decoded.width()) * u64::from(decoded.height()) > 16_777_216 {
        return Err(CliError::Intent("image dimensions exceed limit".into()));
    }
    let digest = hex_sha256(&bytes);
    let profile = format!(
        "schema_version = 2\n\n[wallpaper]\nmode = \"source\"\nsource = \"{source}\"\nselection = \"path\"\npath = \"{id}.{extension}\"\nowned_sha256 = \"{digest}\"\nfit = \"cover\"\nposition = \"center\"\nopacity = {NEW_PROFILE_WALLPAPER_OPACITY}\n\n[colors]\nmode = \"generated\"\n"
    );
    parse_named_profile_toml(id.as_str(), &application.config, &profile)?;
    let profile_path = profiles.join(format!("{id}.toml"));
    let image_path = profiles.join(format!("{id}.{extension}"));
    let _lock = crate::recovery::exclusive_state_lock(&root.join("state.lock"))
        .map_err(|error| CliError::Intent(error.to_string()))?;
    let image_exists = path_exists(&image_path)?;
    let profile_exists = path_exists(&profile_path)?;
    if image_exists && !existing_equal(&image_path, &bytes)? {
        return Err(CliError::Intent(format!(
            "Image {} already differs; no files changed",
            image_path.display()
        )));
    }
    if profile_exists {
        if !image_exists || read_text(&profile_path)? != profile {
            return Err(CliError::Intent(format!(
                "Profile {id} already differs; no files changed"
            )));
        }
    } else {
        if !image_exists {
            create_private(&image_path, &bytes)?;
        }
        if let Err(error) = create_private(&profile_path, profile.as_bytes()) {
            // A published Profile must keep its image even if directory sync failed.
            if !image_exists && fs::symlink_metadata(&profile_path).is_err() {
                let _ = fs::remove_file(&image_path);
            }
            return Err(error);
        }
    }
    drop(_lock);
    writeln!(output, "Saved {id}.")?;
    print_saved_preview(&application, &id, output)?;
    if apply {
        print_apply_outcome(application.apply(&id)?, output)?;
    } else {
        writeln!(output, "Run ghostty-wall apply {id} to activate.")?;
    }
    Ok(())
}

fn hex_sha256(bytes: &[u8]) -> String {
    crate::domain::Sha256Digest::from_bytes(Sha256::digest(bytes).into()).to_string()
}

fn new_generated(name: &str, seed: &str, output: &mut impl Write) -> Result<(), CliError> {
    let id = IntentId::from_str(name)?;
    let seed = crate::domain::Sha256Digest::from_str(seed)?;
    let application = Application::load(None)?;
    let source = application.config.sources.iter()
        .find(|(_, source)| matches!(source, SourceIntent::LocalDirectory { path } if path == "profiles"))
        .map(|(id, _)| id)
        .ok_or_else(|| CliError::Intent("generation needs a local Source pointing to profiles/; no files changed".into()))?;
    let width = 256u32;
    let height = 256u32;
    let raw = seed.as_bytes();
    let image = image::RgbaImage::from_fn(width, height, |x, y| {
        image::Rgba([
            raw[0].wrapping_add((255 * x / (width - 1)) as u8),
            raw[1].wrapping_add((255 * y / (height - 1)) as u8),
            raw[2].wrapping_add((255 * (x + y) / (width + height - 2)) as u8),
            255,
        ])
    });
    let mut cursor = io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut cursor, image::ImageFormat::Png)
        .map_err(|error| CliError::Intent(error.to_string()))?;
    let bytes = cursor.into_inner();
    let digest = hex_sha256(&bytes);
    let text = format!(
        "schema_version = 2\n\n[wallpaper]\nmode = \"source\"\nsource = \"{source}\"\nselection = \"path\"\npath = \"{id}.png\"\nowned_sha256 = \"{digest}\"\nfit = \"cover\"\nposition = \"center\"\nopacity = {NEW_PROFILE_WALLPAPER_OPACITY}\n\n[wallpaper.generation]\nalgorithm = \"gradient-v1\"\nseed = \"{seed}\"\nwidth = {width}\nheight = {height}\n\n[colors]\nmode = \"generated\"\n"
    );
    parse_named_profile_toml(id.as_str(), &application.config, &text)?;
    let root = application.paths.managed_root();
    let profiles = profile_directory(&root)?;
    let profile = profiles.join(format!("{id}.toml"));
    let image = profiles.join(format!("{id}.png"));
    let _lock = crate::recovery::exclusive_state_lock(&root.join("state.lock"))
        .map_err(|error| CliError::Intent(error.to_string()))?;
    if path_exists(&profile)? || path_exists(&image)? {
        return Err(CliError::Intent(format!(
            "Profile or image {id} already exists; no files changed"
        )));
    }
    create_private(&image, &bytes)?;
    if let Err(error) = create_private(&profile, text.as_bytes()) {
        if !path_exists(&profile)? {
            let _ = fs::remove_file(&image);
        }
        return Err(error);
    }
    drop(_lock);
    writeln!(output, "Saved {id} (gradient-v1, seed {seed}).")?;
    print_saved_preview(&application, &id, output)?;
    writeln!(output, "Run ghostty-wall apply {id} to activate.")?;
    Ok(())
}

fn new_from_source(
    name: &str,
    source: &str,
    candidate: &str,
    apply: bool,
    output: &mut impl Write,
) -> Result<(), CliError> {
    let id = IntentId::from_str(name)?;
    let source_id = IntentId::from_str(source)?;
    let application = Application::load(None)?;
    if !application
        .config
        .sources
        .iter()
        .any(|(key, _)| key == &source_id)
    {
        return Err(CliError::Intent(format!(
            "Source {source} not found; no files changed"
        )));
    }
    let candidate = toml_edit::Value::from(candidate).to_string();
    let text = format!(
        "schema_version = 1\n\n[wallpaper]\nmode = \"source\"\nsource = \"{source}\"\nselection = \"path\"\npath = {candidate}\nfit = \"cover\"\nposition = \"center\"\nopacity = {NEW_PROFILE_WALLPAPER_OPACITY}\n\n[colors]\nmode = \"generated\"\n"
    );
    parse_named_profile_toml(id.as_str(), &application.config, &text)?;
    let root = application.paths.managed_root();
    let _lock = crate::recovery::exclusive_state_lock(&root.join("state.lock"))
        .map_err(|error| CliError::Intent(error.to_string()))?;
    create_private(
        &profile_directory(&root)?.join(format!("{id}.toml")),
        text.as_bytes(),
    )?;
    drop(_lock);
    writeln!(output, "Saved {id}.")?;
    print_saved_preview(&application, &id, output)?;
    if apply {
        print_apply_outcome(application.apply(&id)?, output)?;
    } else {
        writeln!(output, "Run ghostty-wall apply {id} to activate.")?;
    }
    Ok(())
}

fn command_source(args: &[String], output: &mut impl Write) -> Result<(), CliError> {
    if args.first().map(String::as_str) != Some("add") {
        return sources::command(args, output);
    }
    let (id, kind, location, options) = match args {
        [action, id, kind, location, options @ ..] if action == "add" => (id, kind, location, options),
        _ => return Err(CliError::Usage("expected source add SOURCE local DIRECTORY or source add SOURCE github OWNER/REPO [--ref REF] [--path PATH]".into())),
    };
    let mut reference = None;
    let mut subpath = None;
    let mut position = 0;
    while position < options.len() {
        match options.get(position).map(String::as_str) {
            Some("--ref") if kind == "github" && reference.is_none() => {
                reference = Some(
                    options
                        .get(position + 1)
                        .ok_or_else(|| CliError::Usage("--ref requires value".into()))?,
                );
            }
            Some("--path") if kind == "github" && subpath.is_none() => {
                subpath = Some(
                    options
                        .get(position + 1)
                        .ok_or_else(|| CliError::Usage("--path requires value".into()))?,
                );
            }
            _ => return Err(CliError::Usage("invalid Source options".into())),
        }
        position += 2;
    }
    let id = IntentId::from_str(id)?;
    if !matches!(kind.as_str(), "local" | "github") {
        return Err(CliError::Usage(
            "Source kind must be local or github".into(),
        ));
    }
    let paths = process_paths()?;
    let root = paths.managed_root();
    let _lock = crate::recovery::exclusive_state_lock(&root.join("state.lock"))
        .map_err(|error| CliError::Intent(error.to_string()))?;
    let config_path = root.join("config.toml");
    let original = read_text(&config_path)?;
    let parsed = parse_config_toml(&original)?;
    let previous = parsed
        .sources
        .iter()
        .find(|(key, _)| key == &id)
        .map(|(_, source)| source);
    let mut doc: toml_edit::DocumentMut = original
        .parse()
        .map_err(|error: toml_edit::TomlError| CliError::Intent(error.to_string()))?;
    let mut table = toml_edit::Table::new();
    if kind == "local" {
        table["kind"] = toml_edit::value("local-directory");
        table["path"] = toml_edit::value(location.as_str());
    } else {
        table["kind"] = toml_edit::value("github");
        table["repository"] = toml_edit::value(location.as_str());
        if let Some(reference) = reference {
            table["ref"] = toml_edit::value(reference.as_str());
        }
        if let Some(path) = subpath {
            table["path"] = toml_edit::value(path.as_str());
        }
    }
    doc["sources"][id.as_str()] = toml_edit::Item::Table(table);
    let revised = doc.to_string();
    let parsed_revised = parse_config_toml(&revised)?;
    if let Some(old) = previous {
        if parsed_revised
            .sources
            .iter()
            .any(|(key, new)| key == &id && new == old)
        {
            writeln!(output, "Source {id} already configured.")?;
            return Ok(());
        }
        return Err(CliError::Intent(format!(
            "Source {id} already differs; no files changed"
        )));
    }
    atomic_intent_edit(&config_path, revised.as_bytes())?;
    writeln!(output, "Added Source {id}.")?;
    Ok(())
}

fn command_edit(args: &[String], output: &mut impl Write) -> Result<(), CliError> {
    if args.len() <= 1 {
        return editor::command(args, output);
    }
    let [name, field, input] = args else {
        return Err(CliError::Usage(
            "expected edit [PROFILE] or edit PROFILE FIELD VALUE".into(),
        ));
    };
    let id = IntentId::from_str(name)?;
    let application = Application::load(None)?;
    let root = application.paths.managed_root();
    let path = profile_directory(&root)?.join(format!("{id}.toml"));
    let prior = read_text(&path)?;
    let (_, validated) = parse_named_profile_toml(id.as_str(), &application.config, &prior)?;
    let planned_colors = if field.starts_with("colors.")
        && field != "colors.theme"
        && field != "colors.mode"
        && !matches!(
            validated.colors,
            Some(ColorsIntent::Explicit { .. } | ColorsIntent::GeneratedWithOverrides(_))
        )
        && !(validated.schema_version == 2
            && matches!(validated.colors, Some(ColorsIntent::Generated)))
    {
        Some(application.plan(&id)?)
    } else {
        None
    };
    let _lock = crate::recovery::exclusive_state_lock(&root.join("state.lock"))
        .map_err(|error| CliError::Intent(error.to_string()))?;
    let original = read_text(&path)?;
    if original != prior {
        return Err(CliError::Intent(format!(
            "Profile {id} changed while editing; no files changed; retry"
        )));
    }
    let mut doc: toml_edit::DocumentMut = original
        .parse()
        .map_err(|error: toml_edit::TomlError| CliError::Intent(error.to_string()))?;
    let value = match field.as_str() {
        "wallpaper.fit"
        | "wallpaper.position"
        | "wallpaper.source"
        | "wallpaper.path"
        | "terminal.cursor_style"
        | "colors.theme"
        | "colors.mode"
        | "wallpaper.mode" => toml_edit::value(input.as_str()),
        "wallpaper.opacity"
        | "terminal.font_size"
        | "terminal.background_opacity"
        | "terminal.background_blur_intensity"
        | "wallpaper.repeat" => toml_edit::Item::Value(
            input
                .parse::<toml_edit::Value>()
                .map_err(|error| CliError::Usage(format!("invalid {field}: {error}")))?,
        ),
        "colors.background"
        | "colors.foreground"
        | "colors.cursor"
        | "colors.selection_background"
        | "colors.selection_foreground" => toml_edit::value(input.as_str()),
        _ if field.starts_with("colors.palette.") => toml_edit::value(input.as_str()),
        _ => {
            return Err(CliError::Usage(format!(
                "unsupported field {field}; see user guide"
            )));
        }
    };
    if let Some(property) = field.strip_prefix("terminal.") {
        doc["terminal"][property] = value;
    } else if let Some(property) = field.strip_prefix("wallpaper.") {
        if property == "mode" {
            if !matches!(input.as_str(), "none" | "unmanaged") {
                return Err(CliError::Usage(
                    "wallpaper.mode must be none or unmanaged".into(),
                ));
            }
            if input == "unmanaged" {
                doc.remove("wallpaper");
            } else {
                let mut table = toml_edit::Table::new();
                table["mode"] = toml_edit::value("none");
                doc["wallpaper"] = toml_edit::Item::Table(table);
            }
        } else {
            if doc["wallpaper"].is_none() {
                return Err(CliError::Usage(
                    "Profile has no wallpaper; use new or edit existing source wallpaper".into(),
                ));
            }
            if doc["wallpaper"]["mode"].as_str() != Some("source") {
                return Err(CliError::Usage(
                    "wallpaper must use a Source to edit its options".into(),
                ));
            }
            if property == "path" {
                doc["wallpaper"]["selection"] = toml_edit::value("path");
            }
            doc["wallpaper"][property] = value;
        }
    } else if let Some(property) = field.strip_prefix("colors.") {
        if property == "mode" {
            if !matches!(input.as_str(), "generated" | "unmanaged") {
                return Err(CliError::Usage(
                    "colors.mode must be generated or unmanaged; use colors.theme for a theme"
                        .into(),
                ));
            }
            if input == "unmanaged" {
                doc.remove("colors");
            } else {
                let mut table = toml_edit::Table::new();
                table["mode"] = toml_edit::value("generated");
                doc["colors"] = toml_edit::Item::Table(table);
            }
        } else if property == "theme" {
            doc["colors"] = toml_edit::Item::Table(toml_edit::Table::new());
            doc["colors"]["mode"] = toml_edit::value("theme");
            doc["colors"]["theme"] = value;
        } else {
            if validated.schema_version == 2 && doc["colors"]["mode"].as_str() == Some("generated")
            {
                if input != "auto" {
                    input.parse::<crate::domain::Color>()?;
                }
                if doc["colors"].get("overrides").is_none() {
                    doc["colors"]["overrides"] = toml_edit::Item::Table(toml_edit::Table::new());
                }
                if let Some(index) = property.strip_prefix("palette.") {
                    let index: usize = index
                        .parse()
                        .map_err(|_| CliError::Usage("palette index must be 0..15".into()))?;
                    if index >= 16 {
                        return Err(CliError::Usage("palette index must be 0..15".into()));
                    }
                    if doc["colors"]["overrides"].get("palette").is_none() {
                        let mut palette = toml_edit::Array::new();
                        for _ in 0..16 {
                            palette.push("auto");
                        }
                        doc["colors"]["overrides"]["palette"] = toml_edit::value(palette);
                    }
                    doc["colors"]["overrides"]["palette"]
                        .as_array_mut()
                        .ok_or_else(|| CliError::Intent("invalid overrides palette".into()))?
                        .replace(index, input.as_str());
                } else {
                    doc["colors"]["overrides"][property] = value;
                }
            } else {
                if input == "auto" {
                    return Err(CliError::Usage(
                        "auto requires generated colors in a version 2 Profile".into(),
                    ));
                }
                if doc["colors"]["mode"].as_str() != Some("explicit") {
                    let colors = planned_colors
                        .as_ref()
                        .and_then(|plan| plan.pointer("/environment/manifest/colors"))
                        .ok_or_else(|| {
                            CliError::Usage("Profile has no resolved colors to customize".into())
                        })?;
                    let mut table = toml_edit::Table::new();
                    table["mode"] = toml_edit::value("explicit");
                    for key in [
                        "background",
                        "foreground",
                        "cursor",
                        "selection_background",
                        "selection_foreground",
                    ] {
                        if let Some(color) = colors[key].as_str() {
                            table[key] = toml_edit::value(color);
                        }
                    }
                    let palette = colors["palette"]
                        .as_array()
                        .ok_or_else(|| CliError::Intent("resolved colors lack palette".into()))?;
                    let mut values = toml_edit::Array::new();
                    for color in palette {
                        values.push(
                            color.as_str().ok_or_else(|| {
                                CliError::Intent("invalid resolved palette".into())
                            })?,
                        );
                    }
                    table["palette"] = toml_edit::value(values);
                    doc["colors"] = toml_edit::Item::Table(table);
                }
                if let Some(index) = property.strip_prefix("palette.") {
                    let index: usize = index
                        .parse()
                        .map_err(|_| CliError::Usage("palette index must be 0..15".into()))?;
                    if index >= 16 {
                        return Err(CliError::Usage("palette index must be 0..15".into()));
                    }
                    doc["colors"]["palette"]
                        .as_array_mut()
                        .ok_or_else(|| CliError::Intent("invalid explicit palette".into()))?
                        .replace(index, input.as_str());
                } else {
                    doc["colors"][property] = value;
                }
            }
        }
    }
    let revised = doc.to_string();
    parse_named_profile_toml(id.as_str(), &application.config, &revised)?;
    atomic_intent_edit(&path, revised.as_bytes())?;
    drop(_lock);
    writeln!(output, "Updated {id}.")?;
    print_saved_preview(&application, &id, output)
}

fn atomic_intent_edit(path: &Path, bytes: &[u8]) -> Result<(), CliError> {
    intent_edit_result(crate::init::atomic_edit(path, path, bytes))
}

fn atomic_intent_edit_if_unchanged(
    path: &Path,
    bytes: &[u8],
    expected: &[u8],
) -> Result<(), CliError> {
    intent_edit_result(crate::init::atomic_edit_if_unchanged(
        path, path, bytes, expected,
    ))
}

fn intent_edit_result(result: Result<(), crate::init::InitError>) -> Result<(), CliError> {
    match result {
        Err(crate::init::InitError::HookPublicationUncertain { path, source }) => {
            Err(CliError::Intent(format!(
                "Intent edit at {} may already be published; inspect before retrying: {source}",
                path.display()
            )))
        }
        other => other.map_err(CliError::Init),
    }
}

fn print_saved_preview(
    application: &Application,
    id: &IntentId,
    output: &mut impl Write,
) -> Result<(), CliError> {
    match application.plan(id) {
        Ok(plan) => write_text(output, &format_plan_preview(&plan)),
        Err(error) => {
            writeln!(
                output,
                "Profile {id} saved, but preview unavailable: {error}. Fix Source, then run preview {id}."
            )?;
            Ok(())
        }
    }
}

fn command_profile_file(
    args: &[String],
    operation: &str,
    output: &mut impl Write,
) -> Result<(), CliError> {
    let (old, new) = match (operation, args) {
        ("rename" | "duplicate", [old, new]) => (old, Some(IntentId::from_str(new)?)),
        _ => return Err(CliError::Usage(format!("expected {operation} PROFILE NEW"))),
    };
    let old = IntentId::from_str(old)?;
    let application = Application::load(None)?;
    let root = application.paths.managed_root();
    let _lock = crate::recovery::exclusive_state_lock(&root.join("state.lock"))
        .map_err(|error| CliError::Intent(error.to_string()))?;
    let directory = profile_directory(&root)?;
    let original = directory.join(format!("{old}.toml"));
    if operation == "rename" && new.as_ref().is_some_and(|id| id == &old) {
        writeln!(output, "Profile {old} unchanged.")?;
        return Ok(());
    }
    let text = read_text(&original)?;
    parse_named_profile_toml(old.as_str(), &application.config, &text)?;
    if operation == "rename" && old.as_str() == "welcome" {
        return Err(CliError::Intent(
            "Installed Welcome cannot be renamed; no files changed".into(),
        ));
    }
    if operation == "rename" {
        // RFC 0003: a rename must not make the active Profile deletable under
        // another name. Hold the lock for both guards and the intent mutation.
        let history = crate::history::inspect_history_unlocked(&root).map_err(|error| {
            CliError::Intent(format!(
                "cannot {operation} {old}: {error}; no files changed"
            ))
        })?;
        if let Some(latest) = history.latest() {
            if latest.profile_id() == Some(&old) {
                return Err(CliError::Intent(format!(
                    "Profile {old} is active; no files changed. Apply another Profile before {operation}"
                )));
            }
            if latest.profile_id().is_none() {
                return Err(CliError::Intent(format!(
                    "Cannot determine an active Profile after History replay; no files changed. Apply another Profile before {operation} of {old}"
                )));
            }
        }
    }
    match operation {
        "duplicate" => {
            let target =
                directory.join(format!("{}.toml", new.as_ref().expect("validated new ID")));
            if path_exists(&target)? {
                if read_text(&target)? != text {
                    return Err(CliError::Intent(format!(
                        "Profile {} already differs; no files changed",
                        new.as_ref().expect("validated new ID")
                    )));
                }
            } else {
                create_private(&target, text.as_bytes())?;
            }
        }
        "rename" => {
            let target =
                directory.join(format!("{}.toml", new.as_ref().expect("validated new ID")));
            if path_exists(&target)? {
                return Err(CliError::Intent(format!(
                    "Profile {} already exists; no files changed",
                    new.as_ref().expect("validated new ID")
                )));
            }
            fs::rename(&original, target)?;
        }
        _ => unreachable!(),
    }
    fs::File::open(&directory)?.sync_all().map_err(|error| {
        CliError::Intent(format!(
            "Profile {operation} may already be published at {}; inspect before retrying: {error}",
            directory.display()
        ))
    })?;
    writeln!(
        output,
        "{operation} Profile {old}{}.",
        new.map_or(String::new(), |id| format!(" → {id}"))
    )?;
    Ok(())
}

fn path_exists(path: &Path) -> Result<bool, CliError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn existing_equal(path: &Path, expected: &[u8]) -> Result<bool, CliError> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    let file = options.open(path)?;
    if !file.metadata()?.is_file() || file.metadata()?.len() != expected.len() as u64 {
        return Ok(false);
    }
    let mut bytes = Vec::new();
    file.take(expected.len() as u64 + 1)
        .read_to_end(&mut bytes)?;
    Ok(bytes == expected)
}

fn profile_directory(root: &Path) -> Result<PathBuf, CliError> {
    for path in [root.to_owned(), root.join("profiles")] {
        let metadata = fs::symlink_metadata(&path)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(CliError::Intent(format!(
                "unsafe managed directory {}; no files changed",
                path.display()
            )));
        }
    }
    Ok(root.join("profiles"))
}

fn create_private(path: &Path, bytes: &[u8]) -> Result<(), CliError> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let parent = path
        .parent()
        .ok_or_else(|| CliError::Intent("invalid Profile path".into()))?;
    let temp = parent.join(format!(
        ".tmp-new-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        // Publish only complete bytes; hard link refuses an existing final path.
        fs::hard_link(&temp, path)?;
        fs::File::open(parent)?.sync_all().map_err(|error| {
            CliError::Intent(format!(
                "File {} may already be published; inspect before retrying: {error}",
                path.display()
            ))
        })?;
        Ok::<(), CliError>(())
    })();
    let _ = fs::remove_file(&temp);
    result
}

fn command_list(output: &mut impl Write) -> Result<(), CliError> {
    let application = Application::load(None)?;
    for profile in profile_ids(&application.paths.managed_root().join("profiles"))? {
        match application.load_profile(&profile) {
            Ok(_) => writeln!(output, "{profile}")?,
            Err(error) => writeln!(output, "{profile} (invalid: {error})")?,
        }
    }
    Ok(())
}

fn command_history(output: &mut impl Write) -> Result<(), CliError> {
    let history = crate::history::inspect_history(&process_paths()?.managed_root())
        .map_err(|error| CliError::Intent(error.to_string()))?;
    for activation in history.activations() {
        writeln!(
            output,
            "{} {}",
            activation.sequence(),
            activation.environment_id()
        )?;
    }
    Ok(())
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
    if matches!(args, [flag] if flag == "--help" || flag == "-h") {
        return write_text(
            output,
            "Usage: ghostty-wall tui [--seed HEX]\n\nProfile management: n Create, e Edit draft, x Delete (confirm), a Use.\nArrows/j/k automatically preview without activating, even while loading.\nCompact layouts show list and sample; Enter/p enlarges the sample.\nTab switches Profiles/Sources; ? opens all Actions.\nv shows scrollable result/error details; q quits.\nCreate: generate or choose an image, then review. Edit: Wallpaper/Colors/Terminal.\nBoth: s Save without applying, u Save and use, Esc/q Cancel.\nSave and use confirms (default: Back to editor); declining keeps the draft.\nA definite save failure retains the draft; uncertainty requires inspection.\nWide forms show a terminal-like sample beside controls; p toggles it when small.\nManagement minimum 60x18 preserves photo space; smaller windows show resize\nguidance (Esc cancels). Create/Edit forms still support 40x12. Compact lists\nelide long IDs; the header shows the selection and * marks the active Profile.\nAll actions, nested image pickers, reports and first-start initialization stay full-screen.\nInput forms: Enter next/submit, Tab/Shift-Tab fields, Ctrl-U clear, Esc cancel.\nInvalid input retains values; F1 shows error details.\nDelete: y confirms, Enter/n/Esc/Ctrl-C cancels; arrows scroll the summary.\nCreate errors: F1 details. Editor/main results: v details.\nGhostty receives a static image behind sample text; other terminals show a\nlabelled color-cell fallback. Wallpaper uses linear-light sRGB (Ghostty's\nLinux default), not inferred native/P3 blending. Saved opacity/RGB are unchanged.\nInherited settings are illustrative; v explains limitations. Preview errors never activate. Random Use resolves again; a\nchanged Source may change the candidate. Local Profile/image changes invalidate\nthe sample. Internal previews are NOT live Ghostty reload. Use commits an\nActivation; best-effort reload is reported separately, never visually verified.\nAdvanced field edits (f/c/t/w) save immediately; Sources (S Show, E Edit, C Check, Z Remove), History, previous,\nsettings/Source configuration, doctor, init/repair, update and uninstall remain in Actions.\nReports: arrows/PageUp/PageDown scroll, Enter/Esc back. i views the original image.\nMaintenance mutations require y; Enter/n/Esc/Ctrl-C/D declines.\nLong jobs show progress; Esc closes read-only jobs (work may finish in background).\nOnce a mutation starts, wait for completion/recovery; cancellation is unavailable.\nUninstall returns to the browser and preserves Intent/History.\nRestart Ghostty Wall after an installed update; this process keeps its old version.\nNon-terminal input retains the legacy line-based browser (key then Enter).\n",
        );
    }
    let seed = parse_seed_only(args)?;
    let paths = process_paths()?;
    if io::stdin().is_terminal() && io::stdout().is_terminal() {
        let mut screen = editor::Screen::open(output)?;
        if !paths.managed_root().join("config.toml").exists()
            && !maintenance::fresh_start(screen.terminal.backend_mut())?
        {
            return Ok(());
        }
        let result = (|| {
            let mut application = Application::load(seed)?;
            let profiles = profile_ids(&application.paths.managed_root().join("profiles"))?;
            let sources = application
                .config
                .sources
                .iter()
                .map(|(id, _)| id.clone())
                .collect();
            let mut browser = TerminalBrowser::new(sources, profiles);
            management::run(
                screen.terminal.backend_mut(),
                &mut application,
                &mut browser,
                seed,
            )
        })();
        if let Err(error) = &result {
            management::details(
                screen.terminal.backend_mut(),
                &format!(
                    "Cannot open browser: {error}\nInspect the reported state before retrying."
                ),
            )?;
        }
        return result;
    }
    if !paths.managed_root().join("config.toml").exists() {
        write_text(
            output,
            "Not initialized. Type i then Enter to initialize, or q to quit: ",
        )?;
        output.flush()?;
        let stdin = io::stdin();
        if stdin.lock().lines().next().transpose()?.as_deref() != Some("i") {
            return Ok(());
        }
        print_init_report(init(&paths)?, output)?;
    }
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
            Some(line)
                if matches!(
                    line.trim(),
                    "n" | "m"
                        | "o"
                        | "S"
                        | "E"
                        | "C"
                        | "Z"
                        | "e"
                        | "r"
                        | "d"
                        | "x"
                        | "h"
                        | "s"
                        | "?"
                        | "t"
                        | "c"
                        | "w"
                        | "l"
                        | "P"
                        | "p"
                        | "D"
                        | "u"
                        | "U"
                        | "I"
                        | "y"
                        | "R"
                        | "W"
                        | "Y"
                        | "M"
                        | "X"
                ) =>
            {
                match tui_command(
                    line.trim(),
                    &mut lines,
                    output,
                    &mut application,
                    &mut browser,
                    seed,
                    true,
                ) {
                    Ok(()) => {}
                    Err(error) => {
                        writeln!(input_errors, "{error}")?;
                    }
                }
                continue;
            }
            Some(line) => match line.trim() {
                "j" => BrowserAction::Down,
                "k" => BrowserAction::Up,
                "tab" => BrowserAction::NextPane,
                "" | "enter" => BrowserAction::Preview,
                "a" => BrowserAction::Apply,
                "b" | "esc" => BrowserAction::Back,
                "q" => BrowserAction::Cancel,
                _ => {
                    writeln!(
                        input_errors,
                        "keys: ? actions, n new, tab pane, j/k move, enter preview, a apply, b back, q quit"
                    )?;
                    continue;
                }
            },
            None => BrowserAction::Cancel,
        };
        if action == BrowserAction::Preview
            && browser.mode() == BrowserMode::Browse
            && browser.focus() == BrowserFocus::Sources
            && browser.selected_source().is_some()
        {
            if let Err(error) = tui_command(
                "m",
                &mut lines,
                output,
                &mut application,
                &mut browser,
                seed,
                true,
            ) {
                writeln!(input_errors, "{error}")?;
            }
            continue;
        }
        if let Err(error) = browser.dispatch(action, &mut application) {
            writeln!(input_errors, "{error}")?;
        }
    }
}

fn prompt_tui(
    input: &mut impl Iterator<Item = io::Result<String>>,
    output: &mut impl Write,
    label: &str,
) -> Result<Option<String>, CliError> {
    presentation::prompt(output, label)?;
    output.flush()?;
    Ok(input
        .next()
        .transpose()?
        .map(|value| value.trim().to_owned())
        .filter(|value| value != "b" && value != "esc"))
}

fn tui_command(
    action: &str,
    input: &mut impl Iterator<Item = io::Result<String>>,
    output: &mut impl Write,
    application: &mut Application,
    browser: &mut TerminalBrowser,
    seed: Option<ResolutionSeed>,
    resolve_preview: bool,
) -> Result<(), CliError> {
    macro_rules! ask {
        ($label:expr) => {
            match prompt_tui(input, output, $label)? {
                Some(value) => value,
                None => return Ok(()),
            }
        };
    }
    let selected = browser.selected_profile().cloned();
    if matches!(action, "U" | "X" | "p" | "M") {
        presentation::heading(output, "Confirm maintenance action:")?;
        presentation::line(
            output,
            presentation::Role::Warning,
            "Warning: Only explicit confirmation performs this action.",
        )?;
        presentation::line(
            output,
            presentation::Role::Choice,
            "Enter or b: cancel (default)",
        )?;
    }
    let target = match action {
        "?" => {
            write_text(
                output,
                "Tab pane  ↑↓/j/k move  Enter use Source / preview Profile  a apply  i image (full-screen)  b back  q quit\nActions (press ? in full-screen browser):\n",
            )?;
            for (key, label) in tui::ACTIONS {
                writeln!(output, "{key}  {label}")?;
            }
            write_text(output, "Type b at prompt to go back.\n")?;
            return Ok(());
        }
        "S" | "E" | "C" | "Z" => {
            sources::line_flow(action, browser.selected_source().cloned(), input, output)?;
            None
        }
        "l" => {
            command_list(output)?;
            return Ok(());
        }
        "P" => {
            let profile = selected.ok_or_else(|| CliError::Usage("select Profile first".into()))?;
            serde_json::to_writer_pretty(&mut *output, &application.plan(&profile)?)?;
            writeln!(output)?;
            return Ok(());
        }
        "D" => {
            command_doctor(output)?;
            return Ok(());
        }
        "u" => {
            update::run(true, output).map_err(CliError::Update)?;
            return Ok(());
        }
        "U" => {
            if ask!("Type update to install release (b to cancel): ") == "update" {
                update::run(false, output).map_err(CliError::Update)?;
            }
            return Ok(());
        }
        "X" => {
            if ask!("Type uninstall to remove integration (Intent and History stay): ")
                == "uninstall"
            {
                command_uninstall(output)?;
                browser.dispatch(BrowserAction::Cancel, application)?;
            }
            return Ok(());
        }
        "p" => {
            if ask!("Type previous to replay prior Environment (b to cancel): ") != "previous" {
                return Ok(());
            }
            command_previous(output)?;
            None
        }
        "I" | "y" | "R" | "W" | "Y" | "M" => {
            if action == "M" && ask!("Type migrate to import legacy configuration: ") != "migrate" {
                return Ok(());
            }
            let args: &[&str] = match action {
                "I" => &[],
                "y" => &["--dry-run"],
                "R" => &["--repair"],
                "W" => &["--welcome"],
                "Y" => &["--migrate-legacy", "--dry-run"],
                _ => &["--migrate-legacy"],
            };
            command_init(
                &args.iter().map(|arg| (*arg).into()).collect::<Vec<_>>(),
                output,
            )?;
            None
        }
        "h" => {
            writeln!(
                output,
                "History (sequence Environment; empty until first apply):"
            )?;
            command_history(output)?;
            return Ok(());
        }
        "s" => {
            writeln!(
                output,
                "Settings: Managed Root {}",
                application.paths.managed_root().display()
            )?;
            writeln!(
                output,
                "Sources: {}. Profiles: {}.",
                application.config.sources.len(),
                profile_ids(&application.paths.managed_root().join("profiles"))?.len()
            )?;
            writeln!(
                output,
                "Use doctor for integration checks, init --repair for explicit repair, update to update binary."
            )?;
            return Ok(());
        }
        "n" | "N" => {
            let name = ask!("New Profile ID: ");
            let image = ask!("PNG/JPEG image path: ");
            command_new(&[name.clone(), image], output)?;
            Some(IntentId::from_str(&name)?)
        }
        "m" => {
            let name = ask!("New Profile ID: ");
            let source = if browser.focus() == BrowserFocus::Sources {
                match browser.selected_source() {
                    Some(id) => {
                        writeln!(output, "Source: {id}")?;
                        id.to_string()
                    }
                    None => ask!("Source ID (see Sources pane): "),
                }
            } else {
                ask!("Source ID (see Sources pane): ")
            };
            let path = ask!("Candidate path relative to Source root: ");
            new_from_source(&name, &source, &path, false, output)?;
            Some(IntentId::from_str(&name)?)
        }
        "o" => {
            let name = ask!("New Source ID: ");
            presentation::choices(
                output,
                "Source kind:",
                &[
                    "local  Directory",
                    "github  GitHub repository",
                    "b  Cancel (no default)",
                ],
            )?;
            let kind = ask!("Source kind (local/github): ");
            let location = ask!("Directory path or owner/repo: ");
            let mut args = vec!["add".into(), name, kind.clone(), location];
            if kind == "github" {
                let reference = ask!("Ref (blank for default): ");
                if !reference.is_empty() {
                    args.extend(["--ref".into(), reference]);
                }
                let path = ask!("Subdirectory (blank for repository root): ");
                if !path.is_empty() {
                    args.extend(["--path".into(), path]);
                }
            }
            command_source(&args, output)?;
            None
        }
        "e" | "f" | "c" | "t" | "w" => {
            let profile = selected.ok_or_else(|| {
                CliError::Usage("select Profile first (tab switches pane)".into())
            })?;
            let prefix = match action {
                "c" => "colors.",
                "t" => "terminal.",
                "w" => "wallpaper.",
                _ => "",
            };
            let fields = match action {
                "c" => {
                    "Color fields: mode, theme, background, foreground, cursor, selection_background, selection_foreground, palette.0..15"
                }
                "t" => {
                    "Terminal fields: font_size, background_opacity, background_blur_intensity, cursor_style"
                }
                "w" => "Wallpaper fields: mode, source, path, fit, position, opacity, repeat",
                _ => "Fields: wallpaper.*, colors.*, terminal.* (see user guide)",
            };
            presentation::heading(output, fields)?;
            let key = ask!("Field: ");
            let field = if !prefix.is_empty() && !key.contains('.') {
                format!("{prefix}{key}")
            } else {
                key
            };
            let value = ask!("Value: ");
            command_edit(&[profile.to_string(), field, value], output)?;
            Some(profile)
        }
        "r" | "d" | "x" => {
            let profile = selected.ok_or_else(|| {
                CliError::Usage("select Profile first (tab switches pane)".into())
            })?;
            if action == "x" {
                delete::flow(Some(profile.as_str()), input, output)?;
                None
            } else {
                let name = ask!("New Profile ID: ");
                let operation = if action == "r" { "rename" } else { "duplicate" };
                command_profile_file(&[profile.to_string(), name.clone()], operation, output)?;
                Some(IntentId::from_str(&name)?)
            }
        }
        _ => return Ok(()),
    };
    *application = Application::load(seed)?;
    let profiles = profile_ids(&application.paths.managed_root().join("profiles"))?;
    let selected_index = target
        .as_ref()
        .and_then(|id| profiles.iter().position(|item| item == id));
    let sources = application
        .config
        .sources
        .iter()
        .map(|(id, _)| id.clone())
        .collect();
    *browser = TerminalBrowser::new(sources, profiles);
    if let Some(index) = selected_index {
        browser.dispatch(BrowserAction::NextPane, application)?;
        for _ in 0..index {
            browser.dispatch(BrowserAction::Down, application)?;
        }
        if resolve_preview {
            browser.dispatch(BrowserAction::Preview, application)?;
        }
    }
    Ok(())
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
        self.plan_intent(profile_id, &profile)
    }

    fn plan_intent(
        &self,
        profile_id: &IntentId,
        profile: &ProfileIntent,
    ) -> Result<Value, CliError> {
        let root = self.paths.managed_root();
        let reload = platform_reload_adapter();
        let platform = PlanPlatform::new(self.paths.ghostty_root_config(), reload.observation());
        let github = uses_github(&self.config, profile);
        let theme = matches!(profile.colors, Some(ColorsIntent::Theme { .. }));
        let seed = self.seed.as_ref();
        let result = match (github, theme) {
            (true, true) => plan_github_profile_with_theme_json(
                &root,
                &self.paths.home,
                &root,
                profile_id,
                &self.config,
                profile,
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
                profile,
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
                profile,
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
                profile,
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
    ids.sort();
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

fn reload_status(outcome: crate::runtime::ReloadOutcome) -> &'static str {
    match outcome {
        crate::runtime::ReloadOutcome::Succeeded => {
            "action accepted; visible change is not verified"
        }
        crate::runtime::ReloadOutcome::Unavailable(_) => {
            "unavailable; Activation remains committed"
        }
        crate::runtime::ReloadOutcome::Failed(_) => "failed; Activation remains committed",
    }
}

fn saved_report(id: &IntentId) -> String {
    format!("Profile saved.\nSaved Profile {id}.\nTerminal unchanged.\n")
}

fn activation_report(
    activation: crate::domain::ActivationId,
    reload: crate::runtime::ReloadOutcome,
    profile: Option<&IntentId>,
) -> String {
    let message = match reload {
        crate::runtime::ReloadOutcome::Succeeded => "Configuration updated; reload requested.",
        _ => "Configuration updated; reload Ghostty manually.",
    };
    let profile = profile
        .map(|id| format!(" for Profile {id}"))
        .unwrap_or_default();
    format!(
        "{message}\nActivated {activation}{profile}.\nGhostty reload: {}.\nReload details: {reload:?}.\n",
        reload_status(reload)
    )
}

fn completion_report(id: &IntentId, outcome: crate::profile_workflow::ProfileOutcome) -> String {
    match outcome {
        crate::profile_workflow::ProfileOutcome::Saved => saved_report(id),
        crate::profile_workflow::ProfileOutcome::SavedAndApplied { activation, reload } => {
            activation_report(activation, reload, Some(id))
        }
    }
}

fn print_apply_outcome(outcome: ApplyOutcome, output: &mut impl Write) -> Result<(), CliError> {
    write_text(
        output,
        &activation_report(outcome.activation_id(), outcome.reload_outcome(), None),
    )
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
    Input(String),
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
    Update(UpdateError),
    Workflow(crate::profile_workflow::WorkflowError),
}

impl CliError {
    fn exit_code(&self) -> i32 {
        match self {
            Self::Usage(_) | Self::Input(_) => 2,
            Self::Intent(_) => 3,
            Self::Workflow(error) => match error {
                crate::profile_workflow::WorkflowError::Id(_) => 2,
                crate::profile_workflow::WorkflowError::Io { .. }
                | crate::profile_workflow::WorkflowError::SaveFailed { .. }
                | crate::profile_workflow::WorkflowError::PublicationUncertain { .. }
                | crate::profile_workflow::WorkflowError::RollbackIncomplete { .. }
                | crate::profile_workflow::WorkflowError::Apply(_)
                | crate::profile_workflow::WorkflowError::Fallback { .. }
                | crate::profile_workflow::WorkflowError::DeletionIncomplete { .. }
                | crate::profile_workflow::WorkflowError::History(_) => 6,
                _ => 3,
            },
            Self::Plan(error) => error.exit_status(),
            Self::JsonPlan(code) => *code,
            Self::Apply(_)
            | Self::DoctorFailed
            | Self::Init(_)
            | Self::Lifecycle(_)
            | Self::Update(_) => 6,
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
            Self::Input(message) | Self::Intent(message) | Self::Internal(message) => {
                formatter.write_str(message)
            }
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
            Self::Update(error) => error.fmt(formatter),
            Self::Workflow(error @ crate::profile_workflow::WorkflowError::Apply(_)) => {
                write!(
                    formatter,
                    "Profile saved; applying failed. See details.\n{error}"
                )
            }
            Self::Workflow(error) => error.fmt(formatter),
        }
    }
}

impl From<crate::profile_workflow::WorkflowError> for CliError {
    fn from(error: crate::profile_workflow::WorkflowError) -> Self {
        match error {
            crate::profile_workflow::WorkflowError::Id(error) => error.into(),
            other => Self::Workflow(other),
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
        if error == crate::domain::ValidationError::InvalidIntentId {
            return Self::Input("Profile and Source names must use lowercase letters, digits, and single internal hyphens (e.g. mia-prova); length 1..=64 bytes".into());
        }
        Self::Input(error.to_string())
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

#[cfg(test)]
mod create_cli_tests {
    use super::*;

    #[test]
    fn user_directories_are_data_not_shell_and_preserve_localized_paths() {
        let home = Path::new("/home/example");
        for (value, expected) in [
            (
                r#""$HOME/Scaricati personali" # localized"#,
                "/home/example/Scaricati personali",
            ),
            (r#""/media/Immagini""#, "/media/Immagini"),
            (
                r#""$HOME/Foto \"estate\"""#,
                "/home/example/Foto \"estate\"",
            ),
            (r#""$HOME/Foto\$""#, "/home/example/Foto$"),
            (r#""$HOME""#, "/home/example"),
        ] {
            assert_eq!(
                parse_user_directory(value, home),
                Some(PathBuf::from(expected)),
                "{value}"
            );
        }
        for invalid in [
            r#""relative/path""#,
            r#""$OTHER/Pictures""#,
            r#""$(touch /tmp/not-executed)""#,
            r#""/tmp/`command`""#,
            r#""/tmp/valid"; command"#,
            r#""unterminated"#,
        ] {
            assert_eq!(parse_user_directory(invalid, home), None, "{invalid}");
        }
    }
}

#[cfg(test)]
mod update_cli_tests {
    use super::*;

    #[test]
    fn update_options() {
        assert!(!parse_update_options(&[]).unwrap());
        assert!(parse_update_options(&["--check".into()]).unwrap());
        assert!(parse_update_options(&["--force".into()]).is_err());
        assert!(parse_update_options(&["--check".into(), "--force".into()]).is_err());
        assert!(HELP.contains("ghostty-wall update [--check]"));
    }
}
