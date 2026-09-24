//! Read-only diagnosis, explicit legacy migration, and safe uninstall from RFC 0008.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs, io,
    path::{Path, PathBuf},
    str::FromStr,
};

use thiserror::Error;
use toml_edit::{DocumentMut, Item, Table, value};

use crate::{
    codec::intent::parse_config_toml,
    domain::{IntentId, SourceIntent, SourcePath},
    history::inspect_history,
    init::{
        InitError, InitPaths, atomic_edit, dry_run, hook_count, init, init_with_config,
        legacy_hook_count, preflight_layout, validate_existing,
    },
    recovery::{ProjectionState, exclusive_state_lock, inspect_recovery_state},
};

const DEFAULT_CONFIG: &str = "schema_version = 1\n\n[sources]\n";
const MAX_TEXT_BYTES: u64 = 1024 * 1024;

/// Outcome class for one Doctor check.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckStatus {
    /// Check completed and contract holds.
    Verified,
    /// Check completed and found invalid or unsafe state.
    Failed,
    /// Check cannot be established without unavailable or mutating facilities.
    Unavailable,
}

/// One named Doctor observation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DoctorCheck {
    /// Stable check name.
    pub name: &'static str,
    /// Check outcome class.
    pub status: CheckStatus,
    /// Human-readable evidence or remediation context.
    pub detail: String,
}

/// Read-only installation health report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DoctorReport {
    /// Resolved or logical Managed Root.
    pub managed_root: PathBuf,
    /// Independent health checks.
    pub checks: Vec<DoctorCheck>,
}

impl DoctorReport {
    /// Returns status for named check.
    pub fn status(&self, name: &str) -> Option<CheckStatus> {
        self.checks
            .iter()
            .find(|check| check.name == name)
            .map(|check| check.status)
    }
}

/// Completed legacy migration observations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MigrationReport {
    /// Legacy Sources newly added to v1 Intent.
    pub imported_sources: usize,
    /// Whether `config.toml` changed.
    pub config_changed: bool,
    /// Strictly recognized v0 hooks removed.
    pub removed_legacy_hooks: usize,
    /// Whether invocation was mutation-free preview.
    pub dry_run: bool,
}

/// Completed safe uninstall observations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UninstallReport {
    /// Owned v1 Integration Hooks removed.
    pub removed_hooks: usize,
    /// Whether managed Projection existed and was removed.
    pub removed_projection: bool,
    /// Whether disposable cache existed and was removed.
    pub removed_cache: bool,
}

/// Lifecycle operation failure. No authoritative or durable state is repaired or removed.
#[derive(Debug, Error)]
pub enum LifecycleError {
    /// Initialization or atomic publication failed.
    #[error(transparent)]
    Init(#[from] InitError),
    /// Filesystem access failed.
    #[error("filesystem error at {path}: {source}")]
    Io {
        /// Path being accessed.
        path: PathBuf,
        /// Underlying filesystem error.
        #[source]
        source: io::Error,
    },
    /// Legacy Source entry cannot map exactly to v1 Intent.
    #[error("invalid legacy entry at {path}:{line}: {reason}")]
    InvalidLegacy {
        /// Legacy Source file.
        path: PathBuf,
        /// One-based source line.
        line: usize,
        /// Validation reason.
        reason: String,
    },
    /// Imported Source conflicts with another imported or current Source.
    #[error("legacy Source collision for {0}")]
    SourceCollision(String),
    /// State changed after preflight or had unsafe ownership, kind, or permissions.
    #[error("unsafe lifecycle state at {0}")]
    Unsafe(PathBuf),
    /// Durable or Projection inspection failed.
    #[error("lifecycle inspection failed: {0}")]
    Inspection(String),
}

#[derive(Clone)]
struct ImportedSource {
    id: String,
    source: SourceIntent,
}

struct ConfigEdit {
    logical: PathBuf,
    target: PathBuf,
    revised: Vec<u8>,
    removed: usize,
}

#[cfg(unix)]
#[derive(Clone, Copy, PartialEq, Eq)]
enum CacheKind {
    Directory,
    File,
    Symlink,
}

#[cfg(unix)]
struct CacheEntry {
    path: PathBuf,
    dev: u64,
    ino: u64,
    kind: CacheKind,
}

/// Inspects installation health without changing files.
pub fn doctor(paths: &InitPaths) -> DoctorReport {
    let mut checks = Vec::new();
    let report = match dry_run(paths) {
        Ok(report) => report,
        Err(error) => {
            checks.push(check(
                "managed-layout",
                CheckStatus::Failed,
                error.to_string(),
            ));
            for name in [
                "intent",
                "durable-history",
                "projection",
                "integration-hook",
            ] {
                checks.push(check(name, CheckStatus::Unavailable, "layout unavailable"));
            }
            checks.push(check(
                "filesystem-capabilities",
                CheckStatus::Unavailable,
                "requires mutating runtime probes",
            ));
            return DoctorReport {
                managed_root: paths.managed_root(),
                checks,
            };
        }
    };

    let required_layout_valid = report.layout.iter().all(|entry| {
        entry.status == "valid"
            || (entry.status == "missing"
                && (entry.path.ends_with("cache") || entry.path.ends_with("current.ghostty")))
    });
    checks.push(check(
        "managed-layout",
        if required_layout_valid {
            CheckStatus::Verified
        } else {
            CheckStatus::Failed
        },
        if required_layout_valid {
            "required managed structure is safe"
        } else {
            "required managed structure is missing or unsafe"
        },
    ));

    let intent_path = report.managed_root.join("config.toml");
    let intent_status = read_text(&intent_path)
        .and_then(|text| {
            parse_config_toml(&text)
                .map(|_| ())
                .map_err(|error| LifecycleError::Inspection(error.to_string()))
        })
        .map_or(CheckStatus::Failed, |()| CheckStatus::Verified);
    checks.push(check(
        "intent",
        intent_status,
        if intent_status == CheckStatus::Verified {
            "config.toml is valid"
        } else {
            "config.toml is missing, unreadable, or invalid"
        },
    ));

    let legacy_hook = legacy_hook_present(paths, &report.root_config).unwrap_or(true);
    let integration_ok = report.hook_state == "present"
        && !report.problems.iter().any(|problem| {
            problem.contains("Integration Hook") || problem.contains("stale Integration Hook")
        })
        && !legacy_hook;
    checks.push(check(
        "integration-hook",
        if integration_ok {
            CheckStatus::Verified
        } else {
            CheckStatus::Failed
        },
        if integration_ok {
            "exactly one effective managed hook is installed"
        } else {
            "managed hook drift or incomplete legacy migration"
        },
    ));

    let history_ok = match inspect_history(&report.managed_root) {
        Ok(_) => {
            checks.push(check(
                "durable-history",
                CheckStatus::Verified,
                "committed History and dependencies are valid",
            ));
            true
        }
        Err(error) => {
            checks.push(check(
                "durable-history",
                CheckStatus::Failed,
                error.to_string(),
            ));
            false
        }
    };

    if history_ok && integration_ok {
        match inspect_recovery_state(&report.managed_root, &report.root_config) {
            Ok(inspection) => {
                let status = if inspection.projection() == ProjectionState::Consistent {
                    CheckStatus::Verified
                } else {
                    CheckStatus::Failed
                };
                checks.push(check(
                    "projection",
                    status,
                    inspection
                        .projection()
                        .diagnostic_code()
                        .unwrap_or("Projection matches committed History"),
                ));
            }
            Err(error) => checks.push(check("projection", CheckStatus::Failed, error.to_string())),
        }
    } else {
        checks.push(check(
            "projection",
            CheckStatus::Unavailable,
            "durable state or integration is not valid",
        ));
    }

    checks.push(check(
        "filesystem-capabilities",
        CheckStatus::Unavailable,
        "read-only Doctor does not run mutating filesystem probes",
    ));
    DoctorReport {
        managed_root: report.managed_root,
        checks,
    }
}

/// Preflights and imports recognized v0 Sources, installs v1 integration, then removes v0 hooks.
pub fn migrate_legacy(
    paths: &InitPaths,
    dry_run_only: bool,
) -> Result<MigrationReport, LifecycleError> {
    let legacy_path = paths.xdg_config_home().join("ghostty/wallpaper_repos.txt");
    let imported = if fs::symlink_metadata(&legacy_path).is_ok() {
        parse_legacy_sources(&legacy_path)?
    } else {
        Vec::new()
    };
    let managed_root = paths.managed_root();
    let config_path = managed_root.join("config.toml");
    let config_exists = match fs::symlink_metadata(&config_path) {
        Ok(metadata) => {
            validate_existing(&config_path, &metadata, false)?;
            true
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => false,
        Err(source) => return Err(io_error(&config_path, source)),
    };
    let current_text = if config_exists {
        read_text(&config_path)?
    } else {
        DEFAULT_CONFIG.to_owned()
    };
    let (revised, imported_sources) = merge_sources(&config_path, &current_text, &imported)?;
    let config_changed = revised.as_bytes() != current_text.as_bytes();

    // Validate every possible root-config edit before init or Intent mutation.
    let legacy_projection = paths.xdg_config_home().join("ghostty/wallpaper.conf");
    let preflight_edits = plan_hook_edits(paths, &legacy_projection, HookKind::Legacy)?;
    let removable = preflight_edits.iter().map(|edit| edit.removed).sum();
    if dry_run_only {
        return Ok(MigrationReport {
            imported_sources,
            config_changed,
            removed_legacy_hooks: removable,
            dry_run: true,
        });
    }

    if config_exists {
        preflight_layout(&managed_root)?;
        if config_changed {
            let current = read_text(&config_path)?;
            if current != current_text {
                return Err(LifecycleError::Unsafe(config_path));
            }
            let target =
                fs::canonicalize(&config_path).map_err(|source| io_error(&config_path, source))?;
            atomic_edit(&config_path, &target, revised.as_bytes())?;
        }
        init(paths)?;
    } else {
        if fs::symlink_metadata(&managed_root).is_ok() {
            return Err(LifecycleError::Init(InitError::RepairRequired(config_path)));
        }
        init_with_config(paths, revised.as_bytes())?;
    }

    // Init adds v1 hook to same target, so derive legacy-only edits again.
    let edits = plan_hook_edits(paths, &legacy_projection, HookKind::Legacy)?;
    let removed_legacy_hooks = commit_hook_edits(edits)?;
    Ok(MigrationReport {
        imported_sources,
        config_changed,
        removed_legacy_hooks,
        dry_run: false,
    })
}

/// Removes owned Integration Hooks and disposable Projection/cache while preserving all Intent and durable state.
pub fn uninstall(paths: &InitPaths) -> Result<UninstallReport, LifecycleError> {
    let logical_root = paths.managed_root();
    preflight_layout(&logical_root)?;
    let root = fs::canonicalize(&logical_root).map_err(|source| io_error(&logical_root, source))?;
    let projection = root.join("current.ghostty");
    let first_edits = plan_hook_edits(paths, &projection, HookKind::Managed)?;
    let removed_hooks = first_edits.iter().map(|edit| edit.removed).sum();
    let removed_projection = fs::symlink_metadata(&projection).is_ok();
    let cache = root.join("cache");
    let removed_cache = fs::symlink_metadata(&cache).is_ok();
    let _cache_plan = plan_cache_removal(&cache)?;

    let _lock = exclusive_state_lock(&root.join("state.lock"))
        .map_err(|error| LifecycleError::Inspection(error.to_string()))?;
    preflight_layout(&root)?;
    let edits = plan_hook_edits(paths, &projection, HookKind::Managed)?;
    if edits.iter().map(|edit| edit.removed).sum::<usize>() != removed_hooks {
        return Err(LifecycleError::Unsafe(projection));
    }
    let cache_plan = plan_cache_removal(&cache)?;
    if fs::symlink_metadata(&projection).is_ok() != removed_projection {
        return Err(LifecycleError::Unsafe(projection));
    }
    commit_hook_edits(edits)?;

    remove_projection_entry(&projection)?;
    remove_cache(cache_plan)?;
    fs::File::open(&root)
        .and_then(|directory| directory.sync_all())
        .map_err(|source| io_error(&root, source))?;
    Ok(UninstallReport {
        removed_hooks,
        removed_projection,
        removed_cache,
    })
}

fn check(name: &'static str, status: CheckStatus, detail: impl Into<String>) -> DoctorCheck {
    DoctorCheck {
        name,
        status,
        detail: detail.into(),
    }
}

fn io_error(path: &Path, source: io::Error) -> LifecycleError {
    LifecycleError::Io {
        path: path.to_owned(),
        source,
    }
}

fn read_text(path: &Path) -> Result<String, LifecycleError> {
    let metadata = fs::symlink_metadata(path).map_err(|source| io_error(path, source))?;
    validate_existing(path, &metadata, false)?;
    if metadata.len() > MAX_TEXT_BYTES {
        return Err(LifecycleError::Unsafe(path.to_owned()));
    }
    fs::read_to_string(path).map_err(|source| io_error(path, source))
}

fn parse_legacy_sources(path: &Path) -> Result<Vec<ImportedSource>, LifecycleError> {
    let text = read_text(path)?;
    let mut sources = Vec::new();
    let mut ids = BTreeSet::new();
    for (index, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<_> = line.split('|').map(str::trim).collect();
        if fields.len() != 4 {
            return Err(invalid_legacy(
                path,
                index,
                "expected name|owner/repo|branch|path",
            ));
        }
        let id = IntentId::from_str(fields[0])
            .map_err(|error| invalid_legacy(path, index, error.to_string()))?;
        if !ids.insert(id.to_string()) {
            return Err(LifecycleError::SourceCollision(id.to_string()));
        }
        let reference = if fields[2].is_empty() {
            "main"
        } else {
            fields[2]
        };
        if reference.chars().any(char::is_control) {
            return Err(invalid_legacy(
                path,
                index,
                "ref contains control characters",
            ));
        }
        let normalized_path = fields[3].trim_matches('/');
        let path_value = if normalized_path.is_empty() {
            None
        } else {
            Some(
                SourcePath::from_str(normalized_path)
                    .map_err(|error| invalid_legacy(path, index, error.to_string()))?,
            )
        };
        let source = SourceIntent::Github {
            repository: fields[1].to_owned(),
            reference: Some(reference.to_owned()),
            path: path_value,
        };
        validate_imported_source(path, index, &id, &source)?;
        sources.push(ImportedSource {
            id: id.to_string(),
            source,
        });
    }
    Ok(sources)
}

fn validate_imported_source(
    path: &Path,
    line: usize,
    id: &IntentId,
    source: &SourceIntent,
) -> Result<(), LifecycleError> {
    let text = render_single_source(id.as_str(), source);
    parse_config_toml(&text)
        .map(|_| ())
        .map_err(|error| invalid_legacy(path, line, error.to_string()))
}

fn invalid_legacy(
    path: &Path,
    zero_based_line: usize,
    reason: impl Into<String>,
) -> LifecycleError {
    LifecycleError::InvalidLegacy {
        path: path.to_owned(),
        line: zero_based_line + 1,
        reason: reason.into(),
    }
}

fn merge_sources(
    config_path: &Path,
    current: &str,
    imported: &[ImportedSource],
) -> Result<(String, usize), LifecycleError> {
    let parsed = parse_config_toml(current).map_err(|error| {
        LifecycleError::Inspection(format!("{}: {error}", config_path.display()))
    })?;
    let existing: BTreeMap<_, _> = parsed
        .sources
        .into_iter()
        .map(|(id, source)| (id.to_string(), source))
        .collect();
    let mut additions = Vec::new();
    for candidate in imported {
        match existing.get(&candidate.id) {
            Some(source) if source == &candidate.source => (),
            Some(_) => return Err(LifecycleError::SourceCollision(candidate.id.clone())),
            None => additions.push(candidate),
        }
    }
    if additions.is_empty() {
        return Ok((current.to_owned(), 0));
    }

    let mut document = current
        .parse::<DocumentMut>()
        .map_err(|error| LifecycleError::Inspection(error.to_string()))?;
    for candidate in &additions {
        document["sources"][&candidate.id] = source_item(&candidate.source);
    }
    let revised = document.to_string();
    parse_config_toml(&revised).map_err(|error| LifecycleError::Inspection(error.to_string()))?;
    Ok((revised, additions.len()))
}

fn source_item(source: &SourceIntent) -> Item {
    let mut table = Table::new();
    match source {
        SourceIntent::Github {
            repository,
            reference,
            path,
        } => {
            table["kind"] = value("github");
            table["repository"] = value(repository);
            if let Some(reference) = reference {
                table["ref"] = value(reference);
            }
            if let Some(path) = path {
                table["path"] = value(path.as_str());
            }
        }
        SourceIntent::LocalDirectory { path } => {
            table["kind"] = value("local-directory");
            table["path"] = value(path);
        }
    }
    Item::Table(table)
}

fn render_single_source(id: &str, source: &SourceIntent) -> String {
    let mut document = DEFAULT_CONFIG
        .parse::<DocumentMut>()
        .expect("default Intent is valid");
    document["sources"][id] = source_item(source);
    document.to_string()
}

#[derive(Clone, Copy)]
enum HookKind {
    Managed,
    Legacy,
}

fn plan_hook_edits(
    paths: &InitPaths,
    target_path: &Path,
    kind: HookKind,
) -> Result<Vec<ConfigEdit>, LifecycleError> {
    let mut edits = Vec::new();
    let mut targets = BTreeSet::new();
    for logical in paths.ghostty_candidates() {
        match fs::symlink_metadata(&logical) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(source) => return Err(io_error(&logical, source)),
            Ok(_) => (),
        }
        let target = fs::canonicalize(&logical).map_err(|source| io_error(&logical, source))?;
        if !targets.insert(target.clone()) {
            continue;
        }
        let metadata = fs::metadata(&target).map_err(|source| io_error(&target, source))?;
        validate_existing(&target, &metadata, false)?;
        let original = read_text(&target)?;
        let mut removed = 0;
        let kept: Vec<_> = original
            .lines()
            .filter(|line| {
                let owned = match kind {
                    HookKind::Managed => {
                        hook_count(&format!("{line}\n"), &target, target_path) != 0
                    }
                    HookKind::Legacy => {
                        legacy_hook_count(&format!("{line}\n"), &target, target_path) != 0
                    }
                };
                if owned {
                    removed += 1;
                }
                !owned
            })
            .collect();
        let mut revised = kept.join("\n");
        if !revised.is_empty() {
            revised.push('\n');
        }
        edits.push(ConfigEdit {
            logical,
            target,
            revised: revised.into_bytes(),
            removed,
        });
    }
    Ok(edits)
}

fn commit_hook_edits(edits: Vec<ConfigEdit>) -> Result<usize, LifecycleError> {
    let mut removed = 0;
    for edit in edits {
        if edit.removed != 0 {
            atomic_edit(&edit.logical, &edit.target, &edit.revised)?;
            removed += edit.removed;
        }
    }
    Ok(removed)
}

fn legacy_hook_present(paths: &InitPaths, config: &Path) -> Result<bool, LifecycleError> {
    let target = fs::canonicalize(config).map_err(|source| io_error(config, source))?;
    let text = read_text(&target)?;
    let legacy = paths.xdg_config_home().join("ghostty/wallpaper.conf");
    Ok(legacy_hook_count(&text, &target, &legacy) != 0)
}

fn remove_projection_entry(path: &Path) -> Result<(), LifecycleError> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(io_error(path, source)),
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
            fs::remove_file(path).map_err(|source| io_error(path, source))
        }
        Ok(_) => Err(LifecycleError::Unsafe(path.to_owned())),
    }
}

#[cfg(unix)]
fn plan_cache_removal(path: &Path) -> Result<Vec<CacheEntry>, LifecycleError> {
    fn visit(path: &Path, root: bool, entries: &mut Vec<CacheEntry>) -> Result<(), LifecycleError> {
        use std::os::unix::fs::MetadataExt;

        let metadata = match fs::symlink_metadata(path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound && root => return Ok(()),
            Err(source) => return Err(io_error(path, source)),
            Ok(metadata) => metadata,
        };
        if metadata.uid() != unsafe { libc::geteuid() } {
            return Err(LifecycleError::Unsafe(path.to_owned()));
        }
        let kind = if metadata.file_type().is_symlink() {
            if root {
                return Err(LifecycleError::Unsafe(path.to_owned()));
            }
            CacheKind::Symlink
        } else if metadata.is_dir() {
            if metadata.mode() & 0o022 != 0 {
                return Err(LifecycleError::Unsafe(path.to_owned()));
            }
            for entry in fs::read_dir(path).map_err(|source| io_error(path, source))? {
                visit(
                    &entry.map_err(|source| io_error(path, source))?.path(),
                    false,
                    entries,
                )?;
            }
            CacheKind::Directory
        } else if metadata.is_file() {
            if metadata.mode() & 0o022 != 0 {
                return Err(LifecycleError::Unsafe(path.to_owned()));
            }
            CacheKind::File
        } else {
            return Err(LifecycleError::Unsafe(path.to_owned()));
        };
        entries.push(CacheEntry {
            path: path.to_owned(),
            dev: metadata.dev(),
            ino: metadata.ino(),
            kind,
        });
        Ok(())
    }

    let mut entries = Vec::new();
    visit(path, true, &mut entries)?;
    Ok(entries)
}

#[cfg(not(unix))]
fn plan_cache_removal(path: &Path) -> Result<Vec<()>, LifecycleError> {
    if fs::symlink_metadata(path).is_ok() {
        Err(LifecycleError::Unsafe(path.to_owned()))
    } else {
        Ok(Vec::new())
    }
}

#[cfg(unix)]
fn remove_cache(entries: Vec<CacheEntry>) -> Result<(), LifecycleError> {
    use std::os::unix::fs::MetadataExt;

    for entry in entries {
        let metadata =
            fs::symlink_metadata(&entry.path).map_err(|source| io_error(&entry.path, source))?;
        let kind = if metadata.file_type().is_symlink() {
            CacheKind::Symlink
        } else if metadata.is_dir() {
            CacheKind::Directory
        } else if metadata.is_file() {
            CacheKind::File
        } else {
            return Err(LifecycleError::Unsafe(entry.path));
        };
        if metadata.dev() != entry.dev || metadata.ino() != entry.ino || kind != entry.kind {
            return Err(LifecycleError::Unsafe(entry.path));
        }
        match kind {
            CacheKind::Directory => fs::remove_dir(&entry.path),
            CacheKind::File | CacheKind::Symlink => fs::remove_file(&entry.path),
        }
        .map_err(|source| io_error(&entry.path, source))?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn remove_cache(_entries: Vec<()>) -> Result<(), LifecycleError> {
    Ok(())
}
