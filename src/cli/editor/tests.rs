use super::*;
use ratatui::backend::TestBackend;

fn editor() -> (tempfile::TempDir, ProfileEditor) {
    let temp = tempfile::tempdir().unwrap();
    let paths = InitPaths {
        home: temp.path().to_owned(),
        xdg_config_home: Some(temp.path().join("xdg")),
    };
    init(&paths).unwrap();
    let workflow = ProfileWorkflows::load(paths).unwrap();
    let mut draft = workflow.create("sample").unwrap();
    draft.set_document(workflow.config(), format!("schema_version = 1\n[colors]\nmode = \"explicit\"\nbackground = \"000000\"\nforeground = \"000000\"\npalette = [{}]\n", ["\"000000\""; 16].join(","))).unwrap();
    let editor = ProfileEditor::new(
        draft,
        workflow.config().clone(),
        EnvironmentManifest::new(None, None, None),
        None,
        None,
    )
    .unwrap();
    (temp, editor)
}
fn render(width: u16, height: u16, editor: &ProfileEditor, view: &View) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| {
            draw(
                frame,
                editor,
                &editor.preview().unwrap(),
                &None,
                view,
                &controls(view.group),
            )
        })
        .unwrap();
    let chrome = terminal.backend().buffer().cell((0, 0)).unwrap();
    assert_eq!(
        chrome.fg,
        Color::White,
        "unreadable draft must not recolor editor chrome"
    );
    assert_eq!(chrome.bg, Color::Black);
    terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|c| c.symbol())
        .collect()
}

#[test]
fn groups_confirmation_and_small_sample_layout_are_readable() {
    let (_temp, editor) = editor();
    assert_eq!(editor.color_origin(0).unwrap(), "Customized");
    assert_eq!(editor.color_origin(2).unwrap(), "Unmanaged");
    let wide = render(120, 30, &editor, &View::default());
    for label in [
        "Wallpaper-image opacity",
        "Internal sample",
        "Save and use",
        "NOT live Ghostty reload",
    ] {
        assert!(wide.contains(label), "missing {label}");
    }
    let small = render(
        60,
        14,
        &editor,
        &View {
            group: 2,
            ..View::default()
        },
    );
    assert!(small.contains("Terminal background opacity"));
    assert!(!small.contains("Internal sample"));
    let small_sample = render(
        60,
        14,
        &editor,
        &View {
            sample: true,
            ..View::default()
        },
    );
    assert!(small_sample.contains("Internal sample"));
    assert!(small_sample.contains("Selected text"));
    let confirmation = render(
        80,
        24,
        &editor,
        &View {
            mode: Mode::Confirm(false),
            ..View::default()
        },
    );
    assert!(confirmation.contains("Save and use Profile sample?"));
    assert!(confirmation.contains("[Back to editor]"));
    for mode in [
        Mode::Color(0, 0),
        Mode::Hex(0, "#AABBCC".into()),
        Mode::Number(NumericControl::FontSize, "14.5".into()),
        Mode::ResetColors,
    ] {
        render(
            40,
            12,
            &editor,
            &View {
                mode,
                ..View::default()
            },
        );
    }
}

#[test]
fn completion_controls_stay_visible_in_compact_editor_and_sample() {
    let (_temp, editor) = editor();
    for sample in [false, true] {
        let screen = render(
            40,
            12,
            &editor,
            &View {
                sample,
                ..View::default()
            },
        );
        for label in ["s Save", "u Save and use", "Esc/q Cancel", "v details"] {
            assert!(screen.contains(label), "missing {label}: {screen}");
        }
    }
    for group in 0..3 {
        let rows = controls(group);
        assert!(matches!(
            rows[rows.len() - 3].1,
            Control::Complete(Completion::Save)
        ));
        assert!(matches!(
            rows[rows.len() - 2].1,
            Control::Complete(Completion::SaveAndUse)
        ));
        assert!(matches!(
            rows[rows.len() - 1].1,
            Control::Complete(Completion::Cancel)
        ));
    }
}

#[test]
fn outcome_reports_separate_save_apply_and_reload_with_details() {
    use crate::{
        plan::ReloadUnavailableReason,
        runtime::{ReloadFailure, ReloadOutcome},
    };
    let id = "sample".parse().unwrap();
    assert!(saved_report(&id).starts_with("Profile saved."));
    let activation = crate::domain::ActivationId::new(1).unwrap();
    for (reload, message, detail) in [
        (
            ReloadOutcome::Succeeded,
            "Configuration updated; reload requested.",
            "visible change is not verified",
        ),
        (
            ReloadOutcome::Failed(ReloadFailure::Reload),
            "Configuration updated; reload Ghostty manually.",
            "Failed(Reload)",
        ),
        (
            ReloadOutcome::Unavailable(ReloadUnavailableReason::AdapterCommandUnavailable),
            "Configuration updated; reload Ghostty manually.",
            "AdapterCommandUnavailable",
        ),
    ] {
        let report = activation_report(activation, reload, Some(&id));
        assert!(report.starts_with(message));
        assert!(report.contains(detail));
    }
    let error = CliError::from(WorkflowError::Apply("uncertain commit; inspect".into()));
    assert!(
        error
            .to_string()
            .starts_with("Profile saved; applying failed. See details.")
    );
    assert!(error.to_string().contains("uncertain commit; inspect"));
    assert_eq!(error.exit_code(), 6);
}

#[test]
fn cancellation_reports_uncertain_save_instead_of_claiming_nothing_was_saved() {
    let error = WorkflowError::PublicationUncertain {
        path: PathBuf::from("profiles/boy.toml"),
        source: io::Error::other("injected fsync error"),
    };
    let result = cancel_result(Some(error)).unwrap_err();
    assert_eq!(result.exit_code(), 6);
    assert!(result.to_string().contains("durability is uncertain"));
    assert!(!result.to_string().contains("no files changed"));
    assert_eq!(cancel_result(None).unwrap(), Completion::Cancel);
}
