use std::{fs, path::Path};

use ghostty_wall::{
    codec::{intent::parse_named_profile_toml, manifest},
    domain::WallpaperIntent,
    init::{InitPaths, init},
    plan::plan_local_profile_json_uninspected,
    profile_editor::{NumericControl, ProfileEditor},
    profile_workflow::ProfileWorkflows,
};

fn resolve(
    workflow: &ProfileWorkflows,
    paths: &InitPaths,
) -> ghostty_wall::domain::EnvironmentManifest {
    let draft = workflow.edit("boy").unwrap();
    let (_, profile) =
        parse_named_profile_toml("boy", workflow.config(), draft.document()).unwrap();
    let root = paths.managed_root();
    let plan = plan_local_profile_json_uninspected(
        &root,
        &paths.home,
        &root,
        draft.id(),
        workflow.config(),
        &profile,
        None,
    )
    .unwrap();
    manifest::decode(&serde_json::to_vec(&plan["environment"]["manifest"]).unwrap()).unwrap()
}

fn customize_generated_colors(version: u8, colors: &str) {
    let home = tempfile::tempdir().unwrap();
    let paths = InitPaths {
        home: home.path().to_owned(),
        xdg_config_home: Some(home.path().join("xdg")),
    };
    init(&paths).unwrap();
    let root = paths.managed_root();
    let profile = root.join("profiles/boy.toml");
    let original = format!(
        "schema_version = {version}\n{colors}\n[wallpaper]\nmode = \"source\"\nsource = \"welcome\"\nselection = \"path\"\npath = \"old.png\"\n"
    );
    fs::write(&profile, &original).unwrap();
    let bytes = include_bytes!("fixtures/white.png");
    fs::write(root.join("profiles/old.png"), bytes).unwrap();
    let workflow = ProfileWorkflows::load(paths.clone()).unwrap();
    let initial = resolve(&workflow, &paths);
    let mut editor = ProfileEditor::new(
        workflow.edit("boy").unwrap(),
        workflow.config().clone(),
        initial.clone(),
        Some(bytes.to_vec()),
        None,
    )
    .unwrap();
    assert_eq!(editor.preview().unwrap(), initial);
    assert_eq!(fs::read_to_string(&profile).unwrap(), original);

    editor.set_color(0, "#123456").unwrap();
    assert_eq!(
        editor.color_origin(0).unwrap(),
        "Customized",
        "successful exact-hex input must not silently discard an override in valid TOML"
    );
    assert_eq!(
        editor
            .preview()
            .unwrap()
            .colors()
            .unwrap()
            .background()
            .to_string(),
        "123456"
    );
    assert_eq!(editor.color_origin(1).unwrap(), "Automatic");
    assert_eq!(fs::read_to_string(&profile).unwrap(), original);

    editor
        .import_image(
            &workflow,
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/palette.png"),
        )
        .unwrap();
    assert_eq!(editor.color_origin(0).unwrap(), "Customized");
    workflow.save_draft(editor.draft()).unwrap();
    let saved = resolve(&workflow, &paths);
    assert_eq!(saved, editor.preview().unwrap());
    assert_eq!(saved.colors().unwrap().background().to_string(), "123456");
    assert_ne!(
        saved.colors().unwrap().foreground(),
        initial.colors().unwrap().foreground()
    );
    assert_eq!(fs::read(root.join("profiles/old.png")).unwrap(), bytes);
    assert_eq!(
        fs::read_dir(root.join("history/activations"))
            .unwrap()
            .count(),
        0
    );
}

#[test]
fn visual_editor_customizes_and_replaces_generated_colors_in_standard_table() {
    customize_generated_colors(1, "[colors]\nmode = \"generated\"");
}

#[test]
fn visual_editor_customizes_existing_inline_overrides() {
    customize_generated_colors(
        2,
        "colors = { mode = \"generated\", overrides = { foreground = \"auto\" } }",
    );
}

#[test]
fn visual_editor_customizes_legacy_inline_generated_colors_without_losing_hex_input() {
    customize_generated_colors(1, "colors = { mode = \"generated\" }");
}

#[test]
fn visual_editor_customizes_v2_inline_generated_colors_without_losing_hex_input() {
    customize_generated_colors(2, "colors = { mode = \"generated\" }");
}

#[test]
fn visual_editor_replaces_images_without_losing_dotted_or_nested_inline_overrides() {
    for colors in [
        "colors.mode = 'generated'\ncolors.overrides.foreground = 'auto'",
        "colors = { mode = 'generated', overrides.foreground = 'auto' }",
        "[colors]\nmode = 'generated'\noverrides = { foreground = 'auto' }",
        "[colors]\nmode = 'generated'\noverrides.foreground = 'auto'",
    ] {
        customize_generated_colors(2, colors);
    }
}

#[test]
fn generated_replacement_is_stable_until_explicit_regeneration_and_jpeg_import_clears_recipe() {
    let home = tempfile::tempdir().unwrap();
    let paths = InitPaths {
        home: home.path().to_owned(),
        xdg_config_home: Some(home.path().join("xdg")),
    };
    init(&paths).unwrap();
    let root = paths.managed_root();
    let profile = root.join("profiles/boy.toml");
    let original = "schema_version = 1\ncolors.mode = 'generated'\nwallpaper = { mode = 'source', source = 'welcome', selection = 'path', path = 'old.png' }\nterminal = { font_size = 12.125 }\n";
    fs::write(&profile, original).unwrap();
    let bytes = include_bytes!("fixtures/white.png");
    fs::write(root.join("profiles/old.png"), bytes).unwrap();
    let workflow = ProfileWorkflows::load(paths.clone()).unwrap();
    let mut editor = ProfileEditor::new(
        workflow.edit("boy").unwrap(),
        workflow.config().clone(),
        resolve(&workflow, &paths),
        Some(bytes.to_vec()),
        None,
    )
    .unwrap();
    editor.set_color(0, "#ABCDEF").unwrap();
    editor.set_color(20, "#12AB34").unwrap();
    let seed = "01".repeat(32).parse().unwrap();
    editor.generate_image(&workflow, seed).unwrap();
    let generated = editor.image().unwrap().to_vec();
    let generated_preview = editor.preview().unwrap();
    let Some(WallpaperIntent::Source {
        owned_image: Some(owned),
        ..
    }) = editor.intent().unwrap().wallpaper
    else {
        panic!("generated replacement needs an ownership claim")
    };
    assert_eq!(owned.generation.unwrap().seed, seed);

    editor.step_number(NumericControl::FontSize, true).unwrap();
    editor.step_number(NumericControl::FontSize, false).unwrap();
    let draft = editor.draft().document().to_owned();
    let broken = home.path().join("broken.png");
    fs::write(&broken, b"not an image").unwrap();
    assert!(editor.import_image(&workflow, &broken).is_err());
    assert_eq!(editor.draft().document(), draft);
    assert_eq!(editor.image().unwrap(), generated);
    assert_eq!(editor.preview().unwrap(), generated_preview);
    editor.generate_image(&workflow, seed).unwrap();
    assert_eq!(editor.image().unwrap(), generated);
    editor
        .generate_image(&workflow, "02".repeat(32).parse().unwrap())
        .unwrap();
    assert_ne!(editor.image().unwrap(), generated);

    let jpeg = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/white.jpg");
    let original_jpeg = fs::read(&jpeg).unwrap();
    editor.import_image(&workflow, &jpeg).unwrap();
    let Some(WallpaperIntent::Source {
        owned_image: Some(owned),
        ..
    }) = editor.intent().unwrap().wallpaper
    else {
        panic!("imported replacement needs an ownership claim")
    };
    assert!(owned.generation.is_none());
    assert_eq!(editor.color_origin(0).unwrap(), "Customized");
    assert_eq!(editor.color_origin(20).unwrap(), "Customized");
    assert_eq!(editor.color_origin(1).unwrap(), "Automatic");
    let preview = editor.preview().unwrap();
    assert_eq!(preview.colors().unwrap().background().to_string(), "abcdef");
    assert_eq!(
        preview.colors().unwrap().palette()[15].to_string(),
        "12ab34"
    );
    assert_eq!(
        preview.terminal().unwrap().font_size().unwrap().get(),
        12125
    );
    assert_eq!(fs::read_to_string(&profile).unwrap(), original);
    assert_eq!(fs::read_dir(root.join("profiles")).unwrap().count(), 4);
    workflow.save_draft(editor.draft()).unwrap();
    assert_eq!(resolve(&workflow, &paths), preview);
    assert_eq!(fs::read_dir(root.join("profiles")).unwrap().count(), 5);
    assert_eq!(fs::read(root.join("profiles/old.png")).unwrap(), bytes);
    assert_eq!(fs::read(jpeg).unwrap(), original_jpeg);
    assert_eq!(
        fs::read_dir(root.join("history/activations"))
            .unwrap()
            .count(),
        0
    );
}
