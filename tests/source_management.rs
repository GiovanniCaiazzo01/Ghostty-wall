//! Public Source maintenance regressions; all commands use a disposable HOME.
use ghostty_wall::codec::intent::parse_config_toml;
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};

struct Fixture {
    home: PathBuf,
    root: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let home = std::env::temp_dir().join(format!(
            "ghostty-source-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&home).unwrap();
        let root = home.join("config/ghostty/ghostty-wall");
        let fixture = Self { home, root };
        fixture.ok(&["init"], "");
        fixture
    }
    fn run(&self, args: &[&str], input: &str) -> Output {
        let mut child = Command::new(env!("CARGO_BIN_EXE_ghostty-wall"))
            .args(args)
            .env("HOME", &self.home)
            .env("XDG_CONFIG_HOME", self.home.join("config"))
            .env("XDG_RUNTIME_DIR", self.home.join("runtime"))
            .env("PATH", "/nonexistent")
            .env_remove("DBUS_SESSION_BUS_ADDRESS")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    }
    fn ok(&self, args: &[&str], input: &str) -> String {
        let result = self.run(args, input);
        assert!(
            result.status.success(),
            "{:?}: {}",
            args,
            String::from_utf8_lossy(&result.stderr)
        );
        String::from_utf8(result.stdout).unwrap()
    }
    fn fail(&self, args: &[&str], message: &str) {
        let result = self.run(args, "");
        assert!(!result.status.success(), "unexpected success: {args:?}");
        assert!(
            String::from_utf8_lossy(&result.stderr).contains(message),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    fn profile(&self, name: &str, source: &str) {
        fs::write(self.root.join(format!("profiles/{name}.toml")), format!("schema_version = 1\n[wallpaper]\nmode = 'source'\nsource = '{source}'\nselection = 'path'\npath = 'sky.png'\n")).unwrap();
    }
    fn add_local(&self, id: &str, path: &Path) {
        self.ok(&["source", "add", id, "local", path.to_str().unwrap()], "");
    }
    fn state(&self) -> BTreeMap<PathBuf, Vec<u8>> {
        fn walk(path: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
            for item in fs::read_dir(path).unwrap() {
                let path = item.unwrap().path();
                if path.is_dir() {
                    walk(&path, files);
                } else {
                    files.insert(path.clone(), fs::read(path).unwrap());
                }
            }
        }
        let mut files = BTreeMap::new();
        walk(&self.home, &mut files);
        files.remove(&self.root.join("config.toml"));
        files
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.home).unwrap();
    }
}

#[test]
fn moved_directory_keeps_id_profiles_comments_and_active_state() {
    let f = Fixture::new();
    let old = f.home.join("old");
    let moved = f.home.join("moved");
    fs::create_dir(&old).unwrap();
    fs::write(old.join("sky.png"), include_bytes!("fixtures/white.png")).unwrap();
    f.add_local("photos", &old);
    f.profile("zebra", "photos");
    f.profile("alpha", "photos");
    f.ok(&["apply", "alpha"], "");
    fs::rename(&old, &moved).unwrap();
    let config = f.root.join("config.toml");
    let original = fs::read_to_string(&config)
        .unwrap()
        .replace("[sources.photos]", "# Personal source\n[sources.photos]")
        .replace(
            &format!("path = \"{}\"", old.display()),
            &format!("path = \"{}\" # keep inline note", old.display()),
        );
    fs::write(&config, &original).unwrap();
    let state = f.state();
    assert!(
        f.ok(&["source", "show", "photos"], "")
            .contains("Profiles: alpha, zebra.")
    );
    let output = f.ok(
        &["source", "edit", "photos", "local", moved.to_str().unwrap()],
        "",
    );
    assert!(output.contains("Profiles: alpha, zebra."));
    assert!(output.contains("terminal unchanged"));
    let changed = fs::read_to_string(&config).unwrap();
    assert_eq!(
        changed,
        original.replace(old.to_str().unwrap(), moved.to_str().unwrap())
    );
    assert_eq!(f.state(), state);
    assert!(
        f.ok(&["plan", "alpha", "--json"], "")
            .contains(moved.to_str().unwrap())
    );
    assert!(
        f.ok(&["source", "check", "photos"], "")
            .contains("available, 1 PNG/JPEG")
    );
    assert_eq!(f.state(), state);
    assert!(
        f.ok(&["source", "list"], "")
            .contains("photos local\nwelcome local\n")
    );
}

#[test]
fn github_complete_edits_validate_and_preserve_other_sources_and_add_contract() {
    let f = Fixture::new();
    f.ok(
        &[
            "source",
            "add",
            "remote",
            "github",
            "owner/repo",
            "--ref",
            "main",
            "--path",
            "wallpapers",
        ],
        "",
    );
    let config = f.root.join("config.toml");
    let original = fs::read(&config).unwrap();
    f.ok(
        &[
            "source",
            "add",
            "remote",
            "github",
            "owner/repo",
            "--ref",
            "main",
            "--path",
            "wallpapers",
        ],
        "",
    );
    assert_eq!(fs::read(&config).unwrap(), original);
    f.fail(
        &["source", "add", "remote", "github", "other/repo"],
        "already differs",
    );
    for args in [
        vec!["source", "edit", "remote", "local", "/tmp"],
        vec!["source", "edit", "remote", "github", "invalid"],
        vec![
            "source",
            "edit",
            "remote",
            "github",
            "owner/repo",
            "--path",
            "../escape",
        ],
        vec![
            "source",
            "edit",
            "remote",
            "github",
            "owner/repo",
            "--ref",
            "",
        ],
        vec!["source", "edit", "remote", "github", "owner/repo", "--ref"],
        vec![
            "source",
            "edit",
            "remote",
            "github",
            "owner/repo",
            "--ref",
            "main",
            "--ref",
            "dev",
        ],
    ] {
        assert!(!f.run(&args, "").status.success());
        assert_eq!(fs::read(&config).unwrap(), original);
    }
    f.ok(
        &[
            "source",
            "edit",
            "remote",
            "github",
            "other/repo",
            "--ref",
            "feature/sky",
            "--path",
            "images",
        ],
        "",
    );
    let text = fs::read_to_string(&config).unwrap();
    assert!(text.contains("feature/sky") && text.contains("images"));
    f.ok(&["source", "edit", "remote", "github", "third/repo"], "");
    let revised = parse_config_toml(&fs::read_to_string(&config).unwrap()).unwrap();
    let previous = parse_config_toml(std::str::from_utf8(&original).unwrap()).unwrap();
    assert_eq!(
        revised
            .sources
            .iter()
            .find(|(id, _)| id.as_str() == "welcome"),
        previous
            .sources
            .iter()
            .find(|(id, _)| id.as_str() == "welcome")
    );
    assert_eq!(
        revised
            .sources
            .iter()
            .find(|(id, _)| id.as_str() == "remote")
            .unwrap()
            .1,
        ghostty_wall::domain::SourceIntent::Github {
            repository: "third/repo".into(),
            reference: None,
            path: None
        }
    );
    assert!(f.ok(&["source", "--help"], "").contains("Omitted GitHub"));
}

#[test]
fn removal_refuses_usage_and_incomplete_dependency_analysis() {
    let f = Fixture::new();
    f.add_local("unused", &f.home.join("missing"));
    let config = fs::read(f.root.join("config.toml")).unwrap();
    f.fail(
        &["source", "remove", "welcome"],
        "used by Profiles: welcome",
    );
    let broken = f.root.join("profiles/broken.toml");
    for text in [
        "not toml",
        "schema_version = 1\n[wallpaper]\nmode = 'source'\nsource = 'unknown'\nselection = 'random'\n",
    ] {
        fs::write(&broken, text).unwrap();
        f.fail(
            &["source", "remove", "unused"],
            "dependency analysis incomplete",
        );
        f.fail(
            &["source", "show", "unused"],
            "dependency analysis incomplete",
        );
        f.fail(
            &["source", "edit", "unused", "local", "/new"],
            "dependency analysis incomplete",
        );
    }
    fs::remove_file(&broken).unwrap();
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(f.home.join("missing-profile"), &broken).unwrap();
        f.fail(&["source", "remove", "unused"], "not a regular file");
        fs::remove_file(&broken).unwrap();
    }
    fs::create_dir(&broken).unwrap();
    f.fail(&["source", "remove", "unused"], "not a regular file");
    fs::remove_dir(&broken).unwrap();
    assert_eq!(fs::read(f.root.join("config.toml")).unwrap(), config);
    let before = f.state();
    for input in ["", "\n", "n\n", "cancel\n", "yes\n"] {
        assert!(
            f.ok(&["source", "remove", "unused"], input)
                .contains("cancelled")
        );
        assert_eq!(fs::read(f.root.join("config.toml")).unwrap(), config);
        assert_eq!(f.state(), before);
    }
    f.ok(&["source", "remove", "unused"], "y\n");
    assert!(!f.ok(&["source", "list"], "").contains("unused"));
    assert_eq!(f.state(), before);
}

#[test]
fn check_is_listing_only_and_unavailable_sources_can_be_saved() {
    let f = Fixture::new();
    let directory = f.home.join("pictures");
    fs::create_dir(&directory).unwrap();
    f.add_local("photos", &directory);
    let before = f.state();
    let config = fs::read(f.root.join("config.toml")).unwrap();
    let empty = f.run(&["source", "check", "photos"], "");
    assert_eq!(empty.status.code(), Some(4));
    assert!(String::from_utf8_lossy(&empty.stdout).contains("empty Candidate Set"));
    fs::write(directory.join("broken.PNG"), b"not decoded by check").unwrap();
    assert!(
        f.ok(&["source", "check", "photos"], "")
            .contains("available, 1 PNG/JPEG")
    );
    fs::remove_file(directory.join("broken.PNG")).unwrap();
    assert_eq!(f.state(), before);
    assert_eq!(fs::read(f.root.join("config.toml")).unwrap(), config);
    f.ok(
        &[
            "source",
            "edit",
            "photos",
            "local",
            "/temporarily-unavailable-ghostty-wall",
        ],
        "",
    );
    let unavailable = f.run(&["source", "check", "photos"], "");
    assert_eq!(unavailable.status.code(), Some(4));
    assert!(String::from_utf8_lossy(&unavailable.stdout).contains("unavailable"));
    f.fail(&["source", "edit", "photos", "local", ""], "nonempty");
    f.fail(&["source", "show", "absent"], "unknown Source");
}

#[test]
fn ownership_invariants_block_relocating_managed_image_source() {
    let f = Fixture::new();
    f.ok(
        &[
            "new",
            "owned",
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/white.png"),
        ],
        "",
    );
    let config = fs::read(f.root.join("config.toml")).unwrap();
    let parsed = parse_config_toml(std::str::from_utf8(&config).unwrap()).unwrap();
    let source = parsed.sources.iter().find(|(_, source)| matches!(source, ghostty_wall::domain::SourceIntent::LocalDirectory { path } if path == "profiles")).unwrap().0.as_str();
    f.fail(
        &["source", "edit", source, "local", "/somewhere-else"],
        "owned image requires local profiles Source",
    );
    assert_eq!(fs::read(f.root.join("config.toml")).unwrap(), config);
}

#[test]
fn committed_environment_replays_after_source_removal_without_original_images() {
    let f = Fixture::new();
    let directory = f.home.join("pictures");
    fs::create_dir(&directory).unwrap();
    fs::write(
        directory.join("sky.png"),
        include_bytes!("fixtures/white.png"),
    )
    .unwrap();
    f.add_local("photos", &directory);
    f.profile("sky", "photos");
    f.ok(&["apply", "sky"], "");
    f.ok(&["apply", "welcome"], "");
    f.ok(&["delete", "sky"], "y\n");
    let before = f.state();
    f.ok(&["source", "remove", "photos"], "y\n");
    assert_eq!(f.state(), before);
    assert!(directory.join("sky.png").is_file());
    fs::remove_dir_all(&directory).unwrap();
    f.ok(&["previous"], "");
    f.ok(&["doctor"], "");
    assert!(
        fs::read_to_string(f.root.join("current.ghostty"))
            .unwrap()
            .contains("assets/sha256")
    );
}

#[test]
fn line_browser_exposes_source_maintenance_and_cancels_without_writing() {
    let f = Fixture::new();
    f.add_local("photos", &f.home.join("pictures"));
    let before = fs::read(f.root.join("config.toml")).unwrap();
    let output = f.ok(&["tui"], "S\nphotos\nE\nphotos\n/new\nn\nZ\nphotos\nn\nq\n");
    assert!(output.contains("Source photos (local)"));
    assert_eq!(fs::read(f.root.join("config.toml")).unwrap(), before);
    f.ok(&["tui"], "E\nphotos\n/new\ny\nZ\nphotos\ny\nq\n");
    assert!(!f.ok(&["source", "list"], "").contains("photos"));
}
