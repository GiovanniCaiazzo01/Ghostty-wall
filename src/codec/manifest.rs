use serde::{Deserialize, Deserializer, Serialize, de};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::domain::{
    BackgroundBlurIntensity, Color, ColorsManifest, EnvironmentId, EnvironmentManifest,
    FontSizeMillipoints, ImageWallpaper, OpacityMillionths, Sha256Digest, TerminalManifest,
    ValidationError, WallpaperManifest,
};

// RFC 0001 fixes these exact bytes. Domain separation prevents a digest from
// another Ghostty Wall protocol from being interpreted as an Environment ID.
const ENVIRONMENT_ID_DOMAIN: &[u8] = b"ghostty-wall.environment-manifest.v1\0";

/// Failure to decode, validate, or canonically encode an Environment Manifest.
#[derive(Debug, Error)]
pub enum ManifestCodecError {
    /// Input was not strict RFC 0001 JSON.
    #[error("invalid Environment Manifest JSON: {0}")]
    DecodeJson(serde_json::Error),

    /// A valid domain value could not be represented as JCS JSON.
    #[error("failed to encode canonical Environment Manifest JSON: {0}")]
    EncodeJson(serde_json::Error),

    /// JSON syntax was valid but violated a domain invariant.
    #[error(transparent)]
    Validation(#[from] ValidationError),
}

/// Decodes strict RFC 0001 JSON into a validated Manifest.
///
/// Unknown fields, duplicate fields, `null`, unsupported schema versions, and
/// invalid managed values are rejected.
pub fn decode(input: &[u8]) -> Result<EnvironmentManifest, ManifestCodecError> {
    let dto = serde_json::from_slice(input).map_err(ManifestCodecError::DecodeJson)?;
    ManifestDto::try_into_domain(dto).map_err(Into::into)
}

/// Encodes a Manifest as RFC 8785 JCS bytes.
///
/// Property order in these bytes is part of the public Environment identity
/// contract even though ordinary JSON object order is not semantic.
pub fn encode_canonical(manifest: &EnvironmentManifest) -> Result<Vec<u8>, ManifestCodecError> {
    serde_jcs::to_vec(&ManifestDto::from_domain(manifest)).map_err(ManifestCodecError::EncodeJson)
}

/// Derives the stable, domain-separated RFC 0001 identity of a Manifest.
pub fn environment_id(manifest: &EnvironmentManifest) -> Result<EnvironmentId, ManifestCodecError> {
    let canonical = encode_canonical(manifest)?;
    let mut hasher = Sha256::new();
    hasher.update(ENVIRONMENT_ID_DOMAIN);
    hasher.update(canonical);
    let digest: [u8; 32] = hasher.finalize().into();

    Ok(EnvironmentId::from_digest(Sha256Digest::from_bytes(digest)))
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ManifestDto {
    schema_version: u64,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_optional_no_null"
    )]
    wallpaper: Option<WallpaperDto>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_optional_no_null"
    )]
    colors: Option<ColorsDto>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_optional_no_null"
    )]
    terminal: Option<TerminalDto>,
}

impl ManifestDto {
    fn from_domain(manifest: &EnvironmentManifest) -> Self {
        Self {
            schema_version: manifest.schema_version(),
            wallpaper: manifest.wallpaper().map(WallpaperDto::from_domain),
            colors: manifest.colors().map(ColorsDto::from_domain),
            terminal: manifest.terminal().map(TerminalDto::from_domain),
        }
    }

    fn try_into_domain(self) -> Result<EnvironmentManifest, ValidationError> {
        if self.schema_version != 1 {
            return Err(ValidationError::UnsupportedManifestSchema {
                found: self.schema_version,
            });
        }

        Ok(EnvironmentManifest::new(
            self.wallpaper
                .map(WallpaperDto::try_into_domain)
                .transpose()?,
            self.colors.map(ColorsDto::try_into_domain).transpose()?,
            self.terminal
                .map(TerminalDto::try_into_domain)
                .transpose()?,
        ))
    }
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "mode", rename_all = "lowercase", deny_unknown_fields)]
enum WallpaperDto {
    None,
    Image {
        asset_sha256: String,
        media_type: String,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            deserialize_with = "deserialize_optional_no_null"
        )]
        fit: Option<String>,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            deserialize_with = "deserialize_optional_no_null"
        )]
        position: Option<String>,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            deserialize_with = "deserialize_optional_no_null"
        )]
        opacity_millionths: Option<u64>,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            deserialize_with = "deserialize_optional_no_null"
        )]
        repeat: Option<bool>,
    },
}

impl WallpaperDto {
    fn from_domain(wallpaper: &WallpaperManifest) -> Self {
        match wallpaper {
            WallpaperManifest::None => Self::None,
            WallpaperManifest::Image(image) => Self::Image {
                asset_sha256: image.asset_sha256().to_string(),
                media_type: image.media_type().as_str().to_owned(),
                fit: image.fit().map(|value| value.as_str().to_owned()),
                position: image.position().map(|value| value.as_str().to_owned()),
                opacity_millionths: image.opacity().map(|value| u64::from(value.get())),
                repeat: image.repeat(),
            },
        }
    }

    fn try_into_domain(self) -> Result<WallpaperManifest, ValidationError> {
        match self {
            Self::None => Ok(WallpaperManifest::None),
            Self::Image {
                asset_sha256,
                media_type,
                fit,
                position,
                opacity_millionths,
                repeat,
            } => {
                let mut image = ImageWallpaper::new(asset_sha256.parse()?, media_type.parse()?);
                if let Some(fit) = fit {
                    image = image.with_fit(fit.parse()?);
                }
                if let Some(position) = position {
                    image = image.with_position(position.parse()?);
                }
                if let Some(opacity) = opacity_millionths {
                    image = image.with_opacity(OpacityMillionths::new(opacity)?);
                }
                if let Some(repeat) = repeat {
                    image = image.with_repeat(repeat);
                }
                Ok(WallpaperManifest::Image(image))
            }
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ColorsDto {
    background: String,
    foreground: String,
    palette: Vec<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_optional_no_null"
    )]
    cursor: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_optional_no_null"
    )]
    selection_background: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_optional_no_null"
    )]
    selection_foreground: Option<String>,
}

impl ColorsDto {
    fn from_domain(colors: &ColorsManifest) -> Self {
        Self {
            background: colors.background().to_string(),
            foreground: colors.foreground().to_string(),
            palette: colors.palette().iter().map(ToString::to_string).collect(),
            cursor: colors.cursor().map(|value| value.to_string()),
            selection_background: colors.selection_background().map(|value| value.to_string()),
            selection_foreground: colors.selection_foreground().map(|value| value.to_string()),
        }
    }

    fn try_into_domain(self) -> Result<ColorsManifest, ValidationError> {
        let palette_len = self.palette.len();
        let palette: [Color; 16] = self
            .palette
            .into_iter()
            .map(|value| value.parse())
            .collect::<Result<Vec<_>, _>>()?
            .try_into()
            .map_err(|_| ValidationError::InvalidPaletteLength { found: palette_len })?;

        let mut colors =
            ColorsManifest::new(self.background.parse()?, self.foreground.parse()?, palette);
        if let Some(cursor) = self.cursor {
            colors = colors.with_cursor(cursor.parse()?);
        }
        if let Some(selection_background) = self.selection_background {
            colors = colors.with_selection_background(selection_background.parse()?);
        }
        if let Some(selection_foreground) = self.selection_foreground {
            colors = colors.with_selection_foreground(selection_foreground.parse()?);
        }

        Ok(colors)
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct TerminalDto {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_optional_no_null"
    )]
    font_size_millipoints: Option<u64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_optional_no_null"
    )]
    background_opacity_millionths: Option<u64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_optional_no_null"
    )]
    background_blur_intensity: Option<u64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_optional_no_null"
    )]
    cursor_style: Option<String>,
}

impl TerminalDto {
    fn from_domain(terminal: &TerminalManifest) -> Self {
        Self {
            font_size_millipoints: terminal.font_size().map(|value| u64::from(value.get())),
            background_opacity_millionths: terminal
                .background_opacity()
                .map(|value| u64::from(value.get())),
            background_blur_intensity: terminal
                .background_blur()
                .map(|value| u64::from(value.get())),
            cursor_style: terminal
                .cursor_style()
                .map(|value| value.as_str().to_owned()),
        }
    }

    fn try_into_domain(self) -> Result<TerminalManifest, ValidationError> {
        TerminalManifest::new(
            self.font_size_millipoints
                .map(FontSizeMillipoints::new)
                .transpose()?,
            self.background_opacity_millionths
                .map(OpacityMillionths::new)
                .transpose()?,
            self.background_blur_intensity
                .map(BackgroundBlurIntensity::new)
                .transpose()?,
            self.cursor_style.map(|value| value.parse()).transpose()?,
        )
    }
}

fn deserialize_optional_no_null<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)?
        .map(Some)
        .ok_or_else(|| de::Error::custom("null is not permitted"))
}
