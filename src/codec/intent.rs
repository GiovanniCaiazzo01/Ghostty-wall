//! RFC 0003 strict TOML intent loading.

use std::{collections::BTreeSet, str::FromStr};

use thiserror::Error;
use toml::Value;

use crate::domain::{
    BackgroundBlurIntensity, CandidatePath, Color, ColorsIntent, ConfigIntent, FontSizeMillipoints,
    IntentId, OpacityMillionths, ProfileIntent, SourceIntent, SourcePath, TerminalIntent,
    ValidationError, WallpaperIntent, WallpaperSelection,
};

/// Failure while loading v1 intent TOML.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum IntentTomlError {
    /// TOML syntax or duplicate key error.
    #[error("invalid TOML: {0}")]
    Toml(String),
    /// Schema version missing or unsupported.
    #[error("schema_version must equal 1")]
    SchemaVersion,
    /// Required field missing.
    #[error("missing required field {0}")]
    Missing(&'static str),
    /// Unknown field present.
    #[error("unknown field {0}")]
    Unknown(String),
    /// Field had wrong TOML type.
    #[error("invalid type for field {0}")]
    Type(&'static str),
    /// Closed tagged union was invalid.
    #[error("invalid {field} mode {value:?}")]
    Mode {
        /// Field whose mode tag was invalid.
        field: &'static str,
        /// Rejected tag value.
        value: String,
    },
    /// Cross-field invariant failed.
    #[error("{0}")]
    Invariant(&'static str),
    /// Domain validation failed.
    #[error(transparent)]
    Validation(#[from] ValidationError),
}

/// Parses `config.toml` Source registry intent.
pub fn parse_config_toml(input: &str) -> Result<ConfigIntent, IntentTomlError> {
    let value = parse_value(input)?;
    let root = value.as_table().ok_or(IntentTomlError::Type("document"))?;
    allow(root, &["schema_version", "sources"])?;
    require_schema(root)?;
    let sources = table(root, "sources")?;
    let mut parsed = Vec::new();
    for (id, value) in sources {
        let id = IntentId::from_str(id)?;
        let source = value
            .as_table()
            .ok_or(IntentTomlError::Type("sources.<id>"))?;
        let kind = string(source, "kind")?;
        parsed.push((
            id,
            match kind {
                "github" => {
                    allow(source, &["kind", "repository", "ref", "path"])?;
                    let repository = string(source, "repository")?;
                    validate_repository(repository)?;
                    let reference = opt_string(source, "ref")?.map(str::to_owned);
                    let path = opt_string(source, "path")?
                        .map(SourcePath::from_str)
                        .transpose()?;
                    SourceIntent::Github {
                        repository: repository.to_owned(),
                        reference,
                        path,
                    }
                }
                "local-directory" => {
                    allow(source, &["kind", "path"])?;
                    SourceIntent::LocalDirectory {
                        path: string(source, "path")?.to_owned(),
                    }
                }
                other => {
                    return Err(IntentTomlError::Mode {
                        field: "source.kind",
                        value: other.to_owned(),
                    });
                }
            },
        ));
    }
    Ok(ConfigIntent { sources: parsed })
}

/// Parses and validates the identifier and Source references of one named Profile.
pub fn parse_named_profile_toml(
    profile_id: &str,
    config: &ConfigIntent,
    input: &str,
) -> Result<(IntentId, ProfileIntent), IntentTomlError> {
    let id = IntentId::from_str(profile_id)?;
    let profile = parse_profile_toml(input)?;
    if let Some(WallpaperIntent::Source { source, .. }) = &profile.wallpaper
        && !config.sources.iter().any(|(id, _)| id == source)
    {
        return Err(IntentTomlError::Unknown(format!("sources.{source}")));
    }
    Ok((id, profile))
}

/// Parses one `profiles/<profile-id>.toml` recipe.
pub fn parse_profile_toml(input: &str) -> Result<ProfileIntent, IntentTomlError> {
    let value = parse_value(input)?;
    let lexical: toml_edit::DocumentMut = input
        .parse()
        .map_err(|error: toml_edit::TomlError| IntentTomlError::Toml(error.to_string()))?;
    let root = value.as_table().ok_or(IntentTomlError::Type("document"))?;
    allow(root, &["schema_version", "wallpaper", "colors", "terminal"])?;
    require_schema(root)?;
    let wallpaper = root
        .get("wallpaper")
        .map(|value| parse_wallpaper(value, &lexical))
        .transpose()?;
    let colors = root.get("colors").map(parse_colors).transpose()?;
    if matches!(colors, Some(ColorsIntent::Generated))
        && !matches!(wallpaper, Some(WallpaperIntent::Source { .. }))
    {
        return Err(IntentTomlError::Invariant(
            "generated colors require source wallpaper",
        ));
    }
    let terminal = root
        .get("terminal")
        .map(|value| parse_terminal(value, &lexical))
        .transpose()?;
    Ok(ProfileIntent {
        wallpaper,
        colors,
        terminal,
    })
}

fn parse_wallpaper(
    value: &Value,
    lexical: &toml_edit::DocumentMut,
) -> Result<WallpaperIntent, IntentTomlError> {
    let table = value.as_table().ok_or(IntentTomlError::Type("wallpaper"))?;
    match string(table, "mode")? {
        "none" => {
            allow(table, &["mode"])?;
            Ok(WallpaperIntent::None)
        }
        "source" => {
            allow(
                table,
                &[
                    "mode",
                    "source",
                    "selection",
                    "path",
                    "fit",
                    "position",
                    "opacity",
                    "repeat",
                ],
            )?;
            let source = IntentId::from_str(string(table, "source")?)?;
            let selection = match string(table, "selection")? {
                "random" => {
                    if table.contains_key("path") {
                        return Err(IntentTomlError::Invariant(
                            "wallpaper.path is forbidden for random selection",
                        ));
                    }
                    WallpaperSelection::Random
                }
                "path" => {
                    WallpaperSelection::Path(CandidatePath::from_str(string(table, "path")?)?)
                }
                other => {
                    return Err(IntentTomlError::Mode {
                        field: "wallpaper.selection",
                        value: other.to_owned(),
                    });
                }
            };
            Ok(WallpaperIntent::Source {
                source,
                selection,
                fit: opt_parse(table, "fit")?,
                position: opt_parse(table, "position")?,
                opacity: decimal_field(table, lexical, "wallpaper", "opacity", 6)?
                    .map(OpacityMillionths::new)
                    .transpose()?,
                repeat: opt_bool(table, "repeat")?,
            })
        }
        other => Err(IntentTomlError::Mode {
            field: "wallpaper.mode",
            value: other.to_owned(),
        }),
    }
}

fn parse_colors(value: &Value) -> Result<ColorsIntent, IntentTomlError> {
    let table = value.as_table().ok_or(IntentTomlError::Type("colors"))?;
    match string(table, "mode")? {
        "generated" => {
            allow(table, &["mode"])?;
            Ok(ColorsIntent::Generated)
        }
        "theme" => {
            allow(table, &["mode", "theme"])?;
            let theme = string(table, "theme")?;
            if theme.is_empty() {
                return Err(IntentTomlError::Invariant("theme must be non-empty"));
            }
            Ok(ColorsIntent::Theme {
                theme: theme.to_owned(),
            })
        }
        "explicit" => {
            allow(
                table,
                &[
                    "mode",
                    "background",
                    "foreground",
                    "palette",
                    "cursor",
                    "selection_background",
                    "selection_foreground",
                ],
            )?;
            let palette_values = table
                .get("palette")
                .and_then(Value::as_array)
                .ok_or(IntentTomlError::Missing("colors.palette"))?;
            let palette_vec: Vec<Color> = palette_values
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .ok_or(IntentTomlError::Type("colors.palette"))?
                        .parse()
                        .map_err(IntentTomlError::from)
                })
                .collect::<Result<_, _>>()?;
            let palette: [Color; 16] = palette_vec.try_into().map_err(|values: Vec<Color>| {
                ValidationError::InvalidPaletteLength {
                    found: values.len(),
                }
            })?;
            Ok(ColorsIntent::Explicit {
                background: parse_required(table, "background")?,
                foreground: parse_required(table, "foreground")?,
                palette,
                cursor: opt_parse(table, "cursor")?,
                selection_background: opt_parse(table, "selection_background")?,
                selection_foreground: opt_parse(table, "selection_foreground")?,
            })
        }
        other => Err(IntentTomlError::Mode {
            field: "colors.mode",
            value: other.to_owned(),
        }),
    }
}

fn parse_terminal(
    value: &Value,
    lexical: &toml_edit::DocumentMut,
) -> Result<TerminalIntent, IntentTomlError> {
    let table = value.as_table().ok_or(IntentTomlError::Type("terminal"))?;
    allow(
        table,
        &[
            "font_size",
            "background_opacity",
            "background_blur_intensity",
            "cursor_style",
        ],
    )?;
    TerminalIntent::new(
        terminal_decimal(table, lexical, "font_size", 3, 1_000, 1_000_000)?
            .map(FontSizeMillipoints::new)
            .transpose()?,
        terminal_decimal(table, lexical, "background_opacity", 6, 0, 1_000_000)?
            .map(OpacityMillionths::new)
            .transpose()?,
        opt_integer(table, "background_blur_intensity")?
            .map(BackgroundBlurIntensity::new)
            .transpose()?,
        opt_parse(table, "cursor_style")?,
    )
    .map_err(IntentTomlError::from)
}

fn parse_value(input: &str) -> Result<Value, IntentTomlError> {
    input
        .parse::<Value>()
        .map_err(|error| IntentTomlError::Toml(error.to_string()))
}

fn require_schema(table: &toml::map::Map<String, Value>) -> Result<(), IntentTomlError> {
    match table.get("schema_version").and_then(Value::as_integer) {
        Some(1) => Ok(()),
        _ => Err(IntentTomlError::SchemaVersion),
    }
}

fn table<'a>(
    table: &'a toml::map::Map<String, Value>,
    key: &'static str,
) -> Result<&'a toml::map::Map<String, Value>, IntentTomlError> {
    table
        .get(key)
        .ok_or(IntentTomlError::Missing(key))?
        .as_table()
        .ok_or(IntentTomlError::Type(key))
}

fn allow(table: &toml::map::Map<String, Value>, allowed: &[&str]) -> Result<(), IntentTomlError> {
    let allowed = allowed.iter().copied().collect::<BTreeSet<_>>();
    for key in table.keys() {
        if !allowed.contains(key.as_str()) {
            return Err(IntentTomlError::Unknown(key.clone()));
        }
    }
    Ok(())
}

fn string<'a>(
    table: &'a toml::map::Map<String, Value>,
    key: &'static str,
) -> Result<&'a str, IntentTomlError> {
    table
        .get(key)
        .ok_or(IntentTomlError::Missing(key))?
        .as_str()
        .ok_or(IntentTomlError::Type(key))
}

fn opt_string<'a>(
    table: &'a toml::map::Map<String, Value>,
    key: &'static str,
) -> Result<Option<&'a str>, IntentTomlError> {
    table
        .get(key)
        .map(|value| value.as_str().ok_or(IntentTomlError::Type(key)))
        .transpose()
}

fn parse_required<T: FromStr<Err = ValidationError>>(
    table: &toml::map::Map<String, Value>,
    key: &'static str,
) -> Result<T, IntentTomlError> {
    string(table, key)?.parse().map_err(IntentTomlError::from)
}

fn opt_parse<T: FromStr<Err = ValidationError>>(
    table: &toml::map::Map<String, Value>,
    key: &'static str,
) -> Result<Option<T>, IntentTomlError> {
    opt_string(table, key)?
        .map(str::parse)
        .transpose()
        .map_err(IntentTomlError::from)
}

fn opt_bool(
    table: &toml::map::Map<String, Value>,
    key: &'static str,
) -> Result<Option<bool>, IntentTomlError> {
    table
        .get(key)
        .map(|value| value.as_bool().ok_or(IntentTomlError::Type(key)))
        .transpose()
}

fn opt_integer(
    table: &toml::map::Map<String, Value>,
    key: &'static str,
) -> Result<Option<u64>, IntentTomlError> {
    table
        .get(key)
        .map(|value| {
            value
                .as_integer()
                .and_then(|value| u64::try_from(value).ok())
                .ok_or(IntentTomlError::Type(key))
        })
        .transpose()
}

fn validate_repository(value: &str) -> Result<(), IntentTomlError> {
    let mut parts = value.split('/');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(owner), Some(repo), None) if !owner.is_empty() && !repo.is_empty() => Ok(()),
        _ => Err(IntentTomlError::Invariant(
            "repository must contain one owner/repo separator",
        )),
    }
}

fn terminal_decimal(
    table: &toml::map::Map<String, Value>,
    lexical: &toml_edit::DocumentMut,
    key: &'static str,
    max_fraction: usize,
    min: u64,
    max: u64,
) -> Result<Option<u64>, IntentTomlError> {
    let Some(value) = decimal_field(table, lexical, "terminal", key, max_fraction)? else {
        return Ok(None);
    };
    if !(min..=max).contains(&value) {
        return Err(match key {
            "font_size" => ValidationError::FontSizeOutOfRange { value },
            _ => ValidationError::OpacityOutOfRange { value },
        }
        .into());
    }
    Ok(Some(value))
}

fn decimal_field(
    table: &toml::map::Map<String, Value>,
    lexical: &toml_edit::DocumentMut,
    section: &str,
    key: &'static str,
    precision: usize,
) -> Result<Option<u64>, IntentTomlError> {
    if !table.contains_key(key) {
        return Ok(None);
    }
    let item = lexical[section]
        .get(key)
        .ok_or(IntentTomlError::Type(key))?;
    let raw = match item.as_value().ok_or(IntentTomlError::Type(key))? {
        toml_edit::Value::Float(value) => value.as_repr(),
        toml_edit::Value::Integer(value) => value.as_repr(),
        _ => return Err(IntentTomlError::Type(key)),
    }
    .and_then(|repr| repr.as_raw().as_str())
    .ok_or(IntentTomlError::Type(key))?;
    parse_decimal_scaled(raw, precision).map(Some)
}

fn parse_decimal_scaled(raw: &str, max_fraction: usize) -> Result<u64, IntentTomlError> {
    if raw.contains(['e', 'E', '+', '-']) || raw.is_empty() {
        return Err(IntentTomlError::Type("decimal"));
    }
    let (whole, fraction) = raw.split_once('.').unwrap_or((raw, ""));
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
        || fraction.len() > max_fraction
    {
        return Err(IntentTomlError::Type("decimal"));
    }
    let scale = 10_u64.pow(max_fraction as u32);
    let whole = whole
        .parse::<u64>()
        .map_err(|_| IntentTomlError::Type("decimal"))?;
    let fraction = format!("{fraction:0<max_fraction$}")
        .parse::<u64>()
        .map_err(|_| IntentTomlError::Type("decimal"))?;
    whole
        .checked_mul(scale)
        .and_then(|value| value.checked_add(fraction))
        .ok_or(IntentTomlError::Type("decimal"))
}
