use super::*;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use ratatui::{
    layout::{Constraint, Layout},
    style::{Color, Style},
    widgets::{Block, Paragraph, Wrap},
};

fn form_key() -> Result<KeyEvent, CliError> {
    let mut key = editor::key()?;
    if key.code == KeyCode::Char('j') && key.modifiers.contains(KeyModifiers::CONTROL) {
        key.code = KeyCode::Enter;
    }
    Ok(key)
}

pub(super) fn fields<T>(
    output: &mut impl Write,
    title: &str,
    labels: &[&str],
    values: &mut [String],
    mut submit: impl FnMut(&[String]) -> Result<T, CliError>,
) -> Result<Option<T>, CliError> {
    assert!(!labels.is_empty() && labels.len() == values.len());
    let mut screen = editor::Screen::open(output)?;
    let mut field = 0;
    let mut error = String::new();
    loop {
        screen.terminal.draw(|frame| {
            if tui::resize_notice(frame) { return; }
            let areas = Layout::vertical([
                Constraint::Length(2), Constraint::Min(1), Constraint::Length(4),
            ]).split(frame.area());
            frame.render_widget(Block::default().style(Style::default().fg(Color::White).bg(Color::Black)), frame.area());
            frame.render_widget(Paragraph::new(title).wrap(Wrap { trim: false }), areas[0]);
            let width = usize::from(areas[1].width.saturating_sub(4));
            let tail = values[field].chars().rev().take(width).collect::<Vec<_>>().into_iter().rev().collect::<String>();
            frame.render_widget(Paragraph::new(format!("{} ({}/{})\n> {}_\n\n{}", labels[field], field + 1, labels.len(), tail, error)).wrap(Wrap { trim: false }), areas[1]);
            frame.render_widget(Paragraph::new("Enter next / submit last field\nTab / Shift-Tab fields · Ctrl-U clear\nEsc / Ctrl-C cancel · F1 error details").wrap(Wrap { trim: false }), areas[2]);
        })?;
        let key = form_key()?;
        if editor::interrupt(key) || key.code == KeyCode::Esc {
            return Ok(None);
        }
        let size = screen.terminal.size()?;
        if tui::needs_resize(size.width, size.height) {
            continue;
        }
        match key.code {
            KeyCode::F(1) => {
                management::details(
                    screen.terminal.backend_mut(),
                    &format!("{title}\n\n{error}"),
                )?;
                screen.redraw()?;
            }
            KeyCode::Tab | KeyCode::Down => field = (field + 1) % labels.len(),
            KeyCode::BackTab | KeyCode::Up => field = (field + labels.len() - 1) % labels.len(),
            KeyCode::Enter if field + 1 < labels.len() => field += 1,
            KeyCode::Enter => match submit(values) {
                Ok(result) => return Ok(Some(result)),
                Err(e) => error = format!("{e}\nCorrect the fields or Esc to cancel."),
            },
            KeyCode::Backspace => {
                values[field].pop();
            }
            KeyCode::Char('u')
                if key
                    .modifiers
                    .contains(crossterm::event::KeyModifiers::CONTROL) =>
            {
                values[field].clear()
            }
            KeyCode::Char(c)
                if !key
                    .modifiers
                    .contains(crossterm::event::KeyModifiers::CONTROL)
                    && values[field].len() < 4096 =>
            {
                values[field].push(c)
            }
            _ => (),
        }
    }
}

pub(super) fn image(
    output: &mut impl Write,
    paths: &InitPaths,
    mut import: impl FnMut(&Path) -> Result<(), crate::profile_workflow::WorkflowError>,
) -> Result<bool, CliError> {
    let roots = create_image_roots(paths);
    let mut current = roots
        .iter()
        .find(|path| path.is_dir())
        .cloned()
        .unwrap_or_else(|| paths.home.clone());
    let mut search = String::new();
    let mut input = String::new();
    let mut status = String::new();
    let mut selected = 0;
    let mut screen = editor::Screen::open(output)?;
    loop {
        let mut entries = Vec::new();
        match fs::read_dir(&current) {
            Ok(items) => {
                for item in items.flatten() {
                    let path = item.path();
                    let Some(name) = item.file_name().to_str().map(str::to_owned) else {
                        continue;
                    };
                    let supported = path.extension().and_then(|s| s.to_str()).is_some_and(|s| {
                        matches!(s.to_ascii_lowercase().as_str(), "png" | "jpg" | "jpeg")
                    });
                    if item
                        .file_type()
                        .is_ok_and(|t| t.is_dir() || (t.is_file() && supported))
                        && name.to_lowercase().contains(&search.to_lowercase())
                    {
                        entries.push(name);
                    }
                }
                entries.sort();
            }
            Err(e) => {
                status = format!(
                    "Cannot browse {}: {e}. Choose another location.",
                    current.display()
                )
            }
        }
        selected = selected.min(entries.len().saturating_sub(1));
        screen.terminal.draw(|frame| {
            if tui::resize_notice(frame) { return; }
            let areas = Layout::vertical([Constraint::Length(3), Constraint::Min(1), Constraint::Length(3), Constraint::Length(4)]).split(frame.area());
            frame.render_widget(Block::default().style(Style::default().fg(Color::White).bg(Color::Black)), frame.area());
            frame.render_widget(Paragraph::new(format!("Images in {}\nDownloads: {} · Pictures: {}", current.display(), roots[0].display(), roots[1].display())).wrap(Wrap { trim: false }), areas[0]);
            let list = ratatui::widgets::List::new(entries.iter().enumerate().map(|(i, name)| format!("{}: {name}", i + 1))).highlight_symbol("> ").highlight_style(Style::default().fg(Color::Black).bg(Color::Cyan));
            frame.render_stateful_widget(list, areas[1], &mut ratatui::widgets::ListState::default().with_selected(if entries.is_empty() { None } else { Some(selected) }));
            let tail = input.chars().rev().take(usize::from(areas[2].width.saturating_sub(3))).collect::<Vec<_>>().into_iter().rev().collect::<String>();
            frame.render_widget(Paragraph::new(format!("> {tail}_\n{status}")).wrap(Wrap { trim: false }), areas[2]);
            frame.render_widget(Paragraph::new("Number/relative path · Enter open\nd/p roots · /text search · .. parent\npath:/absolute/path · Ctrl-U clear\nEsc cancel · ↑↓ select · F1 path/error").wrap(Wrap { trim: false }), areas[3]);
        })?;
        let key = form_key()?;
        if editor::interrupt(key) || key.code == KeyCode::Esc {
            return Ok(false);
        }
        let size = screen.terminal.size()?;
        if tui::needs_resize(size.width, size.height) {
            continue;
        }
        match key.code {
            KeyCode::F(1) => {
                let path = entries
                    .get(selected)
                    .map_or_else(|| current.clone(), |name| current.join(name));
                let report = format!("Selected path:\n{}\n\n{status}", path.display());
                management::details(screen.terminal.backend_mut(), &report)?;
                screen.redraw()?;
            }
            KeyCode::Up => selected = selected.saturating_sub(1),
            KeyCode::Down => selected = (selected + 1).min(entries.len().saturating_sub(1)),
            KeyCode::Backspace => {
                input.pop();
            }
            KeyCode::Char('u')
                if key
                    .modifiers
                    .contains(crossterm::event::KeyModifiers::CONTROL) =>
            {
                input.clear()
            }
            KeyCode::Char(c)
                if !key
                    .modifiers
                    .contains(crossterm::event::KeyModifiers::CONTROL)
                    && input.len() < 4096 =>
            {
                input.push(c)
            }
            KeyCode::Enter => {
                let choice = std::mem::take(&mut input);
                if matches!(choice.as_str(), "cancel" | "b" | "esc") {
                    return Ok(false);
                }
                if let Some(query) = choice.strip_prefix('/') {
                    search = query.into();
                    selected = 0;
                    continue;
                }
                let destination = match choice.as_str() {
                    "d" => Some(roots[0].clone()),
                    "p" => Some(roots[1].clone()),
                    ".." => current.parent().map(Path::to_owned),
                    _ => None,
                };
                if let Some(path) = destination {
                    current = path;
                    search.clear();
                    status.clear();
                    selected = 0;
                    continue;
                }
                let index = if choice.is_empty() {
                    Some(selected)
                } else {
                    choice.parse::<usize>().ok().and_then(|n| n.checked_sub(1))
                };
                let path = if let Some(name) = index.and_then(|i| entries.get(i)) {
                    current.join(name)
                } else if choice.is_empty() {
                    status = "No images; choose another location.".into();
                    continue;
                } else {
                    current.join(choice.strip_prefix("path:").unwrap_or(&choice))
                };
                if path.is_dir() {
                    current = path;
                    search.clear();
                    status.clear();
                    selected = 0;
                    continue;
                }
                match import(&path) {
                    Ok(()) => return Ok(true),
                    Err(e) => {
                        status = format!(
                            "Cannot use {}: {e}. Choose another PNG/JPEG; nothing saved.",
                            path.display()
                        )
                    }
                }
            }
            _ => (),
        }
    }
}

pub(super) fn confirm(
    output: &mut impl Write,
    title: &str,
    report: &str,
) -> Result<bool, CliError> {
    let mut screen = editor::Screen::open(output)?;
    let mut scroll = 0_u16;
    loop {
        screen.terminal.draw(|frame| {
            if tui::resize_notice(frame) {
                return;
            }
            let areas = Layout::vertical([
                Constraint::Length(2),
                Constraint::Min(1),
                Constraint::Length(2),
            ])
            .split(frame.area());
            frame.render_widget(
                Block::default().style(Style::default().fg(Color::White).bg(Color::Black)),
                frame.area(),
            );
            frame.render_widget(Paragraph::new(title), areas[0]);
            frame.render_widget(
                Paragraph::new(report)
                    .wrap(Wrap { trim: false })
                    .scroll((scroll, 0)),
                areas[1],
            );
            frame.render_widget(
                Paragraph::new(
                    "y Confirm · Enter/n/Esc Cancel\nCancel (default) · ↑↓/PgUp/PgDown scroll",
                )
                .wrap(Wrap { trim: false }),
                areas[2],
            );
        })?;
        let key = form_key()?;
        if editor::interrupt(key)
            || matches!(key.code, KeyCode::Enter | KeyCode::Esc | KeyCode::Char('n'))
        {
            return Ok(false);
        }
        let size = screen.terminal.size()?;
        if tui::needs_resize(size.width, size.height) {
            continue;
        }
        match key.code {
            KeyCode::Char('y') => return Ok(true),
            KeyCode::Down => scroll = scroll.saturating_add(1),
            KeyCode::Up => scroll = scroll.saturating_sub(1),
            KeyCode::PageDown => scroll = scroll.saturating_add(10),
            KeyCode::PageUp => scroll = scroll.saturating_sub(10),
            _ => (),
        }
    }
}
