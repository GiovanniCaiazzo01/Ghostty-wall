use super::UpdateError;
use std::{
    ffi::CString,
    fs::{self, File},
    io::{self, Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{
            ffi::OsStrExt,
            fs::{MetadataExt, PermissionsExt},
        },
    },
    path::{Component, Path, PathBuf},
    sync::Arc,
};

#[derive(Clone)]
pub(super) struct Snapshot {
    directory: Arc<File>,
    parent: PathBuf,
    name: CString,
    pub(super) bytes: Vec<u8>,
    metadata: fs::Metadata,
}

fn error() -> io::Error {
    io::Error::last_os_error()
}
fn name(path: &Path) -> io::Result<CString> {
    CString::new(path.as_os_str().as_bytes()).map_err(|_| io::Error::other("invalid path"))
}
fn open_at(dir: &File, name: &CString, flags: i32, mode: u32) -> io::Result<File> {
    let fd = unsafe {
        libc::openat(
            dir.as_raw_fd(),
            name.as_ptr(),
            flags | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            mode,
        )
    };
    if fd < 0 {
        Err(error())
    } else {
        Ok(unsafe { File::from_raw_fd(fd) })
    }
}
fn directory(path: &Path) -> io::Result<File> {
    let mut dir = File::open("/")?;
    if !path.is_absolute() {
        return Err(io::Error::other("installation path must be absolute"));
    }
    for component in path.components() {
        match component {
            Component::RootDir => (),
            Component::Normal(part) => {
                dir = open_at(
                    &dir,
                    &name(Path::new(part))?,
                    libc::O_RDONLY | libc::O_DIRECTORY,
                    0,
                )?
            }
            _ => return Err(io::Error::other("unsafe installation path")),
        }
    }
    Ok(dir)
}
fn read(dir: &File, name: &CString, limit: u64) -> io::Result<(Vec<u8>, fs::Metadata)> {
    let file = open_at(dir, name, libc::O_RDONLY | libc::O_NONBLOCK, 0)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > limit {
        return Err(io::Error::other(
            "expected bounded regular installation file",
        ));
    }
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(io::Error::other("installation file too large"));
    }
    Ok((bytes, metadata))
}
impl Snapshot {
    pub(super) fn capture(path: &Path, limit: u64) -> io::Result<Self> {
        let parent = path
            .parent()
            .ok_or_else(|| io::Error::other("missing installation directory"))?
            .to_path_buf();
        let directory = Arc::new(directory(&parent)?);
        let name = name(Path::new(
            path.file_name()
                .ok_or_else(|| io::Error::other("missing filename"))?,
        ))?;
        let (bytes, metadata) = read(&directory, &name, limit)?;
        Ok(Self {
            directory,
            parent,
            name,
            bytes,
            metadata,
        })
    }
    pub(super) fn lock_exclusive(&self) -> io::Result<File> {
        let file = open_at(&self.directory, &self.name, libc::O_RDONLY, 0)?;
        lock(&file)?;
        self.unchanged()?;
        Ok(file)
    }
    fn unchanged(&self) -> io::Result<()> {
        let visible = directory(&self.parent)?.metadata()?;
        let anchored = self.directory.metadata()?;
        if visible.dev() != anchored.dev() || visible.ino() != anchored.ino() {
            return Err(io::Error::other(
                "installation directory changed during update",
            ));
        }
        self.matches_at(&self.name)
    }
    fn matches_at(&self, name: &CString) -> io::Result<()> {
        let (bytes, current) = read(&self.directory, name, self.bytes.len() as u64)?;
        if current.dev() != self.metadata.dev()
            || current.ino() != self.metadata.ino()
            || current.mode() != self.metadata.mode()
            || bytes != self.bytes
        {
            return Err(io::Error::other(
                "installation changed during update; retry after other installers finish",
            ));
        }
        Ok(())
    }
}
fn lock(file: &File) -> io::Result<()> {
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err(io::Error::other(
            "Cargo installation is locked; wait for the other installer and retry",
        ));
    }
    Ok(())
}

struct Stage {
    directory: Arc<File>,
    name: CString,
    keep: bool,
    swapped: bool,
    prepared: Snapshot,
    cargo_lock: Option<File>,
}
impl Stage {
    fn new(snapshot: &Snapshot, bytes: &[u8], mode: u32) -> io::Result<Self> {
        if snapshot.directory.metadata()?.mode() & 0o222 == 0 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "installation directory is read-only",
            ));
        }
        let random = tempfile::NamedTempFile::new()?;
        let name = CString::new(format!(
            ".ghostty-wall-update-{}-{}",
            snapshot.name.to_string_lossy(),
            random.path().file_name().unwrap().to_string_lossy()
        ))
        .unwrap();
        let mut file = open_at(
            &snapshot.directory,
            &name,
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
            0o600,
        )?;
        let mut stage = Self {
            directory: snapshot.directory.clone(),
            name,
            keep: false,
            swapped: false,
            prepared: snapshot.clone(),
            cargo_lock: None,
        };
        file.write_all(bytes)?;
        file.set_permissions(fs::Permissions::from_mode(mode & 0o777))?;
        file.sync_all()?;
        stage.prepared.bytes = bytes.to_vec();
        stage.prepared.metadata = file.metadata()?;
        if snapshot.name.to_bytes() == b".crates.toml" {
            let held = open_at(&stage.directory, &stage.name, libc::O_RDONLY, 0)?;
            lock(&held)?;
            stage.cargo_lock = Some(held);
        }
        stage.directory.sync_all()?;
        Ok(stage)
    }
    fn exchange(&self, destination: &Snapshot) -> io::Result<()> {
        #[cfg(target_os = "linux")]
        let rc = unsafe {
            libc::renameat2(
                self.directory.as_raw_fd(),
                self.name.as_ptr(),
                destination.directory.as_raw_fd(),
                destination.name.as_ptr(),
                libc::RENAME_EXCHANGE,
            )
        };
        #[cfg(target_os = "macos")]
        let rc = unsafe {
            libc::renameatx_np(
                self.directory.as_raw_fd(),
                self.name.as_ptr(),
                destination.directory.as_raw_fd(),
                destination.name.as_ptr(),
                libc::RENAME_SWAP,
            )
        };
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        let rc = {
            return Err(io::Error::other(
                "safe atomic exchange requires Linux or macOS",
            ));
        };
        if rc != 0 {
            return Err(error());
        }
        Ok(())
    }
    fn publish(&mut self, destination: &Snapshot) -> io::Result<()> {
        destination.unchanged()?;
        self.exchange(destination)?;
        self.swapped = true;
        destination.matches_at(&self.name)?;
        self.prepared.unchanged()?;
        destination.directory.sync_all()
    }
    fn rollback(&mut self, destination: &Snapshot) -> io::Result<()> {
        self.prepared.matches_at(&destination.name)?;
        self.exchange(destination)?;
        if let Err(error) = self.prepared.matches_at(&self.name) {
            self.exchange(destination)?;
            return Err(error);
        }
        self.swapped = false;
        destination.directory.sync_all()
    }
}
impl Drop for Stage {
    fn drop(&mut self) {
        if !self.keep {
            unsafe {
                libc::unlinkat(self.directory.as_raw_fd(), self.name.as_ptr(), 0);
            }
        }
    }
}

pub(super) fn publish(exe: &Path, changes: Vec<(Snapshot, Vec<u8>)>) -> Result<(), UpdateError> {
    let failure = |e| UpdateError::Replace(exe.display().to_string(), e);
    let mut staged = Vec::new();
    for (snapshot, bytes) in &changes {
        snapshot.unchanged().map_err(failure)?;
        staged.push(Stage::new(snapshot, bytes, snapshot.metadata.mode()).map_err(failure)?);
    }
    for (snapshot, _) in &changes {
        snapshot.unchanged().map_err(failure)?;
    }
    let result = (|| {
        for index in 0..changes.len() {
            staged[index].publish(&changes[index].0)?;
        }
        for stage in &staged {
            stage.prepared.unchanged()?;
        }
        Ok::<_, io::Error>(())
    })();
    if let Err(cause) = result {
        let mut recovery = Vec::new();
        for index in (0..changes.len()).rev() {
            if staged[index].swapped && staged[index].rollback(&changes[index].0).is_err() {
                staged[index].keep = true;
                recovery.push(
                    changes[index]
                        .0
                        .parent
                        .join(staged[index].name.to_string_lossy().as_ref())
                        .display()
                        .to_string(),
                );
            }
        }
        if !recovery.is_empty() {
            return Err(UpdateError::Recovery(format!(
                "{cause}; inspect installation and retained recovery files at {}",
                recovery.join(", ")
            )));
        }
        return Err(failure(cause));
    }
    Ok(())
}
