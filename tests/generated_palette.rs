use std::{
    fs,
    str::FromStr,
    sync::atomic::{AtomicU64, Ordering},
};

use ghostty_wall::{
    codec::intent::{parse_config_toml, parse_profile_toml},
    domain::IntentId,
    plan::plan_local_profile_json_uninspected,
};
use serde_json::Value;

#[test]
fn generated_palette_is_deterministic_complete_and_readable() {
    let fixture = fixture("palette.png", include_bytes!("fixtures/palette.png"));
    let first = generated_plan(&fixture).unwrap();
    let second = generated_plan(&fixture).unwrap();
    let colors = &first["environment"]["manifest"]["colors"];

    assert_eq!(colors, &second["environment"]["manifest"]["colors"]);
    assert_eq!(colors["background"], "1a212c");
    assert_eq!(colors["foreground"], "ffffff");
    assert_eq!(colors["palette"][1], "c12a2f");
    assert_eq!(colors["palette"][8], "666b72");
    assert_eq!(colors["palette"][14], "219ac7");
    assert_eq!(first["color_resolution"]["kind"], "generated");
    assert_eq!(first["color_resolution"]["algorithm"], "kmeans-v1");
    let palette = colors["palette"].as_array().unwrap();
    assert_eq!(palette.len(), 16);
    assert!(
        palette
            .iter()
            .all(|value| is_lower_hex_color(value.as_str().unwrap()))
    );
    for field in [
        "background",
        "foreground",
        "cursor",
        "selection_background",
        "selection_foreground",
    ] {
        assert!(is_lower_hex_color(colors[field].as_str().unwrap()));
    }
    assert!(contrast(colors, "background", "foreground") >= 4.5);
    assert!(contrast(colors, "background", "cursor") >= 4.5);
    assert!(contrast(colors, "selection_background", "selection_foreground") >= 4.5);
}

#[test]
fn generated_palette_keeps_monochrome_wallpaper_useful() {
    let fixture = fixture("white.png", include_bytes!("fixtures/white.png"));
    let plan = generated_plan(&fixture).unwrap();
    let colors = &plan["environment"]["manifest"]["colors"];
    let distinct = colors["palette"]
        .as_array()
        .unwrap()
        .iter()
        .collect::<std::collections::HashSet<_>>();

    assert_eq!(colors["background"], "ffffff");
    assert_eq!(colors["foreground"], "000000");
    assert!(distinct.len() >= 12);
}

#[test]
fn generated_colors_deduplicate_with_identical_explicit_colors() {
    let fixture = fixture("palette.png", include_bytes!("fixtures/palette.png"));
    let generated = generated_plan(&fixture).unwrap();
    let colors = &generated["environment"]["manifest"]["colors"];
    let palette = colors["palette"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| format!("\"{}\"", value.as_str().unwrap()))
        .collect::<Vec<_>>()
        .join(", ");
    let explicit = profile_plan(
        &fixture,
        &format!(
            r#"[colors]
mode = "explicit"
background = "{}"
foreground = "{}"
palette = [{}]
cursor = "{}"
selection_background = "{}"
selection_foreground = "{}"
"#,
            colors["background"].as_str().unwrap(),
            colors["foreground"].as_str().unwrap(),
            palette,
            colors["cursor"].as_str().unwrap(),
            colors["selection_background"].as_str().unwrap(),
            colors["selection_foreground"].as_str().unwrap(),
        ),
    )
    .unwrap();

    assert_eq!(
        generated["environment"]["environment_id"],
        explicit["environment"]["environment_id"]
    );
    assert_eq!(explicit["color_resolution"]["kind"], "explicit");
}

#[test]
fn generated_palette_rejects_malformed_and_oversized_images() {
    let malformed = fixture("broken.png", b"not a png");
    let error = generated_plan(&malformed).unwrap_err();
    assert_eq!(
        error.error_response()["error"]["code"],
        "asset.unsupported-image"
    );

    let oversized = temp_root("oversized").join("wide.png");
    fs::create_dir_all(oversized.parent().unwrap()).unwrap();
    image::RgbImage::new(16_385, 1).save(&oversized).unwrap();
    let error = generated_plan(&oversized).unwrap_err();
    assert_eq!(
        error.error_response()["error"]["code"],
        "asset.unsupported-image"
    );
}

fn generated_plan(path: &std::path::Path) -> Result<Value, ghostty_wall::plan::PlanError> {
    profile_plan(path, "[colors]\nmode = \"generated\"\n")
}

fn profile_plan(
    path: &std::path::Path,
    colors: &str,
) -> Result<Value, ghostty_wall::plan::PlanError> {
    let source = path.parent().unwrap();
    let config = parse_config_toml(&format!(
        "schema_version = 1\n[sources.local]\nkind = \"local-directory\"\npath = \"{}\"\n",
        source.display()
    ))
    .unwrap();
    let profile = parse_profile_toml(&format!(
        "schema_version = 1\n[wallpaper]\nmode = \"source\"\nsource = \"local\"\nselection = \"path\"\npath = \"{}\"\n{colors}",
        path.file_name().unwrap().to_str().unwrap()
    ))
    .unwrap();
    plan_local_profile_json_uninspected(
        source,
        source,
        &source.join("managed"),
        &IntentId::from_str("generated").unwrap(),
        &config,
        &profile,
        None,
    )
}

fn fixture(name: &str, bytes: &[u8]) -> std::path::PathBuf {
    let root = temp_root(name);
    fs::create_dir_all(&root).unwrap();
    let path = root.join(name);
    fs::write(&path, bytes).unwrap();
    path
}

fn is_lower_hex_color(value: &str) -> bool {
    value.len() == 6
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn contrast(colors: &Value, left: &str, right: &str) -> f64 {
    let left = luminance(colors[left].as_str().unwrap());
    let right = luminance(colors[right].as_str().unwrap());
    (left.max(right) + 0.05) / (left.min(right) + 0.05)
}

fn luminance(hex: &str) -> f64 {
    let channel = |offset| {
        let value = u8::from_str_radix(&hex[offset..offset + 2], 16).unwrap() as f64 / 255.0;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(0) + 0.7152 * channel(2) + 0.0722 * channel(4)
}

fn temp_root(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "ghostty-wall-generated-{name}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

static NEXT: AtomicU64 = AtomicU64::new(0);
