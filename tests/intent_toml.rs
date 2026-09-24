use ghostty_wall::{
    codec::intent::{
        IntentTomlError, parse_config_toml, parse_named_profile_toml, parse_profile_toml,
    },
    domain::{ColorsIntent, SourceIntent, WallpaperIntent, WallpaperSelection},
};

#[test]
fn config_toml_loads_validated_sources() {
    let config = parse_config_toml(
        r#"
schema_version = 1

[sources.anime]
kind = "github"
repository = "ThePrimeagen/anime"
ref = "master"
path = "wallpapers"

[sources.local]
kind = "local-directory"
path = "~/Pictures/wallpapers"
"#,
    )
    .expect("valid RFC 0003 config loads");

    assert_eq!(config.sources[0].0.as_str(), "anime");
    assert!(matches!(config.sources[0].1, SourceIntent::Github { .. }));
    assert_eq!(config.sources[1].0.as_str(), "local");
}

#[test]
fn minimal_profile_has_empty_managed_intent() {
    let profile = parse_profile_toml("schema_version = 1\n").expect("minimal profile loads");

    assert_eq!(profile.wallpaper, None);
    assert_eq!(profile.colors, None);
    assert_eq!(profile.terminal, None);
}

#[test]
fn profile_toml_preserves_decimal_precision_as_fixed_point() {
    let profile = parse_profile_toml(
        r#"
schema_version = 1

[wallpaper]
mode = "source"
source = "anime"
selection = "random"
opacity = 0.11

[terminal]
font_size = 13.5
background_opacity = 0.92
"#,
    )
    .expect("valid decimals load");

    let WallpaperIntent::Source {
        opacity, selection, ..
    } = profile.wallpaper.unwrap()
    else {
        panic!("wallpaper source expected");
    };
    assert_eq!(opacity.unwrap().get(), 110_000);
    assert_eq!(selection, WallpaperSelection::Random);
    let terminal = profile.terminal.unwrap();
    assert_eq!(terminal.font_size.unwrap().get(), 13_500);
    assert_eq!(terminal.background_opacity.unwrap().get(), 920_000);
}

#[test]
fn invalid_profile_forms_fail_clearly() {
    for toml in [
        "schema_version = 1\nunknown = true\n",
        "schema_version = 2\n",
        "schema_version = 1\n[terminal]\nfont_size = 0.1234\n",
        "schema_version = 1\n[terminal]\nfont_size = 1e1\n",
        "schema_version = 1\n[terminal]\n",
        "schema_version = 1\n[wallpaper]\nmode = \"none\"\nsource = \"anime\"\n",
        "schema_version = 1\n[colors]\nmode = \"generated\"\n",
    ] {
        assert!(
            parse_profile_toml(toml).is_err(),
            "invalid TOML passed: {toml}"
        );
    }
}

#[test]
fn quoted_decimal_keys_are_not_silently_unmanaged() {
    let profile = parse_profile_toml(
        "schema_version = 1\n[terminal]\n\"font_size\" = 13.5\ncursor_style = \"bar\"\n",
    )
    .unwrap();
    assert_eq!(profile.terminal.unwrap().font_size.unwrap().get(), 13_500);

    let profile = parse_profile_toml(
        "schema_version = 1\n[wallpaper]\nmode = \"source\"\nsource = \"local\"\nselection = \"random\"\n\"opacity\" = 0.11\n",
    )
    .unwrap();
    let ghostty_wall::domain::WallpaperIntent::Source { opacity, .. } = profile.wallpaper.unwrap()
    else {
        panic!("source wallpaper expected");
    };
    assert_eq!(opacity.unwrap().get(), 110_000);
    let inline =
        parse_profile_toml("schema_version = 1\nterminal = { font_size = 13.5 }\n").unwrap();
    assert_eq!(inline.terminal.unwrap().font_size.unwrap().get(), 13_500);
}

#[test]
fn named_profile_rejects_bad_filename_and_unknown_source() {
    let config = parse_config_toml("schema_version = 1\n[sources]\n").unwrap();
    let source = "schema_version = 1\n[wallpaper]\nmode = \"source\"\nsource = \"missing\"\nselection = \"random\"\n";
    assert!(parse_named_profile_toml("Bad", &config, source).is_err());
    assert!(parse_named_profile_toml("night", &config, source).is_err());
    let (id, _) = parse_named_profile_toml("night", &config, "schema_version = 1\n").unwrap();
    assert_eq!(id.as_str(), "night");
}

#[test]
fn explicit_colors_require_sixteen_lower_hex_palette_entries() {
    let profile = parse_profile_toml(
        r##"
schema_version = 1

[colors]
mode = "explicit"
background = "1a1b26"
foreground = "c0caf5"
palette = [
  "15161e", "f7768e", "9ece6a", "e0af68",
  "7aa2f7", "bb9af7", "7dcfff", "a9b1d6",
  "414868", "f7768e", "9ece6a", "e0af68",
  "7aa2f7", "bb9af7", "7dcfff", "c0caf5",
]
"##,
    )
    .expect("explicit colors load");

    assert!(matches!(
        profile.colors,
        Some(ColorsIntent::Explicit { .. })
    ));
}

#[test]
fn config_rejects_bad_source_identifiers_and_paths() {
    let err = parse_config_toml(
        r#"
schema_version = 1

[sources.Bad]
kind = "github"
repository = "owner/repo"
"#,
    )
    .unwrap_err();
    assert!(matches!(err, IntentTomlError::Validation(_)));

    assert!(
        parse_config_toml(
            r#"
schema_version = 1

[sources.good]
kind = "github"
repository = "owner/repo"
path = "../wallpapers"
"#,
        )
        .is_err()
    );
}
