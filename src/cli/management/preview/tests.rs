use super::*;
use crate::domain::EnvironmentManifest;

fn entry(profile: &str) -> (Request, Prepared) {
    (
        Request {
            generation: 1,
            profile: IntentId::from_str(profile).unwrap(),
            seed: ResolutionSeed::from_str(&"01".repeat(32)).unwrap(),
            area: ratatui::layout::Rect::new(0, 0, 40, 20),
        },
        Prepared {
            sample: Sample::new(EnvironmentManifest::new(None, None, None), None),
            plan: serde_json::json!({}),
            asset_bytes: Some(Arc::from(&b"captured bytes"[..])),
            files: Vec::new(),
        },
    )
}

#[test]
fn revisit_reuses_captured_buffers_across_generations_but_not_seeds_or_changed_files() {
    let mut cache = PreparedCache::default();
    let (mut request, mut prepared) = entry("sample");
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("image");
    fs::write(&file, b"original").unwrap();
    prepared.files.push(FileVersion::capture(file.clone()));
    let captured = prepared.asset_bytes.clone().unwrap();
    cache.insert(request.clone(), prepared.clone());
    request.generation += 1;
    let (_, reused) = cache
        .take(&request)
        .expect("revisit must not repeat preparation");
    assert!(Arc::ptr_eq(&captured, reused.asset_bytes.as_ref().unwrap()));
    cache.insert(request.clone(), reused);
    let mut other = request.clone();
    other.seed = ResolutionSeed::from_str(&"02".repeat(32)).unwrap();
    assert!(cache.take(&other).is_none());
    fs::write(file, b"changed image").unwrap();
    assert!(cache.take(&request).is_none());
    assert!(cache.0.is_empty());
}

#[test]
fn retention_is_bounded_and_unwatched_theme_and_remote_content_are_not_cached() {
    let mut cache = PreparedCache::default();
    for profile in ["first", "second", "third"] {
        let (request, prepared) = entry(profile);
        cache.insert(request, prepared);
    }
    assert_eq!(cache.0.len(), 2);
    assert!(cache.take(&entry("first").0).is_none());
    for plan in [
        serde_json::json!({"color_resolution": {"kind": "theme"}}),
        serde_json::json!({"source": {"kind": "github"}}),
    ] {
        let (request, mut prepared) = entry("unwatched");
        prepared.plan = plan;
        cache.insert(request.clone(), prepared);
        assert!(cache.take(&request).is_none());
    }
    let (request, mut prepared) = entry("large");
    prepared.asset_bytes = Some(Arc::from(vec![0; 32 * 1024 * 1024 + 1]));
    cache.insert(request.clone(), prepared);
    assert!(cache.take(&request).is_none());
}
