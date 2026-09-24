use ghostty_wall::{
    codec::manifest,
    domain::{
        Color, ColorsManifest, EnvironmentManifest, ImageWallpaper, MediaType, Sha256Digest,
        WallpaperManifest,
    },
    history::{HistoryError, inspect_history},
};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Root(PathBuf);
impl Root {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "ghostty-wall-history-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        fs::create_dir_all(path.join("history/activations")).unwrap();
        fs::create_dir(path.join("environments")).unwrap();
        fs::create_dir_all(path.join("assets/sha256")).unwrap();
        fs::write(path.join("state.lock"), []).unwrap();
        Self(path)
    }
    fn environment(&self) -> String {
        let manifest = EnvironmentManifest::new(None, None, None);
        let id = manifest::environment_id(&manifest).unwrap().to_string();
        let body = serde_json::json!({"record_schema_version":1,"environment_id":id,"manifest":{"schema_version":1}});
        fs::write(
            self.0.join(format!("environments/{id}.json")),
            body.to_string(),
        )
        .unwrap();
        id
    }
    fn record(&self, seq: u64, environment_id: &str) -> serde_json::Value {
        serde_json::json!({"record_schema_version":1,"activation_id":format!("act-v1-{seq:016x}"),"sequence":seq,"history_cursor":seq,"activated_at":"2026-09-23T08:31:15.123456Z","environment_id":environment_id,"cause":{"kind":"profile"},"profile":{"id":"night","schema_version":1}})
    }
    fn put(&self, seq: u64, value: &serde_json::Value) {
        fs::write(
            self.0
                .join(format!("history/activations/act-v1-{seq:016x}.json")),
            value.to_string(),
        )
        .unwrap();
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn empty_contiguous_profile_history_and_corrupt_records() {
    let root = Root::new();
    assert!(inspect_history(&root.0).unwrap().latest().is_none());
    let id = root.environment();
    let first = root.record(1, &id);
    root.put(1, &first);
    assert_eq!(
        inspect_history(&root.0)
            .unwrap()
            .latest()
            .unwrap()
            .sequence(),
        1
    );
    let second = root.record(2, &id);
    root.put(2, &second);
    assert_eq!(
        inspect_history(&root.0)
            .unwrap()
            .latest()
            .unwrap()
            .sequence(),
        2
    );
    fs::remove_file(
        root.0
            .join("history/activations/act-v1-0000000000000001.json"),
    )
    .unwrap();
    assert!(inspect_history(&root.0).is_err(), "gap");
    root.put(1, &first);
    fs::write(
        root.0
            .join("history/activations/act-v1-0000000000000002.JSON"),
        second.to_string(),
    )
    .unwrap();
    assert!(
        inspect_history(&root.0).is_err(),
        "duplicate or unrecognized committed record"
    );
    fs::remove_file(
        root.0
            .join("history/activations/act-v1-0000000000000002.JSON"),
    )
    .unwrap();
    for key in ["sequence", "cause", "activated_at"] {
        let mut invalid = second.clone();
        invalid[key] = match key {
            "sequence" => serde_json::json!(7),
            "cause" => serde_json::json!({"kind":"unknown"}),
            _ => serde_json::json!("2026-09-23T08:31:15Z"),
        };
        root.put(2, &invalid);
        assert!(inspect_history(&root.0).is_err(), "invalid {key}");
    }
}

#[test]
fn replay_uses_target_cursor_and_environment_not_latest_cursor() {
    let root = Root::new();
    let id = root.environment();
    root.put(1, &root.record(1, &id));
    root.put(2, &root.record(2, &id));
    let mut replay = root.record(3, &id);
    replay.as_object_mut().unwrap().remove("profile");
    replay["cause"] =
        serde_json::json!({"kind":"history-replay","activation_id":"act-v1-0000000000000001"});
    replay["history_cursor"] = serde_json::json!(1);
    root.put(3, &replay);
    let history = inspect_history(&root.0).unwrap();
    assert_eq!(history.latest().unwrap().history_cursor(), 1);
    assert_eq!(history.at(2).unwrap().history_cursor(), 2);
    for (key, value) in [
        ("history_cursor", serde_json::json!(2)),
        (
            "cause",
            serde_json::json!({"kind":"history-replay","activation_id":"act-v1-0000000000000003"}),
        ),
        (
            "profile",
            serde_json::json!({"id":"night","schema_version":1}),
        ),
    ] {
        let mut broken = replay.clone();
        broken[key] = value;
        root.put(3, &broken);
        assert!(inspect_history(&root.0).is_err(), "replay invalid {key}");
    }
}

#[test]
fn previous_target_uses_cursor_not_penultimate_event() {
    let root = Root::new();
    let id = root.environment();
    assert!(
        inspect_history(&root.0)
            .unwrap()
            .previous_target()
            .is_none()
    );
    root.put(1, &root.record(1, &id));
    assert!(
        inspect_history(&root.0)
            .unwrap()
            .previous_target()
            .is_none()
    );
    root.put(2, &root.record(2, &id));
    let mut replay = root.record(3, &id);
    replay.as_object_mut().unwrap().remove("profile");
    replay["cause"] =
        serde_json::json!({"kind":"history-replay","activation_id":"act-v1-0000000000000002"});
    replay["history_cursor"] = serde_json::json!(2);
    root.put(3, &replay);
    let history = inspect_history(&root.0).unwrap();
    assert_eq!(history.latest().unwrap().sequence(), 3);
    assert_eq!(history.previous_target().unwrap().sequence(), 1);
}

#[test]
fn strict_record_fields_and_calendar_dates() {
    let root = Root::new();
    let id = root.environment();
    let valid = root.record(1, &id);
    for timestamp in [
        "2026-02-29T08:31:15.123456Z",
        "2026-13-01T08:31:15.123456Z",
        "2026-09-23T24:31:15.123456Z",
        "2026-09-23T08:31:60.123456Z",
        "2026-09-23T08:31:15.123456+00:00",
    ] {
        let mut record = valid.clone();
        record["activated_at"] = timestamp.into();
        root.put(1, &record);
        assert!(
            inspect_history(&root.0).is_err(),
            "invalid timestamp {timestamp}"
        );
    }
    for (key, value) in [
        ("profile", serde_json::Value::Null),
        ("extra", serde_json::json!(1)),
        ("cause", serde_json::json!({"kind":"profile","extra":1})),
    ] {
        let mut record = valid.clone();
        record[key] = value;
        root.put(1, &record);
        assert!(inspect_history(&root.0).is_err(), "invalid {key}");
    }
    let file = root
        .0
        .join("history/activations/act-v1-0000000000000001.json");
    fs::write(
        &file,
        valid
            .to_string()
            .replace("\"sequence\":1", "\"sequence\":1,\"sequence\":1"),
    )
    .unwrap();
    assert!(inspect_history(&root.0).is_err(), "duplicate JSON key");
    root.put(1, &valid);
    assert!(inspect_history(&root.0).is_ok());
    let mut leap_second = valid.clone();
    leap_second["activated_at"] = "2016-12-31T23:59:60.000000Z".into();
    root.put(1, &leap_second);
    assert!(inspect_history(&root.0).is_ok(), "RFC 3339 leap second");
}

#[test]
fn missing_or_invalid_referenced_environment_is_corrupt() {
    let root = Root::new();
    let id = root.environment();
    root.put(1, &root.record(1, &id));
    let path = root.0.join(format!("environments/{id}.json"));
    fs::write(&path, format!("{{\"record_schema_version\":1,\"environment_id\":\"{id}\",\"manifest\":{{\"schema_version\":1,\"unknown\":true}}}}")).unwrap();
    assert!(inspect_history(&root.0).is_err());
    fs::remove_file(path.clone()).unwrap();
    assert!(matches!(inspect_history(&root.0), Err(HistoryError::Corrupt(p)) if p == path));
}

#[test]
fn image_dependency_digest_media_and_profile_provenance() {
    let root = Root::new();
    let bytes = include_bytes!("fixtures/white.png");
    let digest = Sha256Digest::from_bytes(Sha256::digest(bytes).into());
    let manifest = EnvironmentManifest::new(
        Some(WallpaperManifest::Image(ImageWallpaper::new(
            digest,
            MediaType::Png,
        ))),
        None,
        None,
    );
    let id = manifest::environment_id(&manifest).unwrap().to_string();
    let envelope = serde_json::json!({"record_schema_version":1,"environment_id":id,"manifest":serde_json::from_slice::<serde_json::Value>(&manifest::encode_canonical(&manifest).unwrap()).unwrap()});
    fs::write(
        root.0.join(format!("environments/{id}.json")),
        envelope.to_string(),
    )
    .unwrap();
    let shard = root
        .0
        .join(format!("assets/sha256/{}", &digest.to_string()[..2]));
    fs::create_dir(&shard).unwrap();
    let asset = shard.join(format!("{digest}.png"));
    fs::write(&asset, bytes).unwrap();
    let mut record = root.record(1, &id);
    record["source"] = serde_json::json!({"kind":"local-directory","id":"local","configured_path":"~/wallpapers","resolved_root":"/home/user/wallpapers"});
    record["selection"] = serde_json::json!({"kind":"path","candidate":"white.png"});
    record["asset"] = serde_json::json!({"sha256":digest.to_string(),"media_type":"image/png","byte_length":bytes.len()});
    root.put(1, &record);
    assert!(inspect_history(&root.0).is_ok());
    for (key, value) in [
        (
            "asset",
            serde_json::json!({"sha256":digest.to_string(),"media_type":"image/png","byte_length":42}),
        ),
        (
            "selection",
            serde_json::json!({"kind":"random","algorithm":"wrong","seed":"0".repeat(64),"candidate_set_fingerprint":"x","candidate_count":1,"selected_index":0,"candidate":"white.png"}),
        ),
        ("source", serde_json::Value::Null),
        (
            "source",
            serde_json::json!({"kind":"local-directory","id":"local","configured_path":"~bad","resolved_root":"/home/user/wallpapers"}),
        ),
        (
            "source",
            serde_json::json!({"kind":"local-directory","id":"local","configured_path":"~/wallpapers","resolved_root":"/home/../private"}),
        ),
    ] {
        let mut broken = record.clone();
        broken[key] = value;
        root.put(1, &broken);
        assert!(inspect_history(&root.0).is_err(), "invalid {key}");
    }
    root.put(1, &record);
    fs::write(&asset, b"not png").unwrap();
    assert!(inspect_history(&root.0).is_err(), "tampered durable asset");
    fs::write(&asset, bytes).unwrap();
    fs::write(shard.join(format!("{digest}.jpg")), bytes).unwrap();
    assert!(inspect_history(&root.0).is_err(), "duplicate asset media");
}

#[test]
fn referenced_asset_must_decode_not_only_match_digest_and_magic() {
    let root = Root::new();
    let bytes = b"\x89PNG\r\n\x1a\n";
    assert_eq!(image::guess_format(bytes).unwrap(), image::ImageFormat::Png);
    let digest = Sha256Digest::from_bytes(Sha256::digest(bytes).into());
    let manifest = EnvironmentManifest::new(
        Some(WallpaperManifest::Image(ImageWallpaper::new(
            digest,
            MediaType::Png,
        ))),
        None,
        None,
    );
    let id = manifest::environment_id(&manifest).unwrap().to_string();
    let envelope = serde_json::json!({"record_schema_version":1,"environment_id":id,"manifest":serde_json::from_slice::<serde_json::Value>(&manifest::encode_canonical(&manifest).unwrap()).unwrap()});
    fs::write(
        root.0.join(format!("environments/{id}.json")),
        envelope.to_string(),
    )
    .unwrap();
    let shard = root
        .0
        .join(format!("assets/sha256/{}", &digest.to_string()[..2]));
    fs::create_dir(&shard).unwrap();
    fs::write(shard.join(format!("{digest}.png")), bytes).unwrap();
    let mut record = root.record(1, &id);
    record["source"] = serde_json::json!({"kind":"local-directory","id":"local","configured_path":"~/wallpapers","resolved_root":"/home/user/wallpapers"});
    record["selection"] = serde_json::json!({"kind":"path","candidate":"broken.png"});
    record["asset"] = serde_json::json!({"sha256":digest.to_string(),"media_type":"image/png","byte_length":bytes.len()});
    root.put(1, &record);
    assert!(matches!(
        inspect_history(&root.0),
        Err(HistoryError::Corrupt(_))
    ));
}

#[cfg(unix)]
#[test]
fn lock_and_record_symlinks_are_not_followed() {
    use std::os::unix::fs::symlink;
    let root = Root::new();
    let target = root.0.join("external");
    fs::write(&target, b"untouched").unwrap();
    let lock = root.0.join("state.lock");
    fs::remove_file(&lock).unwrap();
    symlink(&target, &lock).unwrap();
    assert!(inspect_history(&root.0).is_err());
    assert_eq!(fs::read(&target).unwrap(), b"untouched");
    fs::remove_file(lock).unwrap();
    fs::write(root.0.join("state.lock"), []).unwrap();
    let record = root
        .0
        .join("history/activations/act-v1-0000000000000001.json");
    symlink(&target, record).unwrap();
    assert!(matches!(
        inspect_history(&root.0),
        Err(HistoryError::Corrupt(_))
    ));
    assert_eq!(fs::read(target).unwrap(), b"untouched");
}

#[test]
fn theme_provenance_must_hash_resolved_colors() {
    let root = Root::new();
    let color: Color = "aabbcc".parse().unwrap();
    let colors = ColorsManifest::new(color, color, [color; 16]);
    let manifest = EnvironmentManifest::new(None, Some(colors), None);
    let id = manifest::environment_id(&manifest).unwrap().to_string();
    let manifest_value: serde_json::Value =
        serde_json::from_slice(&manifest::encode_canonical(&manifest).unwrap()).unwrap();
    let canonical = serde_jcs::to_vec(&manifest_value["colors"]).unwrap();
    let mut hash = Sha256::new();
    hash.update(b"ghostty-wall.theme-resolution.v1\0");
    hash.update(&canonical);
    let digest = Sha256Digest::from_bytes(hash.finalize().into());
    fs::write(root.0.join(format!("environments/{id}.json")), serde_json::json!({"record_schema_version":1,"environment_id":id,"manifest":manifest_value}).to_string()).unwrap();
    let mut record = root.record(1, &id);
    record["color_resolution"] = serde_json::json!({"kind":"theme","theme":"TokyoNight","content_sha256":digest.to_string()});
    root.put(1, &record);
    assert!(inspect_history(&root.0).is_ok());
    record["color_resolution"]["content_sha256"] = "0".repeat(64).into();
    root.put(1, &record);
    assert!(
        inspect_history(&root.0).is_err(),
        "theme must hash actual colors"
    );
}
