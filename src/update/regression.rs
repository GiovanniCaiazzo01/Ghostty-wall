#[cfg(target_os = "linux")]
fn assert_oversized_tar_preserves_installation(extension: Option<EntryType>) {
    use std::os::unix::fs::PermissionsExt;

    let encoder = GzEncoder::new(Vec::new(), Compression::fast());
    let mut builder = Builder::new(encoder);
    let mut filler = Header::new_gnu();
    filler.set_path("root/filler").unwrap();
    filler.set_size(0);
    filler.set_mode(0o644);
    filler.set_cksum();
    let payload = if extension == Some(EntryType::XHeader) {
        b"13 comment=x\n".repeat(256)
    } else {
        vec![b'a'; 4096]
    };
    let mut expanded = 0;
    while expanded <= MAX_UNPACKED {
        if let Some(kind) = extension {
            let mut header = Header::new_gnu();
            header.set_path("extension").unwrap();
            header.set_entry_type(kind);
            header.set_size(payload.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append(&header, payload.as_slice()).unwrap();
            expanded += 512 + (payload.len() as u64).div_ceil(512) * 512;
        }
        builder.append(&filler, io::empty()).unwrap();
        expanded += 512;
    }
    let binary = elf();
    let mut header = Header::new_gnu();
    header.set_path("root/ghostty-wall").unwrap();
    header.set_size(binary.len() as u64);
    header.set_mode(0o755);
    header.set_cksum();
    builder.append(&header, binary.as_slice()).unwrap();
    let data = builder.into_inner().unwrap().finish().unwrap();
    assert!((data.len() as u64) < MAX_ARCHIVE);

    let dir = TempDir::new().unwrap();
    let exe = dir.path().join("ghostty-wall");
    let old = b"#!/bin/sh\necho 'ghostty-wall 1.0.9'\n";
    fs::write(&exe, old).unwrap();
    fs::set_permissions(&exe, fs::Permissions::from_mode(0o755)).unwrap();
    let marker = exe.with_file_name(".ghostty-wall-release.sha256");
    let proof = format!("{}  ghostty-wall\n", sha256_hex(old));
    fs::write(&marker, &proof).unwrap();
    let mut output = Vec::new();
    let result = run_with(false, "1.0.9", Some(&exe), &mut output, |url, _| {
        fixture_get(&data, &mut Vec::new(), url)
    });
    assert!(
        matches!(result, Err(UpdateError::Archive(ref message)) if message.contains("archive too large")),
        "{result:?}"
    );
    assert_eq!(fs::read(&exe).unwrap(), old);
    assert_eq!(fs::read_to_string(marker).unwrap(), proof);
    assert!(!String::from_utf8(output).unwrap().contains("Installing"));
}

#[cfg(target_os = "linux")]
#[test]
fn header_heavy_archive_is_bounded_before_publication() {
    assert_oversized_tar_preserves_installation(None);
}

#[cfg(target_os = "linux")]
#[test]
fn gnu_extension_heavy_archive_is_bounded_before_publication() {
    assert_oversized_tar_preserves_installation(Some(EntryType::GNULongName));
}

#[cfg(target_os = "linux")]
#[test]
fn pax_extension_heavy_archive_is_bounded_before_publication() {
    assert_oversized_tar_preserves_installation(Some(EntryType::XHeader));
}

#[test]
#[ignore = "run via tests/qa_update_publication.py with controlled Cargo process"]
fn source_build_process_boundary() {
    let mode = env::var("GW_QA_CARGO").expect("process runner required");
    let dir = TempDir::new().unwrap();
    let exe = cargo_fixture(dir.path(), "1.0.9", false);
    let before = installation_bytes(dir.path());
    let mut output = Vec::new();
    let result = run_with(false, "1.0.9", Some(&exe), &mut output, |url, _| {
        fixture_get(&[], &mut Vec::new(), url)
    });
    if mode == "success" {
        result.unwrap();
        verify_executable(&exe, "1.0.10").unwrap();
    } else {
        assert!(matches!(result, Err(UpdateError::Build(_))), "{result:?}");
        assert_eq!(installation_bytes(dir.path()), before);
        verify_executable(&exe, "1.0.9").unwrap();
    }
    assert!(!String::from_utf8_lossy(&output).contains("private compiler output"));
}

#[cfg(target_os = "linux")]
#[test]
fn release_upgrade_verifies_invoked_version_and_repeat_checks() {
    use std::os::unix::fs::PermissionsExt;
    let dir = TempDir::new().unwrap();
    let exe = dir.path().join("ghostty-wall");
    let old = b"#!/bin/sh\necho 'ghostty-wall 1.0.9'\n";
    fs::write(&exe, old).unwrap();
    fs::set_permissions(&exe, fs::Permissions::from_mode(0o755)).unwrap();
    let marker = exe.with_file_name(".ghostty-wall-release.sha256");
    fs::write(&marker, format!("{}  ghostty-wall\n", sha256_hex(old))).unwrap();
    let data = archive(&[("root/ghostty-wall", &elf())]);
    run_with(false, "1.0.9", Some(&exe), &mut Vec::new(), |url, _| {
        fixture_get(&data, &mut Vec::new(), url)
    })
    .unwrap();
    verify_executable(&exe, "1.0.10").unwrap();
    verify_ownership(&exe).unwrap();
    let bytes = fs::read(&exe).unwrap();
    let proof = fs::read(&marker).unwrap();
    for check in [true, false] {
        run_with(check, "1.0.10", Some(&exe), &mut Vec::new(), |url, _| {
            assert!(url.ends_with("/releases/latest"));
            fixture_get(&[], &mut Vec::new(), url)
        })
        .unwrap();
    }
    assert_eq!(fs::read(&exe).unwrap(), bytes);
    assert_eq!(fs::read(marker).unwrap(), proof);
}

#[test]
#[ignore = "run via tests/qa_update_publication.py with filesystem fault injection"]
fn publication_fault_preserves_installation_and_substitutions() {
    let fault = env::var("GW_QA_UPDATE_FAULT").expect("fault runner required");
    let dir = TempDir::new().unwrap();
    let root = dir.path().join("fault-prefix");
    let exe = cargo_fixture(&root, "1.0.9", false);
    let before = installation_bytes(&root);
    if fault == "marker" {
        fs::remove_file(root.join(".crates.toml")).unwrap();
        fs::remove_file(root.join(".crates2.json")).unwrap();
        let marker = exe.with_file_name(".ghostty-wall-release.sha256");
        let proof = format!("{}  ghostty-wall\n", sha256_hex(&before[0]));
        fs::write(&marker, &proof).unwrap();
        let data = archive(&[("root/ghostty-wall", &elf())]);
        let result = run_with(false, "1.0.9", Some(&exe), &mut Vec::new(), |url, _| {
            fixture_get(&data, &mut Vec::new(), url)
        });
        assert!(
            matches!(result, Err(UpdateError::Replace(_, _))),
            "{result:?}"
        );
        assert_eq!(fs::read(&exe).unwrap(), before[0]);
        assert_eq!(fs::read_to_string(marker).unwrap(), proof);
        verify_executable(&exe, "1.0.9").unwrap();
        return;
    }
    let result = run_with_builder(
        false,
        "1.0.9",
        Some(&exe),
        &mut Vec::new(),
        |url, _| fixture_get(&[], &mut Vec::new(), url),
        |_, stage| {
            cargo_fixture(stage, "1.0.10", true);
            Ok(())
        },
    );
    assert!(result.is_err(), "fault must prevent success: {fault}");
    let after = installation_bytes(&root);
    if fault == "rollback" {
        assert!(
            matches!(result, Err(UpdateError::Recovery(_))),
            "{result:?}"
        );
        assert_eq!(after[1..], before[1..]);
        let retained: Vec<_> = fs::read_dir(exe.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with(".ghostty-wall-update-ghostty-wall-")
            })
            .collect();
        assert_eq!(retained.len(), 1);
        assert_eq!(fs::read(&retained[0]).unwrap(), before[0]);
        verify_executable(&retained[0], "1.0.9").unwrap();
    } else if fault == "race" {
        assert_eq!(after[0], b"substituted binary");
        assert_eq!(after[1..], before[1..]);
    } else {
        assert_eq!(after, before);
        verify_executable(&exe, "1.0.9").unwrap();
    }
}

fn cargo_fixture(root: &Path, version: &str, official: bool) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    fs::create_dir_all(root.join("bin")).unwrap();
    let exe = root.join("bin/ghostty-wall");
    fs::write(
        &exe,
        format!("#!/bin/sh\nprintf 'ghostty-wall {version}\\n'\n"),
    )
    .unwrap();
    fs::set_permissions(&exe, fs::Permissions::from_mode(0o755)).unwrap();
    let key = if official {
        format!(
            "ghostty-wall {version} (git+https://github.com/{REPOSITORY}?tag=v{version}#0123456789012345678901234567890123456789)"
        )
    } else {
        format!("ghostty-wall {version} (path+file:///old/source)")
    };
    fs::write(root.join(".crates.toml"), format!("[v1]\n{key:?} = [\"ghostty-wall\"]\n\"other-tool 2.0.0 (registry+https://example.org)\" = [\"other-tool\"]\n")).unwrap();
    fs::write(root.join(".crates2.json"), serde_json::to_vec(&serde_json::json!({"installs": {
        key: {"version_req": null, "bins": ["ghostty-wall"], "features": [], "all_features": false, "no_default_features": false, "profile": "release", "target": "x86_64-unknown-linux-gnu", "rustc": "fixture"},
        "other-tool 2.0.0 (registry+https://example.org)": {"bins": ["other-tool"], "custom": "preserved"}
    }})).unwrap()).unwrap();
    exe
}

fn installation_bytes(root: &Path) -> Vec<Vec<u8>> {
    ["bin/ghostty-wall", ".crates.toml", ".crates2.json"]
        .map(|file| fs::read(root.join(file)).unwrap())
        .to_vec()
}

#[test]
fn cargo_upgrade_preserves_other_packages_and_repeats_without_building() {
    let dir = TempDir::new().unwrap();
    let root = dir.path().join("nonstandard-prefix");
    let exe = cargo_fixture(&root, "1.0.9", false);
    let sentinel = dir.path().join("managed-data");
    fs::write(
        &sentinel,
        b"Profiles Sources History Environments Assets Integration",
    )
    .unwrap();
    let mut output = Vec::new();
    run_with_builder(
        false,
        "1.0.9",
        Some(&exe),
        &mut output,
        |url, _| fixture_get(&[], &mut Vec::new(), url),
        |tag, stage| {
            assert_eq!(tag, "v1.0.10");
            cargo_fixture(stage, "1.0.10", true);
            Ok(())
        },
    )
    .unwrap();
    verify_executable(&exe, "1.0.10").unwrap();
    let json: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join(".crates2.json")).unwrap()).unwrap();
    assert_eq!(
        json["installs"]["other-tool 2.0.0 (registry+https://example.org)"]["custom"],
        "preserved"
    );
    assert_eq!(json["installs"].as_object().unwrap().len(), 2);
    assert!(
        !String::from_utf8(fs::read(root.join(".crates.toml")).unwrap())
            .unwrap()
            .contains("1.0.9")
    );
    assert!(!root.join("bin/.ghostty-wall-release.sha256").exists());
    let before = installation_bytes(&root);
    for check in [true, false] {
        run_with_builder(
            check,
            "1.0.10",
            Some(&exe),
            &mut output,
            |url, _| {
                assert!(url.ends_with("/releases/latest"));
                fixture_get(&[], &mut Vec::new(), url)
            },
            |_, _| panic!("already-current update/check must not build"),
        )
        .unwrap();
        assert_eq!(installation_bytes(&root), before);
    }
    assert_eq!(
        fs::read(sentinel).unwrap(),
        b"Profiles Sources History Environments Assets Integration"
    );
    let output = String::from_utf8(output).unwrap();
    for phase in [
        "Checking",
        "Preparing",
        "Verifying",
        "Installing",
        "Updated",
        "already up to date",
    ] {
        assert!(output.contains(phase), "{phase}: {output}");
    }
}

#[test]
fn cargo_failures_preserve_binary_and_both_metadata_files() {
    for case in [
        "offline",
        "invalid-release",
        "build",
        "wrong-version",
        "wrong-source",
        "metadata",
        "permission",
    ] {
        let dir = TempDir::new().unwrap();
        let exe = cargo_fixture(dir.path(), "1.0.9", false);
        let before = installation_bytes(dir.path());
        let result = run_with_builder(
            false,
            "1.0.9",
            Some(&exe),
            &mut Vec::new(),
            |url, _| match case {
                "offline" => Err(UpdateError::Network("controlled offline".into())),
                "invalid-release" => Ok(br#"{"tag_name":"v1.0.10-rc.1"}"#.to_vec()),
                _ => fixture_get(&[], &mut Vec::new(), url),
            },
            |_, stage| {
                if case == "build" {
                    return Err(UpdateError::Build("controlled compiler failure".into()));
                }
                cargo_fixture(stage, "1.0.10", case != "wrong-source");
                if case == "wrong-version" {
                    fs::write(
                        stage.join("bin/ghostty-wall"),
                        b"#!/bin/sh\necho 'ghostty-wall 99.0.0'\n",
                    )
                    .unwrap();
                }
                if case == "metadata" {
                    fs::write(stage.join(".crates2.json"), b"{}").unwrap();
                }
                if case == "permission" {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(exe.parent().unwrap(), fs::Permissions::from_mode(0o555))
                        .unwrap();
                }
                Ok(())
            },
        );
        if case == "permission" {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(exe.parent().unwrap(), fs::Permissions::from_mode(0o755)).unwrap();
        }
        assert!(result.is_err(), "{case}");
        assert_eq!(installation_bytes(dir.path()), before, "{case}");
        verify_executable(&exe, "1.0.9").unwrap();
    }
}

#[test]
fn substituted_cargo_binary_or_metadata_is_never_overwritten() {
    use std::os::unix::fs::symlink;
    for relative in ["bin/ghostty-wall", ".crates.toml", ".crates2.json", "bin"] {
        let dir = TempDir::new().unwrap();
        let root = dir.path().join("prefix");
        let exe = cargo_fixture(&root, "1.0.9", false);
        let other = dir.path().join("unrelated");
        fs::write(&other, b"do not touch").unwrap();
        let path = root.join(relative);
        let detached = dir.path().join("detached");
        let result = run_with_builder(
            false,
            "1.0.9",
            Some(&exe),
            &mut Vec::new(),
            |url, _| fixture_get(&[], &mut Vec::new(), url),
            |_, stage| {
                cargo_fixture(stage, "1.0.10", true);
                fs::rename(&path, &detached).unwrap();
                symlink(if relative == "bin" { &detached } else { &other }, &path).unwrap();
                Ok(())
            },
        );
        assert!(result.is_err(), "{relative}");
        assert_eq!(fs::read(&other).unwrap(), b"do not touch");
        assert!(
            fs::symlink_metadata(&path)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        if relative == "bin" {
            verify_executable(&detached.join("ghostty-wall"), "1.0.9").unwrap();
        }
    }
}

#[test]
fn cargo_installation_lock_prevents_concurrent_publication() {
    use std::os::fd::AsRawFd;
    let dir = TempDir::new().unwrap();
    let exe = cargo_fixture(dir.path(), "1.0.9", false);
    let lock = fs::File::open(dir.path().join(".crates.toml")).unwrap();
    assert_eq!(
        unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
        0
    );
    let before = installation_bytes(dir.path());
    let result = run_with_builder(
        false,
        "1.0.9",
        Some(&exe),
        &mut Vec::new(),
        |url, _| fixture_get(&[], &mut Vec::new(), url),
        |_, _| panic!("another Cargo install owns the metadata lock"),
    );
    assert!(result.is_err());
    assert_eq!(installation_bytes(dir.path()), before);
}

#[test]
fn cargo_check_ignores_ownership_but_install_rejects_inconsistent_metadata() {
    let dir = TempDir::new().unwrap();
    let exe = cargo_fixture(dir.path(), "1.0.9", false);
    fs::write(dir.path().join(".crates2.json"), b"{}").unwrap();
    let before = installation_bytes(dir.path());
    run_with_builder(
        true,
        "1.0.9",
        Some(&exe),
        &mut Vec::new(),
        |url, _| fixture_get(&[], &mut Vec::new(), url),
        |_, _| panic!("check must not build"),
    )
    .unwrap();
    assert!(
        run_with_builder(
            false,
            "1.0.9",
            Some(&exe),
            &mut Vec::new(),
            |url, _| fixture_get(&[], &mut Vec::new(), url),
            |_, _| panic!("invalid ownership must not build")
        )
        .is_err()
    );
    assert_eq!(installation_bytes(dir.path()), before);
}
