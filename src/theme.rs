//! Read-only Ghostty theme-file resolution.

use std::{
    fs::OpenOptions,
    io::Read,
    path::{Component, Path, PathBuf},
};

use thiserror::Error;

use crate::domain::{Color, ColorsManifest};

/// Failure to locate or decode one named Ghostty theme.
#[derive(Debug, Error)]
pub enum ThemeError {
    /// Theme name could escape configured theme roots.
    #[error("invalid Ghostty theme name")]
    InvalidName,
    /// No configured root contains the named theme.
    #[error("Ghostty theme not found")]
    NotFound,
    /// Theme file could not be read safely.
    #[error("cannot read Ghostty theme at {path}: {source}")]
    Io {
        /// Theme path inspected by the adapter.
        path: PathBuf,
        /// Underlying filesystem failure.
        source: std::io::Error,
    },
    /// Theme does not define a complete supported managed color model.
    #[error("malformed or unsupported Ghostty theme")]
    Malformed,
}

/// Narrow adapter boundary for named Ghostty theme resolution.
pub trait ThemeResolver {
    /// Resolves one exact theme name into only Ghostty Wall-managed colors.
    fn resolve(&self, theme: &str) -> Result<ColorsManifest, ThemeError>;
}

/// Filesystem adapter over ordered local and built-in Ghostty theme roots.
#[derive(Clone, Debug)]
pub struct ThemeFileResolver {
    roots: Vec<PathBuf>,
}

impl ThemeFileResolver {
    /// Creates an adapter. Earlier roots take precedence over later roots.
    pub fn new(roots: impl IntoIterator<Item = PathBuf>) -> Self {
        Self {
            roots: roots.into_iter().collect(),
        }
    }
}

impl ThemeResolver for ThemeFileResolver {
    fn resolve(&self, theme: &str) -> Result<ColorsManifest, ThemeError> {
        validate_name(theme)?;
        for root in &self.roots {
            let path = root.join(theme);
            match read_bounded(&path) {
                Ok(bytes) => return parse_theme(&bytes),
                Err(ThemeError::Io { source, .. })
                    if source.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }
        Err(ThemeError::NotFound)
    }
}

fn validate_name(theme: &str) -> Result<(), ThemeError> {
    let mut components = Path::new(theme).components();
    if theme.is_empty()
        || !matches!(components.next(), Some(Component::Normal(_)))
        || components.next().is_some()
    {
        return Err(ThemeError::InvalidName);
    }
    Ok(())
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, ThemeError> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options.open(path).map_err(|source| ThemeError::Io {
        path: path.to_owned(),
        source,
    })?;
    let metadata = file.metadata().map_err(|source| ThemeError::Io {
        path: path.to_owned(),
        source,
    })?;
    if !metadata.is_file() || metadata.len() > MAX_THEME_BYTES {
        return Err(ThemeError::Malformed);
    }
    let mut bytes = Vec::new();
    file.take(MAX_THEME_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|source| ThemeError::Io {
            path: path.to_owned(),
            source,
        })?;
    if bytes.len() as u64 > MAX_THEME_BYTES {
        return Err(ThemeError::Malformed);
    }
    Ok(bytes)
}

fn parse_theme(bytes: &[u8]) -> Result<ColorsManifest, ThemeError> {
    let text = std::str::from_utf8(bytes).map_err(|_| ThemeError::Malformed)?;
    let mut background = None;
    let mut foreground = None;
    let mut palette = [None; 16];
    let mut cursor = None;
    let mut selection_background = None;
    let mut selection_foreground = None;

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = line.split_once('=').ok_or(ThemeError::Malformed)?;
        let key = key.trim();
        let value = value.trim();
        match key {
            "background" => set_once(&mut background, parse_color(value)?)?,
            "foreground" => set_once(&mut foreground, parse_color(value)?)?,
            "cursor-color" => set_once(&mut cursor, parse_color(value)?)?,
            "selection-background" => set_once(&mut selection_background, parse_color(value)?)?,
            "selection-foreground" => set_once(&mut selection_foreground, parse_color(value)?)?,
            "palette" => {
                let (index, color) = value.split_once('=').ok_or(ThemeError::Malformed)?;
                let index = index
                    .trim()
                    .parse::<usize>()
                    .map_err(|_| ThemeError::Malformed)?;
                let slot = palette.get_mut(index).ok_or(ThemeError::Malformed)?;
                set_once(slot, parse_color(color.trim())?)?;
            }
            _ => {}
        }
    }

    let palette: [Color; 16] = palette
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .and_then(|values| values.try_into().ok())
        .ok_or(ThemeError::Malformed)?;
    let mut colors = ColorsManifest::new(
        background.ok_or(ThemeError::Malformed)?,
        foreground.ok_or(ThemeError::Malformed)?,
        palette,
    );
    if let Some(color) = cursor {
        colors = colors.with_cursor(color);
    }
    if let Some(color) = selection_background {
        colors = colors.with_selection_background(color);
    }
    if let Some(color) = selection_foreground {
        colors = colors.with_selection_foreground(color);
    }
    Ok(colors)
}

fn parse_color(value: &str) -> Result<Color, ThemeError> {
    value
        .strip_prefix('#')
        .unwrap_or(value)
        .to_ascii_lowercase()
        .parse()
        .map_err(|_| ThemeError::Malformed)
}

fn set_once<T>(slot: &mut Option<T>, value: T) -> Result<(), ThemeError> {
    if slot.replace(value).is_some() {
        return Err(ThemeError::Malformed);
    }
    Ok(())
}

const MAX_THEME_BYTES: u64 = 1024 * 1024;
