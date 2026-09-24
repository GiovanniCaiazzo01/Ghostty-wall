use std::{
    fs,
    path::PathBuf,
    str::FromStr,
    sync::atomic::{AtomicU64, Ordering},
};

use ghostty_wall::{
    codec::{
        intent::{parse_config_toml, parse_profile_toml},
        manifest,
    },
    domain::{Color, ColorsManifest, EnvironmentManifest, IntentId, ResolutionSeed},
    plan::{PlanPlatform, ReloadObservation, plan_local_profile_json},
    recovery::{ProjectionState, inspect_recovery_state},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Root {
    managed: PathBuf,
    root_config: PathBuf,
}

impl Root {
    fn new() -> Self {
        let base = std::env::temp_dir().join(format!(
            "ghostty-wall-recovery-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let managed = base.join("ghostty-wall");
        fs::create_dir_all(managed.join("history/activations")).unwrap();
        fs::create_dir(managed.join("environments")).unwrap();
        fs::create_dir_all(managed.join("assets/sha256")).unwrap();
        fs::write(managed.join("state.lock"), []).unwrap();
        let root_config = base.join("config.ghostty");
        fs::write(
            &root_config,
            format!(
                "config-file = ?{}\n",
                managed.join("current.ghostty").display()
            ),
        )
        .unwrap();
        Self {
            managed,
            root_config,
        }
    }

    fn commit_colors_environment(&self) {
        let color = Color::from_str("112233").unwrap();
        let colors = ColorsManifest::new(color, color, [color; 16]);
        let environment = EnvironmentManifest::new(None, Some(colors), None);
        let environment_id = manifest::environment_id(&environment).unwrap();
        let manifest_json: serde_json::Value =
            serde_json::from_slice(&manifest::encode_canonical(&environment).unwrap()).unwrap();
        fs::write(
            self.managed
                .join(format!("environments/{environment_id}.json")),
            serde_json::json!({
                "record_schema_version": 1,
                "environment_id": environment_id.to_string(),
                "manifest": manifest_json,
            })
            .to_string(),
        )
        .unwrap();
        fs::write(
            self.managed
                .join("history/activations/act-v1-0000000000000001.json"),
            serde_json::json!({
                "record_schema_version": 1,
                "activation_id": "act-v1-0000000000000001",
                "sequence": 1,
                "history_cursor": 1,
                "activated_at": "2026-09-23T08:31:15.123456Z",
                "environment_id": environment_id.to_string(),
                "cause": {"kind": "profile"},
                "profile": {"id": "night", "schema_version": 1},
                "color_resolution": {"kind": "explicit"},
            })
            .to_string(),
        )
        .unwrap();
    }
}

impl Drop for Root {
    fn drop(&mut self) {
        fs::remove_dir_all(self.managed.parent().unwrap()).unwrap();
    }
}

#[test]
fn empty_history_classifies_absent_and_unexpected_projection() {
    let root = Root::new();
    let inspection = inspect_recovery_state(&root.managed, &root.root_config).unwrap();
    assert_eq!(inspection.projection(), ProjectionState::Consistent);

    fs::write(
        root.managed.join("current.ghostty"),
        "background = 112233\n",
    )
    .unwrap();
    let inspection = inspect_recovery_state(&root.managed, &root.root_config).unwrap();
    assert_eq!(inspection.projection(), ProjectionState::Unexpected);
}

#[test]
fn non_empty_history_compares_projection_semantically() {
    let root = Root::new();
    root.commit_colors_environment();

    assert_eq!(
        inspect_recovery_state(&root.managed, &root.root_config)
            .unwrap()
            .projection(),
        ProjectionState::Missing
    );

    fs::write(
        root.managed.join("current.ghostty"),
        colors_projection_with_comments(),
    )
    .unwrap();
    assert_eq!(
        inspect_recovery_state(&root.managed, &root.root_config)
            .unwrap()
            .projection(),
        ProjectionState::Consistent
    );

    fs::write(root.managed.join("current.ghostty"), "unknown = value\n").unwrap();
    assert_eq!(
        inspect_recovery_state(&root.managed, &root.root_config)
            .unwrap()
            .projection(),
        ProjectionState::OutOfSync
    );
}

#[test]
#[cfg(target_os = "linux")]
fn complete_plan_includes_sorted_recovery_diagnostics_and_reload_observation() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};

    let root = Root::new();
    let source = root.managed.parent().unwrap().join("wallpapers");
    fs::create_dir(&source).unwrap();
    fs::write(
        source.join("valid.png"),
        include_bytes!("fixtures/white.png"),
    )
    .unwrap();
    fs::write(
        source.join(OsString::from_vec(b"bad-\xff.png".to_vec())),
        include_bytes!("fixtures/white.png"),
    )
    .unwrap();
    fs::write(
        root.managed.join("current.ghostty"),
        "background = 112233\n",
    )
    .unwrap();
    let config = parse_config_toml(&format!(
        "schema_version = 1\n[sources.local]\nkind = \"local-directory\"\npath = \"{}\"\n",
        source.display()
    ))
    .unwrap();
    let profile = parse_profile_toml(
        "schema_version = 1\n[wallpaper]\nmode = \"source\"\nsource = \"local\"\nselection = \"random\"\n",
    )
    .unwrap();
    let before = fs::read(root.managed.join("current.ghostty")).unwrap();

    let plan = plan_local_profile_json(
        &root.managed,
        root.managed.parent().unwrap(),
        &root.managed,
        &IntentId::from_str("night").unwrap(),
        &config,
        &profile,
        Some(&ResolutionSeed::from_bytes([3; 32])),
        &PlanPlatform::new(root.root_config.clone(), ReloadObservation::Systemd),
    )
    .unwrap();

    assert_eq!(
        plan["diagnostics"],
        serde_json::json!([
            {"code": "projection.unexpected", "severity": "warning"},
            {"code": "source.skipped-non-utf8-entries", "severity": "warning", "source_id": "local", "count": 1}
        ])
    );
    assert_eq!(
        plan["operations"][3],
        serde_json::json!({"kind": "reload_ghostty", "required": false, "adapter": "systemd"})
    );
    assert_eq!(
        fs::read(root.managed.join("current.ghostty")).unwrap(),
        before
    );
}

#[test]
fn missing_or_duplicate_effective_hook_is_fatal_without_mutation() {
    let root = Root::new();
    let before = fs::read_dir(&root.managed).unwrap().count();
    fs::write(&root.root_config, "font-size = 12\n").unwrap();
    assert!(inspect_recovery_state(&root.managed, &root.root_config).is_err());
    assert_eq!(fs::read_dir(&root.managed).unwrap().count(), before);

    let hook = format!(
        "config-file = ?{}\n",
        root.managed.join("current.ghostty").display()
    );
    fs::write(&root.root_config, format!("{hook}{hook}")).unwrap();
    assert!(inspect_recovery_state(&root.managed, &root.root_config).is_err());
}

#[test]
fn complete_plan_recovery_failures_have_registered_error_responses() {
    let root = Root::new();
    fs::remove_file(root.managed.join("state.lock")).unwrap();
    let config = parse_config_toml("schema_version = 1\n[sources]\n").unwrap();
    let profile = parse_profile_toml("schema_version = 1\n").unwrap();
    let error = plan_local_profile_json(
        &root.managed,
        root.managed.parent().unwrap(),
        &root.managed,
        &IntentId::from_str("night").unwrap(),
        &config,
        &profile,
        None,
        &PlanPlatform::new(
            root.root_config.clone(),
            ReloadObservation::Unavailable(
                ghostty_wall::plan::ReloadUnavailableReason::AdapterCommandUnavailable,
            ),
        ),
    )
    .unwrap_err();

    assert_eq!(error.exit_status(), 3);
    assert_eq!(
        error.error_response(),
        serde_json::json!({
            "schema_version": 1,
            "error": {
                "category": "intent",
                "code": "integration.not-initialized",
                "path": root.managed.join("state.lock").display().to_string()
            }
        })
    );
}

fn colors_projection_with_comments() -> String {
    let mut text = String::from("# generated projection\nforeground = 112233\n");
    for index in (0..16).rev() {
        text.push_str(&format!("palette = {index}=112233\n"));
    }
    text.push_str("\nbackground = 112233\n");
    text
}
