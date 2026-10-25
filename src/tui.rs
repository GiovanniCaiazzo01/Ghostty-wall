//! Profile management presentation; application services own every mutation.

pub(crate) mod sample;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};

use crate::{
    domain::IntentId,
    terminal_browser::{BrowserAction, BrowserFocus, BrowserMode, TerminalBrowser},
};
use sample::Sample;

// Explicit RGB keeps application chrome independent of remapped terminal ANSI slots.
// Window opacity, shaders and compositor effects remain terminal-owned.
pub(crate) const CHROME: Style = Style::new()
    .fg(Color::Rgb(229, 231, 235))
    .bg(Color::Rgb(15, 23, 42));
pub(crate) const SECONDARY: Style = Style::new().fg(Color::Rgb(166, 180, 204));
pub(crate) const EMPHASIS: Style = Style::new()
    .fg(Color::Rgb(170, 196, 255))
    .add_modifier(Modifier::BOLD);
pub(crate) const FOCUS: Style = Style::new()
    .fg(Color::Rgb(229, 231, 235))
    .bg(Color::Rgb(51, 75, 133));

pub(crate) const ACTIONS: &[(char, &str)] = &[
    ('n', "Create Profile"),
    ('e', "Edit Profile (visual draft)"),
    ('x', "Delete Profile (confirm)"),
    ('a', "Use selected Profile"),
    ('v', "View result / error details"),
    ('i', "View original wallpaper image"),
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
    ('s', "Show Settings / Source configuration"),
    ('P', "Show selected Profile Plan JSON"),
    ('p', "Previous Environment (confirm)"),
    ('D', "Doctor"),
    ('u', "Check for updates"),
    ('U', "Install update (confirm)"),
    ('I', "Initialize integration (confirm)"),
    ('y', "Preview initialization (dry-run)"),
    ('R', "Repair integration (confirm)"),
    ('W', "Add welcome Profile (confirm)"),
    ('Y', "Preview legacy migration (dry-run)"),
    ('M', "Migrate legacy configuration (confirm)"),
    ('S', "Show Source / dependent Profiles"),
    ('E', "Edit Source (confirm)"),
    ('C', "Check Source availability"),
    ('Z', "Remove unused Source (confirm)"),
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
        KeyCode::Char(c) if "Nnmoefrdxhstcw?ivSECZ".contains(c) => Input::Command(c),
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
            .style(CHROME)
            .wrap(Wrap { trim: false }),
        frame.area(),
    );
    true
}

pub(crate) fn management_needs_resize(width: u16, height: u16) -> bool {
    width < 60 || height < 18
}

fn compact_item(text: &str, width: u16) -> ListItem<'static> {
    let width = usize::from(width.max(1));
    if text.chars().count() <= width {
        return ListItem::new(text.to_owned());
    }
    ListItem::new(
        text.chars()
            .take(width - 1)
            .chain(['…'])
            .collect::<String>(),
    )
}

pub(crate) fn preview_area(
    area: ratatui::layout::Rect,
    view: &View,
) -> Option<ratatui::layout::Rect> {
    if management_needs_resize(area.width, area.height) || view.menu.is_some() {
        return None;
    }
    let wide = area.width >= 90 && area.height >= 18;
    let areas = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(1),
        Constraint::Length(if wide { 5 } else { 3 }),
    ])
    .split(area);
    if !wide && view.show_sample {
        return Some(areas[1]);
    }
    Some(
        Layout::horizontal([
            if wide {
                Constraint::Length(26)
            } else {
                Constraint::Length(20)
            },
            Constraint::Min(1),
        ])
        .split(areas[1])[1],
    )
}

pub(crate) fn draw(frame: &mut Frame, browser: &TerminalBrowser, view: &View) {
    if management_needs_resize(frame.area().width, frame.area().height) {
        frame.render_widget(
            Paragraph::new("Resize management to 60x18.\nEsc/Ctrl-C cancels.\nCreate/Edit forms still support 40x12.")
                .style(CHROME)
                .wrap(Wrap { trim: false }),
            frame.area(),
        );
        return;
    }
    frame.render_widget(Block::default().style(CHROME), frame.area());
    let wide = frame.area().width >= 90 && frame.area().height >= 18;
    let compact_sample = !wide && view.show_sample && view.menu.is_none();
    let areas = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(1),
        Constraint::Length(if wide { 5 } else { 3 }),
    ])
    .split(frame.area());
    let selected = format!(
        "Preview: {}",
        browser.selected_profile().map_or("none", IntentId::as_str)
    );
    let characters: Vec<_> = selected.chars().collect();
    let mut heading: Vec<String> = characters
        .chunks(usize::from(areas[0].width.max(1)))
        .map(|line| line.iter().collect())
        .collect();
    if heading.len() == 1 {
        heading.push(
            if wide {
                "Internal preview; NOT live Ghostty reload"
            } else {
                "Static preview (not live)"
            }
            .into(),
        );
    }
    frame.render_widget(Paragraph::new(heading.join("\n")), areas[0]);
    if let Some(index) = view.menu {
        let list = List::new(ACTIONS.iter().map(|(key, name)| {
            wrapped_item(&format!("{key}  {name}"), areas[1].width.saturating_sub(4))
        }))
        .block(Block::default().title("Actions").borders(Borders::ALL))
        .highlight_symbol("> ")
        .highlight_style(FOCUS);
        frame.render_stateful_widget(
            list,
            areas[1],
            &mut ListState::default().with_selected(Some(index)),
        );
    } else {
        let panes = Layout::horizontal([
            if wide {
                Constraint::Length(26)
            } else {
                Constraint::Length(20)
            },
            Constraint::Min(1),
        ])
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
            let area = panes[0];
            let list = List::new(items.iter().map(|id| {
                let active = title == "Profiles" && view.active.as_ref() == Some(id);
                if wide {
                    wrapped_item(
                        &format!("{id}{}", if active { " [active]" } else { "" }),
                        area.width.saturating_sub(4),
                    )
                } else {
                    compact_item(
                        &format!("{}{id}", if active { "* " } else { "" }),
                        area.width.saturating_sub(4),
                    )
                }
            }))
            .block(Block::default().title(title).borders(Borders::RIGHT))
            .highlight_symbol("> ")
            .highlight_style(FOCUS);
            frame.render_stateful_widget(
                list,
                area,
                &mut ListState::default()
                    .with_selected(items.iter().position(|id| Some(id) == selected)),
            );
        }
        {
            let area = if compact_sample { areas[1] } else { panes[1] };
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
    } else if !wide {
        "n Create · e Edit · x Delete · a Use\n↑↓ select · p preview · q quit"
    } else {
        "n Create · e Edit · x Delete · a Use · ? Actions · q quit\n↑↓ select · p preview · Tab Sources · v details"
    };
    let status = if view.status.is_empty() && !view.preview_error.is_empty() {
        "Preview unavailable; v details"
    } else if view.status.is_empty() && !wide {
        "? Actions · v details · Tab Sources · * active"
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
    fn preview_geometry_matches_rendered_surface_and_reserves_footer() {
        let browser = TerminalBrowser::new(vec![], vec!["night".parse().unwrap()]);
        for (width, height) in [(60, 18), (80, 20), (90, 18), (100, 30), (160, 48)] {
            for show_sample in [false, true] {
                let view = View {
                    show_sample,
                    ..View::default()
                };
                let area = ratatui::layout::Rect::new(0, 0, width, height);
                let preview = preview_area(area, &view).unwrap();
                let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
                terminal.draw(|frame| draw(frame, &browser, &view)).unwrap();
                let buffer = terminal.backend().buffer();
                assert_eq!(buffer[(0, 0)].fg, Color::Rgb(229, 231, 235));
                assert_eq!(buffer[(0, 0)].bg, Color::Rgb(15, 23, 42));
                assert_eq!(buffer[(preview.x, preview.y)].symbol(), "P");
                assert_eq!(
                    preview.bottom(),
                    height - if width >= 90 { 5 } else { 3 },
                    "footer must stay outside graphics"
                );
                if width >= 90 {
                    assert_eq!(preview.x, 26);
                    assert!(
                        preview.width > width / 2,
                        "preview must dominate wide layout"
                    );
                }
                let menu = View {
                    menu: Some(0),
                    ..View::default()
                };
                assert!(
                    preview_area(area, &menu).is_none(),
                    "graphics must not cover menus"
                );
            }
        }
    }

    #[test]
    fn result_footer_has_room_for_save_outcome_and_detail() {
        let browser = TerminalBrowser::new(vec![], vec!["night".parse().unwrap()]);
        let view = View {
            status: "Profile saved. Saved Profile generated.\nNot applied; terminal and History unchanged.".into(),
            ..View::default()
        };
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
        terminal.draw(|frame| draw(frame, &browser, &view)).unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(text.contains("Saved Profile generated."));
        assert!(text.contains("Not applied; terminal and History unchanged."));
        assert_eq!(
            preview_area(ratatui::layout::Rect::new(0, 0, 100, 30), &view)
                .unwrap()
                .bottom(),
            25
        );
    }

    #[test]
    fn management_minimum_preserves_photo_space_without_changing_form_minimum() {
        let browser = TerminalBrowser::new(vec![], vec!["night".parse().unwrap()]);
        for (width, height) in [(40, 12), (59, 18), (60, 17)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal
                .draw(|frame| draw(frame, &browser, &View::default()))
                .unwrap();
            let text: String = terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .map(|c| c.symbol())
                .collect();
            assert!(text.contains("60x18"), "{width}x{height}: {text}");
            assert!(text.contains("Esc/Ctrl-C"));
        }
        assert!(
            !needs_resize(40, 12),
            "Create/Edit minimum must remain unchanged"
        );
    }

    #[test]
    fn compact_labels_are_elided_in_list_but_selected_name_remains_visible() {
        let name = "a".repeat(64);
        let browser =
            TerminalBrowser::new(vec![name.parse().unwrap()], vec![name.parse().unwrap()]);
        let mut terminal = Terminal::new(TestBackend::new(60, 18)).unwrap();
        terminal
            .draw(|frame| draw(frame, &browser, &View::default()))
            .unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(
            text.contains("…"),
            "compact lists should not wrap long identifiers"
        );
        assert!(text.contains(&format!("Preview: {name}")));
    }

    #[test]
    fn small_layout_keeps_actions_and_last_menu_entry_visible() {
        let browser = TerminalBrowser::new(vec![], vec!["night".parse().unwrap()]);
        for (width, height) in [(60, 18), (80, 20), (120, 30)] {
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
            assert!(terminal.backend().buffer().content().iter().any(|cell| {
                cell.fg == Color::Rgb(229, 231, 235) && cell.bg == Color::Rgb(51, 75, 133)
            }));
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
        assert!(notice.contains("Resize management to 60x18"));
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
