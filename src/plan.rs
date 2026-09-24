//! Read-only Profile planning and Source resolution.

use std::{
    fs,
    io::{Cursor, Read},
    path::{Path, PathBuf},
    str::FromStr,
};

use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{
    codec::{candidate_set::fingerprint, manifest},
    domain::{
        CandidatePath, CandidateSet, ColorsIntent, ColorsManifest, ConfigIntent,
        EnvironmentManifest, ImageWallpaper, IntentId, MediaType, ProfileIntent, Sha256Digest,
        SourceIntent, SourcePath, TerminalManifest, WallpaperIntent, WallpaperSelection,
    },
    github::{GithubApi, GithubApiError, GithubEntryKind},
    recovery::{RecoveryError, inspect_recovery_state},
    selection::{RandomSelectionError, select_random_v1},
    theme::{ThemeError, ThemeResolver},
};

/// Platform observations required to produce complete Plan Operations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanPlatform {
    effective_root_config: PathBuf,
    reload: ReloadObservation,
}

impl PlanPlatform {
    /// Constructs explicit machine-local planning observations.
    pub fn new(effective_root_config: PathBuf, reload: ReloadObservation) -> Self {
        Self {
            effective_root_config,
            reload,
        }
    }
}

/// Reload capability observed during planning.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReloadObservation {
    /// Linux systemd adapter command is available.
    Systemd,
    /// Experimental macOS AppleScript adapter command is available.
    Applescript,
    /// Reload cannot be attempted on this machine.
    Unavailable(ReloadUnavailableReason),
}

/// Registered reason reload is unavailable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReloadUnavailableReason {
    /// Current platform has no supported adapter.
    UnsupportedPlatform,
    /// Supported adapter command is missing.
    AdapterCommandUnavailable,
    /// Ghostty runtime integration cannot be used.
    GhosttyIntegrationUnavailable,
}

/// Safe classification of a GitHub Source resolution failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GithubResolutionError {
    /// Authentication was absent, invalid, or insufficient.
    Authentication,
    /// GitHub rate limit was exhausted.
    RateLimited,
    /// GitHub could not return a usable response.
    Unavailable,
    /// Recursive tree response omitted Candidate membership.
    IncompleteTree,
}

/// Planning failure.
#[derive(Debug, Error)]
pub enum PlanError {
    /// Referenced Source missing.
    #[error("unknown source {0}")]
    UnknownSource(String),
    /// Unsupported intent for this narrow planner.
    #[error("unsupported plan input: {0}")]
    Unsupported(&'static str),
    /// Random selection needed seed.
    #[error("random selection requires seed")]
    MissingSeed,
    /// Seed supplied without random selection.
    #[error("seed is forbidden without random selection")]
    UnexpectedSeed,
    /// Successfully enumerated Candidate Set had no eligible images.
    #[error("Source {0} has an empty Candidate Set")]
    EmptyCandidateSet(String),
    /// Candidate set drifted during local planning.
    #[error("local Source changed during planning")]
    SourceDrift,
    /// GitHub Source could not be resolved completely.
    #[error("GitHub Source {source_id} resolution failed: {kind:?}")]
    Github {
        /// Source identifier safe for public diagnostics.
        source_id: String,
        /// Credential-free failure kind.
        kind: GithubResolutionError,
    },
    /// Filesystem read failed.
    #[error("filesystem error at {path}: {source}")]
    Io {
        /// Path read during planning.
        path: PathBuf,
        /// Original error.
        source: std::io::Error,
    },
    /// Selected asset was not PNG or JPEG.
    #[error("selected asset is not PNG or JPEG")]
    UnsupportedImage,
    /// Selection algorithm failed.
    #[error(transparent)]
    Selection(#[from] RandomSelectionError),
    /// Existing durable record disagrees with its claimed identity.
    #[error("corrupt durable state at {0}")]
    Corrupt(PathBuf),
    /// Manifest codec failed.
    #[error(transparent)]
    Manifest(#[from] manifest::ManifestCodecError),
    /// Named theme could not be resolved into the managed color model.
    #[error(transparent)]
    Theme(#[from] ThemeError),
    /// Recovery Inspection or effective integration validation failed.
    #[error(transparent)]
    Recovery(#[from] RecoveryError),
}

impl PlanError {
    /// Returns strict RFC 0005 Error Response for this planning failure.
    pub fn error_response(&self) -> Value {
        let error = match self {
            Self::UnknownSource(source_id) => json!({
                "category": "intent", "code": "intent.unknown-source", "source_id": source_id
            }),
            Self::Unsupported(_) | Self::Theme(_) => json!({
                "category": "resolution", "code": "resolution.unsupported-input"
            }),
            Self::MissingSeed => json!({
                "category": "usage", "code": "usage.resolution-seed-required"
            }),
            Self::UnexpectedSeed => json!({
                "category": "usage", "code": "usage.resolution-seed-forbidden"
            }),
            Self::EmptyCandidateSet(source_id) => json!({
                "category": "resolution",
                "code": "source.empty-candidate-set",
                "source_id": source_id
            }),
            Self::SourceDrift => json!({
                "category": "resolution", "code": "source.changed-during-planning"
            }),
            Self::Github { source_id, kind } => json!({
                "category": "resolution",
                "code": match kind {
                    GithubResolutionError::Authentication => "source.github-authentication-failed",
                    GithubResolutionError::RateLimited => "source.github-rate-limited",
                    GithubResolutionError::Unavailable => "source.github-unavailable",
                    GithubResolutionError::IncompleteTree => "source.github-incomplete-tree",
                },
                "source_id": source_id,
            }),
            Self::Io { path, .. } => json!({
                "category": "resolution",
                "code": "resolution.filesystem-unavailable",
                "path": path.display().to_string()
            }),
            Self::UnsupportedImage => json!({
                "category": "resolution", "code": "asset.unsupported-image"
            }),
            Self::Corrupt(path) => json!({
                "category": "corruption",
                "code": "durable-state.corrupt",
                "path": path.display().to_string()
            }),
            Self::Recovery(error) => recovery_error(error),
            Self::Selection(_) | Self::Manifest(_) => json!({
                "category": "internal", "code": "internal.invariant"
            }),
        };
        json!({ "schema_version": 1, "error": error })
    }

    /// Process exit status assigned by RFC 0005 category.
    pub fn exit_status(&self) -> i32 {
        match self.error_response()["error"]["category"].as_str() {
            Some("usage") => 2,
            Some("intent") => 3,
            Some("resolution") => 4,
            Some("corruption") => 5,
            Some("apply") => 6,
            _ => 70,
        }
    }
}

fn recovery_error(error: &RecoveryError) -> Value {
    match error {
        RecoveryError::History(crate::history::HistoryError::Corrupt(path)) => json!({
            "category": "corruption",
            "code": "durable-state.corrupt",
            "path": path.display().to_string()
        }),
        RecoveryError::History(crate::history::HistoryError::Io { path, .. }) => json!({
            "category": "corruption",
            "code": "durable-state.unreadable",
            "path": path.display().to_string()
        }),
        RecoveryError::Io { path, source }
            if source.kind() == std::io::ErrorKind::NotFound
                && path.file_name().is_some_and(|name| name == "state.lock") =>
        {
            json!({
                "category": "intent",
                "code": "integration.not-initialized",
                "path": path.display().to_string()
            })
        }
        RecoveryError::Io { path, .. } => json!({
            "category": "resolution",
            "code": "integration.inspection-failed",
            "path": path.display().to_string()
        }),
        RecoveryError::IntegrationDrift(path) => json!({
            "category": "resolution",
            "code": "integration.hook-drift",
            "path": path.display().to_string()
        }),
        RecoveryError::UnsupportedPlatform => json!({
            "category": "resolution", "code": "integration.unsupported-platform"
        }),
    }
}

/// Resolves local Profile JSON without Recovery or platform observations.
///
/// This narrow seam exists for Source-resolution tests. Use
/// [`plan_local_profile_json`] for complete public Plan output.
pub fn plan_local_profile_json_uninspected(
    config_dir: &Path,
    home: &Path,
    managed_root: &Path,
    profile_id: &IntentId,
    config: &ConfigIntent,
    profile: &ProfileIntent,
    seed: Option<&crate::domain::ResolutionSeed>,
) -> Result<Value, PlanError> {
    plan_profile_json_inner(
        config_dir,
        home,
        managed_root,
        profile_id,
        config,
        profile,
        seed,
        None,
        None,
        None,
    )
}

/// Resolves local Profile JSON through a named-theme adapter, without Recovery inspection.
#[allow(clippy::too_many_arguments)]
pub fn plan_local_profile_with_theme_json_uninspected(
    config_dir: &Path,
    home: &Path,
    managed_root: &Path,
    profile_id: &IntentId,
    config: &ConfigIntent,
    profile: &ProfileIntent,
    seed: Option<&crate::domain::ResolutionSeed>,
    themes: &dyn ThemeResolver,
) -> Result<Value, PlanError> {
    plan_profile_json_inner(
        config_dir,
        home,
        managed_root,
        profile_id,
        config,
        profile,
        seed,
        None,
        None,
        Some(themes),
    )
}

/// Resolves Profile JSON with a deterministic GitHub adapter, without Recovery inspection.
#[allow(clippy::too_many_arguments)]
pub fn plan_github_profile_json_uninspected(
    config_dir: &Path,
    home: &Path,
    managed_root: &Path,
    profile_id: &IntentId,
    config: &ConfigIntent,
    profile: &ProfileIntent,
    seed: Option<&crate::domain::ResolutionSeed>,
    github: &dyn GithubApi,
) -> Result<Value, PlanError> {
    plan_profile_json_inner(
        config_dir,
        home,
        managed_root,
        profile_id,
        config,
        profile,
        seed,
        None,
        Some(github),
        None,
    )
}

/// Produces complete RFC 0005 Plan JSON using synchronized platform observations.
#[allow(clippy::too_many_arguments)]
pub fn plan_local_profile_json(
    config_dir: &Path,
    home: &Path,
    managed_root: &Path,
    profile_id: &IntentId,
    config: &ConfigIntent,
    profile: &ProfileIntent,
    seed: Option<&crate::domain::ResolutionSeed>,
    platform: &PlanPlatform,
) -> Result<Value, PlanError> {
    plan_profile_json_inner(
        config_dir,
        home,
        managed_root,
        profile_id,
        config,
        profile,
        seed,
        Some(platform),
        None,
        None,
    )
}

/// Produces complete RFC 0005 Plan JSON through a named-theme adapter.
#[allow(clippy::too_many_arguments)]
pub fn plan_local_profile_with_theme_json(
    config_dir: &Path,
    home: &Path,
    managed_root: &Path,
    profile_id: &IntentId,
    config: &ConfigIntent,
    profile: &ProfileIntent,
    seed: Option<&crate::domain::ResolutionSeed>,
    platform: &PlanPlatform,
    themes: &dyn ThemeResolver,
) -> Result<Value, PlanError> {
    plan_profile_json_inner(
        config_dir,
        home,
        managed_root,
        profile_id,
        config,
        profile,
        seed,
        Some(platform),
        None,
        Some(themes),
    )
}

/// Produces complete RFC 0005 Plan JSON with GitHub Source support.
#[allow(clippy::too_many_arguments)]
pub fn plan_github_profile_json(
    config_dir: &Path,
    home: &Path,
    managed_root: &Path,
    profile_id: &IntentId,
    config: &ConfigIntent,
    profile: &ProfileIntent,
    seed: Option<&crate::domain::ResolutionSeed>,
    platform: &PlanPlatform,
    github: &dyn GithubApi,
) -> Result<Value, PlanError> {
    plan_profile_json_inner(
        config_dir,
        home,
        managed_root,
        profile_id,
        config,
        profile,
        seed,
        Some(platform),
        Some(github),
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn plan_profile_json_inner(
    config_dir: &Path,
    home: &Path,
    managed_root: &Path,
    profile_id: &IntentId,
    config: &ConfigIntent,
    profile: &ProfileIntent,
    seed: Option<&crate::domain::ResolutionSeed>,
    platform: Option<&PlanPlatform>,
    github: Option<&dyn GithubApi>,
    themes: Option<&dyn ThemeResolver>,
) -> Result<Value, PlanError> {
    if seed.is_some()
        && !matches!(
            profile.wallpaper,
            Some(WallpaperIntent::Source {
                selection: WallpaperSelection::Random,
                ..
            })
        )
    {
        return Err(PlanError::UnexpectedSeed);
    }
    let mut diagnostics = Vec::new();
    if let Some(platform) = platform {
        let recovery = inspect_recovery_state(managed_root, &platform.effective_root_config)?;
        if let Some(code) = recovery.projection().diagnostic_code() {
            diagnostics.push(json!({ "code": code, "severity": "warning" }));
        }
    }
    let mut selected_asset_bytes = None;
    let (wallpaper, source_json, selection_json, asset_json) = match &profile.wallpaper {
        Some(WallpaperIntent::None) => (
            Some(crate::domain::WallpaperManifest::None),
            None,
            None,
            None,
        ),
        Some(WallpaperIntent::Source {
            source,
            selection,
            fit,
            position,
            opacity,
            repeat,
        }) => {
            let source_intent = config
                .sources
                .iter()
                .find(|(id, _)| id == source)
                .map(|(_, source)| source)
                .ok_or_else(|| PlanError::UnknownSource(source.to_string()))?;
            let resolved = match source_intent {
                SourceIntent::LocalDirectory { path } => resolve_local_source(
                    config_dir,
                    home,
                    source,
                    path,
                    selection,
                    seed,
                    &mut diagnostics,
                )?,
                SourceIntent::Github {
                    repository,
                    reference,
                    path,
                } => resolve_github_source(
                    source,
                    repository,
                    reference.as_deref(),
                    path.as_ref(),
                    selection,
                    seed,
                    github.ok_or(PlanError::Unsupported("GitHub adapter unavailable"))?,
                )?,
            };
            let media_type = detect_media_type(&resolved.bytes)?;
            let asset_sha256 = sha256(&resolved.bytes);
            let mut image = ImageWallpaper::new(asset_sha256, media_type);
            if let Some(value) = fit {
                image = image.with_fit(*value);
            }
            if let Some(value) = position {
                image = image.with_position(*value);
            }
            if let Some(value) = opacity {
                image = image.with_opacity(*value);
            }
            if let Some(value) = repeat {
                image = image.with_repeat(*value);
            }
            let byte_length = resolved.bytes.len();
            selected_asset_bytes = Some(resolved.bytes);
            (
                Some(crate::domain::WallpaperManifest::Image(image)),
                Some(resolved.source),
                Some(resolved.selection),
                Some(json!({
                    "sha256": asset_sha256.to_string(),
                    "media_type": media_type.as_str(),
                    "byte_length": byte_length,
                })),
            )
        }
        None => (None, None, None, None),
    };

    let (colors, theme_name) = match &profile.colors {
        Some(ColorsIntent::Explicit {
            background,
            foreground,
            palette,
            cursor,
            selection_background,
            selection_foreground,
        }) => {
            let mut colors = ColorsManifest::new(*background, *foreground, *palette);
            if let Some(value) = cursor {
                colors = colors.with_cursor(*value);
            }
            if let Some(value) = selection_background {
                colors = colors.with_selection_background(*value);
            }
            if let Some(value) = selection_foreground {
                colors = colors.with_selection_foreground(*value);
            }
            (Some(colors), None)
        }
        Some(ColorsIntent::Theme { theme }) => (
            Some(
                themes
                    .ok_or(PlanError::Unsupported("named theme adapter unavailable"))?
                    .resolve(theme)?,
            ),
            Some(theme.as_str()),
        ),
        Some(ColorsIntent::Generated) => (
            Some(
                crate::palette::generate_kmeans_v1(
                    selected_asset_bytes
                        .as_deref()
                        .ok_or(PlanError::Unsupported("generated colors require wallpaper"))?,
                )
                .map_err(|_| PlanError::UnsupportedImage)?,
            ),
            None,
        ),
        None => (None, None),
    };
    let terminal = profile
        .terminal
        .as_ref()
        .map(|terminal| {
            TerminalManifest::new(
                terminal.font_size,
                terminal.background_opacity,
                terminal.background_blur_intensity,
                terminal.cursor_style,
            )
        })
        .transpose()
        .map_err(manifest::ManifestCodecError::from)?;
    let environment = EnvironmentManifest::new(wallpaper, colors, terminal);
    let environment_id = manifest::environment_id(&environment)?;
    let manifest_json: Value = serde_json::from_slice(&manifest::encode_canonical(&environment)?)
        .map_err(manifest::ManifestCodecError::DecodeJson)?;

    let mut operations = Vec::new();
    if let Some(asset) = &asset_json {
        operations.push(json!({
            "kind": "ensure_asset",
            "asset_sha256": asset["sha256"],
            "disposition": asset_disposition(managed_root, asset["sha256"].as_str().unwrap_or_default(), asset["media_type"].as_str().unwrap_or_default())?,
        }));
    }
    operations.push(json!({
        "kind": "ensure_environment",
        "environment_id": environment_id.to_string(),
        "disposition": environment_disposition(managed_root, &environment_id.to_string())?,
    }));
    operations.push(json!({
        "kind": "activate_environment",
        "environment_id": environment_id.to_string(),
        "disposition": "apply",
    }));
    operations.push(reload_operation(platform.map_or(
        ReloadObservation::Unavailable(ReloadUnavailableReason::AdapterCommandUnavailable),
        |platform| platform.reload,
    )));

    diagnostics.sort_by(|left, right| {
        serde_jcs::to_vec(left)
            .expect("Diagnostic JSON is canonicalizable")
            .cmp(&serde_jcs::to_vec(right).expect("Diagnostic JSON is canonicalizable"))
    });
    let mut plan = json!({
        "schema_version": 1,
        "profile": { "id": profile_id.as_str(), "schema_version": 1 },
        "environment": { "environment_id": environment_id.to_string(), "manifest": manifest_json },
        "operations": operations,
        "diagnostics": diagnostics,
    });
    insert_optional(&mut plan, "source", source_json);
    insert_optional(&mut plan, "selection", selection_json);
    insert_optional(&mut plan, "asset", asset_json);
    let color_resolution = match &profile.colors {
        Some(ColorsIntent::Generated) => {
            Some(json!({ "kind": "generated", "algorithm": "kmeans-v1" }))
        }
        Some(ColorsIntent::Theme { .. }) => {
            let theme = theme_name.ok_or(PlanError::Unsupported("missing theme provenance"))?;
            Some(json!({
                "kind": "theme",
                "theme": theme,
                "content_sha256": theme_content_digest(&manifest_json["colors"])?.to_string(),
            }))
        }
        Some(ColorsIntent::Explicit { .. }) => Some(json!({ "kind": "explicit" })),
        None => None,
    };
    insert_optional(&mut plan, "color_resolution", color_resolution);
    Ok(plan)
}

fn theme_content_digest(colors: &Value) -> Result<Sha256Digest, PlanError> {
    let canonical = serde_jcs::to_vec(colors).map_err(manifest::ManifestCodecError::EncodeJson)?;
    let mut hash = Sha256::new();
    hash.update(THEME_RESOLUTION_DOMAIN);
    hash.update(canonical);
    Ok(Sha256Digest::from_bytes(hash.finalize().into()))
}

// RFC 0005 fixes these bytes so theme provenance cannot alias another digest protocol.
const THEME_RESOLUTION_DOMAIN: &[u8] = b"ghostty-wall.theme-resolution.v1\0";

struct SourceResolution {
    source: Value,
    selection: Value,
    bytes: Vec<u8>,
}

#[allow(clippy::too_many_arguments)]
fn resolve_local_source(
    config_dir: &Path,
    home: &Path,
    source_id: &IntentId,
    configured_path: &str,
    selection: &WallpaperSelection,
    seed: Option<&crate::domain::ResolutionSeed>,
    diagnostics: &mut Vec<Value>,
) -> Result<SourceResolution, PlanError> {
    let root = resolve_local_root(config_dir, home, configured_path)?;
    let root_file = open_source_root(&root)?;
    let (selected_index, candidate, selection_json) = match selection {
        WallpaperSelection::Random => {
            let seed = seed.ok_or(PlanError::MissingSeed)?;
            let (before, skipped) = enumerate_candidates(&root, &root_file)?;
            if skipped > 0 {
                diagnostics.push(json!({
                    "code": "source.skipped-non-utf8-entries",
                    "severity": "warning",
                    "source_id": source_id.as_str(),
                    "count": skipped,
                }));
            }
            let (index, candidate, selection) = random_selection(source_id, &before, seed)?;
            (Some(index), candidate, selection)
        }
        WallpaperSelection::Path(path) => {
            validate_candidate_extension(path)?;
            (
                None,
                path.clone(),
                json!({ "kind": "path", "candidate": path.as_str() }),
            )
        }
    };
    let bytes = read_candidate(&root, &root_file, &candidate)?;
    if selected_index.is_some()
        && fingerprint(&enumerate_candidates(&root, &root_file)?.0).to_string()
            != selection_json["candidate_set_fingerprint"]
                .as_str()
                .unwrap_or_default()
    {
        return Err(PlanError::SourceDrift);
    }
    Ok(SourceResolution {
        source: json!({
            "id": source_id.as_str(),
            "kind": "local-directory",
            "configured_path": configured_path,
            "resolved_root": root.display().to_string(),
        }),
        selection: selection_json,
        bytes,
    })
}

#[allow(clippy::too_many_arguments)]
fn resolve_github_source(
    source_id: &IntentId,
    repository: &str,
    configured_ref: Option<&str>,
    source_path: Option<&SourcePath>,
    selection: &WallpaperSelection,
    seed: Option<&crate::domain::ResolutionSeed>,
    github: &dyn GithubApi,
) -> Result<SourceResolution, PlanError> {
    let (reference, reference_kind) = match configured_ref {
        Some(reference) => (reference.to_owned(), "configured"),
        None => (
            github
                .default_branch(repository)
                .map_err(|error| github_error(source_id, error))?,
            "default-branch",
        ),
    };
    let commit = github
        .resolve_commit(repository, &reference)
        .map_err(|error| github_error(source_id, error))?;
    if !valid_github_commit(&commit) {
        return Err(github_error(source_id, GithubApiError::Unavailable));
    }

    let (candidate, selection_json) = match selection {
        WallpaperSelection::Random => {
            let seed = seed.ok_or(PlanError::MissingSeed)?;
            let tree = github
                .tree(repository, &commit, true)
                .map_err(|error| github_error(source_id, error))?;
            if tree.truncated() {
                return Err(PlanError::Github {
                    source_id: source_id.to_string(),
                    kind: GithubResolutionError::IncompleteTree,
                });
            }
            let candidates = CandidateSet::new(tree.entries().iter().filter_map(|entry| {
                if entry.kind() != GithubEntryKind::Blob
                    || !matches!(entry.mode(), "100644" | "100755")
                {
                    return None;
                }
                let relative = github_relative_path(entry.path(), source_path)?;
                let candidate = CandidatePath::from_str(relative).ok()?;
                eligible(Path::new(candidate.as_str())).then_some(candidate)
            }));
            let (_, candidate, selection) = random_selection(source_id, &candidates, seed)?;
            (candidate, selection)
        }
        WallpaperSelection::Path(path) => {
            validate_candidate_extension(path)?;
            (
                path.clone(),
                json!({ "kind": "path", "candidate": path.as_str() }),
            )
        }
    };
    let repository_path = github_repository_path(source_path, &candidate);
    let bytes = github
        .blob(repository, &commit, &repository_path)
        .map_err(|error| github_error(source_id, error))?;
    if bytes.len() as u64 > MAX_ASSET_BYTES {
        return Err(github_error(source_id, GithubApiError::Unavailable));
    }

    let mut source = json!({
        "id": source_id.as_str(),
        "kind": "github",
        "repository": repository,
        "ref": { "kind": reference_kind, "value": reference },
        "resolved_commit": commit,
    });
    if let Some(path) = source_path {
        insert_optional(
            &mut source,
            "path",
            Some(Value::String(path.as_str().to_owned())),
        );
    }
    Ok(SourceResolution {
        source,
        selection: selection_json,
        bytes,
    })
}

fn random_selection(
    source_id: &IntentId,
    candidates: &CandidateSet,
    seed: &crate::domain::ResolutionSeed,
) -> Result<(usize, CandidatePath, Value), PlanError> {
    let (index, candidate) = select_random_v1(candidates, seed).map_err(|error| match error {
        RandomSelectionError::EmptyCandidateSet => {
            PlanError::EmptyCandidateSet(source_id.to_string())
        }
        other => PlanError::Selection(other),
    })?;
    Ok((
        index,
        candidate.clone(),
        json!({
            "kind": "random",
            "algorithm": "random-v1",
            "seed": seed.to_string(),
            "candidate_set_fingerprint": fingerprint(candidates).to_string(),
            "candidate_count": candidates.len(),
            "selected_index": index,
            "candidate": candidate.as_str(),
        }),
    ))
}

fn validate_candidate_extension(candidate: &CandidatePath) -> Result<(), PlanError> {
    if eligible(Path::new(candidate.as_str())) {
        Ok(())
    } else {
        Err(PlanError::Unsupported(
            "Candidate path must have PNG or JPEG extension",
        ))
    }
}

fn github_relative_path<'a>(
    repository_path: &'a str,
    source_path: Option<&SourcePath>,
) -> Option<&'a str> {
    match source_path {
        Some(root) => repository_path
            .strip_prefix(root.as_str())?
            .strip_prefix('/'),
        None => Some(repository_path),
    }
}

fn github_repository_path(source_path: Option<&SourcePath>, candidate: &CandidatePath) -> String {
    source_path.map_or_else(
        || candidate.as_str().to_owned(),
        |root| format!("{}/{}", root.as_str(), candidate.as_str()),
    )
}

fn valid_github_commit(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn github_error(source_id: &IntentId, error: GithubApiError) -> PlanError {
    PlanError::Github {
        source_id: source_id.to_string(),
        kind: match error {
            GithubApiError::Authentication => GithubResolutionError::Authentication,
            GithubApiError::RateLimited => GithubResolutionError::RateLimited,
            GithubApiError::Unavailable => GithubResolutionError::Unavailable,
        },
    }
}

pub(crate) fn planned_local_asset_bytes(plan: &Value) -> Result<Option<Vec<u8>>, PlanError> {
    let Some(source) = plan.get("source") else {
        return Ok(None);
    };
    if source.get("kind").and_then(Value::as_str) != Some("local-directory") {
        return Err(PlanError::Unsupported(
            "only local-directory Sources implemented",
        ));
    }
    let root = source
        .get("resolved_root")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .ok_or(PlanError::Unsupported("invalid planned Source"))?;
    let candidate = plan
        .get("selection")
        .and_then(|value| value.get("candidate"))
        .and_then(Value::as_str)
        .ok_or(PlanError::Unsupported("invalid planned Selection"))?
        .parse::<CandidatePath>()
        .map_err(|_| PlanError::Unsupported("invalid planned Candidate"))?;
    let root_file = open_source_root(&root)?;
    let bytes = read_candidate(&root, &root_file, &candidate)?;
    let asset = plan
        .get("asset")
        .ok_or(PlanError::Unsupported("invalid planned Asset"))?;
    if asset.get("sha256").and_then(Value::as_str) != Some(&sha256(&bytes).to_string())
        || asset.get("byte_length").and_then(Value::as_u64) != Some(bytes.len() as u64)
        || asset.get("media_type").and_then(Value::as_str)
            != Some(detect_media_type(&bytes)?.as_str())
    {
        return Err(PlanError::SourceDrift);
    }
    Ok(Some(bytes))
}

pub(crate) fn planned_github_asset_bytes(
    plan: &Value,
    github: &dyn GithubApi,
) -> Result<Option<Vec<u8>>, PlanError> {
    let Some(source) = plan.get("source") else {
        return Ok(None);
    };
    if source.get("kind").and_then(Value::as_str) != Some("github") {
        return Err(PlanError::Unsupported("planned Source is not GitHub"));
    }
    let source_id = source
        .get("id")
        .and_then(Value::as_str)
        .ok_or(PlanError::Unsupported("invalid planned Source"))?
        .parse::<IntentId>()
        .map_err(|_| PlanError::Unsupported("invalid planned Source"))?;
    let repository = source
        .get("repository")
        .and_then(Value::as_str)
        .ok_or(PlanError::Unsupported("invalid planned Source"))?;
    let commit = source
        .get("resolved_commit")
        .and_then(Value::as_str)
        .filter(|value| valid_github_commit(value))
        .ok_or(PlanError::Unsupported("invalid planned Source"))?;
    let candidate = plan
        .get("selection")
        .and_then(|value| value.get("candidate"))
        .and_then(Value::as_str)
        .ok_or(PlanError::Unsupported("invalid planned Selection"))?
        .parse::<CandidatePath>()
        .map_err(|_| PlanError::Unsupported("invalid planned Candidate"))?;
    let source_path = source
        .get("path")
        .map(|value| {
            value
                .as_str()
                .ok_or(PlanError::Unsupported("invalid planned Source"))?
                .parse::<SourcePath>()
                .map_err(|_| PlanError::Unsupported("invalid planned Source"))
        })
        .transpose()?;
    let path = github_repository_path(source_path.as_ref(), &candidate);
    let bytes = github
        .blob(repository, commit, &path)
        .map_err(|error| github_error(&source_id, error))?;
    if bytes.len() as u64 > MAX_ASSET_BYTES {
        return Err(github_error(&source_id, GithubApiError::Unavailable));
    }
    let asset = plan
        .get("asset")
        .ok_or(PlanError::Unsupported("invalid planned Asset"))?;
    if asset.get("sha256").and_then(Value::as_str) != Some(&sha256(&bytes).to_string())
        || asset.get("byte_length").and_then(Value::as_u64) != Some(bytes.len() as u64)
        || asset.get("media_type").and_then(Value::as_str)
            != Some(detect_media_type(&bytes)?.as_str())
    {
        return Err(PlanError::SourceDrift);
    }
    Ok(Some(bytes))
}

fn resolve_local_root(
    config_dir: &Path,
    home: &Path,
    configured: &str,
) -> Result<PathBuf, PlanError> {
    let path = if let Some(rest) = configured.strip_prefix("~/") {
        home.join(rest)
    } else {
        let path = PathBuf::from(configured);
        if path.is_absolute() {
            path
        } else {
            config_dir.join(path)
        }
    };
    let canonical = fs::canonicalize(&path).map_err(|source| PlanError::Io { path, source })?;
    if !canonical.is_dir() {
        return Err(PlanError::Unsupported(
            "local Source root must be a directory",
        ));
    }
    Ok(canonical)
}

#[cfg(target_os = "linux")]
fn enumerate_candidates(
    root: &Path,
    root_file: &fs::File,
) -> Result<(CandidateSet, u64), PlanError> {
    let mut paths = Vec::new();
    let mut skipped = 0;
    enumerate_into(
        root,
        root_file,
        root_file,
        Path::new(""),
        &mut paths,
        &mut skipped,
    )?;
    Ok((CandidateSet::new(paths), skipped))
}

#[cfg(not(target_os = "linux"))]
fn enumerate_candidates(
    _root: &Path,
    _root_file: &fs::File,
) -> Result<(CandidateSet, u64), PlanError> {
    Err(PlanError::Unsupported("safe local enumeration unavailable"))
}

#[cfg(target_os = "linux")]
fn enumerate_into(
    root: &Path,
    root_file: &fs::File,
    dir: &fs::File,
    prefix: &Path,
    paths: &mut Vec<CandidatePath>,
    skipped: &mut u64,
) -> Result<(), PlanError> {
    use std::os::fd::AsRawFd;
    let pinned = PathBuf::from(format!("/proc/self/fd/{}", dir.as_raw_fd()));
    for entry in fs::read_dir(&pinned).map_err(|source| PlanError::Io {
        path: root.join(prefix),
        source,
    })? {
        let entry = entry.map_err(|source| PlanError::Io {
            path: root.join(prefix),
            source,
        })?;
        let relative = prefix.join(entry.file_name());
        let metadata = fs::symlink_metadata(entry.path()).map_err(|source| PlanError::Io {
            path: root.join(&relative),
            source,
        })?;
        if metadata.is_dir() {
            if relative.to_str().is_some() {
                let child = open_beneath(
                    root,
                    root_file,
                    &relative,
                    libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
                )?;
                enumerate_into(root, root_file, &child, &relative, paths, skipped)?;
            } else {
                *skipped = skipped
                    .checked_add(1)
                    .ok_or(PlanError::Unsupported("non-UTF-8 entry count overflow"))?;
            }
        } else if metadata.is_file() && eligible(&relative) {
            if let Some(value) = relative.to_str() {
                paths.push(
                    CandidatePath::from_str(value)
                        .map_err(|_| PlanError::Unsupported("invalid Candidate path"))?,
                );
            } else {
                *skipped = skipped
                    .checked_add(1)
                    .ok_or(PlanError::Unsupported("non-UTF-8 entry count overflow"))?;
            }
        }
    }
    Ok(())
}

fn eligible(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .map(|ext| matches!(ext.to_ascii_lowercase().as_str(), "png" | "jpg" | "jpeg"))
        .unwrap_or(false)
}

const MAX_ASSET_BYTES: u64 = 64 * 1024 * 1024;

#[cfg(target_os = "linux")]
fn open_source_root(root: &Path) -> Result<fs::File, PlanError> {
    use std::os::unix::fs::OpenOptionsExt;
    fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
        .open(root)
        .map_err(|source| PlanError::Io {
            path: root.to_owned(),
            source,
        })
}

#[cfg(not(target_os = "linux"))]
fn open_source_root(_root: &Path) -> Result<fs::File, PlanError> {
    Err(PlanError::Unsupported(
        "safe local Source resolution unavailable",
    ))
}

#[cfg(target_os = "linux")]
fn open_beneath(
    root: &Path,
    root_file: &fs::File,
    relative: &Path,
    flags: i32,
) -> Result<fs::File, PlanError> {
    use std::{
        ffi::CString,
        os::fd::{AsRawFd, FromRawFd},
    };
    let name = CString::new(
        relative
            .to_str()
            .ok_or(PlanError::Unsupported("non-UTF-8 Candidate path"))?,
    )
    .map_err(|_| PlanError::Unsupported("invalid Candidate path"))?;
    // libc's non-exhaustive open_how contains only integer fields; zero is valid.
    let mut how: libc::open_how = unsafe { std::mem::zeroed() };
    how.flags = flags as u64;
    how.resolve = libc::RESOLVE_BENEATH | libc::RESOLVE_NO_SYMLINKS;
    // Held dirfd and openat2 prohibit symlinks in every path component.
    let fd = unsafe {
        libc::syscall(
            libc::SYS_openat2,
            root_file.as_raw_fd(),
            name.as_ptr(),
            &how,
            std::mem::size_of::<libc::open_how>(),
        )
    };
    if fd < 0 {
        return Err(PlanError::Io {
            path: root.join(relative),
            source: std::io::Error::last_os_error(),
        });
    }
    // Successful syscall transfers descriptor ownership to File.
    Ok(unsafe { fs::File::from_raw_fd(fd as i32) })
}

#[cfg(target_os = "linux")]
fn read_candidate(
    root: &Path,
    root_file: &fs::File,
    candidate: &CandidatePath,
) -> Result<Vec<u8>, PlanError> {
    let file = open_beneath(
        root,
        root_file,
        Path::new(candidate.as_str()),
        libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NONBLOCK,
    )?;
    let path = root.join(candidate.as_str());
    if !file
        .metadata()
        .map_err(|source| PlanError::Io {
            path: path.clone(),
            source,
        })?
        .is_file()
    {
        return Err(PlanError::Unsupported("Candidate is not a regular file"));
    }
    let mut bytes = Vec::new();
    file.take(MAX_ASSET_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|source| PlanError::Io { path, source })?;
    if bytes.len() as u64 > MAX_ASSET_BYTES {
        return Err(PlanError::Unsupported("Candidate exceeds 64 MiB"));
    }
    Ok(bytes)
}

#[cfg(not(target_os = "linux"))]
fn read_candidate(
    _root: &Path,
    _root_file: &fs::File,
    _candidate: &CandidatePath,
) -> Result<Vec<u8>, PlanError> {
    Err(PlanError::Unsupported(
        "no safe local Candidate reopen on this platform",
    ))
}

fn detect_media_type(bytes: &[u8]) -> Result<MediaType, PlanError> {
    let format = image::guess_format(bytes).map_err(|_| PlanError::UnsupportedImage)?;
    let media_type = match format {
        image::ImageFormat::Png => MediaType::Png,
        image::ImageFormat::Jpeg => MediaType::Jpeg,
        _ => return Err(PlanError::UnsupportedImage),
    };
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16_384);
    limits.max_image_height = Some(16_384);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    reader.decode().map_err(|_| PlanError::UnsupportedImage)?;
    Ok(media_type)
}

fn sha256(bytes: &[u8]) -> Sha256Digest {
    let digest: [u8; 32] = Sha256::digest(bytes).into();
    Sha256Digest::from_bytes(digest)
}

fn validate_store_dirs(paths: &[PathBuf]) -> Result<(), PlanError> {
    for path in paths {
        match fs::symlink_metadata(path) {
            Ok(meta) if !meta.is_dir() => return Err(PlanError::Corrupt(path.clone())),
            Ok(_) => (),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(source) => {
                return Err(PlanError::Io {
                    path: path.clone(),
                    source,
                });
            }
        }
    }
    Ok(())
}

pub(crate) fn asset_disposition(
    root: &Path,
    digest: &str,
    media: &str,
) -> Result<&'static str, PlanError> {
    let store = root.join("assets/sha256");
    validate_store_dirs(&[root.to_owned(), root.join("assets"), store.clone()])?;
    if let Ok(meta) = fs::symlink_metadata(&store) {
        if !meta.is_dir() {
            return Err(PlanError::Corrupt(store));
        }
        let entries = fs::read_dir(&store).map_err(|source| PlanError::Io {
            path: store.clone(),
            source,
        })?;
        for entry in entries {
            let entry = entry.map_err(|source| PlanError::Io {
                path: store.clone(),
                source,
            })?;
            let shard = entry.path();
            let meta = fs::symlink_metadata(&shard).map_err(|source| PlanError::Io {
                path: shard.clone(),
                source,
            })?;
            if !meta.is_dir() {
                return Err(PlanError::Corrupt(shard));
            }
            if shard.file_name().and_then(|name| name.to_str()) != Some(&digest[..2]) {
                for item in fs::read_dir(&shard).map_err(|source| PlanError::Io {
                    path: shard.clone(),
                    source,
                })? {
                    let item = item.map_err(|source| PlanError::Io {
                        path: shard.clone(),
                        source,
                    })?;
                    if item.file_name().to_string_lossy().starts_with(digest) {
                        return Err(PlanError::Corrupt(item.path()));
                    }
                }
            }
        }
    }
    let directory = store.join(&digest[..2]);
    let png = directory.join(format!("{digest}.png"));
    let jpg = directory.join(format!("{digest}.jpg"));
    let expected = if media == "image/png" { &png } else { &jpg };
    let other = if media == "image/png" { &jpg } else { &png };
    if fs::symlink_metadata(other).is_ok() {
        return Err(PlanError::Corrupt(other.clone()));
    }
    match fs::symlink_metadata(expected) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok("create"),
        Err(source) => Err(PlanError::Io {
            path: expected.clone(),
            source,
        }),
        Ok(metadata) => {
            if !metadata.is_file() || metadata.len() > MAX_ASSET_BYTES {
                return Err(PlanError::Corrupt(expected.clone()));
            }
            let bytes = fs::read(expected).map_err(|source| PlanError::Io {
                path: expected.clone(),
                source,
            })?;
            if sha256(&bytes).to_string() != digest
                || detect_media_type(&bytes).ok().map(MediaType::as_str) != Some(media)
            {
                return Err(PlanError::Corrupt(expected.clone()));
            }
            Ok("reuse")
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EnvironmentRecord<'a> {
    record_schema_version: u64,
    environment_id: String,
    #[serde(borrow)]
    manifest: &'a serde_json::value::RawValue,
}

pub(crate) fn environment_disposition(root: &Path, id: &str) -> Result<&'static str, PlanError> {
    validate_store_dirs(&[root.to_owned(), root.join("environments")])?;
    let path = root.join("environments").join(format!("{id}.json"));
    match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok("create"),
        Err(source) => Err(PlanError::Io { path, source }),
        Ok(metadata) => {
            if !metadata.is_file() || metadata.len() > 1024 * 1024 {
                return Err(PlanError::Corrupt(path));
            }
            let bytes = fs::read(&path).map_err(|source| PlanError::Io {
                path: path.clone(),
                source,
            })?;
            let record: EnvironmentRecord<'_> =
                serde_json::from_slice(&bytes).map_err(|_| PlanError::Corrupt(path.clone()))?;
            if record.record_schema_version != 1 || record.environment_id != id {
                return Err(PlanError::Corrupt(path));
            }
            let manifest = manifest::decode(record.manifest.get().as_bytes())
                .map_err(|_| PlanError::Corrupt(path.clone()))?;
            if manifest::environment_id(&manifest)
                .map_err(|_| PlanError::Corrupt(path.clone()))?
                .to_string()
                != id
            {
                return Err(PlanError::Corrupt(path));
            }
            Ok("reuse")
        }
    }
}

fn reload_operation(observation: ReloadObservation) -> Value {
    match observation {
        ReloadObservation::Systemd => {
            json!({ "kind": "reload_ghostty", "required": false, "adapter": "systemd" })
        }
        ReloadObservation::Applescript => {
            json!({ "kind": "reload_ghostty", "required": false, "adapter": "applescript" })
        }
        ReloadObservation::Unavailable(reason) => json!({
            "kind": "reload_ghostty",
            "required": false,
            "adapter": "unavailable",
            "reason": match reason {
                ReloadUnavailableReason::UnsupportedPlatform => "unsupported-platform",
                ReloadUnavailableReason::AdapterCommandUnavailable => "adapter-command-unavailable",
                ReloadUnavailableReason::GhosttyIntegrationUnavailable => "ghostty-integration-unavailable",
            },
        }),
    }
}

fn insert_optional(plan: &mut Value, key: &'static str, value: Option<Value>) {
    if let Some(value) = value {
        plan.as_object_mut()
            .expect("plan is object")
            .insert(key.to_owned(), value);
    }
}
