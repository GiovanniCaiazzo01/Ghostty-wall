<img width="720" height="720" alt="ghostty-wall" src="https://github.com/user-attachments/assets/ec3d60f0-bf60-49fb-a417-34c87f14adf5" />


# ghostty-wall

A tiny macOS-only tool that sets a random wallpaper for [Ghostty](https://github.com/ghostty-org/ghostty) by picking an image from a list of GitHub repositories.
It can run once on demand or loop in the background as a daemon, and it auto-wires your Ghostty config on first run.

> **Scope:** Ghostty only. The script is tailored to Ghostty’s config and reload behavior and does not target other terminals.

---

## ✨ Features

* Randomly pick an image from one of several GitHub repos (you control the list)
* Writes a small include file (`~/.config/ghostty/wallpaper.conf`) and ensures it’s included in Ghostty’s main config
* Tries to reload Ghostty automatically if it’s running (macOS AppleScript)
* Daemon mode to rotate wallpapers periodically
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

   * `~/.config/ghostty`

3. **Create default config files** if they don’t exist:

   * `~/.config/ghostty/wallpaper_repos.txt` — your list of repos to pull images from
   * `~/.config/ghostty/wallpaper.conf` — the small Ghostty include with the selected image

4. **Run `ghostty-wall` once** so that:

   * a wallpaper gets downloaded to `/tmp/anime_wallpapers/current_wallpaper.<ext>`
   * `~/.config/ghostty/config` gains an include line (if missing):
     `config-file = ~/.config/ghostty/wallpaper.conf`

> Temporary images live in: `/tmp/anime_wallpapers/` (cleared by macOS on reboot).

---

## ✅ Requirements

* macOS (tested with Bash 3.2)
* [Ghostty](https://github.com/ghostty-org/ghostty) installed
* `curl` available (preinstalled on macOS)
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
* Attempt to reload Ghostty automatically if it’s open
* You’ll see log messages in your terminal

---

## 🧩 Repository Layout

```
ghostty-wall/
├─ bin/
│  └─ ghostty-wall
├─ scripts/
│  ├─ install.sh
│  └─ uninstall.sh
├─ examples/
│  └─ wallpaper_repos.example.txt
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

* Detects an install prefix (`/opt/homebrew`, `/usr/local`, or `~/.local`)
* Installs `bin/ghostty-wall` into `<prefix>/bin/ghostty-wall`
* Creates `~/.config/ghostty/` if necessary
* Seeds `~/.config/ghostty/wallpaper_repos.txt` from `examples/wallpaper_repos.example.txt` if missing
* Executes `ghostty-wall` once to generate and wire `wallpaper.conf`

**PATH note:**
If the installer falls back to `~/.local/bin`, it will append that directory to your shell profile (`~/.zshrc` or `~/.bash_profile`). Open a new terminal or `source` your profile to pick it up.

---

## 🧪 Usage

```bash
ghostty-wall                 # one-off: pick repo+image and reload Ghostty if running
ghostty-wall --daemon        # infinite loop (uses INTERVAL_SEC, default 3600s)
ghostty-wall --list          # show the repo list file
ghostty-wall --add "name|owner/repo|branch|path"
ghostty-wall --remove <name>
ghostty-wall --help
```

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

Run as a daemon every 15 minutes:

```bash
export INTERVAL_SEC=900
ghostty-wall --daemon
```

---

## ⚙️ Configuration

### Ghostty include (auto-managed)

* File: `~/.config/ghostty/wallpaper.conf`
  Example content (auto-written):

  ```ini
  background-image=/tmp/anime_wallpapers/current_wallpaper.jpg
  background-image-fit=cover
  background-image-position=center
  background-image-opacity=0.1
  ```
* The tool ensures `~/.config/ghostty/config` contains:
  `config-file = ~/.config/ghostty/wallpaper.conf`
  (If the line is missing, it will append it.)

### Repo list format

* File: `~/.config/ghostty/wallpaper_repos.txt`
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

* `INTERVAL_SEC` — interval for daemon mode (default: `3600`)
* `GITHUB_TOKEN` — optional; if set, the script adds an Authorization header to avoid GitHub API rate limits

  ```bash
  export GITHUB_TOKEN=ghp_XXXXXXXXXXXXXXXXXXXXXXXXXXXX
  ```

---

## 🧰 How it works (under the hood)

1. Reads (or creates) `~/.config/ghostty/wallpaper_repos.txt`
2. Picks a random repo line and constructs:

   * GitHub **API URL** to list files in the specified repo/path/branch
   * **Raw base URL** to download files
3. Filters the file list for image extensions
4. Randomly selects one image and downloads it to:

   * `/tmp/anime_wallpapers/current_wallpaper.<ext>`
5. Writes/updates `~/.config/ghostty/wallpaper.conf` to point to that file
6. Ensures `~/.config/ghostty/config` includes the `wallpaper.conf`
7. If Ghostty is running, attempts a reload via AppleScript (⌘⇧,)

---

## 🧹 Uninstall

Remove the CLI (config files are left in place):

```bash
./scripts/uninstall.sh
```

If you installed the LaunchAgent, also unload it:

```bash
launchctl stop  com.ghostty.wallpaper
launchctl unload ~/Library/LaunchAgents/com.ghostty.wallpaper.plist
rm ~/Library/LaunchAgents/com.ghostty.wallpaper.plist
```

(Optional) Clean temporary wallpapers and the include:

```bash
rm -rf /tmp/anime_wallpapers
rm -f ~/.config/ghostty/wallpaper.conf
# (and remove the include line from ~/.config/ghostty/config if you want)
```

---

## 🛡️ Security & Privacy

* No telemetry.
* Uses GitHub’s public API to list files and raw.githubusercontent.com to download images.
* If `GITHUB_TOKEN` is provided, it is used only for GitHub API authentication headers.

---

## 🪪 Compatibility & Limitations

* macOS only (relies on AppleScript and macOS paths)
* Ghostty only
* Online sources only (GitHub repos); local folders are not supported at this time
* If your Ghostty config is in a non-standard location, ensure `~/.config/ghostty/config` exists or symlink it

---

## ❓ FAQ

**Q: “Command not found” after install?**
A: If the installer used `~/.local/bin`, open a new terminal or `source ~/.zshrc` / `~/.bash_profile` to refresh `PATH`.

**Q: Ghostty didn’t reload.**
A: Make sure Ghostty is running and macOS accessibility permissions allow automation (System Settings → Privacy & Security → Automation/Accessibility). The tool will still apply the wallpaper on Ghostty’s next launch.

**Q: GitHub API rate-limited me.**
A: Export a `GITHUB_TOKEN` (a classic Personal Access Token is enough for public repos).

**Q: My repo shows “No images found”.**
A: Double-check `branch` and `path`. The `path` should be relative to the repo root; leave it blank to use the root.

**Q: Can I control opacity/fit/position?**
A: Edit `~/.config/ghostty/wallpaper.conf` to tweak:

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

* **v0.1.0** – Initial public release (CLI, installer, LaunchAgent, GitHub repo picker)

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

# 5) Run forever every 15 minutes
export INTERVAL_SEC=900
ghostty-wall --daemon
```
