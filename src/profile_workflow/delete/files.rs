//! Deletion stays relative to pinned directories, even if their path names are substituted.

use super::*;
use std::ffi::{CString, OsString};

mod isolation;

pub(super) struct Directory {
    root_path: PathBuf,
    root: File,
    profiles: File,
}

pub(super) struct Entry {
    pub name: String,
    file: File,
    pub bytes: Vec<u8>,
}

impl Entry {
    pub fn text(&self) -> Result<&str, WorkflowError> {
        std::str::from_utf8(&self.bytes)
            .map_err(|_| WorkflowError::Invalid("Profile or config is not UTF-8"))
    }

    pub fn same(&self, other: &Self) -> io::Result<bool> {
        Ok(same_file(&self.file, &other.file)? && self.bytes == other.bytes)
    }

    pub fn single_link(&self) -> io::Result<bool> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            Ok(self.file.metadata()?.nlink() == 1)
        }
        #[cfg(not(unix))]
        Ok(false)
    }
}

impl Directory {
    pub fn open(root: &Path) -> Result<Self, WorkflowError> {
        let file = open_directory(root).map_err(|e| io_at(root, e))?;
        let profiles =
            open_at(&file, "profiles", true).map_err(|e| io_at(&root.join("profiles"), e))?;
        Ok(Self {
            root_path: root.to_owned(),
            root: file,
            profiles,
        })
    }

    pub fn path(&self, name: &str) -> PathBuf {
        self.root_path.join("profiles").join(name)
    }

    pub fn validate(&self) -> Result<(), WorkflowError> {
        let current = Self::open(&self.root_path)?;
        let same = same_file(&self.root, &current.root)
            .and_then(|root| Ok(root && same_file(&self.profiles, &current.profiles)?))
            .map_err(|e| io_at(&self.root_path, e))?;
        if !same {
            return Err(WorkflowError::Invalid(
                "managed directory changed; start delete again",
            ));
        }
        Ok(())
    }

    pub fn config(&self) -> Result<Entry, WorkflowError> {
        self.read_at(
            &self.root,
            "config.toml",
            MAX_INTENT,
            &self.root_path.join("config.toml"),
        )
    }

    pub fn read(&self, name: &str, max: u64) -> Result<Entry, WorkflowError> {
        self.read_at(&self.profiles, name, max, &self.path(name))
    }

    fn read_at(
        &self,
        parent: &File,
        name: &str,
        max: u64,
        path: &Path,
    ) -> Result<Entry, WorkflowError> {
        let file = open_at(parent, name, false).map_err(|e| io_at(path, e))?;
        let meta = file.metadata().map_err(|e| io_at(path, e))?;
        if !meta.is_file() || meta.len() > max {
            return Err(WorkflowError::Invalid("file is not a bounded regular file"));
        }
        let mut bytes = Vec::new();
        (&file)
            .take(max + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| io_at(path, e))?;
        if bytes.len() as u64 > max {
            return Err(WorkflowError::Invalid("file exceeds size limit"));
        }
        Ok(Entry {
            name: name.to_owned(),
            file,
            bytes,
        })
    }

    pub fn unchanged(&self, entry: &Entry, max: u64) -> Result<bool, WorkflowError> {
        entry
            .same(&self.read(&entry.name, max)?)
            .map_err(|e| io_at(&self.path(&entry.name), e))
    }

    /// Requires the writer lock. Isolate and verify the moved inode before unlinking;
    /// even a last-moment replacement of the public entry must not inherit consent.
    pub fn remove(&self, entry: &Entry, max: u64) -> Result<(), WorkflowError> {
        self.validate()?;
        if !self.unchanged(entry, max)? {
            return Err(WorkflowError::Invalid("confirmed file changed; retained"));
        }
        isolation::remove(self, entry, max)
    }

    pub fn require_absent(&self, name: &str) -> Result<(), WorkflowError> {
        let path = self.path(name);
        if entry_exists(&self.profiles, name).map_err(|e| io_at(&path, e))? {
            return Err(io_at(
                &path,
                io::Error::other("new entry at confirmed name retained; start delete again"),
            ));
        }
        Ok(())
    }

    pub fn names(&self) -> Result<Vec<OsString>, WorkflowError> {
        directory_names(&self.profiles).map_err(|e| io_at(&self.path(""), e))
    }
}

#[cfg(unix)]
fn same_file(a: &File, b: &File) -> io::Result<bool> {
    use std::os::unix::fs::MetadataExt;
    let a = a.metadata()?;
    let b = b.metadata()?;
    Ok(a.dev() == b.dev() && a.ino() == b.ino())
}

#[cfg(unix)]
fn open_directory(path: &Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
}

#[cfg(unix)]
fn open_at(parent: &File, name: &str, directory: bool) -> io::Result<File> {
    use std::os::fd::{AsRawFd, FromRawFd};
    if name.contains('/') || name == ".." {
        return Err(io::Error::other("expected a single directory entry"));
    }
    let name = CString::new(name)?;
    let flags = libc::O_RDONLY
        | libc::O_NOFOLLOW
        | libc::O_CLOEXEC
        | libc::O_NONBLOCK
        | if directory { libc::O_DIRECTORY } else { 0 };
    let fd = unsafe { libc::openat(parent.as_raw_fd(), name.as_ptr(), flags) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: openat returned a new descriptor; this File is its sole owner.
    Ok(unsafe { File::from_raw_fd(fd) })
}

#[cfg(unix)]
fn unlink_at(parent: &File, name: &str) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    let name = CString::new(name)?;
    if unsafe { libc::unlinkat(parent.as_raw_fd(), name.as_ptr(), 0) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(unix)]
fn entry_exists(parent: &File, name: &str) -> io::Result<bool> {
    use std::{mem::MaybeUninit, os::fd::AsRawFd};
    let name = CString::new(name)?;
    let mut stat = MaybeUninit::<libc::stat>::uninit();
    if unsafe {
        libc::fstatat(
            parent.as_raw_fd(),
            name.as_ptr(),
            stat.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    } == 0
    {
        return Ok(true);
    }
    let error = io::Error::last_os_error();
    if error.kind() == io::ErrorKind::NotFound {
        Ok(false)
    } else {
        Err(error)
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn directory_names(parent: &File) -> io::Result<Vec<OsString>> {
    use std::{
        ffi::CStr,
        os::{
            fd::{AsRawFd, IntoRawFd},
            unix::ffi::OsStringExt,
        },
    };
    struct Stream(*mut libc::DIR);
    impl Drop for Stream {
        fn drop(&mut self) {
            unsafe {
                libc::closedir(self.0);
            }
        }
    }
    // A separate open description gives each proof its own enumeration offset.
    let file = open_at(parent, ".", true)?;
    let stream = unsafe { libc::fdopendir(file.as_raw_fd()) };
    if stream.is_null() {
        return Err(io::Error::last_os_error());
    }
    let _ = file.into_raw_fd(); // fdopendir now owns the descriptor, including on early return.
    let stream = Stream(stream);
    let mut names = Vec::new();
    loop {
        #[cfg(target_os = "linux")]
        let errno = unsafe { libc::__errno_location() };
        #[cfg(target_os = "macos")]
        let errno = unsafe { libc::__error() };
        unsafe {
            *errno = 0;
        }
        let entry = unsafe { libc::readdir(stream.0) };
        if entry.is_null() {
            return if unsafe { *errno } == 0 {
                Ok(names)
            } else {
                Err(io::Error::last_os_error())
            };
        }
        // SAFETY: readdir's NUL-terminated name stays valid until the next call on this stream.
        let name = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) }.to_bytes();
        if name != b"." && name != b".." {
            names.push(OsString::from_vec(name.to_vec()));
        }
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn directory_names(_: &File) -> io::Result<Vec<OsString>> {
    Err(io::Error::other(
        "safe directory enumeration unavailable; images retained",
    ))
}

#[cfg(not(unix))]
fn same_file(_: &File, _: &File) -> io::Result<bool> {
    Err(io::Error::other("safe deletion unavailable"))
}
#[cfg(not(unix))]
fn open_directory(_: &Path) -> io::Result<File> {
    Err(io::Error::other("safe deletion unavailable"))
}
#[cfg(not(unix))]
fn open_at(_: &File, _: &str, _: bool) -> io::Result<File> {
    Err(io::Error::other("safe deletion unavailable"))
}
#[cfg(not(unix))]
fn unlink_at(_: &File, _: &str) -> io::Result<()> {
    Err(io::Error::other("safe deletion unavailable"))
}
#[cfg(not(unix))]
fn entry_exists(_: &File, _: &str) -> io::Result<bool> {
    Err(io::Error::other("safe deletion unavailable"))
}
