//! Serialized provisional Projection and temporary images (RFC 0009). No Intent or History writes.

use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{
    domain::{EnvironmentId, EnvironmentManifest, ImageWallpaper, MediaType, WallpaperManifest},
    history::{Activation, HistoryError, inspect_history_unlocked},
    recovery::{
        RecoveryError, StateLock, inspect_integration_hook,
        materialize_preview_projection_unlocked, materialize_projection_unlocked,
        reconcile_recovery_state_unlocked, try_exclusive_state_lock,
    },
    runtime::{ReloadAdapter, ReloadOutcome},
};

const MARKER: &str = "preview.session";
const MAX_IMAGE_BYTES: usize = 32 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Marker {
    version: u8,
    sequence: Option<u64>,
    environment_id: Option<String>,
    token: String,
}

/// A preview failed without committing an Activation.
#[derive(Debug, Error)]
pub enum PreviewError {
    /// A different editor owns the state lock.
    #[error("another editor or state writer holds the lock")]
    Busy,
    /// Existing marker is malformed or could not be verified.
    #[error("invalid interrupted preview marker at {0}; inspect before retrying")]
    InvalidMarker(PathBuf),
    /// Filesystem or durability failure; an interrupted marker may remain.
    #[error("preview I/O at {path}: {source}; reconcile before retrying")]
    Io {
        /// Affected managed path.
        path: PathBuf,
        /// Original error.
        #[source]
        source: io::Error,
    },
    /// Committed state is invalid.
    #[error(transparent)]
    History(#[from] HistoryError),
    /// Hook or Projection recovery failed.
    #[error(transparent)]
    Recovery(#[from] RecoveryError),
    /// Draft needs supplied image bytes because it does not use the starting Durable Asset.
    #[error("draft wallpaper differs from the active Durable Asset; supply validated image bytes")]
    AssetUnavailable,
    /// Supplied bytes do not match the draft or cannot be safely decoded.
    #[error(
        "preview image must match the draft digest/media type and be a bounded PNG/JPEG; Projection unchanged"
    )]
    InvalidImage,
    /// Provisional Projection writes are not supported here.
    #[error("live Projection preview currently requires Linux")]
    UnsupportedPlatform,
}

fn error(path: &Path, source: io::Error) -> PreviewError {
    PreviewError::Io {
        path: path.to_owned(),
        source,
    }
}

fn marker_path(root: &Path) -> PathBuf {
    root.join(MARKER)
}

// A shared-lock reader reaches this only after the editor has released its
// exclusive lock. Presence then denotes an interrupted session, not a draft.
pub(crate) fn interrupted_marker(
    root: &Path,
    latest: Option<&Activation>,
) -> Result<bool, RecoveryError> {
    let path = marker_path(root);
    match fs::symlink_metadata(&path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(RecoveryError::Io { path, source: e }),
        Ok(meta) if meta.is_file() && !meta.file_type().is_symlink() => {
            let marker = read_marker(root).map_err(|e| RecoveryError::Io {
                path: path.clone(),
                source: io::Error::other(e),
            })?;
            // A well-formed marker is not proof that it belongs to this History.
            // Never erase evidence of a different starting revision.
            if marker.sequence != latest.map(Activation::sequence)
                || marker.environment_id.as_deref()
                    != latest
                        .map(|activation| activation.environment_id().to_string())
                        .as_deref()
            {
                return Err(RecoveryError::Io {
                    path,
                    source: io::Error::other(
                        "preview marker starting Activation differs from committed History",
                    ),
                });
            }
            Ok(true)
        }
        Ok(_) => Err(RecoveryError::Io {
            path,
            source: io::Error::other("invalid preview marker entry"),
        }),
    }
}

fn read_marker(root: &Path) -> Result<Marker, PreviewError> {
    let path = marker_path(root);
    #[cfg(unix)]
    use std::os::unix::fs::OpenOptionsExt;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK);
    let mut file = options.open(&path).map_err(|e| error(&path, e))?;
    if !file.metadata().map_err(|e| error(&path, e))?.is_file() {
        return Err(PreviewError::InvalidMarker(path));
    }
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(1025)
        .read_to_end(&mut bytes)
        .map_err(|e| error(&path, e))?;
    let marker: Marker =
        serde_json::from_slice(&bytes).map_err(|_| PreviewError::InvalidMarker(path.clone()))?;
    if bytes.len() > 1024
        || marker.version != 1
        || marker.token.len() != 32
        || !marker
            .token
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        || marker.sequence.is_some() != marker.environment_id.is_some()
        || marker.sequence == Some(0)
        || marker
            .environment_id
            .as_ref()
            .is_some_and(|id| id.parse::<EnvironmentId>().is_err())
    {
        return Err(PreviewError::InvalidMarker(path));
    }
    Ok(marker)
}

fn sync_root(root: &Path) -> Result<(), PreviewError> {
    File::open(root)
        .and_then(|f| f.sync_all())
        .map_err(|e| error(root, e))
}

pub(crate) fn clear_interrupted_marker(root: &Path) -> Result<(), RecoveryError> {
    let path = marker_path(root);
    let marker = read_marker(root).map_err(|e| RecoveryError::Io {
        path: path.clone(),
        source: io::Error::other(e),
    })?;
    // RFC 0009: committed Projection is already durable. Keep the marker until
    // its image entries are durably removed, so interrupted cleanup is retryable.
    cleanup_images(root, &marker.token)?;
    fs::remove_file(&path)
        .and_then(|()| File::open(root)?.sync_all())
        .map_err(|source| RecoveryError::Io { path, source })
}

fn cleanup_images(root: &Path, token: &str) -> Result<(), RecoveryError> {
    let prefix = format!(".tmp-preview-image-{token}-");
    let marker_temp = format!(".tmp-preview-{token}");
    let io_error = |path, source| RecoveryError::Io { path, source };
    for entry in fs::read_dir(root).map_err(|e| io_error(root.to_owned(), e))? {
        let entry = entry.map_err(|e| io_error(root.to_owned(), e))?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let owned_image = name.strip_prefix(&prefix).is_some_and(|suffix| {
            suffix
                .strip_suffix(".png")
                .or_else(|| suffix.strip_suffix(".jpg"))
                .is_some_and(|digest| {
                    digest.len() == 64
                        && digest
                            .bytes()
                            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                })
        });
        if owned_image || name == marker_temp {
            // unlink never follows a substituted symlink and fails on directories.
            fs::remove_file(entry.path()).map_err(|e| io_error(entry.path(), e))?;
        }
    }
    File::open(root)
        .and_then(|f| f.sync_all())
        .map_err(|e| io_error(root.to_owned(), e))
}

#[cfg(target_os = "linux")]
fn publish_marker(root: &Path, marker: &Marker) -> Result<(), PreviewError> {
    let temp = root.join(format!(".tmp-preview-{}", marker.token));
    let path = marker_path(root);
    let bytes = serde_json::to_vec(marker).map_err(|e| error(&path, io::Error::other(e)))?;
    let mut temp_created = false;
    use std::os::unix::fs::OpenOptionsExt;
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&temp)
            .map_err(|e| error(&temp, e))?;
        temp_created = true;
        file.write_all(&bytes)
            .and_then(|()| file.sync_all())
            .map_err(|e| error(&temp, e))?;
        fs::hard_link(&temp, &path).map_err(|e| error(&path, e))?;
        sync_root(root)?;
        Ok(())
    })();
    if temp_created {
        let _ = fs::remove_file(&temp);
    }
    result
}

/// Exclusive live draft session. Dropping attempts restoration; crash recovery
/// must still be run by the next writer if that attempt fails or is interrupted.
pub struct PreviewSession<R: ReloadAdapter> {
    root: PathBuf,
    lock: Option<StateLock>,
    reload: R,
    token: String,
    starting: Option<Activation>,
}

impl<R: ReloadAdapter> PreviewSession<R> {
    /// Open an editor only on a validated initialized installation. A busy
    /// second editor fails rather than waiting on an interactive first editor.
    pub fn begin(
        root: &Path,
        effective_root_config: &Path,
        reload: R,
    ) -> Result<Self, PreviewError> {
        if !cfg!(target_os = "linux") {
            return Err(PreviewError::UnsupportedPlatform);
        }
        let lock = try_exclusive_state_lock(&root.join("state.lock"))?.ok_or(PreviewError::Busy)?;
        inspect_integration_hook(effective_root_config, &root.join("current.ghostty"))?;
        reconcile_recovery_state_unlocked(root)?;
        let history = inspect_history_unlocked(root)?;
        let mut random = [0u8; 16];
        #[cfg(target_os = "linux")]
        {
            let mut entropy = File::open("/dev/urandom").map_err(|e| error(root, e))?;
            entropy
                .read_exact(&mut random)
                .map_err(|e| error(root, e))?;
        }
        let token: String = random.iter().map(|b| format!("{b:02x}")).collect();
        let marker = Marker {
            version: 1,
            sequence: history.latest().map(|a| a.sequence()),
            environment_id: history.latest().map(|a| a.environment_id().to_string()),
            token: token.clone(),
        };
        #[cfg(target_os = "linux")]
        publish_marker(root, &marker)?;
        Ok(Self {
            root: root.to_owned(),
            lock: Some(lock),
            reload,
            token,
            starting: history.latest().cloned(),
        })
    }

    /// Validated durable state captured at begin, not the Profile being edited.
    /// None means cancel must restore an absent managed Projection.
    pub fn starting_activation(&self) -> Option<&Activation> {
        self.starting.as_ref()
    }

    /// Publish a validated draft's derived Projection and request one reload.
    /// Images must use the starting Durable Asset; otherwise use `update_with_image`.
    /// Command acceptance is not proof of repaint or window scope.
    pub fn update(&mut self, draft: &EnvironmentManifest) -> Result<ReloadOutcome, PreviewError> {
        self.verify_owner()?;
        if let Some(WallpaperManifest::Image(image)) = draft.wallpaper() {
            let retained = self
                .starting
                .as_ref()
                .and_then(|a| a.environment().wallpaper());
            if !matches!(retained, Some(WallpaperManifest::Image(active)) if active.asset_sha256() == image.asset_sha256() && active.media_type() == image.media_type())
            {
                return Err(PreviewError::AssetUnavailable);
            }
        }
        materialize_projection_unlocked(&self.root, draft)?;
        Ok(self.reload.reload())
    }

    /// Preview a different wallpaper without saving Intent or retaining a Durable Asset.
    /// Bytes are validated before staging in a session-owned temporary file; cancel,
    /// finish, or locked crash recovery removes it after restoring committed Projection.
    /// A reload failure is returned separately from successful Projection publication.
    pub fn update_with_image(
        &mut self,
        draft: &EnvironmentManifest,
        bytes: &[u8],
    ) -> Result<ReloadOutcome, PreviewError> {
        self.verify_owner()?;
        let Some(WallpaperManifest::Image(image)) = draft.wallpaper() else {
            return Err(PreviewError::InvalidImage);
        };
        validate_image(image, bytes)?;
        let path = self.stage_image(image, bytes)?;
        materialize_preview_projection_unlocked(&self.root, draft, &path)?;
        Ok(self.reload.reload())
    }

    fn stage_image(&self, image: &ImageWallpaper, bytes: &[u8]) -> Result<PathBuf, PreviewError> {
        let extension = match image.media_type() {
            MediaType::Png => "png",
            MediaType::Jpeg => "jpg",
        };
        let path = self.root.join(format!(
            ".tmp-preview-image-{}-{}.{extension}",
            self.token,
            image.asset_sha256()
        ));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
        }
        match options.open(&path) {
            Ok(mut file) => {
                // No Projection references this digest-named file until both fsyncs
                // succeed. Leave partial writes for token-owned recovery on failure.
                file.write_all(bytes)
                    .and_then(|()| file.sync_all())
                    .map_err(|e| error(&path, e))?;
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                let mut options = OpenOptions::new();
                options.read(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK);
                }
                let mut file = options.open(&path).map_err(|e| error(&path, e))?;
                let meta = file.metadata().map_err(|e| error(&path, e))?;
                if !meta.is_file() || meta.len() != bytes.len() as u64 {
                    return Err(error(
                        &path,
                        io::Error::other("staged preview image changed"),
                    ));
                }
                let mut actual = Vec::new();
                Read::by_ref(&mut file)
                    .take(MAX_IMAGE_BYTES as u64 + 1)
                    .read_to_end(&mut actual)
                    .map_err(|e| error(&path, e))?;
                if actual != bytes {
                    return Err(error(
                        &path,
                        io::Error::other("staged preview image changed"),
                    ));
                }
                file.sync_all().map_err(|e| error(&path, e))?;
            }
            Err(e) => return Err(error(&path, e)),
        }
        sync_root(&self.root)?;
        Ok(path)
    }

    fn verify_owner(&self) -> Result<(), PreviewError> {
        let marker = read_marker(&self.root)?;
        let history = inspect_history_unlocked(&self.root)?;
        let latest = history.latest();
        if marker.token != self.token
            || marker.sequence != self.starting.as_ref().map(Activation::sequence)
            || marker.environment_id.as_deref()
                != self
                    .starting
                    .as_ref()
                    .map(|a| a.environment_id().to_string())
                    .as_deref()
            || marker.sequence != latest.map(Activation::sequence)
            || marker.environment_id.as_deref()
                != latest
                    .map(|activation| activation.environment_id().to_string())
                    .as_deref()
        {
            return Err(PreviewError::InvalidMarker(marker_path(&self.root)));
        }
        Ok(())
    }

    fn restore(&mut self) -> Result<(), PreviewError> {
        self.verify_owner()?;
        reconcile_recovery_state_unlocked(&self.root)?;
        Ok(())
    }

    /// Discard draft Projection, then report restoration reload separately.
    pub fn cancel(mut self) -> Result<ReloadOutcome, PreviewError> {
        self.restore()?;
        self.lock.take();
        Ok(self.reload.reload())
    }

    /// Restore committed Projection and request a reload before the caller's
    /// normal Profile-save and apply workflow. Return the restoration reload
    /// outcome separately from save, apply, and post-commit reload results;
    /// command acceptance is not proof of visible restoration. This is not an
    /// atomic Profile/Activation transaction.
    pub fn finish(mut self) -> Result<ReloadOutcome, PreviewError> {
        self.restore()?;
        self.lock.take();
        Ok(self.reload.reload())
    }
}

fn validate_image(image: &ImageWallpaper, bytes: &[u8]) -> Result<(), PreviewError> {
    if bytes.len() > MAX_IMAGE_BYTES
        || <[u8; 32]>::from(Sha256::digest(bytes)) != *image.asset_sha256().as_bytes()
    {
        return Err(PreviewError::InvalidImage);
    }
    let format = match image.media_type() {
        MediaType::Png => image::ImageFormat::Png,
        MediaType::Jpeg => image::ImageFormat::Jpeg,
    };
    if image::guess_format(bytes).ok() != Some(format) {
        return Err(PreviewError::InvalidImage);
    }
    let reader = || {
        let mut reader = image::ImageReader::with_format(io::Cursor::new(bytes), format);
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(16_384);
        limits.max_image_height = Some(16_384);
        limits.max_alloc = Some(256 * 1024 * 1024);
        reader.limits(limits);
        reader
    };
    let (width, height) = reader()
        .into_dimensions()
        .map_err(|_| PreviewError::InvalidImage)?;
    if u64::from(width) * u64::from(height) > 16_777_216 {
        return Err(PreviewError::InvalidImage);
    }
    reader().decode().map_err(|_| PreviewError::InvalidImage)?;
    Ok(())
}

impl<R: ReloadAdapter> Drop for PreviewSession<R> {
    fn drop(&mut self) {
        if self.lock.is_some() && self.restore().is_ok() {
            // An unwinding editor must request a repaint too. Restoring only
            // the file would leave a visible draft with no recovery marker.
            self.lock.take();
            let _ = self.reload.reload();
        }
    }
}
