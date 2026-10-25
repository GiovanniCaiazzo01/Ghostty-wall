//! Versioned image-space derivation. Never mutates Source or captured Environment bytes.
use std::io::Cursor;

use crate::domain::WallpaperEffects;

pub(crate) fn render(
    bytes: &[u8],
    effects: WallpaperEffects,
    preview: bool,
) -> Result<Vec<u8>, ()> {
    if effects.blur > 32 || effects.darken > 100 {
        return Err(());
    }
    if !preview && effects.blur == 0 && effects.darken == 0 {
        return Ok(bytes.to_vec());
    }
    let format = image::guess_format(bytes).map_err(|_| ())?;
    if !matches!(format, image::ImageFormat::Png | image::ImageFormat::Jpeg) {
        return Err(());
    }
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16_384);
    limits.max_image_height = Some(16_384);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    let mut image = reader.decode().map_err(|_| ())?;
    if u64::from(image.width()) * u64::from(image.height()) > 16_777_216 {
        return Err(());
    }
    if preview && (image.width() > 384 || image.height() > 256) {
        image = image.thumbnail(384, 256);
    }
    let sigma = f32::from(effects.blur) * image.width().min(image.height()) as f32 / 512.0;
    if sigma > 0.0 {
        // Float premultiplication avoids hidden RGB bleed and low-alpha 8-bit amplification.
        let mut pixels = image.to_rgba32f();
        for p in pixels.pixels_mut() {
            for c in 0..3 {
                p[c] *= p[3];
            }
        }
        pixels = image::imageops::blur(&pixels, sigma);
        for p in pixels.pixels_mut() {
            for c in 0..3 {
                p[c] = if p[3] <= 0.0 { 0.0 } else { p[c] / p[3] };
            }
        }
        image = image::DynamicImage::ImageRgba32F(pixels);
    }
    let mut image = image.to_rgba8();
    if effects.darken > 0 {
        let table: [u8; 256] = std::array::from_fn(|c| {
            let s = c as f64 / 255.0;
            let linear = if s <= 0.04045 {
                s / 12.92
            } else {
                ((s + 0.055) / 1.055).powf(2.4)
            };
            let v = linear * (1.0 - f64::from(effects.darken) / 100.0);
            let s = if v <= 0.0031308 {
                12.92 * v
            } else {
                1.055 * v.powf(1.0 / 2.4) - 0.055
            };
            (s * 255.0).round() as u8
        });
        for p in image.pixels_mut() {
            for c in 0..3 {
                p[c] = table[usize::from(p[c])];
            }
        }
    }
    let mut output = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut output, image::ImageFormat::Png)
        .map_err(|_| ())?;
    if output.get_ref().len() > 32 * 1024 * 1024 {
        return Err(());
    }
    Ok(output.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quantized_two_dimensional_rgba_effects_obey_automatic_text_bound() {
        let image = image::RgbaImage::from_fn(64, 64, |x, y| {
            let rgb = [[255, 0, 0], [0, 255, 0], [0, 0, 255], [255; 3], [0; 3]]
                [((x / 8 + y / 8) % 5) as usize];
            image::Rgba([
                rgb[0],
                rgb[1],
                rgb[2],
                [0, 1, 64, 128, 255][((x + y) % 5) as usize],
            ])
        });
        let mut bytes = Cursor::new(Vec::new());
        image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
        let palette = crate::palette::analyze_oklch(bytes.get_ref()).unwrap();
        for opacity in [50_000, 650_000, 1_000_000] {
            let darken = palette.minimum_darkening(opacity);
            let colors = palette.for_effects(opacity, darken);
            let base = crate::palette::relative_luminance(colors.background().as_rgb());
            let text = std::iter::once(colors.foreground())
                .chain(colors.palette().iter().copied())
                .chain(colors.cursor())
                .map(|c| crate::palette::relative_luminance(c.as_rgb()))
                .fold(1.0, f64::min);
            for blur in [0, 8, 32] {
                let output =
                    render(bytes.get_ref(), WallpaperEffects { blur, darken }, false).unwrap();
                let rendered = image::load_from_memory(&output).unwrap().to_rgba8();
                for p in rendered.pixels() {
                    let alpha = f64::from(opacity) / 1_000_000.0 * f64::from(p[3]) / 255.0;
                    let bg = base * (1.0 - alpha)
                        + crate::palette::relative_luminance([p[0], p[1], p[2]]) * alpha;
                    assert!(
                        (text + 0.05) / (bg + 0.05) >= 4.5,
                        "blur={blur}, darken={darken}, opacity={opacity}, pixel={p:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn zero_identity_blur_repeatability_and_linear_darkening() {
        let image = image::RgbImage::from_fn(64, 64, |x, _| {
            image::Rgb(if x < 32 { [0; 3] } else { [255; 3] })
        });
        let mut bytes = Cursor::new(Vec::new());
        image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
        let bytes = bytes.into_inner();
        let zero = WallpaperEffects { blur: 0, darken: 0 };
        assert_eq!(render(&bytes, zero, false).unwrap(), bytes);
        let blur = WallpaperEffects { blur: 16, ..zero };
        let changed = render(&bytes, blur, false).unwrap();
        assert_ne!(changed, bytes);
        assert_eq!(changed, render(&bytes, blur, false).unwrap());
        let decoded = image::load_from_memory(&changed).unwrap().to_rgb8();
        assert!(decoded.get_pixel(31, 32)[0] > 0);
        assert!(decoded.get_pixel(32, 32)[0] < 255);
        let dark = render(&bytes, WallpaperEffects { darken: 50, ..zero }, false).unwrap();
        assert_eq!(
            image::load_from_memory(&dark)
                .unwrap()
                .to_rgb8()
                .get_pixel(40, 32)[0],
            188
        );
        assert!(render(&bytes, WallpaperEffects { blur: 33, ..zero }, false).is_err());
        assert!(render(b"not an image", blur, false).is_err());
    }
}
