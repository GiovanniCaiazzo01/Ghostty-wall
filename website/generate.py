from pathlib import Path
from html import escape

ROOT=Path(__file__).parent
OUT=ROOT/'dist'
REPO='https://github.com/GiovanniCaiazzo01/Ghostty-wall'
ICONS={
'arrow':'<path d="M7 17 17 7M7 7h10v10"/>',
'right':'<path d="M4 12h16m-6-6 6 6-6 6"/>',
'chevron':'<path d="m9 5 7 7-7 7"/>',
'book':'<path d="M12 5v15M12 5C8 2 4 3 2 4v15c3-1 7-1 10 1 3-2 7-2 10-1V4c-2-1-6-2-10 1Z"/>',
'download':'<path d="M12 3v12m-5-5 5 5 5-5M4 17v4h16v-4"/>',
'bolt':'<path d="m13 2-9 12h7l-1 8 10-12h-7l1-8Z"/>',
'code':'<path d="m7 7-5 5 5 5m10-10 5 5-5 5m-4-13-2 20"/>',
'layers':'<path d="m12 3 10 5-10 5L2 8l10-5Zm-10 9 10 5 10-5M2 16l10 5 10-5"/>',
'image':'<rect x="3" y="3" width="18" height="18" rx="3"/><circle cx="8" cy="8" r="1"/><path d="m21 15-6-6L3 21"/>',
'palette':'<path d="M12 3a9 9 0 1 0 0 18h1a2 2 0 0 0 0-4 2 2 0 0 1 0-4h4a4 4 0 0 0 4-4c0-4-5-6-9-6Z"/><path d="M7 8h.01M11 6h.01M16 7h.01M5 12h.01"/>',
'terminal':'<rect x="2" y="3" width="20" height="18" rx="3"/><path d="m6 8 4 4-4 4m7 0h5"/>',
'settings':'<path d="M4 7h16M4 17h16"/><circle cx="9" cy="7" r="3"/><circle cx="15" cy="17" r="3"/>',
'life':'<circle cx="12" cy="12" r="9"/><circle cx="12" cy="12" r="4"/><path d="m5.5 5.5 3.7 3.7m5.6 5.6 3.7 3.7m0-13-3.7 3.7m-5.6 5.6-3.7 3.7"/>',
'github':'<path d="M9 19c-4 1-4-2-6-2m12 5v-4a3 3 0 0 0-.8-2.3c2.7-.3 5.6-1.3 5.6-6a4.7 4.7 0 0 0-1.3-3.3 4.3 4.3 0 0 0-.1-3.3s-1-.3-3.4 1.3a11.7 11.7 0 0 0-6 0C6.6 2.8 5.6 3.1 5.6 3.1a4.3 4.3 0 0 0-.1 3.3 4.7 4.7 0 0 0-1.3 3.3c0 4.7 2.9 5.7 5.6 6A3 3 0 0 0 9 18v4"/>',
'copy':'<rect x="8" y="8" width="12" height="13" rx="2"/><path d="M16 8V5a2 2 0 0 0-2-2H5a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h3"/>',
'check':'<path d="m5 12 4 4L19 6"/>',
'menu':'<path d="M4 6h16M4 12h16M4 18h16"/>',
'info':'<circle cx="12" cy="12" r="9"/><path d="M12 11v6m0-10v.01"/>',
'rewind':'<path d="M3 10h6M3 10V4m0 6a9 9 0 1 1 1 8"/>',
'heart':'<path d="M20.8 4.6a5.5 5.5 0 0 0-7.8 0L12 5.7l-1.1-1.1a5.5 5.5 0 0 0-7.8 7.8L12 21l8.8-8.6a5.5 5.5 0 0 0 0-7.8Z"/>'}
def icon(name): return f'<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.55" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">{ICONS[name]}</svg>'
def code(text,label='Terminal',lang='shell'):
    text=text.strip('\n')
    lines=[]
    for line in text.splitlines():
        s=escape(line)
        if lang=='toml':
            if line.startswith('['): s=f'<span class="syn-section">{s}</span>'
            elif '=' in line:
                k,v=s.split('=',1); s=f'<span class="syn-key">{k}</span>=<span class="syn-string">{v}</span>'
        elif line.startswith('#'): s=f'<span class="syn-comment">{s}</span>'
        elif line.startswith('ghostty-wall'): s='<span class="syn-command">ghostty-wall</span>'+s[len('ghostty-wall'):]
        elif line.startswith('cargo'): s='<span class="syn-command">cargo</span>'+s[5:]
        lines.append(s)
    return f'<div class="code-block"><div class="code-header"><span>{icon("terminal" if lang=="shell" else "code")}{escape(label)}</span><button class="copy-button" type="button" aria-label="Copy {escape(label)} code">{icon("copy")}<span>Copy</span></button></div><pre tabindex="0"><code>'+ '\n'.join(lines)+'</code></pre></div>'
def note(text,title='Good to know'): return f'<aside class="callout">{icon("info")}<div><strong>{title}</strong><p>{text}</p></div></aside>'
def section(id,title,body): return f'<section id="{id}" class="doc-section"><h2><a href="#{id}">{title}<span aria-hidden="true">#</span></a></h2>{body}</section>'
def table(headers,rows):
    return '<div class="table-wrap"><table><thead><tr>'+''.join(f'<th>{h}</th>' for h in headers)+'</tr></thead><tbody>'+''.join('<tr>'+''.join(f'<td>{c}</td>' for c in row)+'</tr>' for row in rows)+'</tbody></table></div>'
def card(page,title,description,ico): return f'<a class="doc-card" href="/{page}/"><div class="card-top">{icon(ico)}{icon("right")}</div><h3>{title}</h3><p>{description}</p></a>'
def path(page): return '/' if page=='overview' else '/'+page+'/'

cargo='cargo install --git https://github.com/GiovanniCaiazzo01/Ghostty-wall --locked'
profile='''schema_version = 1

[wallpaper]
mode = "source"
source = "wallpapers"
selection = "path"
path = "city/night.png"
fit = "cover"
position = "center"
opacity = 0.1

[colors]
mode = "generated"

[terminal]
font_size = 13.5
background_opacity = 0.94
cursor_style = "bar"'''
source='''[sources.wallpapers]
kind = "local-directory"
path = "~/Pictures/wallpapers"'''
seed='000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f'
pages={}
def page(key,title,desc,group,body,toc): pages[key]=dict(title=title,desc=desc,group=group,body=body,toc=toc)

palette=['#0d1535','#ff9255','#79efcf','#ffc861','#4265ce','#8670ff','#50c4c5','#d3e2ff','#2d3f7c','#ffb078','#a3ffe2','#ffdb95','#799bff','#b8a5ff','#8de9e0','#f1f6ff']
demo=f'''<figure class="terminal-demo"><div class="demo-top"><div class="window-dots"><i></i><i></i><i></i></div><span>welcome to your new terminal</span><span class="demo-shell">ghostty</span></div><div class="demo-body"><img src="/assets/welcome.png" alt="The bundled Ghostty Wall welcome wallpaper" width="1672" height="941"><div class="demo-shade"></div><div class="demo-code"><span class="demo-comment"># A little more you. Every time you open a terminal.</span><div><span class="prompt">❯</span> ghostty-wall apply welcome</div><div class="demo-result"># Wallpaper + generated terminal colors</div><div class="demo-prompt"><span class="prompt">❯</span> <span class="cursor"></span></div></div><div class="demo-tag">BUNDLED WELCOME WALLPAPER</div></div><figcaption><span>Wallpaper → palette → environment</span><div class="swatches" aria-label="Illustrative terminal color palette">{''.join(f'<i style="background:{c}"></i>' for c in palette)}</div></figcaption></figure>'''
page('overview','Make Ghostty<br><em>your own.</em>','Wallpapers, colors, and visual settings. One reusable profile.<br>A small Rust CLI for a terminal that feels like home.','Get started',
 '''<div class="overview-actions"><a class="button primary" href="/installation/">Install Ghostty Wall '''+icon('right')+f'''</a><a class="text-link" href="{REPO}">View on GitHub {icon('arrow')}</a></div><div class="support-line"><span>Linux <b>stable</b></span><span>macOS <b class="experimental">experimental</b></span><span>MIT licensed</span></div>'''+demo+
 section('start-here','A good place to start', '<div class="card-grid">'+card('quick-start','Your first environment','Go from a fresh install to your first wallpaper in a few commands.','bolt')+card('profiles','Build your own profile','Bring your wallpapers. Make the colors and settings yours.','layers')+'</div>')+
 section('why-ghostty-wall','Less configuration. More character.',f'''<div class="feature-list"><div>{icon('palette')}<div><h3>Colors that belong together</h3><p>Generate a complete terminal palette from your wallpaper, use a Ghostty theme, or set every color yourself.</p></div></div><div>{icon('code')}<div><h3>Know what changes</h3><p>Preview a profile with <code>plan</code> before applying it. Your unrelated Ghostty settings stay untouched.</p></div></div><div>{icon('rewind')}<div><h3>A way back, built in</h3><p>Return to a saved environment with <code>previous</code>, even if its original wallpaper source is gone.</p></div></div></div>''')+
 section('how-it-works','A recipe. A result. A record.', '''<div class="concept-grid"><div><span class="step-kicker">01 / INTENT</span><h3>Profile</h3><p>The TOML recipe for the look you want.</p></div><div><span class="step-kicker">02 / RESULT</span><h3>Environment</h3><p>An immutable snapshot of resolved images and settings.</p></div><div><span class="step-kicker">03 / HISTORY</span><h3>Activation</h3><p>A durable record of the environment you applied.</p></div></div>'''),
 [('start-here','Start here'),('why-ghostty-wall','Why Ghostty Wall?'),('how-it-works','How it works')])

page('installation','Installation','A small CLI. A straightforward setup. Choose the installation method that works for your machine.','Get started',
 section('requirements','Before you begin', '<p>You need Ghostty installed. Linux is stable; macOS is experimental and requires a source build. Windows is unsupported.</p><p>The Cargo method requires Rust 1.85 or later. The prebuilt release targets <code>x86_64-unknown-linux-gnu</code>.</p>')+
 section('cargo','Install with Cargo', code(cargo)+ '<p>Cargo installs the binary into its bin directory, normally <code>~/.cargo/bin</code>. Make sure it is in your <code>PATH</code>.</p>')+
 section('linux-binary','Download the Linux binary',f'<p>No Rust toolchain needed. Download the archive and matching SHA-256 checksum from the release page. Verify the checksum, extract the archive, and run its included <code>install.sh</code>.</p><a class="button secondary" href="{REPO}/releases/latest"><span data-release-download>Download latest release</span> {icon("download")}</a><p>The installer uses <code>~/.local/bin</code> by default. You can change the prefix with <code>INSTALL_PREFIX</code>.</p>')+
 section('initialize','Initialize your installation',code('ghostty-wall init\nghostty-wall --version')+'<p>Initialization creates the managed directory, installs the Ghostty integration hook, and adds a bundled <code>welcome</code> profile to fresh installations.</p>'+note('Use <code>ghostty-wall init --dry-run</code> to inspect the changes before writing anything. Existing profiles are not overwritten.','Preview the setup'))+
 section('next-step','Make it yours','<p>You are ready to apply your first environment.</p>'+card('quick-start','Continue to quick start','Try the bundled profile, preview changes, and go back.','bolt')),
 [('requirements','Requirements'),('cargo','Install with Cargo'),('linux-binary','Linux binary'),('initialize','Initialize'),('next-step','Next step')])

page('quick-start','Your first environment','Start with the bundled welcome profile. No wallpaper collection or configuration file needed.','Get started',
 section('initialize','1. Initialize Ghostty Wall','<p>Run this once after installing the CLI. Fresh installations include the welcome profile and its wallpaper.</p>'+code('ghostty-wall init'))+
 section('preview','2. Preview the result','<p>Resolve the profile and inspect its settings before applying anything. Planning does not change files.</p>'+code('ghostty-wall plan welcome'))+
 section('apply','3. Apply your profile','<p>Activate the wallpaper and its generated colors. Ghostty Wall records an activation in local history, then attempts to reload Ghostty.</p>'+code('ghostty-wall apply welcome')+note('If the terminal does not update immediately, reload Ghostty manually. A failed reload does not undo a committed activation.'))+
 section('browse','4. Explore from your terminal',code('ghostty-wall tui')+'<p>Browse profiles, preview the result, and apply it from the terminal browser.</p>')+
 section('go-back','Return to a previous look','<p>Once there is an earlier environment in your history, restore it with:</p>'+code('ghostty-wall previous')+'<p>The saved environment and image are restored from local storage. The original source does not need to be available.</p>')+
 '<div class="card-grid">'+card('profiles','Create a profile','Use your own wallpaper and visual settings.','layers')+card('commands','Explore the commands','The complete CLI reference, in one place.','terminal')+'</div>',
 [('initialize','Initialize'),('preview','Preview'),('apply','Apply'),('browse','Browse'),('go-back','Go back')])

page('profiles','Create a profile','A profile is a declarative recipe for your terminal. Write it once, preview it, and apply it whenever you like.','Guides',
 section('managed-root','Find your configuration','<p>On Linux, your managed root is <code>${XDG_CONFIG_HOME:-$HOME/.config}/ghostty/ghostty-wall</code>. On macOS, it is <code>~/Library/Application Support/com.mitchellh.ghostty/ghostty-wall</code>.</p><p>For everyday customization, edit <code>config.toml</code> and <code>profiles/*.toml</code>.</p>'+note('Do not edit <code>current.ghostty</code>. It is generated and can be replaced on the next apply.','Keep your recipes in TOML'))+
 section('add-source','1. Add a wallpaper source','<p>Add this section to <code>config.toml</code>. Keep the existing <code>schema_version = 1</code> and your other sources.</p>'+code(source,'config.toml','toml'))+
 section('write-profile','2. Write your profile','<p>Create <code>profiles/night.toml</code>. Its filename gives the profile its ID: <code>night</code>. Change the image path to a PNG or JPEG inside your source directory.</p>'+code(profile,'profiles/night.toml','toml'))+
 section('preview-apply','3. Preview, then apply',code('ghostty-wall plan night\nghostty-wall apply night'))+
 section('profile-rules','A few simple rules','<ul><li>IDs use lowercase letters, digits, and internal hyphens, such as <code>my-night</code>.</li><li>Profiles live directly inside <code>profiles/</code>, without subdirectories.</li><li>Wallpaper, colors, and terminal sections are optional. Omitted settings remain unmanaged.</li><li>Generated colors require a wallpaper from a source.</li></ul>'),
 [('managed-root','Configuration location'),('add-source','Add a source'),('write-profile','Write a profile'),('preview-apply','Preview and apply'),('profile-rules','Profile rules')])

page('wallpapers','Wallpaper sources','Keep images on your machine or pull them from a GitHub repository. Choose an exact image or a reproducible random selection.','Guides',
 section('local','Local directories',code(source,'config.toml','toml')+'<p>Paths may be absolute, begin with <code>~/</code>, or be relative to <code>config.toml</code>. Other shell expansions are not supported. PNG and JPEG images are supported.</p>')+
 section('github','GitHub repositories',code('''[sources.wallpapers]
kind = "github"
repository = "owner/repo"
ref = "main"
path = "wallpapers"''','config.toml','toml')+'<p>Replace <code>owner/repo</code> with your repository. Both <code>ref</code> and <code>path</code> are optional: omitting them uses the default branch and repository root.</p><p>Each plan resolves the requested branch to a commit. For authentication or higher rate limits, set <code>GITHUB_TOKEN</code> in your environment; never put a token in a profile or source file.</p>')+
 section('selection','Select an image',code('''[wallpaper]
mode = "source"
source = "wallpapers"
selection = "path"
path = "city/night.png"''','Profile · exact image','toml')+'<p>The image path is relative to the source. For random selection, use <code>selection = "random"</code> and omit <code>path</code>.</p>'+code('ghostty-wall plan night --seed '+seed+'\nghostty-wall apply night --seed '+seed)+ '<p>Random selection requires a 32-byte seed, written as 64 hex digits. The same seed and unchanged source produce the same selection.</p>'+note('Version 1 does not include automatic timed wallpaper rotation.','Random selection'))+
 section('appearance','Control the image',table(['Setting','Supported values'],[('<code>fit</code>','<code>contain</code>, <code>cover</code>, <code>stretch</code>, <code>none</code>'),('<code>position</code>','For example, <code>center</code>, <code>top-left</code>, <code>bottom-right</code>'),('<code>opacity</code>','0 to 1; lower values make text easier to read'),('<code>repeat</code>','<code>true</code> or <code>false</code>')])+'<p>Omit the wallpaper section to leave it unmanaged. Use <code>mode = "none"</code> to explicitly disable the background image.</p>'),
 [('local','Local directories'),('github','GitHub sources'),('selection','Image selection'),('appearance','Image settings')])

page('colors','Colors & themes','Let your wallpaper set the palette, use a familiar Ghostty theme, or take control of every color.','Guides',
 section('generated','Colors from your wallpaper',code('[colors]\nmode = "generated"','Profile · generated colors','toml')+'<p>Ghostty Wall derives the background, foreground, 16 ANSI colors, cursor, and selection colors from the selected image. This mode requires a source wallpaper.</p>')+
 section('theme','Use a Ghostty theme',code('[colors]\nmode = "theme"\ntheme = "TokyoNight"','Profile · named theme','toml')+'<p>The theme must be available locally when the profile is resolved. Its colors are stored in the environment, so replay does not need the original theme file.</p>')+
 section('explicit','Set an explicit palette',code('''[colors]
mode = "explicit"
background = "1a1b26"
foreground = "c0caf5"
palette = [
  "15161e", "f7768e", "9ece6a", "e0af68",
  "7aa2f7", "bb9af7", "7dcfff", "a9b1d6",
  "414868", "f7768e", "9ece6a", "e0af68",
  "7aa2f7", "bb9af7", "7dcfff", "c0caf5",
]
cursor = "c0caf5"
selection_background = "33467c"
selection_foreground = "c0caf5"''','Profile · explicit colors','toml')+'<p>Provide a background, a foreground, and exactly 16 palette colors. Values are six lowercase hexadecimal digits without a <code>#</code>. Cursor and selection colors are optional.</p>')+
 section('readability','Keep your terminal readable','<p>Reduce <code>wallpaper.opacity</code> for bright images. Ghostty also supports <code>minimum-contrast = 4.5</code> in its normal root configuration.</p>'+note('<code>minimum-contrast</code> belongs in your normal Ghostty config, not a Ghostty Wall profile. Omit <code>[colors]</code> entirely to leave colors unmanaged.')),
 [('generated','Generated colors'),('theme','Ghostty themes'),('explicit','Explicit palette'),('readability','Readability')])

page('terminal-browser','Terminal browser','Browse, preview, and apply your profiles without leaving the terminal.','Guides',
 code('ghostty-wall tui')+
 section('controls','Find your way around', '<p>Type a key name and press Enter. The browser supports these controls:</p>'+table(['Key','Action'],[('<kbd>j</kbd> / <kbd>k</kbd>','Move through profiles'),('<kbd>tab</kbd>','Switch pane'),('<kbd>enter</kbd>','Preview the selected profile'),('<kbd>a</kbd>','Apply the previewed profile'),('<kbd>b</kbd>','Go back'),('<kbd>q</kbd>','Quit')]))+
 section('previews','Image previews','<p>Image previews use Ghostty’s Kitty graphics protocol when available. In other environments, the browser falls back to a text preview.</p>')+
 section('random','Random profiles',code('ghostty-wall tui --seed '+seed)+'<p>Pass a seed when browsing profiles that use random wallpaper selection. A seed is 32 bytes, written as 64 hex digits.</p>'),
 [('controls','Keyboard controls'),('previews','Image previews'),('random','Random profiles')])

commands=[('init','Initialize the managed directory and add the Ghostty integration hook.'),('init --dry-run','Preview initialization without writing files.'),('init --repair','Conservatively repair the managed layout and integration hook.'),('init --welcome','Add the bundled welcome profile to an eligible empty installation.'),('init --migrate-legacy','Import recognized wallpaper sources and integration from Bash v0.'),('plan PROFILE','Resolve a profile and inspect its environment without changing files.'),('apply PROFILE','Apply a profile and record an activation in local history.'),('previous','Replay the preceding environment from local history.'),('tui','Browse profiles, preview them, and apply them interactively.'),('doctor','Run read-only checks of installation, integration, and durable state.'),('uninstall','Remove integration and generated files, preserving profiles and history.'),('--help','Show CLI usage.'),('--version','Show the installed version.')]
page('commands','Command reference','Every command, at a glance. All commands start with ghostty-wall.','Reference',
 section('commands','The complete command set',table(['Command','What it does'],[(f'<code>ghostty-wall {escape(c)}</code>',d) for c,d in commands]))+
 section('flags','Useful flags',table(['Flag','Use'],[('<code>--seed HEX</code>','Use with <code>plan</code>, <code>apply</code>, and <code>tui</code> for random-selection profiles.'),('<code>plan PROFILE --json</code>','Compact machine-readable JSON. Errors also use structured JSON and a nonzero exit code.'),('<code>init --migrate-legacy --dry-run</code>','Inspect a legacy migration before making changes.')]))+
 section('examples','An everyday workflow',code('ghostty-wall plan night\nghostty-wall apply night\nghostty-wall previous\nghostty-wall doctor'))+
 note('There are no <code>next</code>, <code>history</code>, direct <code>random</code>/<code>set</code>, or automatic cleanup commands in v1.','Version 1 scope'),
 [('commands','All commands'),('flags','Useful flags'),('examples','Examples')])

page('configuration','Configuration reference','Manage the settings you care about. Leave everything else in your normal Ghostty configuration.','Reference',
 section('files','Know your files',table(['File or directory','Purpose'],[('<code>config.toml</code>','Source definitions and schema version.'),('<code>profiles/*.toml</code>','Your editable visual recipes.'),('<code>current.ghostty</code>','Generated managed configuration; do not edit directly.'),('<code>history/activations/</code>','Durable activation history; preserve it.'),('<code>assets/sha256/</code>','Saved image assets; preserve them.'),('<code>cache/</code>','Disposable cache.')]))+
 section('terminal','Terminal settings',code('''[terminal]
font_size = 13.5
background_opacity = 0.92
background_blur_intensity = 20
cursor_style = "bar"''','Profile · terminal','toml')+table(['Property','Accepted values'],[('<code>font_size</code>','1–1000, up to 3 decimal places.'),('<code>background_opacity</code>','0–1, up to 6 decimal places.'),('<code>background_blur_intensity</code>','Integer from 0–255.'),('<code>cursor_style</code>','<code>block</code>, <code>bar</code>, <code>underline</code>, <code>block_hollow</code>.')])+'<p>An empty <code>[terminal]</code> section is invalid. Omit the section when you do not want to manage any terminal settings.</p>')+
 section('opacity','Two different opacity settings','<p><code>background_opacity</code> controls the terminal background. <code>wallpaper.opacity</code> controls how the image is mixed in. Configure them independently.</p>')+
 section('unmanaged','What stays unmanaged','<p>Omitted fields are unmanaged. Font family, keybindings, minimum contrast, and other Ghostty settings remain in your normal Ghostty configuration.</p><p>Ghostty Wall manages selected visual settings through its integration hook; it does not replace your entire Ghostty config.</p>'),
 [('files','Files and directories'),('terminal','Terminal settings'),('opacity','Opacity'),('unmanaged','Unmanaged settings')])

page('troubleshooting','Maintenance & recovery','Check your setup, repair the integration, and keep your profiles and history safe.','Reference',
 section('doctor','Check your installation',code('ghostty-wall doctor')+'<p>The doctor runs read-only checks and reports results as verified, failed, or unavailable.</p>')+
 section('reload','My terminal did not change','<p>Reload is best-effort and happens after an activation is committed. Check that Ghostty’s config-file hook is active, run the doctor, and reload Ghostty manually if necessary.</p><p>On Linux, reload uses the active systemd user service or the running GTK application’s D-Bus action.</p>')+
 section('repair','Repair the integration',code('ghostty-wall init --repair')+'<p>This performs conservative layout and integration-hook repair. It cannot recreate lost profiles, images, environments, or history.</p>')+
 section('migration','Migrate from Bash v0',code('ghostty-wall init --migrate-legacy --dry-run\nghostty-wall init --migrate-legacy')+f'<p>Migration preserves legacy files and imports recognized sources. It cannot infer profiles: review the imported sources and create your profiles afterward. The previous Bash version remains at the <a href="{REPO}/tree/v0.2.2">v0.2.2 tag</a>.</p>')+
 section('uninstall','Uninstall',code('ghostty-wall uninstall')+'<p>This removes the integration hook and generated projection/cache. Your profiles, environments, assets, and history are preserved. Remove the binary separately using the installation method you originally used.</p>'+note('Only <code>cache/</code> is disposable. Images and activation history are durable data, not cache.','Keep your saved environments'))+
 section('help','Still need a hand?',f'<p><a href="{REPO}/issues">Open a GitHub issue</a> with the command you ran, your operating system, and the relevant error or doctor output.</p>'),
 [('doctor','Run the doctor'),('reload','Reload issues'),('repair','Repair'),('migration','Migrate from v0'),('uninstall','Uninstall'),('help','Get help')])

nav=[('Get started',[('overview','Introduction','book'),('installation','Installation','download'),('quick-start','Quick start','bolt')]),('Guides',[('profiles','Create a profile','layers'),('wallpapers','Wallpaper sources','image'),('colors','Colors & themes','palette'),('terminal-browser','Terminal browser','terminal')]),('Reference',[('commands','Commands','code'),('configuration','Configuration','settings'),('troubleshooting','Maintenance & recovery','life')])]
nav_flat=[(k,t) for _,entries in nav for k,t,_ in entries]
for key,p in pages.items():
    navigation=''.join(f'<div class="nav-group"><p>{group}</p>'+''.join(f'<a href="{path(k)}" class="nav-item {"active" if k==key else ""}" '+('aria-current="page"' if k==key else '')+f'>{icon(i)}<span>{title}</span></a>' for k,title,i in entries)+'</div>' for group,entries in nav)
    toc=''.join(f'<a href="#{id}">{title}</a>' for id,title in p['toc'])
    idx=[k for k,t in nav_flat].index(key)
    prev=nav_flat[idx-1] if idx>0 else None
    nxt=nav_flat[idx+1] if idx+1<len(nav_flat) else None
    pagination='<nav class="pagination" aria-label="Documentation pages">'
    pagination+=f'<a href="{path(prev[0])}"><small>Previous</small><span>← {prev[1]}</span></a>' if prev else '<div></div>'
    pagination+=f'<a href="{path(nxt[0])}" class="next"><small>Up next</small><span>{nxt[1]} →</span></a>' if nxt else '<div></div>'
    pagination+='</nav>'
    plain_title=p['title'].replace('<br>',' ').replace('<em>','').replace('</em>','')
    title='Introduction' if key=='overview' else plain_title
    intro_title=p['title']
    html=f'''<!doctype html>
<html lang="en"><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1"><meta name="theme-color" content="#0d132b"><meta name="description" content="{escape(p['desc'].replace('<br>',' '),quote=True)}"><title>{title} — Ghostty Wall</title><link rel="icon" type="image/png" href="/assets/mascot.png"><link rel="stylesheet" href="/styles.css"><script src="/app.js" defer></script><script src="/release.js" defer></script></head>
<body class="{key}"><a class="skip-link" href="#main">Skip to content</a>
<header class="site-header"><a class="brand" href="/" aria-label="Ghostty Wall home"><img src="/assets/mascot.png" alt="" width="36" height="36"><span>ghostty<span class="brand-dash">—</span>wall</span></a><span class="header-divider"></span><a class="header-docs" href="/">Documentation</a><div class="header-links"><a class="changelog" href="{REPO}/blob/main/CHANGELOG.md">Changelog {icon('arrow')}</a><a class="github-link" href="{REPO}">{icon('github')}<span>GitHub</span>{icon('arrow')}</a><button class="mobile-menu" type="button" aria-label="Open navigation" aria-expanded="false" aria-controls="sidebar">{icon('menu')}</button></div></header>
<aside class="sidebar" id="sidebar"><a class="version-pill" href="{REPO}/releases/latest" title="Open the latest stable release on GitHub"><span data-release-version aria-live="polite">Latest</span><small>Stable release</small>{icon('arrow')}</a><nav aria-label="Documentation">{navigation}</nav><div class="sidebar-bottom"><div class="open-source-label">{icon('heart')} Made for your terminal.</div><a href="{REPO}">Open source. Yours to make your own. {icon('arrow')}</a></div></aside><button class="drawer-backdrop" aria-label="Close navigation" tabindex="-1"></button>
<div class="content-layout"><main id="main" tabindex="-1"><div class="breadcrumb"><a href="/">Docs</a>{icon('chevron')}<span>{title}</span></div><div class="page-heading"><p class="eyebrow">{p['group']}</p><h1>{intro_title}</h1><p class="lead">{p['desc']}</p></div>{p['body']}{pagination}<footer class="page-footer"><span>Ghostty Wall · MIT License</span><a href="{REPO}/blob/main/docs/user-guide.md">Read the source guide {icon('arrow')}</a></footer></main><aside class="toc"><p>ON THIS PAGE</p><nav aria-label="On this page">{toc}</nav><div class="toc-help"><span>Building something good?</span><a href="{REPO}">Star us on GitHub {icon('arrow')}</a></div></aside></div><div id="copy-status" role="status" aria-live="polite"></div></body></html>'''
    target=OUT/'index.html' if key=='overview' else OUT/key/'index.html'
    target.parent.mkdir(exist_ok=True,parents=True)
    # Relative URLs work on both GitHub project Pages and local servers.
    prefix='./' if key=='overview' else '../'
    html=html.replace('href="/', f'href="{prefix}').replace('src="/', f'src="{prefix}')
    target.write_text(html)
print(f'Wrote {len(pages)} documentation pages.')
