use super::*;
use crate::{
    apply::apply_local_profile,
    init::init,
    plan::{PlanPlatform, plan_local_profile_json, planned_local_asset_bytes},
    recovery::try_exclusive_state_lock,
};

const AT: &str = "2026-09-23T08:31:15.123456Z";

fn fixture() -> (tempfile::TempDir, ProfileWorkflows) {
    let tmp = tempfile::tempdir().unwrap();
    let paths = InitPaths {
        home: tmp.path().into(),
        xdg_config_home: Some(tmp.path().join("config")),
    };
    init(&paths).unwrap();
    let workflow = ProfileWorkflows::load(paths).unwrap();
    let mut draft = workflow.create("boy").unwrap();
    workflow
        .import_image(
            &mut draft,
            Path::new(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/white.png"
            )),
        )
        .unwrap();
    workflow.save(draft).unwrap();
    (tmp, workflow)
}

fn resolve(
    workflow: &ProfileWorkflows,
    id: &IntentId,
    intent: &ProfileIntent,
) -> Result<(Value, Option<Vec<u8>>), crate::plan::PlanError> {
    let root = workflow.paths.managed_root();
    let reload = || Ok::<_, &'static str>(());
    let platform = PlanPlatform::new(workflow.paths.ghostty_root_config(), reload.observation());
    let plan = plan_local_profile_json(
        &root,
        &workflow.paths.home,
        &root,
        id,
        &workflow.config,
        intent,
        None,
        &platform,
    )?;
    let bytes = planned_local_asset_bytes(&plan)?;
    Ok((plan, bytes))
}

fn apply(workflow: &ProfileWorkflows, name: &str) {
    let draft = workflow.edit(name).unwrap();
    let (_, intent) = parse_named_profile_toml(name, &workflow.config, draft.document()).unwrap();
    let root = workflow.paths.managed_root();
    apply_local_profile(
        &root,
        &workflow.paths.home,
        &root,
        &workflow.paths.ghostty_root_config(),
        draft.id(),
        &workflow.config,
        &intent,
        None,
        AT,
        || Ok::<_, &'static str>(()),
    )
    .unwrap();
}

#[test]
fn changed_current_activation_aborts_confirmed_delete_without_stale_fallback() {
    let (_tmp, workflow) = fixture();
    apply(&workflow, "boy");
    let request = workflow.prepare_deletion("boy").unwrap();
    let root = workflow.paths.managed_root();
    let error = workflow
        .confirm_deletion(
            &request,
            AT,
            |id, intent| {
                // A competing apply can commit while fallback resolution is outside the lock.
                let resolved = resolve(&workflow, id, intent)?;
                apply(&workflow, "welcome");
                Ok::<_, crate::plan::PlanError>(resolved)
            },
            || -> Result<(), &'static str> { panic!("stale deletion must not reload") },
        )
        .err()
        .unwrap();
    assert!(matches!(error, WorkflowError::DeletionChanged(_)));
    assert!(root.join("profiles/boy.toml").exists());
    assert!(root.join("profiles/boy.png").exists());
    let history = crate::history::inspect_history(&root).unwrap();
    assert_eq!(history.activations().len(), 2);
    assert_eq!(
        history.latest().unwrap().profile_id().unwrap().as_str(),
        "welcome"
    );
}

#[test]
fn changed_profile_or_registry_requires_new_confirmation() {
    for change in ["boy", "welcome", "config"] {
        let (_tmp, workflow) = fixture();
        apply(&workflow, "boy");
        let request = workflow.prepare_deletion("boy").unwrap();
        let root = workflow.paths.managed_root();
        let before_projection = fs::read(root.join("current.ghostty")).unwrap();
        let result = workflow.confirm_deletion(
            &request,
            AT,
            |id, intent| {
                let resolved = resolve(&workflow, id, intent)?;
                let path = if change == "config" {
                    root.join("config.toml")
                } else {
                    root.join(format!("profiles/{change}.toml"))
                };
                let suffix = if change == "config" {
                    "\n[sources.extra]\nkind = \"local-directory\"\npath = \"other\"\n"
                } else {
                    "\n# concurrent edit\n"
                };
                fs::write(
                    &path,
                    format!("{}{suffix}", fs::read_to_string(&path).unwrap()),
                )
                .unwrap();
                Ok::<_, crate::plan::PlanError>(resolved)
            },
            || -> Result<(), &'static str> { panic!("changed state must not reload") },
        );
        assert!(result.is_err(), "{change}");
        assert!(root.join("profiles/boy.toml").exists());
        assert!(root.join("profiles/boy.png").exists());
        assert_eq!(
            fs::read(root.join("current.ghostty")).unwrap(),
            before_projection
        );
        assert_eq!(
            crate::history::inspect_history(&root)
                .unwrap()
                .activations()
                .len(),
            1
        );
    }
}

#[test]
fn reload_happens_after_durable_fallback_removal_and_lock_release() {
    let (_tmp, workflow) = fixture();
    apply(&workflow, "boy");
    let request = workflow.prepare_deletion("boy").unwrap();
    let root = workflow.paths.managed_root();
    let outcome = workflow
        .confirm_deletion(
            &request,
            AT,
            |id, intent| resolve(&workflow, id, intent),
            || {
                let lock = try_exclusive_state_lock(&root.join("state.lock")).unwrap();
                assert!(lock.is_some(), "reload must not hold writer lock");
                let history = inspect_history_unlocked(&root).unwrap();
                assert_eq!(
                    history.latest().unwrap().profile_id().unwrap().as_str(),
                    "welcome"
                );
                assert!(!root.join("profiles/boy.toml").exists());
                assert!(!root.join("profiles/boy.png").exists());
                Err::<(), _>("injected reload failure")
            },
        )
        .unwrap();
    assert!(matches!(
        outcome.fallback,
        Some((_, ReloadOutcome::Failed(_)))
    ));
    assert!(root.join("profiles/welcome.toml").exists());
}

#[test]
fn inactive_deletion_never_resolves_reconciles_or_reloads_even_with_interrupted_preview() {
    let (_tmp, workflow) = fixture();
    let root = workflow.paths.managed_root();
    fs::write(
        root.join("preview.session"),
        "invalid marker left for recovery",
    )
    .unwrap();
    fs::write(root.join("current.ghostty"), "draft Projection").unwrap();
    let request = workflow.prepare_deletion("boy").unwrap();
    workflow
        .confirm_deletion(
            &request,
            AT,
            |_, _| -> Result<_, &'static str> { panic!("inactive must not resolve") },
            || -> Result<(), &'static str> { panic!("inactive must not reload") },
        )
        .unwrap();
    assert_eq!(
        fs::read(root.join("current.ghostty")).unwrap(),
        b"draft Projection"
    );
    assert_eq!(
        fs::read(root.join("preview.session")).unwrap(),
        b"invalid marker left for recovery"
    );
    assert!(
        crate::history::inspect_history(&root)
            .unwrap()
            .latest()
            .is_none()
    );
}

#[test]
fn byte_identical_replacements_do_not_inherit_confirmation() {
    for name in ["boy.toml", "boy.png"] {
        let (_tmp, workflow) = fixture();
        let root = workflow.paths.managed_root();
        let path = root.join("profiles").join(name);
        let request = workflow.prepare_deletion("boy").unwrap();
        let bytes = fs::read(&path).unwrap();
        fs::remove_file(&path).unwrap();
        fs::write(&path, &bytes).unwrap();
        let result = workflow.confirm_deletion(
            &request,
            AT,
            |_, _| Err("inactive must not resolve"),
            || -> Result<(), &'static str> { panic!("inactive must not reload") },
        );
        if name == "boy.toml" {
            assert!(matches!(result, Err(WorkflowError::DeletionChanged(_))));
        } else {
            assert!(result.unwrap().cleanup.contains("Images retained"));
            assert!(!root.join("profiles/boy.toml").exists());
        }
        assert_eq!(fs::read(path).unwrap(), bytes);
        assert!(root.join("profiles/boy.png").exists());
        assert!(
            crate::history::inspect_history(&root)
                .unwrap()
                .latest()
                .is_none()
        );
    }
}

#[cfg(unix)]
#[test]
fn substituted_profile_directory_aborts_before_active_fallback() {
    for symlink in [false, true] {
        let (tmp, workflow) = fixture();
        apply(&workflow, "boy");
        let root = workflow.paths.managed_root();
        let dir = root.join("profiles");
        let request = workflow.prepare_deletion("boy").unwrap();
        let projection = fs::read(root.join("current.ghostty")).unwrap();
        let outside = tmp.path().join("originals");
        fs::create_dir(&outside).unwrap();
        for name in ["boy.toml", "boy.png"] {
            fs::copy(dir.join(name), outside.join(name)).unwrap();
        }
        fs::rename(&dir, root.join("saved-profiles")).unwrap();
        if symlink {
            std::os::unix::fs::symlink(&outside, &dir).unwrap();
        } else {
            fs::rename(&outside, &dir).unwrap();
        }
        let result = workflow.confirm_deletion(
            &request,
            AT,
            |_, _| -> Result<_, &'static str> { panic!("changed directory must not resolve") },
            || -> Result<(), &'static str> { panic!("changed directory must not reload") },
        );
        assert!(result.is_err());
        for name in ["boy.toml", "boy.png"] {
            assert_eq!(
                fs::read(dir.join(name)).unwrap(),
                fs::read(root.join("saved-profiles").join(name)).unwrap()
            );
        }
        assert_eq!(fs::read(root.join("current.ghostty")).unwrap(), projection);
        assert_eq!(
            crate::history::inspect_history(&root)
                .unwrap()
                .activations()
                .len(),
            1
        );
    }
}

#[test]
fn renewed_image_proof_considers_the_removed_id_as_a_live_reference() {
    let (_tmp, workflow) = fixture();
    let request = workflow.prepare_deletion("boy").unwrap();
    assert!(
        request.image.is_some(),
        "pre-removal proof excludes confirmed target"
    );
    let dir = &request.directory;
    let prove = || {
        exclusive_image(
            &workflow.config,
            dir,
            &request.id,
            request.original.text().unwrap(),
            None,
        )
    };
    assert!(
        prove().is_none(),
        "all-live proof must include even the original target"
    );
    dir.remove(&request.original, MAX_INTENT).unwrap();
    assert!(prove().is_some(), "owned image is exclusive after removal");
    for document in [
        request.original.text().unwrap(),
        "schema_version = 1\n[wallpaper]\nmode = 'source'\nsource = 'welcome'\nselection = 'random'\n",
        "schema_version = 999\n",
    ] {
        fs::write(dir.path(&request.original.name), document).unwrap();
        assert!(
            prove().is_none(),
            "replacement cannot inherit the pre-removal exemption"
        );
        let error = dir.require_absent(&request.original.name).unwrap_err();
        assert!(error.to_string().contains("start delete again"));
        assert_eq!(
            fs::read(dir.path("boy.png")).unwrap(),
            request.image.as_ref().unwrap().bytes
        );
    }
}

#[test]
fn new_shared_reference_after_confirmation_retains_image() {
    let (_tmp, workflow) = fixture();
    let root = workflow.paths.managed_root();
    let request = workflow.prepare_deletion("boy").unwrap();
    assert!(request.image.is_some());
    fs::copy(
        root.join("profiles/boy.toml"),
        root.join("profiles/other.toml"),
    )
    .unwrap();
    let outcome = workflow
        .confirm_deletion(
            &request,
            AT,
            |_, _| Err("unexpected resolution"),
            || Ok::<_, &'static str>(()),
        )
        .unwrap();
    assert!(outcome.cleanup.contains("Images retained"));
    assert!(root.join("profiles/boy.png").exists());
}
