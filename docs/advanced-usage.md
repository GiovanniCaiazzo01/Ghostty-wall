# Advanced usage

Already installed? Use this guide when you want more control or need to recover from a problem. For initial setup, follow the [quick start](../README.md#quick-start).

- [Create or edit a profile](#create-or-edit-a-profile)
- [Browse profiles](#browse-profiles)
- [Delete a profile](#delete-a-profile)
- [Update safely](#update-safely)
- [Understand prompts](#understand-prompts)

A **profile** is a saved recipe: wallpaper, colors, and terminal settings. Applying it saves an **environment**—a snapshot that history can restore later. Deleting a profile does not delete that history.

## Create or edit a profile

### Start with an image or generate one

```bash
ghostty-wall create night
```

Choose **Generate** or **Image**, review the result, then **Save**.

- Standalone prompts retain **Use now** / **Not now** after Save; **Not now** is the default and leaves Ghostty unchanged.
- Full-screen Create offers `s` **Save**, `u` **Save and use**, and Esc/`q` **Cancel**. Save and use defaults to **Back to editor**; declining retains the draft.
- Cancelling before Save leaves no profile.
- Generated wallpaper stays fixed after saving. **Another variant** changes only an unsaved draft.

The image picker starts in your system's Downloads or Pictures folders. It copies your chosen PNG/JPEG; it never moves or modifies the original.

| Picker input | What it does |
| --- | --- |
| Arrow keys, then Enter | Select an image or open a directory |
| `d` or `p`, then Enter | Switch to Downloads or Pictures |
| `/text`, then Enter | Search |
| `path:/absolute/path` | Open an exact path |
| Esc or Ctrl-C | Return to the draft without importing |

For a direct import or a registered wallpaper source:

```bash
ghostty-wall new night /path/to/image.png
ghostty-wall source add wallpapers local ~/Pictures/wallpapers
ghostty-wall new forest --source wallpapers --path forest.jpg
```

These examples create profiles without applying them. See [wallpaper sources](https://giovannicaiazzo01.github.io/Ghostty-wall/wallpapers/) for GitHub sources and reproducible generated wallpapers.

### Customize before saving

```bash
ghostty-wall edit night
```

The visual editor groups controls into **Wallpaper**, **Colors**, and **Terminal**. It supports image replacement, color samples, exact hex values, and numeric settings.

Create and visual Edit share these completion choices, also on compact layouts:

| Key | Outcome |
| --- | --- |
| `s` — **Save** | Publish the recipe and any staged image; finish without applying or reloading. |
| `u` — **Save and use** | Confirm, save, then apply once. Enter defaults to **Back to editor**; `n`/Esc also retains the draft. |
| Esc/`q` — **Cancel** | Discard unsaved changes without publishing a Profile or image. |

A definite save failure retains the draft; correct the cause and retry. Concurrent Profile/Source edits require reopening. Uncertain publication or incomplete rollback requires inspection, not a retry; cancellation cannot undo uncertain publication. `v` opens full editor error details.

**Saving, applying, and reloading are separate steps.** Completion reports say **Profile saved.**, **Profile saved; applying failed. See details.**, **Configuration updated; reload requested.**, or **Configuration updated; reload Ghostty manually.** Apply failure leaves the saved Profile available, but uncertain apply effects require inspection. Applying records history before requesting best-effort reload; failed reload does not undo that record. Activation IDs and reload reasons remain in detailed output (`v` in the browser). Accepted reload requests do not prove a visible change; reload Ghostty manually if needed.

For scripts or a single setting, use the field form:

```bash
ghostty-wall edit night wallpaper.opacity 0.2
ghostty-wall edit night colors.foreground e3cd7b
ghostty-wall edit night colors.foreground auto
```

**Field edits save immediately, without applying.** `auto` resets that color to automatic generation. Version 2 generated colors support individual background, foreground, cursor, selection, and ANSI palette slots; version 1 behavior is unchanged. See [color controls](https://giovannicaiazzo01.github.io/Ghostty-wall/colors/#customize).

### Wallpaper strength is not terminal transparency

New image profiles start with wallpaper opacity **0.05**. This includes imports, generated images, source-based creation, and a newly installed Welcome profile.

Lower wallpaper opacity fades the image toward the background. It does not change terminal transparency or text colors. On a light background, fading can make the result lighter—not darker. Readability still depends on your colors.

Existing profiles, edits, duplicates, and restored environments keep their settings, including omitted opacity. There is no automatic migration; an explicit opacity choice in your draft takes precedence.

### Advanced files and compatibility

TOML remains available when you need it. Sources belong in Ghostty Wall's `config.toml`; profile recipes belong in `profiles/*.toml`. Use the [configuration reference](https://giovannicaiazzo01.github.io/Ghostty-wall/configuration/) for examples. Do not edit the generated `current.ghostty` file.

New owned-image profiles use schema version 2. Older binaries may not understand their saved recipes or color-generation metadata. **Back up your profiles and history before downgrading.**

For contributors comparing opacity, with ImageMagick installed:

```bash
bash scripts/compare-wallpaper-opacity.sh /tmp/gw-opacity-comparison
```

This is a static compositing study, not a Ghostty screenshot. Photo comparisons support 0.05 as less distracting on dark backgrounds; light backgrounds fade the photo instead of darkening it.

## Browse profiles

```bash
ghostty-wall
```

The browser opens on Profiles and previews your selection automatically. **Moving selection never applies a profile. Choose Use to apply it.**

| Key | Action |
| --- | --- |
| ↑ / ↓ or `j` / `k` | Select and preview |
| `n` | Create |
| `e` | Edit a visual draft |
| `x` | Delete, with confirmation |
| `a` | Use the selected profile |
| Enter or `p` | Enlarge or restore the sample on small screens |
| Tab | Switch between Profiles and Sources |
| `?` | Open more actions |
| `v` | Read full results, errors, or preview limitations |
| `i` | View the original image, not the opacity-adjusted sample |
| `q` | Quit |

The active marker reflects the most recent profile activation. Restoring history does not invent an active-profile marker.

### What the preview can—and cannot—show

The internal sample shows wallpaper blending, text, ANSI colors, cursor, and selection. **It is not live Ghostty reload.**

In Ghostty, it uses full-color terminal graphics. Elsewhere, a labelled color-cell fallback may appear; that fallback is not a photograph. Missing or corrupt images show errors without preventing navigation.

The sample uses linear-light sRGB blending, matching Ghostty's Linux default. Other blending modes, Display P3, and your overrides can differ. Font rendering, blur, desktop transparency, and exact image geometry are not simulated. Unmanaged or omitted settings use illustrative defaults, not values discovered from your Ghostty configuration. Zero opacity hides the photo; previews cannot guarantee readability for arbitrary colors or opacity.

Navigation stays responsive while previews load. Reselect to retry errors or refresh remote content. Local changes refresh successful samples automatically; no persistent preview cache is created. Random-profile preview and Use share a session seed, but changed sources can still produce a different image when applied.

For exact preview behavior, see [preview limitations](https://giovannicaiazzo01.github.io/Ghostty-wall/terminal-browser/#previews).

### Small screens, forms, and maintenance

The browser needs **60×18** terminal cells. Smaller windows show resize guidance and allow cancellation. Create/Edit forms support **40×12**, with a compact sample toggle.

Forms keep your input after validation errors. Enter advances or submits; Tab/Shift-Tab revisits fields; Ctrl-U clears; F1 shows errors; Esc/Ctrl-C/Ctrl-D cancels. Advanced field actions save immediately, unlike visual drafts.

More actions include sources, duplicate/rename, history, Previous, Doctor, updates, initialization, migration, and uninstall. Source administration supports adding and inspecting sources, not renaming or removing them. Reports scroll with arrows or PageUp/PageDown and are capped at 4 MiB, with truncation disclosed.

Read-only jobs can be dismissed with Esc, though work may finish in the background. Another maintenance read-only job must wait for it. **Once a maintenance change starts, cancellation is unavailable: wait for completion or recovery.** Update requires restarting Ghostty Wall afterward. Uninstall keeps profiles and history and returns to the browser.

Non-terminal input uses the older line-based browser: type a key, then Enter. For contributor builds, performance checks, and remaining shortcuts, see the [management-center guide](https://giovannicaiazzo01.github.io/Ghostty-wall/terminal-browser/).

The experimental Linux live draft API is library-only; it is not connected to Create/Edit or the TUI. Real-window changes, cancel restoration, and affected-window scope remain unverified. An accepted reload request is not proof of a visible change. See [reload troubleshooting](https://giovannicaiazzo01.github.io/Ghostty-wall/troubleshooting/#reload).

## Delete a profile

```bash
ghostty-wall delete night
# Or choose from a list:
ghostty-wall delete
```

Check the named target and removal summary. Confirm with `y`. Enter, EOF, or `n` cancels; the TUI also accepts Esc/Ctrl-C/Ctrl-D to decline. Welcome is protected.

### What stays safe

- Deleting an **inactive** profile leaves Ghostty unchanged.
- Deleting an **active** profile first commits the existing Welcome profile. If that fallback fails, deletion stops.
- Cleanup removes only image data proven to belong exclusively to that profile.
- Originals, shared or ambiguous images, sources, history, and assets needed for replay remain.
- Reusing an identical image does not give the profile ownership of it.

After restoring history, apply a profile before deleting. Older installations without Welcome are not silently given a fallback. See the [deletion guide](https://giovannicaiazzo01.github.io/Ghostty-wall/profiles/#delete).

### If deletion fails or gets interrupted

If Welcome was already committed, it may remain active even when removal fails. Reload is best-effort, not proof that Ghostty visibly changed.

Changed files are retained rather than blindly removed. A profile found again during removal or ownership checks requires fresh confirmation; the committed Welcome fallback is not undone. Cleanup does not follow a substituted `profiles/` symlink.

An interruption may leave files in `<Managed Root>/.tmp-delete-<token>/`. Inspect them; there is no automatic cleanup of these recovery files.

## Update safely

```bash
ghostty-wall update --check  # Check without installing or changing user data
ghostty-wall update         # Install the latest stable release
```

Updates are explicit, never automatic. This updates **Ghostty Wall**, not Ghostty, and preserves profiles, sources, history, saved environments, assets, and integration. An already-current installation is left unchanged.

### Check which copy you are updating

The updater changes the executable you invoked, including installations with a custom prefix. Another copy earlier on your PATH can make an update look ineffective.

```bash
command -v ghostty-wall
ghostty-wall --version
```

See [PATH conflict guidance](https://giovannicaiazzo01.github.io/Ghostty-wall/installation/#path-conflicts). Keep other installers idle during updates. Restart Ghostty Wall afterward: an open TUI still runs the old version until restarted.

### Verification and recovery

The updater downloads the version-pinned official Linux x86_64 archive. It verifies SHA-256, archive layout and platform, and executable version before replacing the binary and its ownership metadata together.

It rejects missing tools, read-only directories, filesystems without atomic-exchange support, inconsistent ownership, substituted symlinks, and unowned/manual binaries. It does not escalate privileges. If ownership metadata is missing or mismatched, use the original installer to repair it—**do not remove safeguards to force an update**.

| Failure | What to do |
| --- | --- |
| Checking, preparation, or verification fails | The old installation is untouched. Fix the reported cause before retrying. |
| Replacement fails | The updater rolls back the executable and metadata. Read the error before retrying. |
| Rollback also fails | Inspect the retained recovery paths named in the error and check the installation first. |
| Forced termination or power loss | Replacement is not crash-atomic across both files. Inspect any `.ghostty-wall-update-*` recovery files before retrying. |

See `ghostty-wall update --help` and the [update reference](https://giovannicaiazzo01.github.io/Ghostty-wall/installation/#update) for more detail.

## Understand prompts

Standalone prompts display their accepted keys and defaults. Type a key or number, then Enter.

```text
Wallpaper:

  [g] Generate wallpaper
  [i] Image from Downloads/Pictures
  cancel  Discard draft

Choice (g/i/cancel; no default):
```

After Save, Enter means **Not now**. Deletion defaults to **Cancel**. Creation choices without a default ask again on invalid input. Existing aliases and EOF/cancellation behavior are unchanged.

Long labels and paths wrap rather than being truncated. Colors appear only with terminal input and output and a non-dumb `TERM`. Set `NO_COLOR=1` to disable them; even an empty `NO_COLOR` disables styling. Piped interaction stays plain, and `plan --json`, normal output, and stderr routing are unchanged. Full-screen TUI forms have their own controls.
