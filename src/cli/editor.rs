//! Keyboard presentation for the shared in-memory editor. Live preview remains gated by RFC 0009.

use super::*;

#[cfg(test)]
mod tests;
use crate::{
    domain::{Color as ProfileColor, EnvironmentManifest, WallpaperSelection},
    profile_editor::{NumericControl, ProfileEditor, color_slots},
    profile_workflow::{ProfileWorkflows, WorkflowError},
    tui::sample::draw_sample,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListState, Paragraph, Wrap},
};

const EDIT_HELP: &str = "Usage: ghostty-wall edit [PROFILE]\n\nOpen a keyboard visual draft editor, or select a Profile when omitted.\nRequires an interactive terminal; a missing named Profile is an error.\nTab switches Wallpaper / Colors / Terminal. Up/Down selects a control.\nLeft/Right increments numbers or cycles choices; Enter opens exact numeric\nentry or a color picker. Color picker: arrows choose a sample, Enter accepts,\nh enters exact #RRGGBB, a resets one slot to Automatic.\nWallpaper: Replace image uses create's Downloads/Pictures picker (Enter prompts);\nGenerate another wallpaper is explicit. Customized colors survive replacement.\nGenerated colors: only edited slots become Customized; others stay Automatic.\nCustomizing version 1 generated colors saves as version 2.\nUnmanaged colors stay unmanaged until you explicitly enable automatic colors.\n\ns opens Save and use confirmation (default: Back to editor); declining keeps\nthe draft. Esc/q cancels. p toggles the internal sample on small terminals.\nNo Profile, Projection or History writes happen before confirmed Save and use.\nThe internal sample is approximate, NOT live Ghostty reload. Save and apply\nare separate transactions; reload acceptance is not proof of visible change.\nA failed save retains the draft; uncertain publication requires inspection.\n\nAdvanced: edit PROFILE FIELD VALUE saves immediately without applying.\nSee the user guide for supported field names. update updates the program only.\n";

pub(super) fn command(args: &[String], output: &mut impl Write) -> Result<(), CliError> {
    let report = flow(args, output, false)?;
    write_text(output, &report)
}

pub(super) fn flow(
    args: &[String],
    output: &mut impl Write,
    embedded: bool,
) -> Result<String, CliError> {
    if matches!(args, [flag] if flag == "--help" || flag == "-h") {
        return Ok(EDIT_HELP.into());
    }
    let paths = process_paths()?;
    let workflow = ProfileWorkflows::load(paths.clone())?;
    // Validate a supplied target before opening the terminal or asking any questions.
    let supplied = args.first().map(|name| workflow.edit(name)).transpose()?;
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(CliError::Input("Visual edit requires an interactive terminal; no files changed. See edit --help (advanced FIELD VALUE is non-interactive).".into()));
    }
    let draft = match supplied {
        Some(draft) => draft,
        None => {
            let profiles = profile_ids(&paths.managed_root().join("profiles"))?;
            let Some(id) = select_profile(output, &profiles)? else {
                return Ok("Cancelled; no Profile saved.\n".into());
            };
            workflow.edit(id.as_str())?
        }
    };
    let profile =
        parse_named_profile_toml(draft.id().as_str(), workflow.config(), draft.document())?.1;
    let seed = if matches!(
        profile.wallpaper,
        Some(WallpaperIntent::Source {
            selection: WallpaperSelection::Random,
            ..
        })
    ) {
        Some(ResolutionSeed::from_str(&random_seed()?.to_string())?)
    } else {
        None
    };
    let application = Application::load(seed)?;
    let plan = application.plan(draft.id())?;
    let image = if uses_github_plan(&plan) {
        planned_github_asset_bytes(&plan, &application.github)
    } else {
        planned_local_asset_bytes(&plan)
    }
    .map_err(CliError::Plan)?;
    let manifest =
        crate::codec::manifest::decode(&serde_json::to_vec(&plan["environment"]["manifest"])?)
            .map_err(|e| CliError::Intent(e.to_string()))?;
    if application.config != *workflow.config()
        || workflow.edit(draft.id().as_str())?.document() != draft.document()
    {
        return Err(WorkflowError::Changed(draft.id().clone()).into());
    }
    let history = crate::history::inspect_history(&paths.managed_root())
        .map_err(|e| CliError::Intent(e.to_string()))?;
    let mut editor = ProfileEditor::new(
        draft,
        workflow.config().clone(),
        manifest,
        image,
        history.latest().cloned(),
    )?;
    if !edit_loop(output, &paths, &workflow, &mut editor)? {
        return Ok("Cancelled; draft discarded. No Profile saved; terminal and History unchanged by editor.\n".into());
    }
    let id = editor.draft().id();
    let mut report = Vec::new();
    if embedded {
        writeln!(report, "Saved Profile {id}. Applying separately...")?;
    } else {
        writeln!(output, "Saved Profile {id}. Applying separately...")?;
    }
    let outcome = workflow.use_saved(id, |id| {
        let still_random = matches!(
            editor.intent()?.wallpaper,
            Some(WallpaperIntent::Source {
                selection: WallpaperSelection::Random,
                ..
            })
        );
        Application::load(if still_random { seed } else { None })?.apply(id)
    })?;
    if let crate::profile_workflow::ProfileOutcome::SavedAndApplied { activation, reload } = outcome
    {
        writeln!(report, "Activated {activation} for Profile {id}.")?;
        let status = match reload {
            crate::runtime::ReloadOutcome::Succeeded => {
                "action accepted; visible change is not verified"
            }
            crate::runtime::ReloadOutcome::Unavailable(_) => {
                "unavailable; Activation remains committed"
            }
            crate::runtime::ReloadOutcome::Failed(_) => "failed; Activation remains committed",
        };
        writeln!(report, "Ghostty reload: {status}.")?;
    }
    Ok(String::from_utf8_lossy(&report).into_owned())
}

pub(super) fn random_seed() -> Result<crate::domain::Sha256Digest, CliError> {
    let mut bytes = [0; 32];
    fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(crate::domain::Sha256Digest::from_bytes(bytes))
}

pub(super) struct Screen<'a, W: Write> {
    pub(super) terminal: Terminal<CrosstermBackend<&'a mut W>>,
}
impl<'a, W: Write> Screen<'a, W> {
    pub(super) fn open(output: &'a mut W) -> Result<Self, CliError> {
        let mut terminal = Terminal::new(CrosstermBackend::new(output))?;
        execute!(terminal.backend_mut(), EnterAlternateScreen)?;
        if let Err(e) = enable_raw_mode() {
            let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen);
            return Err(e.into());
        }
        Ok(Self { terminal })
    }
}
impl<W: Write> Drop for Screen<'_, W> {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            self.terminal.backend_mut(),
            LeaveAlternateScreen,
            crossterm::cursor::Show
        );
    }
}
pub(super) fn key() -> Result<KeyEvent, CliError> {
    loop {
        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => return Ok(key),
            Event::Resize(_, _) => return Ok(KeyEvent::from(KeyCode::Null)),
            _ => (),
        }
    }
}
pub(super) fn interrupt(key: KeyEvent) -> bool {
    key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c' | 'd'))
}
fn select_profile(
    output: &mut impl Write,
    profiles: &[IntentId],
) -> Result<Option<IntentId>, CliError> {
    if profiles.is_empty() {
        return Err(CliError::Intent(
            "No Profiles to edit; no files changed. Run ghostty-wall create first.".into(),
        ));
    }
    let mut screen = Screen::open(output)?;
    let mut index = 0;
    loop {
        screen.terminal.draw(|frame| {
            let list = List::new(profiles.iter().map(|id| id.as_str()))
                .block(
                    Block::default()
                        .title("Select Profile to edit · Enter open · Esc cancel")
                        .borders(Borders::ALL),
                )
                .highlight_style(selected_style())
                .highlight_symbol("> ");
            frame.render_stateful_widget(
                list,
                frame.area(),
                &mut ListState::default().with_selected(Some(index)),
            );
        })?;
        let key = key()?;
        if interrupt(key) {
            return Ok(None);
        }
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => return Ok(None),
            KeyCode::Up => index = index.saturating_sub(1),
            KeyCode::Down => index = (index + 1).min(profiles.len() - 1),
            KeyCode::Enter => return Ok(Some(profiles[index].clone())),
            _ => (),
        }
    }
}

#[derive(Clone, Copy)]
enum Control {
    Image,
    Generate,
    AutomaticColors,
    Color(usize),
    Number(NumericControl),
    Choice(&'static str, &'static str, &'static [&'static str]),
    Save,
}
fn controls(group: usize) -> Vec<(String, Control)> {
    let mut rows: Vec<(String, Control)> = match group {
        0 => vec![
            (
                "Replace image (Downloads / Pictures)".into(),
                Control::Image,
            ),
            ("Generate another wallpaper".into(), Control::Generate),
            (
                "Image fit".into(),
                Control::Choice("wallpaper", "fit", &["contain", "cover", "stretch", "none"]),
            ),
            (
                "Image position".into(),
                Control::Choice(
                    "wallpaper",
                    "position",
                    &[
                        "top-left",
                        "top-center",
                        "top-right",
                        "center-left",
                        "center",
                        "center-right",
                        "bottom-left",
                        "bottom-center",
                        "bottom-right",
                    ],
                ),
            ),
            (
                "Repeat image".into(),
                Control::Choice("wallpaper", "repeat", &["false", "true"]),
            ),
            (
                "Wallpaper-image opacity".into(),
                Control::Number(NumericControl::WallpaperOpacity),
            ),
        ],
        1 => {
            let mut colors: Vec<_> = [
                "Background",
                "Foreground",
                "Cursor",
                "Selection background",
                "Selection text",
            ]
            .into_iter()
            .enumerate()
            .map(|(i, name)| (name.into(), Control::Color(i)))
            .collect();
            colors.extend((0..16).map(|i| (format!("ANSI palette {i}"), Control::Color(i + 5))));
            colors.push((
                "Reset ALL colors to Automatic (confirm)".into(),
                Control::AutomaticColors,
            ));
            colors
        }
        _ => vec![
            (
                "Terminal background opacity".into(),
                Control::Number(NumericControl::BackgroundOpacity),
            ),
            (
                "Font size (points)".into(),
                Control::Number(NumericControl::FontSize),
            ),
            (
                "Cursor style".into(),
                Control::Choice(
                    "terminal",
                    "cursor_style",
                    &["block", "bar", "underline", "block_hollow"],
                ),
            ),
            (
                "Background blur (platform-dependent)".into(),
                Control::Number(NumericControl::Blur),
            ),
        ],
    };
    rows.push(("Save and use".into(), Control::Save));
    rows
}

#[derive(Default)]
enum Mode {
    #[default]
    Browse,
    Number(NumericControl, String),
    Color(usize, usize),
    Hex(usize, String),
    Confirm(bool),
    ResetColors,
}
#[derive(Default)]
struct View {
    group: usize,
    row: usize,
    mode: Mode,
    status: String,
    sample: bool,
}

fn edit_loop(
    output: &mut impl Write,
    paths: &InitPaths,
    workflow: &ProfileWorkflows,
    editor: &mut ProfileEditor,
) -> Result<bool, CliError> {
    let mut view = View::default();
    let mut screen = Screen::open(output)?;
    let mut thumbnail = thumbnail(editor);
    let mut uncertain_save: Option<WorkflowError> = None;
    loop {
        let manifest = editor.preview()?;
        let rows = controls(view.group);
        screen
            .terminal
            .draw(|frame| draw(frame, editor, &manifest, &thumbnail, &view, &rows))?;
        let key = key()?;
        if interrupt(key) {
            return cancel_result(uncertain_save);
        }
        let size = screen.terminal.size()?;
        if tui::needs_resize(size.width, size.height) {
            if key.code == KeyCode::Esc {
                return cancel_result(uncertain_save);
            }
            continue;
        }
        let result: Result<(), WorkflowError> = match &mut view.mode {
            Mode::Confirm(use_now) => {
                match key.code {
                    KeyCode::Left | KeyCode::Right | KeyCode::Tab => *use_now = !*use_now,
                    KeyCode::Char('y') => *use_now = true,
                    KeyCode::Esc | KeyCode::Char('n') => {
                        view.mode = Mode::Browse;
                        view.status = "Back to editor; draft intact.".into();
                        continue;
                    }
                    _ => (),
                }
                if key.code == KeyCode::Enter || key.code == KeyCode::Char('y') {
                    if !*use_now {
                        view.mode = Mode::Browse;
                        view.status = "Back to editor; draft intact.".into();
                        continue;
                    }
                    if uncertain_save.is_some() {
                        view.mode = Mode::Browse;
                        view.status = "Save blocked: inspect uncertain publication before reopening; draft retained until exit.".into();
                        continue;
                    }
                    match workflow.save_draft(editor.draft()) {
                        Ok(_) => return Ok(true),
                        Err(error) => {
                            view.mode = Mode::Browse;
                            if matches!(
                                error,
                                WorkflowError::PublicationUncertain { .. }
                                    | WorkflowError::RollbackIncomplete { .. }
                            ) {
                                view.status =
                                    format!("{error}. Draft retained; no apply attempted.");
                                uncertain_save = Some(error);
                                continue;
                            }
                            Err(error)
                        }
                    }
                } else {
                    Ok(())
                }
            }
            Mode::ResetColors => {
                if key.code == KeyCode::Char('y') {
                    view.mode = Mode::Browse;
                    editor.automatic_colors()
                } else {
                    if matches!(key.code, KeyCode::Enter | KeyCode::Esc | KeyCode::Char('n')) {
                        view.mode = Mode::Browse;
                    }
                    Ok(())
                }
            }
            Mode::Number(control, input) => {
                if key.code == KeyCode::Esc {
                    view.mode = Mode::Browse;
                    Ok(())
                } else if key.code == KeyCode::Enter {
                    let result = editor.set_number(*control, input);
                    if result.is_ok() {
                        view.mode = Mode::Browse;
                    }
                    result
                } else {
                    edit_text(input, key.code);
                    Ok(())
                }
            }
            Mode::Hex(slot, input) => {
                if key.code == KeyCode::Esc {
                    view.mode = Mode::Browse;
                    Ok(())
                } else if key.code == KeyCode::Enter {
                    let result = editor.set_color(*slot, input);
                    if result.is_ok() {
                        view.mode = Mode::Browse;
                    }
                    result
                } else {
                    edit_text(input, key.code);
                    Ok(())
                }
            }
            Mode::Color(slot, index) => {
                let samples = samples(&manifest);
                match key.code {
                    KeyCode::Esc => {
                        view.mode = Mode::Browse;
                        Ok(())
                    }
                    KeyCode::Left | KeyCode::Up => {
                        *index = index.saturating_sub(1);
                        Ok(())
                    }
                    KeyCode::Right | KeyCode::Down => {
                        *index = (*index + 1).min(samples.len() - 1);
                        Ok(())
                    }
                    KeyCode::Char('h') => {
                        view.mode = Mode::Hex(*slot, String::new());
                        Ok(())
                    }
                    KeyCode::Char('a') => {
                        let result = editor.set_color(*slot, "auto");
                        if result.is_ok() {
                            view.mode = Mode::Browse;
                        }
                        result
                    }
                    KeyCode::Enter => {
                        let result = editor.set_color(*slot, &samples[*index].to_string());
                        if result.is_ok() {
                            view.mode = Mode::Browse;
                        }
                        result
                    }
                    _ => Ok(()),
                }
            }
            Mode::Browse => {
                match key.code {
                    KeyCode::Esc | KeyCode::Char('q') => return cancel_result(uncertain_save),
                    KeyCode::Tab => {
                        view.group = (view.group + 1) % 3;
                        view.row = 0;
                    }
                    KeyCode::BackTab => {
                        view.group = (view.group + 2) % 3;
                        view.row = 0;
                    }
                    KeyCode::Up | KeyCode::Char('k') => view.row = view.row.saturating_sub(1),
                    KeyCode::Down | KeyCode::Char('j') => {
                        view.row = (view.row + 1).min(rows.len() - 1)
                    }
                    KeyCode::Home => view.row = 0,
                    KeyCode::End => view.row = rows.len() - 1,
                    KeyCode::Char('s') => view.mode = Mode::Confirm(false),
                    KeyCode::Char('p') => view.sample = !view.sample,
                    KeyCode::Char('v') => {
                        drop(screen);
                        management::details(output, &view.status)?;
                        screen = Screen::open(output)?;
                    }
                    _ => (),
                }
                let control = rows[view.row].1;
                let enter = key.code == KeyCode::Enter;
                let step = matches!(
                    key.code,
                    KeyCode::Left | KeyCode::Right | KeyCode::Char('-' | '+')
                );
                let increase = matches!(key.code, KeyCode::Right | KeyCode::Char('+'));
                match control {
                    Control::Save if enter => {
                        view.mode = Mode::Confirm(false);
                        Ok(())
                    }
                    Control::AutomaticColors if enter => {
                        view.mode = Mode::ResetColors;
                        Ok(())
                    }
                    Control::Color(slot) if enter => {
                        view.mode = Mode::Color(slot, 0);
                        Ok(())
                    }
                    Control::Number(control) if enter => {
                        view.mode = Mode::Number(control, String::new());
                        Ok(())
                    }
                    Control::Number(control) if step => editor.step_number(control, increase),
                    Control::Choice(section, field, values) if step || enter => {
                        let current = choice_value(editor, section, field);
                        let index = current
                            .as_deref()
                            .and_then(|s| values.iter().position(|v| *v == s));
                        let index = match index {
                            Some(i) if increase || enter => (i + 1) % values.len(),
                            Some(i) => (i + values.len() - 1) % values.len(),
                            None => 0,
                        };
                        editor.set_choice(section, field, values[index])
                    }
                    Control::Image if enter => {
                        drop(screen);
                        let stdin = io::stdin();
                        let result =
                            pick_image_with(paths, &mut stdin.lock().lines(), output, |path| {
                                editor.import_image(workflow, path)
                            });
                        screen = Screen::open(output)?;
                        result?;
                        thumbnail = self::thumbnail(editor);
                        Ok(())
                    }
                    Control::Generate if enter => {
                        let result = editor.generate_image(workflow, random_seed()?);
                        if result.is_ok() {
                            thumbnail = self::thumbnail(editor);
                        }
                        result
                    }
                    _ => Ok(()),
                }
            }
        };
        if let Err(error) = result {
            view.status = format!("Draft retained. {error}");
        }
    }
}

// Cancellation must not relabel an uncertain earlier save as "no files changed".
fn cancel_result(uncertain: Option<WorkflowError>) -> Result<bool, CliError> {
    match uncertain {
        Some(error) => Err(error.into()),
        None => Ok(false),
    }
}

fn edit_text(input: &mut String, key: KeyCode) {
    match key {
        KeyCode::Backspace => {
            input.pop();
        }
        KeyCode::Char(c) if input.len() < 32 => input.push(c),
        _ => (),
    }
}
fn choice_value(editor: &ProfileEditor, section: &str, key: &str) -> Option<String> {
    let doc: toml_edit::DocumentMut = editor.draft().document().parse().ok()?;
    let value = doc.get(section)?.get(key)?;
    value
        .as_str()
        .map(str::to_owned)
        .or_else(|| value.as_bool().map(|v| v.to_string()))
}
fn rgb(color: ProfileColor) -> Color {
    let [r, g, b] = color.as_rgb();
    Color::Rgb(r, g, b)
}
fn selected_style() -> Style {
    Style::default().fg(Color::Black).bg(Color::Cyan)
}
fn samples(manifest: &EnvironmentManifest) -> Vec<ProfileColor> {
    manifest
        .colors()
        .map(|c| color_slots(c).into_iter().flatten().collect())
        .unwrap_or_else(|| {
            ["000000", "ffffff", "cc5555", "55cc55", "5555cc"]
                .iter()
                .filter_map(|s| s.parse().ok())
                .collect()
        })
}
fn thumbnail(editor: &ProfileEditor) -> Option<image::RgbImage> {
    editor
        .image()
        .and_then(|bytes| image::load_from_memory(bytes).ok())
        .map(|image| image.thumbnail(40, 16).to_rgb8())
}
fn draw(
    frame: &mut Frame,
    editor: &ProfileEditor,
    manifest: &EnvironmentManifest,
    thumbnail: &Option<image::RgbImage>,
    view: &View,
    rows: &[(String, Control)],
) {
    if tui::resize_notice(frame) {
        return;
    }
    // Editor chrome always uses its own readable colors, not potentially unreadable draft colors.
    frame.render_widget(
        Block::default().style(Style::default().fg(Color::White).bg(Color::Black)),
        frame.area(),
    );
    let wide = frame.area().width >= 100 && frame.area().height >= 18;
    let compact_sample = !wide && view.sample && matches!(view.mode, Mode::Browse);
    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(if wide { 3 } else { 2 }),
            Constraint::Min(1),
            Constraint::Length(if compact_sample {
                2
            } else if wide {
                4
            } else {
                3
            }),
        ])
        .split(frame.area());
    let group = ["Wallpaper", "Colors", "Terminal"][view.group];
    frame.render_widget(
        Paragraph::new(format!(
            "Edit Profile {} · {group}\n{}",
            editor.draft().id(),
            if wide {
                "Tab: Wallpaper / Colors / Terminal · s: Save and use"
            } else {
                "Tab: Wallpaper / Colors / Terminal"
            }
        )),
        areas[0],
    );
    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(58), Constraint::Percentage(42)])
        .split(areas[1]);
    if wide || !view.sample {
        let colors = manifest.colors().map(color_slots).unwrap_or_default();
        let list_area = if wide { panes[0] } else { areas[1] };
        let items: Vec<_> = rows
            .iter()
            .map(|(label, control)| {
                let suffix = match control {
                    Control::Number(control) => editor
                        .number_label(*control)
                        .unwrap_or_else(|_| "Unavailable".into()),
                    Control::Choice(section, key, _) => choice_value(editor, section, key)
                        .map(|s| s.replace(['-', '_'], " "))
                        .unwrap_or_else(|| "Unmanaged".into()),
                    Control::Color(slot) => format!(
                        "{} {}",
                        colors
                            .get(*slot)
                            .and_then(|c| *c)
                            .map(|c| format!("#{c}"))
                            .unwrap_or_else(|| "Unmanaged".into()),
                        editor.color_origin(*slot).unwrap_or("")
                    ),
                    _ => String::new(),
                };
                tui::wrapped_item(
                    &format!("{label}  {suffix}"),
                    list_area.width.saturating_sub(4),
                )
            })
            .collect();
        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title(group))
            .highlight_symbol("> ")
            .highlight_style(selected_style());
        frame.render_stateful_widget(
            list,
            list_area,
            &mut ListState::default().with_selected(Some(view.row)),
        );
    }
    if wide || view.sample {
        draw_sample(
            frame,
            if wide { panes[1] } else { areas[1] },
            manifest,
            thumbnail,
        );
    }
    let footer = if wide {
        format!(
            "↑↓ select · ←→ adjust · Enter edit · Esc/q cancel · p sample\nInternal preview only — NOT live Ghostty reload\n{}",
            view.status
        )
    } else if compact_sample {
        "p controls · s Save · Esc/q cancel\nInternal; NOT live Ghostty reload".into()
    } else {
        format!(
            "↑↓ select · ←→ adjust · Enter edit\ns Save · p sample · Esc/q cancel\nv: {}",
            if view.status.is_empty() {
                "NOT live Ghostty reload"
            } else {
                &view.status
            }
        )
    };
    frame.render_widget(Paragraph::new(footer).wrap(Wrap { trim: false }), areas[2]);
    let popup = match &view.mode {
        Mode::Browse => return,
        Mode::Confirm(yes) => format!("Save and use Profile {}?\nSave changes, then apply once.\n{}\n←→ choose · Enter confirm · y yes · n/Esc back", editor.draft().id(), if *yes { "Back to editor    [Save and use]" } else { "[Back to editor]    Save and use" }),
        Mode::ResetColors => "Reset ALL colors to Automatic?\nThis discards customized colors in the draft.\ny reset · Enter/n/Esc back (default)".into(),
        Mode::Number(control, input) => format!("Exact number: {}\n{input}_\nEnter accept · Esc back · invalid input keeps draft", match control { NumericControl::FontSize => "1..1000 points; up to 3 decimals", NumericControl::Blur => "0..255 integer", _ => "0..1; up to 6 decimals" }),
        Mode::Hex(_, input) => format!("Exact hex color (#RRGGBB)\n{input}_\nEnter accept · Esc back"),
        Mode::Color(_, _) => "Color samples · arrows select · Enter accepts\nh exact hex · a Automatic · Esc back".into(),
    };
    let rect = Rect::new(
        areas[1].x,
        if wide { areas[1].y } else { frame.area().y },
        areas[1].width,
        if wide {
            areas[1].height.min(9)
        } else {
            frame.area().height.saturating_sub(3)
        },
    );
    frame.render_widget(Clear, rect);
    frame.render_widget(
        Paragraph::new(popup)
            .style(Style::default().fg(Color::White).bg(Color::Black))
            .block(Block::default().borders(Borders::ALL))
            .wrap(Wrap { trim: false }),
        rect,
    );
    if let Mode::Color(_, selected) = view.mode {
        let samples = samples(manifest);
        let c = samples[selected];
        let area = Rect::new(
            rect.x + 1,
            rect.y + 4,
            rect.width.saturating_sub(2),
            rect.height.saturating_sub(4),
        );
        frame.render_widget(
            Paragraph::new(vec![Line::from(vec![
                Span::styled(
                    "    SAMPLE    ",
                    Style::default().bg(rgb(c)).fg(Color::Black),
                ),
                Span::raw(format!(" #{c} ({}/{})", selected + 1, samples.len())),
            ])]),
            area,
        );
    }
}
