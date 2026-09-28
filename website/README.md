# Ghostty Wall documentation

A responsive, static documentation site for [Ghostty Wall](https://github.com/GiovanniCaiazzo01/Ghostty-wall).

## Edit

- `generate.py` contains the documentation content and shared page structure.
- `dist/styles.css` contains the visual theme and responsive layouts.
- `dist/app.js` handles navigation, copy controls, and the page outline.
- `dist/release.js` reads GitHub’s public latest-release API when each page loads and updates the release badge and download label. The links always use `/releases/latest`; if the API is unavailable or rate-limited, they keep working with a generic label. No token is required or shipped.
- All pages use the existing mascot PNG as their favicon.
- Run `python3 generate.py` after editing content (Python 3.11+). Generated pages in `dist/` are tracked; Pages regenerates and checks them before deployment. No site runtime dependencies are required.

## Content sources

Content follows `README.md` and current CLI help/source: guided creation, visual editing, safe deletion, automatic read-only Profile previews, contained full-screen forms/maintenance, Sources, palettes, History and safe release/curl and Cargo updates. The historically referenced `docs/user-guide.md` is absent in this checkout; do not treat it as an available content source. Update `generate.py` alongside user-facing changes; prose is curated, not imported automatically. The documentation version and minimum Rust version come from `Cargo.toml`. Tests check links, key feature coverage and version agreement.

`dist/assets/mascot.png` and `dist/assets/welcome.png` are the project's existing images. The introduction terminal is an illustrative example, not a screenshot of CLI output; its palette strip is illustrative.

The visible release version and download destination track GitHub’s latest stable release independently.

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

GitHub Actions regenerates and publishes `website/dist/` to project Pages on pushes to `main` that change the site, `Cargo.toml`, README, user guide or Pages workflow. Set **Settings → Pages → Build and deployment → Source** to **GitHub Actions** once. URL: https://giovannicaiazzo01.github.io/Ghostty-wall/ . Site-only pushes do not trigger the tool release workflow; it runs only for `v1.*` tags.

To host elsewhere, publish the contents of `dist/` under any path. Keep page directories and assets together. No Node.js installation or build step is required. The release badge uses GitHub’s public API; fonts are loaded from Google Fonts. These features need an internet connection.
