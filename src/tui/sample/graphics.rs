use crate::domain::{EnvironmentManifest, WallpaperFit, WallpaperManifest, WallpaperPosition};
use image::{Rgb, RgbImage, RgbaImage};
use std::sync::OnceLock;

pub(super) struct LinearBlend {
    background: [u8; 3],
    linear_background: [f64; 3],
    opacity: f64,
    decode: &'static [f64; 256],
}

impl LinearBlend {
    pub(super) fn new(background: [u8; 3], opacity: u64) -> Self {
        static DECODE: OnceLock<[f64; 256]> = OnceLock::new();
        let decode = DECODE.get_or_init(|| {
            std::array::from_fn(|channel| {
                let c = channel as f64 / 255.0;
                if c <= 0.04045 {
                    c / 12.92
                } else {
                    ((c + 0.055) / 1.055).powf(2.4)
                }
            })
        });
        Self {
            background,
            linear_background: background.map(|c| decode[usize::from(c)]),
            opacity: opacity as f64 / 1_000_000.0,
            decode,
        }
    }

    pub(super) fn apply(&self, pixel: [u8; 4]) -> [u8; 3] {
        let alpha = self.opacity * f64::from(pixel[3]) / 255.0;
        if alpha == 0.0 {
            return self.background;
        }
        if alpha == 1.0 {
            return [pixel[0], pixel[1], pixel[2]];
        }
        // Ghostty's linear/linear-corrected modes blend light, not encoded sRGB bytes.
        // Decode both colors, use the unchanged opacity, then re-encode the result.
        std::array::from_fn(|i| {
            let linear = self.linear_background[i] * (1.0 - alpha)
                + self.decode[usize::from(pixel[i])] * alpha;
            let srgb = if linear <= 0.0031308 {
                linear * 12.92
            } else {
                1.055 * linear.powf(1.0 / 2.4) - 0.055
            };
            (srgb * 255.0).round().clamp(0.0, 255.0) as u8
        })
    }
}

// The sample is a synthetic terminal, not the user's effective Ghostty window.
pub(super) fn compose(
    manifest: &EnvironmentManifest,
    source: &RgbaImage,
    width: u32,
    height: u32,
) -> RgbImage {
    let background = manifest
        .colors()
        .map_or([32, 32, 32], |c| c.background().as_rgb());
    let Some(WallpaperManifest::Image(settings)) = manifest.wallpaper() else {
        return RgbImage::from_pixel(width, height, Rgb(background));
    };
    let sw = f64::from(source.width());
    let sh = f64::from(source.height());
    let (w, h) = (f64::from(width), f64::from(height));
    let (iw, ih) = match settings.fit().unwrap_or(WallpaperFit::Cover) {
        WallpaperFit::Stretch => (w, h),
        WallpaperFit::None => (sw, sh),
        fit => {
            let scale = if fit == WallpaperFit::Contain {
                (w / sw).min(h / sh)
            } else {
                (w / sw).max(h / sh)
            };
            (sw * scale, sh * scale)
        }
    };
    use WallpaperPosition::*;
    let position = settings.position().unwrap_or(Center);
    let ox = match position {
        TopLeft | CenterLeft | BottomLeft => 0.0,
        TopRight | CenterRight | BottomRight => w - iw,
        _ => (w - iw) / 2.0,
    };
    let oy = match position {
        TopLeft | TopCenter | TopRight => 0.0,
        BottomLeft | BottomCenter | BottomRight => h - ih,
        _ => (h - ih) / 2.0,
    };
    let opacity = settings.opacity().map_or(1_000_000, |v| u64::from(v.get()));
    let blend = LinearBlend::new(background, opacity);
    RgbImage::from_fn(width, height, |x, y| {
        let (mut x, mut y) = (f64::from(x) - ox, f64::from(y) - oy);
        if settings.repeat().unwrap_or(false) {
            x = x.rem_euclid(iw);
            y = y.rem_euclid(ih);
        }
        if x < 0.0 || y < 0.0 || x >= iw || y >= ih {
            return Rgb(background);
        }
        let pixel = source
            .get_pixel(
                ((x / iw * sw) as u32).min(source.width() - 1),
                ((y / ih * sh) as u32).min(source.height() - 1),
            )
            .0;
        Rgb(blend.apply(pixel))
    })
}
