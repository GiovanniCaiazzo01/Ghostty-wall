use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use ghostty_wall::{
    apply::apply_local_profile,
    codec::{intent::parse_named_profile_toml, manifest},
    domain::{ColorsIntent, EnvironmentManifest, WallpaperIntent},
    history::inspect_history,
    init::{InitPaths, init},
    plan::plan_local_profile_json_uninspected,
    profile_editor::{NumericControl as N, ProfileEditor},
    profile_workflow::{ProfileWorkflows, WorkflowError},
};

struct Fixture {
    _temp: tempfile::TempDir,
    paths: InitPaths,
    workflow: ProfileWorkflows,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let paths = InitPaths {
            home: temp.path().to_owned(),
            xdg_config_home: Some(temp.path().join("xdg")),
        };
        init(&paths).unwrap();
        let root = paths.managed_root();
        fs::write(
            root.join("profiles/old.png"),
            include_bytes!("fixtures/white.png"),
        )
        .unwrap();
        fs::write(root.join("profiles/boy.toml"), "schema_version = 1\n[wallpaper]\nmode = \"source\"\nsource = \"welcome\"\nselection = \"path\"\npath = \"old.png\"\nfit = \"contain\"\nposition = \"bottom-right\"\nrepeat = true\nopacity = 0.123456\n[colors]\nmode = \"generated\"\n").unwrap();
        let workflow = ProfileWorkflows::load(paths.clone()).unwrap();
        Self {
            _temp: temp,
            paths,
            workflow,
        }
    }
    fn resolve(&self, name: &str) -> (EnvironmentManifest, Option<Vec<u8>>) {
        let draft = self.workflow.edit(name).unwrap();
        let (_, profile) =
            parse_named_profile_toml(name, self.workflow.config(), draft.document()).unwrap();
        let root = self.paths.managed_root();
        let plan = plan_local_profile_json_uninspected(
            &root,
            &self.paths.home,
            &root,
            draft.id(),
            self.workflow.config(),
            &profile,
            None,
        )
        .unwrap();
        (
            manifest::decode(&serde_json::to_vec(&plan["environment"]["manifest"]).unwrap())
                .unwrap(),
            plan.get("source").map(|source| {
                fs::read(
                    Path::new(source["resolved_root"].as_str().unwrap())
                        .join(plan["selection"]["candidate"].as_str().unwrap()),
                )
                .unwrap()
            }),
        )
    }
    fn open(&self) -> ProfileEditor {
        let (resolved, image) = self.resolve("boy");
        ProfileEditor::new(
            self.workflow.edit("boy").unwrap(),
            self.workflow.config().clone(),
            resolved,
            image,
            inspect_history(&self.paths.managed_root())
                .unwrap()
                .latest()
                .cloned(),
        )
        .unwrap()
    }
    fn apply(&self, name: &str) {
        let draft = self.workflow.edit(name).unwrap();
        let (_, profile) =
            parse_named_profile_toml(name, self.workflow.config(), draft.document()).unwrap();
        let root = self.paths.managed_root();
        apply_local_profile(
            &root,
            &self.paths.home,
            &root,
            &self.paths.home.join("xdg/ghostty/config.ghostty"),
            draft.id(),
            self.workflow.config(),
            &profile,
            None,
            "2026-09-23T08:31:15.123456Z",
            || Ok::<_, ()>(()),
        )
        .unwrap();
    }
    fn replacement(&self) -> PathBuf {
        let path = self.paths.home.join("original.png");
        fs::write(&path, include_bytes!("fixtures/palette.png")).unwrap();
        path
    }
}
fn files(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut result = BTreeMap::new();
    for entry in fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            result.extend(files(&path));
        } else {
            result.insert(path.clone(), fs::read(path).unwrap());
        }
    }
    result
}

#[test]
fn every_manual_slot_survives_replacement_while_automatic_slots_recalculate() {
    let f = Fixture::new();
    let mut editor = f.open();
    let before = files(&f.paths.managed_root());
    let original = editor.preview().unwrap();
    for slot in [0, 2, 3, 4, 5, 10, 20] {
        editor.set_color(slot, "#AABBCC").unwrap();
    }
    assert_eq!(editor.intent().unwrap().schema_version, 2);
    assert_eq!(editor.color_origin(1).unwrap(), "Automatic");
    assert_eq!(editor.color_origin(10).unwrap(), "Customized");
    let image = f.replacement();
    editor.import_image(&f.workflow, &image).unwrap();
    let new = editor.preview().unwrap();
    let colors = new.colors().unwrap();
    assert_eq!(colors.background().to_string(), "aabbcc");
    assert_eq!(colors.cursor().unwrap().to_string(), "aabbcc");
    assert_eq!(colors.selection_background().unwrap().to_string(), "aabbcc");
    assert_eq!(colors.selection_foreground().unwrap().to_string(), "aabbcc");
    for i in [0, 5, 15] {
        assert_eq!(colors.palette()[i].to_string(), "aabbcc");
    }
    assert_ne!(original.colors().unwrap().foreground(), colors.foreground());
    let Some(WallpaperIntent::Source {
        fit,
        position,
        repeat,
        opacity,
        ..
    }) = editor.intent().unwrap().wallpaper
    else {
        panic!()
    };
    assert_eq!(fit.unwrap().as_str(), "contain");
    assert_eq!(position.unwrap().as_str(), "bottom-right");
    assert_eq!(repeat, Some(true));
    assert_eq!(opacity.unwrap().get(), 123456);
    assert_eq!(files(&f.paths.managed_root()), before);
    editor.set_color(0, "auto").unwrap();
    assert_eq!(editor.color_origin(0).unwrap(), "Automatic");
    assert_ne!(
        editor
            .preview()
            .unwrap()
            .colors()
            .unwrap()
            .background()
            .to_string(),
        "aabbcc"
    );
    let preview = editor.preview().unwrap();
    f.workflow.save_draft(editor.draft()).unwrap();
    assert_eq!(
        f.resolve("boy").0,
        preview,
        "internal managed preview must agree with normal resolution"
    );
    f.apply("boy");
    assert_eq!(
        inspect_history(&f.paths.managed_root())
            .unwrap()
            .latest()
            .unwrap()
            .sequence(),
        1
    );
    assert_eq!(
        fs::read(image).unwrap(),
        include_bytes!("fixtures/palette.png")
    );
    assert_eq!(
        fs::read(f.paths.managed_root().join("profiles/old.png")).unwrap(),
        include_bytes!("fixtures/white.png")
    );
}

#[test]
fn first_scalar_or_palette_edit_round_trips_regular_inline_and_dotted_colors() {
    let f = Fixture::new();
    let path = f.paths.managed_root().join("profiles/boy.toml");
    for version in [1, 2] {
        for colors in [
            "[colors]\nmode = \"generated\"",
            "colors = { mode = \"generated\" }",
            "colors.mode = \"generated\"",
        ] {
            let document = format!(
                "schema_version = {version}\n{colors}\n[wallpaper]\nmode = \"source\"\nsource = \"welcome\"\nselection = \"path\"\npath = \"old.png\"\n"
            );
            for slot in [0, 5] {
                fs::write(&path, &document).unwrap();
                let before = files(&f.paths.managed_root());
                let mut editor = f.open();
                let automatic = editor.preview().unwrap();
                editor.set_color(slot, "#12AB34").unwrap();
                let intent = editor.intent().unwrap();
                assert_eq!(intent.schema_version, 2);
                let Some(ColorsIntent::GeneratedWithOverrides(overrides)) = intent.colors else {
                    panic!("lost customization: version {version}, slot {slot}, {colors}");
                };
                for (index, color) in overrides
                    .scalars
                    .iter()
                    .chain(&overrides.palette)
                    .enumerate()
                {
                    assert_eq!(
                        *color,
                        (index == slot).then(|| "12ab34".parse().unwrap()),
                        "version {version}, slot {slot}, {colors}, override {index}"
                    );
                    assert_eq!(
                        editor.color_origin(index).unwrap(),
                        if index == slot {
                            "Customized"
                        } else {
                            "Automatic"
                        }
                    );
                }
                let preview = editor.preview().unwrap();
                assert_ne!(preview, automatic);
                assert_eq!(files(&f.paths.managed_root()), before);
                f.workflow.save_draft(editor.draft()).unwrap();
                assert_eq!(f.resolve("boy").0, preview);

                let mut reopened = f.open();
                assert_eq!(reopened.color_origin(slot).unwrap(), "Customized");
                reopened.set_color(slot, "auto").unwrap();
                assert_eq!(reopened.color_origin(slot).unwrap(), "Automatic");
                assert_eq!(reopened.preview().unwrap(), automatic);
                f.workflow.save_draft(reopened.draft()).unwrap();
                assert_eq!(f.resolve("boy").0, automatic);
                assert!(
                    inspect_history(&f.paths.managed_root())
                        .unwrap()
                        .latest()
                        .is_none()
                );
            }
        }
    }
}

#[test]
fn exact_numbers_choices_and_invalid_inputs_preserve_draft_and_saved_bytes() {
    let f = Fixture::new();
    let before = files(&f.paths.managed_root());
    let mut editor = f.open();
    for (control, input, expected) in [
        (N::WallpaperOpacity, "0.123456", 123456),
        (N::BackgroundOpacity, "0.5", 500000),
        (N::FontSize, "13.123", 13123),
        (N::Blur, "42", 42),
    ] {
        editor.set_number(control, input).unwrap();
        assert_eq!(editor.number(control).unwrap(), Some(expected));
        editor.step_number(control, true).unwrap();
        editor.step_number(control, false).unwrap();
        assert_eq!(editor.number(control).unwrap(), Some(expected));
    }
    editor
        .set_choice("terminal", "cursor_style", "underline")
        .unwrap();
    for (section, field, value) in [
        ("wallpaper", "fit", "stretch"),
        ("wallpaper", "position", "top-left"),
        ("wallpaper", "repeat", "false"),
    ] {
        editor.set_choice(section, field, value).unwrap();
    }
    let draft = editor.draft().document().to_owned();
    for (control, input) in [
        (N::WallpaperOpacity, "0.1234567"),
        (N::BackgroundOpacity, "1.01"),
        (N::FontSize, "1e2"),
        (N::FontSize, "13.0001"),
        (N::FontSize, "0"),
        (N::Blur, "256"),
        (N::Blur, "1.5"),
        (N::FontSize, "1\nextra=2"),
    ] {
        assert!(editor.set_number(control, input).is_err());
    }
    for bad in ["ff", "#zz1122", "abcdef0", "😺"] {
        assert!(editor.set_color(0, bad).is_err());
    }
    assert!(editor.set_color(21, "123456").is_err());
    assert!(editor.set_choice("wallpaper", "fit", "bogus").is_err());
    assert_eq!(editor.draft().document(), draft);
    assert_eq!(
        editor
            .preview()
            .unwrap()
            .terminal()
            .unwrap()
            .font_size()
            .unwrap()
            .get(),
        13123
    );
    drop(editor);
    assert_eq!(files(&f.paths.managed_root()), before);
}

#[test]
fn replacing_legacy_unmanaged_explicit_or_theme_colors_does_not_reset_them() {
    let f = Fixture::new();
    let path = f.paths.managed_root().join("profiles/boy.toml");
    let original = fs::read_to_string(&path).unwrap();
    let (base, image) = f.resolve("boy");
    let explicit = format!(
        "[colors]\nmode = \"explicit\"\nbackground = \"112233\"\nforeground = \"ffffff\"\npalette = [{}]\n",
        ["\"112233\""; 16].join(",")
    );
    for colors in [
        "",
        &explicit,
        "[colors]\nmode = \"theme\"\ntheme = \"Example\"\n",
    ] {
        let doc = original.replace("[colors]\nmode = \"generated\"\n", colors);
        fs::write(&path, &doc).unwrap();
        let mut editor = ProfileEditor::new(
            f.workflow.edit("boy").unwrap(),
            f.workflow.config().clone(),
            base.clone(),
            image.clone(),
            None,
        )
        .unwrap();
        let before = editor.intent().unwrap().colors;
        editor.import_image(&f.workflow, &f.replacement()).unwrap();
        assert_eq!(editor.intent().unwrap().colors, before);
        assert_eq!(fs::read_to_string(&path).unwrap(), doc);
        if before.is_none() {
            assert!(editor.preview().unwrap().colors().is_none());
            editor.automatic_colors().unwrap();
            assert_eq!(editor.color_origin(0).unwrap(), "Automatic");
        } else {
            editor.set_color(1, "ff1234").unwrap();
            assert!(matches!(
                editor.intent().unwrap().colors,
                Some(ColorsIntent::Explicit { .. })
            ));
            editor.set_color(0, "auto").unwrap();
            assert_eq!(editor.color_origin(0).unwrap(), "Automatic");
            assert_eq!(
                editor
                    .preview()
                    .unwrap()
                    .colors()
                    .unwrap()
                    .foreground()
                    .to_string(),
                "ff1234"
            );
        }
    }
}

#[test]
fn cancel_keeps_opening_environment_but_never_rolls_back_newer_apply_and_save_detects_conflict() {
    let f = Fixture::new();
    f.apply("welcome");
    let before = files(&f.paths.managed_root());
    let mut editor = f.open();
    assert_eq!(
        editor
            .starting_activation()
            .unwrap()
            .profile_id()
            .unwrap()
            .as_str(),
        "welcome"
    );
    editor.set_number(N::FontSize, "19").unwrap();
    assert_eq!(files(&f.paths.managed_root()), before);
    f.apply("boy");
    let newer = files(&f.paths.managed_root());
    drop(editor);
    assert_eq!(files(&f.paths.managed_root()), newer);
    let editor = f.open();
    let path = f.paths.managed_root().join("profiles/boy.toml");
    fs::write(
        &path,
        format!("{}\n# competing edit\n", editor.draft().document()),
    )
    .unwrap();
    assert!(matches!(
        f.workflow.save_draft(editor.draft()),
        Err(WorkflowError::Changed(_))
    ));
}

#[test]
fn empty_profile_allows_terminal_and_image_edits_without_implicit_color_defaults() {
    let f = Fixture::new();
    fs::write(
        f.paths.managed_root().join("profiles/boy.toml"),
        "schema_version = 1\n",
    )
    .unwrap();
    let mut editor = f.open();
    assert_eq!(
        editor.preview().unwrap(),
        EnvironmentManifest::new(None, None, None)
    );
    editor.set_number(N::FontSize, "14.5").unwrap();
    assert!(editor.preview().unwrap().colors().is_none());
    editor.import_image(&f.workflow, &f.replacement()).unwrap();
    assert!(editor.preview().unwrap().colors().is_none());
    editor.automatic_colors().unwrap();
    assert!(editor.preview().unwrap().colors().is_some());
    f.workflow.save_draft(editor.draft()).unwrap();
    assert_eq!(f.resolve("boy").0, editor.preview().unwrap());
}
