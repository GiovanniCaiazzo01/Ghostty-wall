use std::{fmt, str::FromStr};

use super::{
    BackgroundBlurIntensity, CandidatePath, Color, CursorStyle, FontSizeMillipoints,
    OpacityMillionths, ValidationError, WallpaperFit, WallpaperPosition,
};

/// Valid Source or Profile identifier from RFC 0003.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IntentId(String);

impl IntentId {
    /// Returns identifier text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for IntentId {
    type Err = ValidationError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let bytes = value.as_bytes();
        if bytes.is_empty() || bytes.len() > 64 {
            return Err(ValidationError::InvalidIntentId);
        }
        let mut previous_dash = false;
        for (index, byte) in bytes.iter().copied().enumerate() {
            let ok = byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-';
            if !ok || (byte == b'-' && (index == 0 || previous_dash)) {
                return Err(ValidationError::InvalidIntentId);
            }
            previous_dash = byte == b'-';
        }
        if previous_dash {
            return Err(ValidationError::InvalidIntentId);
        }
        Ok(Self(value.to_owned()))
    }
}

impl fmt::Display for IntentId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Complete Source registry from `config.toml`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigIntent {
    /// Registered Sources keyed by identifier.
    pub sources: Vec<(IntentId, SourceIntent)>,
}

/// Source intent tagged union.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceIntent {
    /// GitHub repository Source.
    Github {
        /// `owner/repo` repository name.
        repository: String,
        /// Requested ref, when pinned by user intent.
        reference: Option<String>,
        /// Optional logical root path inside repository.
        path: Option<SourcePath>,
    },
    /// Local directory Source.
    LocalDirectory {
        /// Authored path; expansion happens at resolution boundary.
        path: String,
    },
}

/// Slash-separated relative Source path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourcePath(String);

impl SourcePath {
    /// Returns path text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for SourcePath {
    type Err = ValidationError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.is_empty()
            || value.starts_with('/')
            || value.contains('\\')
            || value
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == "..")
        {
            return Err(ValidationError::InvalidSourcePath);
        }
        Ok(Self(value.to_owned()))
    }
}

/// Complete Profile recipe from `profiles/<profile-id>.toml`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileIntent {
    /// Optional wallpaper intent.
    pub wallpaper: Option<WallpaperIntent>,
    /// Optional colors intent.
    pub colors: Option<ColorsIntent>,
    /// Optional terminal intent.
    pub terminal: Option<TerminalIntent>,
}

/// Wallpaper intent tagged union.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WallpaperIntent {
    /// Manage wallpaper by disabling image.
    None,
    /// Resolve wallpaper from a Source.
    Source {
        /// Referenced Source identifier.
        source: IntentId,
        /// Candidate selection mode.
        selection: WallpaperSelection,
        /// Optional fit.
        fit: Option<WallpaperFit>,
        /// Optional position.
        position: Option<WallpaperPosition>,
        /// Optional opacity.
        opacity: Option<OpacityMillionths>,
        /// Optional repeat flag.
        repeat: Option<bool>,
    },
}

/// Wallpaper Candidate selection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WallpaperSelection {
    /// Deterministic random selection.
    Random,
    /// Direct Candidate path selection.
    Path(CandidatePath),
}

/// Colors intent tagged union.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ColorsIntent {
    /// Generate palette from wallpaper.
    Generated,
    /// Load named Ghostty theme.
    Theme {
        /// Theme name.
        theme: String,
    },
    /// Explicit managed color model.
    Explicit {
        /// Background color.
        background: Color,
        /// Foreground color.
        foreground: Color,
        /// ANSI palette.
        palette: [Color; 16],
        /// Optional cursor color.
        cursor: Option<Color>,
        /// Optional selection background.
        selection_background: Option<Color>,
        /// Optional selection foreground.
        selection_foreground: Option<Color>,
    },
}

/// Terminal intent with at least one managed field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalIntent {
    /// Optional font size.
    pub font_size: Option<FontSizeMillipoints>,
    /// Optional background opacity.
    pub background_opacity: Option<OpacityMillionths>,
    /// Optional blur intensity.
    pub background_blur_intensity: Option<BackgroundBlurIntensity>,
    /// Optional cursor style.
    pub cursor_style: Option<CursorStyle>,
}

impl TerminalIntent {
    /// Constructs a non-empty terminal intent.
    pub fn new(
        font_size: Option<FontSizeMillipoints>,
        background_opacity: Option<OpacityMillionths>,
        background_blur_intensity: Option<BackgroundBlurIntensity>,
        cursor_style: Option<CursorStyle>,
    ) -> Result<Self, ValidationError> {
        if font_size.is_none()
            && background_opacity.is_none()
            && background_blur_intensity.is_none()
            && cursor_style.is_none()
        {
            return Err(ValidationError::EmptyTerminal);
        }
        Ok(Self {
            font_size,
            background_opacity,
            background_blur_intensity,
            cursor_style,
        })
    }
}
