//! Managed Root initialization from RFC 0008.

use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};
use thiserror::Error;

const DEFAULT_CONFIG: &str = "schema_version = 1\n\n[sources]\n";
const HOOK_LINE: &str = "config-file = ?";

/// Inputs that choose platform paths. Tests inject these instead of env globals.
#[derive(Clone, Debug)]
pub struct InitPaths {
    /// User home directory.
    pub home: PathBuf,
    /// Optional XDG config home; defaults to `$HOME/.config`.
    pub xdg_config_home: Option<PathBuf>,
}

impl InitPaths {
    fn xdg_config_home(&self) -> PathBuf {
        self.xdg_config_home
            .clone()
            .unwrap_or_else(|| self.home.join(".config"))
    }

    /// Platform Managed Root path from RFC 0008.
    pub fn managed_root(&self) -> PathBuf {
        if cfg!(target_os = "macos") {
            self.mac_ghostty_dir().join("ghostty-wall")
        } else {
            self.xdg_config_home().join("ghostty/ghostty-wall")
        }
    }

    fn mac_ghostty_dir(&self) -> PathBuf {
        self.home
            .join("Library/Application Support/com.mitchellh.ghostty")
    }

    fn ghostty_candidates(&self) -> Vec<PathBuf> {
        let xdg = self.xdg_config_home().join("ghostty");
        let mut candidates = vec![xdg.join("config.ghostty"), xdg.join("config")];
        if cfg!(target_os = "macos") {
            let mac = self.mac_ghostty_dir();
            candidates.extend([mac.join("config.ghostty"), mac.join("config")]);
        }
        candidates
    }

    fn ghostty_root_config(&self) -> PathBuf {
        let candidates = self.ghostty_candidates();
        candidates
            .iter()
            .rev()
            .find(|path| fs::symlink_metadata(path).is_ok())
            .cloned()
            .unwrap_or_else(|| {
                if cfg!(target_os = "macos") {
                    candidates[2].clone()
                } else {
                    candidates[0].clone()
                }
            })
    }
}

/// Result of init preview or execution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InitReport {
    /// Managed Root path.
    pub managed_root: PathBuf,
    /// Ghostty root config chosen for integration hook.
    pub root_config: PathBuf,
    /// Human-readable mutations that would run or did run.
    pub mutations: Vec<String>,
    /// Filesystem capability status.
    pub capabilities: &'static str,
    /// Required filesystem capabilities and their probe status.
    pub capability_probes: Vec<CapabilityProbe>,
    /// Inspection of required and optional managed components.
    pub layout: Vec<LayoutEntry>,
    /// State of semantic Integration Hook in effective Ghostty root.
    pub hook_state: &'static str,
    /// Root config candidates in Ghostty load order.
    pub ghostty_candidates: Vec<PathBuf>,
    /// Observed unsafe layout or integration conditions.
    pub problems: Vec<String>,
    /// Whether known v0 artifacts are present.
    pub legacy_detected: bool,
}

/// Observation of one required filesystem capability.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapabilityProbe {
    /// Capability checked by real init.
    pub name: &'static str,
    /// `verified` or `requires-runtime-probe`.
    pub status: &'static str,
}

/// Read-only observation of one managed component.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayoutEntry {
    /// Expected path.
    pub path: PathBuf,
    /// `valid`, `missing`, or `unsafe`.
    pub status: &'static str,
}

/// Init failure.
#[derive(Debug, Error)]
pub enum InitError {
    /// Filesystem operation failed.
    #[error("filesystem error at {path}: {source}")]
    Io {
        /// Path being accessed.
        path: PathBuf,
        /// Original error.
        source: io::Error,
    },
    /// Sensitive managed component was symlink.
    #[error("managed component must not be a symlink: {0}")]
    Symlink(PathBuf),
    /// Sensitive managed component had wrong file kind.
    #[error("managed component has wrong kind: {0}")]
    WrongKind(PathBuf),
    /// Existing installation requires explicit repair.
    #[error("init --repair required for {0}")]
    RepairRequired(PathBuf),
    /// Existing intent is invalid and must not be replaced.
    #[error("invalid config.toml at {0}")]
    InvalidIntent(PathBuf),
    /// Platform path resolution is not implemented.
    #[error("safe initialization is not implemented for this platform")]
    UnsupportedPlatform,
    /// Hook rename completed, but directory durability could not be confirmed.
    #[error(
        "Integration Hook may already be published at {path}; preserve Managed Root and inspect before retrying: {source}"
    )]
    HookPublicationUncertain {
        /// Edited root config target.
        path: PathBuf,
        /// Failure to fsync containing directory.
        source: io::Error,
    },
    /// Root configuration changed while preparing the hook update.
    #[error("root config changed during init: {0}")]
    RootConfigChanged(PathBuf),
}

/// Previews Managed Root initialization without writing.
pub fn dry_run(paths: &InitPaths) -> Result<InitReport, InitError> {
    if !cfg!(any(target_os = "linux", target_os = "macos")) {
        return Err(InitError::UnsupportedPlatform);
    }
    let logical = paths.managed_root();
    let parent = logical.parent().ok_or(InitError::UnsupportedPlatform)?;
    let managed_root = canonical_missing(parent)?.join("ghostty-wall");
    let root_config = paths.ghostty_root_config();
    inspect(paths, managed_root, root_config, "requires-runtime-probe")
}

/// Creates or verifies Managed Root and installs one semantic Ghostty hook.
pub fn init(paths: &InitPaths) -> Result<InitReport, InitError> {
    init_with_publish(paths, probe_capabilities, sync_managed_root)
}

fn init_with_publish(
    paths: &InitPaths,
    probe: fn(&Path) -> Result<(), InitError>,
    publish: fn(&Path) -> Result<(), InitError>,
) -> Result<InitReport, InitError> {
    if !cfg!(any(target_os = "linux", target_os = "macos")) {
        return Err(InitError::UnsupportedPlatform);
    }
    let logical_root = paths.managed_root();
    let root_config = paths.ghostty_root_config();
    let mut mutations = Vec::new();
    let mut created = Vec::new();
    let parent = match prepare_parent(
        logical_root
            .parent()
            .ok_or(InitError::UnsupportedPlatform)?,
        &mut mutations,
        &mut created,
    ) {
        Ok(parent) => parent,
        Err(error) => {
            rollback(&created);
            return Err(error);
        }
    };
    let managed_root = parent.join("ghostty-wall");
    let first_init = fs::symlink_metadata(&managed_root).is_err();
    if let Err(error) = preflight_layout(&managed_root)
        .and_then(|()| preflight_hook(paths, &root_config, &managed_root.join("current.ghostty")))
    {
        rollback(&created);
        return Err(error);
    }
    let mut _published_lock = if first_init {
        None
    } else {
        Some(lock_state(&managed_root.join("state.lock"))?)
    };
    let result = (|| {
        ensure_dir(&managed_root, &mut mutations, &mut created)?;
        if first_init {
            ensure_file(
                &managed_root.join(".init-in-progress"),
                b"",
                &mut mutations,
                &mut created,
            )?;
        }
        for dir in [
            "profiles",
            "assets",
            "assets/sha256",
            "environments",
            "history",
            "history/activations",
            "cache",
        ] {
            ensure_dir(&managed_root.join(dir), &mut mutations, &mut created)?;
        }
        ensure_default_config(&managed_root, &mut mutations, &mut created)?;
        if first_init {
            probe(&managed_root)?;
        }
        ensure_file(
            &managed_root.join("state.lock"),
            b"",
            &mut mutations,
            &mut created,
        )?;
        if first_init {
            _published_lock = Some(lock_state(&managed_root.join("state.lock"))?);
        }
        publish(&managed_root)?;
        if first_init {
            fs::remove_file(managed_root.join(".init-in-progress")).map_err(|source| {
                InitError::Io {
                    path: managed_root.join(".init-in-progress"),
                    source,
                }
            })?;
        }
        install_hook(
            &root_config,
            &managed_root.join("current.ghostty"),
            &mut mutations,
            &mut created,
        )?;
        Ok(())
    })();
    if let Err(error) = &result
        && !matches!(error, InitError::HookPublicationUncertain { .. })
    {
        rollback(&created);
    }
    result?;
    let mut report = inspect(paths, managed_root, root_config, "verified")?;
    report.mutations = mutations;
    Ok(report)
}

struct Created {
    path: PathBuf,
    is_dir: bool,
    dev: u64,
    ino: u64,
    content_digest: Option<[u8; 32]>,
}

impl Created {
    #[cfg(unix)]
    fn capture(path: &Path, is_dir: bool) -> Result<Self, InitError> {
        use std::os::unix::fs::MetadataExt;
        let metadata = fs::symlink_metadata(path).map_err(|source| InitError::Io {
            path: path.to_owned(),
            source,
        })?;
        if metadata.file_type().is_symlink() || metadata.is_dir() != is_dir {
            return Err(InitError::WrongKind(path.to_owned()));
        }
        let content_digest = if is_dir {
            None
        } else {
            let bytes = fs::read(path).map_err(|source| InitError::Io {
                path: path.to_owned(),
                source,
            })?;
            Some(Sha256::digest(bytes).into())
        };
        Ok(Self {
            path: path.to_owned(),
            is_dir,
            dev: metadata.dev(),
            ino: metadata.ino(),
            content_digest,
        })
    }
    #[cfg(not(unix))]
    fn capture(path: &Path, _is_dir: bool) -> Result<Self, InitError> {
        Err(InitError::WrongKind(path.to_owned()))
    }
}

#[cfg(unix)]
fn lock_state(path: &Path) -> Result<fs::File, InitError> {
    use std::{os::fd::AsRawFd, os::unix::fs::OpenOptionsExt};
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .map_err(|source| InitError::Io {
            path: path.to_owned(),
            source,
        })?;
    let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if result != 0 {
        return Err(InitError::Io {
            path: path.to_owned(),
            source: io::Error::last_os_error(),
        });
    }
    Ok(file)
}

#[cfg(not(unix))]
fn lock_state(_path: &Path) -> Result<fs::File, InitError> {
    Err(InitError::UnsupportedPlatform)
}

#[cfg(unix)]
fn prepare_parent(
    path: &Path,
    mutations: &mut Vec<String>,
    created: &mut Vec<Created>,
) -> Result<PathBuf, InitError> {
    use std::os::unix::fs::DirBuilderExt;
    match fs::symlink_metadata(path) {
        Ok(_) => {
            let canonical = fs::canonicalize(path).map_err(|source| InitError::Io {
                path: path.to_owned(),
                source,
            })?;
            if !canonical.is_dir() {
                return Err(InitError::WrongKind(path.to_owned()));
            }
            Ok(canonical)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let parent = prepare_parent(
                path.parent().ok_or(InitError::UnsupportedPlatform)?,
                mutations,
                created,
            )?;
            let name = path.file_name().ok_or(InitError::UnsupportedPlatform)?;
            let next = parent.join(name);
            fs::DirBuilder::new()
                .mode(0o700)
                .create(&next)
                .map_err(|source| InitError::Io {
                    path: next.clone(),
                    source,
                })?;
            created.push(Created::capture(&next, true)?);
            mutations.push(format!("created directory {}", next.display()));
            Ok(next)
        }
        Err(source) => Err(InitError::Io {
            path: path.to_owned(),
            source,
        }),
    }
}

#[cfg(not(unix))]
fn prepare_parent(
    _path: &Path,
    _mutations: &mut Vec<String>,
    _created: &mut Vec<Created>,
) -> Result<PathBuf, InitError> {
    Err(InitError::UnsupportedPlatform)
}

#[cfg(unix)]
fn rollback(created: &[Created]) {
    use std::os::unix::fs::MetadataExt;
    for item in created.iter().rev() {
        if let Ok(metadata) = fs::symlink_metadata(&item.path)
            && metadata.dev() == item.dev
            && metadata.ino() == item.ino
            && !metadata.file_type().is_symlink()
            && (item.is_dir
                || fs::read(&item.path)
                    .ok()
                    .map(|bytes| <[u8; 32]>::from(Sha256::digest(bytes)))
                    == item.content_digest)
        {
            if item.is_dir {
                let _ = fs::remove_dir(&item.path);
            } else {
                let _ = fs::remove_file(&item.path);
            }
        }
    }
}

#[cfg(not(unix))]
fn rollback(_created: &[Created]) {}

#[cfg(unix)]
fn validate_for_repair(
    path: &Path,
    metadata: &fs::Metadata,
    dir: bool,
    tighten: &mut Vec<PathBuf>,
) -> Result<(), InitError> {
    use std::os::unix::fs::MetadataExt;
    if metadata.file_type().is_symlink() {
        return Err(InitError::Symlink(path.to_owned()));
    }
    if (dir && !metadata.is_dir())
        || (!dir && !metadata.is_file())
        || metadata.uid() != unsafe { libc::geteuid() }
    {
        return Err(InitError::WrongKind(path.to_owned()));
    }
    if metadata.mode() & 0o022 != 0 {
        tighten.push(path.to_owned());
    }
    Ok(())
}

#[cfg(not(unix))]
fn validate_for_repair(
    path: &Path,
    _metadata: &fs::Metadata,
    _dir: bool,
    _tighten: &mut Vec<PathBuf>,
) -> Result<(), InitError> {
    Err(InitError::WrongKind(path.to_owned()))
}

#[cfg(unix)]
fn tighten_permissions(paths: &[PathBuf], mutations: &mut Vec<String>) -> Result<(), InitError> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    for path in paths {
        let metadata = fs::symlink_metadata(path).map_err(|source| InitError::Io {
            path: path.clone(),
            source,
        })?;
        let mut pending = Vec::new();
        validate_for_repair(path, &metadata, metadata.is_dir(), &mut pending)?;
        fs::set_permissions(path, fs::Permissions::from_mode(metadata.mode() & !0o022)).map_err(
            |source| InitError::Io {
                path: path.clone(),
                source,
            },
        )?;
        mutations.push(format!("tightened permissions {}", path.display()));
    }
    Ok(())
}

#[cfg(not(unix))]
fn tighten_permissions(_paths: &[PathBuf], _mutations: &mut Vec<String>) -> Result<(), InitError> {
    Err(InitError::UnsupportedPlatform)
}

/// Repairs only reconstructible layout and semantic Integration Hook drift.
pub fn init_repair(paths: &InitPaths) -> Result<InitReport, InitError> {
    if !cfg!(any(target_os = "linux", target_os = "macos")) {
        return Err(InitError::UnsupportedPlatform);
    }
    let logical = paths.managed_root();
    let parent = logical.parent().ok_or(InitError::UnsupportedPlatform)?;
    let root = fs::canonicalize(parent)
        .map_err(|source| InitError::Io {
            path: parent.to_owned(),
            source,
        })?
        .join("ghostty-wall");
    let config = paths.ghostty_root_config();
    let root_meta = fs::symlink_metadata(&root).map_err(|source| InitError::Io {
        path: root.clone(),
        source,
    })?;
    let mut tighten = Vec::new();
    validate_for_repair(&root, &root_meta, true, &mut tighten)?;
    let marker = root.join(".init-in-progress");
    if let Ok(meta) = fs::symlink_metadata(&marker) {
        validate_for_repair(&marker, &meta, false, &mut tighten)?;
        if fs::symlink_metadata(root.join("state.lock")).is_err() {
            return resume_first_init(paths, &root, &marker);
        }
    }
    for dir in [
        "profiles",
        "assets",
        "assets/sha256",
        "environments",
        "history",
        "history/activations",
    ] {
        let path = root.join(dir);
        let metadata =
            fs::symlink_metadata(&path).map_err(|_| InitError::RepairRequired(path.clone()))?;
        validate_for_repair(&path, &metadata, true, &mut tighten)?;
    }
    let intent = root.join("config.toml");
    let metadata =
        fs::symlink_metadata(&intent).map_err(|_| InitError::RepairRequired(intent.clone()))?;
    validate_for_repair(&intent, &metadata, false, &mut tighten)?;
    let text = fs::read_to_string(&intent).map_err(|source| InitError::Io {
        path: intent.clone(),
        source,
    })?;
    crate::codec::intent::parse_config_toml(&text).map_err(|_| InitError::InvalidIntent(intent))?;
    let lock = root.join("state.lock");
    if let Ok(metadata) = fs::symlink_metadata(&lock) {
        validate_for_repair(&lock, &metadata, false, &mut tighten)?;
    }
    for optional in [root.join("cache"), root.join("current.ghostty")] {
        if let Ok(metadata) = fs::symlink_metadata(&optional) {
            validate_for_repair(
                &optional,
                &metadata,
                optional.ends_with("cache"),
                &mut tighten,
            )?;
        }
    }
    let projection = root.join("current.ghostty");
    let candidates = paths.ghostty_candidates();
    // Validate every target before making any repair mutation.
    for path in &candidates {
        if fs::symlink_metadata(path).is_ok() {
            let target = fs::canonicalize(path).map_err(|source| InitError::Io {
                path: path.clone(),
                source,
            })?;
            let meta = fs::metadata(&target).map_err(|source| InitError::Io {
                path: target.clone(),
                source,
            })?;
            validate_existing(&target, &meta, false)?;
            fs::read_to_string(&target).map_err(|source| InitError::Io {
                path: target,
                source,
            })?;
        }
    }
    let mut _published_lock = if fs::symlink_metadata(&lock).is_ok() {
        Some(lock_state(&lock)?)
    } else {
        None
    };
    let mut mutations = Vec::new();
    let mut created = Vec::new();
    tighten_permissions(&tighten, &mut mutations)?;
    if fs::symlink_metadata(root.join("cache")).is_err() {
        ensure_dir(&root.join("cache"), &mut mutations, &mut created)?;
    }
    if fs::symlink_metadata(&lock).is_err() {
        probe_capabilities(&root)?;
        ensure_file(&lock, b"", &mut mutations, &mut created)?;
        _published_lock = Some(lock_state(&lock)?);
        fs::File::open(&root)
            .and_then(|dir| dir.sync_all())
            .map_err(|source| InitError::Io {
                path: root.clone(),
                source,
            })?;
    }
    if fs::symlink_metadata(root.join(".init-in-progress")).is_ok() {
        fs::remove_file(root.join(".init-in-progress")).map_err(|source| InitError::Io {
            path: root.clone(),
            source,
        })?;
    }
    let effective = &config;
    for path in &candidates {
        if let Ok(target) = fs::canonicalize(path) {
            let original = fs::read_to_string(&target).map_err(|source| InitError::Io {
                path: target.clone(),
                source,
            })?;
            let kept: Vec<_> = original
                .lines()
                .filter(|line| hook_count(&format!("{line}\n"), &target, &projection) == 0)
                .collect();
            let mut revised = kept.join("\n");
            if !revised.is_empty() {
                revised.push('\n');
            }
            if path == effective {
                revised.push_str(&format!("{HOOK_LINE}{}\n", projection.display()));
            }
            if original != revised {
                atomic_edit(path, &target, revised.as_bytes())?;
                mutations.push(format!("normalized hook {}", path.display()));
            }
        }
    }
    if fs::symlink_metadata(effective).is_err() {
        install_hook(effective, &projection, &mut mutations, &mut created)?;
    }
    let mut report = inspect(paths, root, config, "verified")?;
    report.mutations = mutations;
    Ok(report)
}

fn resume_first_init(
    paths: &InitPaths,
    root: &Path,
    marker: &Path,
) -> Result<InitReport, InitError> {
    let mut _published_lock = None;
    let mut mutations = Vec::new();
    let mut created = Vec::new();
    for path in [
        root.join("profiles"),
        root.join("assets"),
        root.join("assets/sha256"),
        root.join("environments"),
        root.join("history"),
        root.join("history/activations"),
    ] {
        match fs::symlink_metadata(&path) {
            Ok(meta) => {
                validate_existing(&path, &meta, true)?;
                if fs::read_dir(&path)
                    .map_err(|source| InitError::Io {
                        path: path.clone(),
                        source,
                    })?
                    .next()
                    .is_some()
                {
                    // Parent directories may contain only another required structural directory.
                    let allowed = path == root.join("assets") || path == root.join("history");
                    if !allowed
                        || fs::read_dir(&path)
                            .map_err(|source| InitError::Io {
                                path: path.clone(),
                                source,
                            })?
                            .any(|entry| {
                                let Ok(entry) = entry else {
                                    return true;
                                };
                                !((path == root.join("assets") && entry.file_name() == "sha256")
                                    || (path == root.join("history")
                                        && entry.file_name() == "activations"))
                            })
                    {
                        return Err(InitError::RepairRequired(path));
                    }
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => (),
            Err(source) => return Err(InitError::Io { path, source }),
        }
    }
    for optional in [root.join("cache"), root.join("current.ghostty")] {
        if let Ok(meta) = fs::symlink_metadata(&optional) {
            validate_existing(&optional, &meta, optional.ends_with("cache"))?;
            if optional.ends_with("current.ghostty") {
                return Err(InitError::RepairRequired(optional));
            }
        }
    }
    let config = root.join("config.toml");
    if let Ok(meta) = fs::symlink_metadata(&config) {
        validate_existing(&config, &meta, false)?;
        let text = fs::read_to_string(&config).map_err(|source| InitError::Io {
            path: config.clone(),
            source,
        })?;
        crate::codec::intent::parse_config_toml(&text)
            .map_err(|_| InitError::InvalidIntent(config))?;
    }
    preflight_hook(
        paths,
        &paths.ghostty_root_config(),
        &root.join("current.ghostty"),
    )?;
    let stale_config_temp = root.join(".config.toml-in-progress");
    if let Ok(meta) = fs::symlink_metadata(&stale_config_temp) {
        validate_existing(&stale_config_temp, &meta, false)?;
        fs::remove_file(&stale_config_temp).map_err(|source| InitError::Io {
            path: stale_config_temp.clone(),
            source,
        })?;
        sync_managed_root(root)?;
    }
    let result = (|| {
        for dir in [
            "profiles",
            "assets",
            "assets/sha256",
            "environments",
            "history",
            "history/activations",
            "cache",
        ] {
            ensure_dir(&root.join(dir), &mut mutations, &mut created)?;
        }
        ensure_default_config(root, &mut mutations, &mut created)?;
        probe_capabilities(root)?;
        ensure_file(&root.join("state.lock"), b"", &mut mutations, &mut created)?;
        _published_lock = Some(lock_state(&root.join("state.lock"))?);
        fs::File::open(root)
            .and_then(|dir| dir.sync_all())
            .map_err(|source| InitError::Io {
                path: root.to_owned(),
                source,
            })?;
        Ok(())
    })();
    if result.is_err() {
        rollback(&created);
    }
    result?;
    fs::remove_file(marker).map_err(|source| InitError::Io {
        path: marker.to_owned(),
        source,
    })?;
    install_hook(
        &paths.ghostty_root_config(),
        &root.join("current.ghostty"),
        &mut mutations,
        &mut created,
    )?;
    let mut report = inspect(
        paths,
        root.to_owned(),
        paths.ghostty_root_config(),
        "verified",
    )?;
    report.mutations = mutations;
    Ok(report)
}

fn canonical_missing(path: &Path) -> Result<PathBuf, InitError> {
    match fs::canonicalize(path) {
        Ok(canonical) => Ok(canonical),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            if fs::symlink_metadata(path).is_ok() {
                return Err(InitError::WrongKind(path.to_owned()));
            }
            let parent = path.parent().ok_or(InitError::UnsupportedPlatform)?;
            let name = path.file_name().ok_or(InitError::UnsupportedPlatform)?;
            Ok(canonical_missing(parent)?.join(name))
        }
        Err(source) => Err(InitError::Io {
            path: path.to_owned(),
            source,
        }),
    }
}

fn inspect(
    paths: &InitPaths,
    managed_root: PathBuf,
    root_config: PathBuf,
    capabilities: &'static str,
) -> Result<InitReport, InitError> {
    let mut problems = Vec::new();
    let mut layout = Vec::new();
    let root_published = fs::symlink_metadata(managed_root.join("state.lock")).is_ok();
    for path in required_paths(&managed_root)
        .into_iter()
        .chain([managed_root.join("current.ghostty")])
    {
        let dir = path == managed_root
            || [
                "profiles",
                "assets",
                "assets/sha256",
                "environments",
                "history",
                "history/activations",
                "cache",
            ]
            .iter()
            .any(|suffix| path == managed_root.join(suffix));
        let status = match fs::symlink_metadata(&path) {
            Ok(metadata) => match validate_existing(&path, &metadata, dir) {
                Ok(()) => "valid",
                Err(error) => {
                    problems.push(error.to_string());
                    "unsafe"
                }
            },
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                if root_published && !path.ends_with("cache") && !path.ends_with("current.ghostty")
                {
                    problems.push(format!(
                        "missing published component {}: init --repair required",
                        path.display()
                    ));
                }
                "missing"
            }
            Err(source) => {
                problems.push(format!("{}: {source}", path.display()));
                "unsafe"
            }
        };
        layout.push(LayoutEntry { path, status });
    }
    if let Ok(text) = fs::read_to_string(managed_root.join("config.toml"))
        && let Err(error) = crate::codec::intent::parse_config_toml(&text)
    {
        problems.push(format!("invalid config.toml: {error}"));
    }
    let ghostty = paths.xdg_config_home().join("ghostty");
    let candidates = paths.ghostty_candidates();
    let projection = managed_root.join("current.ghostty");
    let hook_state = match fs::canonicalize(&root_config) {
        Ok(target) => match fs::metadata(&target).and_then(|metadata| {
            validate_existing(&target, &metadata, false)
                .map_err(|error| io::Error::other(error.to_string()))
        }) {
            Err(error) => {
                problems.push(format!("{}: {error}", target.display()));
                "unsafe"
            }
            Ok(()) => match fs::read_to_string(&target) {
                Ok(text) => match hook_count(&text, &target, &projection) {
                    0 => "missing",
                    1 => "present",
                    _ => {
                        problems
                            .push("duplicate Integration Hooks: init --repair required".to_owned());
                        "duplicate"
                    }
                },
                Err(error) => {
                    problems.push(format!("{}: {error}", target.display()));
                    "unsafe"
                }
            },
        },
        Err(error) if error.kind() == io::ErrorKind::NotFound => "missing",
        Err(error) => {
            problems.push(format!("{}: {error}", root_config.display()));
            "unsafe"
        }
    };
    for candidate in &candidates {
        if candidate != &root_config
            && let Ok(target) = fs::canonicalize(candidate)
            && let Ok(text) = fs::read_to_string(&target)
            && hook_count(&text, &target, &projection) != 0
        {
            problems.push(format!(
                "stale Integration Hook at {}: init --repair required",
                candidate.display()
            ));
        }
    }
    let mut mutations = Vec::new();
    if problems.is_empty() {
        for item in &layout {
            if item.status == "missing"
                && !item.path.ends_with("current.ghostty")
                && (!root_published || item.path.ends_with("cache"))
            {
                mutations.push(format!("ensure {}", item.path.display()));
            }
        }
        if hook_state == "missing" {
            mutations.push(format!("install hook {}", root_config.display()));
        }
    }
    Ok(InitReport {
        managed_root,
        root_config,
        mutations,
        capabilities,
        capability_probes: [
            "advisory-locking",
            "file-fsync",
            "directory-fsync",
            "atomic-replacement",
            "atomic-publication-without-replacement",
        ]
        .into_iter()
        .map(|name| CapabilityProbe {
            name,
            status: capabilities,
        })
        .collect(),
        layout,
        hook_state,
        ghostty_candidates: candidates,
        problems,
        legacy_detected: ghostty.join("wallpaper_repos.txt").exists()
            || ghostty.join("wallpaper.conf").exists(),
    })
}

fn required_paths(root: &Path) -> Vec<PathBuf> {
    [
        "",
        "profiles",
        "assets",
        "assets/sha256",
        "environments",
        "history",
        "history/activations",
        "cache",
        "config.toml",
        "state.lock",
    ]
    .into_iter()
    .map(|suffix| root.join(suffix))
    .collect()
}

fn preflight_layout(root: &Path) -> Result<(), InitError> {
    let existing = match fs::symlink_metadata(root) {
        Ok(metadata) => {
            validate_existing(root, &metadata, true)?;
            true
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => false,
        Err(source) => {
            return Err(InitError::Io {
                path: root.to_owned(),
                source,
            });
        }
    };
    if !existing {
        return Ok(());
    }
    for dir in [
        "profiles",
        "assets",
        "assets/sha256",
        "environments",
        "history",
        "history/activations",
        "cache",
    ] {
        let path = root.join(dir);
        match fs::symlink_metadata(&path) {
            Ok(metadata) => validate_existing(&path, &metadata, true)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound && dir == "cache" => (),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Err(InitError::RepairRequired(path));
            }
            Err(source) => return Err(InitError::Io { path, source }),
        }
    }
    for file in ["config.toml", "state.lock", "current.ghostty"] {
        let path = root.join(file);
        match fs::symlink_metadata(&path) {
            Ok(metadata) => validate_existing(&path, &metadata, false)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound && file == "current.ghostty" => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Err(InitError::RepairRequired(path));
            }
            Err(source) => return Err(InitError::Io { path, source }),
        }
    }
    let config = root.join("config.toml");
    let contents = fs::read_to_string(&config).map_err(|source| InitError::Io {
        path: config.clone(),
        source,
    })?;
    crate::codec::intent::parse_config_toml(&contents)
        .map_err(|_| InitError::InvalidIntent(config))?;
    Ok(())
}

#[cfg(unix)]
fn validate_existing(path: &Path, metadata: &fs::Metadata, dir: bool) -> Result<(), InitError> {
    use std::os::unix::fs::MetadataExt;
    if metadata.file_type().is_symlink() {
        return Err(InitError::Symlink(path.to_owned()));
    }
    if (dir && !metadata.is_dir())
        || (!dir && !metadata.is_file())
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o022 != 0
    {
        return Err(InitError::WrongKind(path.to_owned()));
    }
    Ok(())
}

#[cfg(not(unix))]
fn validate_existing(path: &Path, _metadata: &fs::Metadata, _dir: bool) -> Result<(), InitError> {
    Err(InitError::WrongKind(path.to_owned()))
}

fn ensure_dir(
    path: &Path,
    mutations: &mut Vec<String>,
    created: &mut Vec<Created>,
) -> Result<(), InitError> {
    if let Ok(metadata) = fs::symlink_metadata(path) {
        validate_existing(path, &metadata, true)?;
        return Ok(());
    }
    create_private_dir(path)?;
    created.push(Created::capture(path, true)?);
    mutations.push(format!("created directory {}", path.display()));
    Ok(())
}

#[cfg(unix)]
fn ensure_default_config(
    root: &Path,
    mutations: &mut Vec<String>,
    created: &mut Vec<Created>,
) -> Result<(), InitError> {
    use std::os::unix::fs::OpenOptionsExt;
    let path = root.join("config.toml");
    if let Ok(metadata) = fs::symlink_metadata(&path) {
        validate_existing(&path, &metadata, false)?;
        return Ok(());
    }
    let temp = root.join(".config.toml-in-progress");
    let mut temp_created = false;
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&temp)
            .map_err(|source| InitError::Io {
                path: temp.clone(),
                source,
            })?;
        temp_created = true;
        file.write_all(DEFAULT_CONFIG.as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|source| InitError::Io {
                path: temp.clone(),
                source,
            })?;
        fs::hard_link(&temp, &path).map_err(|source| InitError::Io {
            path: path.clone(),
            source,
        })?;
        created.push(Created::capture(&path, false)?);
        fs::File::open(root)
            .and_then(|dir| dir.sync_all())
            .map_err(|source| InitError::Io {
                path: root.to_owned(),
                source,
            })?;
        mutations.push(format!("created file {}", path.display()));
        Ok(())
    })();
    if temp_created {
        let _ = fs::remove_file(&temp);
    }
    result
}

#[cfg(not(unix))]
fn ensure_default_config(
    root: &Path,
    _mutations: &mut Vec<String>,
    _created: &mut Vec<Created>,
) -> Result<(), InitError> {
    Err(InitError::WrongKind(root.to_owned()))
}

fn ensure_file(
    path: &Path,
    bytes: &[u8],
    mutations: &mut Vec<String>,
    created: &mut Vec<Created>,
) -> Result<(), InitError> {
    if let Ok(metadata) = fs::symlink_metadata(path) {
        validate_existing(path, &metadata, false)?;
        return Ok(());
    }
    let mut file = create_private_file(path)?;
    created.push(Created::capture(path, false)?);
    file.write_all(bytes).map_err(|source| InitError::Io {
        path: path.to_owned(),
        source,
    })?;
    file.sync_all().map_err(|source| InitError::Io {
        path: path.to_owned(),
        source,
    })?;
    mutations.push(format!("created file {}", path.display()));
    Ok(())
}

fn preflight_hook(
    paths: &InitPaths,
    root_config: &Path,
    projection: &Path,
) -> Result<(), InitError> {
    for candidate in paths.ghostty_candidates() {
        let path = candidate.as_path();
        match fs::symlink_metadata(path) {
            Ok(_) => {
                let target = fs::canonicalize(path).map_err(|source| InitError::Io {
                    path: path.to_owned(),
                    source,
                })?;
                let metadata = fs::metadata(&target).map_err(|source| InitError::Io {
                    path: target.clone(),
                    source,
                })?;
                validate_existing(&target, &metadata, false)?;
                let content = fs::read_to_string(&target).map_err(|source| InitError::Io {
                    path: target.clone(),
                    source,
                })?;
                let count = hook_count(&content, &target, projection);
                if count > 1 || (path != root_config && count != 0) {
                    return Err(InitError::RepairRequired(path.to_owned()));
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => (),
            Err(source) => {
                return Err(InitError::Io {
                    path: path.to_owned(),
                    source,
                });
            }
        }
    }
    Ok(())
}

pub(crate) fn hook_count(content: &str, config: &Path, projection: &Path) -> usize {
    content
        .lines()
        .filter(|line| {
            let Some((key, value)) = line.trim().split_once('=') else {
                return false;
            };
            if key.trim() != "config-file" {
                return false;
            }
            let Some(value) = value.trim().strip_prefix('?') else {
                return false;
            };
            let value = value.trim().trim_matches('"');
            let path = Path::new(value);
            let resolved = if path.is_absolute() {
                path.to_owned()
            } else {
                config.parent().unwrap_or(Path::new("/")).join(path)
            };
            normalize(&resolved) == normalize(projection)
        })
        .count()
}

fn normalize(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut clean = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => (),
            Component::ParentDir => {
                clean.pop();
            }
            _ => clean.push(part.as_os_str()),
        }
    }
    clean
}

fn install_hook(
    root_config: &Path,
    projection: &Path,
    mutations: &mut Vec<String>,
    created: &mut Vec<Created>,
) -> Result<(), InitError> {
    if let Some(parent) = root_config.parent() {
        fs::create_dir_all(parent).map_err(|source| InitError::Io {
            path: parent.to_owned(),
            source,
        })?;
    }
    if !root_config.exists() {
        ensure_file(root_config, b"", mutations, created)?;
    }

    let target = fs::canonicalize(root_config).map_err(|source| InitError::Io {
        path: root_config.to_owned(),
        source,
    })?;
    let mut content = fs::read_to_string(&target).map_err(|source| InitError::Io {
        path: target.clone(),
        source,
    })?;
    let hook = format!("{HOOK_LINE}{}", projection.display());
    if hook_count(&content, &target, projection) == 1 {
        return Ok(());
    }
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    content.push_str(&hook);
    content.push('\n');
    atomic_edit(root_config, &target, content.as_bytes())?;
    mutations.push(format!("installed hook {}", target.display()));
    Ok(())
}

fn sync_managed_root(root: &Path) -> Result<(), InitError> {
    fs::File::open(root)
        .and_then(|dir| dir.sync_all())
        .map_err(|source| InitError::Io {
            path: root.to_owned(),
            source,
        })
}

#[cfg(unix)]
fn probe_capabilities(root: &Path) -> Result<(), InitError> {
    use std::{
        os::fd::AsRawFd,
        os::unix::fs::OpenOptionsExt,
        sync::atomic::{AtomicU64, Ordering},
    };
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let stem = format!(
        ".ghostty-wall-probe-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    let first = root.join(&stem);
    let published = root.join(format!("{stem}-published"));
    let replaced = root.join(format!("{stem}-replaced"));
    let mut owned = Vec::new();
    let result = (|| {
        let file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&first)
            .map_err(|source| InitError::Io {
                path: first.clone(),
                source,
            })?;
        owned.push(first.clone());
        let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        if rc != 0 {
            return Err(InitError::Io {
                path: first.clone(),
                source: io::Error::last_os_error(),
            });
        }
        file.sync_all().map_err(|source| InitError::Io {
            path: first.clone(),
            source,
        })?;
        fs::hard_link(&first, &published).map_err(|source| InitError::Io {
            path: published.clone(),
            source,
        })?;
        owned.push(published.clone());
        let second = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&replaced)
            .map_err(|source| InitError::Io {
                path: replaced.clone(),
                source,
            })?;
        owned.push(replaced.clone());
        second.sync_all().map_err(|source| InitError::Io {
            path: replaced.clone(),
            source,
        })?;
        fs::rename(&replaced, &published).map_err(|source| InitError::Io {
            path: published.clone(),
            source,
        })?;
        fs::File::open(root)
            .and_then(|directory| directory.sync_all())
            .map_err(|source| InitError::Io {
                path: root.to_owned(),
                source,
            })
    })();
    for path in owned {
        let _ = fs::remove_file(path);
    }
    result
}

#[cfg(not(unix))]
fn probe_capabilities(root: &Path) -> Result<(), InitError> {
    Err(InitError::WrongKind(root.to_owned()))
}

#[cfg(unix)]
fn atomic_edit(root_config: &Path, target: &Path, bytes: &[u8]) -> Result<(), InitError> {
    atomic_edit_with_hooks(
        root_config,
        target,
        bytes,
        || {},
        |directory| fs::File::open(directory).and_then(|dir| dir.sync_all()),
    )
}

#[cfg(unix)]
fn atomic_edit_with_hooks(
    root_config: &Path,
    target: &Path,
    bytes: &[u8],
    before_commit: impl FnOnce(),
    sync_parent: impl FnOnce(&Path) -> io::Result<()>,
) -> Result<(), InitError> {
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let original = fs::metadata(target).map_err(|source| InitError::Io {
        path: target.to_owned(),
        source,
    })?;
    if !original.is_file()
        || original.uid() != unsafe { libc::geteuid() }
        || original.mode() & 0o022 != 0
    {
        return Err(InitError::WrongKind(target.to_owned()));
    }
    let prior_bytes = fs::read(target).map_err(|source| InitError::Io {
        path: target.to_owned(),
        source,
    })?;
    let parent = target
        .parent()
        .ok_or_else(|| InitError::WrongKind(target.to_owned()))?;
    let temp = parent.join(format!(
        ".ghostty-wall-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let mut temp_created = false;
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(original.mode() & 0o777)
            .open(&temp)
            .map_err(|source| InitError::Io {
                path: temp.clone(),
                source,
            })?;
        temp_created = true;
        file.write_all(bytes)
            .and_then(|_| file.sync_all())
            .map_err(|source| InitError::Io {
                path: temp.clone(),
                source,
            })?;
        before_commit();
        let current = fs::canonicalize(root_config).map_err(|source| InitError::Io {
            path: root_config.to_owned(),
            source,
        })?;
        let verified = fs::metadata(&current).map_err(|source| InitError::Io {
            path: current.clone(),
            source,
        })?;
        if current != target || verified.dev() != original.dev() || verified.ino() != original.ino()
        {
            return Err(InitError::WrongKind(root_config.to_owned()));
        }
        if fs::read(&current).map_err(|source| InitError::Io {
            path: current.clone(),
            source,
        })? != prior_bytes
        {
            return Err(InitError::RootConfigChanged(root_config.to_owned()));
        }
        fs::rename(&temp, target).map_err(|source| InitError::Io {
            path: target.to_owned(),
            source,
        })?;
        sync_parent(parent).map_err(|source| InitError::HookPublicationUncertain {
            path: target.to_owned(),
            source,
        })
    })();
    if temp_created {
        let _ = fs::remove_file(&temp);
    }
    result
}

#[cfg(not(unix))]
fn atomic_edit(_root_config: &Path, target: &Path, _bytes: &[u8]) -> Result<(), InitError> {
    Err(InitError::WrongKind(target.to_owned()))
}

#[cfg(unix)]
fn create_private_dir(path: &Path) -> Result<(), InitError> {
    use std::os::unix::fs::DirBuilderExt;
    fs::DirBuilder::new()
        .mode(0o700)
        .create(path)
        .map_err(|source| InitError::Io {
            path: path.to_owned(),
            source,
        })
}

#[cfg(not(unix))]
fn create_private_dir(path: &Path) -> Result<(), InitError> {
    Err(InitError::WrongKind(path.to_owned()))
}

#[cfg(unix)]
fn create_private_file(path: &Path) -> Result<fs::File, InitError> {
    use std::os::unix::fs::OpenOptionsExt;
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|source| InitError::Io {
            path: path.to_owned(),
            source,
        })
}

#[cfg(not(unix))]
fn create_private_file(path: &Path) -> Result<fs::File, InitError> {
    Err(InitError::WrongKind(path.to_owned()))
}

#[cfg(all(test, target_os = "linux"))]
mod failure_tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn paths(label: &str) -> InitPaths {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let home = std::env::temp_dir().join(format!("ghostty-wall-{label}-{suffix}"));
        InitPaths {
            xdg_config_home: Some(home.join("xdg")),
            home,
        }
    }

    fn fail(path: &Path) -> Result<(), InitError> {
        Err(InitError::Io {
            path: path.to_owned(),
            source: io::Error::other("injected failure"),
        })
    }

    #[test]
    fn probe_failure_leaves_no_publication_hook_or_invocation_owned_layout() {
        let paths = paths("probe-failure");
        assert!(init_with_publish(&paths, fail, sync_managed_root).is_err());
        assert!(!paths.managed_root().exists());
        assert!(!paths.xdg_config_home.as_ref().unwrap().exists());
    }

    #[test]
    fn hook_directory_fsync_failure_reports_possible_publication() {
        let paths = paths("hook-fsync-failure");
        fs::create_dir_all(&paths.home).unwrap();
        let target = paths.home.join("config.ghostty");
        fs::write(&target, "old\n").unwrap();
        let error = atomic_edit_with_hooks(
            &target,
            &target,
            b"new\n",
            || {},
            |_| Err(io::Error::other("injected fsync failure")),
        )
        .unwrap_err();
        assert!(matches!(error, InitError::HookPublicationUncertain { .. }));
        assert_eq!(fs::read(&target).unwrap(), b"new\n");
    }

    #[test]
    fn changed_root_config_is_not_overwritten() {
        let paths = paths("hook-concurrent-edit");
        fs::create_dir_all(&paths.home).unwrap();
        let target = paths.home.join("config.ghostty");
        fs::write(&target, "old\n").unwrap();
        let error = atomic_edit_with_hooks(
            &target,
            &target,
            b"new\n",
            || {
                fs::write(&target, "user edit\n").unwrap();
            },
            |_| Ok(()),
        )
        .unwrap_err();
        assert!(matches!(error, InitError::RootConfigChanged(_)));
        assert_eq!(fs::read(&target).unwrap(), b"user edit\n");
    }

    #[test]
    fn directory_fsync_failure_rolls_back_unpublished_layout() {
        let paths = paths("publish-failure");
        assert!(init_with_publish(&paths, probe_capabilities, fail).is_err());
        assert!(!paths.managed_root().exists());
        assert!(!paths.xdg_config_home.as_ref().unwrap().exists());
    }
}
