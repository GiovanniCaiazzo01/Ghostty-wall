# Advanced usage

For initial setup, follow the [quick start](../README.md#quick-start).

- [Create or edit a profile](#create-or-edit-a-profile)
- [Browse profiles](#browse-profiles)
- [Maintain wallpaper sources](#maintain-wallpaper-sources)
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

The Downloads/Pictures picker copies your PNG/JPEG, keeping the original. **Another variant** changes only unsaved generated drafts; saved wallpapers stay fixed. Guided Generate and Edit use `composition-v1`: seed-driven palettes, staggered tiles, oblique ribbons or soft off-center fields, at 768×512. The complete 32-byte seed determines the output. Previously saved `gradient-v1` images and the advanced `new --generate SEED_HEX` command keep their original semantics; generation metadata never regenerates saved or historical image bytes. Older binaries reject the new recipe identifier; back up before downgrading.

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

Wallpaper opacity fades the image toward the background, without changing terminal transparency. Automatic text colors are recalculated for the chosen opacity; customized colors stay unchanged. Light backgrounds can make the image lighter.

### Reversible image blur and darkening

In `edit PROFILE`, Wallpaper controls include **Image blur**, **Image darkening** and **Auto darkening**. Blur is Gaussian sigma 0–32 at a 512-pixel short edge (scaled to image dimensions), not `terminal.background_blur_intensity`. Darkening is a 0–100% linear-light black overlay applied to the image RGB, preserving alpha; it is not wallpaper opacity. Set both to zero to recover the original bytes. Originals and desktop wallpaper are never changed.

```toml
# Inside a schema_version = 2 Profile with a source wallpaper:
[wallpaper.effects]
algorithm = "image-effects-v1"
blur = 8
darken = 30
```

Source bytes and ownership claims stay unchanged. Plan derives a bounded PNG; captured Environments retain the resulting bytes and replay without the Source or effect renderer. Editing reuses a small effect thumbnail; the editor sample is approximate, while browser Use reviews the full prepared result. Save stores the recipe only; Cancel writes nothing. Existing shared-image cleanup and source validation still apply.

### Readability

New guided/imported Profiles and explicitly resetting ALL colors to Automatic use **`oklch-v2`**, stored as `algorithm = "oklch-v2"` in `[colors]`. It builds tinted neutrals and distinct ANSI semantic hues, adjusts OKLCH lightness at fixed hue, reduces chroma for sRGB gamut, and checks quantized RGB. Initial automatic darkening chooses a conservative minimum whole percentage; its numeric value is visible and adjustable. **Auto darkening** recalculates it after an opacity change; manual adjustments are not silently overwritten. Manual color overrides remain outside this automatic contract. No APCA implementation or certification is claimed; arbitrary application truecolor, dim text and Fish configuration are not controlled.

Profiles without an explicit palette algorithm retain `kmeans-v4`: automatic foreground, all 16 ANSI text colors and cursor target at least 4.5:1 against the brightest modeled background. Analysis checks every source pixel, including image alpha, in linear-light sRGB. The solid background is also included, so the bound covers cropping, fit, position, repeat and uncovered areas without assuming window dimensions. This is conservative: cropped-out highlights can still lighten automatic colors. Selection text is checked against its solid selection background, not the wallpaper.

Changing image or opacity recalculates automatic text colors. Image analysis is reused while adjusting opacity in the editor. Customized slots, including background and selection, are preserved; their choices can invalidate automatic contrast bounds. Named themes and explicit palettes are not recolored.

At high image opacity, even white text may not reach 4.5:1. For `oklch-v1`/`oklch-v2`, Plan reports `palette_contrast.status` (`feasible` or `infeasible`) for the **automatic light-on-dark whole-source bound**, with target 4.5 and manual overrides explicitly unchecked. Infeasibility also produces diagnostic `colors.contrast-infeasible`, a persistent browser/editor warning independent of thumbnail sampling, and an Apply warning. This is not a proof that every alternative polarity or viewport is impossible. `oklch-v2` keeps its original distinct roles as a **non-conforming fallback**; it does not claim to solve contrast. `oklch-v1` preserves its historical white fallback byte-for-byte, but now reports infeasibility too. Reduce opacity or adjust darkening explicitly; no hidden effect change, automatic migration or lowered contrast target occurs. Browser details (`v`) report estimated minimum contrast across thumbnail regions, including ANSI, cursor and selection. This estimate can detect problems, not certify every source pixel or terminal application. Dim/faint text, application-specific colors and cursor text are not certified.

Omitted image opacity is unknown: automatic colors retain the flat-background correction, while the sample illustrates opacity 1.0. Neither is a discovered Ghostty setting; set opacity explicitly to enable composition-aware correction. Native blending, Display P3, desktop transparency, external overrides and unmanaged settings can differ from this model. No universal accessibility guarantee is made. Saved Profiles are not migrated; applying a Profile without an explicit algorithm still resolves with v4. History stores resolved colors and transformed image bytes and keeps replaying its original result, including kmeans v1–v4 and `oklch-v1` provenance. Saving/reopening/replacing an existing Profile retains its algorithm; only explicitly confirmed Reset ALL colors opts into `oklch-v2`. Older binaries reject the new palette/effect identifiers: back up the Managed Root before downgrading.

### Advanced files and compatibility

Edit `config.toml` for sources and `profiles/*.toml` for recipes. Never edit generated `current.ghostty`. See the [configuration reference](https://giovannicaiazzo01.github.io/Ghostty-wall/configuration/).

Back up profiles and history before downgrading: older binaries may not read version 2 profiles.

## Maintain wallpaper sources

```bash
ghostty-wall source list
ghostty-wall source show wallpapers
ghostty-wall source edit wallpapers local ~/Pictures/moved-wallpapers
ghostty-wall source edit remote github owner/repo --ref main --path images
ghostty-wall source check wallpapers
ghostty-wall source remove unused
```

`show` lists the definition and dependent saved Profiles. `edit` keeps the ID and kind; it replaces the complete definition, so omitted GitHub ref/path reset to the default branch/repository root. Profiles stay byte-preserved; future resolution may choose a different wallpaper. Active configuration and History stay unchanged.

A valid but unavailable location can be saved. `check` distinguishes an unavailable Source from an empty Candidate Set; a successful PNG/JPEG listing does not prove every image decodes. Check never applies or reloads.

Removal requires `y`; Enter/n/cancel/EOF cancels. Referencing Profiles block removal; unreadable, invalid or unsafe Profile files block dependency analysis rather than count as unused. Only the Source entry is removed—not original images, managed images, replay assets, Environments or History. Concurrent configuration changes require reopening; references are checked again under the mutation lock.

TUI Actions: `S` Show, `E` Edit, `C` Check, `Z` Remove. Choose a Source ID, review prefilled edit fields, then confirm with `y`. Declining Save retains the fields; Esc closes the form without saving. Successful changes refresh the browser preview without applying anything. Source-kind conversion and ID rename are unsupported.

## Browse profiles

```bash
ghostty-wall
```

Moving the selection previews without applying. **Choose Use to apply.**

At 90 columns and above, compact Profile navigation leaves most of the browser for the preview. At 100 columns and 18 rows, Edit shows a dominant sample on the left and contextual controls on the right, with the current Wallpaper / Colors / Terminal group highlighted. The action footer stays separate from the preview. Narrow Edit layouts label the current Controls / Preview view; `p` switches between them instead of compressing both into columns. In Ghostty, Edit reuses the browser's image compositor and Kitty renderer with a reduced source (at most 384×256). Blur/darkening and image composition run on a latest-only preview worker; obsolete results cannot replace the current draft preview. Loading, failed previews and unsupported graphics are explicitly labelled as cell fallback. The source remains unchanged, and the internal sample is not a pixel-exact or live Ghostty view. Control labels wrap at word boundaries; the selected control position and Home/End hints make scrolling discoverable. Editor details (`v`) remain available before the first Preview: they explain how to open it and show known automatic contrast infeasibility without inventing a sampled contrast estimate.

Screen transitions group clearing and redrawing with synchronized output where supported, preventing presentation of an intermediate cleared screen. Preview image preparation/transmission remains asynchronous; this does not certify compositor latency or change Save/Use authority. Browser/editor chrome uses fixed dark RGB colors, not the host ANSI palette or draft colors. Editor headings and action shortcuts use a brighter bold accent; inactive tabs, navigation hints and separators use a secondary RGB color without terminal dimming. Results/errors retain normal text contrast. Focus keeps both its highlighted row and `>` marker. The sample still shows the resolved Profile colors, including any low-contrast warning. Window opacity, host wallpaper, padding, shaders and desktop compositing remain terminal-owned. Ghostty's `background-opacity-cells = true` applies window opacity even to explicit RGB cell backgrounds; the TUI cannot promise opaque panels under that setting and does not change it. Isolating HOME/XDG for the app isolates its data, not the configuration of an already-open Ghostty window. No Fish or personal terminal configuration change is required by the app.

| Key | Action |
| --- | --- |
| ↑ / ↓ or `j` / `k` | Select and preview |
| `n` / `e` / `x` | Create / edit / delete |
| `a` | Use selected profile |
| Tab | Switch Profiles / Sources |
| Enter or `p` | Enlarge/restore the compact sample |
| `?` | More actions |
| `v` | Results, errors and limitations |
| `i` | View the resolved image |
| `q` | Quit |

### What the preview can—and cannot—show

**The sample is internal, not a live Ghostty preview.** Wallpaper blending and colors are approximate; fonts, blur, transparency, geometry, unmanaged settings and overrides can differ. Unsupported graphics use a labelled color-cell fallback.

Browser **Use** commits the prepared image bytes, resolved colors and managed settings shown by the current successful preview, without resolving the Profile/Source again. Input versions are checked under the mutation lock before any publication. If local Profile/config/image/source-root inputs changed, the sample refreshes and you must choose Use again after reviewing it. Loading, failed or obsolete previews cannot apply. A captured named-theme result stays fixed even if its original theme file changes; reselect to resolve it again. Remote inputs are captured once, not fetched again during Use.

The guarantee covers managed content, not pixel-perfect Ghostty rendering or inherited settings. Standalone CLI `apply` still resolves normally; it has no reviewed result. Reselect to retry errors or refresh remote content. History replay assigns no active-profile marker.

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
