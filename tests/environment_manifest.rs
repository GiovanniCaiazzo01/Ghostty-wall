use ghostty_wall::{
    codec::manifest::{decode, encode_canonical, environment_id},
    domain::{FontSizeMillipoints, OpacityMillionths},
};

const RFC_0001_MANIFEST: &str = r##"
{
  "schema_version": 1,
  "wallpaper": {
    "mode": "image",
    "asset_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    "media_type": "image/jpeg",
    "fit": "cover",
    "position": "center",
    "opacity_millionths": 110000,
    "repeat": false
  },
  "colors": {
    "background": "11151c",
    "foreground": "e7e2dc",
    "cursor": "e7e2dc",
    "selection_background": "303846",
    "palette": [
      "11151c", "d35f72", "86b77d", "d3ae6f",
      "6f8faf", "a17cb8", "70b7b1", "c5c8c6",
      "4b5263", "e06c75", "98c379", "e5c07b",
      "61afef", "c678dd", "56b6c2", "e7e2dc"
    ]
  },
  "terminal": {
    "font_size_millipoints": 13500,
    "background_opacity_millionths": 920000,
    "background_blur_intensity": 20
  }
}
"##;

const RFC_0001_CANONICAL: &str = r#"{"colors":{"background":"11151c","cursor":"e7e2dc","foreground":"e7e2dc","palette":["11151c","d35f72","86b77d","d3ae6f","6f8faf","a17cb8","70b7b1","c5c8c6","4b5263","e06c75","98c379","e5c07b","61afef","c678dd","56b6c2","e7e2dc"],"selection_background":"303846"},"schema_version":1,"terminal":{"background_blur_intensity":20,"background_opacity_millionths":920000,"font_size_millipoints":13500},"wallpaper":{"asset_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","fit":"cover","media_type":"image/jpeg","mode":"image","opacity_millionths":110000,"position":"center","repeat":false}}"#;

const RFC_0001_ENVIRONMENT_ID: &str =
    "env-v1-73d59a8d7c8c3470a27402ab2541c421eedb8377f5a414685b34d326f3c32163";

#[test]
fn rfc_0001_manifest_has_stable_canonical_json_and_identity() {
    let manifest = decode(RFC_0001_MANIFEST.as_bytes()).expect("RFC vector must decode");

    assert_eq!(
        std::str::from_utf8(&encode_canonical(&manifest).expect("manifest must encode"))
            .expect("JCS is UTF-8"),
        RFC_0001_CANONICAL
    );
    assert_eq!(
        environment_id(&manifest)
            .expect("manifest must hash")
            .to_string(),
        RFC_0001_ENVIRONMENT_ID
    );
}

#[test]
fn wallpaper_absence_and_managed_none_remain_distinct() {
    let unmanaged = decode(br#"{"schema_version":1}"#).expect("empty managed state is valid");
    let managed_none = decode(br#"{"schema_version":1,"wallpaper":{"mode":"none"}}"#)
        .expect("managed wallpaper reset is valid");
    let managed_image = decode(
        br#"{"schema_version":1,"wallpaper":{"mode":"image","asset_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","media_type":"image/jpeg"}}"#,
    )
    .expect("managed wallpaper image is valid");

    assert_eq!(
        encode_canonical(&unmanaged).expect("unmanaged must encode"),
        br#"{"schema_version":1}"#
    );
    assert_eq!(
        encode_canonical(&managed_none).expect("managed none must encode"),
        br#"{"schema_version":1,"wallpaper":{"mode":"none"}}"#
    );
    assert_ne!(
        environment_id(&unmanaged).expect("unmanaged must hash"),
        environment_id(&managed_none).expect("managed none must hash")
    );
    assert_ne!(
        environment_id(&managed_none).expect("managed none must hash"),
        environment_id(&managed_image).expect("managed image must hash")
    );
    assert_ne!(
        environment_id(&unmanaged).expect("unmanaged must hash"),
        environment_id(&managed_image).expect("managed image must hash")
    );
}

#[test]
fn manifest_rejects_non_contractual_json() {
    for invalid in [
        r#"{"schema_version":1,"wallpaper":null}"#,
        r#"{"schema_version":1,"unknown":true}"#,
        r#"{"schema_version":1,"terminal":{}}"#,
        r#"{"schema_version":1,"terminal":{"font_size_millipoints":999}}"#,
        r#"{"schema_version":1,"terminal":{"background_opacity_millionths":1000001}}"#,
        r#"{"schema_version":1,"terminal":{"background_blur_intensity":256}}"#,
        r#"{"schema_version":1,"colors":{"background":"1a1b26","foreground":"c0caf5","palette":["000000"]}}"#,
        r#"{"schema_version":1,"colors":{"background":"1A1B26","foreground":"c0caf5","palette":["000000","000000","000000","000000","000000","000000","000000","000000","000000","000000","000000","000000","000000","000000","000000","000000"]}}"#,
    ] {
        assert!(decode(invalid.as_bytes()).is_err(), "{invalid} must fail");
    }
}

#[test]
fn fixed_point_types_enforce_rfc_ranges() {
    assert_eq!(
        FontSizeMillipoints::new(13_500)
            .expect("13.5 pt is valid")
            .get(),
        13_500
    );
    assert!(FontSizeMillipoints::new(999).is_err());
    assert!(FontSizeMillipoints::new(1_000_001).is_err());

    assert_eq!(
        OpacityMillionths::new(920_000)
            .expect("0.92 is valid")
            .get(),
        920_000
    );
    assert!(OpacityMillionths::new(1_000_001).is_err());
}
