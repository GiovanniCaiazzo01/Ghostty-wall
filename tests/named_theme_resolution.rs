use std::{
    fs,
    str::FromStr,
    sync::atomic::{AtomicU64, Ordering},
};

use ghostty_wall::{
    apply::apply_local_profile_with_theme,
    codec::intent::{parse_config_toml, parse_profile_toml},
    domain::IntentId,
    plan::plan_local_profile_with_theme_json_uninspected,
    theme::ThemeFileResolver,
};

static NEXT: AtomicU64 = AtomicU64::new(0);

const TOKYO_NIGHT: &str = r#"
# unmanaged Ghostty settings must not enter Environment Manifest
font-family = JetBrains Mono
background = #1A1B26
foreground = #C0CAF5
palette = 0=#15161E
palette = 1=#F7768E
palette = 2=#9ECE6A
palette = 3=#E0AF68
palette = 4=#7AA2F7
palette = 5=#BB9AF7
palette = 6=#7DCFFF
palette = 7=#A9B1D6
palette = 8=#414868
palette = 9=#F7768E
palette = 10=#9ECE6A
palette = 11=#E0AF68
palette = 12=#7AA2F7
palette = 13=#BB9AF7
palette = 14=#7DCFFF
palette = 15=#C0CAF5
cursor-color = #C0CAF5
selection-background = #33467C
selection-foreground = #C0CAF5
"#;

#[test]
fn named_themes_materialize_managed_colors_and_deduplicate_environments() {
    let root = temp_root("resolve");
    let local = root.join("local-themes");
    let builtin = root.join("builtin-themes");
    fs::create_dir_all(&local).unwrap();
    fs::create_dir(&builtin).unwrap();
    fs::write(builtin.join("TokyoNight"), TOKYO_NIGHT).unwrap();
    fs::write(local.join("TokyoNightAlias"), TOKYO_NIGHT).unwrap();
    let themes = ThemeFileResolver::new([local, builtin]);
    let config = parse_config_toml("schema_version = 1\n[sources]\n").unwrap();
    let managed = root.join("managed");

    let tokyo = profile("TokyoNight");
    let alias = profile("TokyoNightAlias");
    let id = IntentId::from_str("night").unwrap();
    let plan = plan_local_profile_with_theme_json_uninspected(
        &root, &root, &managed, &id, &config, &tokyo, None, &themes,
    )
    .unwrap();
    let alias_plan = plan_local_profile_with_theme_json_uninspected(
        &root, &root, &managed, &id, &config, &alias, None, &themes,
    )
    .unwrap();

    assert_eq!(
        plan["environment"]["manifest"]["colors"],
        serde_json::json!({
            "background": "1a1b26",
            "foreground": "c0caf5",
            "palette": [
                "15161e", "f7768e", "9ece6a", "e0af68",
                "7aa2f7", "bb9af7", "7dcfff", "a9b1d6",
                "414868", "f7768e", "9ece6a", "e0af68",
                "7aa2f7", "bb9af7", "7dcfff", "c0caf5"
            ],
            "cursor": "c0caf5",
            "selection_background": "33467c",
            "selection_foreground": "c0caf5"
        })
    );
    assert!(plan["environment"]["manifest"].get("font-family").is_none());
    assert_eq!(
        plan["color_resolution"],
        serde_json::json!({
            "kind": "theme",
            "theme": "TokyoNight",
            "content_sha256": "1beae35787512b949d3a73bbe45ac0dfb6cc2a0e8a1f2001aa0a237b1210f643"
        })
    );
    assert_eq!(
        plan["environment"]["environment_id"],
        alias_plan["environment"]["environment_id"]
    );
    assert_eq!(alias_plan["color_resolution"]["theme"], "TokyoNightAlias");

    initialize_managed_root(&managed, &root);
    for (profile, timestamp) in [
        (&tokyo, "2026-09-23T08:31:15.123456Z"),
        (&alias, "2026-09-23T08:31:16.123456Z"),
    ] {
        apply_local_profile_with_theme(
            &root,
            &root,
            &managed,
            &root.join("config.ghostty"),
            &id,
            &config,
            profile,
            None,
            &themes,
            timestamp,
            || Ok::<(), ()>(()),
        )
        .unwrap();
    }
    assert_eq!(
        fs::read_dir(managed.join("environments")).unwrap().count(),
        1
    );
    for (sequence, theme) in [(1, "TokyoNight"), (2, "TokyoNightAlias")] {
        let activation: serde_json::Value = serde_json::from_slice(
            &fs::read(managed.join(format!("history/activations/act-v1-{sequence:016x}.json")))
                .unwrap(),
        )
        .unwrap();
        assert_eq!(activation["color_resolution"]["theme"], theme);
    }

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn missing_and_malformed_themes_fail_before_planning_writes() {
    let root = temp_root("failures");
    let themes_dir = root.join("themes");
    fs::create_dir_all(&themes_dir).unwrap();
    fs::write(themes_dir.join("Broken"), "background = #000000\n").unwrap();
    let themes = ThemeFileResolver::new([themes_dir]);
    let config = parse_config_toml("schema_version = 1\n[sources]\n").unwrap();
    let managed = root.join("managed");
    let id = IntentId::from_str("night").unwrap();

    for name in ["Missing", "Broken", "../Escape"] {
        let error = plan_local_profile_with_theme_json_uninspected(
            &root,
            &root,
            &managed,
            &id,
            &config,
            &profile(name),
            None,
            &themes,
        )
        .unwrap_err();
        assert_eq!(
            error.error_response()["error"]["code"],
            "resolution.unsupported-input"
        );
        assert_eq!(error.exit_status(), 4);
        assert!(!managed.exists());
    }

    fs::remove_dir_all(root).unwrap();
}

fn profile(theme: &str) -> ghostty_wall::domain::ProfileIntent {
    parse_profile_toml(&format!(
        "schema_version = 1\n[colors]\nmode = \"theme\"\ntheme = \"{theme}\"\n"
    ))
    .unwrap()
}

fn initialize_managed_root(managed: &std::path::Path, root: &std::path::Path) {
    fs::create_dir_all(managed.join("history/activations")).unwrap();
    fs::create_dir(managed.join("environments")).unwrap();
    fs::create_dir_all(managed.join("assets/sha256")).unwrap();
    fs::write(managed.join("state.lock"), []).unwrap();
    fs::write(
        root.join("config.ghostty"),
        format!(
            "config-file = ?{}\n",
            managed.join("current.ghostty").display()
        ),
    )
    .unwrap();
}

fn temp_root(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "ghostty-wall-theme-{name}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}
