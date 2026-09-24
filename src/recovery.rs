//! Read-only Recovery Inspection from RFC 0007.

use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
};

use thiserror::Error;

use crate::{
    domain::{EnvironmentManifest, WallpaperManifest},
    history::{HistoryError, inspect_history_unlocked},
    init::hook_count,
};

/// Managed Projection state relative to committed History.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectionState {
    /// Actual Projection matches committed state.
    Consistent,
    /// History requires Projection, but none exists.
    Missing,
    /// Empty History requires no Projection, but an entry exists.
    Unexpected,
    /// Projection exists but does not match committed Environment.
    OutOfSync,
}

impl ProjectionState {
    /// RFC 0005 Diagnostic code for drift, if any.
    pub const fn diagnostic_code(self) -> Option<&'static str> {
        match self {
            Self::Consistent => None,
            Self::Missing => Some("projection.missing"),
            Self::Unexpected => Some("projection.unexpected"),
            Self::OutOfSync => Some("projection.out-of-sync"),
        }
    }
}

/// Read-only synchronized Recovery result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecoveryInspection {
    projection: ProjectionState,
}

impl RecoveryInspection {
    /// Projection classification observed under state lock.
    pub const fn projection(&self) -> ProjectionState {
        self.projection
    }
}

/// Failure validating committed state or effective Ghostty integration.
#[derive(Debug, Error)]
pub enum RecoveryError {
    /// Committed History or one of its durable dependencies is invalid.
    #[error(transparent)]
    History(#[from] HistoryError),
    /// Read-only inspection failed.
    #[error("cannot inspect {path}: {source}")]
    Io {
        /// Path being inspected.
        path: PathBuf,
        /// Underlying filesystem error.
        source: io::Error,
    },
    /// Effective Ghostty root configuration lacks exactly one managed hook.
    #[error("effective Ghostty integration drift at {0}")]
    IntegrationDrift(PathBuf),
    /// Required race-safe inspection primitive is unavailable.
    #[error("Recovery Inspection unsupported on this platform")]
    UnsupportedPlatform,
}

/// Validate committed state, classify Projection, and verify effective hook.
///
/// Inspection holds a shared state lock and performs no filesystem mutation.
pub fn inspect_recovery_state(
    managed_root: &Path,
    effective_root_config: &Path,
) -> Result<RecoveryInspection, RecoveryError> {
    let _lock = shared_state_lock(&managed_root.join("state.lock"))?;
    let history = inspect_history_unlocked(managed_root)?;
    let projection_path = managed_root.join("current.ghostty");
    let actual = read_projection(&projection_path)?;
    let projection = match history.latest() {
        None => {
            if matches!(actual, ActualProjection::Absent) {
                ProjectionState::Consistent
            } else {
                ProjectionState::Unexpected
            }
        }
        Some(activation) => match actual {
            ActualProjection::Absent => ProjectionState::Missing,
            ActualProjection::Invalid => ProjectionState::OutOfSync,
            ActualProjection::Model(actual) => {
                let expected = projection_model(managed_root, activation.environment());
                if actual == expected {
                    ProjectionState::Consistent
                } else {
                    ProjectionState::OutOfSync
                }
            }
        },
    };
    inspect_integration_hook(effective_root_config, &projection_path)?;
    Ok(RecoveryInspection { projection })
}

/// Restore only derived Projection state from latest committed Activation.
pub fn reconcile_recovery_state(
    managed_root: &Path,
    effective_root_config: &Path,
) -> Result<(), RecoveryError> {
    let _lock = exclusive_state_lock(&managed_root.join("state.lock"))?;
    inspect_integration_hook(effective_root_config, &managed_root.join("current.ghostty"))?;
    reconcile_recovery_state_unlocked(managed_root)
}

pub(crate) fn reconcile_recovery_state_unlocked(managed_root: &Path) -> Result<(), RecoveryError> {
    let history = inspect_history_unlocked(managed_root)?;
    let projection = managed_root.join("current.ghostty");
    match history.latest() {
        Some(activation) => atomic_projection(
            managed_root,
            &projection,
            &render_projection(managed_root, activation.environment()),
        ),
        None => remove_projection(managed_root, &projection),
    }
}

fn io_error(path: &Path, source: io::Error) -> RecoveryError {
    RecoveryError::Io {
        path: path.to_owned(),
        source,
    }
}

#[cfg(unix)]
fn shared_state_lock(path: &Path) -> Result<File, RecoveryError> {
    use std::{os::fd::AsRawFd, os::unix::fs::OpenOptionsExt};

    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .map_err(|error| io_error(path, error))?;
    if !file
        .metadata()
        .map_err(|error| io_error(path, error))?
        .is_file()
    {
        return Err(io_error(
            path,
            io::Error::other("state lock is not a regular file"),
        ));
    }
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_SH) } != 0 {
        return Err(io_error(path, io::Error::last_os_error()));
    }
    Ok(file)
}

#[cfg(not(unix))]
fn shared_state_lock(_path: &Path) -> Result<File, RecoveryError> {
    Err(RecoveryError::UnsupportedPlatform)
}

#[cfg(unix)]
pub(crate) fn exclusive_state_lock(path: &Path) -> Result<File, RecoveryError> {
    use std::{os::fd::AsRawFd, os::unix::fs::OpenOptionsExt};

    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .map_err(|error| io_error(path, error))?;
    if !file
        .metadata()
        .map_err(|error| io_error(path, error))?
        .is_file()
    {
        return Err(io_error(
            path,
            io::Error::other("state lock is not a regular file"),
        ));
    }
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } != 0 {
        return Err(io_error(path, io::Error::last_os_error()));
    }
    Ok(file)
}

#[cfg(not(unix))]
pub(crate) fn exclusive_state_lock(_path: &Path) -> Result<File, RecoveryError> {
    Err(RecoveryError::UnsupportedPlatform)
}

enum ActualProjection {
    Absent,
    Invalid,
    Model(BTreeMap<String, String>),
}

#[cfg(target_os = "linux")]
fn read_projection(path: &Path) -> Result<ActualProjection, RecoveryError> {
    use std::{os::fd::AsRawFd, os::unix::fs::OpenOptionsExt};

    let handle = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_PATH | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(ActualProjection::Absent);
        }
        Err(error) => return Err(io_error(path, error)),
    };
    if !handle
        .metadata()
        .map_err(|error| io_error(path, error))?
        .is_file()
    {
        return Ok(ActualProjection::Invalid);
    }
    let pinned = PathBuf::from(format!("/proc/self/fd/{}", handle.as_raw_fd()));
    let mut file = File::open(&pinned).map_err(|error| io_error(path, error))?;
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| io_error(path, error))?;
    if bytes.len() > 1024 * 1024 {
        return Ok(ActualProjection::Invalid);
    }
    let Ok(text) = String::from_utf8(bytes) else {
        return Ok(ActualProjection::Invalid);
    };
    Ok(parse_projection(&text)
        .map(ActualProjection::Model)
        .unwrap_or(ActualProjection::Invalid))
}

#[cfg(not(target_os = "linux"))]
fn read_projection(_path: &Path) -> Result<ActualProjection, RecoveryError> {
    Err(RecoveryError::UnsupportedPlatform)
}

pub(crate) fn inspect_integration_hook(
    config: &Path,
    projection: &Path,
) -> Result<(), RecoveryError> {
    let target =
        fs::canonicalize(config).map_err(|_| RecoveryError::IntegrationDrift(config.to_owned()))?;
    let bytes = read_regular_bounded(&target, 1024 * 1024)?;
    let unchanged = fs::canonicalize(config)
        .map(|current| current == target)
        .unwrap_or(false);
    let Ok(text) = String::from_utf8(bytes) else {
        return Err(RecoveryError::IntegrationDrift(config.to_owned()));
    };
    if !unchanged || hook_count(&text, &target, projection) != 1 {
        return Err(RecoveryError::IntegrationDrift(config.to_owned()));
    }
    Ok(())
}

fn read_regular_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, RecoveryError> {
    #[cfg(unix)]
    use std::os::unix::fs::OpenOptionsExt;

    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK);
    let file = options.open(path).map_err(|error| io_error(path, error))?;
    if !file
        .metadata()
        .map_err(|error| io_error(path, error))?
        .is_file()
    {
        return Err(io_error(path, io::Error::other("not a regular file")));
    }
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| io_error(path, error))?;
    if bytes.len() as u64 > limit {
        return Err(io_error(path, io::Error::other("file exceeds size limit")));
    }
    Ok(bytes)
}

pub(crate) fn materialize_projection_unlocked(
    root: &Path,
    manifest: &EnvironmentManifest,
) -> Result<(), RecoveryError> {
    atomic_projection(
        root,
        &root.join("current.ghostty"),
        &render_projection(root, manifest),
    )
}

pub(crate) fn render_projection(root: &Path, manifest: &EnvironmentManifest) -> Vec<u8> {
    let mut rendered = String::new();
    for (key, value) in projection_model(root, manifest) {
        if let Some(index) = key.strip_prefix("palette:") {
            rendered.push_str(&format!("palette = {index}={value}\n"));
        } else {
            let value = match key.as_str() {
                "background-image-opacity" | "background-opacity" => render_fixed(&value, 6),
                "font-size" => render_fixed(&value, 3),
                _ => value,
            };
            rendered.push_str(&format!("{key} = {value}\n"));
        }
    }
    rendered.into_bytes()
}

fn render_fixed(value: &str, places: usize) -> String {
    let value = value
        .parse::<u64>()
        .expect("Projection model uses integers");
    let scale = 10u64.pow(u32::try_from(places).expect("small fixed-point precision"));
    format!(
        "{}.{:0width$}",
        value / scale,
        value % scale,
        width = places
    )
}

fn projection_model(root: &Path, manifest: &EnvironmentManifest) -> BTreeMap<String, String> {
    let mut model = BTreeMap::new();
    if let Some(wallpaper) = manifest.wallpaper() {
        match wallpaper {
            WallpaperManifest::None => {
                model.insert("background-image".into(), String::new());
            }
            WallpaperManifest::Image(image) => {
                let digest = image.asset_sha256().to_string();
                let extension = if image.media_type().as_str() == "image/png" {
                    "png"
                } else {
                    "jpg"
                };
                let path = root
                    .join("assets/sha256")
                    .join(&digest[..2])
                    .join(format!("{digest}.{extension}"));
                model.insert(
                    "background-image".into(),
                    normalize_path(&path).display().to_string(),
                );
                if let Some(value) = image.fit() {
                    model.insert("background-image-fit".into(), value.as_str().into());
                }
                if let Some(value) = image.position() {
                    model.insert("background-image-position".into(), value.as_str().into());
                }
                if let Some(value) = image.opacity() {
                    model.insert("background-image-opacity".into(), value.get().to_string());
                }
                if let Some(value) = image.repeat() {
                    model.insert("background-image-repeat".into(), value.to_string());
                }
            }
        }
    }
    if let Some(colors) = manifest.colors() {
        model.insert("background".into(), colors.background().to_string());
        model.insert("foreground".into(), colors.foreground().to_string());
        for (index, color) in colors.palette().iter().enumerate() {
            model.insert(format!("palette:{index}"), color.to_string());
        }
        if let Some(value) = colors.cursor() {
            model.insert("cursor-color".into(), value.to_string());
        }
        if let Some(value) = colors.selection_background() {
            model.insert("selection-background".into(), value.to_string());
        }
        if let Some(value) = colors.selection_foreground() {
            model.insert("selection-foreground".into(), value.to_string());
        }
    }
    if let Some(terminal) = manifest.terminal() {
        if let Some(value) = terminal.font_size() {
            model.insert("font-size".into(), value.get().to_string());
        }
        if let Some(value) = terminal.background_opacity() {
            model.insert("background-opacity".into(), value.get().to_string());
        }
        if let Some(value) = terminal.background_blur() {
            model.insert("background-blur".into(), value.get().to_string());
        }
        if let Some(value) = terminal.cursor_style() {
            model.insert("cursor-style".into(), value.as_str().into());
        }
    }
    model
}

fn parse_projection(text: &str) -> Option<BTreeMap<String, String>> {
    let mut model = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = line.split_once('=')?;
        let key = key.trim();
        let value = value.trim();
        let (key, value) = match key {
            "background-image" => (
                key.to_owned(),
                if value.is_empty() {
                    String::new()
                } else {
                    normalize_path(Path::new(value.trim_matches('"')))
                        .display()
                        .to_string()
                },
            ),
            "background-image-opacity" | "background-opacity" => {
                (key.to_owned(), fixed_decimal(value, 6)?)
            }
            "font-size" => (key.to_owned(), fixed_decimal(value, 3)?),
            "background-blur" => (key.to_owned(), value.parse::<u8>().ok()?.to_string()),
            "background-image-repeat" => match value {
                "true" | "false" => (key.to_owned(), value.to_owned()),
                _ => return None,
            },
            "palette" => {
                let (index, color) = value.split_once('=')?;
                let index = index.trim().parse::<u8>().ok()?;
                if index > 15 {
                    return None;
                }
                (format!("palette:{index}"), normalize_color(color.trim())?)
            }
            "background"
            | "foreground"
            | "cursor-color"
            | "selection-background"
            | "selection-foreground" => (key.to_owned(), normalize_color(value)?),
            "background-image-fit" | "background-image-position" | "cursor-style" => {
                (key.to_owned(), value.to_owned())
            }
            _ => return None,
        };
        if model.insert(key, value).is_some() {
            return None;
        }
    }
    Some(model)
}

fn normalize_color(value: &str) -> Option<String> {
    let value = value
        .strip_prefix('#')
        .unwrap_or(value)
        .to_ascii_lowercase();
    (value.len() == 6 && value.bytes().all(|byte| byte.is_ascii_hexdigit())).then_some(value)
}

fn fixed_decimal(value: &str, places: usize) -> Option<String> {
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
        || fraction.len() > places
    {
        return None;
    }
    let whole = whole.parse::<u64>().ok()?;
    let mut fraction = fraction.to_owned();
    fraction.extend(std::iter::repeat_n('0', places - fraction.len()));
    let scale = 10u64.checked_pow(u32::try_from(places).ok()?)?;
    whole
        .checked_mul(scale)?
        .checked_add(fraction.parse::<u64>().unwrap_or(0))
        .map(|value| value.to_string())
}

fn atomic_projection(root: &Path, path: &Path, bytes: &[u8]) -> Result<(), RecoveryError> {
    #[cfg(unix)]
    use std::os::unix::fs::OpenOptionsExt;

    let temp = root.join(format!(".tmp-projection-{}", std::process::id()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    let result = (|| {
        let mut file = options
            .open(&temp)
            .map_err(|error| io_error(&temp, error))?;
        file.write_all(bytes)
            .map_err(|error| io_error(&temp, error))?;
        file.sync_all().map_err(|error| io_error(&temp, error))?;
        fs::rename(&temp, path).map_err(|error| io_error(path, error))?;
        // Directory fsync makes atomic replacement durable before any Activation commit.
        File::open(root)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| io_error(root, error))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn remove_projection(root: &Path, path: &Path) -> Result<(), RecoveryError> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(io_error(path, error)),
        Ok(metadata) if metadata.is_dir() => {
            fs::remove_dir(path).map_err(|error| io_error(path, error))?;
        }
        Ok(_) => fs::remove_file(path).map_err(|error| io_error(path, error))?,
    }
    File::open(root)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| io_error(root, error))
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}
