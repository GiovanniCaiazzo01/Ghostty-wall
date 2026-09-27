use std::{
    fs,
    process::{Command, Output},
};

fn run(home: &std::path::Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ghostty-wall"))
        .args(args)
        .env_clear()
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join("xdg"))
        .env("PATH", "/nonexistent")
        .output()
        .unwrap()
}

#[test]
fn edit_help_is_read_only_and_missing_named_profile_never_prompts_or_creates() {
    let home = tempfile::tempdir().unwrap();
    let help = run(home.path(), &["edit", "--help"]);
    assert!(help.status.success());
    let text = String::from_utf8(help.stdout).unwrap();
    for expected in [
        "edit [PROFILE]",
        "Save and use",
        "Back to editor",
        "exact #RRGGBB",
        "only edited slots become Customized; others stay Automatic",
        "Customizing version 1 generated colors saves as version 2",
        "NOT live Ghostty reload",
    ] {
        assert!(text.contains(expected), "{text}");
    }
    assert_eq!(fs::read_dir(home.path()).unwrap().count(), 0);
    assert!(run(home.path(), &["init"]).status.success());
    let missing = run(home.path(), &["edit", "boy"]);
    assert_eq!(missing.status.code(), Some(3));
    assert!(missing.stdout.is_empty());
    let error = String::from_utf8(missing.stderr).unwrap();
    assert!(error.contains("Profile boy does not exist"), "{error}");
    assert!(error.contains("no files changed"));
    assert!(
        !home
            .path()
            .join("xdg/ghostty/ghostty-wall/profiles/boy.toml")
            .exists()
    );
    let noninteractive = run(home.path(), &["edit", "welcome"]);
    assert_eq!(noninteractive.status.code(), Some(2));
    assert!(
        String::from_utf8(noninteractive.stderr)
            .unwrap()
            .contains("interactive terminal")
    );
}
