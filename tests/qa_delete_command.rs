//! Ticket 06 public-CLI regressions. Every child uses a disposable home and no reload commands.

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
        let sandbox = Self(tempfile::tempdir().unwrap());
        sandbox.ok(&["init"], "");
        sandbox
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
        let output = self.run(args, input);
        assert!(
            output.status.success(),
            "{args:?}: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    }

    fn profile(&self, id: &str, document: &str) {
        fs::write(self.root().join(format!("profiles/{id}.toml")), document).unwrap();
    }
}

fn snapshot(path: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, result: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if entry.file_type().unwrap().is_dir() {
                walk(root, &path, result);
            } else if entry.file_type().unwrap().is_file() {
                result.insert(
                    path.strip_prefix(root).unwrap().to_owned(),
                    fs::read(path).unwrap(),
                );
            } else {
                panic!("unexpected non-regular fixture entry: {}", path.display());
            }
        }
    }
    let mut result = BTreeMap::new();
    walk(path, path, &mut result);
    result
}

#[test]
fn guided_import_use_cancel_delete_last_profile_and_replay_after_source_disappears() {
    let s = Sandbox::new();
    let root = s.root();
    let original = s.0.path().join("my-original.jpg");
    let original_bytes = include_bytes!("fixtures/white.jpg");
    fs::write(&original, original_bytes).unwrap();
    let welcome = snapshot(&root.join("profiles"));
    let config = fs::read(root.join("config.toml")).unwrap();
    s.ok(
        &["create", "boy"],
        &format!("i\npath:{}\ns\ny\n", original.display()),
    );
    let initial = snapshot(&root);
    let projection = fs::read(root.join("current.ghostty")).unwrap();
    assert_eq!(inspect_history(&root).unwrap().activations().len(), 1);
    assert!(
        s.ok(&["delete", "boy"], "\n")
            .contains("Deletion cancelled")
    );
    assert_eq!(snapshot(&root), initial);

    let output = s.ok(&["delete"], "bad-id\nboy\nyes\n");
    assert!(output.contains("boy [active]"));
    assert!(output.contains("Choose a listed Profile"));
    assert!(output.contains("Delete Profile boy?"));
    assert!(output.contains("profiles/boy.jpg"));
    assert!(output.contains("Welcome Activation"));
    assert!(output.contains("reload: unavailable"));
    assert!(!root.join("profiles/boy.toml").exists());
    assert!(!root.join("profiles/boy.jpg").exists());
    assert_eq!(fs::read(&original).unwrap(), original_bytes);
    assert_eq!(fs::read(root.join("config.toml")).unwrap(), config);
    assert_eq!(snapshot(&root.join("profiles")), welcome);
    for (path, bytes) in &initial {
        if path.starts_with("history")
            || path.starts_with("environments")
            || path.starts_with("assets")
        {
            assert_eq!(
                fs::read(root.join(path)).unwrap(),
                *bytes,
                "{}",
                path.display()
            );
        }
    }
    let selector = s.ok(&["delete"], "\n");
    assert!(selector.contains("welcome [active] (protected fallback)"));
    assert!(!selector.contains("boy"));
    s.ok(&["plan", "welcome", "--json"], "");

    // Replay must not consult even the remaining Welcome candidate or the deleted Profile.
    fs::rename(root.join("profiles"), root.join("offline-profiles")).unwrap();
    fs::create_dir(root.join("profiles")).unwrap();
    s.ok(&["previous"], "");
    assert_eq!(fs::read(root.join("current.ghostty")).unwrap(), projection);
    assert_eq!(fs::read(original).unwrap(), original_bytes);
    s.ok(&["doctor"], "");
}

#[test]
fn customized_v1_install_without_welcome_fails_active_delete_but_allows_inactive_delete() {
    let s = Sandbox::new();
    let root = s.root();
    fs::remove_file(root.join("profiles/welcome.toml")).unwrap();
    fs::remove_file(root.join("profiles/welcome.png")).unwrap();
    fs::write(root.join("config.toml"), "schema_version = 1\n[sources]\n").unwrap();
    s.profile(
        "boy",
        "schema_version = 1\n[terminal]\nfont_size = 17.125\n",
    );
    s.profile("other", "schema_version = 1\n[wallpaper]\nmode = 'none'\n");
    s.ok(&["apply", "boy"], "");
    let initial = snapshot(&root);
    let projection = fs::read(root.join("current.ghostty")).unwrap();
    let failure = s.run(&["delete", "boy"], "y\n");
    assert!(!failure.status.success());
    assert!(String::from_utf8_lossy(&failure.stderr).contains("Welcome fallback failed"));
    assert_eq!(snapshot(&root), initial);
    s.ok(&["apply", "other"], "");
    let mut expected = snapshot(&root);
    expected.remove(Path::new("profiles/boy.toml"));
    s.ok(&["delete", "boy"], "y\n");
    assert_eq!(snapshot(&root), expected);
    assert!(!root.join("profiles/welcome.toml").exists());
    s.ok(&["previous"], "");
    assert_eq!(fs::read(root.join("current.ghostty")).unwrap(), projection);
}

#[test]
fn customized_v1_random_welcome_remains_valid_after_deleting_its_shared_image_owner() {
    let s = Sandbox::new();
    let root = s.root();
    let original = s.0.path().join("original.png");
    let original_bytes = include_bytes!("fixtures/white.png");
    fs::write(&original, original_bytes).unwrap();
    s.ok(
        &["create", "boy"],
        &format!("i\npath:{}\ns\ny\n", original.display()),
    );
    let original_projection = fs::read(root.join("current.ghostty")).unwrap();
    let welcome = "schema_version = 1\n[wallpaper]\nmode = 'source'\nsource = 'welcome'\nselection = 'random'\n[terminal]\nfont_size = 16.125\n";
    s.profile("welcome", welcome);
    let config = fs::read(root.join("config.toml")).unwrap();
    let history_before = snapshot(&root.join("history"));
    let output = s.ok(&["delete", "boy"], "y\n");
    assert!(output.contains("Welcome Activation"));
    assert!(output.contains("Images retained"));
    assert!(!root.join("profiles/boy.toml").exists());
    assert_eq!(
        fs::read(root.join("profiles/boy.png")).unwrap(),
        original_bytes
    );
    assert_eq!(fs::read(&original).unwrap(), original_bytes);
    assert_eq!(
        fs::read(root.join("profiles/welcome.toml")).unwrap(),
        welcome.as_bytes()
    );
    assert_eq!(fs::read(root.join("config.toml")).unwrap(), config);
    for (path, bytes) in history_before {
        assert_eq!(fs::read(root.join("history").join(path)).unwrap(), bytes);
    }
    let activation: serde_json::Value = serde_json::from_slice(
        &fs::read(root.join("history/activations/act-v1-0000000000000002.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(activation["profile"]["id"], "welcome");
    assert_eq!(activation["profile"]["schema_version"], 1);
    assert_eq!(activation["selection"]["kind"], "random");
    assert_eq!(activation["selection"]["candidate_count"], 2);
    assert!(
        fs::read_to_string(root.join("current.ghostty"))
            .unwrap()
            .contains("font-size = 16.125")
    );
    s.ok(
        &["plan", "welcome", "--seed", &"00".repeat(32), "--json"],
        "",
    );
    s.ok(&["doctor"], "");
    fs::rename(root.join("profiles"), root.join("offline-profiles")).unwrap();
    fs::create_dir(root.join("profiles")).unwrap();
    s.ok(&["previous"], "");
    assert_eq!(
        fs::read(root.join("current.ghostty")).unwrap(),
        original_projection
    );
    s.ok(&["doctor"], "");
}

#[test]
fn legacy_source_original_is_preserved_by_active_delete_and_replay_does_not_resolve_it() {
    let s = Sandbox::new();
    let root = s.root();
    let originals = s.0.path().join("originals");
    fs::create_dir(&originals).unwrap();
    fs::write(
        originals.join("boy.png"),
        include_bytes!("fixtures/white.png"),
    )
    .unwrap();
    s.ok(
        &[
            "source",
            "add",
            "legacy",
            "local",
            originals.to_str().unwrap(),
        ],
        "",
    );
    s.profile("boy", "schema_version = 1\nwallpaper = { mode = 'source', source = 'legacy', selection = 'path', path = 'boy.png' }\n");
    s.ok(&["apply", "boy"], "");
    let original = snapshot(&originals);
    let projection = fs::read(root.join("current.ghostty")).unwrap();
    let output = s.ok(&["delete", "boy"], "y\n");
    assert!(output.contains("Images retained"));
    assert_eq!(snapshot(&originals), original);
    fs::rename(&originals, s.0.path().join("offline-originals")).unwrap();
    s.ok(&["previous"], "");
    assert_eq!(fs::read(root.join("current.ghostty")).unwrap(), projection);
    assert_eq!(snapshot(&s.0.path().join("offline-originals")), original);
}

#[test]
fn missing_historical_asset_blocks_even_inactive_deletion_without_removing_intent() {
    let s = Sandbox::new();
    let root = s.root();
    s.ok(
        &[
            "new",
            "boy",
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/white.png"),
        ],
        "",
    );
    s.ok(&["apply", "boy"], "");
    let record: serde_json::Value = serde_json::from_slice(
        &fs::read(root.join("history/activations/act-v1-0000000000000001.json")).unwrap(),
    )
    .unwrap();
    let digest = record["asset"]["sha256"].as_str().unwrap();
    s.ok(&["apply", "welcome"], "");
    let asset = root.join(format!("assets/sha256/{}/{digest}.png", &digest[..2]));
    fs::rename(asset, s.0.path().join("unavailable-asset.png")).unwrap();
    let before = snapshot(&root);
    let output = s.run(&["delete", "boy"], "y\n");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("History"));
    assert_eq!(snapshot(&root), before);
}

#[test]
fn deleting_profile_never_removes_its_selected_original_even_when_import_path_matches_owned_basename()
 {
    let s = Sandbox::new();
    let root = s.root();
    let original = root.join("profiles/boy.png");
    let bytes = include_bytes!("fixtures/white.png");
    fs::write(&original, bytes).unwrap();
    s.ok(
        &["create", "boy"],
        &format!("i\npath:{}\ns\nn\n", original.display()),
    );
    assert_eq!(fs::read(&original).unwrap(), bytes);
    s.ok(&["delete", "boy"], "y\n");
    assert!(
        original.is_file(),
        "delete removed the exact original path selected during create, rather than an imported copy"
    );
    assert_eq!(fs::read(&original).unwrap(), bytes);
    assert!(!root.join("profiles/boy.toml").exists());
    assert!(inspect_history(&root).unwrap().activations().is_empty());
}

#[test]
fn inactive_delete_with_no_history_does_not_reconcile_interrupted_preview_or_integration() {
    let s = Sandbox::new();
    let root = s.root();
    s.profile("boy", "schema_version = 1\n");
    fs::write(root.join("preview.session"), "deliberately invalid marker").unwrap();
    fs::write(root.join("current.ghostty"), "provisional projection").unwrap();
    fs::write(
        s.0.path().join("config/ghostty/config.ghostty"),
        "# no hook\n",
    )
    .unwrap();
    let mut expected = snapshot(s.0.path());
    expected.remove(Path::new("config/ghostty/ghostty-wall/profiles/boy.toml"));
    let output = s.ok(&["delete", "boy"], "y\n");
    assert!(output.contains("Inactive Profile"));
    assert!(!output.contains("Ghostty reload"));
    assert_eq!(snapshot(s.0.path()), expected);
}
