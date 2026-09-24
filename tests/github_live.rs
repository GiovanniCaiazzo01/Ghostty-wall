use std::{path::Path, str::FromStr};

use ghostty_wall::{
    codec::intent::{parse_config_toml, parse_profile_toml},
    domain::{IntentId, ResolutionSeed},
    github::GithubHttpClient,
    plan::plan_github_profile_json_uninspected,
};

#[test]
#[ignore = "requires live GitHub access"]
fn resolves_live_github_source_at_one_commit() {
    let config = parse_config_toml(
        "schema_version = 1\n[sources.anime]\nkind = \"github\"\nrepository = \"ThePrimeagen/anime\"\nref = \"master\"\n",
    )
    .unwrap();
    let profile = parse_profile_toml(
        "schema_version = 1\n[wallpaper]\nmode = \"source\"\nsource = \"anime\"\nselection = \"random\"\n",
    )
    .unwrap();
    let plan = plan_github_profile_json_uninspected(
        Path::new("."),
        Path::new("."),
        Path::new("."),
        &IntentId::from_str("live").unwrap(),
        &config,
        &profile,
        Some(&ResolutionSeed::from_bytes([0; 32])),
        &GithubHttpClient::from_env(),
    )
    .unwrap();

    let commit = plan["source"]["resolved_commit"].as_str().unwrap();
    assert_eq!(commit.len(), 40);
    assert!(commit.bytes().all(|byte| byte.is_ascii_hexdigit()));
    assert!(plan["selection"]["candidate_count"].as_u64().unwrap() > 0);
    assert!(matches!(
        plan["asset"]["media_type"].as_str(),
        Some("image/png" | "image/jpeg")
    ));
}
