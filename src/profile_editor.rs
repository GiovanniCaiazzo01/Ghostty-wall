//! In-memory visual editing over validated Profile Intent. No Projection or History writes.

use std::path::Path;

use crate::{
    codec::intent::parse_named_profile_toml,
    domain::{
        Color, ColorsIntent, ColorsManifest, ConfigIntent, EnvironmentManifest, ImageWallpaper,
        ProfileIntent, Sha256Digest, TerminalManifest, WallpaperIntent, WallpaperManifest,
    },
    history::Activation,
    profile_workflow::{ProfileDraft, ProfileWorkflows, WorkflowError},
};
use sha2::{Digest, Sha256};
use toml_edit::{DocumentMut, Item, Table, value};

pub(crate) const COLOR_KEYS: [&str; 5] = [
    "background",
    "foreground",
    "cursor",
    "selection_background",
    "selection_foreground",
];

/// Numeric controls use fixed-point arithmetic, never rounded floating-point input.
#[derive(Clone, Copy, Debug)]
pub enum NumericControl {
    /// Wallpaper-image opacity, independent of the terminal background.
    WallpaperOpacity,
    /// Ghostty terminal background opacity.
    BackgroundOpacity,
    /// Font size in points.
    FontSize,
    /// Platform-dependent background blur intensity.
    Blur,
}
impl NumericControl {
    fn field(self) -> (&'static str, &'static str, u32, u64, u64) {
        match self {
            Self::WallpaperOpacity => ("wallpaper", "opacity", 6, 10_000, 100_000),
            Self::BackgroundOpacity => ("terminal", "background_opacity", 6, 10_000, 1_000_000),
            Self::FontSize => ("terminal", "font_size", 3, 500, 13_000),
            Self::Blur => ("terminal", "background_blur_intensity", 0, 1, 0),
        }
    }
}

/// Draft plus the resolved opening snapshot. Internal preview is not a live reload.
pub struct ProfileEditor {
    draft: ProfileDraft,
    config: ConfigIntent,
    starting: Option<Activation>,
    image: Option<Vec<u8>>,
    image_identity: Option<ImageWallpaper>,
    generated: Option<ColorsManifest>,
    theme_colors: Option<ColorsManifest>,
}
impl ProfileEditor {
    /// Opens from the caller's read-only resolution and durable starting Activation.
    /// Saved Profile bytes are retained by the draft for optimistic save validation.
    pub fn new(
        draft: ProfileDraft,
        config: ConfigIntent,
        resolved: EnvironmentManifest,
        image: Option<Vec<u8>>,
        starting: Option<Activation>,
    ) -> Result<Self, WorkflowError> {
        let image_identity = match resolved.wallpaper() {
            Some(WallpaperManifest::Image(image)) => Some(image.clone()),
            _ => None,
        };
        if image_identity.as_ref().map(ImageWallpaper::asset_sha256)
            != image
                .as_ref()
                .map(|bytes| Sha256Digest::from_bytes(Sha256::digest(bytes).into()))
        {
            return Err(WorkflowError::Invalid(
                "editor image differs from resolved Profile",
            ));
        }
        let generated = image.as_deref().map(generate_colors).transpose()?;
        Ok(Self {
            draft,
            config,
            starting,
            image,
            image_identity,
            generated,
            theme_colors: resolved.colors().cloned(),
        })
    }

    /// Current draft, not yet saved. Saving is a separate workflow operation.
    pub fn draft(&self) -> &ProfileDraft {
        &self.draft
    }

    /// Starting durable Activation; never used to roll back a concurrent apply in internal mode.
    pub fn starting_activation(&self) -> Option<&Activation> {
        self.starting.as_ref()
    }

    /// Decoded/validated image bytes used by the internal sample.
    pub fn image(&self) -> Option<&[u8]> {
        self.image.as_deref()
    }

    /// Strictly validated current Intent, including automatic/customized color intent.
    pub fn intent(&self) -> Result<ProfileIntent, WorkflowError> {
        Ok(parse_named_profile_toml(
            self.draft.id().as_str(),
            &self.config,
            self.draft.document(),
        )?
        .1)
    }

    fn document(&self) -> Result<DocumentMut, WorkflowError> {
        self.draft
            .document()
            .parse()
            .map_err(|_| WorkflowError::Invalid("invalid editor draft"))
    }
    fn change(&mut self, document: DocumentMut) -> Result<(), WorkflowError> {
        self.draft.set_document(&self.config, document.to_string())
    }

    /// Imports a replacement using the same bounded image workflow as creation.
    pub fn import_image(
        &mut self,
        workflow: &ProfileWorkflows,
        path: &Path,
    ) -> Result<(), WorkflowError> {
        workflow.import_image(&mut self.draft, path)?;
        self.refresh_image()
    }

    /// Explicitly generates another one-time wallpaper; never runs on slider/key movement.
    pub fn generate_image(
        &mut self,
        workflow: &ProfileWorkflows,
        seed: Sha256Digest,
    ) -> Result<(), WorkflowError> {
        workflow.generate_image(&mut self.draft, seed)?;
        self.refresh_image()
    }
    fn refresh_image(&mut self) -> Result<(), WorkflowError> {
        let bytes = self
            .draft
            .staged_image()
            .ok_or(WorkflowError::Invalid("replacement image missing"))?;
        let media = match image::guess_format(bytes) {
            Ok(image::ImageFormat::Png) => crate::domain::MediaType::Png,
            Ok(image::ImageFormat::Jpeg) => crate::domain::MediaType::Jpeg,
            _ => return Err(WorkflowError::Invalid("unsupported replacement image")),
        };
        self.generated = Some(generate_colors(bytes)?);
        self.image_identity = Some(ImageWallpaper::new(
            Sha256Digest::from_bytes(Sha256::digest(bytes).into()),
            media,
        ));
        self.image = Some(bytes.to_vec());
        Ok(())
    }

    /// Sets a numeric value lexically; invalid precision/ranges leave the draft intact.
    pub fn set_number(
        &mut self,
        control: NumericControl,
        input: &str,
    ) -> Result<(), WorkflowError> {
        let (section, key, _, _, _) = control.field();
        if section == "wallpaper"
            && !matches!(
                self.intent()?.wallpaper,
                Some(WallpaperIntent::Source { .. })
            )
        {
            return Err(WorkflowError::Invalid("choose a wallpaper image first"));
        }
        // Parse the full lexical assignment so RFC 0003 precision checks see the original spelling.
        let assignment: DocumentMut = format!("{key} = {input}\n")
            .parse()
            .map_err(|_| WorkflowError::Invalid("enter a decimal number in the displayed range"))?;
        if assignment.len() != 1 || input.chars().any(|c| !(c.is_ascii_digit() || c == '.')) {
            return Err(WorkflowError::Invalid(
                "enter a decimal number, without exponent notation",
            ));
        }
        let mut doc = self.document()?;
        ensure_table(&mut doc, section);
        doc[section][key] = assignment[key].clone();
        self.change(doc)
    }

    /// Increments/decrements one exact step. Range boundaries are errors, not silent clamping.
    pub fn step_number(
        &mut self,
        control: NumericControl,
        increase: bool,
    ) -> Result<(), WorkflowError> {
        let (_, _, precision, step, initial) = control.field();
        let current = self.number(control)?.unwrap_or(initial);
        let next = if increase {
            current.checked_add(step)
        } else {
            current.checked_sub(step)
        }
        .ok_or(WorkflowError::Invalid("numeric boundary reached"))?;
        self.set_number(control, &decimal(next, precision))
    }
    /// Current fixed-point numeric value; None means unmanaged, not a Ghostty default.
    pub fn number(&self, control: NumericControl) -> Result<Option<u64>, WorkflowError> {
        let intent = self.intent()?;
        Ok(match control {
            NumericControl::WallpaperOpacity => match intent.wallpaper {
                Some(WallpaperIntent::Source { opacity, .. }) => {
                    opacity.map(|v| u64::from(v.get()))
                }
                _ => None,
            },
            NumericControl::BackgroundOpacity => intent
                .terminal
                .and_then(|t| t.background_opacity)
                .map(|v| u64::from(v.get())),
            NumericControl::FontSize => intent
                .terminal
                .and_then(|t| t.font_size)
                .map(|v| u64::from(v.get())),
            NumericControl::Blur => intent
                .terminal
                .and_then(|t| t.background_blur_intensity)
                .map(|v| u64::from(v.get())),
        })
    }
    /// Human-readable numeric value.
    pub fn number_label(&self, control: NumericControl) -> Result<String, WorkflowError> {
        Ok(self
            .number(control)?
            .map(|v| decimal(v, control.field().2))
            .unwrap_or_else(|| "Unmanaged".into()))
    }

    /// Sets a closed choice using validated Intent spellings; callers display friendly labels.
    pub fn set_choice(
        &mut self,
        section: &str,
        key: &str,
        choice: &str,
    ) -> Result<(), WorkflowError> {
        if !matches!(
            (section, key),
            ("wallpaper", "fit" | "position" | "repeat") | ("terminal", "cursor_style")
        ) {
            return Err(WorkflowError::Invalid("unknown editor choice"));
        }
        if section == "wallpaper"
            && !matches!(
                self.intent()?.wallpaper,
                Some(WallpaperIntent::Source { .. })
            )
        {
            return Err(WorkflowError::Invalid("choose a wallpaper image first"));
        }
        let mut doc = self.document()?;
        ensure_table(&mut doc, section);
        doc[section][key] = if key == "repeat" {
            value(
                choice
                    .parse::<bool>()
                    .map_err(|_| WorkflowError::Invalid("invalid repeat choice"))?,
            )
        } else {
            value(choice)
        };
        self.change(doc)
    }

    /// Explicitly enables a complete automatic palette, discarding customized slots.
    pub fn automatic_colors(&mut self) -> Result<(), WorkflowError> {
        if self.generated.is_none() {
            return Err(WorkflowError::Invalid(
                "choose a wallpaper before generating colors",
            ));
        }
        let mut doc = self.document()?;
        doc["colors"] = Item::Table(Table::new());
        doc["colors"]["mode"] = value("generated");
        self.change(doc)
    }

    /// Sets one color (slots 0..5 are scalar colors, 5..21 ANSI colors), or resets it to automatic.
    /// Hex input accepts an optional # and uppercase; stored colors remain canonical lowercase.
    pub fn set_color(&mut self, slot: usize, input: &str) -> Result<(), WorkflowError> {
        if slot >= 21 {
            return Err(WorkflowError::Invalid("invalid color slot"));
        }
        let input = input
            .strip_prefix('#')
            .unwrap_or(input)
            .to_ascii_lowercase();
        if input != "auto" {
            input.parse::<Color>()?;
        }
        let intent = self.intent()?;
        if intent.colors.is_none() {
            return Err(WorkflowError::Invalid(
                "enable Automatic colors first using Reset ALL colors",
            ));
        }
        let generated = matches!(
            intent.colors,
            Some(ColorsIntent::Generated | ColorsIntent::GeneratedWithOverrides(_))
        );
        let mut doc = self.document()?;
        if generated || input == "auto" {
            if self.generated.is_none() {
                return Err(WorkflowError::Invalid(
                    "choose a wallpaper before using automatic colors",
                ));
            }
            if !generated {
                doc["colors"] = Item::Table(Table::new());
                doc["colors"]["mode"] = value("generated");
                if let Some(colors) = self.preview()?.colors() {
                    doc["colors"]["overrides"] = Item::Table(color_table(colors));
                }
            }
            doc["schema_version"] = value(2);
            if doc["colors"].get("overrides").is_none() {
                // Inline tables serialize only value children, not nested Item::Table entries.
                doc["colors"]["overrides"] = if doc["colors"].is_inline_table() {
                    value(toml_edit::InlineTable::new())
                } else {
                    Item::Table(Table::new())
                };
            }
            set_slot(&mut doc["colors"]["overrides"], slot, &input, "auto")?;
        } else {
            if matches!(intent.colors, Some(ColorsIntent::Theme { .. })) {
                let colors = self
                    .theme_colors
                    .as_ref()
                    .ok_or(WorkflowError::Invalid("theme colors unavailable"))?;
                doc["colors"] = Item::Table(color_table(colors));
                doc["colors"]["mode"] = value("explicit");
            }
            set_slot(&mut doc["colors"], slot, &input, "auto")?;
        }
        self.change(doc)
    }

    /// Distinguishes automatic slots from customized/theme values and unmanaged colors.
    pub fn color_origin(&self, slot: usize) -> Result<&'static str, WorkflowError> {
        if slot >= 21 {
            return Err(WorkflowError::Invalid("invalid color slot"));
        }
        let intent = self.intent()?;
        Ok(match intent.colors {
            Some(ColorsIntent::Generated) => "Automatic",
            Some(ColorsIntent::GeneratedWithOverrides(ref overrides)) => {
                let manual = if slot < 5 {
                    overrides.scalars.get(slot)
                } else {
                    overrides.palette.get(slot - 5)
                };
                if manual.is_some_and(Option::is_some) {
                    "Customized"
                } else {
                    "Automatic"
                }
            }
            Some(ColorsIntent::Explicit {
                cursor,
                selection_background,
                selection_foreground,
                ..
            }) if (2..5).contains(&slot)
                && [cursor, selection_background, selection_foreground][slot - 2].is_none() =>
            {
                "Unmanaged"
            }
            Some(ColorsIntent::Explicit { .. }) => "Customized",
            Some(ColorsIntent::Theme { .. })
                if self
                    .theme_colors
                    .as_ref()
                    .is_some_and(|colors| color_slots(colors)[slot].is_some()) =>
            {
                "Theme"
            }
            Some(ColorsIntent::Theme { .. }) | None => "Unmanaged",
        })
    }

    /// Resolves only managed draft settings from captured image/theme inputs, without I/O.
    pub fn preview(&self) -> Result<EnvironmentManifest, WorkflowError> {
        let intent = self.intent()?;
        let wallpaper = match intent.wallpaper {
            Some(WallpaperIntent::None) => Some(WallpaperManifest::None),
            None => None,
            Some(WallpaperIntent::Source {
                fit,
                position,
                opacity,
                repeat,
                ..
            }) => {
                let initial = self
                    .image_identity
                    .as_ref()
                    .ok_or(WorkflowError::Invalid("resolved wallpaper unavailable"))?;
                let mut image = ImageWallpaper::new(initial.asset_sha256(), initial.media_type());
                if let Some(v) = fit {
                    image = image.with_fit(v);
                }
                if let Some(v) = position {
                    image = image.with_position(v);
                }
                if let Some(v) = opacity {
                    image = image.with_opacity(v);
                }
                if let Some(v) = repeat {
                    image = image.with_repeat(v);
                }
                Some(WallpaperManifest::Image(image))
            }
        };
        let colors = match intent.colors {
            None => None,
            Some(ColorsIntent::Theme { .. }) => self.theme_colors.clone(),
            Some(ColorsIntent::Generated | ColorsIntent::GeneratedWithOverrides(_)) => {
                let base = self
                    .generated
                    .clone()
                    .ok_or(WorkflowError::Invalid("automatic colors need an image"))?;
                Some(
                    if let Some(ColorsIntent::GeneratedWithOverrides(overrides)) = intent.colors {
                        crate::plan::apply_color_overrides(base, &overrides)
                    } else {
                        base
                    },
                )
            }
            Some(ColorsIntent::Explicit {
                background,
                foreground,
                palette,
                cursor,
                selection_background,
                selection_foreground,
            }) => {
                let mut colors = ColorsManifest::new(background, foreground, palette);
                if let Some(v) = cursor {
                    colors = colors.with_cursor(v);
                }
                if let Some(v) = selection_background {
                    colors = colors.with_selection_background(v);
                }
                if let Some(v) = selection_foreground {
                    colors = colors.with_selection_foreground(v);
                }
                Some(colors)
            }
        };
        let terminal = intent
            .terminal
            .map(|t| {
                TerminalManifest::new(
                    t.font_size,
                    t.background_opacity,
                    t.background_blur_intensity,
                    t.cursor_style,
                )
            })
            .transpose()?;
        Ok(EnvironmentManifest::new(wallpaper, colors, terminal))
    }
}

fn generate_colors(bytes: &[u8]) -> Result<ColorsManifest, WorkflowError> {
    crate::palette::generate_kmeans_v3(bytes)
        .map_err(|_| WorkflowError::Invalid("image cannot generate readable colors"))
}
fn ensure_table(doc: &mut DocumentMut, name: &str) {
    if doc.get(name).is_none() {
        doc[name] = Item::Table(Table::new());
    }
}
fn decimal(value: u64, precision: u32) -> String {
    let scale = 10_u64.pow(precision);
    if precision == 0 {
        value.to_string()
    } else {
        format!(
            "{}.{:0width$}",
            value / scale,
            value % scale,
            width = precision as usize
        )
    }
}
fn color_table(colors: &ColorsManifest) -> Table {
    let mut table = Table::new();
    for (key, color) in COLOR_KEYS.iter().zip(color_slots(colors)) {
        if let Some(color) = color {
            table[key] = value(color.to_string());
        }
    }
    let mut palette = toml_edit::Array::new();
    for color in colors.palette() {
        palette.push(color.to_string());
    }
    table["palette"] = value(palette);
    table
}
fn set_slot(
    table: &mut Item,
    slot: usize,
    input: &str,
    missing: &str,
) -> Result<(), WorkflowError> {
    if slot < 5 {
        table[COLOR_KEYS[slot]] = value(input);
    } else {
        if table.get("palette").is_none() {
            let mut palette = toml_edit::Array::new();
            for _ in 0..16 {
                palette.push(missing);
            }
            table["palette"] = value(palette);
        }
        table["palette"]
            .as_array_mut()
            .ok_or(WorkflowError::Invalid("invalid palette"))?
            .replace(slot - 5, input);
    }
    Ok(())
}
pub(crate) fn color_slots(colors: &ColorsManifest) -> Vec<Option<Color>> {
    [
        Some(colors.background()),
        Some(colors.foreground()),
        colors.cursor(),
        colors.selection_background(),
        colors.selection_foreground(),
    ]
    .into_iter()
    .chain(colors.palette().iter().copied().map(Some))
    .collect()
}
