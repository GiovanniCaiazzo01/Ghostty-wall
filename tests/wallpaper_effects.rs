use ghostty_wall::{
    codec::{intent::parse_profile_toml, manifest},
    domain::Sha256Digest,
    init::{InitPaths, init},
    profile_editor::{NumericControl, ProfileEditor},
    profile_workflow::ProfileWorkflows,
};
use serde_json::Value;
use std::{fs, process::Command};

fn plan(home: &std::path::Path, id: &str) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_ghostty-wall"))
        .args(["plan", id, "--json"])
        .env_clear()
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("PATH", "/nonexistent")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn reversible_effects_save_reopen_and_originals_preserved() {
    let home = tempfile::tempdir().unwrap();
    let paths = InitPaths {
        home: home.path().into(),
        xdg_config_home: Some(home.path().join("config")),
    };
    init(&paths).unwrap();
    let root = paths.managed_root();
    let workflow = ProfileWorkflows::load(paths).unwrap();
    let mut draft = workflow.create("effects").unwrap();
    workflow
        .generate_image(&mut draft, Sha256Digest::from_bytes([7; 32]))
        .unwrap();
    let original = draft.staged_image().unwrap().to_vec();
    let document = draft.document().parse::<toml_edit::DocumentMut>().unwrap();
    let mut document = document;
    document["wallpaper"]["effects"]["darken"] = toml_edit::value(0);
    draft
        .set_document(workflow.config(), document.to_string())
        .unwrap();
    workflow.save(draft).unwrap();
    let before = plan(home.path(), "effects");
    let manifest =
        manifest::decode(&serde_json::to_vec(&before["environment"]["manifest"]).unwrap()).unwrap();
    let mut editor = ProfileEditor::new(
        workflow.edit("effects").unwrap(),
        workflow.config().clone(),
        manifest,
        Some(original.clone()),
        None,
    )
    .unwrap();
    editor.set_number(NumericControl::ImageBlur, "8").unwrap();
    assert_ne!(editor.image().unwrap(), original);
    for bad in ["33", "-1", "1.5", "nan", "1e1"] {
        let document = editor.draft().document().to_owned();
        assert!(
            editor.set_number(NumericControl::ImageBlur, bad).is_err(),
            "{bad}"
        );
        assert_eq!(editor.draft().document(), document);
    }
    workflow.save_draft(editor.draft()).unwrap();
    let blurred = plan(home.path(), "effects");
    assert_ne!(blurred["asset"]["sha256"], before["asset"]["sha256"]);
    assert_eq!(blurred, plan(home.path(), "effects"));
    assert_eq!(
        fs::read(root.join("profiles/effects.png")).unwrap(),
        original
    );
    let manifest =
        manifest::decode(&serde_json::to_vec(&blurred["environment"]["manifest"]).unwrap())
            .unwrap();
    let mut reopened = ProfileEditor::new(
        workflow.edit("effects").unwrap(),
        workflow.config().clone(),
        manifest,
        Some(original.clone()),
        None,
    )
    .unwrap();
    assert_eq!(reopened.number(NumericControl::ImageBlur).unwrap(), Some(8));
    reopened.set_number(NumericControl::ImageBlur, "0").unwrap();
    assert_eq!(reopened.image().unwrap(), original);
    workflow.save_draft(reopened.draft()).unwrap();
    assert_eq!(
        plan(home.path(), "effects")["environment"],
        before["environment"]
    );
    assert_eq!(
        fs::read(root.join("profiles/effects.png")).unwrap(),
        original
    );
}

#[test]
fn oklch_effects_preserve_overrides_and_captured_replay_without_source() {
    let home = tempfile::tempdir().unwrap();
    let paths = InitPaths {
        home: home.path().into(),
        xdg_config_home: Some(home.path().join("config")),
    };
    init(&paths).unwrap();
    let root = paths.managed_root();
    let source = home.path().join("original.png");
    let bytes = include_bytes!("fixtures/white.png").to_vec();
    fs::write(&source, &bytes).unwrap();
    let workflow = ProfileWorkflows::load(paths).unwrap();
    let mut draft = workflow.create("readable").unwrap();
    workflow.import_image(&mut draft, &source).unwrap();
    workflow.save(draft).unwrap();
    let before = plan(home.path(), "readable");
    let manifest =
        manifest::decode(&serde_json::to_vec(&before["environment"]["manifest"]).unwrap()).unwrap();
    let mut editor = ProfileEditor::new(
        workflow.edit("readable").unwrap(),
        workflow.config().clone(),
        manifest,
        Some(bytes.clone()),
        None,
    )
    .unwrap();
    editor
        .set_number(NumericControl::WallpaperOpacity, "0.65")
        .unwrap();
    editor.automatic_darkening().unwrap();
    let darken = editor.number(NumericControl::ImageDarken).unwrap().unwrap();
    assert!(darken > 0 && darken <= 100);
    editor.set_color(6, "123456").unwrap();
    editor.automatic_darkening().unwrap();
    assert_eq!(
        editor.preview().unwrap().colors().unwrap().palette()[1].to_string(),
        "123456"
    );
    workflow.save_draft(editor.draft()).unwrap();
    let after = plan(home.path(), "readable");
    assert_eq!(after["color_resolution"]["algorithm"], "oklch-v2");
    assert_eq!(
        after["environment"]["manifest"]["colors"]["palette"][1],
        "123456"
    );
    assert_eq!(
        manifest::decode(&serde_json::to_vec(&after["environment"]["manifest"]).unwrap()).unwrap(),
        editor.preview().unwrap()
    );
    assert_eq!(fs::read(&source).unwrap(), bytes);
    // Public CLI apply/replay runs only in this isolated HOME with no reload adapters or user bus.
    let cli = |args: &[&str]| {
        let out = Command::new(env!("CARGO_BIN_EXE_ghostty-wall"))
            .args(args)
            .env_clear()
            .env("HOME", home.path())
            .env("XDG_CONFIG_HOME", home.path().join("config"))
            .env("PATH", "/nonexistent")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    };
    cli(&["apply", "readable"]);
    let projection = fs::read(root.join("current.ghostty")).unwrap();
    let digest = after["asset"]["sha256"].as_str().unwrap();
    let asset = root
        .join("assets/sha256")
        .join(&digest[..2])
        .join(format!("{digest}.png"));
    let captured = fs::read(&asset).unwrap();
    assert_ne!(captured, bytes);
    cli(&["apply", "welcome"]);
    fs::remove_file(root.join("profiles/readable.png")).unwrap();
    fs::write(
        root.join("profiles/readable.toml"),
        "unsupported future recipe",
    )
    .unwrap();
    cli(&["previous"]);
    assert_eq!(fs::read(root.join("current.ghostty")).unwrap(), projection);
    assert_eq!(fs::read(asset).unwrap(), captured);
    assert_eq!(fs::read(source).unwrap(), bytes);
}

#[test]
fn infeasible_editor_matches_full_resolution_and_never_migrates_saved_algorithms() {
    let home = tempfile::tempdir().unwrap();
    let paths = InitPaths {
        home: home.path().into(),
        xdg_config_home: Some(home.path().join("config")),
    };
    init(&paths).unwrap();
    let root = paths.managed_root();
    let workflow = ProfileWorkflows::load(paths).unwrap();
    let source = home.path().join("white.png");
    let bytes = include_bytes!("fixtures/white.png");
    fs::write(&source, bytes).unwrap();
    for algorithm in ["oklch-v1", "oklch-v2"] {
        let mut draft = workflow.create(algorithm).unwrap();
        workflow.import_image(&mut draft, &source).unwrap();
        let mut doc: toml_edit::DocumentMut = draft.document().parse().unwrap();
        doc["wallpaper"]["opacity"] = toml_edit::value(0.65);
        doc["wallpaper"]["effects"]["darken"] = toml_edit::value(0);
        doc["colors"]["algorithm"] = toml_edit::value(algorithm);
        draft
            .set_document(workflow.config(), doc.to_string())
            .unwrap();
        let staged = draft.generated_colors().unwrap();
        workflow.save(draft).unwrap();
        let before = plan(home.path(), algorithm);
        let resolved =
            manifest::decode(&serde_json::to_vec(&before["environment"]["manifest"]).unwrap())
                .unwrap();
        assert_eq!(resolved.colors().unwrap(), &staged);
        let mut editor = ProfileEditor::new(
            workflow.edit(algorithm).unwrap(),
            workflow.config().clone(),
            resolved.clone(),
            Some(bytes.to_vec()),
            None,
        )
        .unwrap();
        assert!(editor.palette_contrast_infeasible().unwrap());
        assert_eq!(editor.preview().unwrap(), resolved);
        editor.set_color(6, "123456").unwrap();
        assert!(editor.palette_contrast_infeasible().unwrap());
        workflow.save_draft(editor.draft()).unwrap();
        let after = plan(home.path(), algorithm);
        assert_eq!(after["color_resolution"]["algorithm"], algorithm);
        assert_eq!(after["palette_contrast"]["status"], "infeasible");
        assert_eq!(
            after["environment"]["manifest"]["colors"]["palette"][1],
            "123456"
        );
        assert_eq!(
            manifest::decode(&serde_json::to_vec(&after["environment"]["manifest"]).unwrap())
                .unwrap(),
            editor.preview().unwrap()
        );
        assert_eq!(
            fs::read(root.join(format!("profiles/{algorithm}.png"))).unwrap(),
            bytes
        );
        let cli = |args: &[&str]| {
            let out = Command::new(env!("CARGO_BIN_EXE_ghostty-wall"))
                .args(args)
                .env_clear()
                .env("HOME", home.path())
                .env("XDG_CONFIG_HOME", home.path().join("config"))
                .env("PATH", "/nonexistent")
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            String::from_utf8(out.stdout).unwrap()
        };
        assert!(
            cli(&["apply", algorithm])
                .contains("non-conforming fallback, NOT a valid 4.5:1 palette")
        );
        let captured_projection = fs::read(root.join("current.ghostty")).unwrap();
        cli(&["apply", "welcome"]);
        cli(&["previous"]);
        assert_eq!(
            fs::read(root.join("current.ghostty")).unwrap(),
            captured_projection
        );
        assert_eq!(
            plan(home.path(), algorithm)["environment"],
            after["environment"]
        );
        editor
            .set_number(NumericControl::WallpaperOpacity, "0.01")
            .unwrap();
        assert!(!editor.palette_contrast_infeasible().unwrap());
        assert_eq!(
            editor.preview().unwrap().colors().unwrap().palette()[1].to_string(),
            "123456"
        );
    }
}

#[test]
fn strict_version_bounds_and_unknown_effects() {
    let base = "schema_version = 2\n[wallpaper]\nmode = \"source\"\nsource = \"welcome\"\nselection = \"path\"\npath = \"welcome.png\"\n[wallpaper.effects]\nalgorithm = \"image-effects-v1\"\nblur = 8\ndarken = 0\n";
    assert!(parse_profile_toml(base).is_ok());
    for invalid in [
        base.replace("schema_version = 2", "schema_version = 1"),
        base.replace("image-effects-v1", "image-effects-v2"),
        base.replace("blur = 8", "blur = 33"),
        base.replace("blur = 8", "blur = -1"),
        base.replace("blur = 8", "blur = 8.0"),
        base.replace("darken = 0", "darken = 101"),
        format!("{base}unexpected = 1\n"),
    ] {
        assert!(parse_profile_toml(&invalid).is_err(), "{invalid}");
    }
}
