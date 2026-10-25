//! One-time composition-v1 renderer. Saved bytes, never this recipe, drive replay.
use image::{Rgb, RgbImage};
use sha2::{Digest, Sha256};

use crate::domain::Sha256Digest;

pub(super) const WIDTH: u32 = 768;
pub(super) const HEIGHT: u32 = 512;

// SplitMix64: fixed integer sequence, independent of platform and dependency versions.
struct Random(u64);
impl Random {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        z ^ (z >> 31)
    }
    fn range(&mut self, n: u32) -> u32 {
        (self.next() % u64::from(n)) as u32
    }
}

fn blend(a: [u8; 3], b: [u8; 3], amount: u32) -> [u8; 3] {
    std::array::from_fn(|i| {
        ((u32::from(a[i]) * (1024 - amount) + u32::from(b[i]) * amount + 512) / 1024) as u8
    })
}

pub(super) fn render(seed: Sha256Digest) -> RgbImage {
    let hash = Sha256::digest(seed.as_bytes());
    let mut random = Random(u64::from_le_bytes(
        hash[..8].try_into().expect("eight bytes"),
    ));
    // Rotating a six-stop wheel gives related colors, without wrapped RGB gradient seams.
    let wheel = [
        [230, 105, 91],
        [232, 180, 98],
        [112, 177, 140],
        [86, 155, 186],
        [126, 119, 184],
        [198, 122, 166],
    ];
    let start = random.range(6) as usize;
    let spread = 1 + random.range(3) as usize;
    let tint = [
        random.range(100) as u8,
        random.range(100) as u8,
        random.range(100) as u8,
    ];
    let colors: [[u8; 3]; 5] = std::array::from_fn(|i| {
        blend(
            wheel[(start + i * spread) % 6],
            tint,
            80 + random.range(260),
        )
    });
    let family = random.range(3);
    let columns = 3 + random.range(6);
    let rows = 2 + random.range(4);
    let tiles: Vec<_> = (0..columns * rows)
        .map(|_| random.range(5) as usize)
        .collect();
    let slant = random.range(700) as i32 - 350;
    let offset = random.range(512) as i32;
    let scale = 90 + random.range(200);
    let centers: Vec<_> = (0..3)
        .map(|_| {
            (
                random.range(WIDTH) as i32,
                random.range(HEIGHT) as i32,
                140 + random.range(320) as i32,
                random.range(5) as usize,
            )
        })
        .collect();
    RgbImage::from_fn(WIDTH, HEIGHT, |x, y| {
        let mut color = match family {
            // Staggered, variable-scale tiles with directional shading.
            0 => {
                let row = y * rows / HEIGHT;
                let column = ((x + (row % 2) * WIDTH / (columns * 2)) * columns / WIDTH) % columns;
                let index = tiles[(row * columns + column) as usize];
                blend(
                    colors[index],
                    colors[(index + 1) % 5],
                    (x * 180 / WIDTH + y * 80 / HEIGHT).min(1024),
                )
            }
            // Broad oblique ribbons; seed changes direction, width, phase, palette.
            1 => {
                let coordinate = y as i32 + (x as i32 * slant / WIDTH as i32) + offset + 400;
                let band = coordinate as u32 / scale;
                let fraction = coordinate as u32 % scale * 1024 / scale;
                blend(
                    colors[band as usize % 5],
                    colors[(band as usize + 1) % 5],
                    fraction / 3,
                )
            }
            // Off-center soft discs over a two-axis field.
            _ => blend(
                colors[0],
                colors[1],
                (x * 650 / WIDTH + y * 374 / HEIGHT).min(1024),
            ),
        };
        if family == 2 {
            for &(cx, cy, radius, index) in &centers {
                let dx = x as i32 - cx;
                let dy = y as i32 - cy;
                let distance = dx * dx + dy * dy;
                let edge = radius * radius;
                if distance < edge {
                    let amount = ((edge - distance) as u32 * 1024 / edge as u32).min(1024);
                    color = blend(color, colors[index], amount);
                }
            }
        }
        Rgb(color)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeatable_varied_palettes_and_geometry() {
        let images: Vec<_> = (0..9)
            .map(|i| render(Sha256Digest::from_bytes([i; 32])))
            .collect();
        assert_eq!(images[0], render(Sha256Digest::from_bytes([0; 32])));
        assert_eq!(
            Sha256Digest::from_bytes(Sha256::digest(images[0].as_raw()).into()).to_string(),
            "2c1033a755163707ca200a12ab36af7e95ee3c428b8298cf62c71f8b99c1c47d",
            "composition-v1 pixels must not change without a new algorithm identifier"
        );
        let mut last_byte = [0; 32];
        last_byte[31] = 1;
        assert_ne!(images[0], render(Sha256Digest::from_bytes(last_byte)));
        for (i, image) in images.iter().enumerate() {
            assert_eq!(image.dimensions(), (WIDTH, HEIGHT));
            for other in &images[..i] {
                assert_ne!(image, other);
            }
        }
        // Detect genuinely different geometry, not just a change of global tint.
        let edges: std::collections::BTreeSet<Vec<bool>> = images
            .iter()
            .map(|im| {
                (1..WIDTH)
                    .step_by(7)
                    .map(|x| {
                        let a = im.get_pixel(x - 1, HEIGHT / 2).0;
                        let b = im.get_pixel(x, HEIGHT / 2).0;
                        a.iter().zip(b).any(|(&a, b)| a.abs_diff(b) > 4)
                    })
                    .collect()
            })
            .collect();
        assert!(edges.len() >= 3);
    }
}
