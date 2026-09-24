use std::{
    fs,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use ghostty_wall::init::{InitError, InitPaths, dry_run, init, init_repair};

#[test]
fn dry_run_reports_paths_without_writing() {
    let paths = test_paths("dry-run");
    let report = dry_run(&paths).expect("dry run works");

    assert_eq!(report.capabilities, "requires-runtime-probe");
    assert_eq!(report.capability_probes.len(), 5);
    assert!(
        report
            .capability_probes
            .iter()
            .all(|probe| probe.status == "requires-runtime-probe")
    );
    assert!(
        report
            .mutations
            .iter()
            .any(|item| item.contains("config.toml"))
    );
    assert!(
        report
            .mutations
            .iter()
            .any(|item| item.contains("profiles/welcome.toml"))
    );
    assert!(
        report
            .mutations
            .iter()
            .any(|item| item.contains("profiles/welcome.png"))
    );
    assert!(!paths.home.exists());
}

#[test]
fn dry_run_resolves_existing_symlink_ancestor_without_creating_missing_parent() {
    let paths = test_paths("dry-run-link");
    let real = paths.home.join("real");
    fs::create_dir_all(&real).unwrap();
    symlink(&real, paths.xdg_config_home.as_ref().unwrap());
    let report = dry_run(&paths).unwrap();
    assert_eq!(report.managed_root, real.join("ghostty/ghostty-wall"));
    assert!(!real.join("ghostty").exists());
}

#[test]
fn dry_run_inspects_existing_layout_and_effective_hook_without_writing() {
    let paths = test_paths("dry-run-existing");
    let first = init(&paths).unwrap();
    let before = fs::metadata(paths.managed_root().join("state.lock"))
        .unwrap()
        .modified()
        .unwrap();
    let report = dry_run(&paths).unwrap();
    assert_eq!(report.root_config, first.root_config);
    assert_eq!(report.hook_state, "present");
    assert!(report.mutations.is_empty());
    assert!(
        report
            .layout
            .iter()
            .any(|item| item.path.ends_with("state.lock") && item.status == "valid")
    );
    assert_eq!(
        fs::metadata(paths.managed_root().join("state.lock"))
            .unwrap()
            .modified()
            .unwrap(),
        before
    );
}

#[test]
fn dry_run_reports_unsafe_component_without_mutation() {
    let paths = test_paths("dry-run-unsafe");
    let root = paths.managed_root();
    fs::create_dir_all(&root).unwrap();
    symlink(Path::new("/tmp"), root.join("profiles"));
    let report = dry_run(&paths).unwrap();
    assert!(
        report
            .layout
            .iter()
            .any(|entry| entry.path.ends_with("profiles") && entry.status == "unsafe")
    );
    assert!(!report.problems.is_empty());
    assert!(
        fs::symlink_metadata(root.join("profiles"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[test]
fn init_creates_layout_default_intent_marker_and_hook() {
    let paths = test_paths("creates-layout");
    let report = init(&paths).expect("init succeeds");
    let root = paths.managed_root();

    for relative in [
        "profiles",
        "assets/sha256",
        "environments",
        "history/activations",
        "cache",
    ] {
        assert!(root.join(relative).is_dir(), "missing {relative}");
    }
    assert_eq!(
        fs::read_to_string(root.join("config.toml")).unwrap(),
        "schema_version = 1\n\n[sources.welcome]\nkind = \"local-directory\"\npath = \"profiles\"\n"
    );
    assert!(root.join("profiles/welcome.toml").is_file());
    assert_eq!(
        fs::read(root.join("profiles/welcome.png")).unwrap(),
        include_bytes!("../media/welcome.png")
    );
    assert!(root.join("state.lock").is_file());
    assert!(
        fs::read_to_string(report.root_config)
            .unwrap()
            .contains("config-file = ?")
    );
}

#[test]
fn published_init_respects_exclusive_state_lock() {
    use std::os::fd::AsRawFd;
    let paths = test_paths("locked-init");
    init(&paths).unwrap();
    let held = fs::File::open(paths.managed_root().join("state.lock")).unwrap();
    assert_eq!(
        unsafe { libc::flock(held.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
        0
    );
    assert!(init(&paths).is_err());
    assert!(init_repair(&paths).is_err());
}

#[test]
fn repeated_init_is_noop_for_existing_files() {
    let paths = test_paths("idempotent");
    init(&paths).expect("first init succeeds");
    let config = paths.managed_root().join("config.toml");
    let before = fs::metadata(&config).unwrap().modified().unwrap();
    let root_before = fs::metadata(paths.managed_root())
        .unwrap()
        .modified()
        .unwrap();
    let hook = paths
        .xdg_config_home
        .as_ref()
        .unwrap()
        .join("ghostty/config.ghostty");
    let hook_before = fs::metadata(&hook).unwrap().modified().unwrap();

    let report = init(&paths).expect("second init succeeds");
    let after = fs::metadata(config).unwrap().modified().unwrap();

    assert!(report.mutations.is_empty());
    assert_eq!(before, after);
    assert_eq!(
        fs::metadata(paths.managed_root())
            .unwrap()
            .modified()
            .unwrap(),
        root_before
    );
    assert_eq!(fs::metadata(hook).unwrap().modified().unwrap(), hook_before);
}

#[test]
fn existing_install_never_recreates_or_overwrites_example() {
    let paths = test_paths("edited-example");
    init(&paths).unwrap();
    let root = paths.managed_root();
    fs::write(root.join("profiles/welcome.toml"), "schema_version = 1\n").unwrap();
    fs::remove_file(root.join("profiles/welcome.png")).unwrap();

    assert!(init(&paths).unwrap().mutations.is_empty());
    assert_eq!(
        fs::read_to_string(root.join("profiles/welcome.toml")).unwrap(),
        "schema_version = 1\n"
    );
    assert!(!root.join("profiles/welcome.png").exists());
}

#[test]
fn init_canonicalizes_user_symlink_ancestors() {
    let paths = test_paths("ancestor-link");
    let real = paths.home.join("real");
    fs::create_dir_all(&real).unwrap();
    symlink(&real, paths.xdg_config_home.as_ref().unwrap());
    let report = init(&paths).unwrap();
    assert_eq!(report.managed_root, real.join("ghostty/ghostty-wall"));
    assert!(real.join("ghostty/ghostty-wall/state.lock").exists());
    assert!(
        fs::read_to_string(report.root_config)
            .unwrap()
            .contains(&format!(
                "?{}",
                report.managed_root.join("current.ghostty").display()
            ))
    );
}

#[test]
fn root_config_symlink_is_preserved() {
    let paths = test_paths("symlink-root-config");
    let ghostty_dir = paths.xdg_config_home.as_ref().unwrap().join("ghostty");
    fs::create_dir_all(&ghostty_dir).unwrap();
    let target = paths.home.join("dotfiles/config.ghostty");
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(&target, "font-size = 12\n").unwrap();
    symlink(&target, ghostty_dir.join("config.ghostty"));

    init(&paths).expect("init follows root config symlink safely");

    assert!(
        fs::symlink_metadata(ghostty_dir.join("config.ghostty"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert!(
        fs::read_to_string(target)
            .unwrap()
            .contains("config-file = ?")
    );
}

#[test]
fn managed_symlink_fails_before_hook() {
    let paths = test_paths("managed-symlink");
    let root = paths.managed_root();
    fs::create_dir_all(&root).unwrap();
    symlink(Path::new("/tmp"), root.join("profiles"));

    let err = init(&paths).unwrap_err();

    assert!(matches!(err, InitError::Symlink(_)));
    assert!(!paths.ghostty_config().exists());
}

#[test]
fn init_uses_last_existing_root_config_and_preserves_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let paths = test_paths("root-precedence");
    let first = paths.ghostty_config();
    let last = first.with_file_name("config");
    fs::create_dir_all(first.parent().unwrap()).unwrap();
    fs::write(&first, "font-size = 12\n").unwrap();
    fs::write(&last, "font-size = 13\n").unwrap();
    fs::set_permissions(&last, fs::Permissions::from_mode(0o640)).unwrap();
    let report = init(&paths).unwrap();
    assert_eq!(report.root_config, last);
    assert_eq!(fs::read_to_string(first).unwrap(), "font-size = 12\n");
    assert_eq!(
        fs::metadata(last).unwrap().permissions().mode() & 0o777,
        0o640
    );
}

#[test]
fn equivalent_hook_does_not_duplicate_and_duplicates_require_repair() {
    let paths = test_paths("equivalent-hook");
    let report = init(&paths).unwrap();
    let projection = paths.managed_root().join("current.ghostty");
    fs::write(
        &report.root_config,
        format!(
            "config-file   =   ?{}/./current.ghostty\n",
            paths.managed_root().display()
        ),
    )
    .unwrap();
    assert!(init(&paths).unwrap().mutations.is_empty());
    fs::write(
        &report.root_config,
        format!(
            "config-file = ?{}\nconfig-file = ?{}\n",
            projection.display(),
            projection.display()
        ),
    )
    .unwrap();
    assert!(matches!(init(&paths), Err(InitError::RepairRequired(_))));
}

#[test]
fn repair_normalizes_duplicate_hook_without_touching_unrelated_includes() {
    let paths = test_paths("repair-hooks");
    let report = init(&paths).unwrap();
    let projection = paths.managed_root().join("current.ghostty");
    fs::write(
        &report.root_config,
        format!(
            "config-file = ?{}\nconfig-file = ?{}\nconfig-file = /other.conf\n",
            projection.display(),
            projection.display()
        ),
    )
    .unwrap();
    assert!(matches!(init(&paths), Err(InitError::RepairRequired(_))));
    init_repair(&paths).unwrap();
    let text = fs::read_to_string(&report.root_config).unwrap();
    assert_eq!(text.matches("config-file = ?").count(), 1);
    assert!(text.contains("config-file = /other.conf"));
    assert!(init(&paths).unwrap().mutations.is_empty());
}

#[test]
fn repair_moves_stale_hook_to_effective_root() {
    let paths = test_paths("stale-hook");
    let report = init(&paths).unwrap();
    let first = report.root_config;
    let last = first.with_file_name("config");
    fs::write(&last, "font-size = 14\n").unwrap();
    assert!(matches!(init(&paths), Err(InitError::RepairRequired(_))));
    init_repair(&paths).unwrap();
    assert!(
        !fs::read_to_string(&first)
            .unwrap()
            .contains("config-file = ?")
    );
    assert_eq!(
        fs::read_to_string(&last)
            .unwrap()
            .matches("config-file = ?")
            .count(),
        1
    );
}

#[test]
fn repair_tightens_unsafe_managed_permissions_but_never_changes_owner() {
    use std::os::unix::fs::PermissionsExt;
    let paths = test_paths("repair-mode");
    init(&paths).unwrap();
    let config = paths.managed_root().join("config.toml");
    fs::set_permissions(&config, fs::Permissions::from_mode(0o666)).unwrap();
    assert!(init(&paths).is_err());
    init_repair(&paths).unwrap();
    assert_eq!(
        fs::metadata(&config).unwrap().permissions().mode() & 0o777,
        0o644
    );
}

#[test]
fn repair_completes_pristine_interrupted_first_init_only() {
    let paths = test_paths("interrupted-first-init");
    let root = paths.managed_root();
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join(".init-in-progress"), b"").unwrap();
    fs::write(root.join(".config.toml-in-progress"), b"partial").unwrap();
    init_repair(&paths).unwrap();
    assert!(!root.join(".config.toml-in-progress").exists());
    assert!(root.join("state.lock").is_file());
    assert!(!root.join(".init-in-progress").exists());
    assert!(root.join("profiles/welcome.toml").is_file());
    assert!(root.join("profiles/welcome.png").is_file());

    let partial = test_paths("interrupted-with-example");
    let partial_root = partial.managed_root();
    fs::create_dir_all(partial_root.join("profiles")).unwrap();
    fs::write(partial_root.join(".init-in-progress"), b"").unwrap();
    fs::write(
        partial_root.join("profiles/welcome.png"),
        include_bytes!("../media/welcome.png"),
    )
    .unwrap();
    init_repair(&partial).unwrap();
    assert!(partial_root.join("profiles/welcome.toml").is_file());

    let altered = test_paths("interrupted-with-altered-example");
    let altered_root = altered.managed_root();
    fs::create_dir_all(altered_root.join("profiles")).unwrap();
    fs::write(altered_root.join(".init-in-progress"), b"").unwrap();
    fs::write(altered_root.join("profiles/welcome.png"), b"user data").unwrap();
    assert!(init_repair(&altered).is_err());
    assert_eq!(
        fs::read(altered_root.join("profiles/welcome.png")).unwrap(),
        b"user data"
    );

    let damaged = test_paths("interrupted-with-records");
    let root = damaged.managed_root();
    fs::create_dir_all(root.join("profiles")).unwrap();
    fs::write(root.join("profiles/night.toml"), "schema_version = 1\n").unwrap();
    fs::write(root.join(".init-in-progress"), b"").unwrap();
    assert!(init_repair(&damaged).is_err());
    assert!(!root.join("state.lock").exists());
}

#[test]
fn interrupted_init_repair_never_publishes_unsafe_projection() {
    let paths = test_paths("interrupted-projection-symlink");
    let root = paths.managed_root();
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join(".init-in-progress"), b"").unwrap();
    symlink(Path::new("/tmp"), root.join("current.ghostty"));
    assert!(matches!(init_repair(&paths), Err(InitError::Symlink(_))));
    assert!(!root.join("state.lock").exists());
    assert!(!paths.ghostty_config().exists());
}

#[test]
fn managed_fifo_is_rejected_without_opening_it() {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    let paths = test_paths("managed-fifo");
    init(&paths).unwrap();
    let config = paths.managed_root().join("config.toml");
    fs::remove_file(&config).unwrap();
    let name = CString::new(config.as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    assert!(matches!(init(&paths), Err(InitError::WrongKind(_))));
    assert!(matches!(init_repair(&paths), Err(InitError::WrongKind(_))));
}

#[test]
fn invalid_existing_intent_is_preserved_without_hook_mutation() {
    let paths = test_paths("invalid-intent");
    let report = init(&paths).unwrap();
    let before = fs::read(&report.root_config).unwrap();
    let config = paths.managed_root().join("config.toml");
    fs::write(&config, b"schema_version = 7\n").unwrap();
    assert!(matches!(init(&paths), Err(InitError::InvalidIntent(_))));
    assert_eq!(fs::read(report.root_config).unwrap(), before);
    assert_eq!(fs::read(config).unwrap(), b"schema_version = 7\n");
}

#[test]
fn failed_hook_install_rolls_back_created_managed_layout_only() {
    use std::os::unix::fs::PermissionsExt;
    let paths = test_paths("hook-failure-rollback");
    let config = paths.ghostty_config();
    fs::create_dir_all(config.parent().unwrap()).unwrap();
    let target = paths.home.join("dotfiles/config.ghostty");
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(&target, "font-size = 12\n").unwrap();
    symlink(&target, &config);
    let parent = target.parent().unwrap();
    fs::set_permissions(parent, fs::Permissions::from_mode(0o500)).unwrap();
    let result = init(&paths);
    fs::set_permissions(parent, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(result.is_err());
    assert!(!paths.managed_root().exists());
    assert_eq!(fs::read_to_string(target).unwrap(), "font-size = 12\n");
}

fn test_paths(name: &str) -> InitPaths {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let home = std::env::temp_dir().join(format!("ghostty-wall-{name}-{unique}"));
    InitPaths {
        xdg_config_home: Some(home.join("xdg")),
        home,
    }
}

trait TestPaths {
    fn ghostty_config(&self) -> std::path::PathBuf;
}

impl TestPaths for InitPaths {
    fn ghostty_config(&self) -> std::path::PathBuf {
        self.xdg_config_home
            .as_ref()
            .unwrap()
            .join("ghostty/config.ghostty")
    }
}

#[cfg(unix)]
fn symlink<P: AsRef<Path>, Q: AsRef<Path>>(original: P, link: Q) {
    std::os::unix::fs::symlink(original, link).unwrap();
}

#[cfg(not(unix))]
fn symlink<P: AsRef<Path>, Q: AsRef<Path>>(_original: P, _link: Q) {
    panic!("symlink tests require unix")
}
