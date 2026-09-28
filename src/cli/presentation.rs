use std::{
    env,
    io::{self, IsTerminal, Write},
};

use crossterm::style::{Color, Stylize};

use super::CliError;

#[derive(Clone, Copy)]
pub(super) enum Role {
    Heading,
    Choice,
    Warning,
    Error,
    Success,
}

fn styled() -> bool {
    io::stdin().is_terminal()
        && io::stdout().is_terminal()
        && env::var_os("NO_COLOR").is_none()
        && env::var("TERM").is_ok_and(|term| !term.is_empty() && term != "dumb")
}

fn text(output: &mut impl Write, role: Role, value: &str) -> Result<(), CliError> {
    let wrapped;
    let value = if io::stdout().is_terminal() {
        let width = crossterm::terminal::size()
            .map(|(width, _)| usize::from(width))
            .unwrap_or(80);
        wrapped = wrap(value, width.saturating_sub(1).max(8));
        &wrapped
    } else {
        value
    };
    if styled() {
        let color = match role {
            Role::Heading => Color::Cyan,
            Role::Choice => Color::Blue,
            Role::Warning => Color::Yellow,
            Role::Error => Color::Red,
            Role::Success => Color::Green,
        };
        let value = value.with(color);
        if matches!(role, Role::Heading) {
            write!(output, "{}", value.bold())?;
        } else {
            write!(output, "{value}")?;
        }
    } else {
        write!(output, "{value}")?;
    }
    Ok(())
}

fn wrap(value: &str, width: usize) -> String {
    let mut result = String::new();
    let mut column = 0;
    for ch in value.chars() {
        if ch == '\n' {
            result.push(ch);
            column = 0;
            continue;
        }
        let cells = ratatui::text::Span::raw(ch.to_string()).width();
        if column + cells > width {
            result.push_str("\n    ");
            column = 4;
        }
        result.push(ch);
        column += cells;
    }
    result
}

pub(super) fn line(output: &mut impl Write, role: Role, value: &str) -> Result<(), CliError> {
    text(output, role, value)?;
    writeln!(output)?;
    Ok(())
}

pub(super) fn heading(output: &mut impl Write, value: &str) -> Result<(), CliError> {
    writeln!(output)?;
    line(output, Role::Heading, value)
}

pub(super) fn choices(
    output: &mut impl Write,
    title: &str,
    options: &[&str],
) -> Result<(), CliError> {
    heading(output, title)?;
    writeln!(output)?;
    for option in options {
        line(output, Role::Choice, &format!("  {option}"))?;
    }
    Ok(())
}

pub(super) fn prompt(output: &mut impl Write, label: &str) -> Result<(), CliError> {
    writeln!(output)?;
    text(output, Role::Heading, label)
}

pub(super) fn error(output: &mut impl Write, message: &str) -> Result<(), CliError> {
    writeln!(output)?;
    line(output, Role::Error, &format!("Error: {message}"))
}
