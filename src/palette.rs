//! Deterministic managed colors generated from wallpaper bytes.

use std::{io::Cursor, str::FromStr};

use image::GenericImageView;

use crate::domain::{Color, ColorsManifest};

const CLUSTERS: usize = 8;
const ITERATIONS: usize = 16;
const MAX_PIXELS: u64 = 16_777_216;
const MAX_SAMPLES: usize = 65_536;
const ANSI_NORMAL: [[u8; 3]; 6] = [
    [205, 49, 49],
    [13, 188, 121],
    [229, 229, 16],
    [36, 114, 200],
    [188, 63, 188],
    [17, 168, 205],
];
const ANSI_BRIGHT: [[u8; 3]; 6] = [
    [241, 76, 76],
    [35, 209, 139],
    [245, 245, 67],
    [59, 142, 234],
    [214, 92, 214],
    [41, 184, 219],
];

type Rgb = [u8; 3];

#[derive(Debug)]
pub(crate) struct PaletteError;

#[cfg(test)]
fn generate_kmeans_v1(bytes: &[u8]) -> Result<ColorsManifest, PaletteError> {
    generate(bytes, false, false).map(|generated| generated.colors)
}

#[cfg(test)]
fn generate_kmeans_v2(bytes: &[u8]) -> Result<ColorsManifest, PaletteError> {
    generate(bytes, true, false).map(|generated| generated.colors)
}

pub(crate) fn generate_kmeans_v3(bytes: &[u8]) -> Result<ColorsManifest, PaletteError> {
    generate(bytes, true, true).map(|generated| generated.colors)
}

/// Image analysis is cached by the editor; opacity changes need no decoding or clustering.
#[derive(Clone)]
pub(crate) struct GeneratedPalette {
    colors: ColorsManifest,
    brightest: f64,
}

impl GeneratedPalette {
    pub(crate) fn for_opacity(&self, opacity: u32) -> ColorsManifest {
        let background = self.colors.background();
        let base = relative_luminance(background.as_rgb());
        let brightest = base + f64::from(opacity) / 1_000_000.0 * (self.brightest - base);
        let correct = |value: Color| {
            let candidate = value.as_rgb();
            // All v3 backgrounds are dark. Moving text toward white preserves its hue
            // where possible; if even white fails, preview reports the opacity problem.
            let passes = |rgb| (relative_luminance(rgb) + 0.05) / (brightest + 0.05) >= 4.5;
            if passes(candidate) {
                return value;
            }
            let (mut low, mut high) = (0, 255);
            while low < high {
                let middle = (low + high) / 2;
                if passes(mix(candidate, [255; 3], middle, 255)) {
                    high = middle;
                } else {
                    low = middle + 1;
                }
            }
            color(mix(candidate, [255; 3], low, 255)).expect("RGB is a valid color")
        };
        ColorsManifest::new(
            background,
            correct(self.colors.foreground()),
            self.colors.palette().map(correct),
        )
        .with_cursor(correct(self.colors.cursor().expect("generated cursor")))
        .with_selection_background(
            self.colors
                .selection_background()
                .expect("generated selection"),
        )
        .with_selection_foreground(
            self.colors
                .selection_foreground()
                .expect("generated selection"),
        )
    }
}

pub(crate) fn analyze(bytes: &[u8]) -> Result<GeneratedPalette, PaletteError> {
    generate(bytes, true, true)
}

pub(crate) fn generate_kmeans_v4(
    bytes: &[u8],
    opacity: u32,
) -> Result<ColorsManifest, PaletteError> {
    Ok(analyze(bytes)?.for_opacity(opacity))
}

fn generate(bytes: &[u8], tinted: bool, darken: bool) -> Result<GeneratedPalette, PaletteError> {
    let format = image::guess_format(bytes).map_err(|_| PaletteError)?;
    if !matches!(format, image::ImageFormat::Png | image::ImageFormat::Jpeg) {
        return Err(PaletteError);
    }
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16_384);
    limits.max_image_height = Some(16_384);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    let image = reader.decode().map_err(|_| PaletteError)?;
    if !matches!(
        image.color(),
        image::ColorType::L8
            | image::ColorType::La8
            | image::ColorType::Rgb8
            | image::ColorType::Rgba8
    ) {
        return Err(PaletteError);
    }
    let (width, height) = image.dimensions();
    let pixels = u64::from(width) * u64::from(height);
    if pixels == 0 || pixels > MAX_PIXELS {
        return Err(PaletteError);
    }

    let rgb = image.to_rgb8();
    let raw = rgb.as_raw();
    let sample_count = usize::try_from(pixels.min(MAX_SAMPLES as u64)).map_err(|_| PaletteError)?;
    let mut samples = Vec::with_capacity(sample_count);
    for sample_index in 0..sample_count {
        let pixel_index = sample_index as u64 * pixels / sample_count as u64;
        let offset = usize::try_from(pixel_index * 3).map_err(|_| PaletteError)?;
        samples.push([raw[offset], raw[offset + 1], raw[offset + 2]]);
    }

    let centers = kmeans(&samples);
    let center = *centers
        .iter()
        .min_by_key(|color| (luminance(**color), **color))
        .ok_or(PaletteError)?;
    let background = if darken {
        mix(center, [0, 0, 0], 2, 3)
    } else {
        center
    };
    let original_foreground = contrasting_text(background);
    let foreground = if tinted {
        centers
            .iter()
            .copied()
            .filter(|center| chroma(*center) >= 16)
            .max_by_key(|center| {
                (
                    chroma(*center),
                    luminance(*center).abs_diff(luminance(background)),
                    *center,
                )
            })
            .map(|center| readable(background, mix(center, original_foreground, 1, 3)))
            .unwrap_or(original_foreground)
    } else {
        original_foreground
    };
    let selection_background = mix(background, foreground, 1, 3);
    let selection_foreground = if tinted {
        readable(selection_background, foreground)
    } else {
        contrasting_text(selection_background)
    };

    let mut palette = [[0; 3]; 16];
    palette[0] = if tinted {
        readable(background, background)
    } else {
        background
    };
    palette[7] = foreground;
    palette[8] = if tinted {
        readable(background, selection_background)
    } else {
        selection_background
    };
    palette[15] = foreground;
    for index in 0..6 {
        let normal = mix(
            nearest(ANSI_NORMAL[index], &centers),
            ANSI_NORMAL[index],
            1,
            2,
        );
        let bright = mix(
            nearest(ANSI_BRIGHT[index], &centers),
            ANSI_BRIGHT[index],
            1,
            2,
        );
        palette[index + 1] = if tinted {
            readable(background, normal)
        } else {
            normal
        };
        palette[index + 9] = if tinted {
            readable(background, bright)
        } else {
            bright
        };
    }

    let base_luminance = relative_luminance(background);
    // ponytail: whole-image bound includes cropped-out highlights; use viewport-aware
    // analysis only if this conservative correction proves too strong.
    let brightest = image
        .pixels()
        .fold(base_luminance, |brightest, (_, _, pixel)| {
            let alpha = f64::from(pixel[3]) / 255.0;
            let composed = base_luminance * (1.0 - alpha)
                + relative_luminance([pixel[0], pixel[1], pixel[2]]) * alpha;
            brightest.max(composed)
        });
    let colors = ColorsManifest::new(
        color(background)?,
        color(foreground)?,
        palette
            .map(color)
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?
            .try_into()
            .map_err(|_| PaletteError)?,
    )
    .with_cursor(color(foreground)?)
    .with_selection_background(color(selection_background)?)
    .with_selection_foreground(color(selection_foreground)?);
    Ok(GeneratedPalette { colors, brightest })
}

fn kmeans(samples: &[Rgb]) -> [Rgb; CLUSTERS] {
    let mut centers = [[0; 3]; CLUSTERS];
    let count = samples.len() as u64;
    for channel in 0..3 {
        let sum: u64 = samples
            .iter()
            .map(|sample| u64::from(sample[channel]))
            .sum();
        centers[0][channel] = ((sum + count / 2) / count) as u8;
    }
    for index in 1..CLUSTERS {
        centers[index] = *samples
            .iter()
            .max_by_key(|sample| {
                let distance = centers[..index]
                    .iter()
                    .map(|center| distance_squared(**sample, *center))
                    .min()
                    .unwrap_or_default();
                (distance, **sample)
            })
            .unwrap_or(&centers[index - 1]);
    }

    for _ in 0..ITERATIONS {
        let previous = centers;
        let mut sums = [[0_u64; 3]; CLUSTERS];
        let mut counts = [0_u64; CLUSTERS];
        for sample in samples {
            let index = nearest_index(*sample, &centers);
            counts[index] += 1;
            for channel in 0..3 {
                sums[index][channel] += u64::from(sample[channel]);
            }
        }
        for index in 0..CLUSTERS {
            if counts[index] == 0 {
                continue;
            }
            for channel in 0..3 {
                centers[index][channel] =
                    ((sums[index][channel] + counts[index] / 2) / counts[index]) as u8;
            }
        }
        if centers == previous {
            break;
        }
    }
    centers
}

fn nearest(target: Rgb, centers: &[Rgb; CLUSTERS]) -> Rgb {
    centers[nearest_index(target, centers)]
}

fn nearest_index(target: Rgb, centers: &[Rgb; CLUSTERS]) -> usize {
    centers
        .iter()
        .enumerate()
        .min_by_key(|(index, center)| (distance_squared(target, **center), *index))
        .map_or(0, |(index, _)| index)
}

fn distance_squared(left: Rgb, right: Rgb) -> u32 {
    left.into_iter()
        .zip(right)
        .map(|(left, right)| {
            let difference = i32::from(left) - i32::from(right);
            (difference * difference) as u32
        })
        .sum()
}

fn mix(base: Rgb, overlay: Rgb, overlay_parts: u16, total_parts: u16) -> Rgb {
    std::array::from_fn(|channel| {
        let base_parts = total_parts - overlay_parts;
        ((u16::from(base[channel]) * base_parts
            + u16::from(overlay[channel]) * overlay_parts
            + total_parts / 2)
            / total_parts) as u8
    })
}

fn chroma(color: Rgb) -> u8 {
    color.into_iter().max().unwrap_or(0) - color.into_iter().min().unwrap_or(0)
}

fn readable(background: Rgb, candidate: Rgb) -> Rgb {
    if contrast(background, candidate) >= 4.5 {
        return candidate;
    }
    let extreme = contrasting_text(background);
    // RFC 0005: find first rounded mix meeting threshold, not an approximate float weight.
    let (mut low, mut high) = (0_u16, 255_u16);
    while low < high {
        let middle = (low + high) / 2;
        if contrast(background, mix(candidate, extreme, middle, 255)) >= 4.5 {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    mix(candidate, extreme, low, 255)
}

fn contrasting_text(background: Rgb) -> Rgb {
    let black = [0, 0, 0];
    let white = [255, 255, 255];
    if contrast(background, black) >= contrast(background, white) {
        black
    } else {
        white
    }
}

fn contrast(left: Rgb, right: Rgb) -> f64 {
    let left = relative_luminance(left);
    let right = relative_luminance(right);
    (left.max(right) + 0.05) / (left.min(right) + 0.05)
}

pub(crate) fn relative_luminance(color: Rgb) -> f64 {
    static LINEAR: std::sync::OnceLock<[f64; 256]> = std::sync::OnceLock::new();
    let table = LINEAR.get_or_init(|| {
        std::array::from_fn(|channel| {
            let value = channel as f64 / 255.0;
            if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        })
    });
    let [r, g, b] = color.map(|c| table[usize::from(c)]);
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

fn luminance(color: Rgb) -> u32 {
    2_126 * u32::from(color[0]) + 7_152 * u32::from(color[1]) + 722 * u32::from(color[2])
}

fn color(value: Rgb) -> Result<Color, PaletteError> {
    Color::from_str(&format!("{:02x}{:02x}{:02x}", value[0], value[1], value[2]))
        .map_err(|_| PaletteError)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composition_policy_covers_bright_dark_saturated_mixed_and_alpha_images() {
        use image::{DynamicImage, Rgba, RgbaImage};
        for pixels in [
            vec![[0, 0, 0, 255]],
            vec![[255, 255, 255, 255]],
            vec![[128, 128, 128, 255]],
            vec![[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255]],
            vec![[0, 0, 0, 255], [255, 255, 255, 255]],
            vec![[255, 255, 255, 0]],
            vec![[255, 255, 255, 128], [0, 0, 0, 255]],
        ] {
            let image = RgbaImage::from_fn(pixels.len() as u32, 1, |x, _| Rgba(pixels[x as usize]));
            let mut bytes = Cursor::new(Vec::new());
            DynamicImage::ImageRgba8(image)
                .write_to(&mut bytes, image::ImageFormat::Png)
                .unwrap();
            let generated = analyze(bytes.get_ref()).unwrap();
            assert_eq!(generated.for_opacity(0), generated.colors);
            for opacity in [0, 50_000, 100_000, 200_000, 1_000_000] {
                let colors = generated.for_opacity(opacity);
                let base = relative_luminance(colors.background().as_rgb());
                let brightest =
                    base + f64::from(opacity) / 1_000_000.0 * (generated.brightest - base);
                for text in colors
                    .palette()
                    .iter()
                    .copied()
                    .chain([colors.foreground(), colors.cursor().unwrap()])
                {
                    if 1.05 / (brightest + 0.05) < 4.5 {
                        assert_eq!(
                            text.as_rgb(),
                            [255; 3],
                            "best possible light text when opacity is too high"
                        );
                    } else {
                        assert!(
                            (relative_luminance(text.as_rgb()) + 0.05) / (brightest + 0.05) >= 4.5
                        );
                    }
                }
                assert!(
                    contrast(
                        colors.selection_background().unwrap().as_rgb(),
                        colors.selection_foreground().unwrap().as_rgb()
                    ) >= 4.5
                );
            }
        }
    }

    #[test]
    fn converged_clustering_keeps_exact_centers() {
        let samples = vec![[80, 120, 160]; MAX_SAMPLES];
        let start = std::time::Instant::now();
        assert_eq!(kmeans(&samples), [[80, 120, 160]; CLUSTERS]);
        eprintln!(
            "clustering {} identical samples: {:?}",
            samples.len(),
            start.elapsed()
        );
    }

    #[test]
    fn v3_fixture_stays_byte_compatible_for_historical_provenance() {
        let colors = generate_kmeans_v3(include_bytes!("../tests/fixtures/palette.png")).unwrap();
        assert_eq!(colors.background().to_string(), "090b0f");
        assert_eq!(colors.foreground().to_string(), "eecd6c");
        assert_eq!(colors.palette()[1].to_string(), "cc4f53");
        assert_eq!(colors.palette()[8].to_string(), "807962");
    }

    #[test]
    fn v1_fixture_stays_byte_compatible_for_historical_provenance() {
        let colors = generate_kmeans_v1(include_bytes!("../tests/fixtures/palette.png")).unwrap();
        assert_eq!(colors.background().to_string(), "1a212c");
        assert_eq!(colors.foreground().to_string(), "ffffff");
        assert_eq!(colors.palette()[1].to_string(), "c12a2f");
        assert_eq!(colors.palette()[8].to_string(), "666b72");
        assert_eq!(colors.palette()[14].to_string(), "219ac7");
    }

    #[test]
    fn v2_fixture_stays_byte_compatible_for_historical_provenance() {
        let colors = generate_kmeans_v2(include_bytes!("../tests/fixtures/palette.png")).unwrap();
        assert_eq!(colors.background().to_string(), "1a212c");
        assert_eq!(colors.foreground().to_string(), "eecd6c");
        assert_eq!(colors.palette()[1].to_string(), "d3666a");
        assert_eq!(colors.palette()[8].to_string(), "8d8876");
        assert_eq!(colors.palette()[14].to_string(), "219ac7");
    }
}
