#![cfg(unix)]

use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};

use ghostty_wall::init::InitPaths;
use serde_json::Value;

struct Sandbox {
    home: tempfile::TempDir,
    paths: InitPaths,
}

impl Sandbox {
    fn empty() -> Self {
        let home = tempfile::tempdir().unwrap();
        let paths = InitPaths {
            home: home.path().to_owned(),
            xdg_config_home: Some(home.path().join("config")),
        };
        Self { home, paths }
    }

    fn initialized() -> Self {
        let sandbox = Self::empty();
        success(sandbox.run(&["init"], ""));
        sandbox
    }

    fn root(&self) -> PathBuf {
        self.paths.managed_root()
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_ghostty-wall"));
        command
            .env_clear()
            .env("HOME", self.home.path())
            .env(
                "XDG_CONFIG_HOME",
                self.paths.xdg_config_home.as_ref().unwrap(),
            )
            .env("XDG_RUNTIME_DIR", self.home.path().join("runtime"))
            .env("PATH", "/nonexistent")
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    }

    fn run(&self, args: &[&str], input: &str) -> Output {
        feed(self.command(args), input)
    }

    fn plan(&self, id: &str) -> Value {
        serde_json::from_str(&success(self.run(&["plan", id, "--json"], ""))).unwrap()
    }
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn image_input(image: &Path, use_now: bool) -> String {
    format!(
        "i\npath:{}\ns\n{}\n",
        image.display(),
        if use_now { "y" } else { "n" }
    )
}

fn feed(mut command: Command, input: &str) -> Output {
    let mut child = command.spawn().unwrap();
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(input.as_bytes());
    }
    child.wait_with_output().unwrap()
}

fn limit_file_size(command: &mut Command, bytes: libc::rlim_t) {
    // SAFETY: only this child is limited; the pre-exec callback uses async-signal-safe libc calls.
    unsafe {
        command.pre_exec(move || {
            let limit = libc::rlimit {
                rlim_cur: bytes,
                rlim_max: bytes,
            };
            if libc::setrlimit(libc::RLIMIT_FSIZE, &limit) != 0
                || libc::signal(libc::SIGXFSZ, libc::SIG_IGN) == libc::SIG_ERR
            {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

fn success(output: Output) -> String {
    assert!(
        output.status.success(),
        "exit {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    String::from_utf8(output.stdout).unwrap()
}

fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn visit(root: &Path, dir: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                visit(root, &entry.path(), files);
            } else {
                files.insert(
                    entry.path().strip_prefix(root).unwrap().to_owned(),
                    fs::read(entry.path()).unwrap(),
                );
            }
        }
    }
    let mut files = BTreeMap::new();
    visit(root, root, &mut files);
    files
}

#[test]
fn old_v1_install_without_welcome_supports_create_later_apply_and_history_replay() {
    let s = Sandbox::initialized();
    let root = s.root();
    fs::remove_file(root.join("profiles/welcome.toml")).unwrap();
    fs::remove_file(root.join("profiles/welcome.png")).unwrap();
    let config = "# pre-Welcome customized installation\nschema_version = 1\n[sources.personal-images]\nkind = \"local-directory\"\npath = \"profiles\"\n";
    fs::write(root.join("config.toml"), config).unwrap();
    let old = "# authored v1 recipe: do not migrate\nschema_version = 1\n[wallpaper]\nmode = \"none\"\n[terminal]\nfont_size = 17.5\n";
    fs::write(root.join("profiles/old.toml"), old).unwrap();
    success(s.run(&["apply", "old"], ""));
    let old_plan = s.plan("old");
    let before = snapshot(&root);

    let pictures = s.home.path().join("Pictures");
    fs::create_dir(&pictures).unwrap();
    let original = pictures.join("my image.JPEG");
    fs::copy(fixture("white.jpg"), &original).unwrap();
    let bytes = fs::read(&original).unwrap();
    let modified = fs::metadata(&original).unwrap().modified().unwrap();
    let output = success(s.run(&["create"], "personal\ni\n1\ns\nn\n"));
    assert!(output.contains(&format!("Images in {}", pictures.display())));
    assert!(!output.contains("configure one first"));
    let mut after = snapshot(&root);
    assert!(after.remove(Path::new("profiles/personal.toml")).is_some());
    assert_eq!(
        after.remove(Path::new("profiles/personal.jpg")).unwrap(),
        bytes
    );
    assert_eq!(after, before);
    assert_eq!(fs::read(&original).unwrap(), bytes);
    assert_eq!(
        fs::metadata(&original).unwrap().modified().unwrap(),
        modified
    );

    // Simulate the user moving their original after Save: later apply must use the owned copy.
    fs::remove_file(&original).unwrap();
    let plan = s.plan("personal");
    assert_eq!(plan["profile"]["schema_version"], 2);
    assert_eq!(plan["source"]["id"], "personal-images");
    assert_eq!(plan["selection"]["candidate"], "personal.jpg");
    let list = success(s.run(&["list"], ""));
    assert!(list.contains("old") && list.contains("personal"));
    success(s.run(&["apply", "personal"], ""));
    let history = ghostty_wall::history::inspect_history(&root).unwrap();
    assert_eq!(
        history.latest().unwrap().profile_id().unwrap().as_str(),
        "personal"
    );
    let activation: Value = serde_json::from_slice(
        &fs::read(root.join("history/activations/act-v1-0000000000000002.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(activation["profile"]["schema_version"], 2);
    assert_eq!(
        activation["environment_id"],
        plan["environment"]["environment_id"]
    );
    success(s.run(&["previous"], ""));
    let replay: Value = serde_json::from_slice(
        &fs::read(root.join("history/activations/act-v1-0000000000000003.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        replay["environment_id"],
        old_plan["environment"]["environment_id"]
    );
    assert_eq!(replay["cause"]["kind"], "history-replay");
    assert_eq!(
        fs::read(root.join("config.toml")).unwrap(),
        config.as_bytes()
    );
    assert_eq!(
        fs::read(root.join("profiles/old.toml")).unwrap(),
        old.as_bytes()
    );
    assert!(!root.join("profiles/welcome.toml").exists());
}

#[test]
fn save_failure_after_image_publication_rolls_back_and_allows_a_different_image_on_retry() {
    let s = Sandbox::initialized();
    fs::write(
        s.root().join("profiles/old-random.toml"),
        "schema_version = 1\n[wallpaper]\nmode = \"source\"\nsource = \"welcome\"\nselection = \"random\"\n",
    )
    .unwrap();
    let plan_random = || -> Value {
        serde_json::from_str(&success(s.run(
            &["plan", "old-random", "--seed", &"01".repeat(32), "--json"],
            "",
        )))
        .unwrap()
    };
    let original_selection = plan_random()["selection"].clone();
    let before = snapshot(&s.root());
    let image = fixture("palette.png");
    assert!(fs::metadata(&image).unwrap().len() < 200);
    let mut command = s.command(&["create", "failed-save"]);
    // Fault injection at the public process boundary: the PNG fits, but the Profile TOML does not.
    limit_file_size(&mut command, 200);
    let failed = feed(command, &image_input(&image, false));
    assert_eq!(failed.status.code(), Some(6));
    let error = String::from_utf8_lossy(&failed.stderr);
    assert!(error.contains("was not saved"), "{error}");
    assert!(error.contains("pre-save files preserved"), "{error}");
    assert!(!error.contains("durability is uncertain"), "{error}");
    assert!(!String::from_utf8_lossy(&failed.stdout).contains("Use now"));
    assert!(!s.root().join("profiles/failed-save.toml").exists());
    let after_failure = snapshot(&s.root());
    let failed_selection = plan_random()["selection"].clone();
    let additions: Vec<_> = after_failure
        .keys()
        .filter(|path| !before.contains_key(*path))
        .collect();
    let retry = s.run(
        &["create", "failed-save"],
        &image_input(&fixture("white.png"), false),
    );
    assert!(
        after_failure == before,
        "A failed Save must not leave a partial managed Profile/image. Added {additions:?}; old random Profile's candidate_count {} -> {}; retry stderr: {}",
        original_selection["candidate_count"],
        failed_selection["candidate_count"],
        String::from_utf8_lossy(&retry.stderr)
    );
    assert_eq!(failed_selection, original_selection);
    success(retry);
}

#[test]
fn corrupt_history_use_now_keeps_saved_profile_but_does_not_change_committed_state() {
    let s = Sandbox::initialized();
    success(s.run(&["apply", "welcome"], ""));
    fs::write(
        s.root()
            .join("history/activations/act-v1-0000000000000002.json"),
        b"invalid existing history",
    )
    .unwrap();
    let before = snapshot(&s.root());
    let result = s.run(
        &["create", "saved-only"],
        &image_input(&fixture("palette.png"), true),
    );
    assert_eq!(result.status.code(), Some(6));
    assert!(String::from_utf8_lossy(&result.stdout).contains("Saved Profile saved-only."));
    assert!(!String::from_utf8_lossy(&result.stdout).contains("Activated act-"));
    assert!(String::from_utf8_lossy(&result.stderr).contains("Profile saved but apply failed"));
    let mut after = snapshot(&s.root());
    assert!(
        after
            .remove(Path::new("profiles/saved-only.toml"))
            .is_some()
    );
    assert!(after.remove(Path::new("profiles/saved-only.png")).is_some());
    assert_eq!(after, before);
}

#[test]
fn missing_init_unknown_config_and_missing_lock_fail_without_partial_files() {
    let s = Sandbox::empty();
    let output = s.run(&["create", "missing-init"], "");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("init"));
    assert_eq!(fs::read_dir(s.home.path()).unwrap().count(), 0);
    success(s.run(&["init"], ""));
    let config = s.root().join("config.toml");
    let original = fs::read(&config).unwrap();
    fs::write(&config, b"schema_version = 999\n[sources]\n").unwrap();
    let before = snapshot(&s.root());
    assert!(!s.run(&["create", "future"], "").status.success());
    assert_eq!(snapshot(&s.root()), before);
    fs::write(&config, original).unwrap();
    fs::remove_file(s.root().join("state.lock")).unwrap();
    let before = snapshot(&s.root());
    assert!(
        !s.run(
            &["create", "no-lock"],
            &image_input(&fixture("palette.png"), false)
        )
        .status
        .success()
    );
    assert_eq!(snapshot(&s.root()), before);
}

#[test]
fn id_boundaries_and_malformed_existing_profile_retry_without_overwrite() {
    let s = Sandbox::initialized();
    let existing = s.root().join("profiles/occupied.toml");
    fs::write(&existing, b"schema_version = 999\n").unwrap();
    let before = snapshot(&s.root());
    let valid = "a".repeat(64);
    let input = format!(
        "{}\n../outside\noccupied\n{valid}\n{}",
        "a".repeat(65),
        image_input(&fixture("palette.png"), false)
    );
    let output = success(s.run(&["create"], &input));
    assert_eq!(output.matches("Profile ID (or cancel)").count(), 4);
    assert_eq!(output.matches("Invalid or existing Profile ID:").count(), 3);
    let mut after = snapshot(&s.root());
    assert!(
        after
            .remove(&PathBuf::from(format!("profiles/{valid}.toml")))
            .is_some()
    );
    assert!(
        after
            .remove(&PathBuf::from(format!("profiles/{valid}.png")))
            .is_some()
    );
    assert_eq!(after, before);
}

#[test]
fn not_now_never_repairs_preexisting_projection_drift() {
    let s = Sandbox::initialized();
    fs::write(
        s.root().join("current.ghostty"),
        b"# pre-existing drift\nbackground = 123456\n",
    )
    .unwrap();
    let before = snapshot(&s.root());
    success(s.run(
        &["create", "declined"],
        &image_input(&fixture("palette.png"), false),
    ));
    let mut after = snapshot(&s.root());
    assert!(after.remove(Path::new("profiles/declined.toml")).is_some());
    assert!(after.remove(Path::new("profiles/declined.png")).is_some());
    assert_eq!(after, before);
}

#[test]
fn image_write_failure_preserves_active_state_and_allows_retry_with_another_format() {
    let s = Sandbox::initialized();
    success(s.run(&["apply", "welcome"], ""));
    for (id, input) in [
        ("import-failed", image_input(&fixture("palette.png"), true)),
        ("generation-failed", "g\ns\ny\n".to_owned()),
    ] {
        let before = snapshot(&s.root());
        let mut command = s.command(&["create", id]);
        limit_file_size(&mut command, 64);
        let failed = feed(command, &input);
        assert_eq!(failed.status.code(), Some(6));
        let output = String::from_utf8_lossy(&failed.stdout);
        assert!(!output.contains("Saved Profile"), "{output}");
        assert!(!output.contains("Use now"), "{output}");
        assert_eq!(snapshot(&s.root()), before);
        success(s.run(&["create", id], &image_input(&fixture("white.jpg"), false)));
        let mut after = snapshot(&s.root());
        assert!(
            after
                .remove(&PathBuf::from(format!("profiles/{id}.toml")))
                .is_some()
        );
        assert_eq!(
            after
                .remove(&PathBuf::from(format!("profiles/{id}.jpg")))
                .unwrap(),
            fs::read(fixture("white.jpg")).unwrap()
        );
        assert_eq!(after, before);
    }
}

#[test]
fn decoded_media_not_filename_controls_copy_and_invalid_images_retry_locally() {
    let s = Sandbox::initialized();
    let downloads = s.home.path().join("Downloads");
    fs::create_dir(&downloads).unwrap();
    let jpeg = fs::read(fixture("white.jpg")).unwrap();
    let selected = downloads.join("photo.png");
    fs::write(&selected, &jpeg).unwrap();
    fs::write(downloads.join("truncated.jpg"), &jpeg[..12]).unwrap();
    fs::write(downloads.join("unsupported.png"), b"GIF89a").unwrap();
    image::RgbImage::new(16_385, 1)
        .save(downloads.join("too-wide.png"))
        .unwrap();
    fs::File::create(downloads.join("too-large.png"))
        .unwrap()
        .set_len(32 * 1024 * 1024 + 1)
        .unwrap();
    let before_original = fs::metadata(&selected).unwrap().modified().unwrap();
    let before = snapshot(&s.root());
    let output = success(s.run(
        &["create", "decoded"],
        "i\ntruncated.jpg\nunsupported.png\ntoo-wide.png\ntoo-large.png\nphoto.png\ns\nn\n",
    ));
    assert_eq!(output.matches("Cannot use ").count(), 4, "{output}");
    assert_eq!(output.matches("Create Profile decoded.").count(), 1);
    assert!(output.contains("image cannot be decoded safely"));
    assert!(output.contains("image must be PNG or JPEG"));
    assert!(output.contains("not a bounded regular file"));
    assert_eq!(fs::read(&selected).unwrap(), jpeg);
    assert_eq!(
        fs::metadata(&selected).unwrap().modified().unwrap(),
        before_original
    );
    let mut after = snapshot(&s.root());
    assert!(after.remove(Path::new("profiles/decoded.toml")).is_some());
    assert_eq!(
        after.remove(Path::new("profiles/decoded.jpg")).unwrap(),
        jpeg
    );
    assert_eq!(after, before);
    let plan = s.plan("decoded");
    assert_eq!(plan["asset"]["media_type"], "image/jpeg");
    assert_eq!(plan["selection"]["candidate"], "decoded.jpg");
}

#[test]
#[cfg(target_os = "linux")]
fn reload_action_success_is_reported_only_after_commit_and_not_as_live_verification() {
    use std::os::unix::fs::PermissionsExt;

    let s = Sandbox::initialized();
    success(s.run(&["apply", "welcome"], ""));
    let prior_activation = fs::read(
        s.root()
            .join("history/activations/act-v1-0000000000000001.json"),
    )
    .unwrap();
    let bin = s.home.path().join("recording-adapter");
    fs::create_dir(&bin).unwrap();
    let adapter = bin.join("systemctl");
    fs::write(&adapter, "#!/bin/sh\ncase \"$2\" in\n is-active) exit 0;;\n reload)\n  [ -s \"$QA_ROOT/history/activations/act-v1-0000000000000002.json\" ] || exit 1\n  [ -s \"$QA_ROOT/profiles/accepted-action.toml\" ] || exit 1\n  [ -s \"$QA_ROOT/current.ghostty\" ] || exit 1\n  printf '%s\\n' \"$*\" >> \"$HOME/reload-calls\"\n  exit 0;;\n *) exit 1;;\nesac\n").unwrap();
    fs::set_permissions(&adapter, fs::Permissions::from_mode(0o700)).unwrap();
    let mut command = s.command(&["create", "accepted-action"]);
    command.env("PATH", bin).env("QA_ROOT", s.root());
    let output = success(feed(command, &image_input(&fixture("palette.png"), true)));
    assert!(output.contains("Ghostty reload: action accepted; visible change is not verified."));
    assert!(
        output.find("Saved Profile accepted-action").unwrap()
            < output.find("Activated act-v1-0000000000000002").unwrap()
    );
    assert_eq!(
        fs::read_to_string(s.home.path().join("reload-calls")).unwrap(),
        "--user reload app-com.mitchellh.ghostty.service\n"
    );
    assert_eq!(
        fs::read(
            s.root()
                .join("history/activations/act-v1-0000000000000001.json")
        )
        .unwrap(),
        prior_activation
    );
    let history = ghostty_wall::history::inspect_history(&s.root()).unwrap();
    assert_eq!(
        history.latest().unwrap().profile_id().unwrap().as_str(),
        "accepted-action"
    );
}

#[test]
fn colors_are_image_coordinated_complete_and_repeatably_resolvable() {
    let s = Sandbox::initialized();
    for (id, image) in [("white", "white.png"), ("colorful", "palette.png")] {
        success(s.run(&["create", id], &image_input(&fixture(image), false)));
    }
    let before = snapshot(&s.root());
    let white = s.plan("white");
    let colorful = s.plan("colorful");
    assert_ne!(
        white["environment"]["manifest"]["colors"],
        colorful["environment"]["manifest"]["colors"]
    );
    for plan in [&white, &colorful] {
        let colors = &plan["environment"]["manifest"]["colors"];
        assert_eq!(colors["palette"].as_array().unwrap().len(), 16);
        for key in [
            "background",
            "foreground",
            "cursor",
            "selection_background",
            "selection_foreground",
        ] {
            assert!(
                colors[key]
                    .as_str()
                    .unwrap()
                    .parse::<ghostty_wall::domain::Color>()
                    .is_ok()
            );
        }
        assert_eq!(
            plan["environment"]["manifest"]["wallpaper"]["opacity_millionths"],
            100_000
        );
        assert_eq!(plan["color_resolution"]["algorithm"], "kmeans-v3");
    }
    assert_eq!(colorful["environment"], s.plan("colorful")["environment"]);
    assert_eq!(snapshot(&s.root()), before);
}
