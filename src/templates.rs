//! Procedural On-Demand 2D UV Template Generators (Decal, Banner, Stadium Pitch, Match Ball)
//! with built-in Bitmap Font Labels & Instant 2D Previews (100% Portable).

use crate::custom_studio::decode_texture2d_export;
use crate::icon_system::extract_ball_texture_preview;
use crate::upk_engine::{bak_path_for, detect_car_uv_label, get_appdata_dir, unpack_upk_from_disk, vault_backup_path};
use image::{imageops, imageops::FilterType, Rgba, RgbaImage};
use std::fs;
use std::path::{Path, PathBuf};

pub fn get_templates_dir() -> PathBuf {
    let p = get_appdata_dir().join("templates");
    let _ = fs::create_dir_all(&p);
    p
}

// ── Crisp Built-in 5x7 Pixel Font for Template Labels ────────────────────────

fn glyph_5x7(ch: char) -> [u8; 7] {
    match ch.to_ascii_uppercase() {
        'A' => [0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
        'B' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110],
        'C' => [0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110],
        'D' => [0b11100, 0b10010, 0b10001, 0b10001, 0b10001, 0b10010, 0b11100],
        'E' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111],
        'F' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000],
        'G' => [0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01110],
        'H' => [0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
        'I' => [0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
        'J' => [0b00111, 0b00010, 0b00010, 0b00010, 0b10010, 0b10010, 0b01100],
        'K' => [0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001],
        'L' => [0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111],
        'M' => [0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001],
        'N' => [0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001],
        'O' => [0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        'P' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000],
        'Q' => [0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101],
        'R' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001],
        'S' => [0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110],
        'T' => [0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100],
        'U' => [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        'V' => [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100],
        'W' => [0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b11011, 0b10001],
        'X' => [0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001],
        'Y' => [0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100],
        'Z' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111],
        '0' => [0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110],
        '1' => [0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
        '2' => [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111],
        '3' => [0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110],
        '4' => [0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010],
        '5' => [0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110],
        '6' => [0b01110, 0b10000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110],
        '7' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000],
        '8' => [0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110],
        '9' => [0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00001, 0b01110],
        '-' => [0b00000, 0b00000, 0b00000, 0b11111, 0b00000, 0b00000, 0b00000],
        '/' => [0b00001, 0b00010, 0b00010, 0b00100, 0b01000, 0b01000, 0b10000],
        ':' => [0b00000, 0b00100, 0b00100, 0b00000, 0b00100, 0b00100, 0b00000],
        '(' => [0b00010, 0b00100, 0b01000, 0b01000, 0b01000, 0b00100, 0b00010],
        ')' => [0b01000, 0b00100, 0b00010, 0b00010, 0b00010, 0b00100, 0b01000],
        '.' => [0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b01100, 0b01100],
        '_' => [0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b11111],
        '!' => [0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00000, 0b00100],
        '[' => [0b01110, 0b01000, 0b01000, 0b01000, 0b01000, 0b01000, 0b01110],
        ']' => [0b01110, 0b00010, 0b00010, 0b00010, 0b00010, 0b00010, 0b01110],
        _ => [0; 7],
    }
}

pub fn draw_text_5x7(img: &mut RgbaImage, text: &str, x0: u32, y0: u32, scale: u32, color: Rgba<u8>) {
    let mut cx = x0;
    for ch in text.chars() {
        let rows = glyph_5x7(ch);
        for (ry, &bits) in rows.iter().enumerate() {
            for rx in 0..5 {
                if (bits & (1 << (4 - rx))) != 0 {
                    for sy in 0..scale {
                        for sx in 0..scale {
                            let px = cx + (rx as u32) * scale + sx;
                            let py = y0 + (ry as u32) * scale + sy;
                            if px < img.width() && py < img.height() {
                                img.put_pixel(px, py, color);
                            }
                        }
                    }
                }
            }
        }
        cx += 6 * scale;
    }
}

fn blend_pixel_over(img: &mut RgbaImage, x: u32, y: u32, src: Rgba<u8>) {
    if src[3] == 0 {
        return;
    }
    if src[3] == 255 {
        img.put_pixel(x, y, src);
        return;
    }
    let dst = *img.get_pixel(x, y);
    let sa = src[3] as f32 / 255.0;
    let inv = 1.0 - sa;
    img.put_pixel(
        x,
        y,
        Rgba([
            (src[0] as f32 * sa + dst[0] as f32 * inv).round().clamp(0.0, 255.0) as u8,
            (src[1] as f32 * sa + dst[1] as f32 * inv).round().clamp(0.0, 255.0) as u8,
            (src[2] as f32 * sa + dst[2] as f32 * inv).round().clamp(0.0, 255.0) as u8,
            (src[3] as f32 + dst[3] as f32 * inv).round().clamp(0.0, 255.0) as u8,
        ]),
    );
}

fn draw_rect_box(
    img: &mut RgbaImage,
    x0: u32,
    y0: u32,
    w: u32,
    h: u32,
    fill: Rgba<u8>,
    border: Rgba<u8>,
    border_w: u32,
) {
    let x1 = x0.saturating_add(w).min(img.width());
    let y1 = y0.saturating_add(h).min(img.height());
    for y in y0..y1 {
        for x in x0..x1 {
            let is_b = border_w > 0
                && ((x - x0) < border_w
                    || (x1 - 1 - x) < border_w
                    || (y - y0) < border_w
                    || (y1 - 1 - y) < border_w);
            let px = if is_b { border } else { fill };
            blend_pixel_over(img, x, y, px);
        }
    }
}

// ── 1. Custom Decal UV Template Generator (1024x1024, Car-Specific 1:1 UV) ───

fn extract_real_car_uv_from_game(car_name: &str, cooked_dir: &Path) -> Option<RgbaImage> {
    let low = car_name.to_lowercase();
    let (pkg, primary_tex, mask_tex) = if low.contains("fennec") {
        ("body_grain_SF.upk", "Body_Grain_Curvature", Some("Body_Grain_D"))
    } else if low.contains("dominus") {
        ("Body_MuscleCar_SF.upk", "MuscleCar_Body_Curvature", Some("MuscleCar_BlankSkin"))
    } else {
        ("Body_Octane_SF.upk", "Body_Octane_Bevel_RGB", None)
    };

    let live_p = cooked_dir.join(pkg);
    let vault_p = vault_backup_path(&live_p);
    let bak_p = bak_path_for(&live_p);
    let upk_p = if vault_p.is_file() {
        vault_p
    } else if bak_p.is_file() {
        bak_p
    } else {
        live_p
    };
    if !upk_p.is_file() {
        return None;
    }
    let (upk, _) = unpack_upk_from_disk(&upk_p).ok()?;

    let find_and_decode = |needle: &str| -> Option<RgbaImage> {
        for (idx, exp) in upk.exports.iter().enumerate() {
            if upk.export_class_name(exp) != "Texture2D" {
                continue;
            }
            if upk.full_export_path(idx + 1).contains(needle) {
                if let Some(im) = decode_texture2d_export(&upk, idx, Some(cooked_dir)) {
                    return Some(imageops::resize(&im, 1024, 1024, FilterType::Triangle));
                }
            }
        }
        None
    };

    let edge_src = find_and_decode(primary_tex)?;
    let mask_src = mask_tex.and_then(|m| find_and_decode(m));

    let s = 1024u32;
    let mut out = RgbaImage::from_pixel(s, s, Rgba([12, 14, 20, 255]));
    for y in 1..(s - 1) {
        for x in 1..(s - 1) {
            // Subtle 64px UV coordinate grid
            if x % 64 == 0 || y % 64 == 0 {
                out.put_pixel(x, y, Rgba([22, 24, 34, 255]));
            }

            let l = edge_src.get_pixel(x - 1, y).0;
            let r = edge_src.get_pixel(x + 1, y).0;
            let u = edge_src.get_pixel(x, y - 1).0;
            let d = edge_src.get_pixel(x, y + 1).0;
            let c = edge_src.get_pixel(x, y).0;
            let grad = (l[0] as i32 - r[0] as i32).abs()
                + (l[1] as i32 - r[1] as i32).abs()
                + (l[2] as i32 - r[2] as i32).abs()
                + (u[0] as i32 - d[0] as i32).abs()
                + (u[1] as i32 - d[1] as i32).abs()
                + (u[2] as i32 - d[2] as i32).abs();

            let is_island = if let Some(ref mk) = mask_src {
                let mpx = mk.get_pixel(x, y).0;
                ((mpx[0] as u32 + mpx[1] as u32 + mpx[2] as u32) / 3) > 40
            } else {
                // Octane Bevel_RGB: bright interior panels (> 165) vs dull olive gutter
                ((c[0] as u32 + c[1] as u32 + c[2] as u32) / 3) > 160
            };

            let is_body_paint = if let Some(ref mk) = mask_src {
                mk.get_pixel(x, y).0[0] > 180
            } else {
                is_island
            };

            if is_body_paint {
                out.put_pixel(x, y, Rgba([26, 46, 86, 255]));
            } else if is_island {
                out.put_pixel(x, y, Rgba([22, 28, 38, 255]));
            }

            if grad > 55 {
                let intensity = ((grad as f32 - 55.0) / 140.0).clamp(0.35, 1.0);
                let col = if is_body_paint {
                    Rgba([
                        (56.0 + 180.0 * intensity) as u8,
                        (189.0 + 55.0 * intensity) as u8,
                        255,
                        255,
                    ])
                } else {
                    Rgba([90, 130, 175, (150.0 + 105.0 * intensity) as u8])
                };
                out.put_pixel(x, y, col);
            }
        }
    }

    draw_text_5x7(
        &mut out,
        &format!("{} UV (1024X1024)", car_name.to_uppercase()),
        14,
        12,
        2,
        Rgba([220, 235, 255, 240]),
    );
    Some(out)
}

fn generate_proportional_fallback_uv(car_name: &str) -> RgbaImage {
    let s = 1024u32;
    let mut img = RgbaImage::from_pixel(s, s, Rgba([12, 14, 20, 255]));
    for y in 0..s {
        for x in 0..s {
            if x % 64 == 0 || y % 64 == 0 {
                img.put_pixel(x, y, Rgba([22, 24, 34, 255]));
            }
        }
    }
    let low = car_name.to_lowercase();
    if low.contains("fennec") {
        draw_rect_box(&mut img, 790, 12, 224, 250, Rgba([26, 46, 86, 255]), Rgba([56, 189, 248, 255]), 3);
        draw_text_5x7(&mut img, "ROOF", 865, 125, 3, Rgba([200, 235, 255, 255]));
        draw_rect_box(&mut img, 550, 12, 224, 150, Rgba([26, 46, 86, 255]), Rgba([56, 189, 248, 255]), 3);
        draw_text_5x7(&mut img, "SPOILER", 590, 75, 2, Rgba([200, 235, 255, 255]));
        draw_rect_box(&mut img, 410, 276, 580, 725, Rgba([26, 46, 86, 255]), Rgba([56, 189, 248, 255]), 3);
        draw_text_5x7(&mut img, "BODY WRAP / HOOD / SIDES", 475, 620, 3, Rgba([200, 235, 255, 255]));
        draw_rect_box(&mut img, 20, 90, 370, 900, Rgba([22, 28, 38, 255]), Rgba([100, 135, 175, 255]), 2);
        draw_text_5x7(&mut img, "CHASSIS / TRIM", 100, 520, 2, Rgba([160, 180, 205, 255]));
    } else if low.contains("dominus") {
        draw_rect_box(&mut img, 20, 620, 720, 380, Rgba([26, 46, 86, 255]), Rgba([56, 189, 248, 255]), 3);
        draw_text_5x7(&mut img, "HOOD / BODY SIDES", 180, 800, 3, Rgba([200, 235, 255, 255]));
        draw_rect_box(&mut img, 340, 140, 310, 270, Rgba([26, 46, 86, 255]), Rgba([56, 189, 248, 255]), 3);
        draw_text_5x7(&mut img, "ROOF / TRUNK", 385, 265, 2, Rgba([200, 235, 255, 255]));
        draw_rect_box(&mut img, 575, 395, 315, 480, Rgba([22, 28, 38, 255]), Rgba([100, 135, 175, 255]), 2);
        draw_text_5x7(&mut img, "TRIM / CHASSIS", 625, 630, 2, Rgba([160, 180, 205, 255]));
    } else {
        // Octane exact 1:1 UV layout (Top-left wide roof/hood, lower-center body wrap & fenders, right skirts)
        draw_rect_box(&mut img, 12, 165, 540, 185, Rgba([26, 46, 86, 255]), Rgba([56, 189, 248, 255]), 3);
        draw_text_5x7(&mut img, "ROOF", 230, 245, 3, Rgba([200, 235, 255, 255]));
        draw_rect_box(&mut img, 145, 360, 490, 630, Rgba([26, 46, 86, 255]), Rgba([56, 189, 248, 255]), 3);
        draw_text_5x7(&mut img, "HOOD / FENDERS", 260, 660, 3, Rgba([200, 235, 255, 255]));
        draw_rect_box(&mut img, 560, 15, 290, 310, Rgba([26, 46, 86, 255]), Rgba([56, 189, 248, 255]), 3);
        draw_text_5x7(&mut img, "SPOILER / REAR", 595, 160, 2, Rgba([200, 235, 255, 255]));
        draw_rect_box(&mut img, 670, 340, 330, 650, Rgba([22, 28, 38, 255]), Rgba([100, 135, 175, 255]), 2);
        draw_text_5x7(&mut img, "CHASSIS / SKIRTS", 720, 650, 2, Rgba([160, 180, 205, 255]));
    }
    draw_text_5x7(&mut img, &format!("{} UV (1024X1024)", car_name.to_uppercase()), 14, 12, 2, Rgba([220, 235, 255, 240]));
    img
}

/// Returns the car-specific `1024x1024` 1:1 UV template for `Octane`, `Fennec`, or `Dominus`
/// (extracted directly from RL body curvature/bevel maps when `cooked_dir` is available & cached,
/// or accurate proportional fallback). Used identically for BOTH preview and PNG export!
pub fn generate_decal_template_for_car(car_name: &str, cooked_dir: Option<&Path>) -> RgbaImage {
    let key = match detect_car_uv_label(car_name) {
        Some("Fennec") => "fennec",
        Some("Dominus") => "dominus",
        _ => "octane",
    };
    let cache_p = get_templates_dir().join(format!("uv_real_v2_{}.png", key));
    if cache_p.is_file() {
        if let Ok(im) = image::open(&cache_p) {
            return im.to_rgba8();
        }
    }
    if let Some(cdir) = cooked_dir {
        if let Some(real_uv) = extract_real_car_uv_from_game(key, cdir) {
            let _ = real_uv.save(&cache_p);
            return real_uv;
        }
    }
    generate_proportional_fallback_uv(key)
}

pub fn generate_decal_template(car_name: &str) -> RgbaImage {
    generate_decal_template_for_car(car_name, None)
}

// ── 2. Custom Banner 14:3 Safe-Zone Template Generator (840x200) ─────────────

pub fn generate_banner_template() -> RgbaImage {
    let w = 840u32;
    let h = 200u32;
    let mut img = RgbaImage::from_pixel(w, h, Rgba([14, 14, 20, 255]));

    // Outer frame
    draw_rect_box(&mut img, 0, 0, w, h, Rgba([18, 18, 26, 255]), Rgba([139, 92, 246, 255]), 3);

    // Left Avatar Border Overlap Zone (0..185px)
    draw_rect_box(&mut img, 12, 12, 176, 176, Rgba([127, 29, 29, 130]), Rgba([239, 68, 68, 255]), 2);
    draw_text_5x7(&mut img, "AVATAR", 54, 78, 3, Rgba([252, 165, 165, 255]));
    draw_text_5x7(&mut img, "OVERLAP ZONE", 32, 108, 2, Rgba([254, 202, 202, 240]));

    // Center Player Name & Title Safe Zone (200..560px)
    draw_rect_box(&mut img, 204, 28, 360, 144, Rgba([6, 78, 59, 110]), Rgba([16, 185, 129, 255]), 2);
    draw_text_5x7(&mut img, "PLAYER NAME ZONE", 242, 68, 3, Rgba([167, 243, 208, 255]));
    draw_text_5x7(&mut img, "SAFE TITLE MARGIN", 268, 112, 2, Rgba([209, 250, 229, 230]));

    // Right Artwork Showcase Zone (578..828px)
    draw_rect_box(&mut img, 578, 12, 250, 176, Rgba([30, 58, 138, 120]), Rgba([59, 130, 246, 255]), 2);
    draw_text_5x7(&mut img, "MAIN ARTWORK", 602, 78, 3, Rgba([147, 197, 253, 255]));
    draw_text_5x7(&mut img, "14:3 (840X200)", 618, 112, 2, Rgba([219, 234, 254, 230]));

    img
}

// ── 3. Custom Match Ball UV Template Generator (512x512) ──────────────────────

/// Generates a 512x512 Match Ball UV Template built on top of the live extracted
/// `Ball_Default00_D` texture when `cooked_dir` is available (or clean metallic panel guide offline).
pub fn generate_ball_template(cooked_dir: Option<&Path>) -> RgbaImage {
    let s = 512u32;
    let mut img = cooked_dir
        .and_then(extract_ball_texture_preview)
        .map(|raw| imageops::resize(&raw, s, s, FilterType::Lanczos3))
        .unwrap_or_else(|| {
            let mut base = RgbaImage::from_pixel(s, s, Rgba([42, 46, 56, 255]));
            for y in 0..s {
                for x in 0..s {
                    let gx = ((x / 64) + (y / 64)) % 2 == 0;
                    let col = if gx { Rgba([58, 64, 78, 255]) } else { Rgba([44, 49, 60, 255]) };
                    base.put_pixel(x, y, col);
                }
            }
            base
        });

    // Translucent cyan panel boundary guide overlay
    for y in 0..s {
        for x in 0..s {
            if x % 128 < 2 || y % 128 < 2 {
                let p = img.get_pixel(x, y);
                let blend = Rgba([
                    ((p[0] as u16 + 56) / 2) as u8,
                    ((p[1] as u16 + 210) / 2) as u8,
                    ((p[2] as u16 + 255) / 2) as u8,
                    255,
                ]);
                img.put_pixel(x, y, blend);
            }
        }
    }
    draw_rect_box(&mut img, 16, 12, s - 32, 48, Rgba([10, 14, 24, 195]), Rgba([56, 189, 248, 255]), 2);
    draw_text_5x7(&mut img, "RL BALL DIFFUSE UV TEMPLATE (512X512 DXT1)", 26, 22, 2, Rgba([244, 244, 248, 255]));
    draw_text_5x7(&mut img, "PAINT OVER PANELS TO RESKIN STANDARD MATCH BALL", 26, 40, 1, Rgba([147, 197, 253, 255]));
    img
}

// ── 5. Instant 2D Previews (Top-Down Car Silhouette, Scoreboard Banner, Ball Orb)

/// Renders a crisp 2D top-down Car Chassis preview showing `primary_rgb` paint + `decal_img` UV overlay + `accent_rgb` trim.
/// Renders the exact 1:1 square UV sheet (`size x size`) matching `generate_decal_template_for_car`
/// pixel-for-pixel, compositing `decal_img` (or `primary_rgb` base fill inside body UV islands)
/// with crisp UV seam outlines preserved on top.
pub fn render_2d_car_decal_preview(
    car_model: &str,
    cooked_dir: Option<&Path>,
    decal_img: Option<&RgbaImage>,
    primary_rgb: [u8; 3],
    accent_rgb: [u8; 3],
    size: u32,
) -> RgbaImage {
    render_2d_car_decal_with_mask_preview(
        car_model,
        cooked_dir,
        decal_img,
        None,
        primary_rgb,
        accent_rgb,
        size,
    )
}

/// Renders the exact 1:1 square UV sheet (`size x size`) compositing optional AlphaConsole `Diffuse` (`decal_img`)
/// and `2_Diffuse_Skin_Mask` (`mask_img`) with car primary/accent colors and UV seam outlines preserved.
pub fn render_2d_car_decal_with_mask_preview(
    car_model: &str,
    cooked_dir: Option<&Path>,
    decal_img: Option<&RgbaImage>,
    mask_img: Option<&RgbaImage>,
    primary_rgb: [u8; 3],
    accent_rgb: [u8; 3],
    size: u32,
) -> RgbaImage {
    let size = size.max(32);
    let tpl = generate_decal_template_for_car(car_model, cooked_dir);
    let mut out = imageops::resize(&tpl, size, size, FilterType::Triangle);

    if decal_img.is_some() || mask_img.is_some() {
        let d_scaled = decal_img.map(|d| imageops::resize(d, size, size, FilterType::Lanczos3));
        let m_scaled = mask_img.map(|m| imageops::resize(m, size, size, FilterType::Lanczos3));
        let tpl_nearest = imageops::resize(&tpl, size, size, FilterType::Nearest);
        for y in 0..size {
            for x in 0..size {
                let tpx = out.get_pixel(x, y).0;
                let npx = tpl_nearest.get_pixel(x, y).0;
                // Ignore header label bar (top 30px of 1024px UV guide) when detecting UV seam outlines
                let in_label = y < (size * 30 / 1024).max(10);
                let is_seam = !in_label && npx[2] > 180 && npx[1] > 140 && npx[0] < 185;
                let is_body_fill = tpx[2] > 70 && tpx[0] < 45;
                let mut r = if is_body_fill { primary_rgb[0] as f32 } else { tpx[0] as f32 };
                let mut g = if is_body_fill { primary_rgb[1] as f32 } else { tpx[1] as f32 };
                let mut b = if is_body_fill { primary_rgb[2] as f32 } else { tpx[2] as f32 };
                if let Some(ref ms) = m_scaled {
                    let mp = ms.get_pixel(x, y).0;
                    let mask_amt = (mp[0].max(mp[1]).max(mp[2]) as f32 / 255.0) * (mp[3] as f32 / 255.0);
                    if mask_amt > 0.02 {
                        r = r * (1.0 - mask_amt) + accent_rgb[0] as f32 * mask_amt;
                        g = g * (1.0 - mask_amt) + accent_rgb[1] as f32 * mask_amt;
                        b = b * (1.0 - mask_amt) + accent_rgb[2] as f32 * mask_amt;
                    }
                }
                if let Some(ref ds) = d_scaled {
                    let dpx = ds.get_pixel(x, y).0;
                    let a = dpx[3] as f32 / 255.0;
                    if a > 0.01 {
                        r = r * (1.0 - a) + dpx[0] as f32 * a;
                        g = g * (1.0 - a) + dpx[1] as f32 * a;
                        b = b * (1.0 - a) + dpx[2] as f32 * a;
                    }
                }
                if is_seam {
                    r = r * 0.55 + 56.0 * 0.45;
                    g = g * 0.55 + 189.0 * 0.45;
                    b = b * 0.55 + 248.0 * 0.45;
                }
                out.put_pixel(x, y, Rgba([r as u8, g as u8, b as u8, 255]));
            }
        }
    }
    out
}

/// Optional helper to burn `player_name` into the banner image left-center nameplate zone.
pub fn bake_name_onto_banner(banner_img: &RgbaImage, player_name: &str) -> RgbaImage {
    let clean = player_name.trim();
    if clean.is_empty() {
        return banner_img.clone();
    }
    let mut scaled = imageops::resize(banner_img, 420, 100, FilterType::Lanczos3);
    let max_chars = clean.chars().take(18).collect::<String>();
    draw_rect_box(&mut scaled, 98, 34, 210, 32, Rgba([8, 10, 16, 175]), Rgba([0, 0, 0, 0]), 0);
    draw_text_5x7(&mut scaled, &max_chars, 106, 43, 2, Rgba([255, 255, 255, 255]));
    scaled
}

/// Renders a live 2D Rocket League Scoreboard Player Bar (`14:3` Banner + Left Avatar + Border + Custom Name).
pub fn render_2d_banner_scoreboard_preview(
    banner_img: Option<&RgbaImage>,
    pfp_img: Option<&RgbaImage>,
    player_name: &str,
    w: u32,
    h: u32,
) -> RgbaImage {
    let w = w.max(64);
    let h = h.max(32);
    let mut out = RgbaImage::from_pixel(w, h, Rgba([12, 13, 18, 255]));
    let bar_w = w.saturating_sub(24).max(24);
    let bar_h = (bar_w * 100 / 420).min(h.saturating_sub(16)).max(24);
    let bx = (w - bar_w) / 2;
    let by = (h - bar_h) / 2;

    if let Some(b_raw) = banner_img {
        let scaled = imageops::resize(b_raw, bar_w, bar_h, FilterType::Lanczos3);
        imageops::overlay(&mut out, &scaled, bx as i64, by as i64);
    } else {
        draw_rect_box(&mut out, bx, by, bar_w, bar_h, Rgba([24, 28, 42, 255]), Rgba([59, 130, 246, 255]), 2);
    }

    let av = (bar_h.saturating_sub(4)).clamp(16, bar_w.saturating_sub(8));
    if let Some(p_raw) = pfp_img {
        let p_scaled = imageops::resize(p_raw, av, av, FilterType::Lanczos3);
        imageops::overlay(&mut out, &p_scaled, (bx + 4) as i64, (by + 2) as i64);
    } else {
        draw_rect_box(&mut out, bx + 4, by + 2, av, av, Rgba([18, 20, 28, 255]), Rgba([226, 232, 240, 255]), 2);
    }

    let trimmed = player_name.trim();
    if !trimmed.is_empty() {
        let label: String = trimmed.chars().take(18).collect();
        let avail_pill = bar_w.saturating_sub(av + 16);
        if avail_pill >= 24 && bar_h >= 28 {
            let pill_w = (label.len() as u32 * 12 + 20).min(avail_pill).max(avail_pill.min(90));
            let pill_y = by + bar_h / 2 - 14;
            draw_rect_box(&mut out, bx + av + 12, pill_y, pill_w, 28, Rgba([8, 10, 16, 200]), Rgba([0, 0, 0, 0]), 0);
            draw_text_5x7(&mut out, &label, bx + av + 20, pill_y + 7, 2, Rgba([255, 255, 255, 245]));
        }
    }
    out
}

/// Computes the exact pixel crop rectangle `(x, y, w, h)` inside `(src_w, src_h)` for `target_aspect`,
/// `zoom` (`>= 1.0`), and normalized pan offsets (`pan_x`, `pan_y` in `[-1.0, 1.0]`).
pub fn compute_crop_rect(
    src_w: u32,
    src_h: u32,
    target_aspect: f32,
    zoom: f32,
    pan_x: f32,
    pan_y: f32,
) -> (u32, u32, u32, u32) {
    let sw = src_w.max(1);
    let sh = src_h.max(1);
    let aspect = target_aspect.max(0.05);
    let z = zoom.clamp(1.0, 8.0);
    let src_aspect = sw as f32 / sh as f32;
    let (base_w, base_h) = if src_aspect > aspect {
        (sh as f32 * aspect, sh as f32)
    } else {
        (sw as f32, sw as f32 / aspect)
    };
    let cw = (base_w / z).round().clamp(1.0, sw as f32) as u32;
    let ch = (base_h / z).round().clamp(1.0, sh as f32) as u32;
    let slack_x = sw.saturating_sub(cw);
    let slack_y = sh.saturating_sub(ch);
    let rx = ((pan_x.clamp(-1.0, 1.0) + 1.0) * 0.5 * slack_x as f32)
        .round()
        .clamp(0.0, slack_x as f32) as u32;
    let ry = ((pan_y.clamp(-1.0, 1.0) + 1.0) * 0.5 * slack_y as f32)
        .round()
        .clamp(0.0, slack_y as f32) as u32;
    (rx, ry, cw.max(1), ch.max(1))
}

/// Crops `src` according to `(target_aspect, zoom, pan_x, pan_y)` and resizes cleanly to `(out_w, out_h)`.
pub fn crop_and_resize_rgba(
    src: &RgbaImage,
    target_aspect: f32,
    zoom: f32,
    pan_x: f32,
    pan_y: f32,
    out_w: u32,
    out_h: u32,
) -> RgbaImage {
    let (rx, ry, rw, rh) = compute_crop_rect(src.width(), src.height(), target_aspect, zoom, pan_x, pan_y);
    let cropped = imageops::crop_imm(src, rx, ry, rw, rh).to_image();
    if rw == out_w && rh == out_h {
        cropped
    } else {
        imageops::resize(&cropped, out_w.max(1), out_h.max(1), FilterType::Lanczos3)
    }
}

/// Renders a spherical-lit 2D preview orb from a Match Ball texture (`Ball_Default00_D`).
pub fn render_2d_ball_orb_preview(ball_tex: &RgbaImage, size: u32, glow_rgb: Option<[u8; 3]>) -> RgbaImage {
    let mut out = RgbaImage::from_pixel(size, size, Rgba([10, 11, 15, 255]));
    let tw = ball_tex.width().max(1);
    let th = ball_tex.height().max(1);
    let r = size as f32 * 0.43;
    let cx = size as f32 * 0.5;
    let cy = size as f32 * 0.5;

    for y in 0..size {
        for x in 0..size {
            let dx = (x as f32 - cx) / r;
            let dy = (y as f32 - cy) / r;
            let d2 = dx * dx + dy * dy;
            if d2 <= 1.0 {
                let dz = (1.0 - d2).sqrt();
                let u = 0.5 + dx.atan2(dz) / std::f32::consts::PI * 0.5;
                let v = 0.5 + dy.asin() / std::f32::consts::PI;
                let tx = ((u.clamp(0.0, 0.999)) * tw as f32) as u32;
                let ty = ((v.clamp(0.0, 0.999)) * th as f32) as u32;
                let p = ball_tex.get_pixel(tx, ty).0;
                // Directional key light + fresnel rim
                let light = (0.35 + (dx * -0.4 + dy * -0.45 + dz * 0.78).max(0.0) * 0.85).min(1.2);
                let rim = (1.0 - dz).powf(2.6);
                let g_col = glow_rgb.unwrap_or([56, 189, 248]);
                let cr = (p[0] as f32 * light + g_col[0] as f32 * rim * 0.35).min(255.0) as u8;
                let cg = (p[1] as f32 * light + g_col[1] as f32 * rim * 0.35).min(255.0) as u8;
                let cb = (p[2] as f32 * light + g_col[2] as f32 * rim * 0.35).min(255.0) as u8;
                out.put_pixel(x, y, Rgba([cr, cg, cb, 255]));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bake_name_onto_banner_blends_without_punching_alpha_hole() {
        let base = RgbaImage::from_pixel(420, 100, Rgba([200, 100, 50, 255]));
        let baked = bake_name_onto_banner(&base, "PlayerOne");
        // Every pixel inside the nameplate box (98..308, 34..66) must remain opaque (alpha = 255)
        // and preserve blended warm background tint underneath the semi-transparent dark pill.
        let p = baked.get_pixel(100, 36).0;
        assert_eq!(p[3], 255, "Baked banner pixel alpha must remain 255, got {}", p[3]);
        assert!(p[0] > 40 && p[0] < 90, "Expected blended red channel behind dark scrim, got {:?}", p);
    }

    #[test]
    fn narrow_banner_scoreboard_preview_never_panics() {
        let out = render_2d_banner_scoreboard_preview(None, None, "VeryLongPlayerName123", 120, 60);
        assert_eq!(out.dimensions(), (120, 60));
    }

    #[test]
    fn crop_rect_and_resize_cover_1_1_and_14_3_accurately() {
        // Construct 1920x1080 source: left half red, right half green
        let src = RgbaImage::from_fn(1920, 1080, |x, _y| {
            if x < 960 { Rgba([220, 20, 20, 255]) } else { Rgba([20, 220, 40, 255]) }
        });
        assert_eq!(compute_crop_rect(1920, 1080, 1.0, 1.0, -1.0, 0.0), (0, 0, 1080, 1080));
        assert_eq!(compute_crop_rect(1920, 1080, 1.0, 1.0, 1.0, 0.0), (840, 0, 1080, 1080));
        assert_eq!(compute_crop_rect(1920, 1080, 4.2, 1.0, 0.0, 0.0), (0, 312, 1920, 457));

        let pfp_left = crop_and_resize_rgba(&src, 1.0, 2.0, -1.0, 0.0, 256, 256);
        assert_eq!(pfp_left.dimensions(), (256, 256));
        assert!(pfp_left.get_pixel(128, 128)[0] > 200);

        let ban_right = crop_and_resize_rgba(&src, 4.2, 2.0, 1.0, 0.0, 420, 100);
        assert_eq!(ban_right.dimensions(), (420, 100));
        assert!(ban_right.get_pixel(210, 50)[1] > 200);
    }
}
