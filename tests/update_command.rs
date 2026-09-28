use sha2::{Digest, Sha256};
use std::{
    fs,
    net::TcpListener,
    path::Path,
    process::{Command, Output},
};
use tempfile::TempDir;

fn run(exe: &Path, home: &Path, args: &[&str]) -> Output {
    let closed_port = TcpListener::bind("127.0.0.1:0").unwrap();
    let proxy = format!("http://{}", closed_port.local_addr().unwrap());
    drop(closed_port);
    Command::new(exe)
        .args(args)
        .env_clear()
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_DATA_HOME", home.join("data"))
        .env("XDG_STATE_HOME", home.join("state"))
        .env("XDG_CACHE_HOME", home.join("cache"))
        .env("HTTPS_PROXY", &proxy)
        .env("https_proxy", &proxy)
        .env("ALL_PROXY", &proxy)
        .env("all_proxy", &proxy)
        .env("NO_PROXY", "")
        .env("no_proxy", "")
        .output()
        .unwrap()
}

#[test]
fn public_update_help_documents_routes_without_network_or_user_data() {
    let home = TempDir::new().unwrap();
    let output = run(
        Path::new(env!("CARGO_BIN_EXE_ghostty-wall")),
        home.path(),
        &["update", "--help"],
    );
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    for word in ["--check", "Cargo", "Linux", "macOS", "metadata", "rollback"] {
        assert!(text.contains(word), "{text}");
    }
    assert_eq!(fs::read_dir(home.path()).unwrap().count(), 0);
}

#[test]
fn public_update_network_failure_preserves_both_disposable_installation_routes() {
    for cargo in [false, true] {
        let dir = TempDir::new().unwrap();
        let home = dir.path().join("home");
        let prefix = dir.path().join("unusual-prefix");
        let bin = prefix.join("bin");
        fs::create_dir_all(&bin).unwrap();
        fs::create_dir_all(&home).unwrap();
        let exe = bin.join("ghostty-wall");
        let binary = fs::read(env!("CARGO_BIN_EXE_ghostty-wall")).unwrap();
        fs::copy(env!("CARGO_BIN_EXE_ghostty-wall"), &exe).unwrap();
        let metadata = if cargo {
            let key = format!(
                "ghostty-wall {} (path+file:///disposable/source)",
                env!("CARGO_PKG_VERSION")
            );
            vec![
                (
                    prefix.join(".crates.toml"),
                    format!("[v1]\n{key:?} = [\"ghostty-wall\"]\n").into_bytes(),
                ),
                (
                    prefix.join(".crates2.json"),
                    serde_json::to_vec(
                        &serde_json::json!({"installs": {key: {"bins": ["ghostty-wall"]}}}),
                    )
                    .unwrap(),
                ),
            ]
        } else {
            let digest = Sha256::digest(&binary)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>();
            vec![(
                bin.join(".ghostty-wall-release.sha256"),
                format!("{digest}  ghostty-wall\n").into_bytes(),
            )]
        };
        for (path, bytes) in &metadata {
            fs::write(path, bytes).unwrap();
        }
        let managed = home.join("config/ghostty/ghostty-wall");
        for path in [
            "profiles/kept.toml",
            "config.toml",
            "history/kept",
            "environments/kept",
            "assets/kept",
            "current.ghostty",
        ] {
            let path = managed.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, b"unchanged fixture").unwrap();
        }
        fs::write(home.join("config/ghostty/config"), b"untouched integration").unwrap();
        for args in [vec!["update"], vec!["update", "--check"]] {
            let output = run(&exe, &home, &args);
            assert_eq!(
                output.status.code(),
                Some(6),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(String::from_utf8_lossy(&output.stdout).contains("Checking"));
            assert!(
                String::from_utf8_lossy(&output.stderr).contains("GitHub release request failed")
            );
            assert_eq!(fs::read(&exe).unwrap(), binary);
            for (path, bytes) in &metadata {
                assert_eq!(&fs::read(path).unwrap(), bytes);
            }
            for path in [
                "profiles/kept.toml",
                "config.toml",
                "history/kept",
                "environments/kept",
                "assets/kept",
                "current.ghostty",
            ] {
                assert_eq!(fs::read(managed.join(path)).unwrap(), b"unchanged fixture");
            }
            assert_eq!(
                fs::read(home.join("config/ghostty/config")).unwrap(),
                b"untouched integration"
            );
        }
        let version = run(&exe, &home, &["--version"]);
        assert!(version.status.success());
        assert_eq!(
            String::from_utf8(version.stdout).unwrap().trim(),
            format!("ghostty-wall {}", env!("CARGO_PKG_VERSION"))
        );
    }
}
