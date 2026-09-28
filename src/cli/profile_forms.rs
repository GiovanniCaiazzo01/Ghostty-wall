use super::*;

pub(super) fn run(
    command: char,
    output: &mut impl Write,
    browser: &TerminalBrowser,
) -> Result<(Option<IntentId>, String), CliError> {
    let selected = browser.selected_profile().cloned();
    if matches!(command, 'N' | 'm') {
        let source = if browser.focus() == BrowserFocus::Sources {
            browser
                .selected_source()
                .map(ToString::to_string)
                .unwrap_or_default()
        } else {
            String::new()
        };
        let (title, labels, mut values) = if command == 'N' {
            (
                "Import image · Save Profile without applying",
                vec!["New Profile ID:", "PNG/JPEG image path:"],
                vec![String::new(), String::new()],
            )
        } else {
            (
                "Create from Source · Save Profile without applying",
                vec![
                    "New Profile ID:",
                    "Source ID:",
                    "Candidate path relative to Source root:",
                ],
                vec![String::new(), source, String::new()],
            )
        };
        let result = forms::fields(output, title, &labels, &mut values, |values| {
            let mut report = Vec::new();
            if command == 'N' {
                command_new(values, &mut report)?;
            } else {
                new_from_source(&values[0], &values[1], &values[2], false, &mut report)?;
            }
            Ok((
                Some(IntentId::from_str(&values[0])?),
                String::from_utf8_lossy(&report).into_owned(),
            ))
        })?;
        return Ok(result.unwrap_or((
            selected,
            "Form cancelled; no further changes requested.".into(),
        )));
    }
    let id = selected
        .as_ref()
        .ok_or_else(|| CliError::Input("Select a Profile first; no files changed.".into()))?;
    if command == 'x' {
        let mut report = Vec::new();
        delete::with_confirmation(id.as_str(), &mut report, |summary| {
            forms::confirm(output, "Delete Profile", summary)
        })?;
        return Ok((selected, String::from_utf8_lossy(&report).into_owned()));
    }
    if matches!(command, 'f' | 'c' | 't' | 'w') {
        let (prefix, title) = match command {
            'c' => (
                "colors.",
                "Color fields: mode, theme, background, foreground, cursor, selection_background, selection_foreground, palette.0..15",
            ),
            't' => (
                "terminal.",
                "Terminal fields: font_size, background_opacity, background_blur_intensity, cursor_style",
            ),
            'w' => (
                "wallpaper.",
                "Wallpaper fields: mode, source, path, fit, position, opacity, repeat",
            ),
            _ => ("", "Fields: wallpaper.*, colors.*, terminal.*"),
        };
        let title = format!("Save immediately, without applying · Profile {id}\n{title}");
        let mut values = [String::new(), String::new()];
        let result = forms::fields(
            output,
            &title,
            &["Field:", "Value:"],
            &mut values,
            |values| {
                let field = if !prefix.is_empty() && !values[0].contains('.') {
                    format!("{prefix}{}", values[0])
                } else {
                    values[0].clone()
                };
                let mut report = Vec::new();
                command_edit(&[id.to_string(), field, values[1].clone()], &mut report)?;
                Ok((
                    selected.clone(),
                    String::from_utf8_lossy(&report).into_owned(),
                ))
            },
        )?;
        return Ok(result.unwrap_or((
            selected,
            "Form cancelled; no further changes requested.".into(),
        )));
    }
    let operation = if command == 'r' {
        "rename"
    } else {
        "duplicate"
    };
    let mut values = [String::new()];
    let result = forms::fields(
        output,
        operation,
        &["New Profile ID:"],
        &mut values,
        |values| {
            let mut report = Vec::new();
            command_profile_file(&[id.to_string(), values[0].clone()], operation, &mut report)?;
            Ok((
                Some(IntentId::from_str(&values[0])?),
                String::from_utf8_lossy(&report).into_owned(),
            ))
        },
    )?;
    Ok(result.unwrap_or((
        selected,
        "Form cancelled; no further changes requested.".into(),
    )))
}
