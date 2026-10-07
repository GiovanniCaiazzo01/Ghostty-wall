use std::process::Command;

#[test]
fn management_help_is_read_only_and_distinguishes_sample_from_reload() {
    let home = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_ghostty-wall"))
        .args(["tui", "--help"])
        .env_clear()
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join("config"))
        .env("PATH", "/nonexistent")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(std::fs::read_dir(home.path()).unwrap().count(), 0);
    let help = String::from_utf8(output.stdout).unwrap();
    for label in [
        "n Create",
        "e Edit draft",
        "x Delete",
        "a Use",
        "without activating",
        "s Save without applying",
        "u Save and use",
        "Esc/q Cancel",
        "default: Back to editor",
        "definite save failure retains the draft",
        "uncertainty requires inspection",
        "NOT live Ghostty reload",
        "40x12",
        "v shows",
    ] {
        assert!(help.contains(label), "missing {label}");
    }
}
