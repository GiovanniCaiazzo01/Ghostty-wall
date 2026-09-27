//! RFC 0003 removal: unlink only in invocation-private space, never at a public Profile name.
//! A rename captures the actual entry atomically; checking that captured inode closes the
//! public-name check/unlink race. Interrupted workspaces are retained, not replayed or swept.

use super::*;

pub(super) fn remove(dir: &Directory, entry: &Entry, max: u64) -> Result<(), WorkflowError> {
    let (name, private) = create_workspace(dir)?;
    let workspace = dir.root_path.join(&name);
    let removal = remove_entry(dir, entry, max, &private, &workspace);
    // No recursive Drop cleanup: a failed restoration may have retained an unconfirmed file.
    let cleanup = remove_empty_directory(&dir.root, &name).and_then(|()| dir.root.sync_all());
    match cleanup {
        Ok(()) => {
            removal?;
            // Workspace cleanup and its fsync can also interleave with public publication.
            dir.require_absent(&entry.name)
        }
        Err(error) => Err(io_at(
            &workspace,
            io::Error::other(match removal {
                Ok(()) => format!("file removed but private workspace cleanup incomplete: {error}"),
                Err(removal) => format!("{removal}; private workspace retained: {error}"),
            }),
        )),
    }
}

fn remove_entry(
    dir: &Directory,
    entry: &Entry,
    max: u64,
    private: &File,
    workspace: &Path,
) -> Result<(), WorkflowError> {
    let public_path = dir.path(&entry.name);
    let retained_path = workspace.join(&entry.name);
    dir.root.sync_all().map_err(|e| io_at(workspace, e))?;
    rename_without_replace(&dir.profiles, &entry.name, private, &entry.name)
        .map_err(|e| io_at(&public_path, e))?;

    let unlink = (|| {
        private.sync_all().map_err(|e| io_at(workspace, e))?;
        let moved = dir.read_at(private, &entry.name, max, &retained_path)?;
        if !entry.same(&moved).map_err(|e| io_at(&retained_path, e))? {
            return Err(io_at(
                &public_path,
                io::Error::other("confirmed file changed; retained; start delete again"),
            ));
        }
        unlink_at(private, &entry.name).map_err(|e| io_at(&retained_path, e))
    })();
    if let Err(error) = unlink {
        // A concurrent publisher may have filled the public name after detachment. Never
        // replace that entry to hide a failed deletion; retain both and name the saved path.
        if let Err(restore) =
            rename_without_replace(private, &entry.name, &dir.profiles, &entry.name)
        {
            return Err(io_at(
                &retained_path,
                io::Error::other(format!(
                    "removal stopped ({error}); entry retained here: cannot restore without replacing {}: {restore}",
                    public_path.display()
                )),
            ));
        }
        private
            .sync_all()
            .and_then(|()| dir.profiles.sync_all())
            .map_err(|sync| {
                io_at(
                    &public_path,
                    io::Error::other(format!(
                        "removal stopped ({error}); restored entry visible but durability uncertain: {sync}"
                    )),
                )
            })?;
        return Err(error);
    }

    private.sync_all().map_err(|e| io_at(workspace, e))?;
    dir.profiles
        .sync_all()
        .map_err(|e| io_at(&dir.path(""), e))?;
    Ok(())
}

fn create_workspace(dir: &Directory) -> Result<(String, File), WorkflowError> {
    let mut entropy = File::open("/dev/urandom").map_err(|e| io_at(&dir.root_path, e))?;
    for _ in 0..16 {
        let mut token = [0; 16];
        entropy
            .read_exact(&mut token)
            .map_err(|e| io_at(&dir.root_path, e))?;
        let token: String = token.iter().map(|b| format!("{b:02x}")).collect();
        let name = format!(".tmp-delete-{token}");
        match make_directory(&dir.root, &name) {
            Ok(()) => {
                let private = open_at(&dir.root, &name, true)
                    .map_err(|e| io_at(&dir.root_path.join(&name), e))?;
                return Ok((name, private));
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(io_at(&dir.root_path.join(name), e)),
        }
    }
    Err(io_at(
        &dir.root_path,
        io::Error::other("cannot create private deletion workspace; no entry removed"),
    ))
}

#[cfg(unix)]
fn make_directory(parent: &File, name: &str) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    let name = CString::new(name)?;
    if unsafe { libc::mkdirat(parent.as_raw_fd(), name.as_ptr(), 0o700) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(unix)]
fn remove_empty_directory(parent: &File, name: &str) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    let name = CString::new(name)?;
    if unsafe { libc::unlinkat(parent.as_raw_fd(), name.as_ptr(), libc::AT_REMOVEDIR) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn rename_without_replace(
    from: &File,
    from_name: &str,
    to: &File,
    to_name: &str,
) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    let from_name = CString::new(from_name)?;
    let to_name = CString::new(to_name)?;
    #[cfg(target_os = "linux")]
    let result = unsafe {
        libc::renameat2(
            from.as_raw_fd(),
            from_name.as_ptr(),
            to.as_raw_fd(),
            to_name.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    #[cfg(target_os = "macos")]
    let result = unsafe {
        libc::renameatx_np(
            from.as_raw_fd(),
            from_name.as_ptr(),
            to.as_raw_fd(),
            to_name.as_ptr(),
            libc::RENAME_EXCL,
        )
    };
    if result != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn rename_without_replace(_: &File, _: &str, _: &File, _: &str) -> io::Result<()> {
    Err(io::Error::other("atomic no-replace removal unavailable"))
}
#[cfg(not(unix))]
fn make_directory(_: &File, _: &str) -> io::Result<()> {
    Err(io::Error::other("safe deletion unavailable"))
}
#[cfg(not(unix))]
fn remove_empty_directory(_: &File, _: &str) -> io::Result<()> {
    Err(io::Error::other("safe deletion unavailable"))
}
