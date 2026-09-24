use std::{
    cell::RefCell,
    fs,
    path::PathBuf,
    str::FromStr,
    sync::atomic::{AtomicU64, Ordering},
};

use ghostty_wall::{
    apply::apply_github_profile,
    codec::intent::{parse_config_toml, parse_profile_toml},
    domain::{IntentId, ResolutionSeed},
    github::{GithubApi, GithubApiError, GithubEntryKind, GithubTree, GithubTreeEntry},
    plan::{GithubResolutionError, PlanError, plan_github_profile_json_uninspected},
};

const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";
static NEXT: AtomicU64 = AtomicU64::new(0);

#[derive(Default)]
struct RecordingGithub {
    calls: RefCell<Vec<String>>,
    failure: Option<GithubApiError>,
    truncated: bool,
}

impl RecordingGithub {
    fn calls(&self) -> Vec<String> {
        self.calls.borrow().clone()
    }
}

impl GithubApi for RecordingGithub {
    fn default_branch(&self, repository: &str) -> Result<String, GithubApiError> {
        self.calls
            .borrow_mut()
            .push(format!("default:{repository}"));
        self.failure.map_or_else(|| Ok("main".to_owned()), Err)
    }

    fn resolve_commit(&self, repository: &str, reference: &str) -> Result<String, GithubApiError> {
        self.calls
            .borrow_mut()
            .push(format!("resolve:{repository}:{reference}"));
        self.failure.map_or_else(|| Ok(COMMIT.to_owned()), Err)
    }

    fn tree(
        &self,
        repository: &str,
        commit: &str,
        recursive: bool,
    ) -> Result<GithubTree, GithubApiError> {
        self.calls
            .borrow_mut()
            .push(format!("tree:{repository}:{commit}:{recursive}"));
        if let Some(error) = self.failure {
            return Err(error);
        }
        Ok(GithubTree::new(
            vec![
                GithubTreeEntry::new(
                    "wallpapers/nested/night.JPG",
                    "100644",
                    GithubEntryKind::Blob,
                ),
                GithubTreeEntry::new(
                    "wallpapers/bin/executable.png",
                    "100755",
                    GithubEntryKind::Blob,
                ),
                GithubTreeEntry::new("wallpapers/link.jpg", "120000", GithubEntryKind::Blob),
                GithubTreeEntry::new("wallpapers/vendor", "160000", GithubEntryKind::Commit),
                GithubTreeEntry::new("wallpapers/directory", "040000", GithubEntryKind::Tree),
                GithubTreeEntry::new("outside.jpg", "100644", GithubEntryKind::Blob),
            ],
            self.truncated,
        ))
    }

    fn blob(&self, repository: &str, commit: &str, path: &str) -> Result<Vec<u8>, GithubApiError> {
        self.calls
            .borrow_mut()
            .push(format!("blob:{repository}:{commit}:{path}"));
        self.failure
            .map_or_else(|| Ok(include_bytes!("fixtures/white.jpg").to_vec()), Err)
    }
}

fn github_intent(
    reference: Option<&str>,
) -> (
    ghostty_wall::domain::ConfigIntent,
    ghostty_wall::domain::ProfileIntent,
) {
    let reference = reference
        .map(|value| format!("ref = \"{value}\"\n"))
        .unwrap_or_default();
    let config = parse_config_toml(&format!(
        "schema_version = 1\n[sources.remote]\nkind = \"github\"\nrepository = \"owner/repo\"\n{reference}path = \"wallpapers\"\n"
    ))
    .unwrap();
    let profile = parse_profile_toml(
        "schema_version = 1\n[wallpaper]\nmode = \"source\"\nsource = \"remote\"\nselection = \"random\"\n",
    )
    .unwrap();
    (config, profile)
}

#[test]
fn github_random_plan_pins_one_commit_and_excludes_non_regular_entries() {
    let github = RecordingGithub::default();
    let (config, profile) = github_intent(Some("main"));
    let seed = ResolutionSeed::from_bytes([7; 32]);
    let root = temp_root("plan");

    let plan = plan_github_profile_json_uninspected(
        &root,
        &root,
        &root.join("managed"),
        &IntentId::from_str("night").unwrap(),
        &config,
        &profile,
        Some(&seed),
        &github,
    )
    .unwrap();

    assert_eq!(
        plan["source"],
        serde_json::json!({
            "id": "remote",
            "kind": "github",
            "repository": "owner/repo",
            "ref": { "kind": "configured", "value": "main" },
            "resolved_commit": COMMIT,
            "path": "wallpapers"
        })
    );
    assert_eq!(plan["selection"]["candidate_count"], 2);
    assert!(matches!(
        plan["selection"]["candidate"].as_str(),
        Some("bin/executable.png" | "nested/night.JPG")
    ));
    assert_eq!(plan["asset"]["media_type"], "image/jpeg");
    let candidate = plan["selection"]["candidate"].as_str().unwrap();
    assert_eq!(
        github.calls(),
        vec![
            "resolve:owner/repo:main".to_owned(),
            format!("tree:owner/repo:{COMMIT}:true"),
            format!("blob:owner/repo:{COMMIT}:wallpapers/{candidate}"),
        ]
    );
}

#[test]
fn absent_ref_resolves_default_branch_once_then_uses_only_commit() {
    let github = RecordingGithub::default();
    let (config, profile) = github_intent(None);
    let root = temp_root("default");

    let plan = plan_github_profile_json_uninspected(
        &root,
        &root,
        &root.join("managed"),
        &IntentId::from_str("night").unwrap(),
        &config,
        &profile,
        Some(&ResolutionSeed::from_bytes([9; 32])),
        &github,
    )
    .unwrap();

    assert_eq!(
        plan["source"]["ref"],
        serde_json::json!({ "kind": "default-branch", "value": "main" })
    );
    let candidate = plan["selection"]["candidate"].as_str().unwrap();
    assert_eq!(
        github.calls(),
        vec![
            "default:owner/repo".to_owned(),
            "resolve:owner/repo:main".to_owned(),
            format!("tree:owner/repo:{COMMIT}:true"),
            format!("blob:owner/repo:{COMMIT}:wallpapers/{candidate}"),
        ]
    );
}

#[test]
fn github_direct_path_skips_enumeration_but_still_uses_resolved_commit() {
    let github = RecordingGithub::default();
    let (config, _) = github_intent(Some("release"));
    let profile = parse_profile_toml(
        "schema_version = 1\n[wallpaper]\nmode = \"source\"\nsource = \"remote\"\nselection = \"path\"\npath = \"fixed/wall.jpg\"\n",
    )
    .unwrap();
    let root = temp_root("direct");

    let plan = plan_github_profile_json_uninspected(
        &root,
        &root,
        &root,
        &IntentId::from_str("fixed").unwrap(),
        &config,
        &profile,
        None,
        &github,
    )
    .unwrap();

    assert_eq!(
        plan["selection"],
        serde_json::json!({ "kind": "path", "candidate": "fixed/wall.jpg" })
    );
    assert_eq!(
        github.calls(),
        vec![
            "resolve:owner/repo:release".to_owned(),
            format!("blob:owner/repo:{COMMIT}:wallpapers/fixed/wall.jpg"),
        ]
    );
}

#[test]
fn truncated_tree_and_github_access_failures_are_safe_structured_errors() {
    let (config, profile) = github_intent(Some("secret-ref"));
    let root = temp_root("errors");
    let id = IntentId::from_str("night").unwrap();
    let seed = ResolutionSeed::from_bytes([1; 32]);

    let truncated = RecordingGithub {
        truncated: true,
        ..RecordingGithub::default()
    };
    let error = plan_github_profile_json_uninspected(
        &root,
        &root,
        &root,
        &id,
        &config,
        &profile,
        Some(&seed),
        &truncated,
    )
    .unwrap_err();
    assert!(matches!(
        error,
        PlanError::Github {
            kind: GithubResolutionError::IncompleteTree,
            ..
        }
    ));
    assert_eq!(
        error.error_response(),
        serde_json::json!({
            "schema_version": 1,
            "error": {
                "category": "resolution",
                "code": "source.github-incomplete-tree",
                "source_id": "remote"
            }
        })
    );

    for (failure, code) in [
        (
            GithubApiError::Authentication,
            "source.github-authentication-failed",
        ),
        (GithubApiError::RateLimited, "source.github-rate-limited"),
    ] {
        let github = RecordingGithub {
            failure: Some(failure),
            ..RecordingGithub::default()
        };
        let error = plan_github_profile_json_uninspected(
            &root,
            &root,
            &root,
            &id,
            &config,
            &profile,
            Some(&seed),
            &github,
        )
        .unwrap_err();
        let response = error.error_response();
        assert_eq!(response["error"]["code"], code);
        assert_eq!(response["error"]["source_id"], "remote");
        assert!(!response.to_string().contains("secret-ref"));
        assert_eq!(error.exit_status(), 4);
    }
}

#[test]
fn apply_persists_github_asset_and_commit_provenance() {
    let github = RecordingGithub::default();
    let (config, profile) = github_intent(Some("main"));
    let root = temp_root("apply");
    let managed = root.join("managed");
    fs::create_dir_all(managed.join("history/activations")).unwrap();
    fs::create_dir(managed.join("environments")).unwrap();
    fs::create_dir_all(managed.join("assets/sha256")).unwrap();
    fs::write(managed.join("state.lock"), []).unwrap();
    let root_config = root.join("config.ghostty");
    fs::write(
        &root_config,
        format!(
            "config-file = ?{}\n",
            managed.join("current.ghostty").display()
        ),
    )
    .unwrap();

    apply_github_profile(
        &root,
        &root,
        &managed,
        &root_config,
        &IntentId::from_str("night").unwrap(),
        &config,
        &profile,
        Some(&ResolutionSeed::from_bytes([4; 32])),
        "2026-09-23T08:31:15.123456Z",
        &github,
        || Ok::<(), ()>(()),
    )
    .unwrap();

    let activation: serde_json::Value = serde_json::from_slice(
        &fs::read(managed.join("history/activations/act-v1-0000000000000001.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(activation["source"]["resolved_commit"], COMMIT);
    assert_eq!(activation["source"]["ref"]["value"], "main");
    assert_eq!(
        fs::read_dir(managed.join("assets/sha256"))
            .unwrap()
            .flat_map(|entry| fs::read_dir(entry.unwrap().path()).unwrap())
            .count(),
        1
    );
    assert_eq!(
        github
            .calls()
            .iter()
            .filter(|call| call.starts_with("resolve:"))
            .count(),
        1
    );
    assert!(github.calls().iter().all(|call| {
        !call.starts_with("tree:") && !call.starts_with("blob:") || call.contains(COMMIT)
    }));

    fs::remove_dir_all(root).unwrap();
}

fn temp_root(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "ghostty-wall-github-{name}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}
