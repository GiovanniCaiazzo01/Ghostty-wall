use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Output, Stdio},
};

use ghostty_wall::{
    codec::intent::{IntentTomlError, parse_profile_toml},
    domain::Sha256Digest,
};
use serde_json::Value;
use sha2::{Digest, Sha256};

fn run(home: &Path, args: &[&str]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_ghostty-wall"))
        .args(args)
        // Isolate reload adapters as well as files: a temporary HOME does not isolate the user bus.
        .env_clear()
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_RUNTIME_DIR", home.join("runtime"))
        .env("PATH", "/nonexistent")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if args.first() == Some(&"delete") {
        child.stdin.take().unwrap().write_all(b"y\n").unwrap();
    }
    child.wait_with_output().unwrap()
}

fn ok(home: &Path, args: &[&str]) -> Output {
    let result = run(home, args);
    assert!(
        result.status.success(),
        "{}: {}",
        args.join(" "),
        String::from_utf8_lossy(&result.stderr)
    );
    result
}

fn plan(home: &Path, name: &str) -> Value {
    serde_json::from_slice(&ok(home, &["plan", name, "--json"]).stdout).unwrap()
}

#[test]
fn new_recipe_overrides_replay_and_exclusive_cleanup() {
    let home = tempfile::tempdir().unwrap();
    let home = home.path();
    ok(home, &["init"]);
    let root = home.join("config/ghostty/ghostty-wall");
    let seed = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    ok(home, &["new", "recipe", "--generate", seed]);
    let profile = fs::read_to_string(root.join("profiles/recipe.toml")).unwrap();
    assert!(profile.contains("algorithm = \"gradient-v1\""));
    assert!(root.join("profiles/recipe.png").is_file());
    let original_png = fs::read(root.join("profiles/recipe.png")).unwrap();
    let before = plan(home, "recipe");
    assert_eq!(before["profile"]["schema_version"], 2);
    ok(home, &["edit", "recipe", "colors.background", "abcdef"]);
    ok(home, &["edit", "recipe", "colors.palette.4", "112233"]);
    let customized = plan(home, "recipe");
    assert_eq!(
        customized["environment"]["manifest"]["colors"]["background"],
        "abcdef"
    );
    assert_eq!(
        customized["environment"]["manifest"]["colors"]["palette"][4],
        "112233"
    );
    assert_eq!(
        customized["environment"]["manifest"]["colors"]["foreground"],
        before["environment"]["manifest"]["colors"]["foreground"]
    );
    ok(home, &["edit", "recipe", "colors.background", "auto"]);
    assert_eq!(
        plan(home, "recipe")["environment"]["manifest"]["colors"]["background"],
        before["environment"]["manifest"]["colors"]["background"]
    );
    ok(home, &["apply", "recipe"]);
    assert_eq!(
        fs::read(root.join("profiles/recipe.png")).unwrap(),
        original_png
    );
    let activation =
        fs::read_to_string(root.join("history/activations/act-v1-0000000000000001.json")).unwrap();
    assert!(activation.contains("\"schema_version\":2"), "{activation}");
    ok(home, &["apply", "welcome"]);
    let profile_path = root.join("profiles/recipe.toml");
    let saved = fs::read_to_string(&profile_path).unwrap();
    fs::write(
        &profile_path,
        saved.replace("schema_version = 2", "schema_version = 3"),
    )
    .unwrap();
    let unsupported = fs::read(&profile_path).unwrap();
    assert!(!run(home, &["plan", "recipe"]).status.success());
    assert_eq!(fs::read(&profile_path).unwrap(), unsupported);
    ok(home, &["previous"]); // History replay does not parse the now-unsupported Profile.
    fs::write(&profile_path, saved).unwrap();
    let historical = fs::read_to_string(root.join("current.ghostty")).unwrap();
    assert!(historical.contains("background-image"));
    ok(home, &["apply", "welcome"]);
    ok(home, &["delete", "recipe"]);
    assert!(!root.join("profiles/recipe.png").exists());
    assert!(!root.join("profiles/recipe.toml").exists());
    ok(home, &["previous"]);
    assert!(root.join("current.ghostty").is_file());
}

#[test]
fn shared_and_mismatched_claims_preserve_images() {
    let home = tempfile::tempdir().unwrap();
    let home = home.path();
    ok(home, &["init"]);
    let root = home.join("config/ghostty/ghostty-wall/profiles");
    let seed = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    ok(home, &["new", "shared", "--generate", seed]);
    fs::write(root.join("other.toml"), "schema_version = 1\n[wallpaper]\nmode = \"source\"\nsource = \"welcome\"\nselection = \"path\"\npath = \"shared.png\"\n").unwrap();
    ok(home, &["delete", "shared"]);
    assert!(root.join("shared.png").is_file());
    ok(home, &["new", "mismatch", "--generate", seed]);
    fs::write(root.join("mismatch.png"), b"changed").unwrap();
    let error = run(home, &["plan", "mismatch", "--json"]);
    assert!(!error.status.success());
    let response: Value = serde_json::from_slice(&error.stdout).unwrap();
    assert_eq!(response["error"]["code"], "intent.owned-image-mismatch");
    ok(home, &["delete", "mismatch"]);
    assert!(root.join("mismatch.png").is_file());
}

#[test]
fn replacement_recomputes_only_auto_slots_and_old_owned_bytes_are_not_regenerated() {
    let home = tempfile::tempdir().unwrap();
    let home = home.path();
    ok(home, &["init"]);
    let profiles = home.join("config/ghostty/ghostty-wall/profiles");
    let seed_a = "0000000000000000000000000000000000000000000000000000000000000000";
    let seed_b = "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";
    ok(home, &["new", "recipe", "--generate", seed_a]);
    ok(home, &["new", "replacement", "--generate", seed_b]);
    ok(home, &["edit", "recipe", "colors.background", "112233"]);
    ok(home, &["edit", "recipe", "colors.palette.15", "abcdef"]);
    let original = plan(home, "recipe");
    let replacement = plan(home, "replacement");
    assert_ne!(
        original["environment"]["manifest"]["colors"]["foreground"],
        replacement["environment"]["manifest"]["colors"]["foreground"]
    );
    let path = profiles.join("recipe.png");
    let bytes = fs::read(profiles.join("replacement.png")).unwrap();
    let old_bytes = fs::read(&path).unwrap();
    let old_digest = Sha256Digest::from_bytes(Sha256::digest(&old_bytes).into()).to_string();
    let new_digest = Sha256Digest::from_bytes(Sha256::digest(&bytes).into()).to_string();
    fs::write(&path, &bytes).unwrap();
    let mismatched = run(home, &["plan", "recipe", "--json"]);
    assert_eq!(mismatched.status.code(), Some(3));
    let error: Value = serde_json::from_slice(&mismatched.stdout).unwrap();
    assert_eq!(error["error"]["code"], "intent.owned-image-mismatch");
    assert_eq!(fs::read(&path).unwrap(), bytes);
    let profile_path = profiles.join("recipe.toml");
    let recipe = fs::read_to_string(&profile_path).unwrap();
    fs::write(&profile_path, recipe.replacen(&old_digest, &new_digest, 1)).unwrap();
    let after = plan(home, "recipe");
    assert_eq!(
        after["environment"]["manifest"]["colors"]["background"],
        "112233"
    );
    assert_eq!(
        after["environment"]["manifest"]["colors"]["palette"][15],
        "abcdef"
    );
    assert_eq!(
        after["environment"]["manifest"]["colors"]["foreground"],
        replacement["environment"]["manifest"]["colors"]["foreground"]
    );
    assert_eq!(after["asset"]["sha256"], new_digest);
    assert_eq!(fs::read(&path).unwrap(), bytes);
}

#[test]
fn invalid_claims_and_aliased_sources_are_not_cleanup_authority() {
    let home = tempfile::tempdir().unwrap();
    let home = home.path();
    ok(home, &["init"]);
    let root = home.join("config/ghostty/ghostty-wall");
    let seed = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    ok(home, &["new", "owned", "--generate", seed]);
    let path = root.join("profiles/owned.toml");
    let initial = fs::read(&path).unwrap();
    let invalid = String::from_utf8(initial.clone())
        .unwrap()
        .replace("algorithm = \"gradient-v1\"", "algorithm = \"future-v2\"");
    fs::write(&path, &invalid).unwrap();
    let before = fs::read(&path).unwrap();
    assert!(!run(home, &["plan", "owned", "--json"]).status.success());
    assert!(!run(home, &["apply", "owned"]).status.success());
    assert_eq!(fs::read(&path).unwrap(), before);
    fs::write(&path, &initial).unwrap();
    ok(home, &["source", "add", "alias", "local", "profiles"]);
    ok(home, &["delete", "owned"]);
    assert!(
        root.join("profiles/owned.png").is_file(),
        "another local Source makes ownership ambiguous even without a live Profile reference"
    );
}

#[test]
fn strict_versions_and_metadata_do_not_migrate_old_profiles() {
    let old = "schema_version = 1\n[wallpaper]\nmode = \"source\"\nsource = \"welcome\"\nselection = \"path\"\npath = \"welcome.png\"\n[colors]\nmode = \"generated\"\n";
    assert!(parse_profile_toml(old).is_ok());
    assert_eq!(
        parse_profile_toml("schema_version = 3\n").unwrap_err(),
        IntentTomlError::SchemaVersion
    );
    for bad in [
        old.replace("schema_version = 1", "schema_version = 2").replace("path = \"welcome.png\"", &format!("path = \"welcome.png\"\nowned_sha256 = \"{}\"\n[wallpaper.generation]\nalgorithm = \"gradient-v1\"\nseed = \"{}\"\nwidth = 63\nheight = 256", "0".repeat(64), "0".repeat(64))),
        old.replace("schema_version = 1", "schema_version = 2").replace("mode = \"generated\"", "mode = \"generated\"\n[colors.overrides]\nunknown = \"abcdef\""),
        old.replace("schema_version = 1", "schema_version = 2").replace("mode = \"generated\"", "mode = \"generated\"\n[colors.overrides]\npalette = [\"auto\"]"),
        old.replace("schema_version = 1", "schema_version = 2").replace("mode = \"generated\"", "mode = \"generated\"\n[colors.overrides]\nbackground = \"BADBAD\""),
        old.replace("path = \"welcome.png\"", "path = \"welcome.png\"\nowned_sha256 = \"00\""),
        old.replace("schema_version = 1", "schema_version = 2").replace("path = \"welcome.png\"", "path = \"welcome.png\"\n[wallpaper.generation]\nalgorithm = \"gradient-v2\"\nseed = \"00\"\nwidth = 256\nheight = 256"),
    ] {
        assert!(parse_profile_toml(&bad).is_err(), "{bad}");
    }
    let home = tempfile::tempdir().unwrap();
    let home = home.path();
    ok(home, &["init"]);
    let root = home.join("config/ghostty/ghostty-wall/profiles");
    fs::write(root.join("old.toml"), old).unwrap();
    let before = fs::read(root.join("old.toml")).unwrap();
    ok(home, &["plan", "old"]);
    ok(home, &["apply", "old"]);
    assert_eq!(fs::read(root.join("old.toml")).unwrap(), before);
    ok(home, &["apply", "welcome"]);
    ok(home, &["previous"]);
}
