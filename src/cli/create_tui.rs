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
    Confirm {
        generated: bool,
        use_now: bool,
    },
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
        if editor::interrupt(key) {
            return Ok((None, "Creation cancelled; no Profile saved.".into()));
        }
        if key.code == KeyCode::Esc {
            if let Step::Confirm { generated, .. } = form.step {
                form.step = Step::Review { generated };
                form.status = "Back to editor; draft intact.".into();
                continue;
            }
            return Ok((None, "Creation cancelled; no Profile saved.".into()));
        }
        if key.code == KeyCode::Char('q') && !matches!(form.step, Step::Name) {
            return Ok((None, "Creation cancelled; no Profile saved.".into()));
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
        let mut completion = None;
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
                completion = Some(editor::Completion::Save);
                Ok(())
            }
            Step::Review { generated } if key.code == KeyCode::Char('u') => {
                form.step = Step::Confirm {
                    generated,
                    use_now: false,
                };
                continue;
            }
            Step::Review { .. } if key.code == KeyCode::Char('p') => {
                form.show_sample = !form.show_sample;
                continue;
            }
            Step::Confirm {
                generated,
                mut use_now,
            } => {
                match key.code {
                    KeyCode::Left | KeyCode::Right | KeyCode::Tab => use_now = !use_now,
                    KeyCode::Char('y') => use_now = true,
                    KeyCode::Char('n') => {
                        form.step = Step::Review { generated };
                        form.status = "Back to editor; draft intact.".into();
                        continue;
                    }
                    _ => (),
                }
                form.step = Step::Confirm { generated, use_now };
                if matches!(key.code, KeyCode::Enter | KeyCode::Char('y')) {
                    form.step = Step::Review { generated };
                    if !use_now {
                        form.status = "Back to editor; draft intact.".into();
                        continue;
                    }
                    completion = Some(editor::Completion::SaveAndUse);
                }
                Ok(())
            }
            _ => continue,
        };
        let result = result.and_then(|()| {
            if completion.is_some() {
                workflow.save_draft(draft.draft())?;
            }
            Ok(())
        });
        match result {
            Ok(()) => {
                if let Some(completion) = completion {
                    let id = draft.draft().id().clone();
                    let report = if completion == editor::Completion::Save {
                        saved_report(&id)
                    } else {
                        let outcome =
                            workflow.use_saved(&id, |id| Application::load(None)?.apply(id))?;
                        format!("{}Saved Profile {id}.\n", completion_report(&id, outcome))
                    };
                    return Ok((Some(id), report));
                }
                form.status.clear();
                sample = Some(Sample::new(draft.preview()?, draft.image()));
            }
            // Do not offer a retry or claim cancellation undoes uncertain publication.
            Err(
                e @ (WorkflowError::PublicationUncertain { .. }
                | WorkflowError::RollbackIncomplete { .. }),
            ) => return Err(e.into()),
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
        Constraint::Length(3),
    ])
    .split(frame.area());
    frame.render_widget(
        Paragraph::new(format!(
            "Create Profile {}\n{}",
            form.name,
            if form.status.is_empty() {
                "Internal sample; NOT live Ghostty reload"
            } else {
                "F1/v for details; draft retained"
            }
        )),
        areas[0],
    );
    let (text, keys) = match form.step {
        Step::Name => (format!("Profile ID: {}_\n1..64 lowercase letters/digits; internal hyphens. Existing ids are never overwritten.", form.name), "Enter next · Esc cancel · F1 error"),
        Step::Wallpaper => ("Starting point\n\ng Generate wallpaper for me\ni Choose my image (Downloads/Pictures)\n\nNothing saved yet.".into(), "g Generate · i Image · Esc cancel\nF1 error details"),
        Step::Review { generated } => (format!("Review {}\nWallpaper + generated colors\nText, ANSI, cursor and selection\n\ns Save complete Profile without applying\nu Save and use (confirm)\n{}\nNothing saved yet.", form.name, if generated { "a Another variant (before Save only)" } else { "Original image stays untouched." }), if generated { "s Save · u Save and use · Esc/q Cancel\na Another variant · p sample/form\nv details" } else { "s Save · u Save and use · Esc/q Cancel\np sample/form · v details" }),
        Step::Confirm { use_now, .. } => (format!("Save and use Profile {}?\nSave changes, then apply once.\n{}\nNothing saved yet.", form.name, if use_now { "Back to editor    [Save and use]" } else { "[Back to editor]    Save and use" }), "←→ choose · Enter confirm · y yes\nn/Esc back"),
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
                vec!["s Save", "u Save and use", "p sample/form", "Esc/q Cancel"],
            ),
            (
                Step::Confirm {
                    generated: true,
                    use_now: false,
                },
                vec!["Back to editor", "Enter confirm", "n/Esc back"],
            ),
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
