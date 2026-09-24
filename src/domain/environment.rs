use std::{fmt, str::FromStr};

use super::{Sha256Digest, ValidationError, ids::parse_lower_hex};

pub const ENVIRONMENT_MANIFEST_SCHEMA_VERSION: u64 = 1;

/// Immutable managed visual state whose canonical content determines identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnvironmentManifest {
    pub(crate) wallpaper: Option<WallpaperManifest>,
    pub(crate) colors: Option<ColorsManifest>,
    pub(crate) terminal: Option<TerminalManifest>,
}

impl EnvironmentManifest {
    /// Constructs a Manifest from independently managed optional sections.
    pub const fn new(
        wallpaper: Option<WallpaperManifest>,
        colors: Option<ColorsManifest>,
        terminal: Option<TerminalManifest>,
    ) -> Self {
        Self {
            wallpaper,
            colors,
            terminal,
        }
    }

    /// Returns the only schema version supported by this type.
    pub const fn schema_version(&self) -> u64 {
        ENVIRONMENT_MANIFEST_SCHEMA_VERSION
    }

    /// Returns wallpaper ownership and resolved image state, when managed.
    pub const fn wallpaper(&self) -> Option<&WallpaperManifest> {
        self.wallpaper.as_ref()
    }

    /// Returns the complete managed color model, when managed.
    pub const fn colors(&self) -> Option<&ColorsManifest> {
        self.colors.as_ref()
    }

    /// Returns explicitly managed terminal options.
    pub const fn terminal(&self) -> Option<&TerminalManifest> {
        self.terminal.as_ref()
    }
}

/// Managed wallpaper state, distinct from an unmanaged absent section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WallpaperManifest {
    /// Explicitly reset Ghostty's background image.
    None,
    /// Apply one validated durable image.
    Image(ImageWallpaper),
}

/// Resolved image wallpaper properties stored in an Environment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageWallpaper {
    pub(crate) asset_sha256: Sha256Digest,
    pub(crate) media_type: MediaType,
    pub(crate) fit: Option<WallpaperFit>,
    pub(crate) position: Option<WallpaperPosition>,
    pub(crate) opacity: Option<OpacityMillionths>,
    pub(crate) repeat: Option<bool>,
}

impl ImageWallpaper {
    /// Starts a managed image from its durable Asset identity and media type.
    pub const fn new(asset_sha256: Sha256Digest, media_type: MediaType) -> Self {
        Self {
            asset_sha256,
            media_type,
            fit: None,
            position: None,
            opacity: None,
            repeat: None,
        }
    }

    /// Manages Ghostty's image fitting mode.
    pub const fn with_fit(mut self, fit: WallpaperFit) -> Self {
        self.fit = Some(fit);
        self
    }

    /// Manages Ghostty's image position.
    pub const fn with_position(mut self, position: WallpaperPosition) -> Self {
        self.position = Some(position);
        self
    }

    /// Manages wallpaper opacity as fixed-point millionths.
    pub const fn with_opacity(mut self, opacity: OpacityMillionths) -> Self {
        self.opacity = Some(opacity);
        self
    }

    /// Manages whether Ghostty repeats the image.
    pub const fn with_repeat(mut self, repeat: bool) -> Self {
        self.repeat = Some(repeat);
        self
    }

    /// Returns the durable Asset digest.
    pub const fn asset_sha256(&self) -> Sha256Digest {
        self.asset_sha256
    }

    /// Returns the validated image media type.
    pub const fn media_type(&self) -> MediaType {
        self.media_type
    }

    /// Returns the managed fit, or absence when Ghostty Wall does not own it.
    pub const fn fit(&self) -> Option<WallpaperFit> {
        self.fit
    }

    /// Returns the managed position, or absence when unmanaged.
    pub const fn position(&self) -> Option<WallpaperPosition> {
        self.position
    }

    /// Returns managed wallpaper opacity, or absence when unmanaged.
    pub const fn opacity(&self) -> Option<OpacityMillionths> {
        self.opacity
    }

    /// Returns managed repeat behavior, preserving absent versus `false`.
    pub const fn repeat(&self) -> Option<bool> {
        self.repeat
    }
}

/// Image formats accepted by Ghostty Wall's durable Asset model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaType {
    /// Portable Network Graphics.
    Png,
    /// JPEG image data.
    Jpeg,
}

impl MediaType {
    /// Returns the RFC 0001 media type spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
        }
    }
}

impl FromStr for MediaType {
    type Err = ValidationError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "image/png" => Ok(Self::Png),
            "image/jpeg" => Ok(Self::Jpeg),
            _ => Err(invalid_enum("media_type", value)),
        }
    }
}

/// Ghostty background-image scaling behavior.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WallpaperFit {
    /// Preserve aspect ratio while keeping the complete image visible.
    Contain,
    /// Preserve aspect ratio while covering the terminal.
    Cover,
    /// Fill the terminal without preserving aspect ratio.
    Stretch,
    /// Do not scale the image.
    None,
}

impl WallpaperFit {
    /// Returns the Ghostty configuration spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Contain => "contain",
            Self::Cover => "cover",
            Self::Stretch => "stretch",
            Self::None => "none",
        }
    }
}

impl FromStr for WallpaperFit {
    type Err = ValidationError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "contain" => Ok(Self::Contain),
            "cover" => Ok(Self::Cover),
            "stretch" => Ok(Self::Stretch),
            "none" => Ok(Self::None),
            _ => Err(invalid_enum("wallpaper.fit", value)),
        }
    }
}

/// Ghostty background-image anchor position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WallpaperPosition {
    /// Top-left anchor.
    TopLeft,
    /// Top-center anchor.
    TopCenter,
    /// Top-right anchor.
    TopRight,
    /// Center-left anchor.
    CenterLeft,
    /// Center anchor.
    Center,
    /// Center-right anchor.
    CenterRight,
    /// Bottom-left anchor.
    BottomLeft,
    /// Bottom-center anchor.
    BottomCenter,
    /// Bottom-right anchor.
    BottomRight,
}

impl WallpaperPosition {
    /// Returns the Ghostty configuration spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TopLeft => "top-left",
            Self::TopCenter => "top-center",
            Self::TopRight => "top-right",
            Self::CenterLeft => "center-left",
            Self::Center => "center",
            Self::CenterRight => "center-right",
            Self::BottomLeft => "bottom-left",
            Self::BottomCenter => "bottom-center",
            Self::BottomRight => "bottom-right",
        }
    }
}

impl FromStr for WallpaperPosition {
    type Err = ValidationError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "top-left" => Ok(Self::TopLeft),
            "top-center" => Ok(Self::TopCenter),
            "top-right" => Ok(Self::TopRight),
            "center-left" => Ok(Self::CenterLeft),
            "center" => Ok(Self::Center),
            "center-right" => Ok(Self::CenterRight),
            "bottom-left" => Ok(Self::BottomLeft),
            "bottom-center" => Ok(Self::BottomCenter),
            "bottom-right" => Ok(Self::BottomRight),
            _ => Err(invalid_enum("wallpaper.position", value)),
        }
    }
}

/// Complete managed base colors and ANSI 0–15 palette.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColorsManifest {
    pub(crate) background: Color,
    pub(crate) foreground: Color,
    pub(crate) palette: [Color; 16],
    pub(crate) cursor: Option<Color>,
    pub(crate) selection_background: Option<Color>,
    pub(crate) selection_foreground: Option<Color>,
}

impl ColorsManifest {
    /// Constructs the required color model.
    pub const fn new(background: Color, foreground: Color, palette: [Color; 16]) -> Self {
        Self {
            background,
            foreground,
            palette,
            cursor: None,
            selection_background: None,
            selection_foreground: None,
        }
    }

    /// Adds a managed cursor color.
    pub const fn with_cursor(mut self, cursor: Color) -> Self {
        self.cursor = Some(cursor);
        self
    }

    /// Adds a managed selection background.
    pub const fn with_selection_background(mut self, color: Color) -> Self {
        self.selection_background = Some(color);
        self
    }

    /// Adds a managed selection foreground.
    pub const fn with_selection_foreground(mut self, color: Color) -> Self {
        self.selection_foreground = Some(color);
        self
    }

    /// Returns the terminal background color.
    pub const fn background(&self) -> Color {
        self.background
    }

    /// Returns the terminal foreground color.
    pub const fn foreground(&self) -> Color {
        self.foreground
    }

    /// Returns ANSI palette entries in index order 0 through 15.
    pub const fn palette(&self) -> &[Color; 16] {
        &self.palette
    }

    /// Returns the managed cursor color.
    pub const fn cursor(&self) -> Option<Color> {
        self.cursor
    }

    /// Returns the managed selection background.
    pub const fn selection_background(&self) -> Option<Color> {
        self.selection_background
    }

    /// Returns the managed selection foreground.
    pub const fn selection_foreground(&self) -> Option<Color> {
        self.selection_foreground
    }
}

/// An 8-bit sRGB color rendered as six lowercase hexadecimal digits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Color([u8; 3]);

impl Color {
    /// Returns red, green, and blue channel bytes.
    pub const fn as_rgb(self) -> [u8; 3] {
        self.0
    }
}

impl FromStr for Color {
    type Err = ValidationError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        parse_lower_hex(value)
            .map(Self)
            .ok_or(ValidationError::InvalidColor)
    }
}

impl fmt::Display for Color {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// Non-empty set of terminal options explicitly managed by an Environment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalManifest {
    pub(crate) font_size: Option<FontSizeMillipoints>,
    pub(crate) background_opacity: Option<OpacityMillionths>,
    pub(crate) background_blur: Option<BackgroundBlurIntensity>,
    pub(crate) cursor_style: Option<CursorStyle>,
}

impl TerminalManifest {
    /// Constructs a terminal section, rejecting an empty managed object.
    pub fn new(
        font_size: Option<FontSizeMillipoints>,
        background_opacity: Option<OpacityMillionths>,
        background_blur: Option<BackgroundBlurIntensity>,
        cursor_style: Option<CursorStyle>,
    ) -> Result<Self, ValidationError> {
        if font_size.is_none()
            && background_opacity.is_none()
            && background_blur.is_none()
            && cursor_style.is_none()
        {
            return Err(ValidationError::EmptyTerminal);
        }

        Ok(Self {
            font_size,
            background_opacity,
            background_blur,
            cursor_style,
        })
    }

    /// Returns managed font size.
    pub const fn font_size(&self) -> Option<FontSizeMillipoints> {
        self.font_size
    }

    /// Returns managed terminal background opacity.
    pub const fn background_opacity(&self) -> Option<OpacityMillionths> {
        self.background_opacity
    }

    /// Returns managed background blur intensity.
    pub const fn background_blur(&self) -> Option<BackgroundBlurIntensity> {
        self.background_blur
    }

    /// Returns managed cursor style.
    pub const fn cursor_style(&self) -> Option<CursorStyle> {
        self.cursor_style
    }
}

/// Font size represented exactly in one-thousandth of a point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FontSizeMillipoints(u32);

impl FontSizeMillipoints {
    /// Minimum v1 font size: one point.
    pub const MIN: u32 = 1_000;
    /// Maximum v1 font size: one thousand points.
    pub const MAX: u32 = 1_000_000;

    /// Validates a millipoint value against the v1 range.
    pub fn new(value: u64) -> Result<Self, ValidationError> {
        if (u64::from(Self::MIN)..=u64::from(Self::MAX)).contains(&value) {
            Ok(Self(value as u32))
        } else {
            Err(ValidationError::FontSizeOutOfRange { value })
        }
    }

    /// Returns the validated fixed-point value.
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Opacity represented exactly in millionths from zero through one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpacityMillionths(u32);

impl OpacityMillionths {
    /// Fully opaque in millionths.
    pub const MAX: u32 = 1_000_000;

    /// Validates an opacity value without clamping.
    pub fn new(value: u64) -> Result<Self, ValidationError> {
        if value <= u64::from(Self::MAX) {
            Ok(Self(value as u32))
        } else {
            Err(ValidationError::OpacityOutOfRange { value })
        }
    }

    /// Returns the validated fixed-point value.
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Ghostty background blur intensity in the supported byte range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BackgroundBlurIntensity(u8);

impl BackgroundBlurIntensity {
    /// Validates an integer intensity without clamping.
    pub fn new(value: u64) -> Result<Self, ValidationError> {
        u8::try_from(value)
            .map(Self)
            .map_err(|_| ValidationError::BackgroundBlurOutOfRange { value })
    }

    /// Returns the validated intensity.
    pub const fn get(self) -> u8 {
        self.0
    }
}

/// Ghostty's managed default cursor shapes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorStyle {
    /// Filled block cursor.
    Block,
    /// Vertical bar cursor.
    Bar,
    /// Underline cursor.
    Underline,
    /// Hollow block cursor.
    BlockHollow,
}

impl CursorStyle {
    /// Returns the Ghostty configuration spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Block => "block",
            Self::Bar => "bar",
            Self::Underline => "underline",
            Self::BlockHollow => "block_hollow",
        }
    }
}

impl FromStr for CursorStyle {
    type Err = ValidationError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "block" => Ok(Self::Block),
            "bar" => Ok(Self::Bar),
            "underline" => Ok(Self::Underline),
            "block_hollow" => Ok(Self::BlockHollow),
            _ => Err(invalid_enum("terminal.cursor_style", value)),
        }
    }
}

fn invalid_enum(field: &'static str, value: &str) -> ValidationError {
    ValidationError::InvalidEnumValue {
        field,
        value: value.to_owned(),
    }
}
