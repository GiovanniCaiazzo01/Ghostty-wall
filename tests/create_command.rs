#![cfg(unix)]

use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    os::unix::{
        fs::{MetadataExt, PermissionsExt},
        process::CommandExt,
    },
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};

use ghostty_wall::{domain::Sha256Digest, init::InitPaths};
use serde_json::Value;
use sha2::{Digest, Sha256};

struct Sandbox {
    home: tempfile::TempDir,
    paths: InitPaths,
}
impl Sandbox {
    fn new() -> Self {
        let home = tempfile::tempdir().unwrap();
        let paths = InitPaths {
            home: home.path().to_owned(),
            xdg_config_home: Some(home.path().join("config")),
        };
        let sandbox = Self { home, paths };
        success(sandbox.run(&["init"], ""));
        sandbox
    }
    fn root(&self) -> PathBuf {
        self.paths.managed_root()
    }
    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_ghostty-wall"));
        // HOME isolation alone does not isolate a running Ghostty or the user bus.
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
    fn import_input(&self, ending: &str) -> String {
        format!("i\npath:{}\ns\n{ending}", fixture("white.png").display())
    }
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}
fn feed(mut command: Command, input: &str) -> Output {
    let mut child = command.spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}
fn success(output: Output) -> String {
    assert!(
        output.status.success(),
        "{:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    String::from_utf8(output.stdout).unwrap()
}
fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn visit(root: &Path, path: &Path, result: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                visit(root, &entry.path(), result);
            } else {
                result.insert(
                    entry.path().strip_prefix(root).unwrap().to_owned(),
                    fs::read(entry.path()).unwrap(),
                );
            }
        }
    }
    let mut result = BTreeMap::new();
    visit(root, root, &mut result);
    result
}
fn assert_readable(plan: &Value) {
    fn luminance(hex: &str) -> f64 {
        let rgb = hex.parse::<ghostty_wall::domain::Color>().unwrap().as_rgb();
        let channel = |n| {
            let n = f64::from(n) / 255.0;
            if n <= 0.04045 {
                n / 12.92
            } else {
                ((n + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(rgb[0]) + 0.7152 * channel(rgb[1]) + 0.0722 * channel(rgb[2])
    }
    let contrast = |a: &Value, b: &Value| {
        let a = luminance(a.as_str().unwrap());
        let b = luminance(b.as_str().unwrap());
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    };
    let colors = &plan["environment"]["manifest"]["colors"];
    assert_eq!(plan["color_resolution"]["algorithm"], "kmeans-v3");
    assert_eq!(
        plan["environment"]["manifest"]["wallpaper"]["mode"],
        "image"
    );
    assert!(contrast(&colors["foreground"], &colors["background"]) >= 4.5);
    assert!(contrast(&colors["cursor"], &colors["background"]) >= 4.5);
    assert!(
        contrast(
            &colors["selection_foreground"],
            &colors["selection_background"]
        ) >= 4.5
    );
    let palette = colors["palette"].as_array().unwrap();
    assert_eq!(palette.len(), 16);
    assert!(
        palette
            .iter()
            .all(|color| contrast(color, &colors["background"]) >= 4.5)
    );
}

#[test]
fn missing_id_retries_locally_and_saves_a_complete_profile_without_activation() {
    let s = Sandbox::new();
    success(s.run(&["apply", "welcome"], ""));
    let before = snapshot(&s.root());
    let input = format!(
        "Bad ID\nwelcome\nmine\ninvalid-choice\n{}",
        s.import_input("n\n")
    );
    let output = success(s.run(&["create"], &input));
    assert_eq!(output.matches("Profile ID (or cancel)").count(), 3);
    assert_eq!(output.matches("Invalid or existing Profile ID:").count(), 2);
    assert!(output.contains("lowercase"));
    assert!(output.contains("collision"));
    assert_eq!(output.matches("Create Profile mine.").count(), 1);
    assert!(output.contains("[y] Use now / [n] Not now"));
    assert!(output.contains("terminal unchanged"));
    assert!(!output.contains("Usage:"));
    let mut after = snapshot(&s.root());
    assert!(after.remove(Path::new("profiles/mine.toml")).is_some());
    assert_eq!(
        after.remove(Path::new("profiles/mine.png")).unwrap(),
        fs::read(fixture("white.png")).unwrap()
    );
    assert_eq!(after, before);
    assert_readable(&s.plan("mine"));
    let config = fs::read_to_string(s.root().join("profiles/mine.toml")).unwrap();
    assert!(config.contains("schema_version = 2"));
    assert!(!config.contains("wallpaper.generation"));
    for name in ["mine.toml", "mine.png"] {
        assert_eq!(
            fs::metadata(s.root().join("profiles").join(name))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}

#[test]
fn supplied_id_skips_prompt_and_rejects_invalid_or_duplicate_without_overwrite() {
    let s = Sandbox::new();
    let help = success(s.run(&["create", "--help"], ""));
    for text in [
        "create [PROFILE]",
        "cancel",
        "path:/absolute/path",
        "Not now",
        "does not provide live",
        "Uncertain publication or incomplete rollback requires inspection",
    ] {
        assert!(help.contains(text), "{help}");
    }
    assert!(success(s.run(&["--help"], "")).contains("ghostty-wall create [PROFILE]"));
    for (id, code) in [("Bad-ID", 2), ("welcome", 3)] {
        let before = snapshot(&s.root());
        let output = s.run(&["create", id], "");
        assert_eq!(output.status.code(), Some(code));
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(!error.contains("Usage:"), "{error}");
        assert!(!String::from_utf8_lossy(&output.stdout).contains("Profile ID"));
        assert_eq!(snapshot(&s.root()), before);
    }
    let output = success(s.run(&["create", "boy"], &s.import_input("not now\n")));
    assert!(!output.contains("Profile ID (or cancel)"));
    assert!(s.root().join("profiles/boy.toml").exists());
}

#[test]
fn generated_wallpaper_changes_only_on_explicit_another_before_save_and_remains_stable() {
    let s = Sandbox::new();
    let output = success(s.run(&["create", "generated"], "g\ninvalid\na\ns\na\nn\n"));
    let seeds: Vec<&str> = output
        .split("Generated variant ")
        .skip(1)
        .map(|text| text.split_whitespace().next().unwrap())
        .collect();
    assert_eq!(
        seeds.len(),
        2,
        "invalid choices and post-save actions must not regenerate"
    );
    assert_ne!(seeds[0], seeds[1]);
    assert!(!output.contains("Profile ID (or cancel)"));
    let intent_path = s.root().join("profiles/generated.toml");
    let intent = fs::read(&intent_path).unwrap();
    let profile: toml::Value = toml::from_str(std::str::from_utf8(&intent).unwrap()).unwrap();
    let wallpaper = &profile["wallpaper"];
    assert_eq!(wallpaper["selection"].as_str(), Some("path"));
    assert_eq!(
        wallpaper["generation"]["algorithm"].as_str(),
        Some("gradient-v1")
    );
    assert_eq!(wallpaper["generation"]["seed"].as_str(), Some(seeds[1]));
    let image_path = s.root().join("profiles/generated.png");
    let bytes = fs::read(&image_path).unwrap();
    let digest = Sha256Digest::from_bytes(Sha256::digest(&bytes).into()).to_string();
    assert_eq!(wallpaper["owned_sha256"].as_str(), Some(digest.as_str()));
    let image = image::load_from_memory(&bytes).unwrap().to_rgba8();
    assert_eq!(image.dimensions(), (256, 256));
    let seed: Sha256Digest = seeds[1].parse().unwrap();
    for (x, y) in [(0, 0), (23, 69), (255, 255)] {
        assert_eq!(
            image.get_pixel(x, y).0,
            [
                seed.as_bytes()[0].wrapping_add(x as u8),
                seed.as_bytes()[1].wrapping_add(y as u8),
                seed.as_bytes()[2].wrapping_add(((x + y) / 2) as u8),
                255,
            ]
        );
    }
    let first = s.plan("generated");
    assert_readable(&first);
    success(s.run(&["apply", "generated"], ""));
    let second = s.plan("generated");
    assert_eq!(first["environment"], second["environment"]);
    assert_eq!(fs::read(&image_path).unwrap(), bytes);
    assert_eq!(fs::read(&intent_path).unwrap(), intent);
}

#[test]
fn cancel_and_eof_at_each_pre_save_step_leave_no_profile_or_image() {
    let s = Sandbox::new();
    let before = snapshot(&s.root());
    for input in [
        "",
        "cancel\n",
        "draft\ncancel\n",
        "draft\ni\ncancel\n",
        "draft\ni\n",
    ] {
        assert!(success(s.run(&["create"], input)).contains("Cancelled; no Profile saved"));
        assert_eq!(snapshot(&s.root()), before);
    }
    let selected = format!("i\npath:{}\n", fixture("white.png").display());
    for input in [
        selected.clone(),
        format!("{selected}cancel\n"),
        "g\ncancel\n".into(),
    ] {
        assert!(
            success(s.run(&["create", "draft"], &input)).contains("Cancelled; no Profile saved")
        );
        assert_eq!(snapshot(&s.root()), before);
    }
}

#[test]
fn not_now_blank_cancel_or_eof_after_save_keep_profile_without_changing_terminal() {
    let s = Sandbox::new();
    success(s.run(&["apply", "welcome"], ""));
    for (i, ending) in ["n\n", "\n", "cancel\n", ""].iter().enumerate() {
        let before = snapshot(&s.root());
        let id = format!("saved-{i}");
        let output = success(s.run(&["create", &id], &s.import_input(ending)));
        assert!(output.contains("terminal unchanged"));
        let mut after = snapshot(&s.root());
        assert!(
            after
                .remove(&PathBuf::from(format!("profiles/{id}.toml")))
                .is_some()
        );
        assert!(
            after
                .remove(&PathBuf::from(format!("profiles/{id}.png")))
                .is_some()
        );
        assert_eq!(after, before);
    }
}

#[test]
fn image_picker_uses_localized_roots_navigation_search_filtering_and_decode_validation() {
    let s = Sandbox::new();
    let downloads = s.home.path().join("Scaricati personali");
    let pictures = s.home.path().join("Immagini");
    fs::create_dir_all(downloads.join("folder")).unwrap();
    fs::create_dir(&pictures).unwrap();
    fs::write(s.paths.xdg_config_home.as_ref().unwrap().join("user-dirs.dirs"),
        "XDG_DOWNLOAD_DIR=\"$HOME/Scaricati personali\" # localized\nXDG_PICTURES_DIR=\"$HOME/Immagini\"\n").unwrap();
    fs::write(downloads.join("not-supported.txt"), b"not an image").unwrap();
    fs::write(
        downloads.join("broken.png"),
        &fs::read(fixture("white.png")).unwrap()[..8],
    )
    .unwrap();
    let original = pictures.join("Sky.JPEG");
    fs::copy(fixture("white.jpg"), &original).unwrap();
    fs::set_permissions(&original, fs::Permissions::from_mode(0o444)).unwrap();
    let original_bytes = fs::read(&original).unwrap();
    let modified = fs::metadata(&original).unwrap().modified().unwrap();
    let output = success(s.run(
        &["create", "personal"],
        "i\nfolder\n..\n/BROKEN\n1\np\nd\np\n/SKY\n1\ns\nn\n",
    ));
    assert!(output.contains(&format!("Images in {}", downloads.display())));
    assert!(output.contains(&format!("Images in {}", downloads.join("folder").display())));
    assert!(output.contains(&format!("Images in {}", pictures.display())));
    assert!(!output.contains("not-supported.txt"));
    assert!(output.contains("image cannot be decoded safely"));
    assert_eq!(output.matches("Create Profile personal.").count(), 1);
    assert_eq!(fs::read(&original).unwrap(), original_bytes);
    assert_eq!(
        fs::metadata(&original).unwrap().modified().unwrap(),
        modified
    );
    assert_eq!(
        fs::metadata(&original).unwrap().permissions().mode() & 0o777,
        0o444
    );
    assert_eq!(
        fs::read(s.root().join("profiles/personal.jpg")).unwrap(),
        original_bytes
    );
    let plan = s.plan("personal");
    assert_eq!(plan["asset"]["media_type"], "image/jpeg");
    assert_readable(&plan);
}

#[test]
fn use_now_commits_latest_activation_and_reports_reload_unavailability_separately() {
    let s = Sandbox::new();
    success(s.run(&["apply", "welcome"], ""));
    let prior = fs::read(s.root().join("current.ghostty")).unwrap();
    let output = success(s.run(&["create", "active"], &s.import_input("use now\n")));
    assert!(output.contains("Saved Profile active. No Activation yet."));
    assert!(output.contains("Activated act-v1-0000000000000002 for Profile active"));
    assert!(output.contains("reload: unavailable; Activation remains committed"));
    let history = ghostty_wall::history::inspect_history(&s.root()).unwrap();
    assert_eq!(
        history.latest().unwrap().profile_id().unwrap().as_str(),
        "active"
    );
    assert_ne!(fs::read(s.root().join("current.ghostty")).unwrap(), prior);
    assert_readable(&s.plan("active"));
}

#[test]
fn use_now_reload_failure_is_success_but_apply_failure_keeps_saved_profile() {
    let s = Sandbox::new();
    let bin = s.home.path().join("fake-bin");
    fs::create_dir(&bin).unwrap();
    let systemctl = bin.join("systemctl");
    fs::write(&systemctl, "#!/bin/sh\ncase \"$2\" in\n is-active) exit 0;;\n reload) printf '%s\\n' \"$*\" >> \"$HOME/reload-calls\"; exit 1;;\n *) exit 1;;\nesac\n").unwrap();
    fs::set_permissions(&systemctl, fs::Permissions::from_mode(0o700)).unwrap();
    let mut decline = s.command(&["create", "not-now"]);
    decline.env("PATH", &bin);
    success(feed(decline, &s.import_input("n\n")));
    assert!(!s.home.path().join("reload-calls").exists());
    assert!(snapshot(&s.root().join("history")).is_empty());
    let mut command = s.command(&["create", "reload-failed"]);
    command.env("PATH", &bin);
    let output = success(feed(command, &s.import_input("y\n")));
    assert!(output.contains("reload: failed; Activation remains committed"));
    assert!(s.home.path().join("reload-calls").exists());
    let projection = fs::read(s.root().join("current.ghostty")).unwrap();
    let history = snapshot(&s.root().join("history"));
    let root_config = ghostty_wall::init::dry_run(&s.paths).unwrap().root_config;
    fs::write(root_config, "font-size = 13\n").unwrap();
    let output = s.run(&["create", "apply-failed"], &s.import_input("y\n"));
    assert_eq!(output.status.code(), Some(6));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Profile saved but apply failed"));
    assert!(s.root().join("profiles/apply-failed.toml").exists());
    assert!(s.root().join("profiles/apply-failed.png").exists());
    assert_eq!(
        fs::read(s.root().join("current.ghostty")).unwrap(),
        projection
    );
    assert_eq!(snapshot(&s.root().join("history")), history);
}

#[test]
fn failed_save_preserves_reused_image_and_allows_same_image_retry() {
    let s = Sandbox::new();
    let selected = fixture("palette.png");
    let image = s.root().join("profiles/reused.png");
    fs::copy(&selected, &image).unwrap();
    fs::set_permissions(&image, fs::Permissions::from_mode(0o444)).unwrap();
    let metadata = fs::metadata(&image).unwrap();
    let before = snapshot(&s.root());
    let input = format!("i\npath:{}\ns\nn\n", selected.display());
    let mut command = s.command(&["create", "reused"]);
    // SAFETY: only this child gets the limit; these libc calls are async-signal-safe before exec.
    unsafe {
        command.pre_exec(|| {
            // Reuse writes no image; fail even the shorter, ownership-free Profile document.
            let limit = libc::rlimit {
                rlim_cur: 1,
                rlim_max: 1,
            };
            if libc::setrlimit(libc::RLIMIT_FSIZE, &limit) != 0
                || libc::signal(libc::SIGXFSZ, libc::SIG_IGN) == libc::SIG_ERR
            {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let failed = feed(command, &input);
    assert_eq!(failed.status.code(), Some(6));
    let error = String::from_utf8_lossy(&failed.stderr);
    assert!(error.contains("was not saved"), "{error}");
    assert!(error.contains("pre-save files preserved"), "{error}");
    assert!(!error.contains("durability is uncertain"), "{error}");
    assert!(!String::from_utf8_lossy(&failed.stdout).contains("Use now"));
    assert_eq!(snapshot(&s.root()), before);
    let after = fs::metadata(&image).unwrap();
    assert_eq!(after.ino(), metadata.ino());
    assert_eq!(after.mode(), metadata.mode());
    assert_eq!(after.modified().unwrap(), metadata.modified().unwrap());
    success(s.run(&["create", "reused"], &input));
    assert!(s.root().join("profiles/reused.toml").is_file());
    assert_eq!(fs::metadata(&image).unwrap().ino(), metadata.ino());
}

#[test]
fn image_collision_and_unsafe_selected_files_fail_without_partial_profile() {
    let s = Sandbox::new();
    fs::write(
        s.root().join("profiles/collision.png"),
        b"existing owned data",
    )
    .unwrap();
    let before = snapshot(&s.root());
    let output = s.run(&["create", "collision"], &s.import_input(""));
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("collision"));
    assert_eq!(snapshot(&s.root()), before);
    let link = s.home.path().join("link.png");
    std::os::unix::fs::symlink(fixture("white.png"), &link).unwrap();
    let fifo = s.home.path().join("pipe.png");
    let cpath = std::ffi::CString::new(fifo.as_os_str().as_encoded_bytes()).unwrap();
    // SAFETY: cpath is a live, NUL-terminated path in the isolated test directory.
    assert_eq!(unsafe { libc::mkfifo(cpath.as_ptr(), 0o600) }, 0);
    let output = success(s.run(
        &["create", "unsafe"],
        &format!(
            "i\npath:{}\npath:{}\ncancel\n",
            link.display(),
            fifo.display()
        ),
    ));
    assert_eq!(output.matches("Cannot use ").count(), 2);
    assert!(output.contains("not a bounded regular file"));
    assert_eq!(snapshot(&s.root()), before);
}
