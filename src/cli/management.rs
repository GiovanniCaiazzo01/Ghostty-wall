//! Full-screen orchestration over the same creation, editing, deletion and apply workflows.

use super::*;
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
    refresh(application, browser, &mut view, seed);
    loop {
        screen
            .terminal
            .draw(|frame| tui::draw(frame, browser, &view))?;
        let key = editor::key()?;
        let size = screen.terminal.size()?;
        if tui::needs_resize(size.width, size.height) {
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
        match choice {
            Input::Action(BrowserAction::Cancel) => break,
            Input::Action(BrowserAction::Back) => view.show_sample = false,
            Input::Action(BrowserAction::Preview) => view.show_sample = !view.show_sample,
            Input::Action(action) => {
                let selected = browser.selected_profile().cloned();
                browser.dispatch(action, application)?;
                if browser.selected_profile() != selected.as_ref() {
                    view.sample = None;
                    view.preview_error = "Resolving selected Profile...".into();
                    screen
                        .terminal
                        .draw(|frame| tui::draw(frame, browser, &view))?;
                    refresh(application, browser, &mut view, seed);
                } else {
                    refresh_active(application, &mut view);
                }
            }
            Input::Command('?') => view.menu = Some(0),
            Input::Command('a') => {
                let result = browser
                    .selected_profile()
                    .ok_or_else(|| {
                        CliError::Input("Select a Profile to Use; no files changed.".into())
                    })
                    .and_then(|id| application.apply(id));
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
                drop(screen);
                details(output, &format!("{}\n{}", view.status, view.preview_error))?;
                screen = editor::Screen::open(output)?;
            }
            Input::Command(command) => {
                let selected = browser.selected_profile().cloned();
                drop(screen);
                let mut target = selected.clone();
                let result = match command {
                    'n' => create_tui::flow(output).map(|(id, report)| {
                        if id.is_some() {
                            target = id;
                        }
                        report
                    }),
                    'e' => selected
                        .as_ref()
                        .ok_or_else(|| {
                            CliError::Input("Select a Profile to Edit; no files changed.".into())
                        })
                        .and_then(|id| editor::flow(&[id.to_string()], output, true)),
                    'i' => (|| {
                        if browser.focus() == BrowserFocus::Sources {
                            browser.dispatch(BrowserAction::NextPane, application)?;
                        }
                        browser.dispatch(BrowserAction::Preview, application)?;
                        browser
                            .render_preview_image(output, TerminalGraphics::from_environment())?;
                        let stdin = io::stdin();
                        prompt_tui(&mut stdin.lock().lines(), output, "Press Enter to return: ")?;
                        Ok("Image preview closed; not a Ghostty reload.".into())
                    })(),
                    _ => (|| {
                        let stdin = io::stdin();
                        let mut lines = stdin.lock().lines();
                        let result = tui_command(
                            &command.to_string(),
                            &mut lines,
                            output,
                            application,
                            browser,
                            seed,
                            false,
                        );
                        target = browser.selected_profile().cloned();
                        if matches!(
                            command,
                            'h' | 's'
                                | 'l'
                                | 'P'
                                | 'p'
                                | 'D'
                                | 'u'
                                | 'U'
                                | 'I'
                                | 'y'
                                | 'R'
                                | 'W'
                                | 'Y'
                                | 'M'
                                | 'X'
                        ) {
                            if let Err(e) = &result {
                                writeln!(output, "{e}")?;
                            }
                            prompt_tui(&mut lines, output, "Press Enter to return: ")?;
                        }
                        result.map(|()| "Ready; selection does not activate.".into())
                    })(),
                };
                view.status = result.unwrap_or_else(|e| e.to_string());
                if browser.mode() == BrowserMode::Cancelled {
                    return write_text(output, "Cancelled.\n");
                }
                // Even a failed apply can follow a successful save. Reload the list rather
                // than hiding the saved Profile, and derive the marker only from History.
                reload_list(application, browser, seed, target.as_ref())?;
                if command == 'o' {
                    browser.dispatch(BrowserAction::NextPane, application)?;
                }
                view.sample = None;
                view.preview_error = "Resolving selected Profile...".into();
                refresh_active(application, &mut view);
                screen = editor::Screen::open(output)?;
                screen
                    .terminal
                    .draw(|frame| tui::draw(frame, browser, &view))?;
                refresh(application, browser, &mut view, seed);
            }
            Input::Ignore => (),
        }
    }
    drop(screen);
    write_text(output, "Cancelled.\n")
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
    application: &mut Application,
    browser: &TerminalBrowser,
    view: &mut tui::View,
    seed: Option<ResolutionSeed>,
) {
    refresh_active(application, view);
    let sample = (|| {
        let id = browser
            .selected_profile()
            .ok_or_else(|| CliError::Input("No Profiles; choose Create.".into()))?;
        // Random Profiles get a session selection without requiring a remembered CLI flag.
        // Use reuses this seed; nonrandom Profiles must not receive one.
        let intent = application.load_profile(id)?;
        application.seed = if matches!(
            intent.wallpaper,
            Some(WallpaperIntent::Source {
                selection: WallpaperSelection::Random,
                ..
            })
        ) {
            Some(match seed {
                Some(seed) => seed,
                None => ResolutionSeed::from_str(&editor::random_seed()?.to_string())?,
            })
        } else {
            None
        };
        let plan = application.plan_intent(id, &intent)?;
        let image = if uses_github_plan(&plan) {
            planned_github_asset_bytes(&plan, &application.github)
        } else {
            planned_local_asset_bytes(&plan)
        }
        .map_err(CliError::Plan)?;
        let manifest =
            crate::codec::manifest::decode(&serde_json::to_vec(&plan["environment"]["manifest"])?)
                .map_err(|e| CliError::Intent(e.to_string()))?;
        Ok::<_, CliError>(Sample::new(manifest, image.as_deref()))
    })();
    match sample {
        Ok(sample) => {
            view.sample = Some(sample);
            view.preview_error.clear();
        }
        Err(e) => {
            view.sample = None;
            view.preview_error = e.to_string();
        }
    }
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
