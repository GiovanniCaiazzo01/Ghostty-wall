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
    assert_eq!(plan["color_resolution"]["algorithm"], "kmeans-v1");
    assert!(
        plan.pointer("/environment/manifest/colors/background")
            .is_some()
    );
    assert_success(&run(&home, &config_home, &["apply", "generated"]));
    assert!(managed_root.join("current.ghostty").is_file());

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
fn v1_installer_installs_rust_binary() {
    let home = temp_dir("installer");
    let prefix = home.join("prefix");
    let binary = env!("CARGO_BIN_EXE_ghostty-wall");
    let output = Command::new("bash")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/install-v1.sh"
        ))
        .env("HOME", &home)
        .env("INSTALL_PREFIX", &prefix)
        .env("GHOSTTY_WALL_BINARY", binary)
        .output()
        .unwrap();
    assert_success(&output);

    let installed = prefix.join("bin/ghostty-wall");
    assert!(installed.is_file());
    let version = Command::new(installed).arg("--version").output().unwrap();
    assert_success(&version);
    assert_eq!(
        String::from_utf8_lossy(&version.stdout),
        "ghostty-wall 1.0.0\n"
    );

    fs::remove_dir_all(home).unwrap();
}

fn run(home: &Path, config_home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ghostty-wall"))
        .args(args)
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", config_home)
        .output()
        .unwrap()
}

fn run_with_input(home: &Path, config_home: &Path, args: &[&str], input: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_ghostty-wall"))
        .args(args)
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", config_home)
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
