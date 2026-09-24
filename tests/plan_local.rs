use std::{
    fs,
    str::FromStr,
    time::{SystemTime, UNIX_EPOCH},
};

use ghostty_wall::{
    codec::intent::{parse_config_toml, parse_profile_toml},
    domain::{IntentId, ResolutionSeed},
    plan::plan_local_profile_json_uninspected as plan_local_profile_json,
};

#[test]
fn plans_local_random_wallpaper_explicit_colors_without_writing() {
    let root = temp_root("plan-local");
    let config_dir = root.join("config");
    let source = root.join("wallpapers");
    let managed = root.join("managed");
    fs::create_dir_all(&source).unwrap();
    fs::create_dir_all(&managed).unwrap();
    fs::write(
        source.join("tokyo.jpg"),
        include_bytes!("fixtures/white.jpg"),
    )
    .unwrap();
    fs::write(source.join("note.txt"), b"ignored").unwrap();

    let config = parse_config_toml(&format!(
        r#"
schema_version = 1

[sources.local]
kind = "local-directory"
path = "{}"
"#,
        source.display()
    ))
    .unwrap();
    let profile = parse_profile_toml(
        r##"
schema_version = 1

[wallpaper]
mode = "source"
source = "local"
selection = "random"
fit = "cover"

[colors]
mode = "explicit"
background = "1a1b26"
foreground = "c0caf5"
palette = [
  "15161e", "f7768e", "9ece6a", "e0af68",
  "7aa2f7", "bb9af7", "7dcfff", "a9b1d6",
  "414868", "f7768e", "9ece6a", "e0af68",
  "7aa2f7", "bb9af7", "7dcfff", "c0caf5",
]
"##,
    )
    .unwrap();
    let seed = ResolutionSeed::from_str(
        "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f",
    )
    .unwrap();

    let plan = plan_local_profile_json(
        &config_dir,
        &root,
        &managed,
        &IntentId::from_str("night").unwrap(),
        &config,
        &profile,
        Some(&seed),
    )
    .unwrap();

    assert_eq!(plan["schema_version"], 1);
    assert_eq!(plan["profile"]["id"], "night");
    assert_eq!(plan["source"]["kind"], "local-directory");
    assert_eq!(plan["selection"]["kind"], "random");
    assert_eq!(plan["selection"]["candidate"], "tokyo.jpg");
    assert_eq!(plan["asset"]["media_type"], "image/jpeg");
    assert_eq!(plan["color_resolution"]["kind"], "explicit");
    assert!(
        plan["environment"]["environment_id"]
            .as_str()
            .unwrap()
            .starts_with("env-v1-")
    );
    assert_eq!(
        plan["environment"]["manifest"]["wallpaper"]["mode"],
        "image"
    );
    let environment_id = plan["environment"]["environment_id"].as_str().unwrap();
    let digest = plan["asset"]["sha256"].as_str().unwrap();
    assert_eq!(
        plan["operations"],
        serde_json::json!([
            { "kind": "ensure_asset", "asset_sha256": digest, "disposition": "create" },
            { "kind": "ensure_environment", "environment_id": environment_id, "disposition": "create" },
            { "kind": "activate_environment", "environment_id": environment_id, "disposition": "apply" },
            { "kind": "reload_ghostty", "required": false, "adapter": "unavailable", "reason": "adapter-command-unavailable" }
        ])
    );
    assert!(!managed.join("current.ghostty").exists());

    let digest = plan["asset"]["sha256"].as_str().unwrap();
    let shard = managed.join("assets/sha256").join(&digest[..2]);
    fs::create_dir_all(&shard).unwrap();
    fs::write(shard.join(format!("{digest}.jpg")), b"corrupt").unwrap();
    assert!(
        plan_local_profile_json(
            &config_dir,
            &root,
            &managed,
            &IntentId::from_str("night").unwrap(),
            &config,
            &profile,
            Some(&seed)
        )
        .is_err()
    );
    fs::write(
        shard.join(format!("{digest}.jpg")),
        include_bytes!("fixtures/white.jpg"),
    )
    .unwrap();
    let reused = plan_local_profile_json(
        &config_dir,
        &root,
        &managed,
        &IntentId::from_str("night").unwrap(),
        &config,
        &profile,
        Some(&seed),
    )
    .unwrap();
    assert_eq!(reused["operations"][0]["disposition"], "reuse");
    #[cfg(unix)]
    {
        let owned_assets = managed.join("assets");
        let outside_assets = root.join("elsewhere-assets");
        fs::rename(&owned_assets, &outside_assets).unwrap();
        std::os::unix::fs::symlink(&outside_assets, &owned_assets).unwrap();
        let err = plan_local_profile_json(
            &config_dir,
            &root,
            &managed,
            &IntentId::from_str("night").unwrap(),
            &config,
            &profile,
            Some(&seed),
        )
        .unwrap_err();
        assert!(
            matches!(err, ghostty_wall::plan::PlanError::Corrupt(path) if path == owned_assets)
        );
        fs::remove_file(&owned_assets).unwrap();
        fs::rename(&outside_assets, &owned_assets).unwrap();
    }
    let other_shard = managed.join("assets/sha256/ff");
    fs::create_dir_all(&other_shard).unwrap();
    let duplicate = other_shard.join(format!("{digest}.jpg"));
    fs::write(&duplicate, include_bytes!("fixtures/white.jpg")).unwrap();
    assert!(
        plan_local_profile_json(
            &config_dir,
            &root,
            &managed,
            &IntentId::from_str("night").unwrap(),
            &config,
            &profile,
            Some(&seed)
        )
        .is_err()
    );
    fs::remove_file(duplicate).unwrap();
    let env = plan["environment"]["environment_id"].as_str().unwrap();
    fs::create_dir_all(managed.join("environments")).unwrap();
    fs::write(managed.join("environments").join(format!("{env}.json")), b"{\"record_schema_version\":1,\"record_schema_version\":1,\"environment_id\":\"x\",\"manifest\":{\"schema_version\":1}}").unwrap();
    assert!(
        plan_local_profile_json(
            &config_dir,
            &root,
            &managed,
            &IntentId::from_str("night").unwrap(),
            &config,
            &profile,
            Some(&seed)
        )
        .is_err()
    );
}

#[test]
fn direct_path_plan_omits_random_selection_fields() {
    let root = temp_root("plan-path");
    let source = root.join("wallpapers");
    fs::create_dir_all(&source).unwrap();
    fs::write(
        source.join("tokyo.png"),
        include_bytes!("fixtures/white.png"),
    )
    .unwrap();
    let config = parse_config_toml(&format!(
        "schema_version = 1\n\n[sources.local]\nkind = \"local-directory\"\npath = \"{}\"\n",
        source.display()
    ))
    .unwrap();
    let profile = parse_profile_toml(
        "schema_version = 1\n\n[wallpaper]\nmode = \"source\"\nsource = \"local\"\nselection = \"path\"\npath = \"tokyo.png\"\n",
    )
    .unwrap();

    let plan = plan_local_profile_json(
        &root,
        &root,
        &root.join("managed"),
        &IntentId::from_str("fixed").unwrap(),
        &config,
        &profile,
        None,
    )
    .unwrap();

    assert_eq!(
        plan["selection"],
        serde_json::json!({ "kind": "path", "candidate": "tokyo.png" })
    );
}

#[test]
#[cfg(target_os = "linux")]
fn direct_path_rejects_symlink_escape_and_invalid_image() {
    use std::os::unix::fs::symlink;
    let root = temp_root("plan-symlink");
    let source = root.join("wallpapers");
    fs::create_dir_all(&source).unwrap();
    fs::write(root.join("outside.jpg"), b"\xff\xd8\xffsecret").unwrap();
    symlink(root.join("outside.jpg"), source.join("escape.jpg")).unwrap();
    let config = parse_config_toml(&format!(
        "schema_version = 1\n[sources.local]\nkind = \"local-directory\"\npath = \"{}\"\n",
        source.display()
    ))
    .unwrap();
    let profile = parse_profile_toml("schema_version = 1\n[wallpaper]\nmode = \"source\"\nsource = \"local\"\nselection = \"path\"\npath = \"escape.jpg\"\n").unwrap();
    let id = IntentId::from_str("fixed").unwrap();
    assert!(plan_local_profile_json(&root, &root, &root, &id, &config, &profile, None).is_err());

    let outside = root.join("outside");
    fs::create_dir_all(&outside).unwrap();
    fs::write(
        outside.join("wall.jpg"),
        include_bytes!("fixtures/white.jpg"),
    )
    .unwrap();
    symlink(&outside, source.join("linked-dir")).unwrap();
    let profile = parse_profile_toml("schema_version = 1\n[wallpaper]\nmode = \"source\"\nsource = \"local\"\nselection = \"path\"\npath = \"linked-dir/wall.jpg\"\n").unwrap();
    assert!(plan_local_profile_json(&root, &root, &root, &id, &config, &profile, None).is_err());

    fs::write(source.join("broken.jpg"), b"\xff\xd8\xffnot-jpeg").unwrap();
    let profile = parse_profile_toml("schema_version = 1\n[wallpaper]\nmode = \"source\"\nsource = \"local\"\nselection = \"path\"\npath = \"broken.jpg\"\n").unwrap();
    assert!(plan_local_profile_json(&root, &root, &root, &id, &config, &profile, None).is_err());
}

#[test]
#[cfg(target_os = "linux")]
fn random_plan_reports_non_utf8_entries_and_ignores_directory_symlinks() {
    use std::os::unix::{ffi::OsStringExt, fs::symlink};
    let root = temp_root("non-utf8-source");
    let source = root.join("wallpapers");
    fs::create_dir_all(&source).unwrap();
    fs::write(
        source.join("valid.png"),
        include_bytes!("fixtures/white.png"),
    )
    .unwrap();
    fs::write(
        source.join(std::ffi::OsString::from_vec(b"bad-\xff.png".to_vec())),
        include_bytes!("fixtures/white.png"),
    )
    .unwrap();
    let outside = root.join("outside");
    fs::create_dir_all(&outside).unwrap();
    fs::write(
        outside.join("other.jpg"),
        include_bytes!("fixtures/white.jpg"),
    )
    .unwrap();
    symlink(&outside, source.join("linked-dir")).unwrap();
    let config = parse_config_toml(&format!(
        "schema_version = 1\n[sources.local]\nkind = \"local-directory\"\npath = \"{}\"\n",
        source.display()
    ))
    .unwrap();
    let profile = parse_profile_toml("schema_version = 1\n[wallpaper]\nmode = \"source\"\nsource = \"local\"\nselection = \"random\"\n").unwrap();
    let plan = plan_local_profile_json(
        &root,
        &root,
        &root.join("managed"),
        &IntentId::from_str("night").unwrap(),
        &config,
        &profile,
        Some(&ResolutionSeed::from_bytes([3; 32])),
    )
    .unwrap();
    assert_eq!(plan["selection"]["candidate_count"], 1);
    assert_eq!(plan["selection"]["candidate"], "valid.png");
    assert_eq!(
        plan["diagnostics"],
        serde_json::json!([{"code":"source.skipped-non-utf8-entries", "severity":"warning", "source_id":"local", "count":1}])
    );
}

#[test]
fn rejects_seed_without_random_selection() {
    let profile = parse_profile_toml("schema_version = 1\n").unwrap();
    let config = parse_config_toml("schema_version = 1\n[sources]\n").unwrap();
    let id = IntentId::from_str("fixed").unwrap();
    let seed = ResolutionSeed::from_bytes([0; 32]);
    let root = temp_root("seed");
    assert!(
        plan_local_profile_json(&root, &root, &root, &id, &config, &profile, Some(&seed)).is_err()
    );
}

#[test]
fn empty_local_candidate_set_has_registered_error_response() {
    let root = temp_root("empty-candidates");
    let source = root.join("wallpapers");
    fs::create_dir_all(&source).unwrap();
    let config = parse_config_toml(&format!(
        "schema_version = 1\n[sources.local]\nkind = \"local-directory\"\npath = \"{}\"\n",
        source.display()
    ))
    .unwrap();
    let profile = parse_profile_toml("schema_version = 1\n[wallpaper]\nmode = \"source\"\nsource = \"local\"\nselection = \"random\"\n").unwrap();
    let error = plan_local_profile_json(
        &root,
        &root,
        &root.join("managed"),
        &IntentId::from_str("night").unwrap(),
        &config,
        &profile,
        Some(&ResolutionSeed::from_bytes([0; 32])),
    )
    .unwrap_err();
    assert_eq!(
        error.error_response(),
        serde_json::json!({
            "schema_version": 1,
            "error": {"category": "resolution", "code": "source.empty-candidate-set", "source_id": "local"}
        })
    );
}

#[test]
fn local_tilde_expands_against_home_not_config_dir() {
    let root = temp_root("tilde");
    let source = root.join("Pictures");
    fs::create_dir_all(&source).unwrap();
    fs::write(
        source.join("wall.png"),
        include_bytes!("fixtures/white.png"),
    )
    .unwrap();
    let config = parse_config_toml(
        "schema_version = 1\n[sources.local]\nkind = \"local-directory\"\npath = \"~/Pictures\"\n",
    )
    .unwrap();
    let profile = parse_profile_toml("schema_version = 1\n[wallpaper]\nmode = \"source\"\nsource = \"local\"\nselection = \"path\"\npath = \"wall.png\"\n").unwrap();
    let plan = plan_local_profile_json(
        &root.join("config"),
        &root,
        &root.join("managed"),
        &IntentId::from_str("fixed").unwrap(),
        &config,
        &profile,
        None,
    )
    .unwrap();
    assert_eq!(
        plan["source"]["resolved_root"],
        source.display().to_string()
    );
}

fn temp_root(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "ghostty-wall-{name}-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}
