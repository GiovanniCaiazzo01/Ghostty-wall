//! Source maintenance changes only the registry; saved recipes and replay data stay untouched.
use super::*;

pub(super) const HELP: &str = "Usage: ghostty-wall source list\n  ghostty-wall source show SOURCE\n  ghostty-wall source check SOURCE\n  ghostty-wall source add SOURCE local DIRECTORY\n  ghostty-wall source add SOURCE github OWNER/REPO [--ref REF] [--path PATH]\n  ghostty-wall source edit SOURCE local DIRECTORY\n  ghostty-wall source edit SOURCE github OWNER/REPO [--ref REF] [--path PATH]\n  ghostty-wall source remove SOURCE\n\nEdit replaces the complete definition, retaining its ID and kind. Omitted GitHub\nref/path reset to defaults. Valid unavailable locations can be saved; use check\nto test availability. Future resolution may choose a different wallpaper.\nShow lists dependent saved Profiles without resolving. Invalid/unreadable\nProfiles block edit/removal, never count as unused. Remove requires explicit y;\nEnter/n/cancel/EOF decline. Only the registry entry is removed, never images,\nProfiles, Environments or History. Concurrent changes abort publication.\nCheck lists PNG/JPEG candidates without downloading/decoding every image; a\nsuccessful listing does not prove every image will decode. No apply or reload.\n";

pub(super) fn command(args: &[String], output: &mut impl Write) -> Result<(), CliError> {
    match args {
        [flag] if flag == "--help" || flag == "-h" => write_text(output, HELP),
        [action] if action == "list" => {
            let paths = process_paths()?;
            let config = parse_config_toml(&read_text(&paths.managed_root().join("config.toml"))?)?;
            for (id, source) in &config.sources {
                writeln!(output, "{id} {}", kind(source))?;
            }
            Ok(())
        }
        [action, name] if matches!(action.as_str(), "show" | "check" | "remove") => {
            let id = name.parse()?;
            if action == "check" {
                return check(&process_paths()?, &id, output);
            }
            let snapshot = Snapshot::load(id)?;
            if action == "show" {
                return write_text(output, &snapshot.summary()?);
            }
            remove(&snapshot, &mut io::stdin().lock().lines(), output)
        }
        [action, name, source_kind, location, options @ ..] if action == "edit" => {
            let snapshot = Snapshot::load(name.parse()?)?;
            let revised = snapshot.revised(source_kind, location, options)?;
            let users = snapshot.users()?;
            snapshot.publish(&revised, false)?;
            writeln!(
                output,
                "Updated Source {}.\nProfiles: {}.\nFuture resolution may choose a different wallpaper; terminal unchanged. Availability not checked; use source check {}.",
                snapshot.id,
                names(&users),
                snapshot.id
            )?;
            Ok(())
        }
        _ => Err(CliError::Input(HELP.into())),
    }
}

fn kind(source: &SourceIntent) -> &'static str {
    match source {
        SourceIntent::LocalDirectory { .. } => "local",
        SourceIntent::Github { .. } => "github",
    }
}

fn names(ids: &[IntentId]) -> String {
    if ids.is_empty() {
        "none".into()
    } else {
        ids.iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    }
}

struct Snapshot {
    paths: InitPaths,
    id: IntentId,
    original: String,
    config: ConfigIntent,
}

impl Snapshot {
    fn load(id: IntentId) -> Result<Self, CliError> {
        let paths = process_paths()?;
        let root = paths.managed_root();
        // Atomic registry reads need no mutation lock. Publication rechecks under the lock.
        let original = read_text(&root.join("config.toml"))?;
        let config = parse_config_toml(&original)?;
        let snapshot = Self {
            paths,
            id,
            original,
            config,
        };
        snapshot.source()?;
        Ok(snapshot)
    }

    fn source(&self) -> Result<&SourceIntent, CliError> {
        self.config
            .sources
            .iter()
            .find(|(id, _)| id == &self.id)
            .map(|(_, source)| source)
            .ok_or_else(|| {
                CliError::Intent(format!("unknown Source {}; no files changed", self.id))
            })
    }

    fn users(&self) -> Result<Vec<IntentId>, CliError> {
        users(&self.paths.managed_root(), &self.config, &self.id)
    }

    fn document(&self) -> Result<toml_edit::DocumentMut, CliError> {
        self.original
            .parse()
            .map_err(|error: toml_edit::TomlError| CliError::Intent(error.to_string()))
    }

    fn summary(&self) -> Result<String, CliError> {
        let definition = self.document()?["sources"][self.id.as_str()].to_string();
        Ok(format!(
            "Source {} ({})\n{}\nProfiles: {}.\nAvailability not checked; use source check {}.\n",
            self.id,
            kind(self.source()?),
            definition,
            names(&self.users()?),
            self.id
        ))
    }

    fn revised(
        &self,
        source_kind: &str,
        location: &str,
        options: &[String],
    ) -> Result<String, CliError> {
        if source_kind != kind(self.source()?) {
            return Err(CliError::Input(
                "Source kind cannot change; ID and kind stay fixed. No files changed.".into(),
            ));
        }
        if location.trim().is_empty() || location.chars().any(char::is_control) {
            return Err(CliError::Input(
                "Source location must be nonempty and contain no control characters".into(),
            ));
        }
        let mut reference = None;
        let mut path = None;
        for pair in options.chunks(2) {
            let [flag, value] = pair else {
                return Err(CliError::Input("Source option requires a value".into()));
            };
            if source_kind != "github" || value.is_empty() || value.chars().any(char::is_control) {
                return Err(CliError::Input("invalid Source options".into()));
            }
            match flag.as_str() {
                "--ref" if reference.is_none() => reference = Some(value.as_str()),
                "--path" if path.is_none() => path = Some(value.as_str()),
                _ => return Err(CliError::Input("invalid or repeated Source option".into())),
            }
        }
        let mut doc = self.document()?;
        let table = doc["sources"][self.id.as_str()]
            .as_table_like_mut()
            .ok_or_else(|| CliError::Intent("invalid Source table".into()))?;
        // Keep existing value decor, including inline comments, when replacing fields.
        set(
            table,
            if source_kind == "local" {
                "path"
            } else {
                "repository"
            },
            location,
        );
        if source_kind == "github" {
            for (key, value) in [("ref", reference), ("path", path)] {
                match value {
                    Some(value) => set(table, key, value),
                    None => {
                        table.remove(key);
                    }
                }
            }
        }
        let revised = doc.to_string();
        let config = parse_config_toml(&revised)?;
        users(&self.paths.managed_root(), &config, &self.id)?;
        Ok(revised)
    }

    fn removal(&self) -> Result<String, CliError> {
        let users = self.users()?;
        if !users.is_empty() {
            return Err(CliError::Intent(format!(
                "Source {} is used by Profiles: {}; no files changed",
                self.id,
                names(&users)
            )));
        }
        let mut doc = self.document()?;
        doc["sources"]
            .as_table_like_mut()
            .ok_or_else(|| CliError::Intent("invalid Sources table".into()))?
            .remove(self.id.as_str());
        Ok(doc.to_string())
    }

    fn publish(&self, revised: &str, removing: bool) -> Result<(), CliError> {
        let root = self.paths.managed_root();
        let _lock = crate::recovery::exclusive_state_lock(&root.join("state.lock"))
            .map_err(|error| CliError::Intent(error.to_string()))?;
        let path = root.join("config.toml");
        if read_text(&path)? != self.original {
            return Err(CliError::Intent(
                "Source configuration changed while editing; no files changed; reopen and review"
                    .into(),
            ));
        }
        // Re-scan under the mutation lock: a new Profile may have appeared during consent.
        if removing {
            self.removal()?;
        }
        let config = parse_config_toml(revised)?;
        users(&root, &config, &self.id)?;
        atomic_intent_edit_if_unchanged(&path, revised.as_bytes(), self.original.as_bytes())
    }
}

fn set(table: &mut dyn toml_edit::TableLike, key: &str, value: &str) {
    let mut replacement = toml_edit::Value::from(value);
    if let Some(old) = table.get(key).and_then(toml_edit::Item::as_value) {
        *replacement.decor_mut() = old.decor().clone();
    }
    table.insert(key, toml_edit::Item::Value(replacement));
}

fn users(root: &Path, config: &ConfigIntent, source: &IntentId) -> Result<Vec<IntentId>, CliError> {
    let directory = profile_directory(root)?;
    let mut paths = Vec::new();
    for entry in fs::read_dir(&directory)? {
        let path = entry?.path();
        if path.extension().is_some_and(|ext| ext == "toml") {
            paths.push(path);
        }
    }
    paths.sort();
    let mut users = Vec::new();
    for path in paths {
        let inspected = (|| {
            if !fs::symlink_metadata(&path)?.is_file() {
                return Err(CliError::Intent("Profile is not a regular file".into()));
            }
            let name = path
                .file_stem()
                .and_then(|name| name.to_str())
                .ok_or_else(|| CliError::Intent("Profile filename is not UTF-8".into()))?;
            let (id, profile) = parse_named_profile_toml(name, config, &read_text(&path)?)?;
            Ok((id, profile))
        })();
        let (id, profile) = inspected.map_err(|error: CliError| CliError::Intent(format!("Cannot inspect Profile {}: {error}; dependency analysis incomplete; no files changed", path.display())))?;
        if matches!(profile.wallpaper, Some(WallpaperIntent::Source { source: ref id, .. }) if id == source)
        {
            users.push(id);
        }
    }
    Ok(users)
}

fn remove(
    snapshot: &Snapshot,
    input: &mut impl Iterator<Item = io::Result<String>>,
    output: &mut impl Write,
) -> Result<(), CliError> {
    let revised = snapshot.removal()?;
    write_text(output, &snapshot.summary()?)?;
    writeln!(
        output,
        "Remove only Source {} configuration? Images, Profiles, Environments and History stay.\ny Confirm; Enter/n/cancel/EOF Cancel (default):",
        snapshot.id
    )?;
    output.flush()?;
    if input.next().transpose()?.as_deref().map(str::trim) != Some("y") {
        return writeln!(output, "Source removal cancelled; no files changed.").map_err(Into::into);
    }
    snapshot.publish(&revised, true)?;
    writeln!(
        output,
        "Removed Source {}; images and History preserved.",
        snapshot.id
    )?;
    Ok(())
}

fn check(paths: &InitPaths, id: &IntentId, output: &mut impl Write) -> Result<(), CliError> {
    let config = parse_config_toml(&read_text(&paths.managed_root().join("config.toml"))?)?;
    match crate::plan::source_candidate_count(
        &paths.managed_root(),
        &paths.home,
        &config,
        id,
        &GithubHttpClient::from_env(),
    ) {
        Ok(count) => {
            writeln!(
                output,
                "Source {id}: available, {count} PNG/JPEG candidate(s). Listing only; images not decoded. Terminal unchanged."
            )?;
            Ok(())
        }
        Err(error) => {
            writeln!(
                output,
                "Source {id}: {}. Check location, permissions, repository/ref/subdirectory or GITHUB_TOKEN, then retry. No files changed.",
                if matches!(error, PlanError::EmptyCandidateSet(_)) {
                    "empty Candidate Set"
                } else {
                    "unavailable"
                }
            )?;
            Err(CliError::Plan(error))
        }
    }
}

pub(super) fn tui(
    action: char,
    selected: Option<IntentId>,
    output: &mut impl Write,
) -> Result<String, CliError> {
    let mut ids = vec![selected.map(|id| id.to_string()).unwrap_or_default()];
    let Some(snapshot) = forms::fields(
        output,
        "Manage Source · stable ID",
        &["Source ID:"],
        &mut ids,
        |values| Snapshot::load(values[0].parse()?),
    )?
    else {
        return Ok("Source maintenance cancelled; no files changed.".into());
    };
    if action == 'C' {
        let (_, report) = maintenance::job(output, "Check Source", true, move |out| {
            check(&snapshot.paths, &snapshot.id, out)
        })?;
        return Ok(report);
    }
    let summary = snapshot.summary()?;
    if action == 'S' {
        management::details(output, &summary)?;
        return Ok(summary);
    }
    if action == 'Z' {
        let revised = snapshot.removal()?;
        if !forms::confirm(
            output,
            "Remove Source",
            &format!(
                "{summary}\nRemove only this configuration entry? Images, Profiles, Environments and History stay."
            ),
        )? {
            return Ok("Source removal cancelled; no files changed.".into());
        }
        snapshot.publish(&revised, true)?;
        return Ok(format!(
            "Removed Source {}; images and History preserved.",
            snapshot.id
        ));
    }
    let (labels, mut values) = match snapshot.source()? {
        SourceIntent::LocalDirectory { path } => (vec!["Directory path:"], vec![path.clone()]),
        SourceIntent::Github {
            repository,
            reference,
            path,
        } => (
            vec![
                "Repository (owner/repo):",
                "Ref (blank for default):",
                "Subdirectory (blank for repository root):",
            ],
            vec![
                repository.clone(),
                reference.clone().unwrap_or_default(),
                path.as_ref()
                    .map(|path| path.as_str().to_owned())
                    .unwrap_or_default(),
            ],
        ),
    };
    loop {
        let Some(revised) = forms::fields(
            output,
            "Edit Source · ID and kind stay fixed",
            &labels,
            &mut values,
            |values| {
                let options = github_options(&values[1..]);
                snapshot.revised(kind(snapshot.source()?), &values[0], &options)
            },
        )?
        else {
            return Ok("Source edit cancelled; no files changed.".into());
        };
        let proposed = revised
            .parse::<toml_edit::DocumentMut>()
            .map_err(|error| CliError::Intent(error.to_string()))?["sources"][snapshot.id.as_str()]
        .to_string();
        if !forms::confirm(
            output,
            "Save Source changes",
            &format!(
                "{summary}\nProposed definition:\n{proposed}\nFuture resolution may choose a different wallpaper. Availability not checked; valid unavailable locations can be saved. Terminal unchanged."
            ),
        )? {
            continue;
        }
        snapshot.publish(&revised, false)?;
        return Ok(format!(
            "Updated Source {}; terminal unchanged. Use Check Source to test availability.",
            snapshot.id
        ));
    }
}

fn github_options(values: &[String]) -> Vec<String> {
    let mut options = Vec::new();
    for (flag, value) in ["--ref", "--path"].iter().zip(values) {
        if !value.is_empty() {
            options.extend([(*flag).into(), value.clone()]);
        }
    }
    options
}

pub(super) fn line_flow(
    action: &str,
    selected: Option<IntentId>,
    input: &mut impl Iterator<Item = io::Result<String>>,
    output: &mut impl Write,
) -> Result<(), CliError> {
    let name = prompt_tui(
        input,
        output,
        &format!(
            "Source ID (blank for {}): ",
            selected
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_default()
        ),
    )?;
    let Some(name) = name else {
        return Ok(());
    };
    let id = if name.is_empty() {
        selected.ok_or_else(|| CliError::Input("Select or name a Source".into()))?
    } else {
        name.parse()?
    };
    if action == "C" {
        return check(&process_paths()?, &id, output);
    }
    let snapshot = Snapshot::load(id)?;
    write_text(output, &snapshot.summary()?)?;
    if action == "S" {
        return Ok(());
    }
    if action == "Z" {
        return remove(&snapshot, input, output);
    }
    let Some(location) = prompt_tui(
        input,
        output,
        "New directory path or owner/repo (b cancels): ",
    )?
    else {
        return Ok(());
    };
    let mut options = Vec::new();
    if kind(snapshot.source()?) == "github" {
        for flag in ["--ref", "--path"] {
            let Some(value) = prompt_tui(input, output, &format!("{flag} (blank for default): "))?
            else {
                return Ok(());
            };
            if !value.is_empty() {
                options.extend([flag.into(), value]);
            }
        }
    }
    let revised = snapshot.revised(kind(snapshot.source()?), &location, &options)?;
    writeln!(
        output,
        "Proposed configuration:\n{revised}\nFuture resolution may choose a different wallpaper; availability not checked. Terminal unchanged."
    )?;
    if prompt_tui(input, output, "y Save Source; Enter/n Cancel (default): ")?.as_deref()
        == Some("y")
    {
        snapshot.publish(&revised, false)?;
        writeln!(output, "Updated Source {}.", snapshot.id)?;
    }
    Ok(())
}
