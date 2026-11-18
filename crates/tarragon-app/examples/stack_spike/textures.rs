//! Procedural textures for the spike.

use eframe::egui::ColorImage;

/// Side of one texture in pixels.
const SIDE: usize = 256;

/// Build one procedural texture. Each has its own hue and a stripe pattern.
#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
pub fn make_image(index: usize) -> ColorImage {
    let hue = (index as f32 * 0.618_034).fract();
    let (r, g, b) = palette(hue);
    let mut rgba = Vec::with_capacity(SIDE * SIDE * 4);
    for y in 0..SIDE {
        for x in 0..SIDE {
            let stripe = (((x + y + index * 7) / 16) % 2) as f32;
            let shade = 0.6 + 0.4 * stripe;
            let fade = 0.5 + 0.5 * (y as f32 / SIDE as f32);
            rgba.extend_from_slice(&[
                (f32::from(r) * shade * fade) as u8,
                (f32::from(g) * shade * fade) as u8,
                (f32::from(b) * shade * fade) as u8,
                255,
            ]);
        }
    }
    ColorImage::from_rgba_unmultiplied([SIDE, SIDE], &rgba)
}

/// Map a hue in 0..1 to a bright 8-bit red, green, and blue triple.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn palette(hue: f32) -> (u8, u8, u8) {
    let channel = |shift: f32| {
        let wave = (std::f32::consts::TAU * (hue + shift)).cos();
        (150.0 + 100.0 * wave) as u8
    };
    (channel(0.0), channel(1.0 / 3.0), channel(2.0 / 3.0))
}
