<img width="720" height="720" alt="ghostty-wall" src="https://github.com/user-attachments/assets/ec3d60f0-bf60-49fb-a417-34c87f14adf5" />


# ghostty-wall

A tiny CLI that sets a random wallpaper for [Ghostty](https://github.com/ghostty-org/ghostty) by picking an image from a list of GitHub repositories.
It runs on macOS and Linux, and auto-wires your Ghostty config on first run.

> **Scope:** Ghostty only. The script is tailored to Ghostty's config and does not target other terminals.

---

## ✨ Features

* Randomly pick an image from one of several GitHub repos (you control the list)
* Writes a small include file (`$XDG_CONFIG_HOME/ghostty/wallpaper.conf`, default `~/.config/ghostty/wallpaper.conf`) and ensures it’s included in Ghostty’s main config
* Tries to reload Ghostty automatically if it's running on macOS
* Minimal dependencies (Bash 3.2, curl)
* GitHub token support to avoid API rate limits

---

## 📦 What gets installed & where

Running the installer will:

1. **Install the CLI** `ghostty-wall` into a global binary directory:

   * `/opt/homebrew/bin` (Apple Silicon Homebrew), or
   * `/usr/local/bin` (Intel/Homebrew), or
   * `~/.local/bin` (fallback if neither Homebrew path exists)

2. **Ensure your Ghostty config directory** exists at:

   * `${XDG_CONFIG_HOME:-$HOME/.config}/ghostty`

3. **Create default config files** if they don’t exist:

    * `~/.config/ghostty/wallpaper_repos.txt` — your list of repos to pull images from

4. **Run `ghostty-wall` once** so that, when GitHub access and repo configuration are valid:

    * a wallpaper gets downloaded to `${TMPDIR:-/tmp}/anime_wallpapers/current_wallpaper.<ext>`
    * `~/.config/ghostty/wallpaper.conf` is created
    * `$XDG_CONFIG_HOME/ghostty/config` gains an include line (if missing):
      `config-file = /absolute/path/to/ghostty/wallpaper.conf`

> Temporary images live in `${GHOSTTY_WALL_TEMP_DIR:-${TMPDIR:-/tmp}/anime_wallpapers}`.

---

## ✅ Requirements

* macOS or Linux
* [Ghostty](https://github.com/ghostty-org/ghostty) installed
* `curl` available
* Optional: `GITHUB_TOKEN` to raise rate limits for GitHub API

---

## 🚀 Quick Start

```bash
# from the repo root
./scripts/install.sh

# then simply:
ghostty-wall
```

This will:

* Create/prepare your Ghostty config
* Download a random image
* Attempt to reload Ghostty automatically if it's open on macOS
* You’ll see log messages in your terminal

## 🏷️ GitHub Releases

For now, releases are source-only.

1. Open the latest release on GitHub and download the source archive (`.zip` or `.tar.gz`).
2. Extract it locally.
3. Run from the extracted project root:

```bash
./scripts/install.sh
```

Notes:

* Release assets do not currently include prebuilt binaries.
* The normal CI workflow currently runs on Ubuntu only.
* macOS support is still validated manually before releases.
* Ongoing release history lives in [`CHANGELOG.md`](./CHANGELOG.md).

---

## 🧩 Repository Layout

```
ghostty-wall/
├─ .github/
│  └─ workflows/
│     ├─ ci.yml
│     └─ integration.yml
├─ bin/
│  └─ ghostty-wall
├─ docs/
│  ├─ release-checklist.md
│  └─ release-notes-template.md
├─ scripts/
│  ├─ install.sh
│  ├─ integration-test.sh
│  ├─ uninstall.sh
│  ├─ test.sh
│  ├─ linux/
│  │  └─ install-linux.sh
│  └─ mac/
│     └─ install-mac.sh
├─ examples/
│  └─ wallpaper_repos.example.txt
├─ CHANGELOG.md
├─ README.md
├─ LICENSE
└─ .gitignore
```

---

## 🛠️ Installation (details)

```bash
./scripts/install.sh
```

The installer:

* Detects an install prefix:
  * macOS: `/opt/homebrew`, `/usr/local`, or `~/.local`
  * Linux: `/usr/local` or `~/.local`
* Installs `bin/ghostty-wall` into `<prefix>/bin/ghostty-wall`
* Creates `$XDG_CONFIG_HOME/ghostty/` if necessary (default `~/.config/ghostty/`)
* Seeds `$XDG_CONFIG_HOME/ghostty/wallpaper_repos.txt` from `examples/wallpaper_repos.example.txt` if missing
* Executes `ghostty-wall` once to generate and wire `wallpaper.conf`
* Prints a warning instead of pretending success if the first wallpaper fetch fails

**PATH note:**
If the installer falls back to `~/.local/bin`, it will append that directory to your shell profile. Open a new terminal or `source` your profile to pick it up.

---

## 🧪 Usage

```bash
ghostty-wall                 # one-off: pick repo+image and reload Ghostty on macOS
ghostty-wall --list          # show the repo list file
ghostty-wall --add "name|owner/repo|branch|path"
ghostty-wall --remove <name>
ghostty-wall --help
```

## ✅ Confidence Ladder

1. Run `bash scripts/test.sh` for deterministic local and CI coverage.
2. Run `bash scripts/integration-test.sh` to verify the live GitHub API and image download path.
3. Trigger the `Integration` workflow in GitHub Actions when you want the live integration test on a clean runner.
4. Follow `docs/release-checklist.md` before a release to cover Ubuntu CI, manual macOS validation, and real Ghostty behavior.

**Examples**

Add a repository:

```bash
ghostty-wall --add "k1ngwalls|k1ng440/Wallpapers|master|wallpapers"
```

Remove a repository by name:

```bash
ghostty-wall --remove k1ngwalls
```

List current repositories:

```bash
ghostty-wall --list
```
---

## ⚙️ Configuration

### Ghostty include (auto-managed)

* File: `$XDG_CONFIG_HOME/ghostty/wallpaper.conf` (default `~/.config/ghostty/wallpaper.conf`)
  Example content (auto-written):

  ```ini
  background-image=${TMPDIR:-/tmp}/anime_wallpapers/current_wallpaper.jpg
  background-image-fit=cover
  background-image-position=center
  background-image-opacity=0.1
  ```
* The tool ensures `$XDG_CONFIG_HOME/ghostty/config` contains:
  `config-file = /absolute/path/to/ghostty/wallpaper.conf`
  (If the line is missing, it will append it.)

### Repo list format

* File: `$XDG_CONFIG_HOME/ghostty/wallpaper_repos.txt` (default `~/.config/ghostty/wallpaper_repos.txt`)
* Format: one repo per line as `name|owner/repo|branch|path`

  * `name`: an arbitrary label you choose
  * `owner/repo`: the GitHub repository
  * `branch`: e.g., `main` or `master` (if empty in the file, the script defaults to `main`)
  * `path`: subfolder under the repo that contains images (optional; can be empty)

**Example:**

```txt
# name|owner/repo|branch|path
anime|ThePrimeagen/anime|master|
k1ngwalls|k1ng440/Wallpapers|master|wallpapers
```

**Supported image extensions:** `.jpg`, `.jpeg`, `.png`, `.gif`, `.webp` (case-insensitive)

---

## 🌍 Environment Variables

* `GITHUB_TOKEN` — optional; if set, the script adds an Authorization header to avoid GitHub API rate limits

* `GHOSTTY_WALL_TEMP_DIR` — optional; overrides where downloaded images are stored

* `TMPDIR` — used as the base temporary directory when `GHOSTTY_WALL_TEMP_DIR` is not set

  ```bash
  export GITHUB_TOKEN=ghp_XXXXXXXXXXXXXXXXXXXXXXXXXXXX
  ```

---

## 🧰 How it works (under the hood)

1. Reads (or creates) `$XDG_CONFIG_HOME/ghostty/wallpaper_repos.txt`
2. Picks a random repo line and constructs:

   * a GitHub **API URL** to list files in the specified repo/path/branch
3. Filters the file list for image extensions
4. Randomly selects one image and downloads it to:

   * `${GHOSTTY_WALL_TEMP_DIR:-${TMPDIR:-/tmp}/anime_wallpapers}/current_wallpaper.<ext>`
5. Writes/updates `$XDG_CONFIG_HOME/ghostty/wallpaper.conf` to point to that file
6. Ensures `$XDG_CONFIG_HOME/ghostty/config` includes the `wallpaper.conf`
7. If Ghostty is running on macOS, attempts a reload via AppleScript (Cmd+Shift+,)

---

## 🧹 Uninstall

Remove the CLI (config files are left in place):

```bash
./scripts/uninstall.sh
```

(Optional) Clean temporary wallpapers and the include:

```bash
rm -rf "${GHOSTTY_WALL_TEMP_DIR:-${TMPDIR:-/tmp}/anime_wallpapers}"
rm -f "${XDG_CONFIG_HOME:-$HOME/.config}/ghostty/wallpaper.conf"
# (and remove the include line from "${XDG_CONFIG_HOME:-$HOME/.config}/ghostty/config" if you want)
```

---

## 🛡️ Security & Privacy

* No telemetry.
* Uses GitHub’s public API to list files and raw.githubusercontent.com to download images.
* If `GITHUB_TOKEN` is provided, it is used only for GitHub API authentication headers.

---

## 🪪 Compatibility & Limitations

* Ghostty only
* Online sources only (GitHub repos); local folders are not supported at this time
* Automatic Ghostty reload is macOS-only; Linux applies the new wallpaper on the next config reload or launch
* If your Ghostty config is in a non-standard location, ensure `$XDG_CONFIG_HOME/ghostty/config` exists or symlink it

---

## ❓ FAQ

**Q: “Command not found” after install?**
A: If the installer used `~/.local/bin`, open a new terminal or source the profile the installer updated. On macOS that is typically `~/.zshrc` or `~/.bash_profile`; on Linux it is typically `~/.bashrc` or `~/.profile`.

**Q: Ghostty didn’t reload.**
A: Make sure Ghostty is running and macOS accessibility permissions allow automation (System Settings → Privacy & Security → Automation/Accessibility). The tool will still apply the wallpaper on Ghostty’s next launch.

**Q: GitHub API rate-limited me.**
A: Export a `GITHUB_TOKEN` (a classic Personal Access Token is enough for public repos).

**Q: My repo shows “No images found”.**
A: Double-check `branch` and `path`. The `path` should be relative to the repo root; leave it blank to use the root. If one repo is misconfigured, `ghostty-wall` will try the next configured repo automatically.

**Q: Install finished with a warning about initial wallpaper setup.**
A: The CLI was installed, but the first wallpaper fetch failed. Check network access, `curl`, and your repo list, then run `ghostty-wall` manually.

**Q: Can I control opacity/fit/position?**
A: Edit `${XDG_CONFIG_HOME:-$HOME/.config}/ghostty/wallpaper.conf` to tweak:

```ini
background-image-fit=cover
background-image-position=center
background-image-opacity=0.1
```

---

## 🤝 Contributing

Issues and PRs are welcome! Ideas:

* Local folder support
* More image providers
* Configurable filters (file size, resolution)
* Homebrew formula/tap

---

## 📜 License

MIT — see [`LICENSE`](./LICENSE).

---

## 🧾 Changelog

See [`CHANGELOG.md`](./CHANGELOG.md) for project history and [`docs/release-notes-template.md`](./docs/release-notes-template.md) for the GitHub Release notes template.

---

## 🧭 Appendix: Example session

```bash
# 1) Install
./scripts/install.sh

# 2) List current repos
ghostty-wall --list

# 3) Add your favorite repo/path
ghostty-wall --add "mycats|myuser/mycats|main|images/wallpapers"

# 4) Rotate now
ghostty-wall
```
