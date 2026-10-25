//! Keyboard presentation for the shared in-memory editor. Live preview remains gated by RFC 0009.

use super::*;

mod preview;
#[cfg(test)]
mod tests;
use crate::{
    domain::{Color as ProfileColor, EnvironmentManifest, WallpaperSelection},
    profile_editor::{NumericControl, ProfileEditor, color_slots},
    profile_workflow::{ProfileWorkflows, WorkflowError},
    tui::sample::draw_sample,
};
use crossterm::{
    event::{KeyCode, KeyEvent, KeyModifiers},
    terminal::{BeginSynchronizedUpdate, EndSynchronizedUpdate},
};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListState, Paragraph, Wrap},
};

const EDIT_HELP: &str = "Usage: ghostty-wall edit [PROFILE]\n\nOpen a keyboard visual draft editor, or select a Profile when omitted.\nRequires an interactive terminal; a missing named Profile is an error.\nTab switches Wallpaper / Colors / Terminal; the active group is highlighted.\nAt 100x18 or larger, a dominant preview sits left of contextual controls.\nGhostty uses the browser renderer with a reduced source; other terminals use\nan explicit color-cell fallback. Rendering runs off the input loop.\nChrome uses fixed dark RGB, independent of terminal ANSI colors.\nClear/redraw transitions use synchronized output where supported.\nPreview images remain asynchronous, not live reload.\nEditor headings/actions are emphasized; navigation hints stay secondary.\nWindow opacity, wallpaper, padding and compositor effects remain terminal-owned.\nControl position and Home/End hints expose scrolling; v explains limitations,\neven before the preview is prepared.\nUp/Down selects a control.\nLeft/Right increments numbers or cycles choices; Enter opens exact numeric\nentry or a color picker. Color picker: arrows choose a sample, Enter accepts,\nh enters exact #RRGGBB, a resets one slot to Automatic.\nWallpaper: Replace image opens a full-screen Downloads/Pictures picker;\nGenerate another wallpaper is explicit. Customized colors survive replacement.\nImage blur: reversible sigma 0..32 at 512px short edge, not compositor blur.\nImage darkening: visible 0..100% linear-light black overlay, not opacity.\nZero effects restores original bytes. Auto darkening recalculates the minimum.\nInfeasible contrast is explicit; fallback is NOT 4.5:1. New palettes use oklch-v2; existing Profiles keep their algorithm\nunless Reset ALL colors is explicitly confirmed.\nGenerated colors: only edited slots become Customized; others stay Automatic.\nCustomizing version 1 generated colors saves as version 2.\nUnmanaged colors stay unmanaged until you explicitly enable automatic colors.\n\ns saves without applying. u opens Save and use confirmation\n(default: Back to editor); declining keeps the draft. Esc/q cancels. p switches Controls / Preview\non small terminals. Save and Save and use publish the Profile; Cancel\nwrites nothing. Save alone never changes Projection, History or reloads.\nThe internal sample is approximate, NOT live Ghostty reload. Save and apply\nare separate transactions; reload acceptance is not proof of visible change.\nA failed save retains the draft; uncertain publication requires inspection.\n\nAdvanced: edit PROFILE FIELD VALUE saves immediately without applying.\nSee the user guide for supported field names. update updates the program only.\n";

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
    // Internal editing is read-only: durable-state validation belongs to Use,
    // not opening or cancelling a draft.
    let plan = crate::plan::plan_profile_preview_json(
        &paths.managed_root(),
        &paths.home,
        &paths.managed_root(),
        draft.id(),
        &application.config,
        &profile,
        application.seed.as_ref(),
        &application.github,
        &application.themes,
    )
    .map_err(CliError::Plan)?;
    let image = crate::plan::planned_original_asset_bytes(&plan, &application.github)
        .map_err(CliError::Plan)?;
    let manifest =
        crate::codec::manifest::decode(&serde_json::to_vec(&plan["environment"]["manifest"])?)
            .map_err(|e| CliError::Intent(e.to_string()))?;
    if application.config != *workflow.config()
        || workflow.edit(draft.id().as_str())?.document() != draft.document()
    {
        return Err(WorkflowError::Changed(draft.id().clone()).into());
    }
    let mut editor = ProfileEditor::new(
        draft,
        workflow.config().clone(),
        manifest,
        image,
        None, // Internal mode never restores a starting Activation.
    )?;
    let completion = edit_loop(output, &paths, &workflow, &mut editor)?;
    if completion == Completion::Cancel {
        return Ok("Cancelled; draft discarded. No Profile saved; terminal and History unchanged by editor.\n".into());
    }
    let id = editor.draft().id();
    if completion == Completion::Save {
        return Ok(saved_report(id));
    }
    let saved = format!("Saved Profile {id}. Applying separately...\n");
    if !embedded {
        write_text(output, &saved)?;
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
    Ok(format!("{}{saved}", completion_report(id, outcome)))
}

pub(super) fn random_seed() -> Result<crate::domain::Sha256Digest, CliError> {
    let mut bytes = [0; 32];
    fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(crate::domain::Sha256Digest::from_bytes(bytes))
}

thread_local! {
    static SCREEN_DEPTH: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

pub(super) struct Screen<'a, W: Write> {
    pub(super) terminal: Terminal<CrosstermBackend<&'a mut W>>,
}
impl<'a, W: Write> Screen<'a, W> {
    pub(super) fn open(output: &'a mut W) -> Result<Self, CliError> {
        let mut terminal = Terminal::new(CrosstermBackend::new(output))?;
        if SCREEN_DEPTH.get() == 0 {
            execute!(terminal.backend_mut(), EnterAlternateScreen)?;
            if let Err(e) = enable_raw_mode() {
                let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen);
                return Err(e.into());
            }
        }
        SCREEN_DEPTH.set(SCREEN_DEPTH.get() + 1);
        let mut screen = Self { terminal };
        screen.redraw()?;
        Ok(screen)
    }

    pub(super) fn redraw(&mut self) -> Result<(), CliError> {
        let size = self.terminal.size()?;
        // Resize clears the terminal. Keep that intermediate blank state hidden
        // until the next draw, including when returning from a nested screen.
        execute!(self.terminal.backend_mut(), BeginSynchronizedUpdate)?;
        self.terminal.resize(size.into())?;
        Ok(())
    }

    pub(super) fn draw(&mut self, render: impl FnOnce(&mut Frame)) -> Result<(), CliError> {
        execute!(self.terminal.backend_mut(), BeginSynchronizedUpdate)?;
        let result = self.terminal.draw(render).map(|_| ());
        let finish = execute!(self.terminal.backend_mut(), EndSynchronizedUpdate);
        result?;
        finish?;
        Ok(())
    }
}
impl<W: Write> Drop for Screen<'_, W> {
    fn drop(&mut self) {
        let _ = execute!(self.terminal.backend_mut(), EndSynchronizedUpdate);
        SCREEN_DEPTH.set(SCREEN_DEPTH.get() - 1);
        if SCREEN_DEPTH.get() != 0 {
            return;
        }
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
        screen.draw(|frame| {
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
    AutomaticDarkening,
    Color(usize),
    Number(NumericControl),
    Choice(&'static str, &'static str, &'static [&'static str]),
    Complete(Completion),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Completion {
    Save,
    SaveAndUse,
    Cancel,
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
            (
                "Image blur (0–32)".into(),
                Control::Number(NumericControl::ImageBlur),
            ),
            (
                "Image darkening (0–100%)".into(),
                Control::Number(NumericControl::ImageDarken),
            ),
            (
                "Auto darkening (Automatic roles)".into(),
                Control::AutomaticDarkening,
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
    rows.extend([
        ("Save".into(), Control::Complete(Completion::Save)),
        (
            "Save and use".into(),
            Control::Complete(Completion::SaveAndUse),
        ),
        ("Cancel".into(), Control::Complete(Completion::Cancel)),
    ]);
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
    preview_status: String,
}

fn edit_loop(
    output: &mut impl Write,
    paths: &InitPaths,
    workflow: &ProfileWorkflows,
    editor: &mut ProfileEditor,
) -> Result<Completion, CliError> {
    let mut screen = Screen::open(output)?;
    editor.defer_effects_to_preview_worker();
    let result = edit_screen(&mut screen, paths, workflow, editor);
    if TerminalGraphics::from_environment() == TerminalGraphics::Ghostty {
        let cleanup = management::clear_image(screen.terminal.backend_mut());
        if result.is_ok() {
            cleanup?;
        }
    }
    result
}

fn edit_screen<W: Write>(
    screen: &mut Screen<'_, W>,
    paths: &InitPaths,
    workflow: &ProfileWorkflows,
    editor: &mut ProfileEditor,
) -> Result<Completion, CliError> {
    let mut view = View::default();
    let mut preview = preview::Preview::new()?;
    let graphics = TerminalGraphics::from_environment() == TerminalGraphics::Ghostty;
    let mut image_area = None;
    let mut last_size = None;
    let mut redraw = true;
    let mut uncertain_save: Option<WorkflowError> = None;
    loop {
        let manifest = editor.preview()?;
        let size = screen.terminal.size()?;
        redraw |= last_size != Some(size);
        last_size = Some(size);
        let area = preview_area(Rect::new(0, 0, size.width, size.height), &view);
        let requested = if let Some(area) = area {
            preview.request(editor, &manifest, area)?
        } else {
            false
        };
        let completed = preview.poll();
        redraw |= requested || completed;
        if image_area.is_some() && (requested || completed || image_area != area) {
            management::clear_image(screen.terminal.backend_mut())?;
            image_area = None;
        }
        view.preview_status.clone_from(&preview.status);
        let rows = controls(view.group);
        if redraw {
            screen.draw(|frame| draw(frame, editor, &manifest, &preview.sample, &view, &rows))?;
            redraw = false;
        }
        if graphics
            && image_area.is_none()
            && let (Some(sample), Some(area)) = (&preview.sample, area)
            && sample.render_graphics(screen.terminal.backend_mut(), area)?
        {
            image_area = Some(area);
        }
        if !event::poll(std::time::Duration::from_millis(16))? {
            continue;
        }
        let key = key()?;
        redraw = true;
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
        let mut completion = None;
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
                    completion = Some(Completion::SaveAndUse);
                }
                Ok(())
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
                    KeyCode::Char('s') => completion = Some(Completion::Save),
                    KeyCode::Char('u') => view.mode = Mode::Confirm(false),
                    KeyCode::Char('p') => view.sample = !view.sample,
                    KeyCode::Char('v') => {
                        if image_area.take().is_some() {
                            management::clear_image(screen.terminal.backend_mut())?;
                        }
                        let guidance = match &preview.sample {
                            Some(sample) => sample.guidance(),
                            None => {
                                let contrast = if editor.palette_contrast_infeasible()? {
                                    "\nAutomatic contrast infeasible: non-conforming fallback, NOT a valid 4.5:1 palette. Manual overrides remain unchanged and unchecked."
                                } else {
                                    ""
                                };
                                format!(
                                    "Preview not prepared. On compact layouts, return to editor and press p to open Preview. No sampled contrast estimate is available.\nStatic internal sample, not live Ghostty reload. Window opacity, wallpaper, padding and compositor effects remain terminal-owned.{contrast}"
                                )
                            }
                        };
                        management::details(
                            screen.terminal.backend_mut(),
                            &format!("{}\n{}\n{}", view.status, preview.status, guidance),
                        )?;
                        screen.redraw()?;
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
                    Control::Complete(Completion::Cancel) if enter => {
                        return cancel_result(uncertain_save);
                    }
                    Control::Complete(Completion::SaveAndUse) if enter => {
                        view.mode = Mode::Confirm(false);
                        Ok(())
                    }
                    Control::Complete(Completion::Save) if enter => {
                        completion = Some(Completion::Save);
                        Ok(())
                    }
                    Control::AutomaticDarkening if enter => editor.automatic_darkening(),
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
                        if image_area.take().is_some() {
                            management::clear_image(screen.terminal.backend_mut())?;
                        }
                        forms::image(screen.terminal.backend_mut(), paths, |path| {
                            editor.import_image(workflow, path)
                        })?;
                        screen.redraw()?;
                        Ok(())
                    }
                    Control::Generate if enter => editor.generate_image(workflow, random_seed()?),
                    _ => Ok(()),
                }
            }
        };
        if let Err(error) = result {
            view.status = format!("Draft retained. {error}");
        }
        if let Some(completion) = completion {
            view.mode = Mode::Browse;
            if uncertain_save.is_some() {
                view.status = "Save blocked: inspect uncertain publication before reopening; draft retained until exit.".into();
                continue;
            }
            match workflow.save_draft(editor.draft()) {
                Ok(_) => return Ok(completion),
                Err(error) => {
                    view.status = format!("Draft retained. {error}; no apply attempted.");
                    if matches!(
                        error,
                        WorkflowError::PublicationUncertain { .. }
                            | WorkflowError::RollbackIncomplete { .. }
                    ) {
                        uncertain_save = Some(error);
                    }
                }
            }
        }
    }
}

// Cancellation must not relabel an uncertain earlier save as "no files changed".
fn cancel_result(uncertain: Option<WorkflowError>) -> Result<Completion, CliError> {
    match uncertain {
        Some(error) => Err(error.into()),
        None => Ok(Completion::Cancel),
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
    tui::FOCUS
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
fn editor_areas(area: Rect, view: &View) -> [Rect; 3] {
    Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(1),
        Constraint::Length(
            if area.width >= 100 && area.height >= 18 && !view.status.is_empty() {
                5
            } else {
                3
            },
        ),
    ])
    .areas(area)
}
fn preview_area(area: Rect, view: &View) -> Option<Rect> {
    if tui::needs_resize(area.width, area.height) || !matches!(view.mode, Mode::Browse) {
        return None;
    }
    let body = editor_areas(area, view)[1];
    if area.width >= 100 && area.height >= 18 {
        Some(Layout::horizontal([Constraint::Min(1), Constraint::Length(44)]).split(body)[0])
    } else if view.sample {
        Some(body)
    } else {
        None
    }
}

fn control_item(text: &str, width: u16) -> ratatui::widgets::ListItem<'static> {
    let mut lines = vec![String::new()];
    for word in text.split_whitespace() {
        let line = lines.last_mut().unwrap();
        if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > usize::from(width)
        {
            lines.push(word.to_owned());
        } else {
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
    }
    ratatui::widgets::ListItem::new(lines.into_iter().map(Line::raw).collect::<Vec<_>>())
}
fn draw(
    frame: &mut Frame,
    editor: &ProfileEditor,
    manifest: &EnvironmentManifest,
    sample: &Option<crate::tui::sample::Sample>,
    view: &View,
    rows: &[(String, Control)],
) {
    if tui::resize_notice(frame) {
        return;
    }
    // Editor chrome always uses its own readable colors, not potentially unreadable draft colors.
    frame.render_widget(Block::default().style(tui::CHROME), frame.area());
    let wide = frame.area().width >= 100 && frame.area().height >= 18;
    let compact_sample = !wide && view.sample && matches!(view.mode, Mode::Browse);
    let areas = editor_areas(frame.area(), view);
    let group = ["Wallpaper", "Colors", "Terminal"][view.group];
    frame.render_widget(
        Paragraph::new(vec![
            Line::styled(
                if wide {
                    format!("Edit Profile {} · {group}", editor.draft().id())
                } else {
                    format!(
                        "Edit Profile {} · {} (p)",
                        editor.draft().id(),
                        if compact_sample {
                            "Preview"
                        } else {
                            "Controls"
                        }
                    )
                },
                tui::EMPHASIS,
            ),
            if editor.palette_contrast_infeasible().unwrap_or(false) {
                Line::styled("Contrast infeasible; fallback · v details", tui::EMPHASIS)
            } else {
                Line::from(vec![
                    Span::styled("Tab: ", tui::SECONDARY),
                    Span::styled(
                        "Wallpaper",
                        if view.group == 0 {
                            selected_style()
                        } else {
                            tui::SECONDARY
                        },
                    ),
                    Span::raw(" / "),
                    Span::styled(
                        "Colors",
                        if view.group == 1 {
                            selected_style()
                        } else {
                            tui::SECONDARY
                        },
                    ),
                    Span::raw(" / "),
                    Span::styled(
                        "Terminal",
                        if view.group == 2 {
                            selected_style()
                        } else {
                            tui::SECONDARY
                        },
                    ),
                ])
            },
        ]),
        areas[0],
    );
    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(1), Constraint::Length(44)])
        .split(areas[1]);
    if wide || !view.sample {
        let colors = manifest.colors().map(color_slots).unwrap_or_default();
        let list_area = if wide { panes[1] } else { areas[1] };
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
                control_item(
                    &format!("{label}  {suffix}"),
                    list_area.width.saturating_sub(3),
                )
            })
            .collect();
        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::LEFT)
                    .border_style(tui::SECONDARY)
                    .title(Line::styled(
                        format!("{group} · {}/{} · ↑↓ Home/End", view.row + 1, rows.len()),
                        tui::EMPHASIS,
                    )),
            )
            .highlight_symbol("> ")
            .highlight_style(selected_style());
        frame.render_stateful_widget(
            list,
            list_area,
            &mut ListState::default().with_selected(Some(view.row)),
        );
    }
    if wide || view.sample {
        let area = if wide { panes[0] } else { areas[1] };
        if let Some(sample) = sample {
            sample.draw(frame, area);
            let title = if sample.thumbnail.is_none() {
                "Internal sample (illustrative)"
            } else if TerminalGraphics::from_environment() == TerminalGraphics::Ghostty {
                "Internal sample (reduced; approximate)"
            } else {
                "Internal sample (cell fallback)"
            };
            frame.render_widget(
                Paragraph::new(title).style(tui::CHROME.patch(tui::SECONDARY)),
                Rect::new(
                    area.x + 1,
                    area.y,
                    area.width.saturating_sub(2).min(title.len() as u16),
                    1,
                ),
            );
        } else {
            draw_sample(frame, area, manifest, &None);
            if area.height > 1 {
                frame.render_widget(
                    Paragraph::new(view.preview_status.as_str()).style(tui::CHROME),
                    Rect::new(
                        area.x + 1,
                        area.bottom() - 1,
                        area.width.saturating_sub(2),
                        1,
                    ),
                );
            }
        }
    }
    let footer = if wide {
        format!(
            "s Save · u Save and use · Esc/q Cancel · v details\n↑↓ select · ←→ adjust · Enter edit · NOT live Ghostty reload\n{}",
            view.status
        )
    } else {
        format!(
            "s Save · u Save and use · Esc/q Cancel\n↑↓/←→ · Enter · p {} · v details\n{}",
            if compact_sample {
                "controls"
            } else {
                "preview"
            },
            if view.status.is_empty() {
                "NOT live Ghostty reload"
            } else {
                &view.status
            }
        )
    };
    let mut footer = ratatui::text::Text::from(footer);
    footer.lines[0].style = tui::EMPHASIS;
    footer.lines[1].style = tui::SECONDARY;
    frame.render_widget(Paragraph::new(footer).wrap(Wrap { trim: false }), areas[2]);
    let popup = match &view.mode {
        Mode::Browse => return,
        Mode::Confirm(yes) => format!("Save and use Profile {}?\nSave changes, then apply once.\n{}\n←→ choose · Enter confirm · y yes · n/Esc back", editor.draft().id(), if *yes { "Back to editor    [Save and use]" } else { "[Back to editor]    Save and use" }),
        Mode::ResetColors => "Reset ALL colors to Automatic?\nThis discards customized colors in the draft.\ny reset · Enter/n/Esc back (default)".into(),
        Mode::Number(control, input) => format!("Exact number: {}\n{input}_\nEnter accept · Esc back · invalid input keeps draft", match control { NumericControl::FontSize => "1..1000 points; up to 3 decimals", NumericControl::Blur => "0..255 integer", NumericControl::ImageBlur => "0..32 sigma at 512px short edge", NumericControl::ImageDarken => "0..100 linear-light black percent", _ => "0..1; up to 6 decimals" }),
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
            .style(tui::CHROME)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(tui::SECONDARY),
            )
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
                    Style::default().bg(rgb(c)).fg(Color::Rgb(15, 23, 42)),
                ),
                Span::raw(format!(" #{c} ({}/{})", selected + 1, samples.len())),
            ])]),
            area,
        );
    }
}
