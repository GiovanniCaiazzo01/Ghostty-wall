//! Terminal browser state machine over Profile planning and apply services.

use std::{
    fmt::Write as FmtWrite,
    io::{self, Cursor, Write},
};

use image::ImageFormat;
use serde_json::Value;
use thiserror::Error;

use crate::domain::IntentId;

/// Read-only Plan result plus selected image bytes supplied by application services.
#[derive(Clone, Debug, PartialEq)]
pub struct PlannedProfile {
    plan: Value,
    image: Option<Vec<u8>>,
}

impl PlannedProfile {
    /// Constructs browser input from same resolved Plan used by normal commands.
    pub fn new(plan: Value, image: Option<Vec<u8>>) -> Self {
        Self { plan, image }
    }
}

/// Terminal graphics capability used for wallpaper preview.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalGraphics {
    /// Ghostty's supported Kitty graphics protocol.
    Ghostty,
    /// Terminal does not advertise Ghostty-compatible image support.
    Unsupported,
}

impl TerminalGraphics {
    /// Detects Ghostty from standard terminal identification variables.
    pub fn detect(term_program: Option<&str>, term: Option<&str>) -> Self {
        if term_program.is_some_and(|value| value.eq_ignore_ascii_case("ghostty"))
            || term.is_some_and(|value| value.to_ascii_lowercase().contains("ghostty"))
        {
            Self::Ghostty
        } else {
            Self::Unsupported
        }
    }

    /// Detects graphics support from current process environment.
    pub fn from_environment() -> Self {
        let term_program = std::env::var("TERM_PROGRAM").ok();
        let term = std::env::var("TERM").ok();
        Self::detect(term_program.as_deref(), term.as_deref())
    }
}

/// Wallpaper preview rendering failure.
#[derive(Debug, Error)]
pub enum ImagePreviewError {
    /// Selected image could not be decoded or converted to PNG.
    #[error("cannot prepare terminal image: {0}")]
    Image(#[from] image::ImageError),
    /// Selected image violated supported format or dimension limits.
    #[error("image preview supports bounded PNG and JPEG images only")]
    UnsupportedImage,
    /// Terminal output failed.
    #[error("cannot write terminal image: {0}")]
    Io(#[from] io::Error),
}

/// Writes image with Ghostty's Kitty graphics protocol or readable fallback.
///
/// Returns `true` when graphics were emitted and `false` for text fallback.
pub fn render_terminal_image(
    output: &mut impl Write,
    image_bytes: &[u8],
    graphics: TerminalGraphics,
) -> Result<bool, ImagePreviewError> {
    if graphics == TerminalGraphics::Unsupported {
        output.write_all(b"[image preview unavailable: terminal graphics unsupported]\n")?;
        return Ok(false);
    }

    let format =
        image::guess_format(image_bytes).map_err(|_| ImagePreviewError::UnsupportedImage)?;
    if !matches!(format, ImageFormat::Png | ImageFormat::Jpeg) {
        return Err(ImagePreviewError::UnsupportedImage);
    }
    let mut reader = image::ImageReader::with_format(Cursor::new(image_bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16_384);
    limits.max_image_height = Some(16_384);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    let image = reader.decode()?;
    if u64::from(image.width()) * u64::from(image.height()) > 16_777_216 {
        return Err(ImagePreviewError::UnsupportedImage);
    }
    let mut png = Cursor::new(Vec::new());
    image.write_to(&mut png, ImageFormat::Png)?;
    let encoded = base64(png.get_ref());
    let chunks = encoded.as_bytes().chunks(4096);
    let last = chunks.len().saturating_sub(1);
    for (index, chunk) in chunks.enumerate() {
        if index == 0 {
            write!(
                output,
                "\x1b_Ga=T,f=100,q=2,m={};",
                usize::from(index != last)
            )?;
        } else {
            write!(output, "\x1b_Gm={};", usize::from(index != last))?;
        }
        output.write_all(chunk)?;
        output.write_all(b"\x1b\\")?;
    }
    Ok(true)
}

/// Existing application-service boundary used by terminal browser.
pub trait BrowserApplication {
    /// Resolves Profile without mutating durable or derived state.
    fn plan_profile(&mut self, profile: &IntentId) -> Result<PlannedProfile, String>;

    /// Resolves and applies Profile through normal durable apply path.
    fn apply_profile(&mut self, profile: &IntentId) -> Result<(), String>;
}

/// Browser pane receiving navigation actions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowserFocus {
    /// Configured Wallpaper Sources.
    Sources,
    /// Named Profiles available for preview and apply.
    Profiles,
}

/// High-level browser state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowserMode {
    /// Navigating Source and Profile lists.
    Browse,
    /// Inspecting one resolved Profile.
    Preview,
    /// Profile apply completed.
    Applied,
    /// User exited without requesting further work.
    Cancelled,
}

/// Input actions understood by browser state machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowserAction {
    /// Select preceding item.
    Up,
    /// Select following item.
    Down,
    /// Switch between Source and Profile panes.
    NextPane,
    /// Resolve selected Profile for preview.
    Preview,
    /// Apply currently previewed Profile.
    Apply,
    /// Return from preview to list browsing.
    Back,
    /// Exit browser without applying.
    Cancel,
}

/// Browser orchestration failure.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum BrowserError {
    /// Planning or apply application service failed.
    #[error("application service failed: {0}")]
    Application(String),
    /// Application service returned malformed or mismatched Plan JSON.
    #[error("application service returned invalid Plan: {0}")]
    InvalidPlan(&'static str),
}

/// Resolved colors shown by Profile preview.
#[derive(Clone, Debug, PartialEq)]
pub struct PreviewColors {
    background: String,
    foreground: String,
    contrast_ratio: f64,
}

impl PreviewColors {
    /// Resolved background color without leading `#`.
    pub fn background(&self) -> &str {
        &self.background
    }

    /// Resolved foreground color without leading `#`.
    pub fn foreground(&self) -> &str {
        &self.foreground
    }

    /// WCAG relative-luminance contrast ratio between foreground and background.
    pub const fn contrast_ratio(&self) -> f64 {
        self.contrast_ratio
    }
}

/// Read-only presentation extracted from resolved Plan output.
#[derive(Clone, Debug, PartialEq)]
pub struct ProfilePreview {
    profile_id: IntentId,
    source_id: Option<IntentId>,
    candidate: Option<String>,
    environment_id: String,
    colors: Option<PreviewColors>,
    image: Option<Vec<u8>>,
}

impl ProfilePreview {
    fn from_planned(planned: PlannedProfile) -> Result<Self, BrowserError> {
        let profile_id = string_at(&planned.plan, &["profile", "id"])?
            .parse()
            .map_err(|_| BrowserError::InvalidPlan("invalid Profile id"))?;
        let environment_id = string_at(&planned.plan, &["environment", "environment_id"])?;
        let source_id = optional_string_at(&planned.plan, &["source", "id"])
            .map(|value| value.parse())
            .transpose()
            .map_err(|_| BrowserError::InvalidPlan("invalid Source id"))?;
        let candidate = optional_string_at(&planned.plan, &["selection", "candidate"]);
        let colors = planned
            .plan
            .pointer("/environment/manifest/colors")
            .map(|colors| {
                let background = string_at(colors, &["background"])?;
                let foreground = string_at(colors, &["foreground"])?;
                let background_rgb = parse_color(&background)?;
                let foreground_rgb = parse_color(&foreground)?;
                Ok(PreviewColors {
                    background,
                    foreground,
                    contrast_ratio: contrast_ratio(background_rgb, foreground_rgb),
                })
            })
            .transpose()?;

        Ok(Self {
            profile_id,
            source_id,
            candidate,
            environment_id,
            colors,
            image: planned.image,
        })
    }

    /// Profile resolved by application Plan service.
    pub fn profile_id(&self) -> &IntentId {
        &self.profile_id
    }

    /// Resolved Source, absent for Profiles without source wallpaper.
    pub fn source_id(&self) -> Option<&IntentId> {
        self.source_id.as_ref()
    }

    /// Selected Candidate path, absent for Profiles without source wallpaper.
    pub fn candidate(&self) -> Option<&str> {
        self.candidate.as_deref()
    }

    /// Resolved Environment identity.
    pub fn environment_id(&self) -> &str {
        &self.environment_id
    }

    /// Resolved managed colors, when Profile manages colors.
    pub fn colors(&self) -> Option<&PreviewColors> {
        self.colors.as_ref()
    }

    /// Selected image bytes supplied by application planning service.
    pub fn image(&self) -> Option<&[u8]> {
        self.image.as_deref()
    }
}

/// Deterministic terminal browser state. It owns no resolution or persistence logic.
#[derive(Clone, Debug)]
pub struct TerminalBrowser {
    sources: Vec<IntentId>,
    profiles: Vec<IntentId>,
    source_index: usize,
    profile_index: usize,
    focus: BrowserFocus,
    mode: BrowserMode,
    preview: Option<ProfilePreview>,
}

impl TerminalBrowser {
    /// Constructs browser with canonically sorted, deduplicated identifiers.
    pub fn new(mut sources: Vec<IntentId>, mut profiles: Vec<IntentId>) -> Self {
        sources.sort();
        sources.dedup();
        profiles.sort();
        profiles.dedup();
        Self {
            sources,
            profiles,
            source_index: 0,
            profile_index: 0,
            focus: BrowserFocus::Sources,
            mode: BrowserMode::Browse,
            preview: None,
        }
    }

    /// Current focused pane.
    pub const fn focus(&self) -> BrowserFocus {
        self.focus
    }

    /// Current high-level state.
    pub const fn mode(&self) -> BrowserMode {
        self.mode
    }

    /// Selected Source, if any exist.
    pub fn selected_source(&self) -> Option<&IntentId> {
        self.sources.get(self.source_index)
    }

    /// Selected Profile, if any exist.
    pub fn selected_profile(&self) -> Option<&IntentId> {
        self.profiles.get(self.profile_index)
    }

    /// Current resolved preview.
    pub const fn preview(&self) -> Option<&ProfilePreview> {
        self.preview.as_ref()
    }

    /// Renders compact text UI for current state.
    pub fn render(&self) -> String {
        let mut output = String::new();
        match self.mode {
            BrowserMode::Browse => {
                output.push_str("Ghostty Wall\nSources:\n");
                render_items(
                    &mut output,
                    &self.sources,
                    self.source_index,
                    self.focus == BrowserFocus::Sources,
                );
                output.push_str("Profiles:\n");
                render_items(
                    &mut output,
                    &self.profiles,
                    self.profile_index,
                    self.focus == BrowserFocus::Profiles,
                );
                output.push_str("[tab] pane  [j/k] navigate  [enter] preview  [q] cancel\n");
            }
            BrowserMode::Preview => {
                let preview = self
                    .preview
                    .as_ref()
                    .expect("Preview mode always owns a preview");
                writeln!(output, "Profile: {}", preview.profile_id()).unwrap();
                writeln!(
                    output,
                    "Source: {}",
                    preview.source_id().map_or("unmanaged", IntentId::as_str)
                )
                .unwrap();
                writeln!(
                    output,
                    "Wallpaper: {}",
                    preview.candidate().unwrap_or("unmanaged")
                )
                .unwrap();
                writeln!(output, "Environment: {}", preview.environment_id()).unwrap();
                if let Some(colors) = preview.colors() {
                    writeln!(
                        output,
                        "Colors: #{} on #{}",
                        colors.background(),
                        colors.foreground()
                    )
                    .unwrap();
                    writeln!(output, "Contrast: {:.2}:1", colors.contrast_ratio()).unwrap();
                } else {
                    output.push_str("Colors: unmanaged\n");
                }
                output.push_str("[a] apply  [esc] back  [q] cancel\n");
            }
            BrowserMode::Applied => output.push_str("Profile applied.\n"),
            BrowserMode::Cancelled => output.push_str("Cancelled.\n"),
        }
        output
    }

    /// Writes current wallpaper preview using selected terminal graphics capability.
    ///
    /// Returns `false` when preview has no image or terminal graphics are unsupported.
    pub fn render_preview_image(
        &self,
        output: &mut impl Write,
        graphics: TerminalGraphics,
    ) -> Result<bool, ImagePreviewError> {
        match self.preview().and_then(ProfilePreview::image) {
            Some(image) => render_terminal_image(output, image, graphics),
            None => {
                output.write_all(b"[profile has no wallpaper image]\n")?;
                Ok(false)
            }
        }
    }

    /// Applies one navigation or command action.
    pub fn dispatch<A: BrowserApplication>(
        &mut self,
        action: BrowserAction,
        application: &mut A,
    ) -> Result<(), BrowserError> {
        if matches!(self.mode, BrowserMode::Applied | BrowserMode::Cancelled) {
            return Ok(());
        }
        match action {
            BrowserAction::Cancel => self.mode = BrowserMode::Cancelled,
            BrowserAction::Back if self.mode == BrowserMode::Preview => {
                self.preview = None;
                self.mode = BrowserMode::Browse;
            }
            BrowserAction::Apply if self.mode == BrowserMode::Preview => {
                let profile = self
                    .preview
                    .as_ref()
                    .expect("Preview mode always owns a preview")
                    .profile_id
                    .clone();
                application
                    .apply_profile(&profile)
                    .map_err(BrowserError::Application)?;
                self.mode = BrowserMode::Applied;
            }
            BrowserAction::Preview
                if self.mode == BrowserMode::Browse
                    && self.focus == BrowserFocus::Profiles
                    && self.selected_profile().is_some() =>
            {
                let profile = self
                    .selected_profile()
                    .expect("guarded by selected Profile")
                    .clone();
                let preview = ProfilePreview::from_planned(
                    application
                        .plan_profile(&profile)
                        .map_err(BrowserError::Application)?,
                )?;
                if preview.profile_id != profile {
                    return Err(BrowserError::InvalidPlan("Profile id mismatch"));
                }
                self.preview = Some(preview);
                self.mode = BrowserMode::Preview;
            }
            BrowserAction::NextPane if self.mode == BrowserMode::Browse => {
                self.focus = match self.focus {
                    BrowserFocus::Sources => BrowserFocus::Profiles,
                    BrowserFocus::Profiles => BrowserFocus::Sources,
                };
            }
            BrowserAction::Up if self.mode == BrowserMode::Browse => self.move_selection(false),
            BrowserAction::Down if self.mode == BrowserMode::Browse => self.move_selection(true),
            _ => {}
        }
        Ok(())
    }

    fn move_selection(&mut self, down: bool) {
        let (index, len) = match self.focus {
            BrowserFocus::Sources => (&mut self.source_index, self.sources.len()),
            BrowserFocus::Profiles => (&mut self.profile_index, self.profiles.len()),
        };
        if len == 0 {
            return;
        }
        *index = if down {
            (*index + 1) % len
        } else {
            (*index + len - 1) % len
        };
    }
}

fn render_items(output: &mut String, items: &[IntentId], selected: usize, focused: bool) {
    if items.is_empty() {
        output.push_str("  (none)\n");
        return;
    }
    for (index, item) in items.iter().enumerate() {
        let marker = if focused && index == selected {
            '>'
        } else {
            ' '
        };
        writeln!(output, "{marker} {item}").unwrap();
    }
}

fn base64(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(input.len().div_ceil(3) * 4);
    for bytes in input.chunks(3) {
        let value = (u32::from(bytes[0]) << 16)
            | (u32::from(*bytes.get(1).unwrap_or(&0)) << 8)
            | u32::from(*bytes.get(2).unwrap_or(&0));
        output.push(char::from(TABLE[((value >> 18) & 63) as usize]));
        output.push(char::from(TABLE[((value >> 12) & 63) as usize]));
        output.push(if bytes.len() > 1 {
            char::from(TABLE[((value >> 6) & 63) as usize])
        } else {
            '='
        });
        output.push(if bytes.len() > 2 {
            char::from(TABLE[(value & 63) as usize])
        } else {
            '='
        });
    }
    output
}

fn string_at(value: &Value, path: &[&str]) -> Result<String, BrowserError> {
    path.iter()
        .try_fold(value, |current, key| current.get(key))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or(BrowserError::InvalidPlan("missing string field"))
}

fn optional_string_at(value: &Value, path: &[&str]) -> Option<String> {
    path.iter()
        .try_fold(value, |current, key| current.get(key))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn parse_color(value: &str) -> Result<[u8; 3], BrowserError> {
    if value.len() != 6 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(BrowserError::InvalidPlan("invalid resolved color"));
    }
    Ok([
        u8::from_str_radix(&value[0..2], 16)
            .map_err(|_| BrowserError::InvalidPlan("invalid resolved color"))?,
        u8::from_str_radix(&value[2..4], 16)
            .map_err(|_| BrowserError::InvalidPlan("invalid resolved color"))?,
        u8::from_str_radix(&value[4..6], 16)
            .map_err(|_| BrowserError::InvalidPlan("invalid resolved color"))?,
    ])
}

fn contrast_ratio(left: [u8; 3], right: [u8; 3]) -> f64 {
    let left = luminance(left);
    let right = luminance(right);
    (left.max(right) + 0.05) / (left.min(right) + 0.05)
}

fn luminance(color: [u8; 3]) -> f64 {
    let channel = |value: u8| {
        let value = f64::from(value) / 255.0;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(color[0]) + 0.7152 * channel(color[1]) + 0.0722 * channel(color[2])
}
