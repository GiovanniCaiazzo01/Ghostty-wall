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
    assert!(
        matches!(chrome.fg, Color::Rgb(..)),
        "draft and terminal ANSI slots must not recolor editor chrome"
    );
    assert_eq!(chrome.bg, Color::Rgb(15, 23, 42));
    terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|c| c.symbol())
        .collect()
}

#[test]
fn infeasibility_stays_visible_before_png_and_in_compact_controls() {
    let (temp, mut editor) = editor();
    let workflow = ProfileWorkflows::load(InitPaths {
        home: temp.path().to_owned(),
        xdg_config_home: Some(temp.path().join("xdg")),
    })
    .unwrap();
    let path = temp.path().join("white.png");
    std::fs::write(&path, include_bytes!("../../../tests/fixtures/white.png")).unwrap();
    editor.import_image(&workflow, &path).unwrap();
    editor
        .set_number(NumericControl::WallpaperOpacity, "0.65")
        .unwrap();
    editor.set_number(NumericControl::ImageDarken, "0").unwrap();
    assert!(editor.palette_contrast_infeasible().unwrap());
    for (width, height) in [(40, 12), (60, 18), (100, 18), (160, 48)] {
        for sample in [false, true] {
            let text = render(
                width,
                height,
                &editor,
                &View {
                    sample,
                    ..View::default()
                },
            );
            assert!(
                text.contains("Contrast infeasible; fallback"),
                "{width}x{height}"
            );
            assert!(text.contains("Save and use"));
            assert!(text.contains("Cancel"));
        }
    }
}

#[test]
fn preview_led_layout_keeps_group_focus_and_scrolled_actions_reachable() {
    let (_temp, editor) = editor();
    for (width, height) in [(40, 12), (60, 18), (99, 30), (100, 18), (160, 48)] {
        for group in 0..3 {
            let rows = controls(group);
            let view = View {
                group,
                row: rows.len() - 1,
                ..View::default()
            };
            let text = render(width, height, &editor, &view);
            assert!(
                text.contains("> Cancel"),
                "selected last action lost at {width}x{height}"
            );
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal
                .draw(|frame| {
                    draw(
                        frame,
                        &editor,
                        &editor.preview().unwrap(),
                        &None,
                        &view,
                        &rows,
                    )
                })
                .unwrap();
            let buffer = terminal.backend().buffer();
            assert_eq!(buffer[(0, 0)].fg, Color::Rgb(170, 196, 255));
            assert!(
                buffer[(0, 0)]
                    .modifier
                    .contains(ratatui::style::Modifier::BOLD)
            );
            assert_eq!(buffer[(0, height - 3)].fg, Color::Rgb(170, 196, 255));
            assert_eq!(buffer[(0, height - 2)].fg, Color::Rgb(166, 180, 204));
            assert_eq!(buffer[(0, height - 1)].fg, Color::Rgb(229, 231, 235));
            let tab_x = [5, 17, 26][group];
            assert_eq!(
                terminal.backend().buffer()[(tab_x, 1)].bg,
                Color::Rgb(51, 75, 133)
            );
            assert!(
                text.contains("Home/End"),
                "scroll hint lost at {width}x{height}"
            );
            assert!(text.contains(&format!("{}/{}", rows.len(), rows.len())));
            if width >= 100 {
                assert!(text.contains("Internal sample"));
                assert!(width - 44 > width / 2, "preview must dominate");
                assert_eq!(terminal.backend().buffer()[(0, 2)].symbol(), "┌");
                assert_eq!(
                    preview_area(Rect::new(0, 0, width, height), &view).unwrap(),
                    Rect::new(0, 2, width - 44, height - 5),
                    "graphics and preview pane must share geometry"
                );
                assert_eq!(terminal.backend().buffer()[(width - 44, 3)].symbol(), "│");
            }
        }
    }
}

#[test]
fn editor_worker_receives_only_bounded_source_and_deduplicates_unchanged_requests() {
    use crate::domain::{ImageWallpaper, MediaType, Sha256Digest, WallpaperManifest};
    use sha2::{Digest, Sha256};
    let (temp, _) = editor();
    let paths = InitPaths {
        home: temp.path().to_owned(),
        xdg_config_home: Some(temp.path().join("xdg")),
    };
    let workflow = ProfileWorkflows::load(paths).unwrap();
    let mut draft = workflow.create("bounded").unwrap();
    draft.set_document(workflow.config(), "schema_version = 1\n[wallpaper]\nmode = \"source\"\nsource = \"welcome\"\nselection = \"path\"\npath = \"welcome.png\"\n".into()).unwrap();
    let image = image::RgbImage::from_fn(1024, 768, |x, y| {
        image::Rgb([(x % 256) as u8, (y % 256) as u8, 90])
    });
    let mut png = std::io::Cursor::new(Vec::new());
    image.write_to(&mut png, image::ImageFormat::Png).unwrap();
    let manifest = EnvironmentManifest::new(
        Some(WallpaperManifest::Image(ImageWallpaper::new(
            Sha256Digest::from_bytes(Sha256::digest(png.get_ref()).into()),
            MediaType::Png,
        ))),
        None,
        None,
    );
    let mut editor = ProfileEditor::new(
        draft,
        workflow.config().clone(),
        manifest,
        Some(png.into_inner()),
        None,
    )
    .unwrap();
    let opening_image = editor.image().unwrap().to_vec();
    editor.defer_effects_to_preview_worker();
    let source = editor.preview_image_source().unwrap().to_vec();
    let reduced = image::load_from_memory(&source).unwrap();
    assert!(reduced.width() <= 384 && reduced.height() <= 256);
    let mut preview = preview::Preview::new().unwrap();
    let area = Rect::new(0, 2, 56, 25);
    assert!(
        preview
            .request(&editor, &editor.preview().unwrap(), area)
            .unwrap()
    );
    assert!(
        !preview
            .request(&editor, &editor.preview().unwrap(), area)
            .unwrap()
    );
    for (control, value) in [
        (NumericControl::ImageBlur, "32"),
        (NumericControl::ImageDarken, "60"),
        (NumericControl::ImageBlur, "0"),
    ] {
        editor.set_number(control, value).unwrap();
        assert_eq!(editor.preview_image_source().unwrap(), source);
        assert_eq!(
            editor.image().unwrap(),
            opening_image,
            "input setter rendered an image"
        );
        assert!(
            preview
                .request(&editor, &editor.preview().unwrap(), area)
                .unwrap()
        );
    }
}

#[test]
fn preview_geometry_hides_images_for_popups_and_small_controls() {
    let mut view = View::default();
    for (width, height) in [(40, 12), (60, 18), (100, 18), (160, 48)] {
        let area = Rect::new(0, 0, width, height);
        let preview = preview_area(area, &view);
        assert_eq!(preview.is_some(), width >= 100);
        view.sample = true;
        assert!(preview_area(area, &view).is_some());
        view.mode = Mode::Confirm(false);
        assert!(preview_area(area, &view).is_none());
        view = View::default();
    }
    let (_temp, editor) = editor();
    let text = render(100, 18, &editor, &View::default());
    assert!(text.contains("Auto darkening (Automatic roles)"));
    assert!(text.contains("Image darkening (0–100%)"));
}

#[test]
fn ready_preview_keeps_contrast_warning_visible() {
    let (_temp, editor) = editor();
    let manifest = editor.preview().unwrap();
    let mut sample = crate::tui::sample::Sample::for_preview(
        manifest.clone(),
        Some(include_bytes!("../../../tests/fixtures/white.png")),
    )
    .unwrap();
    sample.prepare_graphics(Rect::new(0, 2, 56, 25)).unwrap();
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    terminal
        .draw(|frame| {
            draw(
                frame,
                &editor,
                &manifest,
                &Some(sample.clone()),
                &View {
                    preview_status: "Image preview · reduced source · v details".into(),
                    ..View::default()
                },
                &controls(0),
            )
        })
        .unwrap();
    let text: String = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|c| c.symbol())
        .collect();
    assert!(text.contains("Low contrast; reduce image opacity"));
    assert!(text.contains("Internal sample"));
}

#[test]
fn save_failure_footer_keeps_outcome_after_long_profile_path() {
    let (_temp, editor) = editor();
    let view = View {
        status: format!(
            "Draft retained. Profile at /tmp/{}/profiles/sample.toml was not saved: injected error; no apply attempted.",
            "x".repeat(80)
        ),
        ..View::default()
    };
    let text = render(100, 30, &editor, &view);
    assert!(text.contains("was not saved:"));
    assert!(text.contains("no apply attempted."));
    assert!(text.contains("v details"));
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
    assert!(small.contains("Edit Profile sample · Controls (p)"));
    assert!(small.contains("p preview"));
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
    assert!(small_sample.contains("Edit Profile sample · Preview (p)"));
    assert!(small_sample.contains("p controls"));
    assert!(!small_sample.contains("Wallpaper-image opacity"));
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
