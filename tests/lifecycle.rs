use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use ghostty_wall::{
    codec::intent::parse_config_toml,
    domain::SourceIntent,
    init::{InitError, InitPaths, init, init_repair},
    lifecycle::{CheckStatus, LifecycleError, doctor, migrate_legacy, uninstall},
    recovery::inspect_recovery_state,
};

#[test]
fn doctor_reports_verified_failed_and_unavailable_without_mutation() {
    let paths = test_paths("doctor");
    init(&paths).unwrap();
    let root = paths.managed_root();
    let before = snapshot(&paths.home);

    let healthy = doctor(&paths);
    assert_eq!(
        healthy.status("managed-layout"),
        Some(CheckStatus::Verified)
    );
    assert_eq!(
        healthy.status("durable-history"),
        Some(CheckStatus::Verified)
    );
    assert_eq!(healthy.status("projection"), Some(CheckStatus::Verified));
    assert_eq!(
        healthy.status("integration-hook"),
        Some(CheckStatus::Verified)
    );
    assert_eq!(
        healthy.status("filesystem-capabilities"),
        Some(CheckStatus::Unavailable)
    );
    assert_eq!(snapshot(&paths.home), before);

    fs::remove_file(root.join("profiles/welcome.toml")).unwrap();
    fs::remove_file(root.join("profiles/welcome.png")).unwrap();
    fs::remove_dir(root.join("profiles")).unwrap();
    let damaged = doctor(&paths);
    assert_eq!(damaged.status("managed-layout"), Some(CheckStatus::Failed));
    assert!(!root.join("profiles").exists());
    assert!(init_repair(&paths).is_err());
    assert!(!root.join("profiles").exists());
}

#[test]
fn legacy_migration_is_atomic_idempotent_and_preserves_legacy_files() {
    let paths = test_paths("migration");
    let ghostty = ghostty_dir(&paths);
    fs::create_dir_all(&ghostty).unwrap();
    let repos = ghostty.join("wallpaper_repos.txt");
    let legacy_projection = ghostty.join("wallpaper.conf");
    let root_config = ghostty.join("config");
    fs::write(
        &repos,
        "# old sources\nanime|ThePrimeagen/anime||/wallpapers/\nlandscape|owner/repo|dev|\n",
    )
    .unwrap();
    fs::write(&legacy_projection, "background-image=/tmp/old.jpg\n").unwrap();
    fs::write(
        &root_config,
        format!(
            "font-size = 12\nconfig-file {}\nconfig-file /tmp/wallpaper.conf\n",
            legacy_projection.display()
        ),
    )
    .unwrap();

    let report = migrate_legacy(&paths, false).unwrap();
    assert_eq!(report.imported_sources, 2);
    assert!(report.config_changed);
    assert_eq!(report.removed_legacy_hooks, 1);

    let config_path = paths.managed_root().join("config.toml");
    let config_bytes = fs::read(&config_path).unwrap();
    let config = parse_config_toml(std::str::from_utf8(&config_bytes).unwrap()).unwrap();
    assert_eq!(config.sources.len(), 2);
    assert!(config.sources.iter().any(|(id, source)| {
        id.as_str() == "anime"
            && matches!(
                source,
                SourceIntent::Github {
                    repository,
                    reference: Some(reference),
                    path: Some(path),
                } if repository == "ThePrimeagen/anime"
                    && reference == "main"
                    && path.as_str() == "wallpapers"
            )
    }));
    assert!(repos.exists());
    assert!(legacy_projection.exists());
    let root_text = fs::read_to_string(&root_config).unwrap();
    assert!(!root_text.contains(&format!("config-file {}", legacy_projection.display())));
    assert!(root_text.contains("config-file /tmp/wallpaper.conf"));
    assert!(root_text.contains("config-file = ?"));

    let mut interrupted_root = fs::read_to_string(&root_config).unwrap();
    interrupted_root.push_str(&format!("config-file {}\n", legacy_projection.display()));
    fs::write(&root_config, interrupted_root).unwrap();
    assert!(inspect_recovery_state(&paths.managed_root(), &root_config).is_err());

    let resumed = migrate_legacy(&paths, false).unwrap();
    assert_eq!(resumed.imported_sources, 0);
    assert!(!resumed.config_changed);
    assert_eq!(resumed.removed_legacy_hooks, 1);
    assert_eq!(fs::read(&config_path).unwrap(), config_bytes);
    inspect_recovery_state(&paths.managed_root(), &root_config).unwrap();

    let repeated = migrate_legacy(&paths, false).unwrap();
    assert_eq!(repeated.imported_sources, 0);
    assert!(!repeated.config_changed);
    assert_eq!(repeated.removed_legacy_hooks, 0);
}

#[test]
fn legacy_migration_dry_run_reports_without_mutation() {
    let paths = test_paths("migration-dry-run");
    let ghostty = ghostty_dir(&paths);
    fs::create_dir_all(&ghostty).unwrap();
    fs::write(ghostty.join("wallpaper_repos.txt"), "anime|owner/repo||\n").unwrap();
    let before = snapshot(&paths.home);

    let report = migrate_legacy(&paths, true).unwrap();
    assert!(report.dry_run);
    assert_eq!(report.imported_sources, 1);
    assert!(report.config_changed);
    assert_eq!(snapshot(&paths.home), before);
    assert!(!paths.managed_root().exists());
}

#[test]
fn legacy_migration_preflights_every_entry_before_mutation() {
    let paths = test_paths("migration-invalid");
    let ghostty = ghostty_dir(&paths);
    fs::create_dir_all(&ghostty).unwrap();
    let repos = ghostty.join("wallpaper_repos.txt");
    let root_config = ghostty.join("config");
    fs::write(
        &repos,
        "valid|owner/repo|main|\nNot-A-Slug|owner/other|main|\n",
    )
    .unwrap();
    fs::write(&root_config, "font-size = 12\n").unwrap();
    let before = snapshot(&paths.home);

    assert!(migrate_legacy(&paths, false).is_err());
    assert_eq!(snapshot(&paths.home), before);
    assert!(!paths.managed_root().exists());
}

#[test]
fn legacy_source_collision_preserves_published_installation() {
    let paths = test_paths("migration-collision");
    init(&paths).unwrap();
    let config = paths.managed_root().join("config.toml");
    fs::write(
        &config,
        "schema_version = 1\n\n[sources.anime]\nkind = \"github\"\nrepository = \"owner/one\"\nref = \"main\"\n",
    )
    .unwrap();
    fs::write(
        ghostty_dir(&paths).join("wallpaper_repos.txt"),
        "anime|owner/two|main|\n",
    )
    .unwrap();
    let before = snapshot(&paths.home);

    assert!(migrate_legacy(&paths, false).is_err());
    assert_eq!(snapshot(&paths.home), before);
}

#[test]
fn incomplete_legacy_migration_blocks_recovery() {
    let paths = test_paths("migration-incomplete");
    let initialized = init(&paths).unwrap();
    let legacy_projection = ghostty_dir(&paths).join("wallpaper.conf");
    fs::write(&legacy_projection, "background-image=/tmp/old.jpg\n").unwrap();
    let mut root_config = fs::read_to_string(&initialized.root_config).unwrap();
    root_config.push_str(&format!("config-file {}\n", legacy_projection.display()));
    fs::write(&initialized.root_config, root_config).unwrap();

    assert!(inspect_recovery_state(&paths.managed_root(), &initialized.root_config).is_err());
}

#[test]
fn uninstall_removes_only_integration_and_disposable_state() {
    let paths = test_paths("uninstall");
    let initialized = init(&paths).unwrap();
    let root = paths.managed_root();
    fs::write(root.join("current.ghostty"), "background = 000000\n").unwrap();
    fs::write(root.join("cache/item"), b"cache").unwrap();
    let outside = paths.home.join("outside");
    fs::write(&outside, b"keep").unwrap();
    symlink(&outside, root.join("cache/link"));

    let report = uninstall(&paths).unwrap();
    assert_eq!(report.removed_hooks, 1);
    assert!(report.removed_projection);
    assert!(report.removed_cache);
    assert!(!root.join("current.ghostty").exists());
    assert!(!root.join("cache").exists());
    assert_eq!(fs::read(&outside).unwrap(), b"keep");
    for relative in [
        "state.lock",
        "config.toml",
        "profiles",
        "assets",
        "environments",
        "history",
    ] {
        assert!(root.join(relative).exists(), "preserved {relative}");
    }
    let root_text = fs::read_to_string(initialized.root_config).unwrap();
    assert!(!root_text.contains("config-file = ?"));
}

#[cfg(unix)]
#[test]
fn uninstall_fails_closed_before_mutation_for_unsafe_config_or_cache() {
    use std::os::unix::fs::PermissionsExt;

    let paths = test_paths("uninstall-unsafe-config");
    let initialized = init(&paths).unwrap();
    let root = paths.managed_root();
    fs::write(root.join("current.ghostty"), b"projection").unwrap();
    fs::set_permissions(&initialized.root_config, fs::Permissions::from_mode(0o666)).unwrap();
    let hook_before = fs::read(&initialized.root_config).unwrap();

    assert!(uninstall(&paths).is_err());
    assert_eq!(fs::read(&initialized.root_config).unwrap(), hook_before);
    assert!(root.join("current.ghostty").exists());
    assert!(root.join("cache").exists());

    fs::set_permissions(&initialized.root_config, fs::Permissions::from_mode(0o600)).unwrap();
    fs::remove_dir(root.join("cache")).unwrap();
    let outside = paths.home.join("outside-cache");
    fs::create_dir(&outside).unwrap();
    symlink(&outside, root.join("cache"));
    assert!(uninstall(&paths).is_err());
    assert!(root.join("current.ghostty").exists());
    assert!(
        fs::read_to_string(&initialized.root_config)
            .unwrap()
            .contains("config-file = ?")
    );
}

#[cfg(unix)]
#[test]
fn uninstall_rejects_symlink_managed_root() {
    let paths = test_paths("uninstall-root-link");
    let logical = paths.managed_root();
    let real = paths.home.join("real-root");
    fs::create_dir_all(logical.parent().unwrap()).unwrap();
    fs::create_dir(&real).unwrap();
    symlink(&real, &logical);

    assert!(matches!(
        uninstall(&paths),
        Err(LifecycleError::Init(InitError::Symlink(_)))
    ));
    assert!(
        fs::symlink_metadata(logical)
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

fn test_paths(name: &str) -> InitPaths {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let home = std::env::temp_dir().join(format!("ghostty-wall-lifecycle-{name}-{unique}"));
    InitPaths {
        xdg_config_home: Some(home.join("xdg")),
        home,
    }
}

fn ghostty_dir(paths: &InitPaths) -> PathBuf {
    paths.xdg_config_home.as_ref().unwrap().join("ghostty")
}

fn snapshot(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    fn visit(root: &Path, path: &Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
        let Ok(metadata) = fs::symlink_metadata(path) else {
            return;
        };
        let relative = path.strip_prefix(root).unwrap().to_owned();
        if metadata.file_type().is_symlink() {
            out.push((
                relative,
                fs::read_link(path)
                    .unwrap()
                    .as_os_str()
                    .as_encoded_bytes()
                    .to_vec(),
            ));
        } else if metadata.is_file() {
            out.push((relative, fs::read(path).unwrap()));
        } else if metadata.is_dir() {
            out.push((relative, Vec::new()));
            let mut entries: Vec<_> = fs::read_dir(path)
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .collect();
            entries.sort();
            for entry in entries {
                visit(root, &entry, out);
            }
        }
    }

    let mut result = Vec::new();
    visit(root, root, &mut result);
    result
}

#[cfg(unix)]
fn symlink<P: AsRef<Path>, Q: AsRef<Path>>(original: P, link: Q) {
    std::os::unix::fs::symlink(original, link).unwrap();
}

#[cfg(not(unix))]
fn symlink<P: AsRef<Path>, Q: AsRef<Path>>(_original: P, _link: Q) {
    panic!("symlink tests require unix")
}
