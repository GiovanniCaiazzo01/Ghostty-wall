//! Read-only committed History validation (RFC 0006). Recovery Projection and
//! integration inspection from RFC 0007 are separate operations, not covered here.
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Cursor, Read},
    path::{Path, PathBuf},
    str::FromStr,
};

use serde::{Deserialize, Deserializer, de};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{
    codec::manifest,
    domain::{
        ActivationId, CandidatePath, CandidateSetFingerprint, EnvironmentId, IntentId,
        MAX_ACTIVATION_SEQUENCE, MediaType, ResolutionSeed, Sha256Digest, WallpaperManifest,
    },
};

/// Failure while inspecting local committed History. No files are changed.
#[derive(Debug, Error)]
pub enum HistoryError {
    /// Filesystem failure during read or lock acquisition.
    #[error("cannot inspect {path}: {source}")]
    Io {
        /// Path being accessed.
        path: PathBuf,
        /// Underlying filesystem error.
        #[source]
        source: io::Error,
    },
    /// Invalid committed state or a missing committed dependency.
    #[error("corrupt committed state at {0}")]
    Corrupt(PathBuf),
}
fn io_error(path: &Path, source: io::Error) -> HistoryError {
    HistoryError::Io {
        path: path.to_owned(),
        source,
    }
}
fn corrupt(path: &Path) -> HistoryError {
    HistoryError::Corrupt(path.to_owned())
}

/// A validated Activation's navigational and durable identity.
#[derive(Clone, Debug)]
pub struct Activation {
    id: ActivationId,
    cursor: u64,
    environment_id: EnvironmentId,
    environment: crate::domain::EnvironmentManifest,
}
impl Activation {
    /// Sequence determines event order, not timestamp.
    pub fn sequence(&self) -> u64 {
        self.id.sequence()
    }
    /// History Cursor determines previous navigation.
    pub fn history_cursor(&self) -> u64 {
        self.cursor
    }
    /// Referenced immutable Environment.
    pub fn environment_id(&self) -> EnvironmentId {
        self.environment_id
    }
    /// Validated immutable Environment referenced by this Activation.
    pub fn environment(&self) -> &crate::domain::EnvironmentManifest {
        &self.environment
    }
}
/// Every final record in ascending sequence order, validated against persisted
/// dependencies. Historical Profile/Source observations cannot be re-derived.
#[derive(Debug)]
pub struct History(Vec<Activation>);
impl History {
    /// Latest durable Activation, if any.
    pub fn latest(&self) -> Option<&Activation> {
        self.0.last()
    }
    /// Activation at a particular one-based sequence.
    pub fn at(&self, sequence: u64) -> Option<&Activation> {
        sequence
            .checked_sub(1)
            .and_then(|i| usize::try_from(i).ok())
            .and_then(|i| self.0.get(i))
    }

    /// Predecessor addressed by durable History Cursor, not last event.
    pub fn previous_target(&self) -> Option<&Activation> {
        self.latest()
            .and_then(|current| current.history_cursor().checked_sub(1))
            .and_then(|sequence| self.at(sequence))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    record_schema_version: u64,
    activation_id: String,
    sequence: u64,
    history_cursor: u64,
    activated_at: String,
    environment_id: String,
    cause: Cause,
    #[serde(default, deserialize_with = "optional")]
    profile: Option<Profile>,
    #[serde(default, deserialize_with = "optional")]
    source: Option<Source>,
    #[serde(default, deserialize_with = "optional")]
    selection: Option<Selection>,
    #[serde(default, deserialize_with = "optional")]
    asset: Option<Asset>,
    #[serde(default, deserialize_with = "optional")]
    color_resolution: Option<ColorResolution>,
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
enum Cause {
    Profile {},
    HistoryReplay { activation_id: String },
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Profile {
    id: String,
    schema_version: u64,
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
enum Source {
    Github {
        id: String,
        repository: String,
        #[serde(rename = "ref")]
        reference: SourceRef,
        resolved_commit: String,
        #[serde(default, deserialize_with = "optional")]
        path: Option<String>,
    },
    LocalDirectory {
        id: String,
        configured_path: String,
        resolved_root: String,
    },
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
enum SourceRef {
    Configured { value: String },
    DefaultBranch { value: String },
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
enum Selection {
    Random {
        algorithm: String,
        seed: String,
        candidate_set_fingerprint: String,
        candidate_count: u64,
        selected_index: u64,
        candidate: String,
    },
    Path {
        candidate: String,
    },
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Asset {
    sha256: String,
    media_type: String,
    byte_length: u64,
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
enum ColorResolution {
    Generated {
        algorithm: String,
    },
    Explicit {},
    Theme {
        theme: String,
        content_sha256: String,
    },
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EnvironmentRecord<'a> {
    record_schema_version: u64,
    environment_id: String,
    #[serde(borrow)]
    manifest: &'a serde_json::value::RawValue,
}
fn optional<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)?
        .map(Some)
        .ok_or_else(|| de::Error::custom("null is forbidden"))
}

/// Inspect every final Activation and referenced Environment/Asset under shared
/// state lock. Caller supplies published Managed Root; this does not inspect
/// Projection or Ghostty include, and cannot re-derive historical Profile,
/// Candidate Set, or Plan observations.
pub fn inspect_history(root: &Path) -> Result<History, HistoryError> {
    let root_type = fs::symlink_metadata(root).map_err(|e| io_error(root, e))?;
    if !root_type.is_dir() || root_type.file_type().is_symlink() {
        return Err(corrupt(root));
    }
    let lock = root.join("state.lock");
    let _lock = open_regular(&lock)?;
    #[cfg(not(unix))]
    return Err(io_error(
        &lock,
        io::Error::new(
            io::ErrorKind::Unsupported,
            "shared state locking unavailable",
        ),
    ));
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        let rc = unsafe { libc::flock(_lock.as_raw_fd(), libc::LOCK_SH) };
        if rc != 0 {
            return Err(io_error(&lock, io::Error::last_os_error()));
        }
    }
    let directory = root.join("history/activations");
    for dir in [
        root.to_owned(),
        root.join("history"),
        directory.clone(),
        root.join("environments"),
        root.join("assets"),
        root.join("assets/sha256"),
    ] {
        let meta = fs::symlink_metadata(&dir).map_err(|e| io_error(&dir, e))?;
        if !meta.is_dir() || meta.file_type().is_symlink() {
            return Err(corrupt(&dir));
        }
    }
    let mut records = Vec::new();
    for entry in fs::read_dir(&directory).map_err(|e| io_error(&directory, e))? {
        let entry = entry.map_err(|e| io_error(&directory, e))?;
        let path = entry.path();
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            return Err(corrupt(&path));
        };
        // Only reserved unpublished atomic-writer files are ignored, never malformed final names.
        if name.starts_with(".tmp-") {
            continue;
        }
        let Some(stem) = name.strip_suffix(".json") else {
            return Err(corrupt(&path));
        };
        let id = ActivationId::from_str(stem).map_err(|_| corrupt(&path))?;
        records.push((id, path));
    }
    records.sort_by_key(|(id, _)| id.sequence());
    let mut history: Vec<Activation> = Vec::with_capacity(records.len());
    for (filename_id, path) in records {
        let expected = u64::try_from(history.len())
            .ok()
            .and_then(|n| n.checked_add(1))
            .ok_or_else(|| corrupt(&path))?;
        if filename_id.sequence() != expected {
            return Err(corrupt(&path));
        }
        let bytes = read_bounded(&path, 1024 * 1024)?;
        let record: Record = serde_json::from_slice(&bytes).map_err(|_| corrupt(&path))?;
        let id = ActivationId::from_str(&record.activation_id).map_err(|_| corrupt(&path))?;
        let env_id = EnvironmentId::from_str(&record.environment_id).map_err(|_| corrupt(&path))?;
        if record.record_schema_version != 1
            || id != filename_id
            || record.sequence != expected
            || !(1..=expected).contains(&record.history_cursor)
            || !timestamp_valid(&record.activated_at)
        {
            return Err(corrupt(&path));
        }
        let environment = validate_environment(root, env_id)?;
        let all = record.source.is_some() && record.selection.is_some() && record.asset.is_some();
        let none = record.source.is_none() && record.selection.is_none() && record.asset.is_none();
        match &record.cause {
            Cause::Profile {} => {
                let profile = record.profile.as_ref().ok_or_else(|| corrupt(&path))?;
                if profile.schema_version != 1
                    || profile.id.parse::<IntentId>().is_err()
                    || record.history_cursor != expected
                    || !(all || none)
                    || record.color_resolution.is_some() != environment.colors().is_some()
                {
                    return Err(corrupt(&path));
                }
                if let Some(source) = &record.source {
                    validate_source(source)
                        .then_some(())
                        .ok_or_else(|| corrupt(&path))?;
                }
                if let Some(selection) = &record.selection {
                    validate_selection(selection)
                        .then_some(())
                        .ok_or_else(|| corrupt(&path))?;
                }
                if let Some(asset) = &record.asset {
                    let WallpaperManifest::Image(image) =
                        environment.wallpaper().ok_or_else(|| corrupt(&path))?
                    else {
                        return Err(corrupt(&path));
                    };
                    if asset.sha256.parse::<Sha256Digest>().ok() != Some(image.asset_sha256())
                        || asset.media_type != image.media_type().as_str()
                        || asset.byte_length > MAX_ACTIVATION_SEQUENCE
                    {
                        return Err(corrupt(&path));
                    }
                    validate_asset(
                        root,
                        image.asset_sha256(),
                        image.media_type(),
                        Some(asset.byte_length),
                    )?;
                }
                if !validate_color(record.color_resolution.as_ref(), all, &environment) {
                    return Err(corrupt(&path));
                }
            }
            Cause::HistoryReplay { activation_id } => {
                let target_id = activation_id
                    .parse::<ActivationId>()
                    .map_err(|_| corrupt(&path))?;
                let target = target_id
                    .sequence()
                    .checked_sub(1)
                    .and_then(|i| usize::try_from(i).ok())
                    .and_then(|i| history.get(i))
                    .ok_or_else(|| corrupt(&path))?;
                if record.profile.is_some()
                    || !none
                    || record.color_resolution.is_some()
                    || target.environment_id != env_id
                    || target.cursor != record.history_cursor
                {
                    return Err(corrupt(&path));
                }
            }
        }
        history.push(Activation {
            id,
            cursor: record.history_cursor,
            environment_id: env_id,
            environment,
        });
    }
    Ok(History(history))
}

fn open_regular(path: &Path) -> Result<File, HistoryError> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options.open(path).map_err(|e| {
        #[cfg(unix)]
        if e.raw_os_error() == Some(libc::ELOOP) {
            return corrupt(path);
        }
        io_error(path, e)
    })?;
    if !file.metadata().map_err(|e| io_error(path, e))?.is_file() {
        return Err(corrupt(path));
    }
    Ok(file)
}
fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, HistoryError> {
    let file = open_regular(path)?;
    if file.metadata().map_err(|e| io_error(path, e))?.len() > limit {
        return Err(corrupt(path));
    }
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| io_error(path, e))?;
    if bytes.len() as u64 > limit {
        return Err(corrupt(path));
    }
    Ok(bytes)
}
fn read_required(path: &Path, limit: u64) -> Result<Vec<u8>, HistoryError> {
    read_bounded(path, limit).map_err(|e| match e {
        HistoryError::Io { source, .. } if source.kind() == io::ErrorKind::NotFound => {
            corrupt(path)
        }
        other => other,
    })
}
fn validate_environment(
    root: &Path,
    id: EnvironmentId,
) -> Result<crate::domain::EnvironmentManifest, HistoryError> {
    let path = root.join("environments").join(format!("{id}.json"));
    let bytes = read_required(&path, 1024 * 1024)?;
    let record: EnvironmentRecord<'_> =
        serde_json::from_slice(&bytes).map_err(|_| corrupt(&path))?;
    let manifest =
        manifest::decode(record.manifest.get().as_bytes()).map_err(|_| corrupt(&path))?;
    if record.record_schema_version != 1
        || record.environment_id != id.to_string()
        || manifest::environment_id(&manifest).map_err(|_| corrupt(&path))? != id
    {
        return Err(corrupt(&path));
    }
    if let Some(WallpaperManifest::Image(image)) = manifest.wallpaper() {
        validate_asset(root, image.asset_sha256(), image.media_type(), None)?;
    }
    Ok(manifest)
}
fn validate_asset(
    root: &Path,
    digest: Sha256Digest,
    media: MediaType,
    length: Option<u64>,
) -> Result<(), HistoryError> {
    let hex = digest.to_string();
    let shard = root.join("assets/sha256").join(&hex[..2]);
    let meta = fs::symlink_metadata(&shard).map_err(|e| {
        if e.kind() == io::ErrorKind::NotFound {
            corrupt(&shard)
        } else {
            io_error(&shard, e)
        }
    })?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err(corrupt(&shard));
    }
    let extension = if media.as_str() == "image/png" {
        "png"
    } else {
        "jpg"
    };
    let path = shard.join(format!("{hex}.{extension}"));
    for entry in fs::read_dir(&shard).map_err(|e| io_error(&shard, e))? {
        let entry = entry.map_err(|e| io_error(&shard, e))?;
        let name = entry.file_name();
        if name.to_string_lossy().starts_with(&hex) && entry.path() != path {
            return Err(corrupt(&entry.path()));
        }
    }
    let bytes = read_required(&path, 64 * 1024 * 1024)?;
    let format = if extension == "png" {
        image::ImageFormat::Png
    } else {
        image::ImageFormat::Jpeg
    };
    if length.is_some_and(|n| n != bytes.len() as u64)
        || Sha256::digest(&bytes).as_slice() != digest.as_bytes()
        || image::guess_format(&bytes).ok() != Some(format)
    {
        return Err(corrupt(&path));
    }
    let mut reader = image::ImageReader::with_format(Cursor::new(&bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16_384);
    limits.max_image_height = Some(16_384);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    reader.decode().map_err(|_| corrupt(&path))?;
    Ok(())
}
fn validate_source(source: &Source) -> bool {
    match source {
        Source::Github {
            id,
            repository,
            reference,
            resolved_commit,
            path,
        } => {
            id.parse::<IntentId>().is_ok()
                && repository.split_once('/').is_some_and(|(owner, repo)| {
                    !owner.is_empty() && !repo.is_empty() && !repo.contains('/')
                })
                && match reference {
                    SourceRef::Configured { value } | SourceRef::DefaultBranch { value } => {
                        !value.is_empty()
                    }
                }
                && lower_hex(resolved_commit, 40)
                && path
                    .as_ref()
                    .is_none_or(|p| p.parse::<CandidatePath>().is_ok())
        }
        Source::LocalDirectory {
            id,
            configured_path,
            resolved_root,
        } => {
            id.parse::<IntentId>().is_ok()
                && !configured_path.is_empty()
                && (!configured_path.starts_with('~') || configured_path.starts_with("~/"))
                && Path::new(resolved_root).is_absolute()
                && !Path::new(resolved_root).components().any(|part| {
                    matches!(
                        part,
                        std::path::Component::ParentDir | std::path::Component::CurDir
                    )
                })
        }
    }
}
fn validate_selection(selection: &Selection) -> bool {
    match selection {
        Selection::Path { candidate } => candidate.parse::<CandidatePath>().is_ok(),
        Selection::Random {
            algorithm,
            seed,
            candidate_set_fingerprint,
            candidate_count,
            selected_index,
            candidate,
        } => {
            algorithm == "random-v1"
                && seed.parse::<ResolutionSeed>().is_ok()
                && candidate_set_fingerprint
                    .parse::<CandidateSetFingerprint>()
                    .is_ok()
                && (1..=MAX_ACTIVATION_SEQUENCE).contains(candidate_count)
                && selected_index < candidate_count
                && candidate.parse::<CandidatePath>().is_ok()
        }
    }
}
fn validate_color(
    color: Option<&ColorResolution>,
    has_asset: bool,
    environment: &crate::domain::EnvironmentManifest,
) -> bool {
    match color {
        None => true,
        Some(ColorResolution::Explicit {}) => true,
        Some(ColorResolution::Theme {
            theme,
            content_sha256,
        }) => {
            if theme.is_empty() || !lower_hex(content_sha256, 64) {
                return false;
            }
            let Ok(bytes) = manifest::encode_canonical(environment) else {
                return false;
            };
            let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
                return false;
            };
            let Some(colors) = value.get("colors") else {
                return false;
            };
            let Ok(canonical) = serde_jcs::to_vec(colors) else {
                return false;
            };
            let mut hash = Sha256::new();
            hash.update(b"ghostty-wall.theme-resolution.v1\0");
            hash.update(&canonical);
            Sha256Digest::from_bytes(hash.finalize().into()).to_string() == *content_sha256
        }
        Some(ColorResolution::Generated { algorithm }) => {
            algorithm == "kmeans-v1"
                && has_asset
                && matches!(environment.wallpaper(), Some(WallpaperManifest::Image(_)))
        }
    }
}
fn lower_hex(text: &str, size: usize) -> bool {
    text.len() == size
        && text
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn timestamp_valid(text: &str) -> bool {
    let b = text.as_bytes();
    if b.len() != 27
        || b[4] != b'-'
        || b[7] != b'-'
        || b[10] != b'T'
        || b[13] != b':'
        || b[16] != b':'
        || b[19] != b'.'
        || b[26] != b'Z'
    {
        return false;
    }
    let n = |start: usize, end: usize| -> Option<u32> {
        b[start..end].iter().try_fold(0u32, |acc, c| {
            c.is_ascii_digit().then_some(acc * 10 + u32::from(c - b'0'))
        })
    };
    let (Some(year), Some(month), Some(day), Some(hour), Some(minute), Some(second), Some(_)) = (
        n(0, 4),
        n(5, 7),
        n(8, 10),
        n(11, 13),
        n(14, 16),
        n(17, 19),
        n(20, 26),
    ) else {
        return false;
    };
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => 0,
    };
    year > 0
        && (1..=days).contains(&day)
        && hour < 24
        && minute < 60
        && (second < 60
            || (second == 60
                && hour == 23
                && minute == 59
                && day == days
                && matches!(month, 6 | 12)))
}
