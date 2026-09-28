use super::*;
use crossterm::event::KeyCode;
use ratatui::{
    layout::{Constraint, Layout},
    style::{Color, Style},
    widgets::{Block, Paragraph, Wrap},
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc,
};

static READ_ONLY_ACTIVE: AtomicBool = AtomicBool::new(false);
struct ReadOnly;
impl ReadOnly {
    fn acquire() -> Option<Self> {
        READ_ONLY_ACTIVE
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| Self)
    }
}
impl Drop for ReadOnly {
    fn drop(&mut self) {
        READ_ONLY_ACTIVE.store(false, Ordering::Release);
    }
}
const BUSY: &str = "Previous read-only operation is still finishing. Retry later; no additional work started. You can still quit the browser.";

pub(super) fn report(
    command: char,
    output: &mut impl Write,
    selected: Option<IntentId>,
    seed: Option<ResolutionSeed>,
) -> Result<String, CliError> {
    let (title, consent) = match command {
        'p' => (
            "Previous Environment",
            Some(
                "Replay the prior Environment and append an Activation. Ghostty reload is best-effort.",
            ),
        ),
        'U' => (
            "Install update",
            Some(
                "Update Ghostty Wall at its owned installation path. Cargo builds can take minutes. After starting, cancellation is unavailable until the updater finishes publication or recovery. Do not terminate the process. Restart Ghostty Wall after replacement.",
            ),
        ),
        'X' => (
            "Uninstall integration",
            Some(
                "Remove the integration hook, Projection and cache. Intent and History stay. The browser remains open; use Initialize/Repair to restore integration.",
            ),
        ),
        'I' => (
            "Initialize integration",
            Some(
                "Create missing managed layout and install the Ghostty integration hook. This does not apply a Profile.",
            ),
        ),
        'R' => (
            "Repair integration",
            Some(
                "Repair managed layout and Ghostty integration. This does not restore a missing Projection; Use a Profile explicitly afterward.",
            ),
        ),
        'W' => (
            "Add welcome Profile",
            Some(
                "Create Welcome only for an eligible empty installation. Existing Profiles are preserved.",
            ),
        ),
        'M' => (
            "Migrate legacy configuration",
            Some(
                "Import legacy Sources and remove legacy integration hooks. Preview migration (dry-run) first if unsure.",
            ),
        ),
        'y' => ("Initialization dry-run", None),
        'Y' => ("Legacy migration dry-run", None),
        'l' => ("Profiles", None),
        'h' => ("History", None),
        's' => ("Settings and Sources", None),
        'P' => ("Plan JSON", None),
        'D' => ("Doctor", None),
        'u' => ("Check for updates", None),
        _ => return Ok("No action performed.".into()),
    };
    if let Some(consent) = consent
        && !forms::confirm(output, title, consent)?
    {
        return Ok(format!("{title} cancelled; no action started."));
    }
    let (_, report) = job(output, title, consent.is_none(), move |out| match command {
        'l' => command_list(out),
        'h' => {
            writeln!(
                out,
                "History (sequence Environment; empty until first apply):"
            )?;
            command_history(out)
        }
        's' => {
            let application = Application::load(seed)?;
            writeln!(
                out,
                "Settings: Managed Root {}",
                application.paths.managed_root().display()
            )?;
            writeln!(
                out,
                "Sources: {}. Profiles: {}.",
                application.config.sources.len(),
                profile_ids(&application.paths.managed_root().join("profiles"))?.len()
            )?;
            writeln!(
                out,
                "{}",
                read_text(&application.paths.managed_root().join("config.toml"))?
            )?;
            Ok(())
        }
        'P' => {
            let id = selected.ok_or_else(|| CliError::Usage("select Profile first".into()))?;
            serde_json::to_writer_pretty(
                &mut *out,
                &management::preview_application(&id, seed)?.plan(&id)?,
            )?;
            writeln!(out)?;
            Ok(())
        }
        'D' => command_doctor(out),
        'p' => command_previous(out),
        'u' | 'U' => {
            update::run(command == 'u', out).map_err(CliError::Update)?;
            if command == 'U' {
                writeln!(
                    out,
                    "Restart Ghostty Wall if an update was installed. This running process still uses its original version."
                )?;
            } else {
                writeln!(
                    out,
                    "To install here, return to Actions and choose Install update (U)."
                )?;
            }
            Ok(())
        }
        'X' => command_uninstall(out),
        _ => {
            let args: &[&str] = match command {
                'I' => &[],
                'R' => &["--repair"],
                'W' => &["--welcome"],
                'y' => &["--dry-run"],
                'Y' => &["--migrate-legacy", "--dry-run"],
                _ => &["--migrate-legacy"],
            };
            command_init(
                &args.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>(),
                out,
            )
        }
    })?;
    Ok(report)
}

pub(super) fn image(
    output: &mut impl Write,
    selected: Option<IntentId>,
    seed: Option<ResolutionSeed>,
) -> Result<String, CliError> {
    let Some(permit) = ReadOnly::acquire() else {
        management::details(output, BUSY)?;
        return Ok(BUSY.into());
    };
    let graphics = TerminalGraphics::from_environment();
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let _permit = permit;
        let result = (|| {
            let id = selected.ok_or_else(|| "Select a Profile first.".to_owned())?;
            let mut application =
                management::preview_application(&id, seed).map_err(|e| e.to_string())?;
            let mut browser = TerminalBrowser::new(Vec::new(), vec![id]);
            browser
                .dispatch(BrowserAction::NextPane, &mut application)
                .map_err(|e| e.to_string())?;
            browser
                .dispatch(BrowserAction::Preview, &mut application)
                .map_err(|e| e.to_string())?;
            let mut bytes = Vec::new();
            browser
                .render_preview_image(&mut bytes, graphics)
                .map_err(|e| e.to_string())?;
            Ok::<_, String>(String::from_utf8_lossy(&bytes).into_owned())
        })();
        let _ = sender.send(result);
    });
    let mut screen = editor::Screen::open(output)?;
    let mut prepared = None;
    let mut painted_size = None;
    let mut status = "Loading wallpaper image · read-only; not a Ghostty reload".to_owned();
    loop {
        if let Ok(result) = receiver.try_recv() {
            match result {
                Ok(image) if image.starts_with("\x1b_G") => {
                    prepared = Some(image);
                    status =
                        "Wallpaper image · original image, not Profile opacity or live Ghostty"
                            .into();
                }
                Ok(text) => status = text,
                Err(error) => status = format!("Image preview failed: {error}"),
            }
        }
        let size = screen.terminal.size()?;
        if painted_size.is_some_and(|old| old != size) {
            screen
                .terminal
                .backend_mut()
                .write_all(b"\x1b_Ga=d,d=I,i=43,q=2;\x1b\\")?;
            painted_size = None;
            screen.redraw()?;
        }
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
            frame.render_widget(
                Paragraph::new(status.as_str()).wrap(Wrap { trim: false }),
                areas[0],
            );
            frame.render_widget(
                Paragraph::new("Enter/Esc/q back · cancellation closes this read-only view")
                    .wrap(Wrap { trim: false }),
                areas[2],
            );
        })?;
        if painted_size.is_none()
            && !tui::needs_resize(size.width, size.height)
            && let Some(image) = &prepared
        {
            let image = image.replacen(
                "a=T,f=100,q=2,",
                &format!(
                    "a=T,f=100,q=2,i=43,C=1,c={},r={},",
                    size.width.max(1),
                    size.height.saturating_sub(4).max(1)
                ),
                1,
            );
            let out = screen.terminal.backend_mut();
            out.write_all(b"\x1b[3;1H")?;
            out.write_all(image.as_bytes())?;
            out.flush()?;
            painted_size = Some(size);
        }
        if !event::poll(std::time::Duration::from_millis(16))? {
            continue;
        }
        let key = match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => key,
            _ => continue,
        };
        if editor::interrupt(key)
            || matches!(key.code, KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q'))
        {
            screen
                .terminal
                .backend_mut()
                .write_all(b"\x1b_Ga=d,d=I,i=43,q=2;\x1b\\")?;
            return Ok(status);
        }
    }
}

pub(super) fn fresh_start(output: &mut impl Write) -> Result<bool, CliError> {
    let mut screen = editor::Screen::open(output)?;
    loop {
        screen.terminal.draw(|frame| {
            if tui::resize_notice(frame) { return; }
            frame.render_widget(Block::default().style(Style::default().fg(Color::White).bg(Color::Black)), frame.area());
            frame.render_widget(Paragraph::new("Ghostty Wall · Not initialized\n\ni Initialize integration (confirmation)\ny Preview initialization (dry-run)\nq / Esc / Ctrl-C quit\n\nInitialization creates managed files and the Ghostty integration hook. It does not activate a Profile.").wrap(Wrap { trim: false }), frame.area());
        })?;
        let key = editor::key()?;
        if editor::interrupt(key) || matches!(key.code, KeyCode::Esc | KeyCode::Char('q')) {
            return Ok(false);
        }
        let size = screen.terminal.size()?;
        if tui::needs_resize(size.width, size.height) {
            continue;
        }
        match key.code {
            KeyCode::Char('y') => {
                report('y', screen.terminal.backend_mut(), None, None)?;
            }
            KeyCode::Char('i') => {
                if forms::confirm(
                    screen.terminal.backend_mut(),
                    "Initialize integration",
                    "Create managed files and install the Ghostty integration hook? No Profile will be applied. Once started, wait for completion.",
                )? {
                    let (success, _) = job(
                        screen.terminal.backend_mut(),
                        "Initialize integration",
                        false,
                        |out| command_init(&[], out),
                    )?;
                    if success {
                        return Ok(true);
                    }
                }
            }
            _ => continue,
        }
        screen.redraw()?;
    }
}

pub(super) fn source(output: &mut impl Write) -> Result<String, CliError> {
    let mut values = vec![String::new(), String::new(), String::new()];
    loop {
        let mut report = Vec::new();
        let args = forms::fields(
            output,
            "Add Source · existing differing Sources are never overwritten",
            &[
                "New Source ID:",
                "Source kind (local/github):",
                "Directory path or owner/repo:",
            ],
            &mut values,
            |values| {
                IntentId::from_str(&values[0])?;
                if !matches!(values[1].as_str(), "local" | "github") {
                    return Err(CliError::Usage(
                        "Source kind must be local or github".into(),
                    ));
                }
                let args = vec![
                    "add".into(),
                    values[0].clone(),
                    values[1].clone(),
                    values[2].clone(),
                ];
                if values[1] == "local" {
                    command_source(&args, &mut report)?;
                }
                Ok(args)
            },
        )?;
        let Some(args) = args else {
            return Ok("Source form closed. No further changes requested.".into());
        };
        if values[1] == "local" {
            return Ok(String::from_utf8_lossy(&report).into_owned());
        }
        let mut options = vec![String::new(), String::new()];
        let saved = forms::fields(
            output,
            "GitHub Source options · Esc returns to Source fields",
            &[
                "Ref (blank for default):",
                "Subdirectory (blank for repository root):",
            ],
            &mut options,
            |options| {
                let mut args = args.clone();
                for (flag, value) in ["--ref", "--path"].iter().zip(options) {
                    if !value.is_empty() {
                        args.extend([(*flag).into(), value.clone()]);
                    }
                }
                report.clear();
                command_source(&args, &mut report)
            },
        )?;
        if saved.is_some() {
            return Ok(String::from_utf8_lossy(&report).into_owned());
        }
    }
}

enum Progress {
    Bytes(Vec<u8>),
    Done(bool),
}

pub(super) struct ProgressWriter {
    sender: mpsc::Sender<Progress>,
    written: usize,
}

impl Write for ProgressWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        const LIMIT: usize = 4 * 1024 * 1024;
        let count = bytes.len().min(LIMIT.saturating_sub(self.written));
        if count > 0 {
            self.sender
                .send(Progress::Bytes(bytes[..count].to_vec()))
                .map_err(|_| io::ErrorKind::BrokenPipe)?;
            self.written += count;
            if self.written == LIMIT {
                let _ = self.sender.send(Progress::Bytes(
                    b"\n[Report truncated at 4 MiB; use the standalone CLI for full output.]\n"
                        .to_vec(),
                ));
            }
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct Worker(Option<std::thread::JoinHandle<()>>);
impl Drop for Worker {
    fn drop(&mut self) {
        if let Some(worker) = self.0.take() {
            let _ = worker.join();
        }
    }
}

pub(super) fn job(
    output: &mut impl Write,
    title: &str,
    read_only: bool,
    work: impl FnOnce(&mut ProgressWriter) -> Result<(), CliError> + Send + 'static,
) -> Result<(bool, String), CliError> {
    let permit = if read_only {
        match ReadOnly::acquire() {
            Some(permit) => Some(permit),
            None => {
                management::details(output, BUSY)?;
                return Ok((false, BUSY.into()));
            }
        }
    } else {
        None
    };
    let mut screen = editor::Screen::open(output)?;
    let (sender, receiver) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let _permit = permit;
        let mut out = ProgressWriter { sender, written: 0 };
        let result = work(&mut out);
        if let Err(error) = &result {
            let _ = out.sender.send(Progress::Bytes(
                format!("\nFailed: {error}\nInspect the reported state before retrying.\n")
                    .into_bytes(),
            ));
        }
        let _ = out.sender.send(Progress::Done(result.is_ok()));
    });
    let mut worker = Worker(Some(worker));
    let mut bytes = Vec::new();
    let mut done = None;
    let mut scroll = 0_u16;
    let mut notice = String::new();
    loop {
        loop {
            match receiver.try_recv() {
                Ok(Progress::Bytes(chunk)) => bytes.extend(chunk),
                Ok(Progress::Done(success)) => done = Some(success),
                Err(mpsc::TryRecvError::Disconnected) => {
                    if done.is_none() {
                        bytes.extend(b"\nFailed: worker stopped unexpectedly. Inspect state before retrying.");
                        done = Some(false);
                    }
                    break;
                }
                Err(mpsc::TryRecvError::Empty) => break,
            }
        }
        let report: String = String::from_utf8_lossy(&bytes)
            .chars()
            .map(|c| {
                if c.is_control() && c != '\n' && c != '\t' {
                    '\u{fffd}'
                } else {
                    c
                }
            })
            .collect();
        screen.terminal.draw(|frame| {
            if tui::needs_resize(frame.area().width, frame.area().height) {
                let message = if !read_only && done.is_none() {
                    "Resize to 40x12.\nOperation running; cancellation unavailable. Wait for completion."
                } else {
                    "Resize to 40x12.\nEsc/Ctrl-C closes view."
                };
                frame.render_widget(Paragraph::new(message).style(Style::default().fg(Color::White).bg(Color::Black)).wrap(Wrap { trim: false }), frame.area());
                return;
            }
            let areas = Layout::vertical([
                Constraint::Length(2),
                Constraint::Min(1),
                Constraint::Length(3),
            ])
            .split(frame.area());
            frame.render_widget(
                Block::default().style(Style::default().fg(Color::White).bg(Color::Black)),
                frame.area(),
            );
            let status = match done {
                Some(true) => "Completed",
                Some(false) => "Failed",
                None => "Working",
            };
            frame.render_widget(
                Paragraph::new(format!("{title} — {status}")).wrap(Wrap { trim: false }),
                areas[0],
            );
            frame.render_widget(
                Paragraph::new(report.as_str())
                    .wrap(Wrap { trim: false })
                    .scroll((scroll, 0)),
                areas[1],
            );
            let controls = if done.is_some() {
                "↑↓/PgUp/PgDn scroll · Home top · Enter/Esc/q back"
            } else if read_only {
                "Working (read-only) · Esc/Ctrl-C closes view; work may finish in background"
            } else {
                "Working · Cannot cancel once started; waiting for safe completion"
            };
            frame.render_widget(
                Paragraph::new(format!("{controls}\n{notice}")).wrap(Wrap { trim: false }),
                areas[2],
            );
        })?;
        if !event::poll(std::time::Duration::from_millis(16))? {
            continue;
        }
        let key = match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => key,
            _ => continue,
        };
        let back = editor::interrupt(key) || matches!(key.code, KeyCode::Esc | KeyCode::Char('q'));
        if (back || key.code == KeyCode::Enter) && done.is_some() {
            return Ok((done == Some(true), report));
        }
        if back {
            if read_only {
                worker.0.take();
                return Ok((false, "Read-only report closed; work may finish in background. No maintenance mutation started.".into()));
            }
            notice = "Cancellation unavailable after consent. Wait for completion; do not terminate the process.".into();
        }
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => scroll = scroll.saturating_add(1),
            KeyCode::Up | KeyCode::Char('k') => scroll = scroll.saturating_sub(1),
            KeyCode::PageDown => scroll = scroll.saturating_add(10),
            KeyCode::PageUp => scroll = scroll.saturating_sub(10),
            KeyCode::Home => scroll = 0,
            _ => (),
        }
    }
}
