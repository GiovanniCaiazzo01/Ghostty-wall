//! Shared, non-rendering Profile workflow boundary. No draft touches Projection or History.

use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    str::FromStr,
};

use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{
    apply::ApplyOutcome,
    codec::intent::{IntentTomlError, parse_config_toml, parse_named_profile_toml},
    domain::{
        ColorsManifest, ConfigIntent, IntentId, Sha256Digest, SourceIntent, WallpaperIntent,
        WallpaperSelection,
    },
    history::inspect_history_unlocked,
    init::InitPaths,
    recovery::exclusive_state_lock,
    runtime::ReloadOutcome,
};

mod delete;

const MAX_IMAGE: u64 = 32 * 1024 * 1024;
const MAX_INTENT: u64 = 1024 * 1024;

/// Workflow failure. A publication error can mean that bytes were already published; inspect before retry.
#[derive(Debug, Error)]
pub enum WorkflowError {
    /// Invalid identifier, Source, or Profile data.
    #[error(transparent)]
    Intent(#[from] IntentTomlError),
    /// Invalid identifier.
    #[error(transparent)]
    Id(#[from] crate::domain::ValidationError),
    /// Filesystem access or publication failed.
    #[error("Profile workflow filesystem error at {path}: {source}; inspect files before retrying")]
    Io {
        /// Path whose operation failed.
        path: PathBuf,
        /// Underlying operating-system failure.
        #[source]
        source: io::Error,
    },
    /// Profile changes were not published and any newly published image was rolled back.
    #[error(
        "Profile at {path} was not saved: {source}; pre-save files preserved; fix the error and retry saving"
    )]
    SaveFailed {
        /// Intended Profile path.
        path: PathBuf,
        /// Failure before Profile publication.
        #[source]
        source: Box<WorkflowError>,
    },
    /// Final bytes are visible, but their directory fsync failed; referenced images must remain.
    #[error(
        "Publication at {path} is visible but durability is uncertain: {source}; files retained; inspect before retrying"
    )]
    PublicationUncertain {
        /// Published file whose durability is uncertain.
        path: PathBuf,
        /// Directory sync failure.
        #[source]
        source: io::Error,
    },
    /// Profile publication failed, but its new image could not be safely rolled back.
    #[error(
        "Profile was not saved ({save}); image rollback incomplete at {path}: {source}; files may remain; inspect before retrying"
    )]
    RollbackIncomplete {
        /// Image path requiring inspection.
        path: PathBuf,
        /// Original publication failure.
        save: Box<WorkflowError>,
        /// Cleanup failure or ownership change.
        #[source]
        source: io::Error,
    },
    /// An existing Profile or image would be overwritten.
    #[error("Profile workflow collision at {0}; no files changed")]
    Collision(PathBuf),
    /// User input cannot produce a complete managed Profile.
    #[error("Profile workflow: {0}; no files changed")]
    Invalid(&'static str),
    /// Named Profile does not exist; editing never creates it implicitly.
    #[error(
        "Profile {0} does not exist; no files changed. Run ghostty-wall list or create a new Profile"
    )]
    Missing(IntentId),
    /// State changed since the draft was opened.
    #[error("Profile changed during editing: {0}; no files changed; reopen it")]
    Changed(IntentId),
    /// The low-level inactive-only API requires another Profile to be applied first.
    #[error(
        "Profile {0} is active; no files changed. Apply another Profile (for example Welcome) first"
    )]
    Active(IntentId),
    /// History replay has no authoritative Profile provenance.
    #[error(
        "History replay has no active Profile hint; no files changed. Apply a Profile before deleting {0}"
    )]
    Replay(IntentId),
    /// History cannot be validated; do not remove Intent.
    #[error("cannot validate History before deleting Profile: {0}; no files changed")]
    History(String),
    /// Confirmation is stale; nothing from this deletion has been changed.
    #[error(
        "Profile {0} or current Activation changed since confirmation; no files changed; start delete again"
    )]
    DeletionChanged(IntentId),
    /// Fallback resolution, reconciliation, or publication failed; target Intent is retained.
    #[error(
        "Cannot delete active Profile {id}: Welcome fallback failed: {reason}; Profile and image retained. Fallback may have changed Projection or History; inspect doctor/history before retrying, or Apply another Profile first"
    )]
    Fallback {
        /// Profile whose removal was blocked.
        id: IntentId,
        /// Failure from resolution or durable apply.
        reason: String,
    },
    /// Removal failed or its durability is uncertain, possibly after committing Welcome.
    #[error(
        "Deletion of Profile {id} incomplete: {source}; {fallback}; Profile removal may be visible, image retained; inspect before retrying"
    )]
    DeletionIncomplete {
        /// Target Profile.
        id: IntentId,
        /// Whether fallback committed and its reload outcome.
        fallback: String,
        /// Removal or directory synchronization failure.
        #[source]
        source: Box<WorkflowError>,
    },
    /// Apply did not commit (Profile remains saved).
    #[error("Profile saved but apply failed: {0}")]
    Apply(String),
}

fn io_at(path: &Path, source: io::Error) -> WorkflowError {
    WorkflowError::Io {
        path: path.to_owned(),
        source,
    }
}
fn exists(path: &Path) -> Result<bool, WorkflowError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(io_at(path, e)),
    }
}
fn open_regular(path: &Path, max: u64) -> Result<File, WorkflowError> {
    let mut opts = OpenOptions::new();
    opts.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // A picker path may become a FIFO between listing and open; never wait for a writer.
        opts.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK);
    }
    let file = opts.open(path).map_err(|e| io_at(path, e))?;
    let meta = file.metadata().map_err(|e| io_at(path, e))?;
    if !meta.is_file() || meta.len() > max {
        return Err(WorkflowError::Invalid("file is not a bounded regular file"));
    }
    Ok(file)
}
fn read_regular(path: &Path, max: u64) -> Result<Vec<u8>, WorkflowError> {
    let file = open_regular(path, max)?;
    let mut bytes = Vec::new();
    file.take(max + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| io_at(path, e))?;
    if bytes.len() as u64 > max {
        return Err(WorkflowError::Invalid("file exceeds size limit"));
    }
    Ok(bytes)
}
fn text(path: &Path) -> Result<String, WorkflowError> {
    String::from_utf8(read_regular(path, MAX_INTENT)?)
        .map_err(|_| WorkflowError::Invalid("Profile or config is not UTF-8"))
}
fn image_bytes(path: &Path) -> Result<(Vec<u8>, &'static str), WorkflowError> {
    let bytes = read_regular(path, MAX_IMAGE)?;
    let format = image::guess_format(&bytes)
        .map_err(|_| WorkflowError::Invalid("image must be PNG or JPEG"))?;
    let extension = match format {
        image::ImageFormat::Png => "png",
        image::ImageFormat::Jpeg => "jpg",
        _ => return Err(WorkflowError::Invalid("image must be PNG or JPEG")),
    };
    let mut reader = image::ImageReader::with_format(io::Cursor::new(&bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16_384);
    limits.max_image_height = Some(16_384);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    let decoded = reader
        .decode()
        .map_err(|_| WorkflowError::Invalid("image cannot be decoded safely"))?;
    if u64::from(decoded.width()) * u64::from(decoded.height()) > 16_777_216 {
        return Err(WorkflowError::Invalid("image dimensions exceed limit"));
    }
    Ok((bytes, extension))
}

/// Non-persisted Profile work in progress. Dropping it cancels without writing files.
#[derive(Debug)]
pub struct ProfileDraft {
    id: IntentId,
    original: Option<String>,
    document: String,
    image: Option<(Vec<u8>, &'static str)>,
}
impl ProfileDraft {
    /// Profile identifier.
    pub fn id(&self) -> &IntentId {
        &self.id
    }
    /// Editable TOML backing for advanced clients; draft edits alone never write files or reload Ghostty.
    pub fn document(&self) -> &str {
        &self.document
    }
    /// Saved Intent captured when editing began; None for a new unsaved Profile.
    pub fn saved_document(&self) -> Option<&str> {
        self.original.as_deref()
    }
    /// Validated staged image bytes; saved images are supplied by the caller's resolver.
    pub fn staged_image(&self) -> Option<&[u8]> {
        self.image.as_ref().map(|(bytes, _)| bytes.as_slice())
    }
    /// Complete readable generated colors for the staged image, before any file is saved.
    pub fn generated_colors(&self) -> Result<ColorsManifest, WorkflowError> {
        let (bytes, _) = self
            .image
            .as_ref()
            .ok_or(WorkflowError::Invalid("draft has no staged image"))?;
        crate::palette::generate_kmeans_v3(bytes)
            .map_err(|_| WorkflowError::Invalid("image cannot generate readable colors"))
    }
    /// Replaces draft Intent after strict validation. No disk write occurs.
    pub fn set_document(
        &mut self,
        config: &ConfigIntent,
        document: String,
    ) -> Result<(), WorkflowError> {
        let (_, profile) = parse_named_profile_toml(self.id.as_str(), config, &document)?;
        if let Some((bytes, _)) = &self.image {
            let digest = Sha256Digest::from_bytes(Sha256::digest(bytes).into());
            if !matches!(profile.wallpaper, Some(WallpaperIntent::Source { owned_image: Some(ref claim), .. }) if claim.sha256 == digest)
            {
                return Err(WorkflowError::Invalid(
                    "draft edit would detach staged image",
                ));
            }
        }
        self.document = document;
        Ok(())
    }
}

/// Save happened independently of the later durable Activation and runtime reload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfileOutcome {
    /// Intent saved; activation declined.
    Saved,
    /// Intent saved; Activation committed. Runtime reload is separately reported.
    SavedAndApplied {
        /// Durable Activation committed by the existing apply path.
        activation: crate::domain::ActivationId,
        /// Best-effort reload result, not proof of visible Ghostty change.
        reload: ReloadOutcome,
    },
}

/// Injected paths and parsed Source registry for CLI and TUI workflow callers.
pub struct ProfileWorkflows {
    paths: InitPaths,
    config: ConfigIntent,
}
impl ProfileWorkflows {
    /// Loads existing config; does not initialize an installation or change files.
    pub fn load(paths: InitPaths) -> Result<Self, WorkflowError> {
        let root = paths.managed_root();
        let config = parse_config_toml(&text(&root.join("config.toml"))?)?;
        directory(&root)?;
        Ok(Self { paths, config })
    }
    /// Parsed Source registry for draft validation or UI display.
    pub fn config(&self) -> &ConfigIntent {
        &self.config
    }
    /// Starts a new unsaved draft; an existing id is always a collision, never overwrite.
    pub fn create(&self, name: &str) -> Result<ProfileDraft, WorkflowError> {
        let id = IntentId::from_str(name)?;
        let path = self.profile_path(&id);
        if exists(&path)? {
            return Err(WorkflowError::Collision(path));
        }
        Ok(ProfileDraft {
            id,
            original: None,
            document: "schema_version = 2\n".into(),
            image: None,
        })
    }
    /// Opens an existing Profile draft; save later checks the original bytes under lock.
    pub fn edit(&self, name: &str) -> Result<ProfileDraft, WorkflowError> {
        let id = IntentId::from_str(name)?;
        let path = self.profile_path(&id);
        if !exists(&path)? {
            return Err(WorkflowError::Missing(id));
        }
        let original = text(&path)?;
        parse_named_profile_toml(id.as_str(), &self.config, &original)?;
        Ok(ProfileDraft {
            id,
            document: original.clone(),
            original: Some(original),
            image: None,
        })
    }
    fn check_config(&self, root: &Path) -> Result<(), WorkflowError> {
        if parse_config_toml(&text(&root.join("config.toml"))?)? != self.config {
            return Err(WorkflowError::Invalid(
                "Source registry changed; reopen the Profile",
            ));
        }
        Ok(())
    }
    fn profile_path(&self, id: &IntentId) -> PathBuf {
        self.paths
            .managed_root()
            .join("profiles")
            .join(format!("{id}.toml"))
    }
    fn source(&self) -> Result<&IntentId, WorkflowError> {
        self.config
            .sources
            .iter()
            .filter(
                |(_, s)| matches!(s, SourceIntent::LocalDirectory { path } if path == "profiles"),
            )
            .min_by_key(|(id, _)| (id.as_str() != "welcome", id.as_str()))
            .map(|(id, _)| id)
            .ok_or(WorkflowError::Invalid(
                "no local Source rooted at profiles; configure one first",
            ))
    }
    /// Stages a decoded PNG/JPEG copy from the selected file; original stays untouched.
    pub fn import_image(&self, draft: &mut ProfileDraft, path: &Path) -> Result<(), WorkflowError> {
        let (bytes, extension) = image_bytes(path)?;
        self.stage_image(draft, bytes, extension, None)
    }
    /// Stages a stable, one-time gradient-v1 PNG with versioned provenance.
    pub fn generate_image(
        &self,
        draft: &mut ProfileDraft,
        seed: Sha256Digest,
    ) -> Result<(), WorkflowError> {
        let raw = seed.as_bytes();
        let image = image::RgbaImage::from_fn(256, 256, |x, y| {
            image::Rgba([
                raw[0].wrapping_add((255 * x / 255) as u8),
                raw[1].wrapping_add((255 * y / 255) as u8),
                raw[2].wrapping_add((255 * (x + y) / 510) as u8),
                255,
            ])
        });
        let mut buffer = io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut buffer, image::ImageFormat::Png)
            .map_err(|_| WorkflowError::Invalid("cannot encode generated image"))?;
        self.stage_image(draft, buffer.into_inner(), "png", Some(seed))
    }
    fn stage_image(
        &self,
        draft: &mut ProfileDraft,
        bytes: Vec<u8>,
        extension: &'static str,
        seed: Option<Sha256Digest>,
    ) -> Result<(), WorkflowError> {
        let source = self.source()?;
        crate::palette::generate_kmeans_v3(&bytes)
            .map_err(|_| WorkflowError::Invalid("image cannot generate readable colors"))?;
        let digest = Sha256Digest::from_bytes(Sha256::digest(&bytes).into());
        let generation = seed.map(|seed| format!("\n[wallpaper.generation]\nalgorithm = \"gradient-v1\"\nseed = \"{seed}\"\nwidth = 256\nheight = 256\n")).unwrap_or_default();
        // Replacing an image in an edit must not overwrite the old owned candidate.
        let basename = if draft.original.is_some() {
            format!("{}-{}.{extension}", draft.id, &digest.to_string()[..16])
        } else {
            format!("{}.{extension}", draft.id)
        };
        let wallpaper = format!(
            "schema_version = 2\n\n[wallpaper]\nmode = \"source\"\nsource = \"{source}\"\nselection = \"path\"\npath = \"{basename}\"\nowned_sha256 = \"{digest}\"\nfit = \"cover\"\nposition = \"center\"\nopacity = 0.1\n{generation}\n[colors]\nmode = \"generated\"\n"
        );
        let mut new: toml_edit::DocumentMut = wallpaper
            .parse()
            .map_err(|_| WorkflowError::Invalid("cannot construct wallpaper"))?;
        if draft.original.is_some() {
            let previous: toml_edit::DocumentMut = draft
                .document
                .parse()
                .map_err(|_| WorkflowError::Invalid("invalid draft"))?;
            if let Some(colors) = previous.get("colors") {
                new["colors"] = colors.clone();
            } else {
                new.remove("colors");
            }
            if let Some(wallpaper) = previous.get("wallpaper") {
                for key in ["fit", "position", "opacity", "repeat"] {
                    if let Some(value) = wallpaper.get(key) {
                        new["wallpaper"][key] = value.clone();
                    } else {
                        new["wallpaper"]
                            .as_table_mut()
                            .map(|table| table.remove(key));
                    }
                }
            }
            if let Some(terminal) = previous.get("terminal") {
                new["terminal"] = terminal.clone();
            }
        }
        let document = new.to_string();
        parse_named_profile_toml(draft.id.as_str(), &self.config, &document)?;
        draft.document = document;
        draft.image = Some((bytes, extension));
        Ok(())
    }
    /// Publishes complete Intent (and staged image) under the state lock. Never applies or reloads.
    /// A definitively failed save rolls back only its own unchanged image; uncertain publication
    /// retains the image and requires inspection before retry (RFC 0003).
    pub fn save(&self, draft: ProfileDraft) -> Result<IntentId, WorkflowError> {
        self.save_draft(&draft)
    }
    /// Saves without consuming the editor's draft, so failed saves retain in-memory work.
    /// Finish any preview session first to release its state lock. Publication uncertainty
    /// requires inspection, not a blind retry; save and apply are separate transactions.
    pub fn save_draft(&self, draft: &ProfileDraft) -> Result<IntentId, WorkflowError> {
        parse_named_profile_toml(draft.id.as_str(), &self.config, &draft.document)?;
        if draft.original.is_none() && draft.image.is_none() {
            return Err(WorkflowError::Invalid("new Profile needs a staged image"));
        }
        let root = self.paths.managed_root();
        let dir = directory(&root)?;
        let _lock = exclusive_state_lock(&root.join("state.lock"))
            .map_err(|e| WorkflowError::History(e.to_string()))?;
        self.check_config(&root)?;
        let path = self.profile_path(&draft.id);
        match &draft.original {
            Some(original) => {
                if text(&path)? != *original {
                    return Err(WorkflowError::Changed(draft.id.clone()));
                }
            }
            None if exists(&path)? => return Err(WorkflowError::Collision(path)),
            None => (),
        }
        let mut document = draft.document.clone();
        let mut new_image = None;
        if let Some((bytes, _)) = &draft.image {
            let (_, profile) =
                parse_named_profile_toml(draft.id.as_str(), &self.config, &draft.document)?;
            let Some(WallpaperIntent::Source {
                selection: WallpaperSelection::Path(candidate),
                owned_image: Some(claim),
                ..
            }) = profile.wallpaper
            else {
                return Err(WorkflowError::Invalid("staged image claim missing"));
            };
            if claim.sha256 != Sha256Digest::from_bytes(Sha256::digest(bytes).into())
                || candidate.as_str().contains('/')
            {
                return Err(WorkflowError::Invalid("staged image claim changed"));
            }
            let image = dir.join(candidate.as_str());
            if exists(&image)? {
                if read_regular(&image, MAX_IMAGE)? != *bytes {
                    return Err(WorkflowError::Collision(image));
                }
                // Equal bytes allow reuse, not ownership of a preexisting (possibly original) image.
                // RFC 0003 keeps it as an ordinary Source candidate, ineligible for delete cleanup.
                let mut unowned: toml_edit::DocumentMut = document
                    .parse()
                    .map_err(|_| WorkflowError::Invalid("invalid draft"))?;
                let wallpaper = unowned["wallpaper"]
                    .as_table_like_mut()
                    .ok_or(WorkflowError::Invalid("staged wallpaper missing"))?;
                wallpaper.remove("owned_sha256");
                wallpaper.remove("generation");
                document = unowned.to_string();
                parse_named_profile_toml(draft.id.as_str(), &self.config, &document)?;
            } else {
                new_image = Some(publish_new(&image, bytes)?);
            }
        }
        if draft.original.as_deref() == Some(document.as_str()) {
            return Ok(draft.id.clone());
        }
        let publication = if draft.original.is_some() {
            crate::init::atomic_edit(&path, &path, document.as_bytes()).map_err(|e| match e {
                crate::init::InitError::HookPublicationUncertain { path, source } => {
                    WorkflowError::PublicationUncertain { path, source }
                }
                other => io_at(&path, io::Error::other(other)),
            })
        } else {
            publish_new(&path, document.as_bytes()).map(|_| ())
        };
        finish_save(&path, publication, new_image)?;
        Ok(draft.id.clone())
    }
    /// Uses the existing durable apply path supplied by the caller (including remote/theme adapters).
    /// A failed apply never converts an already saved Profile into an unsaved draft.
    pub fn use_saved<E: std::fmt::Display>(
        &self,
        id: &IntentId,
        apply: impl FnOnce(&IntentId) -> Result<ApplyOutcome, E>,
    ) -> Result<ProfileOutcome, WorkflowError> {
        if !exists(&self.profile_path(id))? {
            return Err(WorkflowError::Invalid("Profile is not saved"));
        }
        let outcome = apply(id).map_err(|e| WorkflowError::Apply(e.to_string()))?;
        Ok(ProfileOutcome::SavedAndApplied {
            activation: outcome.activation_id(),
            reload: outcome.reload_outcome(),
        })
    }
    /// Low-level inactive-only deletion for callers that already obtained consent.
    /// CLI/TUI use the confirmed workflow, which also supports locked Welcome fallback.
    pub fn delete(&self, name: &str) -> Result<bool, WorkflowError> {
        let request = match self.prepare_deletion(name) {
            Err(WorkflowError::Missing(_)) => return Ok(false),
            other => other?,
        };
        if request.active {
            return Err(WorkflowError::Active(request.id));
        }
        self.confirm_deletion(
            &request,
            "",
            |_, _| Err("inactive deletion must not resolve Welcome"),
            || Ok::<_, &'static str>(()),
        )?;
        Ok(true)
    }
}
fn directory(root: &Path) -> Result<PathBuf, WorkflowError> {
    let dir = root.join("profiles");
    for path in [root, dir.as_path()] {
        let meta = fs::symlink_metadata(path).map_err(|e| io_at(path, e))?;
        if !meta.is_dir() || meta.file_type().is_symlink() {
            return Err(WorkflowError::Invalid("unsafe managed directory"));
        }
    }
    Ok(dir)
}
// Keep the inode open so rollback cannot mistake a replacement for our unpublished save's image.
#[derive(Debug)]
struct PublishedFile {
    path: PathBuf,
    file: File,
    sha256: [u8; 32],
}

impl PublishedFile {
    // Under the state lock, roll back only the image newly published by this failed save.
    #[cfg(unix)]
    fn rollback(self) -> io::Result<()> {
        use std::os::unix::fs::MetadataExt;
        let original = self.file.metadata()?;
        let unchanged_entry = || -> io::Result<()> {
            let current = fs::symlink_metadata(&self.path)?;
            if !current.is_file()
                || current.dev() != original.dev()
                || current.ino() != original.ino()
            {
                return Err(io::Error::other("image entry changed; retained"));
            }
            Ok(())
        };
        unchanged_entry()?;
        let bytes = read_regular(&self.path, MAX_IMAGE).map_err(io::Error::other)?;
        if <[u8; 32]>::from(Sha256::digest(&bytes)) != self.sha256 {
            return Err(io::Error::other("image bytes changed; retained"));
        }
        unchanged_entry()?;
        fs::remove_file(&self.path)?;
        sync_parent(
            self.path
                .parent()
                .ok_or_else(|| io::Error::other("invalid image path"))?,
        )
    }

    #[cfg(not(unix))]
    fn rollback(self) -> io::Result<()> {
        Err(io::Error::other("safe image rollback unavailable"))
    }
}

fn finish_save(
    path: &Path,
    publication: Result<(), WorkflowError>,
    image: Option<PublishedFile>,
) -> Result<(), WorkflowError> {
    match publication {
        Ok(_) => Ok(()),
        // A visible Profile may reference this image even if its directory fsync failed.
        Err(error @ WorkflowError::PublicationUncertain { .. }) => Err(error),
        Err(error) => {
            if let Some(image) = image {
                let path = image.path.clone();
                if let Err(source) = image.rollback() {
                    return Err(WorkflowError::RollbackIncomplete {
                        path,
                        save: Box::new(error),
                        source,
                    });
                }
            }
            Err(WorkflowError::SaveFailed {
                path: path.to_owned(),
                source: Box::new(error),
            })
        }
    }
}

fn sync_parent(path: &Path) -> io::Result<()> {
    File::open(path).and_then(|file| file.sync_all())
}

fn publish_new(path: &Path, bytes: &[u8]) -> Result<PublishedFile, WorkflowError> {
    publish_new_with_sync(path, bytes, sync_parent)
}

fn publish_new_with_sync(
    path: &Path,
    bytes: &[u8],
    sync: impl FnOnce(&Path) -> io::Result<()>,
) -> Result<PublishedFile, WorkflowError> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let parent = path
        .parent()
        .ok_or(WorkflowError::Invalid("invalid Profile path"))?;
    let tmp = parent.join(format!(
        ".tmp-workflow-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let mut temp_created = false;
    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut f = options.open(&tmp).map_err(|e| io_at(&tmp, e))?;
        temp_created = true;
        f.write_all(bytes)
            .and_then(|_| f.sync_all())
            .map_err(|e| io_at(&tmp, e))?;
        fs::hard_link(&tmp, path).map_err(|e| {
            if e.kind() == io::ErrorKind::AlreadyExists {
                WorkflowError::Collision(path.to_owned())
            } else {
                io_at(path, e)
            }
        })?;
        sync(parent).map_err(|source| WorkflowError::PublicationUncertain {
            path: path.to_owned(),
            source,
        })?;
        Ok(PublishedFile {
            path: path.to_owned(),
            file: f,
            sha256: Sha256::digest(bytes).into(),
        })
    })();
    if temp_created {
        let _ = fs::remove_file(&tmp);
    }
    result
}
#[cfg(all(test, unix))]
mod publication_tests {
    use super::*;

    #[test]
    fn unpublished_profile_rolls_back_new_image_without_removing_collision() {
        let directory = tempfile::tempdir().unwrap();
        let image_path = directory.path().join("personal.png");
        let profile_path = directory.path().join("personal.toml");
        let image = publish_new(&image_path, b"staged image").unwrap();
        fs::write(&profile_path, b"competing Profile").unwrap();
        fs::write(directory.path().join("unrelated.png"), b"existing data").unwrap();

        let error = finish_save(
            &profile_path,
            publish_new(&profile_path, b"new Profile").map(|_| ()),
            Some(image),
        )
        .unwrap_err();
        assert!(matches!(error, WorkflowError::SaveFailed { .. }));
        assert!(error.to_string().contains("pre-save files preserved"));
        assert!(!image_path.exists());
        assert_eq!(fs::read(&profile_path).unwrap(), b"competing Profile");
        assert_eq!(
            fs::read(directory.path().join("unrelated.png")).unwrap(),
            b"existing data"
        );
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 2);
    }

    #[test]
    fn profile_directory_sync_failure_retains_image_and_reports_uncertainty() {
        let directory = tempfile::tempdir().unwrap();
        let image_path = directory.path().join("personal.png");
        let profile_path = directory.path().join("personal.toml");
        let image = publish_new(&image_path, b"staged image").unwrap();
        let result = publish_new_with_sync(&profile_path, b"complete Profile", |_| {
            Err(io::Error::other("injected directory fsync failure"))
        });
        let error = finish_save(&profile_path, result.map(|_| ()), Some(image)).unwrap_err();
        assert!(matches!(error, WorkflowError::PublicationUncertain { .. }));
        assert!(error.to_string().contains("durability is uncertain"));
        assert_eq!(fs::read(&profile_path).unwrap(), b"complete Profile");
        assert_eq!(fs::read(&image_path).unwrap(), b"staged image");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 2);
    }

    #[test]
    fn image_directory_sync_failure_is_not_reported_as_clean_rollback() {
        let directory = tempfile::tempdir().unwrap();
        let image_path = directory.path().join("personal.png");
        let error = publish_new_with_sync(&image_path, b"staged image", |_| {
            Err(io::Error::other("injected directory fsync failure"))
        })
        .unwrap_err();
        assert!(matches!(error, WorkflowError::PublicationUncertain { .. }));
        assert_eq!(fs::read(&image_path).unwrap(), b"staged image");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn rollback_retains_replaced_or_modified_images_and_symlink_targets() {
        for change in ["equal replacement", "different bytes", "symlink"] {
            let directory = tempfile::tempdir().unwrap();
            let image_path = directory.path().join("personal.png");
            let profile_path = directory.path().join("personal.toml");
            let image = publish_new(&image_path, b"staged image").unwrap();
            let unrelated = directory.path().join("original.png");
            fs::write(&unrelated, b"user original").unwrap();
            match change {
                "equal replacement" => {
                    fs::remove_file(&image_path).unwrap();
                    fs::write(&image_path, b"staged image").unwrap();
                }
                "different bytes" => fs::write(&image_path, b"new bytes").unwrap(),
                "symlink" => {
                    fs::remove_file(&image_path).unwrap();
                    std::os::unix::fs::symlink(&unrelated, &image_path).unwrap();
                }
                _ => unreachable!(),
            }
            let before = fs::read(&image_path).unwrap();
            let error = finish_save(
                &profile_path,
                Err(io_at(
                    &profile_path,
                    io::Error::other("Profile write failed"),
                )),
                Some(image),
            )
            .unwrap_err();
            assert!(matches!(error, WorkflowError::RollbackIncomplete { .. }));
            assert!(error.to_string().contains("rollback incomplete"));
            assert!(!error.to_string().contains("pre-save files preserved"));
            assert_eq!(fs::read(&image_path).unwrap(), before, "{change}");
            assert_eq!(fs::read(&unrelated).unwrap(), b"user original");
            assert!(!profile_path.exists());
            assert_eq!(
                fs::symlink_metadata(&image_path)
                    .unwrap()
                    .file_type()
                    .is_symlink(),
                change == "symlink"
            );
        }
    }
}
