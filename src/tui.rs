//! Profile management presentation; application services own every mutation.

pub(crate) mod sample;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Layout},
    style::{Color, Style},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};

use crate::{
    domain::IntentId,
    terminal_browser::{BrowserAction, BrowserFocus, BrowserMode, TerminalBrowser},
};
use sample::Sample;

pub(crate) const ACTIONS: &[(char, &str)] = &[
    ('n', "Create Profile"),
    ('e', "Edit Profile (visual draft)"),
    ('x', "Delete Profile (confirm)"),
    ('a', "Use selected Profile"),
    ('v', "View result / error details"),
    ('N', "Advanced: new image Profile"),
    ('m', "New Profile from Source"),
    ('o', "Add Source"),
    ('f', "Advanced: edit field immediately"),
    ('c', "Advanced: edit color field"),
    ('t', "Advanced: edit terminal field"),
    ('w', "Advanced: edit wallpaper field"),
    ('r', "Rename Profile"),
    ('d', "Duplicate Profile"),
    ('l', "List Profiles"),
    ('h', "Show History"),
    ('s', "Show Settings"),
    ('P', "Show selected Profile Plan JSON"),
    ('p', "Previous Environment (confirm)"),
    ('D', "Doctor"),
    ('u', "Check for updates"),
    ('U', "Install update (confirm)"),
    ('I', "Initialize integration"),
    ('y', "Preview initialization (dry-run)"),
    ('R', "Repair integration"),
    ('W', "Add welcome Profile"),
    ('Y', "Preview legacy migration (dry-run)"),
    ('M', "Migrate legacy configuration (confirm)"),
    ('X', "Uninstall integration (confirm)"),
];

pub(crate) enum Input {
    Action(BrowserAction),
    Command(char),
    Ignore,
}

pub(crate) fn input(key: KeyEvent, _mode: BrowserMode) -> Input {
    if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c' | 'd'))
    {
        return Input::Action(BrowserAction::Cancel);
    }
    match key.code {
        KeyCode::Char('q') => Input::Action(BrowserAction::Cancel),
        KeyCode::Esc | KeyCode::Backspace | KeyCode::Char('b') => {
            Input::Action(BrowserAction::Back)
        }
        KeyCode::Up | KeyCode::Char('k') => Input::Action(BrowserAction::Up),
        KeyCode::Down | KeyCode::Char('j') => Input::Action(BrowserAction::Down),
        KeyCode::Tab => Input::Action(BrowserAction::NextPane),
        KeyCode::Enter | KeyCode::Char('p') => Input::Action(BrowserAction::Preview),
        KeyCode::Char('a') => Input::Command('a'),
        KeyCode::Char(c) if "Nnmoefrdxhstcw?iv".contains(c) => Input::Command(c),
        _ => Input::Ignore,
    }
}

pub(crate) fn menu_input(key: KeyEvent, menu: &mut Option<usize>) -> Input {
    if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c' | 'd'))
    {
        return Input::Action(BrowserAction::Cancel);
    }
    let Some(index) = menu.as_mut() else {
        return Input::Ignore;
    };
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => *index = index.saturating_sub(1),
        KeyCode::Down | KeyCode::Char('j') => *index = (*index + 1).min(ACTIONS.len() - 1),
        KeyCode::Home => *index = 0,
        KeyCode::End => *index = ACTIONS.len() - 1,
        KeyCode::Esc | KeyCode::Char('?') => *menu = None,
        KeyCode::Enter => {
            let command = ACTIONS[*index].0;
            *menu = None;
            return Input::Command(command);
        }
        KeyCode::Char(c) if ACTIONS.iter().any(|(key, _)| *key == c) => {
            *menu = None;
            return Input::Command(c);
        }
        _ => {}
    }
    Input::Ignore
}

#[derive(Default)]
pub(crate) struct View {
    pub status: String,
    pub menu: Option<usize>,
    pub sample: Option<Sample>,
    pub preview_error: String,
    pub active: Option<IntentId>,
    pub show_sample: bool,
}

/// Wrap list entries so a selected long label/value remains reachable at small widths.
pub(crate) fn wrapped_item(text: &str, width: u16) -> ListItem<'static> {
    let width = usize::from(width.max(1));
    let chars: Vec<_> = text.chars().collect();
    let lines = chars
        .chunks(width)
        .map(|c| c.iter().collect::<String>())
        .collect::<Vec<_>>();
    ListItem::new(lines.join("\n"))
}

pub(crate) fn needs_resize(width: u16, height: u16) -> bool {
    width < 40 || height < 12
}

pub(crate) fn resize_notice(frame: &mut Frame) -> bool {
    if !needs_resize(frame.area().width, frame.area().height) {
        return false;
    }
    frame.render_widget(
        Paragraph::new("Resize to 40x12.\nEsc/Ctrl-C cancels.")
            .style(Style::default().fg(Color::White).bg(Color::Black))
            .wrap(Wrap { trim: false }),
        frame.area(),
    );
    true
}

pub(crate) fn draw(frame: &mut Frame, browser: &TerminalBrowser, view: &View) {
    if resize_notice(frame) {
        return;
    }
    frame.render_widget(
        Block::default().style(Style::default().fg(Color::White).bg(Color::Black)),
        frame.area(),
    );
    let wide = frame.area().width >= 90 && frame.area().height >= 18;
    let compact_sample = !wide && view.show_sample && view.menu.is_none();
    let areas = Layout::vertical([
        Constraint::Length(if compact_sample { 1 } else { 2 }),
        Constraint::Min(1),
        Constraint::Length(if compact_sample { 2 } else { 5 }),
    ])
    .split(frame.area());
    frame.render_widget(
        Paragraph::new(if compact_sample {
            "Internal; NOT live Ghostty reload"
        } else {
            "Ghostty Wall · Profiles\nInternal preview; NOT live Ghostty reload"
        }),
        areas[0],
    );
    if let Some(index) = view.menu {
        let list = List::new(ACTIONS.iter().map(|(key, name)| {
            wrapped_item(&format!("{key}  {name}"), areas[1].width.saturating_sub(4))
        }))
        .block(Block::default().title("Actions").borders(Borders::ALL))
        .highlight_symbol("> ")
        .highlight_style(Style::default().fg(Color::Black).bg(Color::Cyan));
        frame.render_stateful_widget(
            list,
            areas[1],
            &mut ListState::default().with_selected(Some(index)),
        );
    } else {
        let panes = Layout::horizontal([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(areas[1]);
        if wide || !view.show_sample {
            let (title, items, selected) = if browser.focus() == BrowserFocus::Profiles {
                ("Profiles", browser.profiles(), browser.selected_profile())
            } else {
                (
                    "Sources (advanced)",
                    browser.sources(),
                    browser.selected_source(),
                )
            };
            let area = if wide { panes[0] } else { areas[1] };
            let list = List::new(items.iter().map(|id| {
                wrapped_item(
                    &format!(
                        "{id}{}",
                        if title == "Profiles" && view.active.as_ref() == Some(id) {
                            " [active]"
                        } else {
                            ""
                        }
                    ),
                    area.width.saturating_sub(4),
                )
            }))
            .block(Block::default().title(title).borders(Borders::ALL))
            .highlight_symbol("> ")
            .highlight_style(Style::default().fg(Color::Black).bg(Color::Cyan));
            frame.render_stateful_widget(
                list,
                area,
                &mut ListState::default()
                    .with_selected(items.iter().position(|id| Some(id) == selected)),
            );
        }
        if wide || view.show_sample {
            let area = if wide { panes[1] } else { areas[1] };
            if let Some(sample) = &view.sample {
                sample.draw(frame, area);
            } else {
                frame.render_widget(
                    Paragraph::new(format!(
                        "Preview unavailable: {}\nNo activation; v shows details.",
                        view.preview_error
                    ))
                    .wrap(Wrap { trim: false }),
                    area,
                );
            }
        }
    }
    let keys = if view.menu.is_some() {
        "↑↓ choose · Enter run · Esc back\nLetter shortcut · q via Esc then q"
    } else if compact_sample {
        "p list · n Create · e Edit\nx Delete · a Use · q quit"
    } else {
        "n Create · e Edit · x Delete · a Use\n↑↓ select · p preview · Tab Sources\n? Actions · v details · q quit"
    };
    let status = if view.status.is_empty() && !view.preview_error.is_empty() {
        "Preview unavailable; v details"
    } else {
        &view.status
    };
    frame.render_widget(
        Paragraph::new(format!("{keys}\n{status}")).wrap(Wrap { trim: false }),
        areas[2],
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn small_layout_keeps_actions_and_last_menu_entry_visible() {
        let browser = TerminalBrowser::new(vec![], vec!["night".parse().unwrap()]);
        for (width, height) in [(40, 12), (60, 14), (120, 30)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            let mut view = View::default();
            terminal.draw(|frame| draw(frame, &browser, &view)).unwrap();
            let text = |t: &Terminal<TestBackend>| {
                t.backend()
                    .buffer()
                    .content()
                    .iter()
                    .map(|c| c.symbol())
                    .collect::<String>()
            };
            for label in [
                "n Create",
                "e Edit",
                "x Delete",
                "a Use",
                "p preview",
                "q quit",
            ] {
                assert!(
                    text(&terminal).contains(label),
                    "missing {label} at {width}x{height}"
                );
            }
            view.menu = Some(ACTIONS.len() - 1);
            terminal.draw(|frame| draw(frame, &browser, &view)).unwrap();
            assert!(text(&terminal).contains("Uninstall"));
        }
        let mut tiny = Terminal::new(TestBackend::new(30, 8)).unwrap();
        tiny.draw(|frame| draw(frame, &browser, &View::default()))
            .unwrap();
        let notice = tiny
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect::<String>();
        assert!(notice.contains("Resize to 40x12"));
        assert!(notice.contains("Esc/Ctrl-C cancels"));
        assert!(matches!(
            input(KeyEvent::from(KeyCode::Down), BrowserMode::Browse),
            Input::Action(BrowserAction::Down)
        ));
        assert!(matches!(
            input(KeyEvent::from(KeyCode::Char('a')), BrowserMode::Browse),
            Input::Command('a')
        ));
        let mut menu = Some(0);
        assert!(matches!(
            menu_input(KeyEvent::from(KeyCode::Char('p')), &mut menu),
            Input::Command('p')
        ));
    }
}
