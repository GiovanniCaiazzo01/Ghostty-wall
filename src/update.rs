//! Explicit, ownership-checked executable updates; never touches the Managed Root.

mod cargo_install;
#[cfg(unix)]
mod publication;

use std::{
    env, fs,
    io::{self, Read, Write},
    path::{Component, Path},
    time::Duration,
};

use flate2::read::GzDecoder;
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use thiserror::Error;

const REPOSITORY: &str = "GiovanniCaiazzo01/Ghostty-wall";
const ASSET: &str = "ghostty-wall-x86_64-unknown-linux-gnu.tar.gz";
const MAX_ARCHIVE: u64 = 64 * 1024 * 1024;
const MAX_UNPACKED: u64 = 256 * 1024 * 1024;

/// Error from an explicit update request. No credentials appear in messages.
#[derive(Debug, Error)]
pub enum UpdateError {
    /// No official prebuilt artifact exists for this platform.
    #[error(
        "Automatic updates require Linux or macOS. Official prebuilt releases support Linux x86_64; Cargo installations on other supported architectures build from source."
    )]
    Unsupported,
    /// GitHub could not provide release metadata or bytes.
    #[error("GitHub release request failed: {0}")]
    Network(String),
    /// Latest release metadata did not meet the version contract.
    #[error("Invalid latest GitHub Release metadata: {0}")]
    Release(String),
    /// Release checksum was invalid or did not match the archive.
    #[error("Release SHA-256 verification failed: {0}")]
    Checksum(String),
    /// Verified archive did not meet the binary layout contract.
    #[error("Invalid Ghostty Wall release archive: {0}")]
    Archive(String),
    /// Neither release nor Cargo ownership could be established safely.
    #[error(
        "Cannot update {0}: installation ownership could not be verified. Expected a matching release checksum or consistent Cargo .crates.toml and .crates2.json beside bin/. Repair missing/mismatched metadata using the original installer; manual binaries are not overwritten."
    )]
    Ownership(String),
    /// Executable replacement failed without intentionally escalating privilege.
    #[error(
        "Cannot update {0}: {1}. Reinstall Ghostty Wall using the installation method that owns this binary."
    )]
    Replace(String, #[source] io::Error),
    /// A source build could not be prepared; the live installation is unchanged.
    #[error("Preparing source update failed: {0}")]
    Build(String),
    /// Publication and rollback both failed; original files are retained for recovery.
    #[error("Update needs manual recovery: {0}")]
    Recovery(String),
    /// Output could not be written.
    #[error(transparent)]
    Io(#[from] io::Error),
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
}

/// Checks or installs the latest official stable GitHub Release, only when invoked explicitly.
pub fn run(check: bool, output: &mut impl Write) -> Result<(), UpdateError> {
    if !check && !cfg!(any(target_os = "linux", target_os = "macos")) {
        return Err(UpdateError::Unsupported);
    }
    let agent = ureq::Agent::config_builder()
        .https_only(true)
        .http_status_as_error(false)
        .max_redirects(5)
        .timeout_global(Some(Duration::from_secs(120)))
        .build();
    let agent: ureq::Agent = agent.into();
    let token = env::var("GITHUB_TOKEN").ok().filter(|t| !t.is_empty());
    let exe = if check {
        None
    } else {
        Some(env::current_exe()?)
    };
    run_with(
        check,
        env!("CARGO_PKG_VERSION"),
        exe.as_deref(),
        output,
        |url, limit| {
            let mut request = agent
                .get(url)
                .header("User-Agent", "ghostty-wall")
                .header("Accept", "application/vnd.github+json");
            if let Some(token) = &token {
                request = request.header("Authorization", &format!("Bearer {token}"));
            }
            let mut response = request.call().map_err(|_| {
                UpdateError::Network("GitHub is unavailable; check your connection or token".into())
            })?;
            if response.status().as_u16() != 200 {
                return Err(UpdateError::Network(match response.status().as_u16() {
                    404 if url.ends_with("/releases/latest") => {
                        "no latest stable GitHub Release found".into()
                    }
                    404 => "release asset not found".into(),
                    status => format!("GitHub returned HTTP {status} (check connection or token)"),
                }));
            }
            let bytes = response
                .body_mut()
                .with_config()
                .limit(limit + 1)
                .read_to_vec()
                .map_err(|_| UpdateError::Network("could not read GitHub response".into()))?;
            if bytes.len() as u64 > limit {
                return Err(UpdateError::Network(
                    "GitHub response exceeds size limit".into(),
                ));
            }
            Ok(bytes)
        },
    )
}

fn supported(os: &str, arch: &str) -> bool {
    os == "linux" && matches!(arch, "x86_64" | "amd64")
}

fn version(tag: &str) -> Result<Version, UpdateError> {
    let text = tag
        .strip_prefix('v')
        .ok_or_else(|| UpdateError::Release("expected vMAJOR.MINOR.PATCH tag".into()))?;
    let parsed = Version::parse(text)
        .map_err(|_| UpdateError::Release("expected vMAJOR.MINOR.PATCH tag".into()))?;
    if parsed.to_string() != text || !parsed.pre.is_empty() || !parsed.build.is_empty() {
        return Err(UpdateError::Release(
            "expected stable vMAJOR.MINOR.PATCH tag".into(),
        ));
    }
    Ok(parsed)
}

fn run_with(
    check: bool,
    installed: &str,
    exe: Option<&Path>,
    output: &mut impl Write,
    get: impl FnMut(&str, u64) -> Result<Vec<u8>, UpdateError>,
) -> Result<(), UpdateError> {
    run_with_builder(check, installed, exe, output, get, cargo_install::build)
}

fn run_with_builder(
    check: bool,
    installed: &str,
    exe: Option<&Path>,
    output: &mut impl Write,
    mut get: impl FnMut(&str, u64) -> Result<Vec<u8>, UpdateError>,
    build: impl FnOnce(&str, &Path) -> Result<(), UpdateError>,
) -> Result<(), UpdateError> {
    writeln!(output, "Checking latest stable Ghostty Wall release...")?;
    let api = format!("https://api.github.com/repos/{REPOSITORY}/releases/latest");
    let metadata = get(&api, 64 * 1024)?;
    let release: Release = serde_json::from_slice(&metadata)
        .map_err(|_| UpdateError::Release("missing or malformed tag_name".into()))?;
    let latest = version(&release.tag_name)?;
    let current = Version::parse(installed)
        .map_err(|_| UpdateError::Release("compiled package version is invalid".into()))?;
    if check {
        writeln!(
            output,
            "Current version: {current}\nLatest version:  {latest}\n"
        )?;
        if latest > current {
            writeln!(
                output,
                "Update available.\nRun `ghostty-wall update` to install it."
            )?;
        } else {
            writeln!(output, "Ghostty Wall is up to date.")?;
        }
        return Ok(());
    }
    if latest <= current {
        writeln!(output, "Ghostty Wall {current} is already up to date.")?;
        return Ok(());
    }
    let exe = exe.ok_or_else(|| UpdateError::Release("executable path unavailable".into()))?;
    install(exe, installed, &release.tag_name, output, &mut get, build)?;
    writeln!(output, "\nUpdated Ghostty Wall {current} -> {latest}.")?;
    Ok(())
}

#[cfg(unix)]
fn install(
    exe: &Path,
    installed: &str,
    tag: &str,
    output: &mut impl Write,
    get: &mut impl FnMut(&str, u64) -> Result<Vec<u8>, UpdateError>,
    build: impl FnOnce(&str, &Path) -> Result<(), UpdateError>,
) -> Result<(), UpdateError> {
    use publication::Snapshot;
    if !cfg!(any(target_os = "linux", target_os = "macos")) {
        return Err(UpdateError::Unsupported);
    }
    let ownership = |_| UpdateError::Ownership(exe.display().to_string());
    let previous = Snapshot::capture(exe, MAX_UNPACKED).map_err(ownership)?;
    let marker_path = exe.with_file_name(".ghostty-wall-release.sha256");
    let root = exe
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| ownership(io::Error::other("missing prefix")))?;
    let temporary = TempDir::new()?;
    let temporary_root = temporary.path().canonicalize()?;
    let mut changes = Vec::new();
    let binary;
    let _cargo_lock;
    if fs::symlink_metadata(&marker_path).is_ok() {
        _cargo_lock = None;
        let marker = Snapshot::capture(&marker_path, 1024).map_err(ownership)?;
        verify_checksum(&previous.bytes, &marker.bytes, "ghostty-wall")
            .map_err(|_| ownership(io::Error::other("checksum")))?;
        if root.join(".crates.toml").exists() || root.join(".crates2.json").exists() {
            return Err(UpdateError::Ownership(
                "ambiguous release and Cargo ownership".into(),
            ));
        }
        if !supported(env::consts::OS, env::consts::ARCH) {
            return Err(UpdateError::Unsupported);
        }
        writeln!(output, "Preparing release download {tag}...")?;
        let base = format!("https://github.com/{REPOSITORY}/releases/download/{tag}/{ASSET}");
        let archive = get(&base, MAX_ARCHIVE)?;
        let checksum = get(&format!("{base}.sha256"), 1024)?;
        writeln!(output, "Verifying SHA-256 and release archive...")?;
        verify_checksum(&archive, &checksum, ASSET)?;
        binary = extract_binary(&archive, &temporary_root)?;
        let prepared = temporary_root.join("ghostty-wall");
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&prepared, fs::Permissions::from_mode(0o755))?;
        verify_executable(&prepared, &tag[1..])?;
        changes.push((
            marker,
            format!("{}  ghostty-wall\n", sha256_hex(&binary)).into_bytes(),
        ));
    } else {
        if exe.file_name().and_then(|s| s.to_str()) != Some("ghostty-wall")
            || exe
                .parent()
                .and_then(Path::file_name)
                .and_then(|s| s.to_str())
                != Some("bin")
        {
            return Err(UpdateError::Ownership(exe.display().to_string()));
        }
        let toml =
            Snapshot::capture(&root.join(".crates.toml"), 4 * 1024 * 1024).map_err(ownership)?;
        _cargo_lock = Some(
            toml.lock_exclusive()
                .map_err(|e| UpdateError::Replace(exe.display().to_string(), e))?,
        );
        let json =
            Snapshot::capture(&root.join(".crates2.json"), 4 * 1024 * 1024).map_err(ownership)?;
        let record = cargo_install::Record::parse(&toml.bytes, &json.bytes, installed)?;
        writeln!(
            output,
            "Preparing Cargo source build {tag} (requires Rust, native linker and network; this may take several minutes)..."
        )?;
        build(tag, &temporary_root)?;
        writeln!(
            output,
            "Verifying source-build executable and Cargo metadata..."
        )?;
        let built_toml = Snapshot::capture(&temporary_root.join(".crates.toml"), 4 * 1024 * 1024)?;
        let built_json = Snapshot::capture(&temporary_root.join(".crates2.json"), 4 * 1024 * 1024)?;
        let built = cargo_install::Record::parse(&built_toml.bytes, &built_json.bytes, &tag[1..])?;
        let (new_toml, new_json) = record.updated(built, tag)?;
        let path = temporary_root.join("bin/ghostty-wall");
        binary = Snapshot::capture(&path, MAX_UNPACKED)?.bytes;
        verify_executable(&path, &tag[1..])?;
        changes.push((toml, new_toml));
        changes.push((json, new_json));
    }
    writeln!(
        output,
        "Installing {} and ownership metadata...",
        exe.display()
    )?;
    changes.insert(0, (previous, binary));
    publication::publish(exe, changes)
}

#[cfg(not(unix))]
fn install(
    _: &Path,
    _: &str,
    _: &str,
    _: &mut impl Write,
    _: &mut impl FnMut(&str, u64) -> Result<Vec<u8>, UpdateError>,
    _: impl FnOnce(&str, &Path) -> Result<(), UpdateError>,
) -> Result<(), UpdateError> {
    Err(UpdateError::Unsupported)
}

fn verify_executable(path: &Path, version: &str) -> Result<(), UpdateError> {
    let result = std::process::Command::new(path)
        .arg("--version")
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| UpdateError::Build(format!("prepared executable could not run: {e}")))?;
    if !result.status.success()
        || String::from_utf8_lossy(&result.stdout).trim() != format!("ghostty-wall {version}")
    {
        return Err(UpdateError::Build(
            "prepared executable version does not match the requested release".into(),
        ));
    }
    Ok(())
}

fn verify_checksum(archive: &[u8], checksum: &[u8], name: &str) -> Result<(), UpdateError> {
    let text =
        std::str::from_utf8(checksum).map_err(|_| UpdateError::Checksum("invalid UTF-8".into()))?;
    let line = text.strip_suffix('\n').unwrap_or(text);
    let (hex, filename) = line
        .split_once("  ")
        .ok_or_else(|| UpdateError::Checksum("expected one sha256sum line".into()))?;
    if hex.len() != 64 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) || filename != name {
        return Err(UpdateError::Checksum(
            "wrong digest or asset filename".into(),
        ));
    }
    if !hex.eq_ignore_ascii_case(&sha256_hex(archive)) {
        return Err(UpdateError::Checksum(
            "archive does not match checksum".into(),
        ));
    }
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

struct UnpackedArchive<R> {
    reader: io::Take<R>,
}

impl<R: Read> Read for UnpackedArchive<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if !buffer.is_empty() && self.reader.limit() == 0 {
            let mut excess = [0];
            if self.reader.get_mut().read(&mut excess)? != 0 {
                return Err(io::Error::other("archive too large"));
            }
            return Ok(0);
        }
        self.reader.read(buffer)
    }
}

fn extract_binary(archive: &[u8], directory: &Path) -> Result<Vec<u8>, UpdateError> {
    let decoder = UnpackedArchive {
        reader: GzDecoder::new(archive).take(MAX_UNPACKED),
    };
    let mut tar = tar::Archive::new(decoder);
    let mut found = None;
    let mut total = 0u64;
    for entry in tar
        .entries()
        .map_err(|e| UpdateError::Archive(e.to_string()))?
    {
        let mut entry = entry.map_err(|e| UpdateError::Archive(e.to_string()))?;
        let path = entry
            .path()
            .map_err(|e| UpdateError::Archive(e.to_string()))?;
        let parts: Vec<_> = path.components().collect();
        if parts.is_empty() || parts.iter().any(|p| !matches!(p, Component::Normal(_))) {
            return Err(UpdateError::Archive("unsafe entry path".into()));
        }
        let kind = entry.header().entry_type();
        if !kind.is_file() && !kind.is_dir() {
            return Err(UpdateError::Archive(
                "links and special entries are not allowed".into(),
            ));
        }
        total = total
            .checked_add(entry.size())
            .ok_or_else(|| UpdateError::Archive("archive too large".into()))?;
        if total > MAX_UNPACKED {
            return Err(UpdateError::Archive("archive too large".into()));
        }
        if parts.len() == 2 && parts[1].as_os_str() == "ghostty-wall" {
            if found.is_some()
                || !kind.is_file()
                || entry
                    .header()
                    .mode()
                    .map_err(|e| UpdateError::Archive(e.to_string()))?
                    & 0o111
                    == 0
            {
                return Err(UpdateError::Archive(
                    "expected one executable ghostty-wall".into(),
                ));
            }
            if entry.size() > MAX_ARCHIVE {
                return Err(UpdateError::Archive("binary too large".into()));
            }
            let mut bytes = Vec::new();
            entry
                .read_to_end(&mut bytes)
                .map_err(|e| UpdateError::Archive(e.to_string()))?;
            if bytes.len() < 20
                || &bytes[..4] != b"\x7fELF"
                || bytes[4] != 2
                || bytes[5] != 1
                || bytes[18..20] != [62, 0]
            {
                return Err(UpdateError::Archive(
                    "expected Linux x86_64 ELF executable".into(),
                ));
            }
            found = Some(bytes);
        }
    }
    io::copy(&mut tar.into_inner(), &mut io::sink())
        .map_err(|e| UpdateError::Archive(e.to_string()))?;
    let bytes =
        found.ok_or_else(|| UpdateError::Archive("missing executable ghostty-wall".into()))?;
    fs::write(directory.join("ghostty-wall"), &bytes)
        .map_err(|e| UpdateError::Archive(e.to_string()))?;
    Ok(bytes)
}

#[cfg(test)]
fn verify_ownership(exe: &Path) -> Result<(), UpdateError> {
    let ownership = || UpdateError::Ownership(exe.display().to_string());
    if !fs::symlink_metadata(exe)
        .map_err(|_| ownership())?
        .file_type()
        .is_file()
    {
        return Err(ownership());
    }
    let marker = exe.with_file_name(".ghostty-wall-release.sha256");
    let checksum = fs::read(&marker).map_err(|_| ownership())?;
    let binary = fs::read(exe).map_err(|_| ownership())?;
    verify_checksum(&binary, &checksum, "ghostty-wall").map_err(|_| ownership())
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{Compression, write::GzEncoder};
    use tar::{Builder, EntryType, Header};

    #[cfg(unix)]
    include!("update/regression.rs");

    fn elf() -> Vec<u8> {
        static BINARY: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
        BINARY
            .get_or_init(|| {
                let dir = TempDir::new().unwrap();
                let source = dir.path().join("release.rs");
                fs::write(&source, "fn main() { println!(\"ghostty-wall 1.0.10\"); }").unwrap();
                let binary = dir.path().join("ghostty-wall");
                assert!(
                    std::process::Command::new("rustc")
                        .arg(&source)
                        .arg("-o")
                        .arg(&binary)
                        .status()
                        .unwrap()
                        .success()
                );
                fs::read(binary).unwrap()
            })
            .clone()
    }

    fn archive(files: &[(&str, &[u8])]) -> Vec<u8> {
        let encoder = GzEncoder::new(Vec::new(), Compression::default());
        let mut builder = Builder::new(encoder);
        for (name, data) in files {
            let mut header = Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o755);
            header.set_entry_type(EntryType::Regular);
            header.set_path("root/ghostty-wall").unwrap();
            if *name != "root/ghostty-wall" {
                header.as_mut_bytes()[..100].fill(0);
                header.as_mut_bytes()[..name.len()].copy_from_slice(name.as_bytes());
            }
            header.set_cksum();
            builder.append(&header, *data).unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap()
    }

    fn fixture_get(
        archive: &[u8],
        urls: &mut Vec<String>,
        url: &str,
    ) -> Result<Vec<u8>, UpdateError> {
        urls.push(url.into());
        if url.ends_with("/releases/latest") {
            Ok(br#"{"tag_name":"v1.0.10"}"#.to_vec())
        } else if url.ends_with(".sha256") {
            Ok(format!("{}  {ASSET}\n", sha256_hex(archive)).into_bytes())
        } else {
            Ok(archive.to_vec())
        }
    }

    #[test]
    fn cargo_installation_updates_at_its_original_prefix() {
        let dir = TempDir::new().unwrap();
        let exe = dir.path().join("bin/ghostty-wall");
        fs::create_dir(exe.parent().unwrap()).unwrap();
        fs::write(&exe, b"old").unwrap();
        let key = "ghostty-wall 1.0.9 (git+https://github.com/GiovanniCaiazzo01/Ghostty-wall#abc)";
        fs::write(
            dir.path().join(".crates.toml"),
            format!("[v1]\n{key:?} = [\"ghostty-wall\"]\n"),
        )
        .unwrap();
        fs::write(
            dir.path().join(".crates2.json"),
            serde_json::to_vec(&serde_json::json!({"installs": {key: {"bins": ["ghostty-wall"]}}}))
                .unwrap(),
        )
        .unwrap();
        let result = run_with_builder(
            false,
            "1.0.9",
            Some(&exe),
            &mut Vec::new(),
            |url, _| fixture_get(&[], &mut Vec::new(), url),
            |_, _| Err(UpdateError::Network("controlled build failure".into())),
        );
        assert!(
            matches!(result, Err(UpdateError::Network(ref text)) if text == "controlled build failure"),
            "{result:?}"
        );
        assert_eq!(fs::read(exe).unwrap(), b"old");
    }

    #[cfg(unix)]
    #[test]
    fn rejects_verified_archive_with_unusable_executable() {
        let dir = TempDir::new().unwrap();
        let exe = dir.path().join("ghostty-wall");
        fs::write(&exe, b"old").unwrap();
        let marker = exe.with_file_name(".ghostty-wall-release.sha256");
        fs::write(&marker, format!("{}  ghostty-wall\n", sha256_hex(b"old"))).unwrap();
        let old_marker = fs::read(&marker).unwrap();
        let mut fake = vec![0; 20];
        fake[..6].copy_from_slice(b"\x7fELF\x02\x01");
        fake[18] = 62;
        let data = archive(&[("root/ghostty-wall", &fake)]);
        let result = run_with(false, "1.0.9", Some(&exe), &mut Vec::new(), |url, _| {
            fixture_get(&data, &mut Vec::new(), url)
        });
        assert!(result.is_err(), "unusable release was published");
        assert_eq!(fs::read(&exe).unwrap(), b"old");
        assert_eq!(fs::read(&marker).unwrap(), old_marker);
    }

    #[test]
    fn version_and_metadata() {
        assert!(version("v1.0.10").unwrap() > version("v1.0.9").unwrap());
        for tag in [
            "1.0.3",
            "v1.0",
            "v1.0.3-rc.1",
            "v01.0.3",
            "v1.0.3+build",
            "v1.0.3/evil",
        ] {
            assert!(version(tag).is_err(), "{tag}");
        }
        let mut output = Vec::new();
        let err = run_with(true, "1.0.9", None, &mut output, |_, _| {
            Ok(br#"{}"#.to_vec())
        })
        .unwrap_err();
        assert!(matches!(err, UpdateError::Release(_)));
        assert!(String::from_utf8_lossy(&output).contains("Checking"));
        assert!(matches!(
            run_with(true, "1.0.9", None, &mut output, |_, _| Ok(
                br#"{"tag_name":"bad"}"#.to_vec()
            )),
            Err(UpdateError::Release(_))
        ));
        assert!(matches!(
            run_with(true, "1.0.9", None, &mut output, |_, _| Err(
                UpdateError::Network("offline".into())
            )),
            Err(UpdateError::Network(_))
        ));
        assert!(!String::from_utf8_lossy(&output).contains("Installing"));
        assert!(supported("linux", "x86_64"));
        assert!(!supported("macos", "aarch64"));
        assert!(!supported("linux", "aarch64"));
    }

    #[test]
    fn checks_never_download_or_write() {
        let dir = TempDir::new().unwrap();
        let exe = dir.path().join("ghostty-wall");
        fs::write(&exe, b"old").unwrap();
        let mut urls = Vec::new();
        let mut output = Vec::new();
        run_with(true, "1.0.9", Some(&exe), &mut output, |url, _| {
            fixture_get(&[], &mut urls, url)
        })
        .unwrap();
        assert_eq!(urls.len(), 1);
        assert!(
            String::from_utf8(output)
                .unwrap()
                .contains("Update available")
        );
        assert_eq!(fs::read(&exe).unwrap(), b"old");
        output = Vec::new();
        run_with(true, "1.0.10", Some(&exe), &mut output, |url, _| {
            fixture_get(&[], &mut urls, url)
        })
        .unwrap();
        assert!(String::from_utf8(output).unwrap().contains("up to date"));
        assert_eq!(urls.len(), 2);
        output = Vec::new();
        run_with(false, "1.0.10", Some(&exe), &mut output, |url, _| {
            fixture_get(&[], &mut urls, url)
        })
        .unwrap();
        assert!(
            String::from_utf8(output)
                .unwrap()
                .contains("already up to date")
        );
        assert_eq!(urls.len(), 3);
        assert!(!exe.with_file_name(".ghostty-wall-release.sha256").exists());
    }

    #[test]
    fn verifies_digest_and_archive_shape() {
        let binary = elf();
        let data = archive(&[("root/ghostty-wall", &binary)]);
        let checksum = format!("{}  {ASSET}\n", sha256_hex(&data));
        verify_checksum(&data, checksum.as_bytes(), ASSET).unwrap();
        for bad in [
            "garbage".to_owned(),
            format!("{}  wrong.tar.gz\n", sha256_hex(&data)),
            format!(
                "{}  {ASSET}\n{}  {ASSET}\n",
                sha256_hex(&data),
                sha256_hex(&data)
            ),
        ] {
            assert!(verify_checksum(&data, bad.as_bytes(), ASSET).is_err());
        }
        assert!(verify_checksum(b"tampered", checksum.as_bytes(), ASSET).is_err());
        let dir = TempDir::new().unwrap();
        assert_eq!(extract_binary(&data, dir.path()).unwrap(), binary);
        assert!(
            extract_binary(&archive(&[("root/ghostty-wall", b"#!/bin/sh")]), dir.path()).is_err()
        );
        assert!(extract_binary(&archive(&[("root/LICENSE", b"license")]), dir.path()).is_err());
        assert!(
            extract_binary(
                &archive(&[
                    ("root/ghostty-wall", &binary),
                    ("other/ghostty-wall", &binary)
                ]),
                dir.path()
            )
            .is_err()
        );
        assert!(extract_binary(&archive(&[("../ghostty-wall", b"evil")]), dir.path()).is_err());
        assert!(!dir.path().parent().unwrap().join("ghostty-wall").exists());
    }

    #[cfg(unix)]
    #[test]
    fn updates_only_owned_temp_executable_and_rejects_bad_checksum() {
        let dir = TempDir::new().unwrap();
        let exe = dir.path().join("ghostty-wall");
        fs::write(&exe, b"old").unwrap();
        let binary = elf();
        let data = archive(&[("root/ghostty-wall", &binary)]);
        let mut output = Vec::new();
        let mut urls = Vec::new();
        assert!(matches!(
            run_with(false, "1.0.9", Some(&exe), &mut output, |url, _| {
                fixture_get(&data, &mut urls, url)
            }),
            Err(UpdateError::Ownership(_))
        ));
        assert_eq!(urls.len(), 1);
        fs::write(
            exe.with_file_name(".ghostty-wall-release.sha256"),
            format!("{}  ghostty-wall\n", sha256_hex(b"old")),
        )
        .unwrap();
        output.clear();
        run_with(false, "1.0.9", Some(&exe), &mut output, |url, _| {
            fixture_get(&data, &mut urls, url)
        })
        .unwrap();
        assert_eq!(fs::read(&exe).unwrap(), binary);
        verify_ownership(&exe).unwrap();
        assert!(
            String::from_utf8(output)
                .unwrap()
                .contains("Updated Ghostty Wall 1.0.9 -> 1.0.10")
        );
        assert!(urls.iter().any(|url| url.contains("/download/v1.0.10/")));
        assert!(!urls.iter().any(|url| url.contains("/latest/download/")));

        fs::write(&exe, b"old").unwrap();
        assert!(verify_ownership(&exe).is_err());
        fs::write(
            exe.with_file_name(".ghostty-wall-release.sha256"),
            format!("{}  ghostty-wall\n", sha256_hex(b"old")),
        )
        .unwrap();
        let result = run_with(false, "1.0.9", Some(&exe), &mut Vec::new(), |url, _| {
            if url.ends_with(".sha256") {
                Ok(b"0"
                    .repeat(64)
                    .into_iter()
                    .chain(format!("  {ASSET}\n").into_bytes())
                    .collect())
            } else {
                fixture_get(&data, &mut Vec::new(), url)
            }
        });
        assert!(matches!(result, Err(UpdateError::Checksum(_))));
        assert_eq!(fs::read(&exe).unwrap(), b"old");
    }
}
