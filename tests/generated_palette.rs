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
    assert_eq!(colors["background"], "090b0f");
    assert!(luminance(colors["background"].as_str().unwrap()) < luminance("1a212c"));
    assert_eq!(colors["foreground"], "eecd6c");
    assert_eq!(colors["palette"][1], "cc4f53");
    assert_eq!(colors["palette"][8], "807962");
    assert_eq!(colors["palette"][14], "219ac7");
    assert_eq!(first["color_resolution"]["kind"], "generated");
    assert_eq!(first["color_resolution"]["algorithm"], "kmeans-v4");
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
fn colorful_wallpaper_tints_all_text_colors_without_losing_contrast() {
    let fixture = fixture("palette.png", include_bytes!("fixtures/palette.png"));
    let plan = generated_plan(&fixture).unwrap();
    let colors = &plan["environment"]["manifest"]["colors"];
    let background = colors["background"].as_str().unwrap();
    let foreground = colors["foreground"].as_str().unwrap();
    assert_eq!(plan["color_resolution"]["algorithm"], "kmeans-v4");
    assert_ne!(foreground, "ffffff");
    assert_ne!(foreground, "000000");
    for color in colors["palette"].as_array().unwrap() {
        let text = color.as_str().unwrap();
        let left = luminance(background);
        let right = luminance(text);
        assert!(
            (left.max(right) + 0.05) / (left.min(right) + 0.05) >= 4.5,
            "ANSI #{text} on #{background}"
        );
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

    assert_eq!(colors["background"], "555555");
    assert_eq!(colors["foreground"], "ffffff");
    assert!(distinct.len() >= 12);
    for entry in colors["palette"].as_array().unwrap() {
        let text_luminance = luminance(entry.as_str().unwrap());
        let background = luminance(colors["background"].as_str().unwrap());
        assert!(
            (background.max(text_luminance) + 0.05) / (background.min(text_luminance) + 0.05)
                >= 4.5
        );
    }
}

#[test]
fn white_wallpaper_text_is_readable_at_default_opacity() {
    let fixture = fixture("white.png", include_bytes!("fixtures/white.png"));
    let plan = profile_plan(&fixture, "opacity = 0.05\n[colors]\nmode = \"generated\"\n").unwrap();
    let colors = &plan["environment"]["manifest"]["colors"];
    let background = luminance(colors["background"].as_str().unwrap());
    let composed = background * 0.95 + 0.05;
    for entry in colors["palette"]
        .as_array()
        .unwrap()
        .iter()
        .chain([&colors["foreground"], &colors["cursor"]])
    {
        let text = luminance(entry.as_str().unwrap());
        let ratio = (text.max(composed) + 0.05) / (text.min(composed) + 0.05);
        assert!(
            ratio >= 4.5,
            "{entry} on composed white wallpaper: {ratio:.3}:1"
        );
    }
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
            r#"opacity = 0.0
[colors]
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

#[test]
fn versioned_fallback_reports_infeasibility_without_certifying_distinct_roles() {
    for (name, pixels, feasible) in [
        ("black", vec![[0; 3]], true),
        ("white", vec![[255; 3]], false),
        ("gray", (0..=255).map(|v| [v; 3]).collect(), false),
        (
            "saturated",
            vec![[255, 0, 0], [0, 255, 0], [0, 0, 255]],
            false,
        ),
        ("mixed", vec![[0; 3], [255; 3]], false),
    ] {
        let image = image::RgbImage::from_fn(pixels.len() as u32, 1, |x, _| {
            image::Rgb(pixels[x as usize])
        });
        let mut bytes = std::io::Cursor::new(Vec::new());
        image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
        let path = fixture(&format!("{name}.png"), bytes.get_ref());
        let recipe = |algorithm: &str| {
            format!("opacity = 0.65\n[colors]\nmode = \"generated\"\nalgorithm = \"{algorithm}\"\n")
        };
        let old = profile_plan(&path, &recipe("oklch-v1")).unwrap();
        let new = profile_plan(&path, &recipe("oklch-v2")).unwrap();
        assert_eq!(new, profile_plan(&path, &recipe("oklch-v2")).unwrap());
        let colors = |p: &Value| p["environment"]["manifest"]["colors"].clone();
        let distinct = |p: &Value| {
            colors(p)["palette"]
                .as_array()
                .unwrap()
                .iter()
                .cloned()
                .collect::<std::collections::HashSet<_>>()
                .len()
        };
        for p in [&old, &new] {
            assert_eq!(
                p["palette_contrast"]["status"],
                if feasible { "feasible" } else { "infeasible" }
            );
            assert_eq!(p["palette_contrast"]["manual_overrides_checked"], false);
            assert_eq!(
                p["diagnostics"].as_array().unwrap().len(),
                usize::from(!feasible)
            );
        }
        assert_eq!(distinct(&new), 16, "{name}");
        if feasible {
            assert_eq!(colors(&old), colors(&new));
        } else {
            assert_eq!(distinct(&old), 1, "historical v1 white fallback unchanged");
            assert_eq!(colors(&old)["foreground"], "ffffff");
            assert_eq!(new["diagnostics"][0]["fallback"], "original-roles");
            assert!(
                ghostty_wall::terminal_browser::format_plan_preview(&new)
                    .contains("NOT a valid 4.5:1 palette")
            );
        }
        let custom = profile_plan(
            &path,
            &format!(
                "{}[colors.overrides]\nforeground = \"123456\"\n",
                recipe("oklch-v2")
            ),
        )
        .unwrap();
        assert_eq!(colors(&custom)["foreground"], "123456");
        assert_eq!(custom["palette_contrast"], new["palette_contrast"]);
        assert_eq!(fs::read(&path).unwrap(), *bytes.get_ref());
    }
}

fn generated_plan(path: &std::path::Path) -> Result<Value, ghostty_wall::plan::PlanError> {
    profile_plan(path, "opacity = 0.0\n[colors]\nmode = \"generated\"\n")
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
    let schema = if colors.contains("oklch-") { 2 } else { 1 };
    let profile = parse_profile_toml(&format!(
        "schema_version = {schema}\n[wallpaper]\nmode = \"source\"\nsource = \"local\"\nselection = \"path\"\npath = \"{}\"\n{colors}",
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
