//! Full-screen orchestration over the same creation, editing, deletion and apply workflows.

use super::*;
mod preview;
use crate::{domain::WallpaperSelection, tui::sample::Sample};
use crossterm::event::KeyCode;
use ratatui::{
    layout::{Constraint, Layout},
    widgets::{Paragraph, Wrap},
};

pub(super) fn run(
    output: &mut impl Write,
    application: &mut Application,
    browser: &mut TerminalBrowser,
    seed: Option<ResolutionSeed>,
) -> Result<(), CliError> {
    browser.dispatch(BrowserAction::NextPane, application)?;
    let mut view = tui::View {
        preview_error: "Resolving selected Profile...".into(),
        ..tui::View::default()
    };
    let mut screen = editor::Screen::open(output)?;
    screen
        .terminal
        .draw(|frame| tui::draw(frame, browser, &view))?;
    refresh_active(application, &mut view);
    let mut previews = preview::Previews::new(seed)?;
    let size = screen.terminal.size()?;
    previews.area = tui::preview_area(
        ratatui::layout::Rect::new(0, 0, size.width, size.height),
        &view,
    )
    .unwrap_or_default();
    refresh(application, browser, &mut view, &mut previews);
    let graphics = TerminalGraphics::from_environment() == TerminalGraphics::Ghostty;
    let mut image_shown = false;
    loop {
        let size = screen.terminal.size()?;
        let area = tui::preview_area(
            ratatui::layout::Rect::new(0, 0, size.width, size.height),
            &view,
        );
        if let Some(area) = area
            && previews.area != area
        {
            previews.resize(browser, &mut view, area);
        }
        let completed = previews.poll(&mut view);
        if image_shown && (completed || view.sample.is_none() || area.is_none()) {
            clear_image(screen.terminal.backend_mut())?;
            image_shown = false;
        }
        screen
            .terminal
            .draw(|frame| tui::draw(frame, browser, &view))?;
        if graphics
            && !image_shown
            && let (Some(sample), Some(area)) = (&view.sample, area)
        {
            image_shown = sample.render_graphics(screen.terminal.backend_mut(), area)?;
        }
        if !event::poll(std::time::Duration::from_millis(16))? {
            continue;
        }
        let key = match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => key,
            Event::Resize(_, _) => crossterm::event::KeyEvent::from(KeyCode::Null),
            _ => continue,
        };
        let size = screen.terminal.size()?;
        if tui::management_needs_resize(size.width, size.height) {
            if editor::interrupt(key) || key.code == KeyCode::Esc {
                break;
            }
            continue;
        }
        let choice = if view.menu.is_some() {
            tui::menu_input(key, &mut view.menu)
        } else {
            tui::input(key, browser.mode())
        };
        let choice = match choice {
            Input::Action(BrowserAction::Preview) if browser.focus() == BrowserFocus::Sources => {
                Input::Command('m')
            }
            choice => choice,
        };
        if matches!(choice, Input::Command(_)) && image_shown {
            clear_image(screen.terminal.backend_mut())?;
            image_shown = false;
        }
        match choice {
            Input::Action(BrowserAction::Cancel) => break,
            Input::Action(BrowserAction::Back) => view.show_sample = false,
            Input::Action(BrowserAction::Preview) => view.show_sample = !view.show_sample,
            Input::Action(action) => {
                let selected = browser.selected_profile().cloned();
                browser.dispatch(action, application)?;
                if browser.selected_profile() != selected.as_ref() {
                    refresh(application, browser, &mut view, &mut previews);
                }
            }
            Input::Command('?') => view.menu = Some(0),
            Input::Command('a') => {
                let result = browser
                    .selected_profile()
                    .ok_or_else(|| {
                        CliError::Input("Select a Profile to Use; no files changed.".into())
                    })
                    .and_then(|id| {
                        preview::set_seed(application, id, previews.seed)?;
                        application.apply(id)
                    });
                view.status = match result {
                    Ok(outcome) => format!(
                        "Profile applied. Ghostty reload: {}.\nActivated {}.",
                        reload_status(outcome.reload_outcome()),
                        outcome.activation_id()
                    ),
                    Err(e) => e.to_string(),
                };
                // Use already resolves again. Retain the inspected sample rather than
                // paying for another image resolution merely to update the active marker.
                refresh_active(application, &mut view);
            }
            Input::Command('v') => {
                details(
                    screen.terminal.backend_mut(),
                    &format!(
                        "{}\n{}\n{}",
                        view.status,
                        view.preview_error,
                        view.sample
                            .as_ref()
                            .map(Sample::guidance)
                            .unwrap_or_default()
                    ),
                )?;
                screen.redraw()?;
            }
            Input::Command(
                command @ ('n' | 'N' | 'm' | 'e' | 'r' | 'd' | 'x' | 'f' | 'c' | 't' | 'w'),
            ) => {
                let mut target = browser.selected_profile().cloned();
                let result = match command {
                    'n' => create_tui::flow(screen.terminal.backend_mut()),
                    'e' => target
                        .as_ref()
                        .ok_or_else(|| {
                            CliError::Input("Select a Profile to Edit; no files changed.".into())
                        })
                        .and_then(|id| {
                            editor::flow(&[id.to_string()], screen.terminal.backend_mut(), true)
                                .map(|report| (Some(id.clone()), report))
                        }),
                    _ => profile_forms::run(command, screen.terminal.backend_mut(), browser),
                };
                view.status = match result {
                    Ok((id, report)) => {
                        if id.is_some() {
                            target = id;
                        }
                        report
                    }
                    Err(e) => e.to_string(),
                };
                reload_list(application, browser, seed, target.as_ref())?;
                refresh_active(application, &mut view);
                refresh(application, browser, &mut view, &mut previews);
                screen.redraw()?;
            }
            Input::Command(
                command @ ('l' | 'h' | 's' | 'P' | 'D' | 'p' | 'u' | 'U' | 'X' | 'I' | 'R' | 'W'
                | 'M' | 'y' | 'Y'),
            ) => {
                let result = maintenance::report(
                    command,
                    screen.terminal.backend_mut(),
                    browser.selected_profile().cloned(),
                    Some(previews.seed),
                );
                view.status = match result {
                    Ok(report) => report,
                    Err(error) => {
                        let report = format!("Maintenance failed: {error}");
                        details(screen.terminal.backend_mut(), &report)?;
                        report
                    }
                };
                if matches!(command, 'p' | 'X' | 'I' | 'R' | 'W' | 'M') {
                    let selected = browser.selected_profile().cloned();
                    if let Err(error) = reload_list(application, browser, seed, selected.as_ref()) {
                        view.status.push_str(&format!(
                            "\nRefresh failed: {error}. Inspect Settings/Doctor before retrying."
                        ));
                    }
                    refresh_active(application, &mut view);
                    refresh(application, browser, &mut view, &mut previews);
                }
                screen.redraw()?;
            }
            Input::Command('o') => {
                view.status = maintenance::source(screen.terminal.backend_mut())
                    .unwrap_or_else(|error| error.to_string());
                let selected = browser.selected_profile().cloned();
                if let Err(error) = reload_list(application, browser, seed, selected.as_ref()) {
                    view.status.push_str(&format!("\nRefresh failed: {error}"));
                }
                browser.dispatch(BrowserAction::NextPane, application)?;
                refresh(application, browser, &mut view, &mut previews);
                screen.redraw()?;
            }
            Input::Command('i') => {
                let result = maintenance::image(
                    screen.terminal.backend_mut(),
                    browser.selected_profile().cloned(),
                    Some(previews.seed),
                );
                view.status = match result {
                    Ok(report) => report,
                    Err(error) => {
                        let report = format!("Image preview failed: {error}");
                        details(screen.terminal.backend_mut(), &report)?;
                        report
                    }
                };
                screen.redraw()?;
            }
            Input::Ignore | Input::Command(_) => (),
        }
    }
    if graphics {
        clear_image(screen.terminal.backend_mut())?;
    }
    drop(screen);
    write_text(output, "Cancelled.\n")
}

pub(super) fn preview_application(
    id: &IntentId,
    seed: Option<ResolutionSeed>,
) -> Result<Application, CliError> {
    let mut application = Application::load(None)?;
    if let Some(seed) = seed {
        preview::set_seed(&mut application, id, seed)?;
    }
    Ok(application)
}

fn clear_image(output: &mut impl Write) -> io::Result<()> {
    output.write_all(b"\x1b_Ga=d,d=I,i=42,q=2;\x1b\\")?;
    output.flush()
}

fn reload_list(
    application: &mut Application,
    browser: &mut TerminalBrowser,
    seed: Option<ResolutionSeed>,
    target: Option<&IntentId>,
) -> Result<(), CliError> {
    *application = Application::load(seed)?;
    let profiles = profile_ids(&application.paths.managed_root().join("profiles"))?;
    let index = target
        .and_then(|id| profiles.iter().position(|p| p == id))
        .unwrap_or(0);
    *browser = TerminalBrowser::new(
        application
            .config
            .sources
            .iter()
            .map(|(id, _)| id.clone())
            .collect(),
        profiles,
    );
    browser.dispatch(BrowserAction::NextPane, application)?;
    for _ in 0..index {
        browser.dispatch(BrowserAction::Down, application)?;
    }
    Ok(())
}

fn refresh(
    _application: &mut Application,
    browser: &TerminalBrowser,
    view: &mut tui::View,
    previews: &mut preview::Previews,
) {
    previews.request(browser, view);
}

fn refresh_active(application: &Application, view: &mut tui::View) {
    view.active = None;
    match crate::history::inspect_history(&application.paths.managed_root()) {
        Ok(history) => view.active = history.latest().and_then(|a| a.profile_id()).cloned(),
        Err(e) => {
            view.status = format!("Active marker unavailable: {e}; inspect History with doctor.")
        }
    }
}

pub(super) fn details(output: &mut impl Write, report: &str) -> Result<(), CliError> {
    let mut screen = editor::Screen::open(output)?;
    let mut scroll = 0_u16;
    loop {
        screen.terminal.draw(|frame| {
            let areas =
                Layout::vertical([Constraint::Min(1), Constraint::Length(2)]).split(frame.area());
            frame.render_widget(
                ratatui::widgets::Block::default().style(
                    ratatui::style::Style::default()
                        .fg(ratatui::style::Color::White)
                        .bg(ratatui::style::Color::Black),
                ),
                frame.area(),
            );
            frame.render_widget(
                Paragraph::new(report)
                    .wrap(Wrap { trim: false })
                    .scroll((scroll, 0)),
                areas[0],
            );
            frame.render_widget(
                Paragraph::new("↑↓ scroll · Enter/Esc/q back").wrap(Wrap { trim: false }),
                areas[1],
            );
        })?;
        let key = editor::key()?;
        if editor::interrupt(key)
            || matches!(key.code, KeyCode::Enter | KeyCode::Esc | KeyCode::Char('q'))
        {
            return Ok(());
        }
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => scroll = scroll.saturating_add(1),
            KeyCode::Up | KeyCode::Char('k') => scroll = scroll.saturating_sub(1),
            KeyCode::PageDown => scroll = scroll.saturating_add(10),
            KeyCode::PageUp => scroll = scroll.saturating_sub(10),
            _ => (),
        }
    }
}
