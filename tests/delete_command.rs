use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};

use ghostty_wall::history::inspect_history;

struct Sandbox(tempfile::TempDir);
impl Sandbox {
    fn new() -> Self {
        let s = Self(tempfile::tempdir().unwrap());
        s.ok(&["init"], "");
        s
    }
    fn root(&self) -> PathBuf {
        self.0.path().join("config/ghostty/ghostty-wall")
    }
    fn run(&self, args: &[&str], input: &str) -> Output {
        let mut child = Command::new(env!("CARGO_BIN_EXE_ghostty-wall"))
            .args(args)
            .env_clear()
            .env("HOME", self.0.path())
            .env("XDG_CONFIG_HOME", self.0.path().join("config"))
            .env("XDG_RUNTIME_DIR", self.0.path().join("runtime"))
            .env("PATH", "/nonexistent")
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
            "{args:?}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        String::from_utf8(result.stdout).unwrap()
    }
    fn personal(&self) -> PathBuf {
        let original = self.0.path().join("original.png");
        fs::copy(
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/white.png"),
            &original,
        )
        .unwrap();
        self.ok(&["new", "boy", original.to_str().unwrap()], "");
        original
    }
}

fn files(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, result: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if entry.file_type().unwrap().is_dir() {
                walk(root, &path, result);
            } else if entry.file_type().unwrap().is_file() {
                result.insert(
                    path.strip_prefix(root).unwrap().to_owned(),
                    fs::read(&path).unwrap(),
                );
            }
        }
    }
    let mut result = BTreeMap::new();
    walk(root, root, &mut result);
    result
}

#[test]
fn named_target_confirmation_defaults_to_cancel_without_selection_or_mutation() {
    let s = Sandbox::new();
    s.personal();
    s.ok(&["apply", "boy"], "");
    let before = files(&s.root());
    for input in ["", "\n", "n\n", "cancel\n", "wrong\n", "\x1b\n"] {
        let output = s.ok(&["delete", "boy"], input);
        assert!(output.contains("Delete Profile boy?"));
        assert!(output.contains("profiles/boy.toml"));
        assert!(output.contains("boy.png"));
        assert!(output.contains("Cancel (default)"));
        assert!(output.contains("Deletion cancelled"));
        assert!(!output.contains("Select Profile"));
        assert_eq!(files(&s.root()), before);
    }
    let missing = s.run(&["delete", "missing"], "yes\n");
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("does not exist"));
    assert!(missing.stdout.is_empty());
    assert!(!s.run(&["delete", "../boy"], "yes\n").status.success());
    assert!(!s.run(&["delete", "boy", "--yes"], "yes\n").status.success());
    assert!(!s.run(&["delete", "welcome"], "yes\n").status.success());
    assert_eq!(files(&s.root()), before);
    let help = s.ok(&["delete", "--help"], "");
    for expected in [
        "delete [PROFILE]",
        "Cancel",
        "Welcome",
        "Durable Assets",
        "best-effort",
        ".tmp-delete-<token>/",
        "not automatically restored or cleaned up",
        "renewed ownership check keeps its image",
        "fresh confirmation",
    ] {
        assert!(help.contains(expected), "{expected}");
    }
}

#[test]
fn selector_marks_active_and_last_personal_deletion_commits_welcome_and_keeps_replay() {
    let s = Sandbox::new();
    let original = s.personal();
    let original_bytes = fs::read(&original).unwrap();
    s.ok(&["apply", "boy"], "");
    let root = s.root();
    let first_projection = fs::read(root.join("current.ghostty")).unwrap();
    let first = files(&root.join("history"));
    let assets = files(&root.join("assets"));
    let environments = files(&root.join("environments"));
    let before = files(&root);
    let selector = s.ok(&["delete"], "\n");
    assert!(selector.contains("boy [active]"));
    assert!(selector.contains("welcome (protected fallback)"));
    assert_eq!(files(&root), before);
    let output = s.ok(&["delete"], "1\ny\n");
    assert!(output.contains("Welcome Activation"));
    assert!(output.contains("committed before removal"));
    assert!(output.contains("reload: unavailable"));
    assert!(output.contains("Profile boy deleted"));
    assert!(!root.join("profiles/boy.toml").exists());
    assert!(!root.join("profiles/boy.png").exists());
    assert!(fs::read_dir(&root).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".tmp-delete-")
    }));
    assert_eq!(fs::read(original).unwrap(), original_bytes);
    assert!(root.join("profiles/welcome.toml").is_file());
    assert!(root.join("profiles/welcome.png").is_file());
    let history = inspect_history(&root).unwrap();
    assert_eq!(history.activations().len(), 2);
    assert_eq!(
        history.latest().unwrap().profile_id().unwrap().as_str(),
        "welcome"
    );
    for (name, bytes) in first {
        assert_eq!(fs::read(root.join("history").join(name)).unwrap(), bytes);
    }
    for (name, bytes) in assets {
        assert_eq!(fs::read(root.join("assets").join(name)).unwrap(), bytes);
    }
    for (name, bytes) in environments {
        assert_eq!(
            fs::read(root.join("environments").join(name)).unwrap(),
            bytes
        );
    }
    s.ok(&["doctor"], "");
    s.ok(&["previous"], "");
    assert_eq!(
        fs::read(root.join("current.ghostty")).unwrap(),
        first_projection
    );
    s.ok(&["doctor"], "");
}

#[test]
fn inactive_deletion_does_not_reconcile_drift_reload_or_change_durable_records() {
    let s = Sandbox::new();
    s.personal();
    s.ok(&["apply", "boy"], "");
    s.ok(&["apply", "welcome"], "");
    let root = s.root();
    fs::write(
        root.join("current.ghostty"),
        "# deliberately stale Projection\n",
    )
    .unwrap();
    let before = files(&root);
    let output = s.ok(&["delete", "boy"], "yes\n");
    assert!(output.contains("Inactive Profile"));
    assert!(!output.contains("Ghostty reload:"));
    let mut expected = before;
    expected.remove(Path::new("profiles/boy.toml"));
    expected.remove(Path::new("profiles/boy.png"));
    assert_eq!(files(&root), expected);
}

#[test]
fn failed_fallback_or_reconciliation_retains_profile_image_and_original() {
    for failure in [
        "missing-welcome",
        "invalid-welcome",
        "missing-image",
        "hook-drift",
        "projection-directory",
        "corrupt-history",
    ] {
        let s = Sandbox::new();
        let original = s.personal();
        s.ok(&["apply", "boy"], "");
        let root = s.root();
        match failure {
            "missing-welcome" => fs::remove_file(root.join("profiles/welcome.toml")).unwrap(),
            "invalid-welcome" => {
                fs::write(root.join("profiles/welcome.toml"), "schema_version = 999\n").unwrap()
            }
            "missing-image" => fs::remove_file(root.join("profiles/welcome.png")).unwrap(),
            "hook-drift" => fs::write(
                s.0.path().join("config/ghostty/config.ghostty"),
                "# hook absent\n",
            )
            .unwrap(),
            "projection-directory" => {
                fs::remove_file(root.join("current.ghostty")).unwrap();
                fs::create_dir(root.join("current.ghostty")).unwrap();
                fs::write(
                    root.join("current.ghostty/valuable"),
                    "never remove recursively",
                )
                .unwrap();
            }
            "corrupt-history" => fs::write(
                root.join("history/activations/act-v1-0000000000000001.json"),
                "corrupt",
            )
            .unwrap(),
            _ => unreachable!(),
        }
        let before = files(&root);
        let original_bytes = fs::read(&original).unwrap();
        let result = s.run(&["delete", "boy"], "y\n");
        assert!(!result.status.success(), "{failure}");
        assert_eq!(files(&root), before, "{failure}");
        assert_eq!(fs::read(&original).unwrap(), original_bytes);
    }
}

#[test]
fn history_replay_does_not_invent_active_profile_or_authorize_deletion() {
    let s = Sandbox::new();
    s.personal();
    s.ok(&["apply", "welcome"], "");
    s.ok(&["apply", "boy"], "");
    s.ok(&["previous"], "");
    let before = files(&s.root());
    let selector = s.ok(&["delete"], "\n");
    assert!(!selector.contains("[active]"));
    let result = s.run(&["delete", "boy"], "y\n");
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("History replay"));
    assert_eq!(files(&s.root()), before);
}

#[test]
fn ambiguous_ownership_never_deletes_images() {
    for ambiguity in [
        "shared",
        "random",
        "source-alias",
        "corrupt-peer",
        "changed-bytes",
        "renamed",
        "v1",
    ] {
        let s = Sandbox::new();
        let original = s.personal();
        let root = s.root();
        let target = if ambiguity == "renamed" {
            "renamed"
        } else {
            "boy"
        };
        match ambiguity {
            "shared" => { s.ok(&["duplicate", "boy", "other"], ""); }
            "random" => fs::write(root.join("profiles/other.toml"), "schema_version = 1\n[wallpaper]\nmode = \"source\"\nsource = \"welcome\"\nselection = \"random\"\n").unwrap(),
            "source-alias" => { s.ok(&["source", "add", "alias", "local", "./profiles"], ""); }
            "corrupt-peer" => fs::write(root.join("profiles/other.toml"), "schema_version = 999\n").unwrap(),
            "changed-bytes" => fs::write(root.join("profiles/boy.png"), b"modified bytes").unwrap(),
            "renamed" => { s.ok(&["rename", "boy", "renamed"], ""); }
            "v1" => fs::write(root.join("profiles/boy.toml"), "schema_version = 1\n[wallpaper]\nmode = \"source\"\nsource = \"welcome\"\nselection = \"path\"\npath = \"boy.png\"\n").unwrap(),
            _ => unreachable!(),
        }
        let image = fs::read(root.join("profiles/boy.png")).unwrap();
        let original_bytes = fs::read(&original).unwrap();
        let output = s.ok(&["delete", target], "y\n");
        assert!(output.contains("Images retained"), "{ambiguity}");
        assert!(!root.join(format!("profiles/{target}.toml")).exists());
        assert_eq!(
            fs::read(root.join("profiles/boy.png")).unwrap(),
            image,
            "{ambiguity}"
        );
        assert_eq!(fs::read(original).unwrap(), original_bytes);
    }
}

#[cfg(unix)]
#[test]
fn image_symlinks_hardlinks_and_unreadable_peer_intent_are_not_cleanup_authority() {
    use std::os::unix::fs::symlink;
    for kind in ["image-symlink", "image-hardlink", "profile-symlink"] {
        let s = Sandbox::new();
        let original = s.personal();
        let root = s.root();
        let image = root.join("profiles/boy.png");
        if kind == "profile-symlink" {
            symlink(root.join("missing"), root.join("profiles/other.toml")).unwrap();
        } else {
            fs::remove_file(&image).unwrap();
            if kind == "image-symlink" {
                symlink(&original, &image).unwrap();
            } else {
                fs::hard_link(&original, &image).unwrap();
            }
        }
        let original_bytes = fs::read(&original).unwrap();
        s.ok(&["delete", "boy"], "y\n");
        assert!(fs::symlink_metadata(&image).is_ok());
        assert_eq!(fs::read(original).unwrap(), original_bytes);
    }
}

#[test]
fn tui_delete_uses_same_default_cancel_confirmation_and_active_fallback() {
    let s = Sandbox::new();
    s.personal();
    s.ok(&["apply", "boy"], "");
    let before = files(&s.root());
    let output = s.ok(&["tui"], "tab\nx\n\nq\n");
    assert!(output.contains("Delete Profile boy?"));
    assert!(output.contains("Cancel (default)"));
    assert_eq!(files(&s.root()), before);
    let output = s.ok(&["tui"], "tab\nx\ny\nq\n");
    assert!(output.contains("Welcome Activation"));
    assert!(!s.root().join("profiles/boy.toml").exists());
    assert_eq!(inspect_history(&s.root()).unwrap().activations().len(), 2);
}
