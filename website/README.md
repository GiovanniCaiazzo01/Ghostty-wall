# Ghostty Wall documentation

A responsive, static documentation site for [Ghostty Wall](https://github.com/GiovanniCaiazzo01/Ghostty-wall).

## Edit

- `generate.py` contains the documentation content and shared page structure.
- `dist/styles.css` contains the visual theme and responsive layouts.
- `dist/app.js` handles navigation, copy controls, and the page outline.
- `dist/release.js` reads GitHub’s public latest-release API when each page loads and updates the release badge. The links always use `/releases/latest`; if the API is unavailable or rate-limited, they keep working with a generic label. No token is required or shipped.
- All pages use the existing mascot PNG as their favicon.
- Run `python3 generate.py` after editing content (Python 3.11+). Generated pages in `dist/` are tracked; Pages regenerates and checks them before deployment. No site runtime dependencies are required.

## Content sources

README and site introduction show the result first, followed by installation and a short quick start. Detailed behavior lives in guide/reference pages and [`docs/advanced-usage.md`](../docs/advanced-usage.md). Content follows current CLI help/source; update `generate.py` alongside user-facing changes. The curl installer remains the supported route for Linux x86_64; legacy Cargo copies appear only in PATH conflict guidance. The documentation version comes from `Cargo.toml`. Tests check links, supported installation guidance, screenshot placement, feature coverage and version agreement.

## Screenshots

`media/screenshots/` contains real Ghostty captures, copied into `dist/assets/` (`welcome.png` becomes `ghostty-welcome.png`). The welcome screenshot uses the bundled Profile's generated Ghostty configuration; its terminal shows actual `list` and `--version` output. The browser screenshot and GIF show internal previews, not live reload. The GIF is a short sequence of captured frames, linked rather than autoplayed. Captures use an isolated HOME and private D-Bus session; no personal desktop or existing terminal contents are included. Update both asset locations when replacing screenshots.

`dist/assets/mascot.png` and `dist/assets/welcome.png` remain the original project artwork, not screenshots.

The visible release version and release links track GitHub’s latest stable release independently.

## Run locally

From the repository root, run:

```sh
python3 -m http.server 8000 --directory website/dist
```

Open http://localhost:8000. Links and assets use relative URLs, supporting both local serving and GitHub project Pages.

To edit the documentation content and regenerate the pages:

```sh
python3 website/generate.py
python3 website/test_site.py
```

## Host elsewhere

GitHub Actions regenerates and publishes `website/dist/` to project Pages on pushes to `main` that change the site, `Cargo.toml`, README or Pages workflow. Documentation corrections need no version bump or release tag; a Pages deployment can keep the same product version. Set **Settings → Pages → Build and deployment → Source** to **GitHub Actions** once. URL: https://giovannicaiazzo01.github.io/Ghostty-wall/ . Site-only pushes do not trigger the tool release workflow; it runs only for `v1.*` tags.

To host elsewhere, publish the contents of `dist/` under any path. Keep page directories and assets together. No Node.js installation or build step is required. The release badge uses GitHub’s public API; fonts are loaded from Google Fonts. These features need an internet connection.
