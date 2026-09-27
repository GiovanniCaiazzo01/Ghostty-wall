use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Sandbox(PathBuf);
impl Sandbox {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "ghostty-wall-rfc-qa-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn root(&self) -> PathBuf {
        self.0.join("config/ghostty/ghostty-wall")
    }
    fn run(&self, args: &[&str]) -> Output {
        self.run_input(args, b"")
    }
    fn run_input(&self, args: &[&str], input: &[u8]) -> Output {
        let mut child = Command::new(env!("CARGO_BIN_EXE_ghostty-wall"))
            .args(args)
            .env("HOME", &self.0)
            .env("XDG_CONFIG_HOME", self.0.join("config"))
            .env("XDG_RUNTIME_DIR", self.0.join("runtime"))
            .env_remove("DBUS_SESSION_BUS_ADDRESS")
            // Disable all live reload adapters: an isolated HOME alone does not
            // isolate the user's systemd/D-Bus session or running Ghostty.
            .env("PATH", "/nonexistent")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(input).unwrap();
        child.wait_with_output().unwrap()
    }
    fn ok(&self, args: &[&str]) -> Output {
        let output = self.run_input(
            args,
            if args.first() == Some(&"delete") {
                b"y\n"
            } else {
                b""
            },
        );
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }
}
impl Drop for Sandbox {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn bytes(path: impl AsRef<Path>) -> Vec<u8> {
    fs::read(path).unwrap()
}

#[test]
fn installed_welcome_cannot_be_removed_by_rename() {
    let s = Sandbox::new();
    s.ok(&["init"]);
    let before = bytes(s.root().join("profiles/welcome.toml"));
    let config = bytes(s.root().join("config.toml"));
    let renamed = s.run(&["rename", "welcome", "away"]);
    assert!(
        !renamed.status.success(),
        "rename must not remove the installed, reserved Welcome Profile"
    );
    assert_eq!(bytes(s.root().join("profiles/welcome.toml")), before);
    assert_eq!(bytes(s.root().join("config.toml")), config);
    assert!(!s.root().join("profiles/away.toml").exists());
}

#[test]
fn active_profile_cannot_be_renamed_then_deleted_without_fallback() {
    let s = Sandbox::new();
    s.ok(&["init"]);
    let profile = s.root().join("profiles/night.toml");
    fs::write(
        &profile,
        "schema_version = 1\n[terminal]\nfont_size = 13.0\n",
    )
    .unwrap();
    s.ok(&["apply", "night"]);
    let projection = bytes(s.root().join("current.ghostty"));
    let history = bytes(
        s.root()
            .join("history/activations/act-v1-0000000000000001.json"),
    );
    let renamed = s.run(&["rename", "night", "evening"]);
    assert!(
        !renamed.status.success(),
        "active rename bypassed fallback guard"
    );
    assert!(String::from_utf8_lossy(&renamed.stderr).contains("Apply another Profile"));
    assert!(s.root().join("profiles/night.toml").exists());
    assert!(!s.root().join("profiles/evening.toml").exists());
    let cancelled = s.run(&["delete", "night"]);
    assert!(cancelled.status.success());
    assert!(String::from_utf8_lossy(&cancelled.stdout).contains("Deletion cancelled"));
    assert_eq!(bytes(s.root().join("current.ghostty")), projection);
    assert_eq!(
        bytes(
            s.root()
                .join("history/activations/act-v1-0000000000000001.json")
        ),
        history
    );
}

#[test]
fn imported_image_full_path_and_cancelled_delete_preserve_original_and_durable_data() {
    let s = Sandbox::new();
    s.ok(&["init"]);
    let original = s.0.join("my-image.png");
    fs::copy(
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/white.png"),
        &original,
    )
    .unwrap();
    let image_before = bytes(&original);
    s.ok(&["new", "mine", original.to_str().unwrap()]);
    let profile = s.root().join("profiles/mine.toml");
    let intent = fs::read_to_string(&profile).unwrap();
    assert!(intent.contains("schema_version = 2"));
    assert!(intent.contains("owned_sha256 = \""));
    assert!(intent.contains("path = \"mine.png\""));
    assert!(!s.root().join("profiles/mine/profile.toml").exists());
    assert_eq!(bytes(s.root().join("profiles/mine.png")), image_before);
    s.ok(&["plan", "mine", "--json"]);
    s.ok(&["apply", "mine"]);
    let projection = bytes(s.root().join("current.ghostty"));
    let cancelled = s.run(&["delete", "mine"]);
    assert!(cancelled.status.success());
    assert!(String::from_utf8_lossy(&cancelled.stdout).contains("Deletion cancelled"));
    assert_eq!(bytes(&profile), intent.as_bytes());
    assert_eq!(bytes(s.root().join("current.ghostty")), projection);
    assert_eq!(
        fs::read_dir(s.root().join("history/activations"))
            .unwrap()
            .count(),
        1
    );
    s.ok(&["apply", "welcome"]);
    s.ok(&["delete", "mine"]);
    assert!(!profile.exists());
    assert_eq!(bytes(&original), image_before);
    assert!(!s.root().join("profiles/mine.png").exists());
    s.ok(&["previous"]);
    assert_eq!(
        fs::read_dir(s.root().join("history/activations"))
            .unwrap()
            .count(),
        3
    );
    s.ok(&["doctor"]);
}

#[test]
fn old_install_without_welcome_and_corrupt_history_do_not_silently_repair() {
    let s = Sandbox::new();
    s.ok(&["init"]);
    fs::write(
        s.root().join("config.toml"),
        "schema_version = 1\n[sources]\n",
    )
    .unwrap();
    fs::remove_file(s.root().join("profiles/welcome.toml")).unwrap();
    fs::remove_file(s.root().join("profiles/welcome.png")).unwrap();
    let profile = s.root().join("profiles/legacy.toml");
    fs::write(
        &profile,
        "schema_version = 1\n[terminal]\nfont_size = 13.0\n",
    )
    .unwrap();
    s.ok(&["init"]);
    assert!(!s.root().join("profiles/welcome.toml").exists());
    s.ok(&["apply", "legacy"]);
    let projection = bytes(s.root().join("current.ghostty"));
    let activation = s
        .root()
        .join("history/activations/act-v1-0000000000000001.json");
    let mut record: serde_json::Value = serde_json::from_slice(&bytes(&activation)).unwrap();
    record["record_schema_version"] = 999.into();
    fs::write(&activation, serde_json::to_vec(&record).unwrap()).unwrap();
    let bad_history = bytes(&activation);
    assert!(!s.run(&["delete", "legacy"]).status.success());
    assert!(!s.run(&["rename", "legacy", "replacement"]).status.success());
    assert!(profile.exists());
    assert!(!s.root().join("profiles/replacement.toml").exists());
    assert_eq!(bytes(&activation), bad_history);
    assert_eq!(bytes(s.root().join("current.ghostty")), projection);
    assert!(!s.root().join("profiles/welcome.toml").exists());
}

#[test]
fn valid_old_install_without_welcome_guards_active_intent_without_bootstrapping() {
    let s = Sandbox::new();
    s.ok(&["init"]);
    let root = s.root();
    fs::write(root.join("config.toml"), "schema_version = 1\n[sources]\n").unwrap();
    fs::remove_file(root.join("profiles/welcome.toml")).unwrap();
    fs::remove_file(root.join("profiles/welcome.png")).unwrap();
    let legacy = root.join("profiles/legacy.toml");
    fs::write(
        &legacy,
        "schema_version = 1\n[terminal]\nfont_size = 12.5\n",
    )
    .unwrap();
    s.ok(&["init"]);
    s.ok(&["apply", "legacy"]);
    let projection = bytes(root.join("current.ghostty"));
    let record = root.join("history/activations/act-v1-0000000000000001.json");
    let history = bytes(&record);
    let intent = bytes(&legacy);
    for args in [
        ["delete", "legacy"].as_slice(),
        &["rename", "legacy", "renamed"],
    ] {
        let result = s.run_input(args, b"y\n");
        assert!(!result.status.success(), "{args:?} bypassed active guard");
        assert!(String::from_utf8_lossy(&result.stderr).contains("Apply another Profile"));
        assert_eq!(bytes(&legacy), intent);
        assert_eq!(bytes(&record), history);
        assert_eq!(bytes(root.join("current.ghostty")), projection);
    }
    assert!(!root.join("profiles/welcome.toml").exists());
    assert!(!root.join("profiles/renamed.toml").exists());
    assert_eq!(
        fs::read_dir(root.join("history/activations"))
            .unwrap()
            .count(),
        1
    );
}

#[test]
fn primary_create_entry_point_is_available_without_internal_file_editing() {
    let s = Sandbox::new();
    s.ok(&["init"]);
    let created = s.run(&["create", "trial"]);
    assert!(
        created.status.success(),
        "create trial must open guided flow: {}",
        String::from_utf8_lossy(&created.stderr)
    );
    let help = s.ok(&["--help"]);
    assert!(
        String::from_utf8_lossy(&help.stdout).contains("ghostty-wall create"),
        "the spec's primary create command is absent from CLI help"
    );
}

#[test]
fn invalid_import_and_existing_profile_collision_roll_back_without_touching_original() {
    let s = Sandbox::new();
    s.ok(&["init"]);
    let original = s.0.join("input.png");
    fs::copy(
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/white.png"),
        &original,
    )
    .unwrap();
    let source_bytes = bytes(&original);
    let root = s.root();
    let config = bytes(root.join("config.toml"));
    let bad = s.0.join("not-an-image.png");
    fs::write(&bad, b"not PNG bytes").unwrap();
    assert!(
        !s.run(&["new", "mine", bad.to_str().unwrap()])
            .status
            .success()
    );
    assert!(!root.join("profiles/mine.toml").exists());
    assert!(!root.join("profiles/mine.png").exists());
    s.ok(&["new", "mine", original.to_str().unwrap()]);
    let profile = bytes(root.join("profiles/mine.toml"));
    let image = bytes(root.join("profiles/mine.png"));
    let alternate = s.0.join("other.png");
    // Same valid format but different bytes from the image already imported.
    fs::copy(
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/palette.png"),
        &alternate,
    )
    .unwrap();
    let rejected = s.run(&["new", "mine", alternate.to_str().unwrap()]);
    assert!(!rejected.status.success());
    assert_eq!(bytes(root.join("profiles/mine.toml")), profile);
    assert_eq!(bytes(root.join("profiles/mine.png")), image);
    assert_eq!(bytes(&original), source_bytes);
    assert_eq!(bytes(root.join("config.toml")), config);
    assert_eq!(
        fs::read_dir(root.join("history/activations"))
            .unwrap()
            .count(),
        0
    );
    assert!(!root.join("current.ghostty").exists());
}

#[test]
fn preview_and_failed_apply_do_not_advance_activation_or_projection() {
    let s = Sandbox::new();
    s.ok(&["init"]);
    s.ok(&["apply", "welcome"]);
    let root = s.root();
    let projection = bytes(root.join("current.ghostty"));
    let activation = bytes(root.join("history/activations/act-v1-0000000000000001.json"));
    let original = s.0.join("input.png");
    fs::copy(
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/white.png"),
        &original,
    )
    .unwrap();
    s.ok(&["new", "mine", original.to_str().unwrap()]);
    let imported = bytes(root.join("profiles/mine.toml"));
    s.ok(&["preview", "mine"]);
    assert_eq!(bytes(root.join("current.ghostty")), projection);
    assert_eq!(
        fs::read_dir(root.join("history/activations"))
            .unwrap()
            .count(),
        1
    );
    // Removing the original must not affect the copy, but losing the managed
    // Source candidate must fail rather than commit a partial Activation.
    fs::remove_file(original).unwrap();
    s.ok(&["plan", "mine"]);
    fs::remove_file(root.join("profiles/mine.png")).unwrap();
    assert!(!s.run(&["apply", "mine"]).status.success());
    assert_eq!(bytes(root.join("profiles/mine.toml")), imported);
    assert_eq!(bytes(root.join("current.ghostty")), projection);
    assert_eq!(
        bytes(root.join("history/activations/act-v1-0000000000000001.json")),
        activation
    );
    assert_eq!(
        fs::read_dir(root.join("history/activations"))
            .unwrap()
            .count(),
        1
    );
}

#[test]
fn replay_does_not_authorize_profile_rename_or_delete() {
    let s = Sandbox::new();
    s.ok(&["init"]);
    let image = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/white.png");
    s.ok(&["new", "night", image]);
    s.ok(&["apply", "welcome"]);
    s.ok(&["apply", "night"]);
    s.ok(&["previous"]);
    let profile = s.root().join("profiles/night.toml");
    let before = bytes(&profile);
    let projection = bytes(s.root().join("current.ghostty"));
    for args in [["rename", "night", "dusk"].as_slice(), &["delete", "night"]] {
        let outcome = s.run(args);
        assert!(!outcome.status.success(), "{args:?} bypassed replay guard");
        assert!(String::from_utf8_lossy(&outcome.stderr).contains("History replay"));
    }
    assert_eq!(bytes(&profile), before);
    assert!(!s.root().join("profiles/dusk.toml").exists());
    assert_eq!(bytes(s.root().join("current.ghostty")), projection);
    assert_eq!(
        fs::read_dir(s.root().join("history/activations"))
            .unwrap()
            .count(),
        3
    );
    s.ok(&["apply", "welcome"]);
    s.ok(&["rename", "night", "dusk"]);
    s.ok(&["delete", "dusk"]);
    s.ok(&["doctor"]);
}
