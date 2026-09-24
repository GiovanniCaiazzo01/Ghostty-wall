use std::str::FromStr;

use ghostty_wall::{
    domain::IntentId,
    terminal_browser::{
        BrowserAction, BrowserApplication, BrowserFocus, BrowserMode, PlannedProfile,
        TerminalBrowser, TerminalGraphics, render_terminal_image,
    },
};
use serde_json::json;

#[derive(Default)]
struct RecordingApplication {
    planned: Vec<String>,
    applied: Vec<String>,
}

impl BrowserApplication for RecordingApplication {
    fn plan_profile(&mut self, profile: &IntentId) -> Result<PlannedProfile, String> {
        self.planned.push(profile.to_string());
        Ok(PlannedProfile::new(
            json!({
                "profile": { "id": profile.as_str(), "schema_version": 1 },
                "source": { "id": "landscapes" },
                "selection": { "candidate": "coast.png" },
                "environment": {
                    "environment_id": "env-v1-test",
                    "manifest": {
                        "colors": {
                            "background": "000000",
                            "foreground": "ffffff"
                        }
                    }
                }
            }),
            Some(vec![1, 2, 3]),
        ))
    }

    fn apply_profile(&mut self, profile: &IntentId) -> Result<(), String> {
        self.applied.push(profile.to_string());
        Ok(())
    }
}

#[test]
fn navigation_preview_and_apply_use_application_service() {
    let mut browser =
        TerminalBrowser::new(ids(&["landscapes", "portraits"]), ids(&["day", "night"]));
    let mut app = RecordingApplication::default();

    assert_eq!(browser.focus(), BrowserFocus::Sources);
    assert_eq!(browser.selected_source().unwrap().as_str(), "landscapes");
    browser.dispatch(BrowserAction::Down, &mut app).unwrap();
    assert_eq!(browser.selected_source().unwrap().as_str(), "portraits");

    browser.dispatch(BrowserAction::NextPane, &mut app).unwrap();
    browser.dispatch(BrowserAction::Down, &mut app).unwrap();
    assert_eq!(browser.selected_profile().unwrap().as_str(), "night");

    browser.dispatch(BrowserAction::Preview, &mut app).unwrap();
    assert_eq!(browser.mode(), BrowserMode::Preview);
    let preview = browser.preview().unwrap();
    assert_eq!(preview.source_id().unwrap().as_str(), "landscapes");
    assert_eq!(preview.candidate(), Some("coast.png"));
    assert_eq!(preview.colors().unwrap().contrast_ratio(), 21.0);
    assert_eq!(
        browser.render(),
        "Profile: night\nSource: landscapes\nWallpaper: coast.png\nEnvironment: env-v1-test\nColors: #000000 on #ffffff\nContrast: 21.00:1\n[a] apply  [esc] back  [q] cancel\n"
    );
    assert_eq!(app.planned, ["night"]);
    assert!(app.applied.is_empty());

    browser.dispatch(BrowserAction::Apply, &mut app).unwrap();
    assert_eq!(browser.mode(), BrowserMode::Applied);
    assert_eq!(app.applied, ["night"]);
}

#[test]
fn navigation_wraps_and_back_returns_to_profile_list() {
    let mut browser =
        TerminalBrowser::new(ids(&["landscapes", "portraits"]), ids(&["day", "night"]));
    let mut app = RecordingApplication::default();

    browser.dispatch(BrowserAction::Up, &mut app).unwrap();
    assert_eq!(browser.selected_source().unwrap().as_str(), "portraits");
    browser.dispatch(BrowserAction::NextPane, &mut app).unwrap();
    browser.dispatch(BrowserAction::Preview, &mut app).unwrap();
    browser.dispatch(BrowserAction::Back, &mut app).unwrap();

    assert_eq!(browser.mode(), BrowserMode::Browse);
    assert_eq!(browser.focus(), BrowserFocus::Profiles);
    assert!(browser.preview().is_none());
    assert!(app.applied.is_empty());
}

#[test]
fn cancel_never_calls_mutating_application_service() {
    let mut browser = TerminalBrowser::new(ids(&["landscapes"]), ids(&["night"]));
    let mut app = RecordingApplication::default();

    browser.dispatch(BrowserAction::NextPane, &mut app).unwrap();
    browser.dispatch(BrowserAction::Preview, &mut app).unwrap();
    browser.dispatch(BrowserAction::Cancel, &mut app).unwrap();
    browser.dispatch(BrowserAction::Apply, &mut app).unwrap();

    assert_eq!(browser.mode(), BrowserMode::Cancelled);
    assert_eq!(app.planned, ["night"]);
    assert!(app.applied.is_empty());
}

#[test]
fn image_preview_uses_ghostty_kitty_protocol() {
    let mut output = Vec::new();

    let rendered = render_terminal_image(
        &mut output,
        include_bytes!("fixtures/white.jpg"),
        TerminalGraphics::Ghostty,
    )
    .unwrap();

    assert!(rendered);
    assert!(output.starts_with(b"\x1b_Ga=T,f=100,q=2,"));
    assert!(output.ends_with(b"\x1b\\"));
}

#[test]
fn unsupported_terminal_gets_readable_image_fallback() {
    let mut output = Vec::new();

    let rendered = render_terminal_image(
        &mut output,
        include_bytes!("fixtures/white.png"),
        TerminalGraphics::Unsupported,
    )
    .unwrap();

    assert!(!rendered);
    assert_eq!(
        String::from_utf8(output).unwrap(),
        "[image preview unavailable: terminal graphics unsupported]\n"
    );
}

fn ids(values: &[&str]) -> Vec<IntentId> {
    values
        .iter()
        .map(|value| IntentId::from_str(value).unwrap())
        .collect()
}
