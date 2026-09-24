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

pub(crate) fn generate_kmeans_v1(bytes: &[u8]) -> Result<ColorsManifest, PaletteError> {
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
    let background = *centers
        .iter()
        .min_by_key(|color| (luminance(**color), **color))
        .ok_or(PaletteError)?;
    let foreground = contrasting_text(background);
    let selection_background = mix(background, foreground, 1, 3);
    let selection_foreground = contrasting_text(selection_background);

    let mut palette = [[0; 3]; 16];
    palette[0] = background;
    palette[7] = foreground;
    palette[8] = mix(background, foreground, 1, 3);
    palette[15] = foreground;
    for index in 0..6 {
        palette[index + 1] = mix(
            nearest(ANSI_NORMAL[index], &centers),
            ANSI_NORMAL[index],
            1,
            2,
        );
        palette[index + 9] = mix(
            nearest(ANSI_BRIGHT[index], &centers),
            ANSI_BRIGHT[index],
            1,
            2,
        );
    }

    Ok(ColorsManifest::new(
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
    .with_selection_foreground(color(selection_foreground)?))
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

fn relative_luminance(color: Rgb) -> f64 {
    let linear = |channel: u8| {
        let value = f64::from(channel) / 255.0;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(color[0]) + 0.7152 * linear(color[1]) + 0.0722 * linear(color[2])
}

fn luminance(color: Rgb) -> u32 {
    2_126 * u32::from(color[0]) + 7_152 * u32::from(color[1]) + 722 * u32::from(color[2])
}

fn color(value: Rgb) -> Result<Color, PaletteError> {
    Color::from_str(&format!("{:02x}{:02x}{:02x}", value[0], value[1], value[2]))
        .map_err(|_| PaletteError)
}
