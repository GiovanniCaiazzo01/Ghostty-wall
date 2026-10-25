//! Independently implemented OKLab matrices and fixed-hue sRGB gamut mapping.
//! No CM-Colors or APCA code. Contrast is WCAG relative luminance after 8-bit quantization.
use super::*;
use std::f64::consts::TAU;

fn lch(rgb: Rgb) -> [f64; 3] {
    let [r, g, b] = rgb.map(|v| {
        let s = f64::from(v) / 255.0;
        if s <= 0.04045 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    });
    let l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
    let m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
    let s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();
    let light = 0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s;
    let a = 1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s;
    let b = 0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s;
    [light, a.hypot(b), b.atan2(a).rem_euclid(TAU)]
}

fn linear([light, chroma, hue]: [f64; 3]) -> [f64; 3] {
    let (a, b) = (chroma * hue.cos(), chroma * hue.sin());
    let l = (light + 0.3963377774 * a + 0.2158037573 * b).powi(3);
    let m = (light - 0.1055613458 * a - 0.0638541728 * b).powi(3);
    let s = (light - 0.0894841775 * a - 1.2914855480 * b).powi(3);
    [
        4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
        -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
        -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s,
    ]
}

fn gamut(mut value: [f64; 3]) -> Rgb {
    let in_gamut = |v: [f64; 3]| v.into_iter().all(|c| (0.0..=1.0).contains(&c));
    if !in_gamut(linear(value)) {
        let (mut low, mut high) = (0.0, value[1]);
        for _ in 0..24 {
            let middle = (low + high) / 2.0;
            if in_gamut(linear([value[0], middle, value[2]])) {
                low = middle;
            } else {
                high = middle;
            }
        }
        value[1] = low;
    }
    linear(value).map(|v| {
        let v = v.clamp(0.0, 1.0);
        let s = if v <= 0.0031308 {
            v * 12.92
        } else {
            1.055 * v.powf(1.0 / 2.4) - 0.055
        };
        (s * 255.0).round() as u8
    })
}

pub(super) fn roles(source: Color) -> ColorsManifest {
    let [_, chroma, hue] = lch(source.as_rgb());
    let hue = if chroma < 0.005 {
        250_f64.to_radians()
    } else {
        hue
    };
    let make = |l, c, h| color(gamut([l, c, h])).expect("quantized RGB");
    let background = make(0.16, 0.018, hue);
    let foreground = make(0.9, 0.018, hue);
    let mut palette = [foreground; 16];
    palette[0] = make(0.68, 0.012, hue);
    palette[7] = make(0.85, 0.018, hue);
    palette[8] = make(0.76, 0.012, hue);
    palette[15] = make(0.96, 0.01, hue);
    // ANSI semantic hues stay recognizable; wallpaper adds a small shared hue offset.
    let offset = (hue.sin() * 5.0).to_radians();
    for (index, degrees) in [25_f64, 145.0, 95.0, 260.0, 320.0, 205.0]
        .into_iter()
        .enumerate()
    {
        palette[index + 1] = make(0.73, 0.13, degrees.to_radians() + offset);
        palette[index + 9] = make(0.85, 0.11, degrees.to_radians() + offset);
    }
    ColorsManifest::new(background, foreground, palette)
        .with_cursor(make(0.9, 0.07, hue))
        .with_selection_background(make(0.34, 0.035, hue))
        .with_selection_foreground(make(0.96, 0.01, hue))
}

fn text_roles(colors: &ColorsManifest) -> impl Iterator<Item = Color> + '_ {
    std::iter::once(colors.foreground())
        .chain(colors.palette().iter().copied())
        .chain(colors.cursor())
}

pub(super) fn minimum_darkening(palette: &GeneratedPalette, opacity: u32) -> u8 {
    let a = f64::from(opacity.min(1_000_000)) / 1_000_000.0;
    let image = a * (palette.source_max + 0.0045).min(1.0);
    if image == 0.0 {
        return 0;
    }
    let base = relative_luminance(palette.colors.background().as_rgb()) * (1.0 - a);
    let limit = text_roles(&palette.colors)
        .map(|c| (relative_luminance(c.as_rgb()) + 0.05) / 4.5 - 0.05)
        .fold(1.0, f64::min);
    // Reserve the same worst-case 8-bit darkening-rounding bound used by resolve.
    ((1.0 - (limit - base - a * 0.0045) / image) * 100.0)
        .ceil()
        .clamp(0.0, 100.0) as u8
}

pub(super) fn brightest(palette: &GeneratedPalette, opacity: u32, darken: u8) -> f64 {
    let a = f64::from(opacity.min(1_000_000)) / 1_000_000.0;
    // Whole-image bound includes alpha-hidden and cropped highlights. Deliberately conservative.
    // Bound half-code sRGB rounding after float blur, then after linear darkening.
    let max = (palette.source_max + 0.0045).min(1.0) * (1.0 - f64::from(darken) / 100.0);
    let rounding = if darken == 0 || darken == 100 {
        0.0
    } else {
        0.0045
    };
    let base = relative_luminance(palette.colors.background().as_rgb());
    (base * (1.0 - a) + a * (max + rounding).min(1.0)).max(base)
}

pub(super) fn resolve(palette: &GeneratedPalette, opacity: u32, darken: u8) -> ColorsManifest {
    let brightest = brightest(palette, opacity, darken);
    let correct = |value: Color| {
        let passes = |rgb| (relative_luminance(rgb) + 0.05) / (brightest + 0.05) >= 4.5;
        if passes(value.as_rgb()) {
            return value;
        }
        let [light, chroma, hue] = lch(value.as_rgb());
        // Bounded quantized lightness search: hue fixed, chroma only reduced for gamut.
        // If even white fails, return white; the sample reports the remaining limitation.
        for step in 1..=256 {
            let rgb = gamut([light + (1.0 - light) * f64::from(step) / 256.0, chroma, hue]);
            if passes(rgb) {
                return color(rgb).expect("quantized RGB");
            }
        }
        color([255; 3]).expect("white")
    };
    ColorsManifest::new(
        palette.colors.background(),
        correct(palette.colors.foreground()),
        palette.colors.palette().map(correct),
    )
    .with_cursor(correct(palette.colors.cursor().expect("cursor")))
    .with_selection_background(palette.colors.selection_background().expect("selection"))
    .with_selection_foreground(
        palette
            .colors
            .selection_foreground()
            .expect("selection text"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reference_vectors_gamut_roles_and_quantized_contract() {
        let red = lch([255, 0, 0]);
        assert!((red[0] - 0.62795536).abs() < 1e-7);
        assert!((red[1] - 0.2576833).abs() < 1e-6);
        for rgb in [
            [255, 0, 0],
            [0, 255, 0],
            [0, 0, 255],
            [0; 3],
            [128; 3],
            [255; 3],
        ] {
            let roundtrip = gamut(lch(rgb));
            for (a, b) in rgb.into_iter().zip(roundtrip) {
                assert!(a.abs_diff(b) <= 1);
            }
            let colors = roles(color(rgb).unwrap());
            let palette = GeneratedPalette {
                colors: colors.clone(),
                brightest: 1.0,
                source_max: 1.0,
                role_aware: true,
            };
            for opacity in [0, 50_000, 100_000, 500_000, 1_000_000] {
                let darken = minimum_darkening(&palette, opacity);
                let resolved = resolve(&palette, opacity, darken);
                let a = f64::from(opacity) / 1_000_000.0;
                let bg = relative_luminance(resolved.background().as_rgb()) * (1.0 - a)
                    + a * (1.0 - f64::from(darken) / 100.0
                        + if darken == 0 || darken == 100 {
                            0.0
                        } else {
                            0.0045
                        })
                    .min(1.0);
                for text in text_roles(&resolved) {
                    assert!((relative_luminance(text.as_rgb()) + 0.05) / (bg + 0.05) >= 4.5);
                }
                assert!(
                    contrast(
                        resolved.selection_background().unwrap().as_rgb(),
                        resolved.selection_foreground().unwrap().as_rgb()
                    ) >= 4.5
                );
                let distinct: std::collections::BTreeSet<_> =
                    resolved.palette().iter().map(ToString::to_string).collect();
                assert_eq!(distinct.len(), 16);
            }
        }
    }
}
