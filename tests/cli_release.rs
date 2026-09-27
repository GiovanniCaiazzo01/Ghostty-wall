use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn fresh_init_provides_working_welcome_profile() {
    let home = temp_dir("welcome");
    let config_home = home.join("config");
    assert_success(&run(&home, &config_home, &["init"]));
    let root = config_home.join("ghostty/ghostty-wall");
    assert!(root.join("profiles/welcome.toml").is_file());
    assert!(root.join("profiles/welcome.png").is_file());

    let plan = run(&home, &config_home, &["plan", "welcome", "--json"]);
    assert_success(&plan);
    let plan: serde_json::Value = serde_json::from_slice(&plan.stdout).unwrap();
    assert_eq!(plan["profile"]["id"], "welcome");
    assert_success(&run(&home, &config_home, &["apply", "welcome"]));
    assert_success(&run(&home, &config_home, &["doctor"]));
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn cli_preview_does_not_activate_another_profile() {
    let home = temp_dir("preview-not-activation");
    let config_home = home.join("config");
    assert_success(&run(&home, &config_home, &["init"]));
    assert_success(&run(&home, &config_home, &["apply", "welcome"]));
    let root = config_home.join("ghostty/ghostty-wall");
    let projection = fs::read(root.join("current.ghostty")).unwrap();
    let history_count = fs::read_dir(root.join("history/activations"))
        .unwrap()
        .count();
    let image = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/white.png");
    assert_success(&run(&home, &config_home, &["new", "night", image]));
    assert_success(&run(&home, &config_home, &["preview", "night"]));
    assert_eq!(fs::read(root.join("current.ghostty")).unwrap(), projection);
    assert_eq!(
        fs::read_dir(root.join("history/activations"))
            .unwrap()
            .count(),
        history_count
    );
    let help = run(&home, &config_home, &["--help"]);
    assert_success(&help);
    assert!(String::from_utf8_lossy(&help.stdout).contains("not live Ghostty reload"));
    assert!(String::from_utf8_lossy(&help.stdout).contains("Live draft sessions are library-only"));
    assert!(String::from_utf8_lossy(&help.stdout).contains("doctor is read-only"));
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn welcome_cannot_be_deleted_even_when_active() {
    let home = temp_dir("welcome-delete");
    let config_home = home.join("config");
    assert_success(&run(&home, &config_home, &["init"]));
    assert_success(&run(&home, &config_home, &["apply", "welcome"]));
    let root = config_home.join("ghostty/ghostty-wall");
    let before = fs::read(root.join("profiles/welcome.toml")).unwrap();
    let projection = fs::read(root.join("current.ghostty")).unwrap();
    let result = run(&home, &config_home, &["delete", "welcome"]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("cannot be deleted"));
    assert_eq!(
        fs::read(root.join("profiles/welcome.toml")).unwrap(),
        before
    );
    assert_eq!(fs::read(root.join("current.ghostty")).unwrap(), projection);
    assert_success(&run(&home, &config_home, &["doctor"]));
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn installed_welcome_and_active_profile_cannot_be_renamed_away() {
    let home = temp_dir("rename-guards");
    let config_home = home.join("config");
    assert_success(&run(&home, &config_home, &["init"]));
    let root = config_home.join("ghostty/ghostty-wall");
    let welcome = root.join("profiles/welcome.toml");
    let before = fs::read(&welcome).unwrap();
    let config = fs::read(root.join("config.toml")).unwrap();
    let result = run(&home, &config_home, &["rename", "welcome", "away"]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("cannot be renamed"));
    assert_eq!(fs::read(&welcome).unwrap(), before);
    assert_eq!(fs::read(root.join("config.toml")).unwrap(), config);
    assert!(!root.join("profiles/away.toml").exists());

    let night = root.join("profiles/night.toml");
    fs::write(&night, "schema_version = 1\n[terminal]\nfont_size = 13.0\n").unwrap();
    assert_success(&run(&home, &config_home, &["apply", "night"]));
    let projection = fs::read(root.join("current.ghostty")).unwrap();
    let history = root.join("history/activations/act-v1-0000000000000001.json");
    let activation = fs::read(&history).unwrap();
    let result = run(&home, &config_home, &["rename", "night", "evening"]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("Apply another Profile"));
    assert!(night.exists());
    assert!(!root.join("profiles/evening.toml").exists());
    assert_eq!(fs::read(&history).unwrap(), activation);
    assert_eq!(fs::read(root.join("current.ghostty")).unwrap(), projection);
    assert_success(&run(&home, &config_home, &["apply", "welcome"]));
    assert_success(&run(&home, &config_home, &["rename", "night", "evening"]));
    assert_success(&run_with_input(
        &home,
        &config_home,
        &["delete", "evening"],
        b"y\n",
    ));
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn deleting_active_profile_on_customized_install_without_welcome_fails_safely() {
    let home = temp_dir("active-delete-no-welcome");
    let config_home = home.join("config");
    assert_success(&run(&home, &config_home, &["init"]));
    let root = config_home.join("ghostty/ghostty-wall");
    // Model a customized older installation: no Welcome intent or Source.
    fs::write(
        root.join("config.toml"),
        "schema_version = 1\n\n[sources]\n",
    )
    .unwrap();
    fs::remove_file(root.join("profiles/welcome.toml")).unwrap();
    fs::remove_file(root.join("profiles/welcome.png")).unwrap();
    let absent = run(&home, &config_home, &["delete", "welcome"]);
    assert!(!absent.status.success());
    assert!(String::from_utf8_lossy(&absent.stderr).contains("does not exist"));
    fs::write(
        root.join("profiles/night.toml"),
        "schema_version = 1\n\n[terminal]\nfont_size = 13.0\n",
    )
    .unwrap();
    fs::write(
        root.join("profiles/day.toml"),
        "schema_version = 1\n\n[terminal]\nfont_size = 14.0\n",
    )
    .unwrap();
    assert_success(&run(&home, &config_home, &["apply", "night"]));
    let before = fs::read(root.join("current.ghostty")).unwrap();
    let history = fs::read_dir(root.join("history/activations"))
        .unwrap()
        .count();
    let result = run_with_input(&home, &config_home, &["delete", "night"], b"y\n");
    assert!(!result.status.success());
    let error = String::from_utf8_lossy(&result.stderr);
    assert!(
        error.contains("active") && error.contains("no files changed"),
        "{error}"
    );
    assert!(root.join("profiles/night.toml").exists());
    assert!(!root.join("profiles/welcome.toml").exists());
    assert_eq!(fs::read(root.join("current.ghostty")).unwrap(), before);
    assert_eq!(
        fs::read_dir(root.join("history/activations"))
            .unwrap()
            .count(),
        history
    );
    assert_success(&run_with_input(
        &home,
        &config_home,
        &["delete", "day"],
        b"y\n",
    ));
    assert_success(&run(&home, &config_home, &["doctor"]));
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn deleting_inactive_profile_retains_shared_imported_image() {
    let home = temp_dir("shared-image-delete");
    let config_home = home.join("config");
    assert_success(&run(&home, &config_home, &["init"]));
    let root = config_home.join("ghostty/ghostty-wall");
    let image = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/white.png");
    assert_success(&run(&home, &config_home, &["new", "one", image]));
    assert_success(&run(&home, &config_home, &["duplicate", "one", "two"]));
    assert_success(&run_with_input(
        &home,
        &config_home,
        &["delete", "one"],
        b"y\n",
    ));
    assert!(root.join("profiles/one.png").exists());
    assert_success(&run(&home, &config_home, &["plan", "two"]));
    assert_eq!(
        fs::read(image).unwrap(),
        fs::read(root.join("profiles/one.png")).unwrap()
    );
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn replay_does_not_invent_an_active_profile_for_deletion() {
    let home = temp_dir("replay-delete");
    let config_home = home.join("config");
    assert_success(&run(&home, &config_home, &["init"]));
    let root = config_home.join("ghostty/ghostty-wall");
    assert_success(&run(&home, &config_home, &["apply", "welcome"]));
    fs::write(
        root.join("profiles/night.toml"),
        "schema_version = 1\n\n[terminal]\nfont_size = 14.0\n",
    )
    .unwrap();
    assert_success(&run(&home, &config_home, &["apply", "night"]));
    let active = run(&home, &config_home, &["delete", "night"]);
    assert_success(&active);
    assert!(String::from_utf8_lossy(&active.stdout).contains("Deletion cancelled"));
    assert_success(&run(&home, &config_home, &["previous"]));
    let result = run(&home, &config_home, &["delete", "night"]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("History replay"));
    let result = run(&home, &config_home, &["rename", "night", "evening"]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("History replay"));
    assert!(!root.join("profiles/evening.toml").exists());
    assert!(root.join("profiles/night.toml").exists());
    assert_success(&run(&home, &config_home, &["apply", "welcome"]));
    assert_success(&run_with_input(
        &home,
        &config_home,
        &["delete", "night"],
        b"y\n",
    ));
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn old_empty_install_can_opt_in_to_welcome_without_creating_files() {
    let home = temp_dir("welcome-upgrade");
    let config_home = home.join("config");
    assert_success(&run(&home, &config_home, &["init"]));
    let root = config_home.join("ghostty/ghostty-wall");
    fs::write(
        root.join("config.toml"),
        "schema_version = 1\n\n[sources]\n",
    )
    .unwrap();
    fs::remove_file(root.join("profiles/welcome.toml")).unwrap();
    fs::remove_file(root.join("profiles/welcome.png")).unwrap();
    assert!(
        !run(&home, &config_home, &["plan", "welcome"])
            .status
            .success()
    );

    assert_success(&run(&home, &config_home, &["init", "--welcome"]));
    assert_success(&run(&home, &config_home, &["init", "--welcome"]));
    assert_success(&run(&home, &config_home, &["plan", "welcome"]));
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn cli_runs_profile_to_environment_workflow() {
    let home = temp_dir("workflow");
    let config_home = home.join("config");

    let init = run(&home, &config_home, &["init"]);
    assert_success(&init);

    let managed_root = config_home.join("ghostty/ghostty-wall");
    fs::write(
        managed_root.join("profiles/minimal.toml"),
        "schema_version = 1\n\n[terminal]\nfont_size = 13.0\n",
    )
    .unwrap();

    let plan = run(&home, &config_home, &["plan", "minimal", "--json"]);
    assert_success(&plan);
    let plan: serde_json::Value = serde_json::from_slice(&plan.stdout).unwrap();
    assert_eq!(plan["schema_version"], 1);
    assert_eq!(plan["profile"]["id"], "minimal");
    assert_eq!(plan["operations"][0]["kind"], "ensure_environment");

    assert_success(&run(&home, &config_home, &["apply", "minimal"]));
    assert_success(&run(&home, &config_home, &["apply", "minimal"]));
    assert_success(&run(&home, &config_home, &["previous"]));
    assert!(
        managed_root
            .join("history/activations/act-v1-0000000000000003.json")
            .is_file()
    );

    let doctor = run(&home, &config_home, &["doctor"]);
    assert_success(&doctor);
    assert!(String::from_utf8_lossy(&doctor.stdout).contains("managed-layout: verified"));

    let tui = run_with_input(&home, &config_home, &["tui"], b"q\n");
    assert_success(&tui);
    assert!(String::from_utf8_lossy(&tui.stdout).contains("Cancelled."));

    fs::remove_dir_all(home).unwrap();
}

#[test]
fn invalid_profile_id_explains_lowercase_without_dumping_help() {
    let home = temp_dir("invalid-id");
    let config_home = home.join("config");
    assert_success(&run(&home, &config_home, &["init"]));
    let image = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/white.png");
    let created = run(&home, &config_home, &["new", "miaProva", image]);
    assert_eq!(created.status.code(), Some(2));
    let error = String::from_utf8_lossy(&created.stderr);
    assert!(error.contains("lowercase"), "{error}");
    assert!(error.contains("mia-prova"), "{error}");
    assert!(!error.contains("Usage:"), "{error}");
    let applied = run(&home, &config_home, &["apply", "miaProva"]);
    assert_eq!(applied.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&applied.stderr).contains("mia-prova"));
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn new_image_uses_existing_profiles_source_when_welcome_source_was_removed() {
    let home = temp_dir("custom-source-new");
    let config_home = home.join("config");
    assert_success(&run(&home, &config_home, &["init"]));
    let root = config_home.join("ghostty/ghostty-wall");
    fs::write(
        root.join("config.toml"),
        "schema_version = 1\n\n[sources.custom]\nkind = \"local-directory\"\npath = \"profiles\"\n",
    )
    .unwrap();
    let image = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/white.png");
    assert_success(&run(&home, &config_home, &["new", "trial", image]));
    assert!(
        fs::read_to_string(root.join("profiles/trial.toml"))
            .unwrap()
            .contains("source = \"custom\"")
    );
    assert_success(&run(&home, &config_home, &["preview", "trial"]));
    let listing = run(&home, &config_home, &["list"]);
    assert_success(&listing);
    let text = String::from_utf8_lossy(&listing.stdout);
    assert!(text.contains("trial\n"), "{text}");
    assert!(text.contains("welcome (invalid:"), "{text}");
    let tui = run_with_input(
        &home,
        &config_home,
        &["tui"],
        b"tab\nj\nenter\nk\nenter\nq\n",
    );
    assert_success(&tui);
    assert!(String::from_utf8_lossy(&tui.stderr).contains("unknown field sources.welcome"));
    assert!(String::from_utf8_lossy(&tui.stdout).contains("Profile: trial"));
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn new_image_profile_previews_without_apply_and_rejects_overwrite() {
    let home = temp_dir("new-image");
    let config_home = home.join("config");
    assert_success(&run(&home, &config_home, &["init"]));
    let root = config_home.join("ghostty/ghostty-wall");
    let image = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/white.png");
    let created = run_with_input(
        &home,
        &config_home,
        &["new", "my-night"],
        format!("{image}\n").as_bytes(),
    );
    assert_success(&created);
    assert!(String::from_utf8_lossy(&created.stdout).contains("ANSI 15: #"));
    assert!(root.join("profiles/my-night.png").is_file());
    assert!(!root.join("current.ghostty").exists());
    let prior = fs::read(root.join("profiles/my-night.toml")).unwrap();
    assert_success(&run(&home, &config_home, &["new", "my-night", image]));
    assert_eq!(
        fs::read(root.join("profiles/my-night.toml")).unwrap(),
        prior
    );
    let different = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/palette.png");
    assert!(
        !run(&home, &config_home, &["new", "my-night", different])
            .status
            .success()
    );
    assert_eq!(
        fs::read(root.join("profiles/my-night.toml")).unwrap(),
        prior
    );
    let bad = root.join("bad.png");
    fs::write(&bad, b"not an image").unwrap();
    assert!(
        !run(
            &home,
            &config_home,
            &["new", "broken", bad.to_str().unwrap()]
        )
        .status
        .success()
    );
    assert!(!root.join("profiles/broken.toml").exists());
    assert!(!root.join("profiles/broken.png").exists());
    assert_success(&run(&home, &config_home, &["plan", "my-night"]));
    assert_success(&run(&home, &config_home, &["apply", "my-night"]));
    let first_projection = fs::read(root.join("current.ghostty")).unwrap();
    assert_success(&run(&home, &config_home, &["new", "alternate", different]));
    assert_success(&run(&home, &config_home, &["apply", "alternate"]));
    assert_ne!(
        fs::read(root.join("current.ghostty")).unwrap(),
        first_projection
    );
    fs::remove_file(root.join("profiles/my-night.png")).unwrap();
    assert_success(&run(&home, &config_home, &["previous"]));
    assert_eq!(
        fs::read(root.join("current.ghostty")).unwrap(),
        first_projection
    );
    let tui = run_with_input(
        &home,
        &config_home,
        &[],
        format!("n\ntui-image\n{image}\nq\n").as_bytes(),
    );
    assert_success(&tui);
    assert!(root.join("profiles/tui-image.toml").is_file());
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn tui_profile_lifecycle_keeps_back_navigation_and_history() {
    let home = temp_dir("tui-manage");
    let config_home = home.join("config");
    assert_success(&run(&home, &config_home, &["init"]));
    let image = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/white.png");
    let input = format!("n\nfirst\n{image}\nd\nsecond\nr\nthird\nb\nh\nx\ny\nq\n");
    let output = run_with_input(&home, &config_home, &[], input.as_bytes());
    assert_success(&output);
    let root = config_home.join("ghostty/ghostty-wall");
    assert!(root.join("profiles/first.toml").is_file());
    assert!(!root.join("profiles/second.toml").exists());
    assert!(!root.join("profiles/third.toml").exists());
    assert!(String::from_utf8_lossy(&output.stdout).contains("Profile third deleted"));
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn source_registration_is_single_file_atomic_and_validated() {
    let home = temp_dir("sources");
    let config_home = home.join("config");
    assert_success(&run(&home, &config_home, &["init"]));
    let config = config_home.join("ghostty/ghostty-wall/config.toml");
    assert_success(&run(
        &home,
        &config_home,
        &[
            "source",
            "add",
            "remote",
            "github",
            "owner/repo",
            "--path",
            "wallpapers",
        ],
    ));
    let saved = fs::read_to_string(&config).unwrap();
    assert_success(&run(
        &home,
        &config_home,
        &[
            "source",
            "add",
            "remote",
            "github",
            "owner/repo",
            "--path",
            "wallpapers",
        ],
    ));
    assert_eq!(fs::read_to_string(&config).unwrap(), saved);
    assert!(saved.contains("path = \"wallpapers\""));
    assert!(!saved.contains("ref ="));
    assert!(
        !run(
            &home,
            &config_home,
            &["source", "add", "bad", "github", "invalid-repo"]
        )
        .status
        .success()
    );
    assert_eq!(fs::read_to_string(config).unwrap(), saved);
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn first_launch_tui_initializes_creates_tweaks_and_applies() {
    let home = temp_dir("first-tui");
    let config_home = home.join("config");
    let image = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/white.png");
    let input = format!("i\nn\nfirst\n{image}\nt\nfont_size\n19.0\na\n");
    let output = run_with_input(&home, &config_home, &[], input.as_bytes());
    assert_success(&output);
    let root = config_home.join("ghostty/ghostty-wall");
    assert!(root.join("profiles/first.toml").is_file());
    assert!(root.join("current.ghostty").is_file());
    assert!(String::from_utf8_lossy(&output.stdout).contains("Terminal font size: 19.000"));
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn user_manages_profile_and_source_without_toml_editor() {
    let home = temp_dir("manage");
    let config_home = home.join("config");
    assert_success(&run(&home, &config_home, &["init"]));
    let root = config_home.join("ghostty/ghostty-wall");
    let images = home.join("pictures");
    fs::create_dir(&images).unwrap();
    fs::copy(
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/white.jpg"),
        images.join("sky.jpg"),
    )
    .unwrap();
    assert_success(&run(
        &home,
        &config_home,
        &[
            "source",
            "add",
            "pictures",
            "local",
            images.to_str().unwrap(),
        ],
    ));
    assert_success(&run(
        &home,
        &config_home,
        &["new", "day", "--source", "pictures", "--path", "sky.jpg"],
    ));
    assert_success(&run(
        &home,
        &config_home,
        &["edit", "day", "terminal.font_size", "18.5"],
    ));
    assert_success(&run(
        &home,
        &config_home,
        &["edit", "day", "colors.background", "202020"],
    ));
    assert_success(&run(
        &home,
        &config_home,
        &["edit", "day", "wallpaper.fit", "contain"],
    ));
    assert_success(&run(
        &home,
        &config_home,
        &["edit", "day", "wallpaper.repeat", "true"],
    ));
    assert_success(&run(
        &home,
        &config_home,
        &["edit", "day", "colors.palette.1", "ff0000"],
    ));
    let prior = fs::read(root.join("profiles/day.toml")).unwrap();
    assert!(
        !run(
            &home,
            &config_home,
            &["edit", "day", "wallpaper.opacity", "2.0"]
        )
        .status
        .success()
    );
    assert_eq!(fs::read(root.join("profiles/day.toml")).unwrap(), prior);
    let preview = run(&home, &config_home, &["preview", "day"]);
    assert_success(&preview);
    let text = String::from_utf8_lossy(&preview.stdout);
    assert!(text.contains("Terminal font size: 18.500"), "{text}");
    assert!(text.contains("background: #202020"), "{text}");
    assert!(text.contains("ANSI 1: #ff0000"), "{text}");
    assert!(text.contains("Wallpaper fit: contain"), "{text}");
    assert!(text.contains("Wallpaper repeat: true"), "{text}");
    assert_success(&run(
        &home,
        &config_home,
        &["edit", "day", "colors.mode", "generated"],
    ));
    let reset = run(&home, &config_home, &["preview", "day"]);
    assert_success(&reset);
    assert!(!String::from_utf8_lossy(&reset.stdout).contains("ANSI 1: #ff0000"));
    assert_success(&run(&home, &config_home, &["apply", "day"]));
    assert_success(&run(&home, &config_home, &["duplicate", "day", "copy"]));
    assert_success(&run(&home, &config_home, &["duplicate", "day", "copy"]));
    assert_success(&run(&home, &config_home, &["rename", "copy", "evening"]));
    assert_success(&run(&home, &config_home, &["apply", "evening"]));
    assert_success(&run_with_input(
        &home,
        &config_home,
        &["delete", "day"],
        b"y\n",
    ));
    let missing = run(&home, &config_home, &["delete", "day"]);
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("does not exist"));
    assert!(!root.join("profiles/day.toml").exists());
    assert_success(&run(&home, &config_home, &["previous"]));
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn common_read_only_paths_preview_and_history() {
    let home = temp_dir("common-paths");
    let config_home = home.join("config");
    assert_success(&run(&home, &config_home, &["init"]));
    let root = config_home.join("ghostty/ghostty-wall");
    let before = fs::read_dir(root.join("history/activations"))
        .unwrap()
        .count();
    let list = run(&home, &config_home, &["list"]);
    assert_success(&list);
    assert_eq!(String::from_utf8_lossy(&list.stdout), "welcome\n");
    let preview = run(&home, &config_home, &["preview", "welcome"]);
    assert_success(&preview);
    let text = String::from_utf8_lossy(&preview.stdout);
    for field in [
        "Wallpaper: welcome.png",
        "background: #",
        "foreground: #",
        "ANSI 15: #",
        "Terminal: unmanaged",
    ] {
        assert!(text.contains(field), "{field}: {text}");
    }
    assert_eq!(
        fs::read_dir(root.join("history/activations"))
            .unwrap()
            .count(),
        before
    );
    let empty = run(&home, &config_home, &["history"]);
    assert_success(&empty);
    assert!(empty.stdout.is_empty());
    assert_success(&run(&home, &config_home, &["apply", "welcome"]));
    let history = run(&home, &config_home, &["history"]);
    assert_success(&history);
    assert!(String::from_utf8_lossy(&history.stdout).contains("1 env-v1-"));
    let tui = run_with_input(&home, &config_home, &[], b"q\n");
    assert_success(&tui);
    assert!(String::from_utf8_lossy(&tui.stdout).contains("Cancelled."));
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn cli_applies_generated_colors_from_local_profile() {
    let home = temp_dir("generated");
    let config_home = home.join("config");
    assert_success(&run(&home, &config_home, &["init"]));

    let managed_root = config_home.join("ghostty/ghostty-wall");
    let wallpapers = home.join("wallpapers");
    fs::create_dir(&wallpapers).unwrap();
    fs::copy(
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/white.png"),
        wallpapers.join("white.png"),
    )
    .unwrap();
    fs::write(
        managed_root.join("config.toml"),
        format!(
            "schema_version = 1\n\n[sources.local]\nkind = \"local-directory\"\npath = {:?}\n",
            wallpapers
        ),
    )
    .unwrap();
    fs::write(
        managed_root.join("profiles/generated.toml"),
        "schema_version = 1\n\n[wallpaper]\nmode = \"source\"\nsource = \"local\"\nselection = \"path\"\npath = \"white.png\"\n\n[colors]\nmode = \"generated\"\n",
    )
    .unwrap();

    let plan = run(&home, &config_home, &["plan", "generated", "--json"]);
    assert_success(&plan);
    let plan: serde_json::Value = serde_json::from_slice(&plan.stdout).unwrap();
    assert_eq!(plan["asset"]["media_type"], "image/png");
    assert_eq!(plan["color_resolution"]["algorithm"], "kmeans-v3");
    assert!(
        plan.pointer("/environment/manifest/colors/background")
            .is_some()
    );
    assert_success(&run(&home, &config_home, &["apply", "generated"]));
    assert!(managed_root.join("current.ghostty").is_file());

    fs::remove_dir_all(home).unwrap();
}

#[test]
fn new_profiles_apply_distinct_wallpaper_colors_to_ghostty() {
    let home = temp_dir("color-switch");
    let config_home = home.join("config");
    assert_success(&run(&home, &config_home, &["init"]));
    let root = config_home.join("ghostty/ghostty-wall");
    let wallpapers = home.join("wallpapers");
    fs::create_dir(&wallpapers).unwrap();
    for name in ["white", "palette"] {
        fs::copy(
            format!("{}/tests/fixtures/{name}.png", env!("CARGO_MANIFEST_DIR")),
            wallpapers.join(format!("{name}.png")),
        )
        .unwrap();
    }
    fs::write(
        root.join("config.toml"),
        format!(
            "schema_version = 1\n\n[sources.local]\nkind = \"local-directory\"\npath = {:?}\n",
            wallpapers
        ),
    )
    .unwrap();

    let mut projections = Vec::new();
    for name in ["white", "palette"] {
        fs::write(
            root.join(format!("profiles/{name}.toml")),
            format!("schema_version = 1\n\n[wallpaper]\nmode = \"source\"\nsource = \"local\"\nselection = \"path\"\npath = \"{name}.png\"\n\n[colors]\nmode = \"generated\"\n"),
        )
        .unwrap();
        let plan = run(&home, &config_home, &["plan", name, "--json"]);
        assert_success(&plan);
        let plan: serde_json::Value = serde_json::from_slice(&plan.stdout).unwrap();
        let colors = &plan["environment"]["manifest"]["colors"];
        assert_success(&run(&home, &config_home, &["apply", name]));
        let projection = fs::read_to_string(root.join("current.ghostty")).unwrap();
        for (key, field) in [
            ("background", "background"),
            ("foreground", "foreground"),
            ("cursor-color", "cursor"),
            ("selection-background", "selection_background"),
            ("selection-foreground", "selection_foreground"),
        ] {
            assert!(projection.contains(&format!("{key} = {}", colors[field].as_str().unwrap())));
        }
        for (index, color) in colors["palette"].as_array().unwrap().iter().enumerate() {
            assert!(projection.contains(&format!("palette = {index}={}", color.as_str().unwrap())));
        }
        projections.push(projection);
    }
    assert_ne!(projections[0], projections[1]);

    let root_config = fs::read_to_string(config_home.join("ghostty/config.ghostty")).unwrap();
    assert!(root_config.contains(&format!(
        "config-file = ?{}",
        root.join("current.ghostty").display()
    )));
    if let Ok(probe) = Command::new("ghostty")
        .args(["+show-config", "--changes-only"])
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", &config_home)
        .output()
    {
        assert_success(&probe);
        let loaded = String::from_utf8_lossy(&probe.stdout);
        let background = projections[1]
            .lines()
            .find(|line| line.starts_with("background = "))
            .unwrap();
        let (key, hex) = background.split_once(" = ").unwrap();
        assert!(
            loaded.lines().any(|line| line == format!("{key} = #{hex}")),
            "Ghostty did not load {background}"
        );
    }
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn plan_json_failure_is_machine_readable() {
    let home = temp_dir("json-error");
    let config_home = home.join("config");
    assert_success(&run(&home, &config_home, &["init"]));
    let managed_root = config_home.join("ghostty/ghostty-wall");
    let wallpapers = home.join("wallpapers");
    fs::create_dir(&wallpapers).unwrap();
    fs::copy(
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/white.png"),
        wallpapers.join("white.png"),
    )
    .unwrap();
    fs::write(
        managed_root.join("config.toml"),
        format!(
            "schema_version = 1\n\n[sources.local]\nkind = \"local-directory\"\npath = {:?}\n",
            wallpapers
        ),
    )
    .unwrap();
    fs::write(
        managed_root.join("profiles/random.toml"),
        "schema_version = 1\n\n[wallpaper]\nmode = \"source\"\nsource = \"local\"\nselection = \"random\"\n",
    )
    .unwrap();

    let output = run(&home, &config_home, &["plan", "random", "--json"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stderr.is_empty());
    let error: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(error["error"]["code"], "usage.resolution-seed-required");

    fs::remove_dir_all(home).unwrap();
}

#[test]
fn installer_installs_rust_binary() {
    let home = temp_dir("installer");
    let prefix = home.join("prefix");
    let binary = env!("CARGO_BIN_EXE_ghostty-wall");
    let output = Command::new("bash")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/install.sh"))
        .env("HOME", &home)
        .env("INSTALL_PREFIX", &prefix)
        .env("GHOSTTY_WALL_BINARY", binary)
        .output()
        .unwrap();
    assert_success(&output);

    let installed = prefix.join("bin/ghostty-wall");
    assert!(installed.is_file());
    let version = Command::new(&installed).arg("--version").output().unwrap();
    assert_success(&version);
    assert_eq!(
        String::from_utf8_lossy(&version.stdout),
        format!("ghostty-wall {}\n", env!("CARGO_PKG_VERSION"))
    );

    let config_home = home.join("config");
    let image = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/white.png");
    for args in [
        vec!["init"],
        vec!["new", "installed", image],
        vec!["preview", "installed"],
        vec!["apply", "welcome"],
        vec!["apply", "installed"],
        vec!["previous"],
        vec!["history"],
    ] {
        let output = Command::new(&installed)
            .args(&args)
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", &config_home)
            .env("XDG_RUNTIME_DIR", home.join("runtime"))
            .env_remove("DBUS_SESSION_BUS_ADDRESS")
            .env("PATH", "/nonexistent")
            .output()
            .unwrap();
        assert_success(&output);
    }
    assert!(
        config_home
            .join("ghostty/ghostty-wall/current.ghostty")
            .is_file()
    );
    fs::remove_dir_all(home).unwrap();
}

fn run(home: &Path, config_home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ghostty-wall"))
        .args(args)
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", config_home)
        .env("XDG_RUNTIME_DIR", home.join("runtime"))
        .env_remove("DBUS_SESSION_BUS_ADDRESS")
        .env("PATH", "/nonexistent")
        .output()
        .unwrap()
}

fn run_with_input(home: &Path, config_home: &Path, args: &[&str], input: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_ghostty-wall"))
        .args(args)
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", config_home)
        .env("XDG_RUNTIME_DIR", home.join("runtime"))
        .env_remove("DBUS_SESSION_BUS_ADDRESS")
        .env("PATH", "/nonexistent")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "status: {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn temp_dir(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("ghostty-wall-cli-{name}-{unique}"));
    fs::create_dir_all(&path).unwrap();
    path
}
