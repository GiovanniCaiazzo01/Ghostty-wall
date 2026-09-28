//! Embedded creation form; persistence and image handling belong to ProfileWorkflows.

use super::*;
use crate::{
    domain::EnvironmentManifest,
    profile_editor::ProfileEditor,
    profile_workflow::{ProfileWorkflows, WorkflowError},
    tui::sample::Sample,
};
use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph, Wrap},
};

#[derive(Default)]
enum Step {
    #[default]
    Name,
    Wallpaper,
    Review {
        generated: bool,
    },
    Use,
}

#[derive(Default)]
struct Form {
    step: Step,
    name: String,
    status: String,
    show_sample: bool,
}

pub(super) fn flow(output: &mut impl Write) -> Result<(Option<IntentId>, String), CliError> {
    let paths = process_paths()?;
    let workflow = ProfileWorkflows::load(paths.clone())?;
    let mut form = Form::default();
    let mut draft: Option<ProfileEditor> = None;
    let mut sample = None;
    let mut screen = editor::Screen::open(output)?;
    loop {
        screen
            .terminal
            .draw(|frame| draw(frame, &form, sample.as_ref()))?;
        let key = editor::key()?;
        if editor::interrupt(key) || key.code == KeyCode::Esc {
            return Ok(if matches!(form.step, Step::Use) {
                (
                    draft.as_ref().map(|d| d.draft().id().clone()),
                    "Profile saved; Not now. Terminal unchanged.".into(),
                )
            } else {
                (None, "Creation cancelled; no Profile saved.".into())
            });
        }
        let size = screen.terminal.size()?;
        if tui::needs_resize(size.width, size.height) {
            continue;
        }
        if (key.code == KeyCode::F(1)
            || (key.code == KeyCode::Char('v') && !matches!(form.step, Step::Name)))
            && !form.status.is_empty()
        {
            management::details(screen.terminal.backend_mut(), &form.status)?;
            screen.redraw()?;
            continue;
        }
        if matches!(form.step, Step::Name) {
            match key.code {
                KeyCode::Backspace => {
                    form.name.pop();
                }
                KeyCode::Char(c) if form.name.len() < 128 => form.name.push(c),
                KeyCode::Enter => match workflow.create(&form.name) {
                    Ok(new) => {
                        draft = Some(ProfileEditor::new(
                            new,
                            workflow.config().clone(),
                            EnvironmentManifest::new(None, None, None),
                            None,
                            None,
                        )?);
                        form.step = Step::Wallpaper;
                        form.status.clear();
                    }
                    Err(e) => form.status = format!("{e}. Choose another id."),
                },
                _ => (),
            }
            continue;
        }
        let Some(draft) = draft.as_mut() else {
            continue;
        };
        let result = match form.step {
            Step::Wallpaper if key.code == KeyCode::Char('g') => draft
                .generate_image(&workflow, editor::random_seed()?)
                .map(|()| {
                    form.step = Step::Review { generated: true };
                }),
            Step::Wallpaper if key.code == KeyCode::Char('i') => {
                let chosen = forms::image(screen.terminal.backend_mut(), &paths, |path| {
                    draft.import_image(&workflow, path)
                })?;
                screen.redraw()?;
                if chosen {
                    form.step = Step::Review { generated: false };
                }
                Ok(())
            }
            Step::Review { generated: true } if key.code == KeyCode::Char('a') => {
                draft.generate_image(&workflow, editor::random_seed()?)
            }
            Step::Review { .. } if key.code == KeyCode::Char('s') => {
                match workflow.save_draft(draft.draft()) {
                    Ok(_) => {
                        form.step = Step::Use;
                        Ok(())
                    }
                    // Do not offer a retry or claim cancellation undoes uncertain publication.
                    Err(
                        e @ (WorkflowError::PublicationUncertain { .. }
                        | WorkflowError::RollbackIncomplete { .. }),
                    ) => return Err(e.into()),
                    Err(e) => Err(e),
                }
            }
            Step::Review { .. } if key.code == KeyCode::Char('p') => {
                form.show_sample = !form.show_sample;
                continue;
            }
            Step::Use => {
                let id = draft.draft().id().clone();
                if matches!(key.code, KeyCode::Enter | KeyCode::Char('n')) {
                    return Ok((
                        Some(id.clone()),
                        format!("Saved Profile {id}; Not now. Terminal unchanged."),
                    ));
                }
                if key.code == KeyCode::Char('y') {
                    let outcome =
                        workflow.use_saved(&id, |id| Application::load(None)?.apply(id))?;
                    let report = match outcome {
                        crate::profile_workflow::ProfileOutcome::SavedAndApplied {
                            activation,
                            reload,
                        } => format!(
                            "Saved Profile {id}. Activated {activation}. Ghostty reload: {}.",
                            reload_status(reload)
                        ),
                        crate::profile_workflow::ProfileOutcome::Saved => {
                            format!("Saved Profile {id}; terminal unchanged.")
                        }
                    };
                    return Ok((Some(id), report));
                }
                continue;
            }
            _ => continue,
        };
        match result {
            Ok(()) => {
                form.status.clear();
                sample = Some(Sample::new(draft.preview()?, draft.image()));
            }
            Err(e) => form.status = format!("{e}. Draft retained."),
        }
    }
}

fn draw(frame: &mut Frame, form: &Form, sample: Option<&Sample>) {
    if tui::resize_notice(frame) {
        return;
    }
    frame.render_widget(
        Block::default().style(Style::default().fg(Color::White).bg(Color::Black)),
        frame.area(),
    );
    let areas = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(1),
        Constraint::Length(2),
    ])
    .split(frame.area());
    frame.render_widget(
        Paragraph::new(format!(
            "Create Profile {}\n{}",
            form.name,
            if form.status.is_empty() {
                "Internal sample; NOT live Ghostty reload"
            } else {
                "Error: F1 for details; draft retained"
            }
        )),
        areas[0],
    );
    let (text, keys) = match form.step {
        Step::Name => (format!("Profile ID: {}_\n1..64 lowercase letters/digits; internal hyphens. Existing ids are never overwritten.", form.name), "Enter next · Esc cancel · F1 error"),
        Step::Wallpaper => ("Starting point\n\ng Generate wallpaper for me\ni Choose my image (Downloads/Pictures)\n\nNothing saved yet.".into(), "g Generate · i Image · Esc cancel\nF1 error details"),
        Step::Review { generated } => (format!("Review {}\nWallpaper + generated colors\nText, ANSI, cursor and selection\n\ns Save complete Profile\n{}\nNothing saved yet.", form.name, if generated { "a Another variant (before Save only)" } else { "Original image stays untouched." }), if generated { "s Save · a Another variant\np sample/form · Esc cancel · v error" } else { "s Save · p sample/form\nEsc cancel · v error" }),
        Step::Use => (format!("Saved Profile {}.\nNo Activation yet.\n\ny Use now\nn Not now (default)\n\nReload is best-effort, not verified visible change.", form.name), "y Use now · Enter/n/Esc Not now"),
    };
    let wide = frame.area().width >= 90 && frame.area().height >= 18;
    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(areas[1]);
    if !wide && form.show_sample && matches!(form.step, Step::Review { .. }) {
        if let Some(sample) = sample {
            sample.draw(frame, areas[1]);
        }
    } else {
        frame.render_widget(
            Paragraph::new(format!("{text}\n{}", form.status))
                .wrap(Wrap { trim: false })
                .block(Block::default().borders(Borders::ALL).title("Create")),
            if wide { panes[0] } else { areas[1] },
        );
    }
    if wide && let Some(sample) = sample {
        sample.draw(frame, panes[1]);
    }
    frame.render_widget(Paragraph::new(keys).wrap(Wrap { trim: false }), areas[2]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn every_small_form_keeps_its_action_controls_visible() {
        for (step, labels) in [
            (Step::Name, vec!["Enter next", "Esc cancel"]),
            (Step::Wallpaper, vec!["g Generate", "i Image", "Esc cancel"]),
            (
                Step::Review { generated: true },
                vec!["s Save", "p sample/form", "Esc cancel"],
            ),
            (Step::Use, vec!["y Use now", "Not now"]),
        ] {
            let mut terminal = Terminal::new(TestBackend::new(40, 12)).unwrap();
            terminal
                .draw(|frame| {
                    draw(
                        frame,
                        &Form {
                            step,
                            name: "night".into(),
                            ..Form::default()
                        },
                        None,
                    )
                })
                .unwrap();
            let screen = terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .map(|c| c.symbol())
                .collect::<String>();
            for label in labels {
                assert!(screen.contains(label), "missing {label}: {screen}");
            }
        }
    }
}
