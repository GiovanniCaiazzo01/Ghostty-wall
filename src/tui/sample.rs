//! Approximate, read-only terminal sample shared by browsing and draft forms.

mod graphics;
use crate::domain::{
    Color as ProfileColor, ColorsManifest, EnvironmentManifest, WallpaperManifest,
};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use std::{
    io::{Cursor, Write},
    sync::Arc,
};

#[derive(Clone)]
pub(crate) struct Sample {
    pub manifest: EnvironmentManifest,
    pub thumbnail: Option<image::RgbImage>,
    source: Option<Arc<image::RgbaImage>>,
    graphics: Option<Arc<str>>,
}

impl Sample {
    /// Inputs have already passed the application workflow's bounded image validation.
    pub fn new(manifest: EnvironmentManifest, image: Option<&[u8]>) -> Self {
        Self {
            manifest,
            thumbnail: image
                .and_then(|bytes| image::load_from_memory(bytes).ok())
                .map(|image| image.thumbnail(40, 16).to_rgb8()),
            source: None,
            graphics: None,
        }
    }

    pub fn for_preview(
        manifest: EnvironmentManifest,
        image: Option<&[u8]>,
    ) -> Result<Self, image::ImageError> {
        let source = image
            .map(image::load_from_memory)
            .transpose()?
            .map(|i| Arc::new(i.to_rgba8()));
        Ok(Self {
            manifest,
            thumbnail: None,
            source,
            graphics: None,
        })
    }

    pub fn prepare_graphics(&mut self, area: Rect) -> Result<(), image::ImageError> {
        if let Some(source) = &self.source {
            let mut surface = graphics::compose(
                &self.manifest,
                source,
                (u32::from(area.width.saturating_sub(2)) * 8).clamp(1, 1600),
                (u32::from(area.height.saturating_sub(2)) * 16).clamp(1, 1200),
            );
            self.thumbnail = Some(image::imageops::thumbnail(&surface, 80, 40));
            if let Some(colors) = self.manifest.colors()
                && let Some(background) = colors.selection_background()
            {
                // Kitty's negative z sits above cell backgrounds, so selection belongs
                // in the image as well as the terminal buffer; glyphs stay terminal text.
                let width = u32::from(area.width.saturating_sub(2)).max(1);
                let height = u32::from(area.height.saturating_sub(2)).max(1);
                let row = if width < 24 { 6 } else { 4 };
                let end_x = (selection_label(colors).len() as u32 * surface.width() / width)
                    .min(surface.width());
                let start_y = (row * surface.height() / height).min(surface.height());
                let end_y = ((row + 1) * surface.height() / height).min(surface.height());
                for y in start_y..end_y {
                    for x in 0..end_x {
                        surface.put_pixel(x, y, image::Rgb(background.as_rgb()));
                    }
                }
            }
            let mut png = Cursor::new(Vec::new());
            image::DynamicImage::ImageRgb8(surface).write_to(&mut png, image::ImageFormat::Png)?;
            self.graphics = Some(crate::terminal_browser::base64(png.get_ref()).into());
        }
        Ok(())
    }

    pub fn guidance(&self) -> String {
        let mut text = String::from(
            "Static internal preview; not effective Ghostty configuration. Wallpaper uses linear-light sRGB compositing with the unchanged managed opacity and RGB values, matching Ghostty linear/linear-corrected blending (Linux default). Native blending, Display P3, user overrides, terminal font, blur and desktop transparency are not inferred or simulated. Image geometry uses a synthetic 8x16-pixel cell.\n",
        );
        if self.manifest.colors().is_none() {
            text.push_str("Colors unmanaged: neutral illustrative text/background, not your inherited colors.\n");
        }
        match self.manifest.wallpaper() {
            Some(WallpaperManifest::Image(i)) => {
                text.push_str(&format!(
                    "Wallpaper: fit {}, position {}, repeat {}, opacity {}.\n",
                    i.fit()
                        .map_or("inherited (illustrated as cover)", |v| v.as_str()),
                    i.position()
                        .map_or("inherited (illustrated as center)", |v| v.as_str()),
                    i.repeat()
                        .map_or("inherited (illustrated as false)", |v| if v {
                            "true"
                        } else {
                            "false"
                        }),
                    i.opacity().map_or_else(
                        || "inherited (illustrated as 1.0)".into(),
                        |v| format!("{:.6}", f64::from(v.get()) / 1_000_000.0)
                    )
                ));
            }
            Some(WallpaperManifest::None) => text.push_str("Wallpaper disabled.\n"),
            None => {
                text.push_str("Wallpaper unmanaged: inherited image is unknown and is not shown.\n")
            }
        }
        text.push_str("Unmanaged cursor/selection colors are labelled inherited. Unsupported graphics use a color-cell fallback, not a recognizable photograph. Random Use resolves again with the session seed; changed Sources can produce another candidate.\n");
        text
    }

    pub fn render_graphics(&self, output: &mut impl Write, area: Rect) -> std::io::Result<bool> {
        let Some(encoded) = &self.graphics else {
            return Ok(false);
        };
        if area.width < 3 || area.height < 3 {
            return Ok(false);
        }
        write!(output, "\x1b7\x1b[{};{}H", area.y + 2, area.x + 2)?;
        let chunks = encoded.as_bytes().chunks(4096);
        let last = chunks.len().saturating_sub(1);
        for (index, chunk) in chunks.enumerate() {
            if index == 0 {
                write!(
                    output,
                    "\x1b_Ga=T,f=100,i=42,q=2,C=1,z=-1,c={},r={},m={};",
                    area.width - 2,
                    area.height - 2,
                    usize::from(index != last)
                )?;
            } else {
                write!(output, "\x1b_Gm={};", usize::from(index != last))?;
            }
            output.write_all(chunk)?;
            output.write_all(b"\x1b\\")?;
        }
        output.write_all(b"\x1b8")?;
        output.flush()?;
        Ok(true)
    }

    pub fn draw(&self, frame: &mut Frame, area: Rect) {
        draw_surface(
            frame,
            area,
            &self.manifest,
            &self.thumbnail,
            self.source.is_some(),
        );
    }
}

fn selection_label(colors: &ColorsManifest) -> &'static str {
    if colors.selection_background().is_some() && colors.selection_foreground().is_some() {
        " Selected text "
    } else {
        "Selected text (inherited)"
    }
}

fn rgb(color: ProfileColor) -> Color {
    let [r, g, b] = color.as_rgb();
    Color::Rgb(r, g, b)
}

pub(crate) fn draw_sample(
    frame: &mut Frame,
    area: Rect,
    manifest: &EnvironmentManifest,
    thumbnail: &Option<image::RgbImage>,
) {
    draw_surface(frame, area, manifest, thumbnail, false);
}

fn draw_surface(
    frame: &mut Frame,
    area: Rect,
    manifest: &EnvironmentManifest,
    thumbnail: &Option<image::RgbImage>,
    composited: bool,
) {
    let inherited = match manifest.wallpaper() {
        Some(WallpaperManifest::Image(i)) => {
            i.opacity().is_none()
                || i.fit().is_none()
                || i.position().is_none()
                || i.repeat().is_none()
        }
        _ => false,
    };
    let note = if thumbnail.is_none() {
        match manifest.wallpaper() {
            Some(WallpaperManifest::None) => "Wallpaper disabled",
            None => "Wallpaper unmanaged; not shown",
            Some(WallpaperManifest::Image(_)) => "Wallpaper image unavailable",
        }
    } else if crate::terminal_browser::TerminalGraphics::from_environment()
        == crate::terminal_browser::TerminalGraphics::Unsupported
    {
        "Graphics unsupported; v details"
    } else if manifest.colors().is_none() {
        "Colors unmanaged; inherited sample only"
    } else if inherited {
        "Inherited settings; v details"
    } else {
        "Linear sRGB sample; v details"
    };
    let block = Block::default()
        .title("Internal sample (approximate)")
        .title_bottom(note)
        .borders(Borders::ALL);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let neutral = ColorsManifest::new(
        "202020".parse().unwrap(),
        "dddddd".parse().unwrap(),
        ["888888".parse().unwrap(); 16],
    );
    let colors = manifest.colors().unwrap_or(&neutral);
    let opacity = if composited {
        1_000_000
    } else {
        match manifest.wallpaper() {
            Some(WallpaperManifest::Image(image)) => {
                image.opacity().map_or(1_000_000, |v| u64::from(v.get()))
            }
            _ => 0,
        }
    };
    let blend = graphics::LinearBlend::new(colors.background().as_rgb(), opacity);
    for y in 0..inner.height {
        for x in 0..inner.width {
            let mut background = colors.background().as_rgb();
            if let Some(image) = thumbnail {
                let pixel = image
                    .get_pixel(
                        u32::from(x) * image.width() / u32::from(inner.width.max(1)),
                        u32::from(y) * image.height() / u32::from(inner.height.max(1)),
                    )
                    .0;
                background = blend.apply([pixel[0], pixel[1], pixel[2], 255]);
            }
            frame.buffer_mut()[(inner.x + x, inner.y + y)]
                .set_symbol(" ")
                .set_bg(Color::Rgb(background[0], background[1], background[2]));
        }
    }
    // Four-column swatches keep all ANSI colors visible in a narrow sample panel.
    let swatches = if inner.width < 24 { 4 } else { 8 };
    let mut lines = vec![
        Line::from(Span::styled(
            if manifest.colors().is_some() {
                "$ echo Hello, Ghostty"
            } else {
                "Colors unmanaged (illustrative)"
            },
            Style::default().fg(rgb(colors.foreground())),
        )),
        Line::from(Span::styled(
            if manifest.colors().is_some() {
                "Readable text sample"
            } else {
                "Wallpaper settings may be inherited"
            },
            Style::default().fg(rgb(colors.foreground())),
        )),
    ];
    if manifest.colors().is_none() {
        lines.push(Line::from("ANSI palette unmanaged"));
        lines.push(Line::from("Not effective Ghostty colors"));
    } else {
        lines.extend(colors.palette().chunks(swatches).map(|row| {
            Line::from(
                row.iter()
                    .map(|c| Span::styled("██ ", Style::default().fg(rgb(*c))))
                    .collect::<Vec<_>>(),
            )
        }));
    }
    lines.push(Line::from(Span::styled(
        selection_label(colors),
        Style::default()
            .bg(rgb(colors
                .selection_background()
                .unwrap_or(colors.background())))
            .fg(rgb(colors
                .selection_foreground()
                .unwrap_or(colors.foreground()))),
    )));
    lines.push(Line::from(Span::styled(
        if colors.cursor().is_some() {
            "Cursor █"
        } else {
            "Cursor inherited"
        },
        Style::default().fg(rgb(colors.cursor().unwrap_or(colors.foreground()))),
    )));
    frame.render_widget(Paragraph::new(lines), inner);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        ColorsManifest, ImageWallpaper, MediaType, OpacityMillionths, Sha256Digest, WallpaperFit,
        WallpaperPosition,
    };
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn preview_matches_ghostty_linear_light_reference_pixels_without_raising_opacity() {
        // Ghostty 1.3.1/OpenGL, alpha-blending=linear-corrected, opacity=.05,
        // opaque black background: owned-window PNG probes (100,550)..(900,550).
        let reference = [
            ([255, 255, 255, 255], [63, 63, 63]),
            ([200, 100, 50, 255], [47, 19, 5]),
            ([255, 0, 0, 255], [63, 0, 0]),
            ([0, 255, 0, 255], [0, 63, 0]),
            ([0, 0, 255, 255], [0, 0, 63]),
            ([255, 255, 255, 128], [44, 44, 44]),
            ([255, 255, 255, 0], [0, 0, 0]),
        ];
        let image = image::RgbaImage::from_fn(reference.len() as u32, 1, |x, _| {
            image::Rgba(reference[x as usize].0)
        });
        let mut png = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let manifest = EnvironmentManifest::new(
            Some(WallpaperManifest::Image(
                ImageWallpaper::new(Sha256Digest::from_bytes([0; 32]), MediaType::Png)
                    .with_opacity(OpacityMillionths::new(50_000).unwrap())
                    .with_fit(WallpaperFit::Stretch),
            )),
            Some(ColorsManifest::new(
                "000000".parse().unwrap(),
                "ffffff".parse().unwrap(),
                ["ffffff".parse().unwrap(); 16],
            )),
            None,
        );
        let mut sample = Sample::for_preview(manifest.clone(), Some(png.get_ref())).unwrap();
        let area = Rect::new(0, 0, reference.len() as u16 + 2, 4);
        sample.prepare_graphics(area).unwrap();
        let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
        terminal
            .draw(|frame| sample.draw(frame, frame.area()))
            .unwrap();
        for (x, (_, [r, g, b])) in reference.iter().enumerate() {
            assert_eq!(
                terminal.backend().buffer()[(x as u16 + 1, 1)].bg,
                Color::Rgb(*r, *g, *b)
            );
        }
        assert_eq!(
            sample.manifest, manifest,
            "compositing must not rewrite managed values"
        );
    }

    #[test]
    fn zero_and_full_opacity_keep_background_and_image_rgb_unchanged() {
        let mut png = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            2,
            2,
            image::Rgb([200, 100, 50]),
        ))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
        for (opacity, expected) in [(0, [10, 20, 30]), (1_000_000, [200, 100, 50])] {
            let manifest = EnvironmentManifest::new(
                Some(WallpaperManifest::Image(
                    ImageWallpaper::new(Sha256Digest::from_bytes([0; 32]), MediaType::Png)
                        .with_opacity(OpacityMillionths::new(opacity).unwrap()),
                )),
                Some(ColorsManifest::new(
                    "0a141e".parse().unwrap(),
                    "ffffff".parse().unwrap(),
                    ["ffffff".parse().unwrap(); 16],
                )),
                None,
            );
            let mut sample = Sample::for_preview(manifest.clone(), Some(png.get_ref())).unwrap();
            sample.prepare_graphics(Rect::new(0, 0, 8, 4)).unwrap();
            let mut terminal = Terminal::new(TestBackend::new(8, 4)).unwrap();
            terminal
                .draw(|frame| sample.draw(frame, frame.area()))
                .unwrap();
            assert_eq!(
                terminal.backend().buffer()[(1, 1)].bg,
                Color::Rgb(expected[0], expected[1], expected[2])
            );
            assert_eq!(sample.manifest, manifest);
        }
    }

    #[test]
    fn compact_sample_discloses_its_blending_model_without_clipping() {
        let manifest = EnvironmentManifest::new(
            Some(WallpaperManifest::Image(
                ImageWallpaper::new(Sha256Digest::from_bytes([0; 32]), MediaType::Png)
                    .with_opacity(OpacityMillionths::new(50_000).unwrap())
                    .with_fit(WallpaperFit::Cover)
                    .with_position(WallpaperPosition::Center)
                    .with_repeat(false),
            )),
            Some(ColorsManifest::new(
                "000000".parse().unwrap(),
                "ffffff".parse().unwrap(),
                ["ffffff".parse().unwrap(); 16],
            )),
            None,
        );
        let mut sample = Sample::for_preview(
            manifest,
            Some(include_bytes!("../../tests/fixtures/white.png")),
        )
        .unwrap();
        sample.prepare_graphics(Rect::new(0, 0, 40, 13)).unwrap();
        let mut terminal = Terminal::new(TestBackend::new(40, 13)).unwrap();
        terminal
            .draw(|frame| sample.draw(frame, frame.area()))
            .unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(text.contains("Linear sRGB sample; v details"));
    }

    #[test]
    fn absent_and_disabled_wallpaper_have_distinct_honest_feedback() {
        for (wallpaper, expected) in [
            (None, "Wallpaper unmanaged"),
            (Some(WallpaperManifest::None), "Wallpaper disabled"),
        ] {
            let sample =
                Sample::for_preview(EnvironmentManifest::new(wallpaper, None, None), None).unwrap();
            let mut terminal = Terminal::new(TestBackend::new(60, 10)).unwrap();
            terminal
                .draw(|frame| sample.draw(frame, frame.area()))
                .unwrap();
            let text: String = terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .map(|c| c.symbol())
                .collect();
            assert!(text.contains(expected), "{expected}: {text}");
        }
    }

    #[test]
    fn transmitted_image_keeps_managed_selection_background_above_wallpaper() {
        let mut png = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            2,
            2,
            image::Rgb([200, 100, 50]),
        ))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
        let manifest = EnvironmentManifest::new(
            Some(WallpaperManifest::Image(
                ImageWallpaper::new(Sha256Digest::from_bytes([0; 32]), MediaType::Png)
                    .with_opacity(OpacityMillionths::new(1_000_000).unwrap()),
            )),
            Some(
                ColorsManifest::new(
                    "000000".parse().unwrap(),
                    "ffffff".parse().unwrap(),
                    ["ffffff".parse().unwrap(); 16],
                )
                .with_selection_background("abcdef".parse().unwrap())
                .with_selection_foreground("123456".parse().unwrap()),
            ),
            None,
        );
        let mut sample = Sample::for_preview(manifest, Some(png.get_ref())).unwrap();
        let area = Rect::new(0, 0, 40, 8);
        sample.prepare_graphics(area).unwrap();
        let mut output = Vec::new();
        sample.render_graphics(&mut output, area).unwrap();
        let output = String::from_utf8(output).unwrap();
        let encoded: String = output
            .split("\x1b_G")
            .skip(1)
            .map(|s| s.split_once(';').unwrap().1.split("\x1b\\").next().unwrap())
            .collect();
        let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut png = Vec::new();
        for group in encoded.as_bytes().as_chunks::<4>().0 {
            let values: Vec<u32> = group
                .iter()
                .map(|c| alphabet.iter().position(|v| v == c).unwrap_or(0) as u32)
                .collect();
            let bits = (values[0] << 18) | (values[1] << 12) | (values[2] << 6) | values[3];
            png.push((bits >> 16) as u8);
            if group[2] != b'=' {
                png.push((bits >> 8) as u8);
            }
            if group[3] != b'=' {
                png.push(bits as u8);
            }
        }
        let rendered = image::load_from_memory(&png).unwrap().to_rgb8();
        assert_eq!(rendered.get_pixel(4, 68).0, [171, 205, 239]);
        assert_eq!(rendered.get_pixel(4, 4).0, [200, 100, 50]);
    }

    #[test]
    fn prepared_preview_represents_fit_anchor_repeat_and_opacity() {
        let mut png = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            16,
            8,
            image::Rgb([200, 100, 50]),
        ))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
        for (fit, repeat, position, expected) in [
            (
                WallpaperFit::Contain,
                false,
                WallpaperPosition::BottomRight,
                [[0, 0, 0], [146, 71, 34]],
            ),
            (
                WallpaperFit::Cover,
                false,
                WallpaperPosition::Center,
                [[146, 71, 34]; 2],
            ),
            (
                WallpaperFit::Stretch,
                false,
                WallpaperPosition::TopLeft,
                [[146, 71, 34]; 2],
            ),
            (
                WallpaperFit::None,
                true,
                WallpaperPosition::BottomRight,
                [[146, 71, 34]; 2],
            ),
            (
                WallpaperFit::None,
                false,
                WallpaperPosition::TopLeft,
                [[146, 71, 34], [0, 0, 0]],
            ),
        ] {
            let manifest = EnvironmentManifest::new(
                Some(WallpaperManifest::Image(
                    ImageWallpaper::new(Sha256Digest::from_bytes([0; 32]), MediaType::Png)
                        .with_opacity(OpacityMillionths::new(500_000).unwrap())
                        .with_fit(fit)
                        .with_position(position)
                        .with_repeat(repeat),
                )),
                Some(ColorsManifest::new(
                    "000000".parse().unwrap(),
                    "ffffff".parse().unwrap(),
                    ["ffffff".parse().unwrap(); 16],
                )),
                None,
            );
            let mut sample = Sample::for_preview(manifest, Some(png.get_ref())).unwrap();
            sample.prepare_graphics(Rect::new(0, 0, 6, 4)).unwrap();
            let mut terminal = Terminal::new(TestBackend::new(6, 4)).unwrap();
            terminal
                .draw(|frame| sample.draw(frame, frame.area()))
                .unwrap();
            for (row, [r, g, b]) in expected.into_iter().enumerate() {
                assert_eq!(
                    terminal.backend().buffer()[(1, row as u16 + 1)].bg,
                    Color::Rgb(r, g, b),
                    "{fit:?}, repeat={repeat}"
                );
            }
        }
    }

    #[test]
    fn unmanaged_colors_do_not_hide_wallpaper_and_inherited_opacity_is_disclosed() {
        let manifest = EnvironmentManifest::new(
            Some(WallpaperManifest::Image(ImageWallpaper::new(
                Sha256Digest::from_bytes([0; 32]),
                MediaType::Png,
            ))),
            None,
            None,
        );
        let image = Some(image::RgbImage::from_pixel(
            2,
            2,
            image::Rgb([200, 100, 50]),
        ));
        let mut terminal = Terminal::new(TestBackend::new(80, 12)).unwrap();
        terminal
            .draw(|frame| draw_sample(frame, frame.area(), &manifest, &image))
            .unwrap();
        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(1, 8)].bg, Color::Rgb(200, 100, 50));
        let text: String = buffer.content().iter().map(|c| c.symbol()).collect();
        assert!(text.contains("inherited"));
        assert!(text.contains("unmanaged"));
    }

    #[test]
    fn sample_blends_wallpaper_and_renders_text_all_ansi_cursor_and_selection() {
        let palette = std::array::from_fn(|i| format!("{i:02x}1122").parse().unwrap());
        let colors = ColorsManifest::new(
            "000000".parse().unwrap(),
            "ffffff".parse().unwrap(),
            palette,
        )
        .with_cursor("123456".parse().unwrap())
        .with_selection_background("abcdef".parse().unwrap())
        .with_selection_foreground("fedcba".parse().unwrap());
        let manifest = EnvironmentManifest::new(
            Some(WallpaperManifest::Image(
                ImageWallpaper::new(Sha256Digest::from_bytes([0; 32]), MediaType::Png)
                    .with_opacity(OpacityMillionths::new(500_000).unwrap()),
            )),
            Some(colors),
            None,
        );
        let image = Some(image::RgbImage::from_pixel(
            2,
            2,
            image::Rgb([200, 100, 50]),
        ));
        let mut terminal = Terminal::new(TestBackend::new(40, 8)).unwrap();
        terminal
            .draw(|frame| draw_sample(frame, frame.area(), &manifest, &image))
            .unwrap();
        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(1, 1)].fg, Color::Rgb(255, 255, 255));
        assert_eq!(buffer[(1, 1)].bg, Color::Rgb(146, 71, 34));
        for i in 0..16_u16 {
            assert_eq!(
                buffer[(1 + (i % 8) * 3, 3 + i / 8)].fg,
                rgb(palette[usize::from(i)])
            );
        }
        assert_eq!(buffer[(1, 5)].fg, Color::Rgb(254, 220, 186));
        assert_eq!(buffer[(1, 5)].bg, Color::Rgb(171, 205, 239));
        assert_eq!(buffer[(1, 6)].fg, Color::Rgb(18, 52, 86));
    }
}
