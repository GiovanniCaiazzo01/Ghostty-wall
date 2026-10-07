# Advanced usage

For initial setup, follow the [quick start](../README.md#quick-start).

- [Create or edit a profile](#create-or-edit-a-profile)
- [Browse profiles](#browse-profiles)
- [Delete a profile](#delete-a-profile)
- [Update safely](#update-safely)
- [Understand prompts](#understand-prompts)

A **profile** saves a recipe for wallpaper, colors and terminal settings. Applying it saves a restorable **environment** snapshot. Deleting a profile keeps its history.

## Create or edit a profile

### Start with an image or generate one

```bash
ghostty-wall create night
```

Choose **Generate** or **Image**. Full-screen Create uses the Edit actions below. Standalone line prompts offer **Use now** / **Not now** after Save; Enter defaults to **Not now**.

The Downloads/Pictures picker copies your PNG/JPEG, keeping the original. **Another variant** changes only unsaved generated drafts; saved wallpapers stay fixed.

To import directly without applying:

```bash
ghostty-wall new night /path/to/image.png
```

For registered folders, GitHub sources and reproducible generation, see [wallpaper sources](https://giovannicaiazzo01.github.io/Ghostty-wall/wallpapers/).

### Customize before saving

```bash
ghostty-wall edit night
```

Adjust **Wallpaper**, **Colors** or **Terminal**. Full-screen Create/Edit share these actions, including compact layouts:

| Key | Result |
| --- | --- |
| `s` — **Save** | Save the profile and any staged image; finish without applying or reloading. |
| `u` — **Save and use** | Confirm, save, then apply. Enter defaults to **Back to editor**; `n`/Esc also retains the draft. |
| Esc/`q` — **Cancel** | Discard unsaved changes without publishing a profile or image. |

Definite save failures retain the draft; concurrent Profile/Source edits require reopening. **Uncertain publication or incomplete rollback requires inspection, not retry**; Cancel cannot undo it. Press `v` for details.

**Save, apply and reload are separate.** Apply failure leaves the saved profile; uncertain effects require inspection. Applying records history before reload. Failed reload does not undo history; accepted reload does not prove a visible change. [Reload Ghostty manually when needed](https://giovannicaiazzo01.github.io/Ghostty-wall/troubleshooting/#reload).

To change one setting:

```bash
ghostty-wall edit night wallpaper.opacity 0.2
ghostty-wall edit night colors.foreground e3cd7b
ghostty-wall edit night colors.foreground auto
ghostty-wall apply night
```

Field edits **save immediately without applying**. Run `apply` when ready; `auto` restores automatic colors. See [color controls](https://giovannicaiazzo01.github.io/Ghostty-wall/colors/#customize) for individual and ANSI slots.

### Wallpaper strength is not terminal transparency

New image profiles start at opacity **0.05**, unless you choose otherwise. Existing profiles, edits, duplicates and restored environments keep their settings, including omitted opacity.

Wallpaper opacity fades the image toward the background, without changing transparency or text colors. Light backgrounds can make it lighter. Readability still depends on the colors.

### Advanced files and compatibility

Edit `config.toml` for sources and `profiles/*.toml` for recipes. Never edit generated `current.ghostty`. See the [configuration reference](https://giovannicaiazzo01.github.io/Ghostty-wall/configuration/).

Back up profiles and history before downgrading: older binaries may not read version 2 profiles.

## Browse profiles

```bash
ghostty-wall
```

Moving the selection previews without applying. **Choose Use to apply.**

| Key | Action |
| --- | --- |
| ↑ / ↓ or `j` / `k` | Select and preview |
| `n` / `e` / `x` | Create / edit / delete |
| `a` | Use selected profile |
| Tab | Switch Profiles / Sources |
| Enter or `p` | Enlarge/restore the compact sample |
| `?` | More actions |
| `v` | Results, errors and limitations |
| `i` | View the original image |
| `q` | Quit |

### What the preview can—and cannot—show

**The sample is internal, not a live Ghostty preview.** Wallpaper blending and colors are approximate; fonts, blur, transparency, geometry, unmanaged settings and overrides can differ. Unsupported graphics use a labelled color-cell fallback.

Random previews and Use share a seed, but source changes can change the applied image. Reselect to retry errors or refresh remote content. History replay assigns no active-profile marker.

Details: [preview limitations](https://giovannicaiazzo01.github.io/Ghostty-wall/terminal-browser/#previews).

### Small screens, forms, and maintenance

The browser needs **60×18** cells; Create/Edit forms need **40×12**. A started maintenance change cannot be cancelled; wait for completion or recovery.

More controls and source administration: [management-center guide](https://giovannicaiazzo01.github.io/Ghostty-wall/terminal-browser/).

## Delete a profile

```bash
ghostty-wall delete night
ghostty-wall delete       # Choose from a list
```

Review the target and removal summary. Confirm with `y`; Enter, EOF or `n` cancels. The TUI also accepts Esc/Ctrl-C/Ctrl-D to decline. **Welcome is protected.**

### What stays safe

- Inactive deletion leaves Ghostty unchanged.
- Active deletion first commits existing Welcome. If that fallback fails, deletion stops.
- Cleanup removes only images proven to belong exclusively to the profile.
- Originals, shared or ambiguous images, sources, history and replay assets stay. Reusing an identical image does not establish ownership.

After restoring history, apply a profile before deleting. Older installations without Welcome receive no automatic fallback.

### If deletion fails or gets interrupted

If removal fails after Welcome was committed, Welcome may remain active. Changed files stay; a changed target requires fresh confirmation. Cleanup does not follow a substituted `profiles/` symlink.

Interrupted deletion can retain `<Managed Root>/.tmp-delete-<token>/`. Inspect recovery paths before retrying; these files are not cleaned automatically. See the [deletion guide](https://giovannicaiazzo01.github.io/Ghostty-wall/profiles/#delete).

## Update safely

```bash
ghostty-wall update --check  # Check only
ghostty-wall update         # Install latest stable
```

Updates are explicit and affect **Ghostty Wall**, not Ghostty. Profiles, sources, history, environments, assets and integration stay intact. Current installations are unchanged.

### Check which copy you are updating

The updater changes the executable you invoked. If the version looks wrong, check:

```bash
command -v ghostty-wall
ghostty-wall --version
```

See [PATH conflicts](https://giovannicaiazzo01.github.io/Ghostty-wall/installation/#path-conflicts). Keep other installers idle and restart Ghostty Wall after updating.

### Verification and recovery

The updater verifies the official archive, SHA-256, layout, platform and version. Unsupported/unowned installations are rejected. Repair missing or mismatched ownership with the original installer.

| Failure | Next step |
| --- | --- |
| Check, preparation or verification | Old installation is untouched; fix the reported cause. |
| Replacement | The updater attempts to roll back executable and metadata; read the error. |
| Rollback also fails | Inspect retained recovery paths before retrying. |
| Forced termination or power loss | Both files are not replaced crash-atomically; inspect `.ghostty-wall-update-*` recovery files. |

See the [update reference](https://giovannicaiazzo01.github.io/Ghostty-wall/installation/#update) for requirements and recovery.

## Understand prompts

Standalone prompts use **key or number, then Enter**. Full-screen forms show their controls.

After standalone creation's Save, Enter means **Not now**. Deletion defaults to **Cancel**. Full-screen Save and use defaults to **Back to editor**. Prompts without a default ask again after invalid input.

Set `NO_COLOR=1` to disable styling; an empty `NO_COLOR` also works. Piped interaction stays plain.
