//! Approximate, read-only terminal sample shared by browsing and draft forms.

use crate::domain::{Color as ProfileColor, EnvironmentManifest, WallpaperManifest};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

pub(crate) struct Sample {
    pub manifest: EnvironmentManifest,
    pub thumbnail: Option<image::RgbImage>,
}

impl Sample {
    /// Inputs have already passed the application workflow's bounded image validation.
    pub fn new(manifest: EnvironmentManifest, image: Option<&[u8]>) -> Self {
        Self {
            manifest,
            thumbnail: image
                .and_then(|bytes| image::load_from_memory(bytes).ok())
                .map(|image| image.thumbnail(40, 16).to_rgb8()),
        }
    }

    pub fn draw(&self, frame: &mut Frame, area: Rect) {
        draw_sample(frame, area, &self.manifest, &self.thumbnail);
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
    let block = Block::default()
        .title("Internal sample (approximate)")
        .borders(Borders::ALL);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let Some(colors) = manifest.colors() else {
        frame.render_widget(Paragraph::new("Colors unmanaged. Choose Colors to enable Automatic.\nSample is not effective Ghostty configuration.").wrap(Wrap { trim: false }), inner);
        return;
    };
    let opacity = match manifest.wallpaper() {
        Some(WallpaperManifest::Image(image)) => {
            image.opacity().map(|v| u64::from(v.get())).unwrap_or(0)
        }
        _ => 0,
    };
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
                background = std::array::from_fn(|i| {
                    ((u64::from(background[i]) * (1_000_000 - opacity)
                        + u64::from(pixel[i]) * opacity)
                        / 1_000_000) as u8
                });
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
            "$ echo Hello, Ghostty",
            Style::default().fg(rgb(colors.foreground())),
        )),
        Line::from(Span::styled(
            "Readable text sample",
            Style::default().fg(rgb(colors.foreground())),
        )),
    ];
    lines.extend(colors.palette().chunks(swatches).map(|row| {
        Line::from(
            row.iter()
                .map(|c| Span::styled("██ ", Style::default().fg(rgb(*c))))
                .collect::<Vec<_>>(),
        )
    }));
    lines.push(Line::from(Span::styled(
        " Selected text ",
        Style::default()
            .bg(rgb(colors
                .selection_background()
                .unwrap_or(colors.background())))
            .fg(rgb(colors
                .selection_foreground()
                .unwrap_or(colors.foreground()))),
    )));
    lines.push(Line::from(Span::styled(
        "Cursor █",
        Style::default().fg(rgb(colors.cursor().unwrap_or(colors.foreground()))),
    )));
    frame.render_widget(Paragraph::new(lines), inner);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        ColorsManifest, ImageWallpaper, MediaType, OpacityMillionths, Sha256Digest,
    };
    use ratatui::{Terminal, backend::TestBackend};

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
        assert_eq!(buffer[(1, 1)].bg, Color::Rgb(100, 50, 25));
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
