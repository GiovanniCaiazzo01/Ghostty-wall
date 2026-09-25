//! Explicit, release-owned executable updates; never touches the Managed Root.

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
use tempfile::{NamedTempFile, TempDir};
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
        "Automatic updates are not available for this platform.\nCurrent official prebuilt releases support Linux x86_64. Use the installation method appropriate for your platform."
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
    /// Executable is not known to be owned by the release installer.
    #[error(
        "Cannot update {0}: release installer ownership could not be verified. Reinstall using the release installer to enable self-updates; Cargo and manual installations should use their original installation method."
    )]
    Ownership(String),
    /// Executable replacement failed without intentionally escalating privilege.
    #[error(
        "Cannot update {0}: {1}. Reinstall Ghostty Wall using the installation method that owns this binary."
    )]
    Replace(String, #[source] io::Error),
    /// Binary was replaced but the companion ownership proof failed to publish.
    #[error(
        "Updated {0}, but could not refresh release ownership marker: {1}. Reinstall using the release installer before the next update."
    )]
    Marker(String, #[source] io::Error),
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
    if !check && !supported(env::consts::OS, env::consts::ARCH) {
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
    mut get: impl FnMut(&str, u64) -> Result<Vec<u8>, UpdateError>,
) -> Result<(), UpdateError> {
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
    verify_ownership(exe)?;
    writeln!(
        output,
        "Current version: {current}\nLatest version:  {latest}\n"
    )?;
    writeln!(output, "Downloading {}...", release.tag_name)?;
    let base = format!(
        "https://github.com/{REPOSITORY}/releases/download/{}/{ASSET}",
        release.tag_name
    );
    let archive = get(&base, MAX_ARCHIVE)?;
    let checksum = get(&format!("{base}.sha256"), 1024)?;
    writeln!(output, "Verifying SHA-256...")?;
    verify_checksum(&archive, &checksum, ASSET)?;
    let temporary = TempDir::new().map_err(|e| UpdateError::Archive(e.to_string()))?;
    let binary = extract_binary(&archive, temporary.path())?;
    writeln!(output, "Updating {}...", exe.display())?;
    replace_executable(exe, &binary)?;
    writeln!(output, "\nUpdated Ghostty Wall {current} -> {latest}.")?;
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

fn extract_binary(archive: &[u8], directory: &Path) -> Result<Vec<u8>, UpdateError> {
    let decoder = GzDecoder::new(archive);
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
    let bytes =
        found.ok_or_else(|| UpdateError::Archive("missing executable ghostty-wall".into()))?;
    fs::write(directory.join("ghostty-wall"), &bytes)
        .map_err(|e| UpdateError::Archive(e.to_string()))?;
    Ok(bytes)
}

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

#[cfg(unix)]
fn replace_executable(exe: &Path, binary: &[u8]) -> Result<(), UpdateError> {
    use std::os::unix::fs::PermissionsExt;

    let parent = exe
        .parent()
        .ok_or_else(|| UpdateError::Ownership(exe.display().to_string()))?;
    let failure = |e| UpdateError::Replace(exe.display().to_string(), e);
    let mut staged = NamedTempFile::new_in(parent).map_err(failure)?;
    staged.write_all(binary).map_err(failure)?;
    staged
        .as_file()
        .set_permissions(fs::Permissions::from_mode(0o755))
        .map_err(failure)?;
    staged.as_file().sync_all().map_err(failure)?;

    let marker = exe.with_file_name(".ghostty-wall-release.sha256");
    let mut proof = NamedTempFile::new_in(parent).map_err(failure)?;
    writeln!(proof, "{}  ghostty-wall", sha256_hex(binary)).map_err(failure)?;
    proof
        .as_file()
        .set_permissions(fs::Permissions::from_mode(0o644))
        .map_err(failure)?;
    proof.as_file().sync_all().map_err(failure)?;

    staged.persist(exe).map_err(|e| failure(e.error))?;
    proof
        .persist(&marker)
        .map_err(|e| UpdateError::Marker(exe.display().to_string(), e.error))?;
    Ok(())
}

#[cfg(not(unix))]
fn replace_executable(_: &Path, _: &[u8]) -> Result<(), UpdateError> {
    Err(UpdateError::Unsupported)
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{Compression, write::GzEncoder};
    use tar::{Builder, EntryType, Header};

    fn elf() -> Vec<u8> {
        let mut bytes = vec![0; 20];
        bytes[..4].copy_from_slice(b"\x7fELF");
        bytes[4] = 2;
        bytes[5] = 1;
        bytes[18] = 62;
        bytes.extend_from_slice(b"new");
        bytes
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
        assert!(output.is_empty());
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
        assert!(output.is_empty());
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
