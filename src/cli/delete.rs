//! CLI and browser share the same default-cancel deletion flow and application service.

use super::*;
use crate::profile_workflow::ProfileWorkflows;

const DELETE_HELP: &str = "Usage: ghostty-wall delete [PROFILE]\n\nOmit PROFILE to select one; [active] marks the latest Profile Activation.\nA supplied missing id is an error. Welcome cannot be deleted.\nConfirmation names the Profile and owned data eligible for removal.\nCancel is the default. Answer y then Enter to delete; Enter, n, cancel, or EOF cancels.\nNo --yes bypass is provided. TUI Delete uses the same checks in a full-screen\nconfirmation: y confirms; Enter/n/Esc/Ctrl-C cancels, arrows scroll.\n\nInactive deletion does not change Projection, History, or request reload.\nActive deletion commits existing Welcome before removing the Profile, under\none writer lock. Failed fallback/reconciliation retains the Profile.\nOlder installs without Welcome must make it available explicitly or apply\nanother Profile first. After History replay, apply a Profile before deleting.\nIf Intent, managed directories, or the current Activation changes while\nconfirming, start again.\n\nOnly the Profile file and proven-exclusive owned image are removed. Shared,\nreused, replaced, or ambiguous images remain; original user images, Sources, History,\nEnvironments, and Durable Assets are preserved for replay.\nRemoval isolates and verifies the captured file before unlinking it, so a\nreplacement at the public filename never inherits confirmation. A Profile\nfound again during removal or the renewed ownership check keeps its image;\ndeletion is incomplete and needs fresh confirmation. Committed Welcome\nfallback is not undone. Interrupted removal or failed restoration can retain\nfiles under .tmp-delete-<token>/ in the Managed Root; inspect the reported\npath before retrying. These files are not automatically restored or cleaned up.\nFallback can remain committed even if subsequent removal fails. Reload is\nbest-effort and reported separately, not proof of visible Ghostty change.\n";

pub(super) fn command(args: &[String], output: &mut impl Write) -> Result<(), CliError> {
    let name = match args {
        [] => None,
        [flag] if flag == "--help" || flag == "-h" => return write_text(output, DELETE_HELP),
        [name] => Some(name.as_str()),
        _ => {
            return Err(CliError::Input(
                "expected delete [PROFILE]; see delete --help".into(),
            ));
        }
    };
    let stdin = io::stdin();
    flow(name, &mut stdin.lock().lines(), output)
}

pub(super) fn flow(
    name: Option<&str>,
    input: &mut impl Iterator<Item = io::Result<String>>,
    output: &mut impl Write,
) -> Result<(), CliError> {
    let paths = process_paths()?;
    let name = match name {
        Some(name) => name.to_owned(),
        None => {
            let profiles = profile_ids(&paths.managed_root().join("profiles"))?;
            let history = crate::history::inspect_history(&paths.managed_root()).map_err(|e| {
                CliError::Intent(format!("Cannot select a Profile: {e}; no files changed"))
            })?;
            let active = history.latest().and_then(|a| a.profile_id());
            presentation::heading(
                output,
                "Select Profile to delete (number or id; Enter cancels):",
            )?;
            writeln!(output)?;
            for (index, id) in profiles.iter().enumerate() {
                presentation::line(
                    output,
                    presentation::Role::Choice,
                    &format!(
                        "  {}. {id}{}{}",
                        index + 1,
                        if active == Some(id) { " [active]" } else { "" },
                        if id.as_str() == "welcome" {
                            " (protected fallback)"
                        } else {
                            ""
                        }
                    ),
                )?;
            }
            loop {
                let Some(choice) = create_prompt(input, output, "Profile (default: Cancel): ")?
                else {
                    return cancelled(output);
                };
                if choice.is_empty() {
                    return cancelled(output);
                }
                let selected = choice
                    .parse::<usize>()
                    .ok()
                    .and_then(|n| n.checked_sub(1))
                    .and_then(|n| profiles.get(n))
                    .or_else(|| profiles.iter().find(|id| id.as_str() == choice));
                if let Some(id) = selected {
                    break id.to_string();
                }
                presentation::error(
                    output,
                    "Choose a listed Profile, or cancel; no files changed.",
                )?;
            }
        }
    };
    let mut report = Vec::new();
    with_confirmation(&name, &mut report, |summary| {
        for (index, line) in summary.lines().enumerate() {
            if index == 0 {
                presentation::heading(output, line)?;
            } else if line.starts_with("Warning:") {
                presentation::line(output, presentation::Role::Warning, line)?;
            } else {
                writeln!(output, "{line}")?;
            }
        }
        presentation::choices(
            output,
            "Confirm deletion:",
            &["[y] Delete", "[n] Cancel (default)"],
        )?;
        let confirm = create_prompt(input, output, "Choice (y/n; default: n): ")?;
        Ok(confirm
            .is_some_and(|s| matches!(s.to_ascii_lowercase().as_str(), "y" | "yes" | "delete")))
    })?;
    for line in String::from_utf8_lossy(&report).lines() {
        if line.starts_with("Profile ") && line.contains(" deleted.") {
            presentation::line(output, presentation::Role::Success, line)?;
        } else {
            writeln!(output, "{line}")?;
        }
    }
    Ok(())
}

pub(super) fn with_confirmation(
    name: &str,
    output: &mut impl Write,
    confirm: impl FnOnce(&str) -> Result<bool, CliError>,
) -> Result<(), CliError> {
    let workflow = ProfileWorkflows::load(process_paths()?)?;
    let request = workflow.prepare_deletion(name)?;
    let mut summary = Vec::new();
    {
        let output = &mut summary;
        writeln!(output, "Delete Profile {}?", request.id)?;
        writeln!(output, "Remove: profiles/{}.toml", request.id)?;
        match request.image_path() {
            Some(path) => writeln!(
                output,
                "Eligible owned image: {} (only if still proven exclusive).",
                path.display()
            )?,
            None => writeln!(output, "Images retained: no proven-exclusive owned copy.")?,
        }
        writeln!(
            output,
            "Preserve: original images, Sources, History, Environments, Durable Assets."
        )?;
        if request.active {
            writeln!(
                output,
                "Warning: Profile {} is active: apply Welcome durably before removal; reload is best-effort.",
                request.id
            )?;
        } else {
            writeln!(
                output,
                "Inactive Profile: terminal appearance and History unchanged."
            )?;
        }
    }
    if !confirm(&String::from_utf8_lossy(&summary))? {
        return cancelled(output);
    }
    let outcome = workflow.confirm_deletion(
        &request,
        &timestamp()?,
        |id, intent| {
            let seed = if matches!(
                intent.wallpaper,
                Some(WallpaperIntent::Source {
                    selection: crate::domain::WallpaperSelection::Random,
                    ..
                })
            ) {
                let mut bytes = [0; 32];
                fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
                Some(ResolutionSeed::from_str(
                    &crate::domain::Sha256Digest::from_bytes(bytes).to_string(),
                )?)
            } else {
                None
            };
            let application = Application::load(seed)?;
            if application.config != *workflow.config() {
                return Err(crate::profile_workflow::WorkflowError::Invalid(
                    "Source registry changed; start delete again",
                )
                .into());
            }
            let plan = application.plan_intent(id, intent)?;
            let bytes = if uses_github_plan(&plan) {
                planned_github_asset_bytes(&plan, &application.github)
            } else {
                planned_local_asset_bytes(&plan)
            }
            .map_err(CliError::Plan)?;
            Ok::<_, CliError>((plan, bytes))
        },
        platform_reload_adapter(),
    )?;
    if let Some((activation, reload)) = outcome.fallback {
        writeln!(
            output,
            "Welcome Activation {activation} committed before removal."
        )?;
        let status = match reload {
            crate::runtime::ReloadOutcome::Succeeded => {
                "action accepted; visible change is not verified"
            }
            crate::runtime::ReloadOutcome::Unavailable(_) => {
                "unavailable; Welcome Activation remains committed"
            }
            crate::runtime::ReloadOutcome::Failed(_) => {
                "failed; Welcome Activation remains committed"
            }
        };
        writeln!(output, "Ghostty reload: {status}.")?;
    }
    writeln!(
        output,
        "Profile {} deleted. {}",
        request.id, outcome.cleanup
    )?;
    Ok(())
}

fn cancelled(output: &mut impl Write) -> Result<(), CliError> {
    write_text(output, "Deletion cancelled; no files changed.\n")
}
