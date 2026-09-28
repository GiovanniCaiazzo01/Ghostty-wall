use std::{fs, path::Path, str::FromStr};

use ghostty_wall::{
    apply::apply_local_profile,
    codec::intent::parse_named_profile_toml,
    domain::Sha256Digest,
    init::{InitPaths, init},
    profile_workflow::{ProfileOutcome, ProfileWorkflows, WorkflowError},
    runtime::ReloadOutcome,
};

fn fixture() -> (tempfile::TempDir, InitPaths) {
    let tmp = tempfile::tempdir().unwrap();
    let paths = InitPaths {
        home: tmp.path().to_owned(),
        xdg_config_home: Some(tmp.path().join("xdg")),
    };
    init(&paths).unwrap();
    (tmp, paths)
}

fn seed() -> Sha256Digest {
    Sha256Digest::from_bytes([31; 32])
}

#[test]
fn adding_first_wallpaper_to_existing_profile_keeps_editor_semantics() {
    let (_tmp, paths) = fixture();
    let workflow = ProfileWorkflows::load(paths.clone()).unwrap();
    let image = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/white.png");
    for (id, wallpaper, expected) in [
        ("unmanaged", "", Some(0.1)),
        ("disabled", "[wallpaper]\nmode = \"none\"\n", None),
    ] {
        let path = paths.managed_root().join(format!("profiles/{id}.toml"));
        fs::write(&path, format!("schema_version = 2\n{wallpaper}")).unwrap();
        let mut draft = workflow.edit(id).unwrap();
        workflow.import_image(&mut draft, &image).unwrap();
        let document = draft.document().parse::<toml_edit::DocumentMut>().unwrap();
        assert_eq!(
            document["wallpaper"]
                .get("opacity")
                .and_then(|v| v.as_float()),
            expected
        );
        assert!(document.get("terminal").is_none());
    }
}

#[test]
fn existing_image_replacement_preserves_explicit_and_omitted_opacity() {
    let (_tmp, paths) = fixture();
    let workflow = ProfileWorkflows::load(paths.clone()).unwrap();
    let image = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/white.png");
    for (id, opacity, expected) in [
        ("explicit", "opacity = 0.37\n", Some(0.37)),
        ("omitted", "", None),
    ] {
        let path = paths.managed_root().join(format!("profiles/{id}.toml"));
        fs::write(&path, format!("schema_version = 2\n[wallpaper]\nmode = \"source\"\nsource = \"welcome\"\nselection = \"path\"\npath = \"welcome.png\"\n{opacity}\n[terminal]\nbackground_opacity = 0.84\n")).unwrap();
        let original = fs::read(&path).unwrap();
        let mut draft = workflow.edit(id).unwrap();
        workflow.import_image(&mut draft, &image).unwrap();
        let document = draft.document().parse::<toml_edit::DocumentMut>().unwrap();
        assert_eq!(
            document["wallpaper"]
                .get("opacity")
                .and_then(|v| v.as_float()),
            expected
        );
        assert_eq!(
            document["terminal"]["background_opacity"].as_float(),
            Some(0.84)
        );
        assert_eq!(fs::read(&path).unwrap(), original);
        workflow.save(draft).unwrap();
        let saved = workflow.edit(id).unwrap();
        let document = saved.document().parse::<toml_edit::DocumentMut>().unwrap();
        assert_eq!(
            document["wallpaper"]
                .get("opacity")
                .and_then(|v| v.as_float()),
            expected
        );
    }
}

#[test]
fn new_draft_image_replacement_keeps_explicit_opacity_and_terminal_transparency() {
    let (_tmp, paths) = fixture();
    let workflow = ProfileWorkflows::load(paths).unwrap();
    let mut draft = workflow.create("chosen").unwrap();
    assert!(!draft.document().contains("opacity"));
    let image = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/white.png");
    workflow.import_image(&mut draft, &image).unwrap();
    let mut document = draft.document().parse::<toml_edit::DocumentMut>().unwrap();
    document["wallpaper"]["opacity"] = toml_edit::value(0.37);
    document["terminal"]["background_opacity"] = toml_edit::value(0.84);
    draft
        .set_document(workflow.config(), document.to_string())
        .unwrap();
    workflow.import_image(&mut draft, &image).unwrap();
    let document = draft.document().parse::<toml_edit::DocumentMut>().unwrap();
    assert_eq!(document["wallpaper"]["opacity"].as_float(), Some(0.37));
    assert_eq!(
        document["terminal"]["background_opacity"].as_float(),
        Some(0.84)
    );
    workflow.save(draft).unwrap();
}

#[test]
fn draft_import_cancel_collision_and_complete_generated_colors() {
    let (_tmp, paths) = fixture();
    let workflow = ProfileWorkflows::load(paths.clone()).unwrap();
    assert!(workflow.create("Invalid Name").is_err());
    let original = paths.home.join("original.png");
    fs::copy(paths.managed_root().join("profiles/welcome.png"), &original).unwrap();
    let mut draft = workflow.create("personal").unwrap();
    assert!(workflow.save(workflow.create("empty").unwrap()).is_err());
    workflow.import_image(&mut draft, &original).unwrap();
    let generated = draft.generated_colors().unwrap();
    assert_eq!(generated.palette().len(), 16);
    let bytes = fs::read(&original).unwrap();
    drop(draft);
    assert!(!paths.managed_root().join("profiles/personal.toml").exists());
    assert!(!paths.managed_root().join("profiles/personal.png").exists());
    let mut draft = workflow.create("personal").unwrap();
    workflow.import_image(&mut draft, &original).unwrap();
    assert_eq!(workflow.save(draft).unwrap().as_str(), "personal");
    assert_eq!(fs::read(&original).unwrap(), bytes);
    assert_eq!(
        fs::read(paths.managed_root().join("profiles/personal.png")).unwrap(),
        bytes
    );
    assert!(matches!(
        workflow.create("personal"),
        Err(WorkflowError::Collision(_))
    ));
    assert!(workflow.delete("personal").unwrap());
    assert!(!paths.managed_root().join("profiles/personal.png").exists());
    assert_eq!(fs::read(&original).unwrap(), bytes);
    assert!(workflow.edit("missing").is_err());
    assert!(matches!(
        workflow.delete("welcome"),
        Err(WorkflowError::Invalid(_))
    ));
}

#[test]
fn save_apply_reload_are_distinct_and_active_deletion_fails_closed() {
    let (_tmp, paths) = fixture();
    let workflow = ProfileWorkflows::load(paths.clone()).unwrap();
    let mut draft = workflow.create("generated").unwrap();
    workflow.generate_image(&mut draft, seed()).unwrap();
    assert!(draft.document().contains("gradient-v1"));
    let id = workflow.save(draft).unwrap();
    assert!(matches!(
        workflow.use_saved(&id, |_| Err::<ghostty_wall::apply::ApplyOutcome, _>(
            "offline"
        )),
        Err(WorkflowError::Apply(_))
    ));
    assert!(
        paths
            .managed_root()
            .join("profiles/generated.toml")
            .exists()
    );
    assert!(
        !paths
            .managed_root()
            .join("history/activations/act-v1-0000000000000001.json")
            .exists()
    );
    let root = paths.managed_root();
    let config = workflow.config();
    let (_, profile) = parse_named_profile_toml(
        id.as_str(),
        config,
        &fs::read_to_string(root.join("profiles/generated.toml")).unwrap(),
    )
    .unwrap();
    let result = workflow
        .use_saved(&id, |id| {
            apply_local_profile(
                &root,
                &paths.home,
                &root,
                &paths.home.join("xdg/ghostty/config.ghostty"),
                id,
                config,
                &profile,
                None,
                "2026-09-23T08:31:15.123456Z",
                || Err::<(), _>("reload unavailable"),
            )
        })
        .unwrap();
    assert!(matches!(
        result,
        ProfileOutcome::SavedAndApplied {
            reload: ReloadOutcome::Failed(_),
            ..
        }
    ));
    assert!(matches!(
        workflow.delete("generated"),
        Err(WorkflowError::Active(_))
    ));
    assert!(root.join("profiles/generated.png").exists());
    assert!(root.join("current.ghostty").exists());
    assert!(
        root.join("history/activations/act-v1-0000000000000001.json")
            .exists()
    );
}

#[test]
fn edit_conflict_and_inactive_cleanup_preserve_history_and_shared_images() {
    let (_tmp, paths) = fixture();
    let root = paths.managed_root();
    let workflow = ProfileWorkflows::load(paths.clone()).unwrap();
    let mut draft = workflow.create("other").unwrap();
    workflow.generate_image(&mut draft, seed()).unwrap();
    workflow.save(draft).unwrap();
    let stale = workflow.edit("other").unwrap();
    let path = root.join("profiles/other.toml");
    fs::write(
        &path,
        format!("{}\n# changed\n", fs::read_to_string(&path).unwrap()),
    )
    .unwrap();
    assert!(matches!(
        workflow.save(stale),
        Err(WorkflowError::Changed(_))
    ));
    fs::write(root.join("profiles/ref.toml"), "schema_version = 1\n[wallpaper]\nmode = \"source\"\nsource = \"welcome\"\nselection = \"path\"\npath = \"other.png\"\n").unwrap();
    assert!(workflow.delete("other").unwrap());
    assert!(root.join("profiles/other.png").exists());
    assert!(!workflow.delete("other").unwrap());
    assert!(Path::new(&paths.home).exists());
}

#[test]
fn generated_replacement_preserves_manual_slots_and_original_image() {
    let (_tmp, paths) = fixture();
    let root = paths.managed_root();
    let workflow = ProfileWorkflows::load(paths).unwrap();
    let mut draft = workflow.create("change-me").unwrap();
    workflow.generate_image(&mut draft, seed()).unwrap();
    let revised = draft.document().replace(
        "mode = \"generated\"",
        "mode = \"generated\"\n[colors.overrides]\nbackground = \"abcdef\"",
    );
    draft.set_document(workflow.config(), revised).unwrap();
    workflow.save(draft).unwrap();
    let original = fs::read(root.join("profiles/change-me.png")).unwrap();
    let mut editing = workflow.edit("change-me").unwrap();
    workflow
        .generate_image(&mut editing, Sha256Digest::from_bytes([42; 32]))
        .unwrap();
    assert!(editing.document().contains("background = \"abcdef\""));
    assert!(editing.document().contains("change-me-"));
    workflow.save(editing).unwrap();
    assert_eq!(
        fs::read(root.join("profiles/change-me.png")).unwrap(),
        original
    );
    let (_, parsed) = parse_named_profile_toml(
        "change-me",
        workflow.config(),
        &fs::read_to_string(root.join("profiles/change-me.toml")).unwrap(),
    )
    .unwrap();
    assert!(matches!(
        parsed.colors,
        Some(ghostty_wall::domain::ColorsIntent::GeneratedWithOverrides(
            _
        ))
    ));
    assert!(workflow.delete("change-me").unwrap());
    assert!(root.join("profiles/change-me.png").exists()); // not proven exclusive to the edited recipe
}

#[test]
fn registry_drift_aborts_before_publishing() {
    let (_tmp, paths) = fixture();
    let workflow = ProfileWorkflows::load(paths.clone()).unwrap();
    let mut draft = workflow.create("drift").unwrap();
    workflow.generate_image(&mut draft, seed()).unwrap();
    let config = paths.managed_root().join("config.toml");
    fs::write(
        &config,
        format!(
            "{}\n[sources.more]\nkind = \"local-directory\"\npath = \"other\"\n",
            fs::read_to_string(&config).unwrap()
        ),
    )
    .unwrap();
    assert!(matches!(
        workflow.save(draft),
        Err(WorkflowError::Invalid(_))
    ));
    assert!(!paths.managed_root().join("profiles/drift.toml").exists());
    assert!(!paths.managed_root().join("profiles/drift.png").exists());
}

#[test]
fn old_v1_profile_and_inactive_delete_preserve_committed_history_and_projection() {
    let (_tmp, paths) = fixture();
    let root = paths.managed_root();
    let workflow = ProfileWorkflows::load(paths.clone()).unwrap();
    let mut draft = workflow.create("personal").unwrap();
    workflow.generate_image(&mut draft, seed()).unwrap();
    let personal = workflow.save(draft).unwrap();
    let apply = |id: &ghostty_wall::domain::IntentId, at| {
        let (_, profile) = parse_named_profile_toml(
            id.as_str(),
            workflow.config(),
            &fs::read_to_string(root.join(format!("profiles/{id}.toml"))).unwrap(),
        )
        .unwrap();
        workflow.use_saved(id, |id| {
            apply_local_profile(
                &root,
                &paths.home,
                &root,
                &paths.home.join("xdg/ghostty/config.ghostty"),
                id,
                workflow.config(),
                &profile,
                None,
                at,
                || Err::<(), _>("no live Ghostty in isolated fixture"),
            )
        })
    };
    assert!(matches!(
        apply(&personal, "2026-09-23T08:31:15.123456Z"),
        Ok(ProfileOutcome::SavedAndApplied {
            reload: ReloadOutcome::Failed(_),
            ..
        })
    ));
    let welcome = ghostty_wall::domain::IntentId::from_str("welcome").unwrap();
    let mut old_draft = workflow.edit("welcome").unwrap();
    let with_comment = format!("{}\n# retained v1\n", old_draft.document());
    old_draft
        .set_document(workflow.config(), with_comment)
        .unwrap();
    workflow.save(old_draft).unwrap();
    assert!(
        fs::read_to_string(root.join("profiles/welcome.toml"))
            .unwrap()
            .starts_with("schema_version = 1")
    );
    assert!(apply(&welcome, "2026-09-23T08:31:16.123456Z").is_ok());
    let before_projection = fs::read(root.join("current.ghostty")).unwrap();
    let before_history = fs::read_dir(root.join("history/activations"))
        .unwrap()
        .count();
    assert_eq!(before_history, 2);
    assert!(workflow.delete("personal").unwrap());
    assert_eq!(
        fs::read(root.join("current.ghostty")).unwrap(),
        before_projection
    );
    assert_eq!(
        fs::read_dir(root.join("history/activations"))
            .unwrap()
            .count(),
        before_history
    );
    assert!(root.join("profiles/welcome.toml").exists());
    assert!(root.join("assets/sha256").exists());
}

#[test]
fn corrupt_history_and_missing_lock_fail_closed_on_delete() {
    let (_tmp, paths) = fixture();
    let root = paths.managed_root();
    let workflow = ProfileWorkflows::load(paths).unwrap();
    let mut draft = workflow.create("victim").unwrap();
    workflow.generate_image(&mut draft, seed()).unwrap();
    workflow.save(draft).unwrap();
    let intent = root.join("profiles/victim.toml");
    let image = root.join("profiles/victim.png");
    let record = root.join("history/activations/act-v1-0000000000000001.json");
    fs::write(&record, b"not a valid activation").unwrap();
    assert!(matches!(
        workflow.delete("victim"),
        Err(WorkflowError::History(_))
    ));
    assert!(intent.exists() && image.exists());
    fs::remove_file(record).unwrap();
    fs::remove_file(root.join("state.lock")).unwrap();
    assert!(workflow.delete("victim").is_err());
    assert!(intent.exists() && image.exists());
}

#[test]
fn image_collision_fails_before_profile_publication_and_preserves_existing_bytes() {
    let (_tmp, paths) = fixture();
    let root = paths.managed_root();
    let workflow = ProfileWorkflows::load(paths).unwrap();
    let mut draft = workflow.create("collision").unwrap();
    workflow.generate_image(&mut draft, seed()).unwrap();
    let image = root.join("profiles/collision.png");
    let existing = fs::read(root.join("profiles/welcome.png")).unwrap();
    fs::write(&image, &existing).unwrap();
    assert!(matches!(
        workflow.save(draft),
        Err(WorkflowError::Collision(_))
    ));
    assert_eq!(fs::read(image).unwrap(), existing);
    assert!(!root.join("profiles/collision.toml").exists());
}

#[test]
fn reusing_identical_candidate_never_grants_cleanup_ownership() {
    for generated in [false, true] {
        let (_tmp, paths) = fixture();
        let root = paths.managed_root();
        let workflow = ProfileWorkflows::load(paths).unwrap();
        let mut draft = workflow.create("reused").unwrap();
        if generated {
            workflow.generate_image(&mut draft, seed()).unwrap();
        } else {
            workflow
                .import_image(
                    &mut draft,
                    Path::new(concat!(
                        env!("CARGO_MANIFEST_DIR"),
                        "/tests/fixtures/white.png"
                    )),
                )
                .unwrap();
        }
        let bytes = draft.staged_image().unwrap().to_vec();
        let existing = root.join("profiles/reused.png");
        fs::write(&existing, &bytes).unwrap();
        workflow.save(draft).unwrap();
        let document = fs::read_to_string(root.join("profiles/reused.toml")).unwrap();
        assert!(!document.contains("owned_sha256"));
        assert!(!document.contains("generation"));
        parse_named_profile_toml("reused", workflow.config(), &document).unwrap();
        workflow.delete("reused").unwrap();
        assert_eq!(fs::read(existing).unwrap(), bytes);
        assert!(!root.join("profiles/reused.toml").exists());
    }
}

#[test]
fn staged_image_source_redirection_rejected_before_publishing() {
    let (_tmp, paths) = fixture();
    let root = paths.managed_root();
    fs::create_dir(root.join("elsewhere")).unwrap();
    let config = root.join("config.toml");
    fs::write(
        &config,
        format!(
            "{}\n[sources.other]\nkind = \"local-directory\"\npath = \"elsewhere\"\n",
            fs::read_to_string(&config).unwrap()
        ),
    )
    .unwrap();
    let workflow = ProfileWorkflows::load(paths).unwrap();
    let mut draft = workflow.create("broken").unwrap();
    workflow.generate_image(&mut draft, seed()).unwrap();
    let redirected = draft
        .document()
        .replace("source = \"welcome\"", "source = \"other\"");
    assert!(draft.set_document(workflow.config(), redirected).is_err());
    drop(draft);
    assert!(!root.join("profiles/broken.toml").exists());
    assert!(!root.join("profiles/broken.png").exists());
}
