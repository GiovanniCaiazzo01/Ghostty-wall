//! Durable Profile apply and History replay.

use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

use serde_json::{Value, json};
use thiserror::Error;

use crate::{
    codec::manifest,
    domain::{
        ActivationId, ConfigIntent, EnvironmentId, IntentId, MAX_ACTIVATION_SEQUENCE,
        ProfileIntent, ResolutionSeed,
    },
    github::GithubApi,
    history::{HistoryError, inspect_history_unlocked, timestamp_valid},
    plan::{
        PlanError, PlanPlatform, asset_disposition, environment_disposition,
        plan_github_profile_json, plan_github_profile_with_theme_json, plan_local_profile_json,
        plan_local_profile_with_theme_json, planned_github_asset_bytes, planned_local_asset_bytes,
    },
    recovery::{
        RecoveryError, exclusive_state_lock, inspect_integration_hook,
        materialize_projection_unlocked, reconcile_recovery_state_unlocked,
    },
    runtime::{ReloadAdapter, ReloadOutcome},
    theme::ThemeResolver,
};

/// Failure before or during durable Activation publication.
#[derive(Debug, Error)]
pub enum ApplyError {
    /// Profile resolution or Plan validation failed before mutation.
    #[error(transparent)]
    Plan(#[from] PlanError),
    /// Committed History or one of its dependencies is corrupt.
    #[error(transparent)]
    History(#[from] HistoryError),
    /// Recovery or integration validation failed.
    #[error(transparent)]
    Recovery(#[from] RecoveryError),
    /// Filesystem mutation failed.
    #[error("cannot apply at {path}: {source}")]
    Io {
        /// Path being changed.
        path: PathBuf,
        /// Underlying filesystem error.
        #[source]
        source: io::Error,
    },
    /// Caller supplied a non-canonical Activation timestamp.
    #[error("activated_at must be UTC RFC 3339 with six fractional digits")]
    InvalidTimestamp,
    /// Current History Cursor has no predecessor.
    #[error("current Activation has no previous target")]
    NoPrevious,
    /// History exhausted v1's JSON-safe sequence range.
    #[error("Activation sequence limit reached")]
    SequenceExhausted,
    /// Internally generated Plan disagreed with validated domain content.
    #[error("resolved Plan violated an internal invariant")]
    InvalidPlan,
}

/// Durable Activation result plus separate best-effort reload outcome.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ApplyOutcome {
    activation_id: ActivationId,
    reload_outcome: ReloadOutcome,
}

impl ApplyOutcome {
    /// Newly committed durable Activation identity.
    pub const fn activation_id(self) -> ActivationId {
        self.activation_id
    }

    /// Whether post-commit runtime reload succeeded.
    pub const fn reload_succeeded(self) -> bool {
        matches!(self.reload_outcome, ReloadOutcome::Succeeded)
    }

    /// Post-commit runtime outcome, separate from durable Activation success.
    pub const fn reload_outcome(self) -> ReloadOutcome {
        self.reload_outcome
    }
}

/// Resolve and durably apply one local Profile, then attempt reload.
#[allow(clippy::too_many_arguments)]
pub fn apply_local_profile<R: ReloadAdapter>(
    config_dir: &Path,
    home: &Path,
    managed_root: &Path,
    effective_root_config: &Path,
    profile_id: &IntentId,
    config: &ConfigIntent,
    profile: &ProfileIntent,
    seed: Option<&ResolutionSeed>,
    activated_at: &str,
    reload: R,
) -> Result<ApplyOutcome, ApplyError> {
    validate_timestamp(activated_at)?;
    let platform = PlanPlatform::new(effective_root_config.to_owned(), reload.observation());
    let plan = plan_local_profile_json(
        config_dir,
        home,
        managed_root,
        profile_id,
        config,
        profile,
        seed,
        &platform,
    )?;
    let asset_bytes = planned_local_asset_bytes(&plan)?;
    apply_resolved_profile(
        managed_root,
        effective_root_config,
        activated_at,
        reload,
        plan,
        asset_bytes,
    )
}

/// Resolve and durably apply one local Profile through a named-theme adapter.
#[allow(clippy::too_many_arguments)]
pub fn apply_local_profile_with_theme<R: ReloadAdapter>(
    config_dir: &Path,
    home: &Path,
    managed_root: &Path,
    effective_root_config: &Path,
    profile_id: &IntentId,
    config: &ConfigIntent,
    profile: &ProfileIntent,
    seed: Option<&ResolutionSeed>,
    themes: &dyn ThemeResolver,
    activated_at: &str,
    reload: R,
) -> Result<ApplyOutcome, ApplyError> {
    validate_timestamp(activated_at)?;
    let platform = PlanPlatform::new(effective_root_config.to_owned(), reload.observation());
    let plan = plan_local_profile_with_theme_json(
        config_dir,
        home,
        managed_root,
        profile_id,
        config,
        profile,
        seed,
        &platform,
        themes,
    )?;
    let asset_bytes = planned_local_asset_bytes(&plan)?;
    apply_resolved_profile(
        managed_root,
        effective_root_config,
        activated_at,
        reload,
        plan,
        asset_bytes,
    )
}

/// Resolve and durably apply one Profile through a commit-pinned GitHub adapter.
#[allow(clippy::too_many_arguments)]
pub fn apply_github_profile<R: ReloadAdapter>(
    config_dir: &Path,
    home: &Path,
    managed_root: &Path,
    effective_root_config: &Path,
    profile_id: &IntentId,
    config: &ConfigIntent,
    profile: &ProfileIntent,
    seed: Option<&ResolutionSeed>,
    activated_at: &str,
    github: &dyn GithubApi,
    reload: R,
) -> Result<ApplyOutcome, ApplyError> {
    validate_timestamp(activated_at)?;
    let platform = PlanPlatform::new(effective_root_config.to_owned(), reload.observation());
    let plan = plan_github_profile_json(
        config_dir,
        home,
        managed_root,
        profile_id,
        config,
        profile,
        seed,
        &platform,
        github,
    )?;
    let asset_bytes = planned_github_asset_bytes(&plan, github)?;
    apply_resolved_profile(
        managed_root,
        effective_root_config,
        activated_at,
        reload,
        plan,
        asset_bytes,
    )
}

fn apply_resolved_profile<R: ReloadAdapter>(
    managed_root: &Path,
    effective_root_config: &Path,
    activated_at: &str,
    reload: R,
    plan: Value,
    asset_bytes: Option<Vec<u8>>,
) -> Result<ApplyOutcome, ApplyError> {
    let lock = exclusive_state_lock(&managed_root.join("state.lock"))?;
    inspect_integration_hook(effective_root_config, &managed_root.join("current.ghostty"))?;
    let history = inspect_history_unlocked(managed_root)?;
    let next = next_sequence(history.latest().map(|activation| activation.sequence()))?;
    reconcile_recovery_state_unlocked(managed_root)?;

    let environment_value = plan.get("environment").ok_or(ApplyError::InvalidPlan)?;
    let environment_id = environment_value
        .get("environment_id")
        .and_then(Value::as_str)
        .ok_or(ApplyError::InvalidPlan)?
        .parse::<EnvironmentId>()
        .map_err(|_| ApplyError::InvalidPlan)?;
    let manifest_value = environment_value
        .get("manifest")
        .ok_or(ApplyError::InvalidPlan)?;
    let manifest_bytes = serde_jcs::to_vec(manifest_value).map_err(|_| ApplyError::InvalidPlan)?;
    let environment = manifest::decode(&manifest_bytes).map_err(|_| ApplyError::InvalidPlan)?;
    if manifest::environment_id(&environment).map_err(|_| ApplyError::InvalidPlan)?
        != environment_id
    {
        return Err(ApplyError::InvalidPlan);
    }

    if let Some(asset) = plan.get("asset") {
        ensure_asset(
            managed_root,
            asset,
            asset_bytes.as_deref().ok_or(ApplyError::InvalidPlan)?,
        )?;
    } else if asset_bytes.is_some() {
        return Err(ApplyError::InvalidPlan);
    }
    ensure_environment(managed_root, environment_id, manifest_value)?;
    materialize_projection_unlocked(managed_root, &environment)?;

    let id = ActivationId::new(next).map_err(|_| ApplyError::SequenceExhausted)?;
    let mut record = json!({
        "record_schema_version": 1,
        "activation_id": id.to_string(),
        "sequence": next,
        "history_cursor": next,
        "activated_at": activated_at,
        "environment_id": environment_id.to_string(),
        "cause": { "kind": "profile" },
        "profile": plan.get("profile").ok_or(ApplyError::InvalidPlan)?,
    });
    for key in ["source", "selection", "asset", "color_resolution"] {
        if let Some(value) = plan.get(key) {
            record
                .as_object_mut()
                .expect("Activation record is an object")
                .insert(key.to_owned(), value.clone());
        }
    }
    publish_activation(managed_root, id, &record)?;
    drop(lock);

    Ok(ApplyOutcome {
        activation_id: id,
        reload_outcome: reload.reload(),
    })
}

/// Resolve and durably apply one GitHub Profile through a named-theme adapter.
#[allow(clippy::too_many_arguments)]
pub fn apply_github_profile_with_theme<R: ReloadAdapter>(
    config_dir: &Path,
    home: &Path,
    managed_root: &Path,
    effective_root_config: &Path,
    profile_id: &IntentId,
    config: &ConfigIntent,
    profile: &ProfileIntent,
    seed: Option<&ResolutionSeed>,
    themes: &dyn ThemeResolver,
    activated_at: &str,
    github: &dyn GithubApi,
    reload: R,
) -> Result<ApplyOutcome, ApplyError> {
    validate_timestamp(activated_at)?;
    let platform = PlanPlatform::new(effective_root_config.to_owned(), reload.observation());
    let plan = plan_github_profile_with_theme_json(
        config_dir,
        home,
        managed_root,
        profile_id,
        config,
        profile,
        seed,
        &platform,
        github,
        themes,
    )?;
    let asset_bytes = planned_github_asset_bytes(&plan, github)?;
    apply_resolved_profile(
        managed_root,
        effective_root_config,
        activated_at,
        reload,
        plan,
        asset_bytes,
    )
}

/// Replay predecessor selected by current durable History Cursor, then reload.
pub fn previous<R: ReloadAdapter>(
    managed_root: &Path,
    effective_root_config: &Path,
    activated_at: &str,
    reload: R,
) -> Result<ApplyOutcome, ApplyError> {
    validate_timestamp(activated_at)?;
    let lock = exclusive_state_lock(&managed_root.join("state.lock"))?;
    inspect_integration_hook(effective_root_config, &managed_root.join("current.ghostty"))?;
    let history = inspect_history_unlocked(managed_root)?;
    reconcile_recovery_state_unlocked(managed_root)?;
    let current = history.latest().ok_or(ApplyError::NoPrevious)?;
    let next = next_sequence(Some(current.sequence()))?;
    let target = history.previous_target().ok_or(ApplyError::NoPrevious)?;

    materialize_projection_unlocked(managed_root, target.environment())?;
    let id = ActivationId::new(next).map_err(|_| ApplyError::SequenceExhausted)?;
    let target_id = ActivationId::new(target.sequence()).map_err(|_| ApplyError::InvalidPlan)?;
    let record = json!({
        "record_schema_version": 1,
        "activation_id": id.to_string(),
        "sequence": next,
        "history_cursor": target.history_cursor(),
        "activated_at": activated_at,
        "environment_id": target.environment_id().to_string(),
        "cause": {
            "kind": "history-replay",
            "activation_id": target_id.to_string(),
        },
    });
    publish_activation(managed_root, id, &record)?;
    drop(lock);

    Ok(ApplyOutcome {
        activation_id: id,
        reload_outcome: reload.reload(),
    })
}

fn validate_timestamp(value: &str) -> Result<(), ApplyError> {
    if timestamp_valid(value) {
        Ok(())
    } else {
        Err(ApplyError::InvalidTimestamp)
    }
}

fn next_sequence(current: Option<u64>) -> Result<u64, ApplyError> {
    let next = current
        .map_or(Some(1), |sequence| sequence.checked_add(1))
        .ok_or(ApplyError::SequenceExhausted)?;
    if next <= MAX_ACTIVATION_SEQUENCE {
        Ok(next)
    } else {
        Err(ApplyError::SequenceExhausted)
    }
}

fn ensure_asset(root: &Path, asset: &Value, bytes: &[u8]) -> Result<(), ApplyError> {
    let digest = asset
        .get("sha256")
        .and_then(Value::as_str)
        .ok_or(ApplyError::InvalidPlan)?;
    let media = asset
        .get("media_type")
        .and_then(Value::as_str)
        .ok_or(ApplyError::InvalidPlan)?;
    if asset.get("byte_length").and_then(Value::as_u64) != Some(bytes.len() as u64) {
        return Err(ApplyError::InvalidPlan);
    }
    if asset_disposition(root, digest, media)? == "reuse" {
        return Ok(());
    }
    let shard = PathBuf::from("assets/sha256").join(&digest[..2]);
    create_private_dir(&root.join(&shard))?;
    let extension = if media == "image/png" { "png" } else { "jpg" };
    publish_new(root, &shard.join(format!("{digest}.{extension}")), bytes)
}

fn ensure_environment(root: &Path, id: EnvironmentId, manifest: &Value) -> Result<(), ApplyError> {
    if environment_disposition(root, &id.to_string())? == "reuse" {
        return Ok(());
    }
    let record = json!({
        "record_schema_version": 1,
        "environment_id": id.to_string(),
        "manifest": manifest,
    });
    let bytes = serde_jcs::to_vec(&record).map_err(|_| ApplyError::InvalidPlan)?;
    publish_new(
        root,
        &PathBuf::from("environments").join(format!("{id}.json")),
        &bytes,
    )
}

fn publish_activation(root: &Path, id: ActivationId, record: &Value) -> Result<(), ApplyError> {
    let bytes = serde_jcs::to_vec(record).map_err(|_| ApplyError::InvalidPlan)?;
    publish_new(
        root,
        &PathBuf::from("history/activations").join(format!("{id}.json")),
        &bytes,
    )
}

fn create_private_dir(path: &Path) -> Result<(), ApplyError> {
    if path.exists() {
        let metadata = fs::symlink_metadata(path).map_err(|source| io_error(path, source))?;
        if metadata.is_dir() && !metadata.file_type().is_symlink() {
            return Ok(());
        }
        return Err(io_error(path, io::Error::other("not a directory")));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        let mut builder = fs::DirBuilder::new();
        builder.mode(0o700);
        builder
            .create(path)
            .map_err(|source| io_error(path, source))?;
    }
    #[cfg(not(unix))]
    fs::create_dir(path).map_err(|source| io_error(path, source))?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn publish_new(root: &Path, relative: &Path, bytes: &[u8]) -> Result<(), ApplyError> {
    use std::{
        ffi::CString,
        os::fd::{AsRawFd, FromRawFd},
    };

    let parent = relative.parent().ok_or(ApplyError::InvalidPlan)?;
    let directory = open_managed_directory(root, parent)?;
    let final_label = relative
        .file_name()
        .ok_or(ApplyError::InvalidPlan)?
        .to_string_lossy();
    let temp_label = format!(".tmp-{}-{final_label}", std::process::id());
    let final_name = CString::new(final_label.as_bytes()).map_err(|_| ApplyError::InvalidPlan)?;
    let temp_name = CString::new(temp_label.as_bytes()).map_err(|_| ApplyError::InvalidPlan)?;
    let temp_path = root.join(parent).join(&temp_label);
    let final_path = root.join(relative);
    let result = (|| {
        let fd = unsafe {
            libc::openat(
                directory.as_raw_fd(),
                temp_name.as_ptr(),
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                0o600,
            )
        };
        if fd < 0 {
            return Err(io_error(&temp_path, io::Error::last_os_error()));
        }
        let mut file = unsafe { File::from_raw_fd(fd) };
        file.write_all(bytes)
            .map_err(|source| io_error(&temp_path, source))?;
        file.sync_all()
            .map_err(|source| io_error(&temp_path, source))?;
        if unsafe {
            libc::linkat(
                directory.as_raw_fd(),
                temp_name.as_ptr(),
                directory.as_raw_fd(),
                final_name.as_ptr(),
                0,
            )
        } != 0
        {
            return Err(io_error(&final_path, io::Error::last_os_error()));
        }
        // No-replace publication plus directory fsync is durable commit point;
        // temporary-link cleanup cannot revoke successful commit.
        directory
            .sync_all()
            .map_err(|source| io_error(&root.join(parent), source))?;
        let _ = unsafe { libc::unlinkat(directory.as_raw_fd(), temp_name.as_ptr(), 0) };
        Ok(())
    })();
    if result.is_err() {
        let _ = unsafe { libc::unlinkat(directory.as_raw_fd(), temp_name.as_ptr(), 0) };
    }
    result
}

#[cfg(not(target_os = "linux"))]
fn publish_new(root: &Path, relative: &Path, bytes: &[u8]) -> Result<(), ApplyError> {
    let path = root.join(relative);
    let directory = path.parent().ok_or(ApplyError::InvalidPlan)?;
    let temp = directory.join(format!(
        ".tmp-{}-{}",
        std::process::id(),
        path.file_name().unwrap().to_string_lossy()
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|source| io_error(&temp, source))?;
        file.write_all(bytes)
            .map_err(|source| io_error(&temp, source))?;
        file.sync_all().map_err(|source| io_error(&temp, source))?;
        fs::hard_link(&temp, &path).map_err(|source| io_error(&path, source))?;
        File::open(directory)
            .and_then(|handle| handle.sync_all())
            .map_err(|source| io_error(directory, source))?;
        let _ = fs::remove_file(&temp);
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

#[cfg(target_os = "linux")]
fn open_managed_directory(root: &Path, relative: &Path) -> Result<File, ApplyError> {
    use std::{
        ffi::CString,
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::{ffi::OsStrExt, fs::OpenOptionsExt},
        },
    };

    let root_file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(root)
        .map_err(|source| io_error(root, source))?;
    let name =
        CString::new(relative.as_os_str().as_bytes()).map_err(|_| ApplyError::InvalidPlan)?;
    let mut how: libc::open_how = unsafe { std::mem::zeroed() };
    how.flags = (libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC) as u64;
    how.resolve = libc::RESOLVE_BENEATH | libc::RESOLVE_NO_SYMLINKS;
    let fd = unsafe {
        libc::syscall(
            libc::SYS_openat2,
            root_file.as_raw_fd(),
            name.as_ptr(),
            &how,
            std::mem::size_of::<libc::open_how>(),
        )
    } as i32;
    if fd < 0 {
        return Err(io_error(&root.join(relative), io::Error::last_os_error()));
    }
    Ok(unsafe { File::from_raw_fd(fd) })
}

fn io_error(path: &Path, source: io::Error) -> ApplyError {
    ApplyError::Io {
        path: path.to_owned(),
        source,
    }
}
