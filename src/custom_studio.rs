//! Custom Decal (incl. AlphaConsole .ZIP/.JSON + Masks), Custom Ball (Diffuse + Normal),
//! Custom Profile Picture (PFP), Custom Player Banner & Item Tint Injection Engines.

use crate::icon_system::{
    decode_bgra8, decode_dxt1, decode_dxt5, encode_bgra8, encode_dxt1, encode_dxt5,
};
use crate::upk_engine::{
    bak_path_for, build_surgically_rewired_upk_with_guid, clear_rl_texture_cache,
    decompress_chunk_payload, decrypt_upk_header, ensure_pristine_before_swap,
    extract_package_guid, extract_target_fnames_from_upk, find_export_properties,
    load_catalog_items, parse_property_stream, parse_upk_prefix, read_i32_le, read_i64_le,
    read_u32_le, rebuild_upk_selective_chunks, rebuild_upk_with_mod_resized_added_and_names_to_vec,
    unpack_upk_from_disk, unpack_upk_from_raw, vault_backup_path, write_i32_le, CatalogItem,
    PaintColorSpec, UnpackedUpk, PACKAGE_FILE_TAG,
};
use image::{imageops::FilterType, DynamicImage, Rgba, RgbaImage};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// Discovers all `AvatarBorder_*_SF.upk` (and obfuscated catalog border) packages in `cooked_dir`,
/// using the official catalog product names so names match the Avatar Border swapper section 1:1.
pub fn list_avatar_border_packages_with_catalog(cooked_dir: &Path, catalog: &[CatalogItem]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(cooked_dir) else {
        return out;
    };
    let mut disk_map: HashMap<String, String> = HashMap::new();
    for entry in entries.flatten() {
        let nm = entry.file_name().to_string_lossy().to_string();
        let low = nm.to_lowercase();
        if low.ends_with(".upk") {
            disk_map.insert(low, nm);
        }
    }

    let mut seen_pkgs: HashSet<String> = HashSet::new();
    for it in catalog {
        if !(it.slot.eq_ignore_ascii_case("Avatar Border") || it.slot.eq_ignore_ascii_case("PlayerAvatarBorder")) {
            continue;
        }
        let low_pkg = it.asset_package.to_lowercase();
        if low_pkg.is_empty() || low_pkg.ends_with("_t_sf.upk") {
            continue;
        }
        if let Some(real_nm) = disk_map.get(&low_pkg) {
            if seen_pkgs.insert(low_pkg) {
                let lbl = if it.product.trim().is_empty() {
                    real_nm.clone()
                } else {
                    it.product.trim().to_string()
                };
                out.push((lbl, real_nm.clone()));
            }
        }
    }

    for (low, nm) in &disk_map {
        if low.starts_with("avatarborder_") && low.ends_with("_sf.upk") && !low.ends_with("_t_sf.upk") {
            if seen_pkgs.insert(low.clone()) {
                let label = nm
                    .strip_prefix("AvatarBorder_")
                    .or_else(|| nm.strip_prefix("avatarborder_"))
                    .unwrap_or(nm)
                    .strip_suffix("_SF.upk")
                    .or_else(|| nm.strip_suffix("_sf.upk"))
                    .unwrap_or(nm)
                    .replace('_', " ");
                out.push((label, nm.clone()));
            }
        }
    }

    out.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
    // Keep Default at the top for quick selection
    if let Some(pos) = out.iter().position(|(_, pkg)| pkg.to_lowercase() == "avatarborder_default_sf.upk") {
        let def = out.remove(pos);
        out.insert(0, def);
    }
    out
}

/// Discovers all 177+ Avatar Border packages in `cooked_dir` using catalog display names.
pub fn list_avatar_border_packages(cooked_dir: &Path) -> Vec<(String, String)> {
    let catalog = load_catalog_items(Some(cooked_dir));
    list_avatar_border_packages_with_catalog(cooked_dir, &catalog)
}

/// Discovers all Player Banner packages in `cooked_dir`, matching `Player Banner` catalog names 1:1.
pub fn list_player_banner_packages_with_catalog(cooked_dir: &Path, catalog: &[CatalogItem]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(cooked_dir) else {
        return out;
    };
    let mut disk_map: HashMap<String, String> = HashMap::new();
    for entry in entries.flatten() {
        let nm = entry.file_name().to_string_lossy().to_string();
        let low = nm.to_lowercase();
        if low.ends_with(".upk") {
            disk_map.insert(low, nm);
        }
    }

    let mut seen_pkgs: HashSet<String> = HashSet::new();
    for it in catalog {
        if !it.slot.eq_ignore_ascii_case("Player Banner") {
            continue;
        }
        let low_pkg = it.asset_package.to_lowercase();
        if low_pkg.is_empty() || low_pkg.ends_with("_t_sf.upk") {
            continue;
        }
        if let Some(real_nm) = disk_map.get(&low_pkg) {
            if seen_pkgs.insert(low_pkg) {
                let lbl = if it.product.trim().is_empty() {
                    real_nm.clone()
                } else {
                    it.product.trim().to_string()
                };
                out.push((lbl, real_nm.clone()));
            }
        }
    }

    for (low, nm) in &disk_map {
        if (low.starts_with("playerbanner_") || low.starts_with("banner_"))
            && low.ends_with("_sf.upk")
            && !low.ends_with("_t_sf.upk")
        {
            if seen_pkgs.insert(low.clone()) {
                let prefix_len = if low.starts_with("playerbanner_") { 13 } else { 7 };
                let clean = &nm[prefix_len..nm.len() - 7];
                let label = clean.replace('_', " ");
                out.push((label, nm.clone()));
            }
        }
    }
    out.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
    out
}

/// Discovers all 730+ `PlayerBanner_*_SF.upk` and `Banner_*_SF.upk` packages in `cooked_dir`.
pub fn list_player_banner_packages(cooked_dir: &Path) -> Vec<(String, String)> {
    let catalog = load_catalog_items(Some(cooked_dir));
    list_player_banner_packages_with_catalog(cooked_dir, &catalog)
}

// ── 1. Custom Profile Picture Injector & Composite Border Preview ──────────

fn render_pfp_into_border_frame(border_img: &RgbaImage, user_img: Option<&RgbaImage>) -> RgbaImage {
    use std::collections::VecDeque;

    let w = border_img.width() as usize;
    let h = border_img.height() as usize;
    if w == 0 || h == 0 {
        return border_img.clone();
    }
    let mut canvas = RgbaImage::from_pixel(w as u32, h as u32, Rgba([0, 0, 0, 0]));

    // 1. Flood-fill from canvas perimeter across low-alpha pixels to find every pixel strictly outside the border frame.
    let mut outside = vec![false; w * h];
    let mut q = VecDeque::new();
    for x in 0..w {
        for y in [0usize, h - 1] {
            let idx = y * w + x;
            if !outside[idx] && border_img.get_pixel(x as u32, y as u32)[3] < 140 {
                outside[idx] = true;
                q.push_back((x, y));
            }
        }
    }
    for y in 0..h {
        for x in [0usize, w - 1] {
            let idx = y * w + x;
            if !outside[idx] && border_img.get_pixel(x as u32, y as u32)[3] < 140 {
                outside[idx] = true;
                q.push_back((x, y));
            }
        }
    }
    while let Some((x, y)) = q.pop_front() {
        for (nx, ny) in [
            (x.wrapping_sub(1), y),
            (x + 1, y),
            (x, y.wrapping_sub(1)),
            (x, y + 1),
        ] {
            if nx < w && ny < h {
                let idx = ny * w + nx;
                if !outside[idx] && border_img.get_pixel(nx as u32, ny as u32)[3] < 140 {
                    outside[idx] = true;
                    q.push_back((nx, ny));
                }
            }
        }
    }

    // 2. Collect the enclosed interior cutout mask; if a border has an open top/bottom gap so outside reaches center,
    //    fall back to an inscribed ellipse strictly within the border's opaque frame bounds.
    let mut inside = vec![false; w * h];
    let mut inside_cnt = 0usize;
    let (mid_x, mid_y) = (w / 2, h / 2);
    let (core_x0, core_x1) = (w * 35 / 100, w * 65 / 100);
    let (core_y0, core_y1) = (h * 35 / 100, h * 65 / 100);
    if !outside[mid_y * w + mid_x] {
        // Flood from center through everything not marked outside (includes soft inner edge pixels under border AA)
        let mut iq = VecDeque::new();
        inside[mid_y * w + mid_x] = true;
        iq.push_back((mid_x, mid_y));
        while let Some((x, y)) = iq.pop_front() {
            inside_cnt += 1;
            for (nx, ny) in [
                (x.wrapping_sub(1), y),
                (x + 1, y),
                (x, y.wrapping_sub(1)),
                (x, y + 1),
            ] {
                if nx < w && ny < h {
                    let idx = ny * w + nx;
                    if !inside[idx]
                        && !outside[idx]
                        && (border_img.get_pixel(nx as u32, ny as u32)[3] < 235
                            || (nx >= core_x0 && nx <= core_x1 && ny >= core_y0 && ny <= core_y1))
                    {
                        inside[idx] = true;
                        iq.push_back((nx, ny));
                    }
                }
            }
        }
    }
    let min_inside = (w * h / 36).max(200);
    if inside_cnt < min_inside {
        // Fallback for open-frame decorative borders: tight inscribed circle inside frame bounds
        inside.fill(false);
        let (mut bx0, mut by0, mut bx1, mut by1) = (w * 13 / 100, h * 13 / 100, w * 86 / 100, h * 86 / 100);
        let mut any_opaque = false;
        for y in 0..h {
            for x in 0..w {
                if border_img.get_pixel(x as u32, y as u32)[3] > 64 {
                    if !any_opaque {
                        bx0 = x; bx1 = x; by0 = y; by1 = y; any_opaque = true;
                    } else {
                        bx0 = bx0.min(x); bx1 = bx1.max(x); by0 = by0.min(y); by1 = by1.max(y);
                    }
                }
            }
        }
        let cx = (bx0 + bx1) as f32 * 0.5;
        let cy = (by0 + by1) as f32 * 0.5;
        let pad = (w.min(h) as f32 * 0.05).max(3.0);
        let rx = ((bx1 - bx0) as f32 * 0.5 - pad).max(w as f32 * 0.16);
        let ry = ((by1 - by0) as f32 * 0.5 - pad).max(h as f32 * 0.16);
        for y in 0..h {
            for x in 0..w {
                let norm = ((x as f32 - cx) / rx).powi(2) + ((y as f32 - cy) / ry).powi(2);
                if norm <= 1.0 {
                    inside[y * w + x] = true;
                }
            }
        }
    }

    // 3. Bounding box of interior cutout so user's PFP fills the exact visible hole cleanly.
    let (mut ix0, mut iy0, mut ix1, mut iy1) = (w - 1, h - 1, 0usize, 0usize);
    for y in 0..h {
        for x in 0..w {
            if inside[y * w + x] {
                ix0 = ix0.min(x); ix1 = ix1.max(x);
                iy0 = iy0.min(y); iy1 = iy1.max(y);
            }
        }
    }
    let hole_w = (ix1.saturating_sub(ix0) + 1).max(1) as u32;
    let hole_h = (iy1.saturating_sub(iy0) + 1).max(1) as u32;

    if let Some(u) = user_img {
        let pfp_resized = DynamicImage::ImageRgba8(u.clone())
            .resize_exact(hole_w, hole_h, FilterType::Lanczos3)
            .to_rgba8();
        for y in iy0..=iy1 {
            for x in ix0..=ix1 {
                if inside[y * w + x] {
                    let px = pfp_resized.get_pixel((x - ix0) as u32, (y - iy0) as u32);
                    canvas.put_pixel(x as u32, y as u32, *px);
                }
            }
        }
    } else {
        let sc = (w.min(h) as f32) / 120.0;
        let cx_f = w as f32 * 0.5;
        let head_cy = h as f32 * (52.0 / 120.0);
        let body_cy = h as f32 * (96.0 / 120.0);
        for y in iy0..=iy1 {
            for x in ix0..=ix1 {
                if inside[y * w + x] {
                    let dx = (x as f32 - cx_f) / sc;
                    let dy_h = (y as f32 - head_cy) / sc;
                    let dy_b = (y as f32 - body_cy) / sc;
                    let head = (dx * dx + dy_h * dy_h).sqrt() < 14.0;
                    let body = (dx / 24.0).powi(2) + (dy_b / 22.0).powi(2) < 1.0;
                    let col = if head || body {
                        Rgba([78, 92, 118, 255])
                    } else {
                        Rgba([28, 34, 48, 255])
                    };
                    canvas.put_pixel(x as u32, y as u32, col);
                }
            }
        }
    }

    // 4. Composite the border frame on top; pixels outside the border stay alpha == 0.
    image::imageops::overlay(&mut canvas, border_img, 0, 0);
    canvas
}

fn is_ignored_border_aux_texture(full_lower: &str) -> bool {
    full_lower.contains("thumb_")
        || full_lower.contains(".thumb")
        || full_lower.ends_with("_dial")
        || full_lower.ends_with("_glass")
        || full_lower.contains(".pips_")
}

fn border_layer_priority(full_lower: &str) -> u8 {
    if full_lower.contains("staticlayer") || full_lower.contains("staticbg") || full_lower.contains("staticframe") {
        0
    } else if full_lower.contains("paintedlayer") {
        2
    } else if full_lower.contains("tintedlayer") {
        3
    } else {
        1
    }
}

/// Selects decodable border frame `Texture2D` export indices (`(priority, exp_idx)`),
/// skipping inventory `Thumb_*` icons and tiny accessory sprites without a center cutout.
fn select_avatar_border_frame_exports(upk: &UnpackedUpk, cooked_dir: Option<&Path>) -> Vec<(u8, usize, RgbaImage)> {
    let mut cands = Vec::new();
    for (idx, exp) in upk.exports.iter().enumerate() {
        if upk.export_class_name(exp) != "Texture2D" {
            continue;
        }
        let full_low = upk.full_export_path(idx + 1).to_lowercase();
        if is_ignored_border_aux_texture(&full_low) {
            continue;
        }
        if let Some(im) = decode_texture2d_export(upk, idx, cooked_dir) {
            let (w, h) = im.dimensions();
            if w < 64 || h < 64 {
                continue;
            }
            let center_a = im.get_pixel(w / 2, h / 2)[3];
            // Require center cutout transparency unless it's the only texture candidate
            if center_a < 180 {
                cands.push((border_layer_priority(&full_low), idx, im));
            }
        }
    }
    cands.sort_by_key(|(prio, idx, _)| (*prio, *idx));
    cands
}

/// Extracts the `AvatarBorder` frame texture (120x120..600x600, BGRA8 or DXT5, from pristine vault if modified)
/// and composites the user's PFP cleanly inside the frame cutout as a 120x120 RGBA preview.
pub fn compose_pfp_with_border_preview(
    cooked_dir: &Path,
    border_pkg: &str,
    user_img: Option<&RgbaImage>,
) -> Option<RgbaImage> {
    let raw_p = cooked_dir.join(border_pkg);
    let bak = bak_path_for(&raw_p);
    let vpath = vault_backup_path(&raw_p);
    let src_p = if vpath.is_file() {
        vpath
    } else if bak.is_file() {
        bak
    } else {
        raw_p
    };
    let (upk, _) = unpack_upk_from_disk(&src_p).ok()?;

    let layers = select_avatar_border_frame_exports(&upk, Some(cooked_dir));
    let composite_frame = if !layers.is_empty() {
        let (_, _, first) = &layers[0];
        let (bw, bh) = first.dimensions();
        let mut combined = first.clone();
        // Overlay PaintedLayer / TintedLayer cleanly if present alongside StaticLayer,
        // without stacking multiple animation frames (same priority != painted/tinted).
        for (prio, _, layer_im) in layers.iter().skip(1) {
            if *prio == 2 || *prio == 3 {
                let resized = if layer_im.dimensions() != (bw, bh) {
                    DynamicImage::ImageRgba8(layer_im.clone())
                        .resize_exact(bw, bh, FilterType::Lanczos3)
                        .to_rgba8()
                } else {
                    layer_im.clone()
                };
                image::imageops::overlay(&mut combined, &resized, 0, 0);
            }
        }
        combined
    } else {
        // Legacy byte-pattern fallback for any 120x120 BGRA8 border
        let marker = [0x00, 0xE1, 0x00, 0x00, 0x00, 0xE1, 0x00, 0x00];
        let rel = upk.file_bytes.windows(8).position(|w| w == marker)?;
        let pix = &upk.file_bytes[rel + 8..rel.checked_add(8 + 57600)?];
        decode_bgra8(120, 120, pix)?
    };

    let preview = render_pfp_into_border_frame(&composite_frame, user_img);
    if preview.dimensions() != (120, 120) {
        Some(
            DynamicImage::ImageRgba8(preview)
                .resize_exact(120, 120, FilterType::Lanczos3)
                .to_rgba8(),
        )
    } else {
        Some(preview)
    }
}

fn encode_border_mip_bytes(img: &RgbaImage, fmt: &str) -> Option<Vec<u8>> {
    if fmt == "PF_A8R8G8B8" {
        Some(encode_bgra8(img))
    } else if fmt == "PF_DXT1" {
        Some(encode_dxt1(img))
    } else if fmt == "PF_DXT5" {
        Some(encode_dxt5(img))
    } else {
        None
    }
}

/// Patches every inline mipmap level inside a border `Texture2D` export in-place (preserving exact UPK offsets).
fn patch_border_tex2d_export_in_place(
    upk: &UnpackedUpk,
    exp_idx: usize,
    mod_bytes: &mut [u8],
    user_img: &RgbaImage,
) -> Option<usize> {
    let exp = upk.exports.get(exp_idx)?;
    if upk.export_class_name(exp) != "Texture2D" || exp.serial_offset + exp.serial_size > mod_bytes.len() {
        return None;
    }
    let slice = &upk.file_bytes[exp.serial_offset..exp.serial_offset + exp.serial_size];
    let (props_end, fmt, _, _, _) = parse_tex2d_header_props(upk, slice);
    if props_end == 0 {
        return None;
    }
    let (mip_off, mcnt) = find_tex2d_mip_array_start(slice, props_end)?;

    let mut inline_mips: Vec<(usize, u32, u32, usize)> = Vec::new();
    let mut cur = mip_off + 4;
    for _ in 0..mcnt {
        if cur + 12 > slice.len() {
            break;
        }
        let mf = read_u32_le(slice, cur);
        let md_i = read_i32_le(slice, cur + 8);
        if (mf & 0x11) == 0 && md_i > 0 {
            let pix_rel = cur + 12;
            let md = md_i as usize;
            if pix_rel + md + 8 > slice.len() {
                break;
            }
            let mw = read_u32_le(slice, pix_rel + md);
            let mh = read_u32_le(slice, pix_rel + md + 4);
            inline_mips.push((pix_rel, mw, mh, md));
            cur = pix_rel + md + 8;
        } else if cur + 28 <= slice.len() {
            cur += 28;
        } else {
            break;
        }
    }
    if inline_mips.is_empty() {
        return None;
    }
    let (m0_rel, w0, h0, md0) = inline_mips[0];
    let border0 = decode_pixel_bytes(w0, h0, &fmt, &slice[m0_rel..m0_rel + md0])?;
    if border0.get_pixel(w0 / 2, h0 / 2)[3] >= 180 {
        return None;
    }
    let composited0 = render_pfp_into_border_frame(&border0, Some(user_img));

    let mut written = 0usize;
    for (pix_rel, mw, mh, md) in inline_mips {
        if mw == 0 || mh == 0 {
            continue;
        }
        let mip_im = if mw == w0 && mh == h0 {
            composited0.clone()
        } else {
            DynamicImage::ImageRgba8(composited0.clone())
                .resize_exact(mw, mh, FilterType::Lanczos3)
                .to_rgba8()
        };
        if let Some(encoded) = encode_border_mip_bytes(&mip_im, &fmt) {
            if encoded.len() == md {
                let abs = exp.serial_offset + pix_rel;
                mod_bytes[abs..abs + md].copy_from_slice(&encoded);
                written += 1;
            }
        }
    }
    if written > 0 { Some(written) } else { None }
}

pub fn inject_custom_pfp(
    cooked_dir: &Path,
    target_border_pkg: &str,
    user_img: &RgbaImage,
) -> Result<String, String> {
    let target_path = cooked_dir.join(target_border_pkg);
    if !target_path.exists() {
        return Err(format!("Avatar border package not found: {}", target_path.display()));
    }

    ensure_pristine_before_swap(&target_path)?;
    let (unpacked, orig_raw) = unpack_upk_from_disk(&target_path)?;
    let mut mod_bytes = unpacked.file_bytes.clone();
    let mut patched_count = 0usize;

    // 1. Export-aware injection covering all 177+ BGRA8 & DXT5 borders across all dimensions
    let cands = select_avatar_border_frame_exports(&unpacked, Some(cooked_dir));
    if !cands.is_empty() {
        let has_static = cands.iter().any(|(prio, _, _)| *prio == 0);
        for (prio, exp_idx, _) in &cands {
            // When StaticLayer/StaticBG exists alongside PaintedLayer/TintedLayer, patch only the static base layer
            // (and fallback to all frame layers when no Static* layer exists).
            if has_static && *prio > 0 {
                continue;
            }
            if let Some(cnt) = patch_border_tex2d_export_in_place(&unpacked, *exp_idx, &mut mod_bytes, user_img) {
                patched_count += cnt;
            }
        }
    }

    // 2. Fallback byte-pattern scanner for legacy 120x120 BGRA8 mips if export walk didn't match
    if patched_count == 0 {
        let marker = [0x00, 0xE1, 0x00, 0x00, 0x00, 0xE1, 0x00, 0x00]; // (57600, 57600)
        let mut search_from = 0usize;
        while search_from + 8 <= mod_bytes.len() {
            let Some(rel) = mod_bytes[search_from..].windows(8).position(|w| w == marker) else {
                break;
            };
            let pos = search_from + rel;
            let pix_start = pos + 8;
            if pix_start + 57600 <= mod_bytes.len() {
                if let Some(border_img) = decode_bgra8(120, 120, &mod_bytes[pix_start..pix_start + 57600]) {
                    let canvas = render_pfp_into_border_frame(&border_img, Some(user_img));
                    let bgra = encode_bgra8(&canvas);
                    mod_bytes[pix_start..pix_start + 57600].copy_from_slice(&bgra);
                    patched_count += 1;
                }
                search_from = pix_start + 57600;
            } else {
                break;
            }
        }
    }
    if patched_count == 0 {
        return Err(format!("Unsupported or empty avatar border texture in {}", target_border_pkg));
    }

    rebuild_upk_selective_chunks(&orig_raw, &unpacked, &mod_bytes, &unpacked.key, &target_path)?;
    clear_rl_texture_cache();
    Ok(format!("Injected custom Profile Picture into {} ({} mips)", target_border_pkg, patched_count))
}

// ── 2. Custom Player Banner Injector (420x100 BGRA8) ─────────────────────────

pub fn inject_custom_banner(
    cooked_dir: &Path,
    target_banner_pkg: &str,
    user_img: &RgbaImage,
) -> Result<String, String> {
    let target_path = cooked_dir.join(target_banner_pkg);
    if !target_path.exists() {
        return Err(format!("Player banner package not found: {}", target_path.display()));
    }

    ensure_pristine_before_swap(&target_path)?;
    let (unpacked, orig_raw) = unpack_upk_from_disk(&target_path)?;
    let mut mod_bytes = unpacked.file_bytes.clone();

    let banner_resized = DynamicImage::ImageRgba8(user_img.clone())
        .resize_exact(420, 100, FilterType::Lanczos3)
        .to_rgba8();
    let bgra = encode_bgra8(&banner_resized);

    let marker = [0x40, 0x90, 0x02, 0x00, 0x40, 0x90, 0x02, 0x00]; // (168000, 168000)
    let mut patched_count = 0usize;
    let mut search_from = 0usize;
    while search_from + 8 <= mod_bytes.len() {
        let Some(rel) = mod_bytes[search_from..].windows(8).position(|w| w == marker) else {
            break;
        };
        let pos = search_from + rel;
        let pix_start = pos + 8;
        if pix_start + 168000 <= mod_bytes.len() {
            mod_bytes[pix_start..pix_start + 168000].copy_from_slice(&bgra);
            patched_count += 1;
            search_from = pix_start + 168000;
        } else {
            break;
        }
    }
    if patched_count == 0 {
        patched_count = patch_any_inline_texture_mips(&unpacked, &mut mod_bytes, user_img);
    }
    if patched_count == 0 {
        return Err(format!("Banner texture mip not found in {}", target_banner_pkg));
    }

    rebuild_upk_selective_chunks(&orig_raw, &unpacked, &mod_bytes, &unpacked.key, &target_path)?;
    clear_rl_texture_cache();
    Ok(format!("Injected custom 14:3 Player Banner into {} ({} mips)", target_banner_pkg, patched_count))
}

// ── Shared Deterministic UE3 Texture2D Decoder & Inline Mip Rebuilder ────────

fn parse_tex2d_header_props(upk: &UnpackedUpk, slice: &[u8]) -> (usize, String, String, u32, u32) {
    let limit = slice.len().min(32);
    let mut props_end = 0usize;
    let mut props = Vec::new();
    for st in (0..limit).step_by(4).chain((0..limit).filter(|x| x % 4 != 0)) {
        let (e, pr, ok) = parse_property_stream(upk, slice, st);
        if ok && !pr.is_empty() {
            props_end = e;
            props = pr;
            break;
        }
    }
    let mut fmt = String::from("PF_DXT5");
    let mut tfc = String::from("None");
    let mut sx = 0u32;
    let mut sy = 0u32;
    for pr in &props {
        let abs = pr.value_offset;
        if pr.name == "Format" && pr.size >= 8 && abs + 4 <= slice.len() {
            fmt = upk.resolve_name(read_i32_le(slice, abs)).to_string();
        } else if pr.name == "TextureFileCacheName" && pr.size >= 8 && abs + 4 <= slice.len() {
            tfc = upk.resolve_name(read_i32_le(slice, abs)).to_string();
        } else if pr.name == "SizeX" && pr.size == 4 && abs + 4 <= slice.len() {
            sx = read_u32_le(slice, abs);
        } else if pr.name == "SizeY" && pr.size == 4 && abs + 4 <= slice.len() {
            sy = read_u32_le(slice, abs);
        }
    }
    (props_end, fmt, tfc, sx, sy)
}

fn find_tex2d_mip_array_start(slice: &[u8], props_end: usize) -> Option<(usize, usize)> {
    // SourceArt FByteBulkData sits immediately at props_end (12 bytes when unused/empty)
    for off in (props_end..props_end.saturating_add(40).min(slice.len().saturating_sub(28))).step_by(4) {
        let cnt = read_i32_le(slice, off);
        if (1..=16).contains(&cnt) {
            let mf = read_u32_le(slice, off + 4);
            let mu = read_i32_le(slice, off + 8);
            let sx = read_u32_le(slice, off + 24);
            let sy = read_u32_le(slice, off + 28);
            if mu > 0 && ((mf & 0x01) != 0 || mu <= 67_108_864) {
                return Some((off, cnt as usize));
            }
            if mu == 0 && (mf & 0x21) != 0 && sx >= 4 && sy >= 4 && sx <= 8192 && sy <= 8192 && sx.is_power_of_two() && sy.is_power_of_two() {
                return Some((off, cnt as usize));
            }
        }
    }
    None
}

fn decode_pixel_bytes(w: u32, h: u32, fmt: &str, raw: &[u8]) -> Option<RgbaImage> {
    let px_cnt = (w as usize) * (h as usize);
    if fmt == "PF_A8R8G8B8" || raw.len() == px_cnt * 4 {
        decode_bgra8(w, h, raw)
    } else if fmt == "PF_DXT1" || raw.len() * 2 == px_cnt {
        decode_dxt1(w, h, raw)
    } else {
        decode_dxt5(w, h, raw)
    }
}

/// Decodes the highest-resolution mip of a `Texture2D` export (`exp_idx`) from either external
/// `.tfc` cache (using `cooked_dir`) or inline mips (`12`-byte inline mip headers).
pub fn decode_texture2d_export(
    upk: &UnpackedUpk,
    exp_idx: usize,
    cooked_dir: Option<&Path>,
) -> Option<RgbaImage> {
    let exp = upk.exports.get(exp_idx)?;
    if upk.export_class_name(exp) != "Texture2D" || exp.serial_offset + exp.serial_size > upk.file_bytes.len() {
        return None;
    }
    let slice = &upk.file_bytes[exp.serial_offset..exp.serial_offset + exp.serial_size];
    let (props_end, fmt, tfc, _, _) = parse_tex2d_header_props(upk, slice);
    if props_end == 0 {
        return None;
    }
    let (mip_off, mcnt) = find_tex2d_mip_array_start(slice, props_end)?;
    let mut cur = mip_off + 4;
    let mut best_tfc: Option<(u32, u32, u64, usize)> = None;
    let mut best_inline: Option<(u32, u32, &[u8])> = None;

    for _ in 0..mcnt {
        if cur + 12 > slice.len() {
            break;
        }
        let mf = read_u32_le(slice, cur);
        let md_i = read_i32_le(slice, cur + 8);
        if (mf & 0x11) == 0 && md_i > 0 {
            cur += 12; // Inline mip header = flags(4) + elem(4) + disk(4)
            let md = md_i as usize;
            if cur + md + 8 > slice.len() {
                break;
            }
            let pix = &slice[cur..cur + md];
            cur += md;
            let mw = read_u32_le(slice, cur);
            let mh = read_u32_le(slice, cur + 4);
            cur += 8;
            if best_inline.map_or(true, |b| mw > b.0) {
                best_inline = Some((mw, mh, pix));
            }
        } else if cur + 28 <= slice.len() {
            let moff = read_i64_le(slice, cur + 12) as u64;
            let mw = read_u32_le(slice, cur + 20);
            let mh = read_u32_le(slice, cur + 24);
            cur += 28;
            if (mf & 0x01) != 0 && md_i > 16 && tfc != "None" && best_tfc.is_none() {
                best_tfc = Some((mw, mh, moff, md_i as usize));
            }
        } else {
            break;
        }
    }

    if let (Some((w, h, off, dsz)), Some(cdir)) = (best_tfc, cooked_dir) {
        let tfcp = cdir.join(format!("{}.tfc", tfc));
        if let Ok(mut f) = File::open(&tfcp) {
            if f.seek(SeekFrom::Start(off)).is_ok() {
                let mut buf = vec![0u8; dsz];
                if f.read_exact(&mut buf).is_ok() {
                    let raw = if buf.len() >= 4 && read_u32_le(&buf, 0) == PACKAGE_FILE_TAG {
                        decompress_chunk_payload(&buf).unwrap_or(buf)
                    } else {
                        buf
                    };
                    if let Some(im) = decode_pixel_bytes(w, h, &fmt, &raw) {
                        return Some(im);
                    }
                }
            }
        }
    }

    if let Some((w, h, pix)) = best_inline {
        return decode_pixel_bytes(w, h, &fmt, pix);
    }
    None
}

fn resolve_or_add_name(upk: &UnpackedUpk, added_names: &mut Vec<String>, name: &str) -> i32 {
    if let Some(existing) = upk.find_name_index(name) {
        existing
    } else if let Some(pos) = added_names.iter().position(|n| n.eq_ignore_ascii_case(name)) {
        (upk.names.len() + pos) as i32
    } else {
        let idx = (upk.names.len() + added_names.len()) as i32;
        added_names.push(name.to_string());
        idx
    }
}

/// Converts any `Texture2D` export (inline OR `.tfc`-backed) into a self-contained inline mip
/// chain (`target_dim -> 1x1`) preserving alpha via `PF_DXT5` when needed, setting `NeverStream=1`,
/// and stripping `.tfc` streaming properties (`TextureFileCacheName`, `FirstResourceMemMip`).
pub fn build_inline_texture2d_serial_with_names(
    upk: &UnpackedUpk,
    orig_serial: &[u8],
    user_img: &RgbaImage,
    target_w: u32,
    target_h: u32,
    force_dxt5: bool,
    added_names: &mut Vec<String>,
) -> Result<(Vec<u8>, usize, String), String> {
    build_inline_texture2d_inner(upk, orig_serial, user_img, target_w, target_h, force_dxt5, Some(added_names))
}

/// Converts any `Texture2D` export into a self-contained inline mip chain (without growing NameTable).
pub fn build_inline_texture2d_serial(
    upk: &UnpackedUpk,
    orig_serial: &[u8],
    user_img: &RgbaImage,
    target_w: u32,
    target_h: u32,
) -> Result<(Vec<u8>, usize), String> {
    let (ser, mips, _) = build_inline_texture2d_inner(upk, orig_serial, user_img, target_w, target_h, false, None)?;
    Ok((ser, mips))
}

fn build_inline_texture2d_inner(
    upk: &UnpackedUpk,
    orig_serial: &[u8],
    user_img: &RgbaImage,
    target_w: u32,
    target_h: u32,
    force_dxt5: bool,
    mut added_names: Option<&mut Vec<String>>,
) -> Result<(Vec<u8>, usize, String), String> {
    let none_idx = upk.find_name_index("None").unwrap_or(0);
    let mut props_end = 0usize;
    let mut props_start = 0usize;
    let mut parsed_props = Vec::new();

    let limit = orig_serial.len().min(32);
    for st in (0..limit).step_by(4).chain((0..limit).filter(|x| x % 4 != 0)) {
        let (end_off, parsed, ok) = parse_property_stream(upk, orig_serial, st);
        if ok && !parsed.is_empty() {
            props_start = st;
            props_end = end_off;
            parsed_props = parsed;
            break;
        }
    }
    if props_end == 0 {
        return Err("Texture2D properties not found".into());
    }

    let (mip_off, mip_count) =
        find_tex2d_mip_array_start(orig_serial, props_end).ok_or("Texture2D mip array not found")?;

    let mut walk = mip_off + 4;
    for _ in 0..mip_count {
        if walk + 12 > orig_serial.len() {
            break;
        }
        let flags = read_u32_le(orig_serial, walk);
        let disk = read_i32_le(orig_serial, walk + 8).max(0) as usize;
        if (flags & 0x21) != 0 {
            walk += 28;
        } else {
            walk += 12 + disk + 8;
        }
    }
    let tail_slice = if walk < orig_serial.len() {
        &orig_serial[walk..]
    } else {
        &[]
    };

    let base_img = DynamicImage::ImageRgba8(user_img.clone())
        .resize_exact(target_w, target_h, FilterType::Lanczos3)
        .to_rgba8();
    let has_alpha = base_img.pixels().any(|p| p[3] < 250);

    // Determine target UE3 PixelFormat before serializing properties.
    // Tangent normal maps (`TC_Normalmap` / `TEXTUREGROUP_*NormalMap`) MUST remain `PF_DXT1` when
    // authored as RGB normals (`orig_fmt == "PF_DXT1"`), because UE3 treats `PF_DXT5`+`TC_Normalmap`
    // as DXT5n (`X=Alpha, Y=Green`) and dummy PNG alpha (`A=113`) destroys shading.
    let is_normal_map = parsed_props.iter().any(|p| {
        (p.name == "CompressionSettings" || p.name == "LODGroup")
            && p.size >= 8
            && upk.resolve_name(read_i32_le(orig_serial, p.value_offset)).contains("Normal")
    });
    let orig_fmt = parsed_props
        .iter()
        .find(|p| p.name == "Format" && p.size >= 8)
        .map(|p| upk.resolve_name(read_i32_le(orig_serial, p.value_offset)).to_string())
        .unwrap_or_else(|| "PF_DXT5".to_string());
    let fmt = if orig_fmt == "PF_A8R8G8B8" {
        "PF_A8R8G8B8".to_string()
    } else if (force_dxt5 || has_alpha) && !(is_normal_map && !force_dxt5 && orig_fmt == "PF_DXT1") {
        if upk.find_name_index("PF_DXT5").is_some() || added_names.is_some() {
            "PF_DXT5".to_string()
        } else {
            orig_fmt
        }
    } else if orig_fmt == "PF_DXT1" || orig_fmt == "PF_DXT5" {
        orig_fmt
    } else if upk.find_name_index("PF_DXT5").is_some() || added_names.is_some() {
        "PF_DXT5".to_string()
    } else {
        orig_fmt
    };

    let resolve_nm = |added_opt: &mut Option<&mut Vec<String>>, nm: &str| -> Option<i32> {
        if let Some(existing) = upk.find_name_index(nm) {
            Some(existing)
        } else if let Some(ref mut v) = added_opt {
            Some(resolve_or_add_name(upk, v, nm))
        } else {
            None
        }
    };

    let mut no_mipmaps = false;
    let mut has_never_stream = false;
    let mut mip_tail_pos: Option<usize> = None;
    let mut rebuilt_hdr = Vec::with_capacity(props_end + 32);
    rebuilt_hdr.extend_from_slice(&orig_serial[..props_start]);

    for (i, p) in parsed_props.iter().enumerate() {
        let tag_start = if i == 0 {
            props_start
        } else {
            parsed_props[i - 1].value_offset + parsed_props[i - 1].size
        };
        let tag_end = p.value_offset + p.size;
        if tag_start >= tag_end || tag_end > props_end {
            continue;
        }
        // Native inline NeverStream textures omit TFC streaming properties completely.
        if p.name == "TextureFileCacheName" || p.name == "FirstResourceMemMip" {
            continue;
        }
        let mut chunk = orig_serial[tag_start..tag_end].to_vec();
        let rel_val = p.value_offset - tag_start;
        match p.name.as_str() {
            "Format" if p.size >= 8 => {
                if let Some(fmt_idx) = resolve_nm(&mut added_names, &fmt) {
                    write_i32_le(&mut chunk, rel_val, fmt_idx);
                    write_i32_le(&mut chunk, rel_val + 4, 0);
                }
            }
            "SizeX" | "OriginalSizeX" if p.size == 4 => {
                write_i32_le(&mut chunk, rel_val, target_w as i32);
            }
            "SizeY" | "OriginalSizeY" if p.size == 4 => {
                write_i32_le(&mut chunk, rel_val, target_h as i32);
            }
            "LODBias" if p.size == 4 => {
                write_i32_le(&mut chunk, rel_val, 0);
            }
            "MipTailBaseIdx" if p.size == 4 => {
                mip_tail_pos = Some(rebuilt_hdr.len() + rel_val);
                let tail_idx = target_w.max(target_h).trailing_zeros() as i32;
                write_i32_le(&mut chunk, rel_val, tail_idx);
            }
            "MipGenSettings" if p.size >= 8 => {
                if upk.resolve_name(read_i32_le(&chunk, rel_val)) == "TMGS_NoMipmaps" {
                    no_mipmaps = true;
                }
            }
            "NeverStream" if p.tag_type == "BoolProperty" => {
                chunk[rel_val] = 1;
                has_never_stream = true;
            }
            _ => {}
        }
        rebuilt_hdr.extend_from_slice(&chunk);
    }

    if no_mipmaps {
        if let Some(pos) = mip_tail_pos {
            write_i32_le(&mut rebuilt_hdr, pos, 0);
        }
    }
    if !has_never_stream {
        if let (Some(ns_idx), Some(bool_idx)) = (
            resolve_nm(&mut added_names, "NeverStream"),
            resolve_nm(&mut added_names, "BoolProperty"),
        ) {
            rebuilt_hdr.extend_from_slice(&ns_idx.to_le_bytes());
            rebuilt_hdr.extend_from_slice(&0i32.to_le_bytes());
            rebuilt_hdr.extend_from_slice(&bool_idx.to_le_bytes());
            rebuilt_hdr.extend_from_slice(&0i32.to_le_bytes());
            rebuilt_hdr.extend_from_slice(&0i32.to_le_bytes());
            rebuilt_hdr.extend_from_slice(&0i32.to_le_bytes());
            rebuilt_hdr.push(1);
        }
    }
    rebuilt_hdr.extend_from_slice(&none_idx.to_le_bytes());
    rebuilt_hdr.extend_from_slice(&0i32.to_le_bytes());
    // Copy SourceArt trailer verbatim between None and MipCount.
    rebuilt_hdr.extend_from_slice(&orig_serial[props_end..mip_off]);

    let mut mips_data: Vec<(u32, u32, Vec<u8>)> = Vec::new();
    let mut w = target_w;
    let mut h = target_h;
    loop {
        let lvl = if w == target_w && h == target_h {
            base_img.clone()
        } else {
            DynamicImage::ImageRgba8(base_img.clone())
                .resize_exact(w, h, FilterType::Triangle)
                .to_rgba8()
        };
        let encoded = if fmt == "PF_A8R8G8B8" {
            encode_bgra8(&lvl)
        } else if fmt == "PF_DXT1" {
            encode_dxt1(&lvl)
        } else {
            encode_dxt5(&lvl)
        };
        mips_data.push((w, h, encoded));
        if no_mipmaps || (w == 1 && h == 1) {
            break;
        }
        w = (w / 2).max(1);
        h = (h / 2).max(1);
    }

    let total_pix: usize = mips_data.iter().map(|(_, _, b)| 20 + b.len()).sum();
    let mut new_serial = Vec::with_capacity(rebuilt_hdr.len() + 4 + total_pix + tail_slice.len());
    new_serial.extend_from_slice(&rebuilt_hdr);
    new_serial.extend_from_slice(&(mips_data.len() as i32).to_le_bytes());
    for (mw, mh, pix) in &mips_data {
        let (out_mw, out_mh) = if fmt == "PF_A8R8G8B8" {
            (*mw, *mh)
        } else {
            ((*mw).max(4), (*mh).max(4))
        };
        new_serial.extend_from_slice(&(0x0001_0000u32).to_le_bytes());
        new_serial.extend_from_slice(&(pix.len() as i32).to_le_bytes());
        new_serial.extend_from_slice(&(pix.len() as i32).to_le_bytes());
        new_serial.extend_from_slice(pix);
        new_serial.extend_from_slice(&(out_mw as i32).to_le_bytes());
        new_serial.extend_from_slice(&(out_mh as i32).to_le_bytes());
    }
    new_serial.extend_from_slice(tail_slice);
    Ok((new_serial, mips_data.len(), fmt))
}

// ── 3. Custom Decal & AlphaConsole (.ZIP / .JSON / .PNG) Engine ──────────────

#[derive(Debug, Clone)]
pub struct AlphaConsoleDecalPack {
    pub name: String,
    pub body_id: Option<i64>,
    pub skin_id: Option<i64>,
    pub diffuse_base_img: Option<RgbaImage>,
    pub skin_img: Option<RgbaImage>,
    pub mask_img: Option<RgbaImage>,
    pub chassis_diffuse_img: Option<RgbaImage>,
    pub source_label: String,
}

#[derive(Debug, Clone)]
pub struct AlphaConsoleBallPack {
    pub name: String,
    pub group: String,
    pub diffuse_img: RgbaImage,
    pub normal_img: Option<RgbaImage>,
    pub mask_img: Option<RgbaImage>,
    pub source_label: String,
}

enum AcAssetStore {
    Zip(HashMap<String, Vec<u8>>),
    Dir(PathBuf),
}

impl AcAssetStore {
    fn from_zip_path(path: &Path) -> Result<(Self, Vec<(String, String)>), String> {
        let f = File::open(path).map_err(|e| format!("Failed opening archive: {e}"))?;
        let mut archive = zip::ZipArchive::new(f).map_err(|e| format!("Invalid ZIP archive: {e}"))?;
        let mut files = HashMap::new();
        let mut jsons = Vec::new();
        for i in 0..archive.len() {
            let Ok(mut entry) = archive.by_index(i) else {
                continue;
            };
            if entry.is_dir() {
                continue;
            }
            let raw_name = entry.name().replace('\\', "/");
            if raw_name.to_lowercase().contains("__macosx/") {
                continue;
            }
            let mut buf = Vec::with_capacity(entry.size() as usize);
            if entry.read_to_end(&mut buf).is_err() {
                continue;
            }
            if raw_name.to_lowercase().ends_with(".json") {
                if let Ok(txt) = String::from_utf8(buf.clone()) {
                    jsons.push((raw_name.clone(), txt));
                }
            }
            let low = raw_name.to_lowercase();
            let base = Path::new(&raw_name)
                .file_name()
                .map(|s| s.to_string_lossy().to_lowercase())
                .unwrap_or_else(|| low.clone());
            files.insert(low, buf.clone());
            files.entry(base).or_insert(buf);
        }
        Ok((Self::Zip(files), jsons))
    }

    fn load_image(&self, rel_or_name: &str, cache: &mut HashMap<String, RgbaImage>) -> Option<RgbaImage> {
        let clean = rel_or_name.trim().replace('\\', "/");
        if clean.is_empty() {
            return None;
        }
        let low = clean.to_lowercase();
        if let Some(im) = cache.get(&low) {
            return Some(im.clone());
        }
        let base = Path::new(&clean)
            .file_name()
            .map(|s| s.to_string_lossy().to_lowercase())
            .unwrap_or_else(|| low.clone());
        if let Some(im) = cache.get(&base) {
            return Some(im.clone());
        }

        let loaded = match self {
            AcAssetStore::Zip(map) => {
                let bytes = map.get(&low).or_else(|| map.get(&base))?;
                image::load_from_memory(bytes).ok().map(|im| im.to_rgba8())?
            }
            AcAssetStore::Dir(root) => {
                let p1 = root.join(&clean);
                let p2 = root.join(&base);
                let found = if p1.is_file() {
                    Some(p1)
                } else if p2.is_file() {
                    Some(p2)
                } else {
                    find_file_ci(root, &base)
                }?;
                image::open(found).ok().map(|im| im.to_rgba8())?
            }
        };
        cache.insert(low, loaded.clone());
        cache.insert(base, loaded.clone());
        Some(loaded)
    }
}

fn find_file_ci(dir: &Path, file_low: &str) -> Option<PathBuf> {
    if let Ok(entries) = fs::read_dir(dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_file() {
                if p.file_name()
                    .map(|n| n.to_string_lossy().to_lowercase() == file_low)
                    .unwrap_or(false)
                {
                    return Some(p);
                }
            } else if p.is_dir() {
                if let Ok(sub) = fs::read_dir(&p) {
                    for se in sub.flatten() {
                        let sp = se.path();
                        if sp.is_file()
                            && sp
                                .file_name()
                                .map(|n| n.to_string_lossy().to_lowercase() == file_low)
                                .unwrap_or(false)
                        {
                            return Some(sp);
                        }
                    }
                }
            }
        }
    }
    None
}

fn json_field_ci<'a>(obj: &'a serde_json::Map<String, serde_json::Value>, keys: &[&str]) -> Option<&'a serde_json::Value> {
    for want in keys {
        if let Some((_, v)) = obj.iter().find(|(k, _)| k.eq_ignore_ascii_case(want)) {
            return Some(v);
        }
    }
    None
}

fn json_i64_ci(obj: &serde_json::Map<String, serde_json::Value>, keys: &[&str]) -> Option<i64> {
    let v = json_field_ci(obj, keys)?;
    v.as_i64().or_else(|| v.as_str().and_then(|s| s.trim().parse::<i64>().ok()))
}

fn parse_decal_preset_obj(
    name: &str,
    obj: &serde_json::Map<String, serde_json::Value>,
    store: &AcAssetStore,
    cache: &mut HashMap<String, RgbaImage>,
    src_label: &str,
) -> Option<AlphaConsoleDecalPack> {
    let body_id = json_i64_ci(obj, &["BodyID", "body_id", "Body"]);
    let skin_id = json_i64_ci(obj, &["SkinID", "skin_id", "DecalID"]);

    let body_params = json_field_ci(obj, &["Body", "Params", "Decal", "Textures"])
        .and_then(|v| v.as_object())
        .unwrap_or(obj);

    let mut find_tex = |map: &serde_json::Map<String, serde_json::Value>, keys: &[&str]| -> Option<RgbaImage> {
        let val = json_field_ci(map, keys)?;
        let s = val.as_str()?.trim();
        if s.is_empty() {
            return None;
        }
        store.load_image(s, cache)
    };

    let diffuse_base_img = find_tex(body_params, &["Diffuse", "BaseDiffuse", "BodyDiffuse"]);
    let skin_1d = find_tex(body_params, &["1_Diffuse_Skin", "Decal", "SkinDiffuse"]);
    let skin_legacy = find_tex(body_params, &["Skin", "BodySkin"]);
    let skin_img = skin_1d
        .or(skin_legacy)
        .or_else(|| diffuse_base_img.clone());
    let mask_img = find_tex(
        body_params,
        &[
            "2_Diffuse_Skin_Mask",
            "1_Diffuse_Skin_Mask",
            "SkinMask",
            "Mask",
            "Masks",
            "BodyMasks",
            "Diffuse_Mask",
        ],
    );
    let chassis_diffuse_img = json_field_ci(obj, &["Chassis"])
        .and_then(|v| v.as_object())
        .and_then(|m| find_tex(m, &["Diffuse", "1_Diffuse_Skin", "Masks"]));

    if skin_img.is_none() && mask_img.is_none() && diffuse_base_img.is_none() {
        return None;
    }

    Some(AlphaConsoleDecalPack {
        name: name.to_string(),
        body_id,
        skin_id,
        diffuse_base_img,
        skin_img,
        mask_img,
        chassis_diffuse_img,
        source_label: src_label.to_string(),
    })
}

fn parse_ball_preset_obj(
    name: &str,
    obj: &serde_json::Map<String, serde_json::Value>,
    store: &AcAssetStore,
    cache: &mut HashMap<String, RgbaImage>,
    src_label: &str,
) -> Option<AlphaConsoleBallPack> {
    let group = json_field_ci(obj, &["Group"])
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let params = json_field_ci(obj, &["Params", "Textures", "Ball", "Body"])
        .and_then(|v| v.as_object())
        .unwrap_or(obj);

    let mut find_tex = |keys: &[&str]| -> Option<RgbaImage> {
        let s = json_field_ci(params, keys)?.as_str()?.trim();
        if s.is_empty() {
            return None;
        }
        store.load_image(s, cache)
    };

    let diffuse_img = find_tex(&["Diffuse", "1_Diffuse_Skin", "Albedo", "Texture"])?;
    let normal_img = find_tex(&["Normal", "NormalMap", "Bump", "Masks_N"]);
    let mask_img = find_tex(&["Masks", "Mask", "RGB"]);

    Some(AlphaConsoleBallPack {
        name: name.to_string(),
        group,
        diffuse_img,
        normal_img,
        mask_img,
        source_label: src_label.to_string(),
    })
}

fn collect_jsons_from_path(path: &Path) -> Result<(AcAssetStore, Vec<(String, String)>, String), String> {
    let label = path.file_name().unwrap_or_default().to_string_lossy().to_string();
    if path.is_file() {
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if ext == "zip" {
            let (store, jsons) = AcAssetStore::from_zip_path(path)?;
            return Ok((store, jsons, label));
        }
        if ext == "json" {
            let txt = fs::read_to_string(path).map_err(|e| format!("Failed reading JSON: {e}"))?;
            let dir = path.parent().unwrap_or_else(|| Path::new(".")).to_path_buf();
            return Ok((AcAssetStore::Dir(dir), vec![(label.clone(), txt)], label));
        }
        return Err("Unsupported single file".into());
    }
    if path.is_dir() {
        let mut jsons = Vec::new();
        if let Ok(entries) = fs::read_dir(path) {
            for e in entries.flatten() {
                let p = e.path();
                if p.is_file()
                    && p.extension()
                        .map(|s| s.eq_ignore_ascii_case("json"))
                        .unwrap_or(false)
                {
                    if let Ok(txt) = fs::read_to_string(&p) {
                        jsons.push((p.file_name().unwrap_or_default().to_string_lossy().to_string(), txt));
                    }
                }
            }
        }
        return Ok((AcAssetStore::Dir(path.to_path_buf()), jsons, label));
    }
    Err(format!("Path does not exist: {}", path.display()))
}

/// Parses AlphaConsole Decal `.zip`, `.json`, folder, OR standalone `.png/.jpg/.webp` into preset(s).
pub fn parse_alphaconsole_decal_source(path: &Path) -> Result<Vec<AlphaConsoleDecalPack>, String> {
    let label = path.file_name().unwrap_or_default().to_string_lossy().to_string();
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "webp" | "bmp") {
        let im = image::open(path).map_err(|e| format!("Failed loading image: {e}"))?.to_rgba8();
        return Ok(vec![AlphaConsoleDecalPack {
            name: path.file_stem().unwrap_or_default().to_string_lossy().to_string(),
            body_id: None,
            skin_id: None,
            diffuse_base_img: None,
            skin_img: Some(im),
            mask_img: None,
            chassis_diffuse_img: None,
            source_label: label,
        }]);
    }

    let (store, jsons, src_label) = collect_jsons_from_path(path)?;
    let mut cache = HashMap::new();
    let mut out = Vec::new();
    for (_jname, txt) in &jsons {
        let clean = txt.trim_start_matches('\u{feff}');
        let Ok(val) = serde_json::from_str::<serde_json::Value>(clean) else {
            continue;
        };
        if let Some(root) = val.as_object() {
            if json_field_ci(root, &["Body", "BodyID", "1_Diffuse_Skin"]).is_some() {
                let nm = json_field_ci(root, &["Name"])
                    .and_then(|v| v.as_str())
                    .unwrap_or(&src_label);
                if let Some(pack) = parse_decal_preset_obj(nm, root, &store, &mut cache, &src_label) {
                    out.push(pack);
                }
            } else {
                for (preset_name, sub_val) in root {
                    if let Some(sub_obj) = sub_val.as_object() {
                        if let Some(pack) = parse_decal_preset_obj(preset_name, sub_obj, &store, &mut cache, &src_label) {
                            out.push(pack);
                        }
                    }
                }
            }
        }
    }

    // Fallback: ZIP containing PNG(s) without a JSON manifest
    if out.is_empty() {
        if let AcAssetStore::Zip(ref map) = store {
            let mut png_names: Vec<&String> = map
                .keys()
                .filter(|k| k.contains('/') && (k.ends_with(".png") || k.ends_with(".jpg")))
                .collect();
            png_names.sort();
            if let Some(first) = png_names.first() {
                if let Some(im) = store.load_image(first, &mut cache) {
                    out.push(AlphaConsoleDecalPack {
                        name: src_label.clone(),
                        body_id: None,
                        skin_id: None,
                        diffuse_base_img: None,
                        skin_img: Some(im),
                        mask_img: None,
                        chassis_diffuse_img: None,
                        source_label: src_label,
                    });
                }
            }
        }
    }

    if out.is_empty() {
        return Err("No valid AlphaConsole Decal textures found in selection.".into());
    }
    Ok(out)
}

/// Parses AlphaConsole Ball `.zip`, `.json`, folder, OR standalone `.png/.jpg/.webp` (`Diffuse` + `Normal` + optional `Masks`).
pub fn parse_alphaconsole_ball_source(path: &Path) -> Result<Vec<AlphaConsoleBallPack>, String> {
    let label = path.file_name().unwrap_or_default().to_string_lossy().to_string();
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "webp" | "bmp") {
        let im = image::open(path).map_err(|e| format!("Failed loading image: {e}"))?.to_rgba8();
        return Ok(vec![AlphaConsoleBallPack {
            name: path.file_stem().unwrap_or_default().to_string_lossy().to_string(),
            group: String::new(),
            diffuse_img: im,
            normal_img: None,
            mask_img: None,
            source_label: label,
        }]);
    }

    let (store, jsons, src_label) = collect_jsons_from_path(path)?;
    let mut cache = HashMap::new();
    let mut out = Vec::new();
    for (_jname, txt) in &jsons {
        let clean = txt.trim_start_matches('\u{feff}');
        let Ok(val) = serde_json::from_str::<serde_json::Value>(clean) else {
            continue;
        };
        if let Some(root) = val.as_object() {
            if json_field_ci(root, &["Params", "Diffuse"]).is_some() {
                if let Some(pack) = parse_ball_preset_obj(&src_label, root, &store, &mut cache, &src_label) {
                    out.push(pack);
                }
            } else {
                for (preset_name, sub_val) in root {
                    if let Some(sub_obj) = sub_val.as_object() {
                        if let Some(pack) = parse_ball_preset_obj(preset_name, sub_obj, &store, &mut cache, &src_label) {
                            out.push(pack);
                        }
                    }
                }
            }
        }
    }

    if out.is_empty() {
        return Err("No valid AlphaConsole Ball textures found in selection.".into());
    }
    Ok(out)
}

#[derive(Debug, Clone)]
struct DecalShaderProfile {
    pa_path: String,
    has_diffuse_mat: bool,
    has_1skin: bool,
    has_2mask: bool,
    is_color_locked: bool,
    clean_default_mic: bool,
}

fn inspect_decal_profile(upk: &UnpackedUpk, preferred_pa: Option<&str>) -> Option<DecalShaderProfile> {
    let mut pa_exp = preferred_pa.and_then(|target_pa| {
        let low = target_pa.to_lowercase();
        upk.exports.iter().find(|e| {
            upk.export_class_name(e) == "ProductAsset_Skin_TA"
                && upk.full_export_path(e.index).to_lowercase() == low
        })
    });
    if pa_exp.is_none() {
        pa_exp = upk
            .exports
            .iter()
            .find(|e| upk.export_class_name(e) == "ProductAsset_Skin_TA");
    }
    let pa = pa_exp?;
    let props = find_export_properties(upk, pa, &upk.file_bytes);
    let is_color_locked = props.iter().any(|pr| {
        matches!(
            pr.name.as_str(),
            "ForcedTeamColors" | "ForcedCustomColor" | "bTeamFinishDisabled" | "bCustomFinishDisabled"
        )
    });
    let skin_mic_idxs: Vec<usize> = upk
        .exports
        .iter()
        .enumerate()
        .filter(|(idx, exp)| {
            if upk.export_class_name(exp) != "MaterialInstanceConstant" {
                return false;
            }
            let full = upk.full_export_path(*idx + 1).to_lowercase();
            let obj = upk.resolve_name(exp.object_name).to_lowercase();
            !obj.contains("chassis") && !obj.contains("painted") && !full.starts_with("body_")
        })
        .map(|(i, _)| i)
        .collect();
    let has_1skin = skin_mic_idxs
        .iter()
        .any(|&m| find_mic_tex_param_exp(upk, m, "1_Diffuse_Skin").is_some());
    let has_2mask = skin_mic_idxs
        .iter()
        .any(|&m| find_mic_tex_param_exp(upk, m, "2_Diffuse_Skin_Mask").is_some());

    Some(DecalShaderProfile {
        pa_path: upk.full_export_path(pa.index),
        has_diffuse_mat: upk.find_name_index("Body_Paintable_Diffuse_Mat").is_some(),
        has_1skin,
        has_2mask,
        is_color_locked,
        clean_default_mic: skin_mic_idxs.len() == 1,
    })
}

fn car_skin_prefix_for_target(target_pkg: &str) -> Option<String> {
    let low = target_pkg.to_lowercase();
    let mut segs = low.split('_');
    match (segs.next(), segs.next()) {
        (Some(p1), Some(p2)) if (p1 == "skin" || p1 == "body") && !p2.is_empty() => {
            return Some(format!("skin_{}_", p2));
        }
        _ => {}
    }
    match crate::upk_engine::detect_car_uv_label(&low) {
        Some("Octane") => Some("skin_octane_".into()),
        Some("Fennec") => Some("skin_grain_".into()),
        Some("Dominus") => Some("skin_musclecar_".into()),
        Some("Breakout") => Some("skin_force_".into()),
        Some("Backfire") => Some("skin_backfire_".into()),
        _ => None,
    }
}

fn resolve_pristine_donor_path(p: &Path) -> Option<PathBuf> {
    let vpath = vault_backup_path(p);
    if vpath.is_file() {
        return Some(vpath);
    }
    let bpath = bak_path_for(p);
    if bpath.is_file() {
        return Some(bpath);
    }
    if p.is_file() {
        return Some(p.to_path_buf());
    }
    None
}

fn find_diffuse_donor_for_target(
    cooked_dir: &Path,
    target_pkg: &str,
    preferred_skin_pkg: Option<&str>,
    allow_locked_pref: bool,
) -> Option<(PathBuf, String, bool)> {
    // Honor preferred_skin_pkg (e.g. SkinID: 8273 -> skin_octane_dignitas_home_SF.upk) from its
    // PRISTINE backup whenever it natively supports 2_Diffuse_Skin_Mask; locked 1-layer packs
    // (e.g. SkinID: 8945 / badbeats) still fall back to an unlocked 2_Diffuse_Skin_Mask donor.
    if let Some(pref) = preferred_skin_pkg {
        let p = cooked_dir.join(pref);
        if let Some(pristine) = resolve_pristine_donor_path(&p) {
            if let Ok((upk, _)) = unpack_upk_from_disk(&pristine) {
                if let Some(prof) = inspect_decal_profile(&upk, None) {
                    if prof.has_diffuse_mat
                        && prof.has_1skin
                        && prof.has_2mask
                        && (allow_locked_pref || !prof.is_color_locked)
                    {
                        return Some((pristine, prof.pa_path, true));
                    }
                }
            }
        }
    }

    let prefix = car_skin_prefix_for_target(target_pkg)?;
    let mut cands: Vec<PathBuf> = fs::read_dir(cooked_dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            let nm = p.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
            nm.starts_with(&prefix) && nm.ends_with("_sf.upk") && !nm.ends_with("_t_sf.upk")
        })
        .collect();
    cands.sort();

    let mut best: Option<(u8, PathBuf, String, bool)> = None;
    for raw_p in cands {
        let nm = raw_p.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
        let Some(pristine) = resolve_pristine_donor_path(&raw_p) else {
            continue;
        };
        let Ok((upk, _)) = unpack_upk_from_disk(&pristine) else {
            continue;
        };
        let Some(prof) = inspect_decal_profile(&upk, None) else {
            continue;
        };
        if !prof.has_diffuse_mat || !prof.has_1skin {
            continue;
        }
        let known_clean = matches!(
            nm.as_str(),
            "skin_octane_leaves_sf.upk" | "skin_grain_bugbite_sf.upk" | "skin_musclecar_bubblebobbler_sf.upk"
        );
        let rank = match (!prof.is_color_locked, prof.has_2mask, known_clean, prof.clean_default_mic) {
            (true, true, true, _) => 0u8,
            (true, true, false, true) => 1u8,
            (true, true, false, false) => 2u8,
            (true, false, _, _) => 3u8,
            (false, true, _, _) => 4u8,
            (false, false, _, _) => 5u8,
        };
        let usable_2m_unlocked = !prof.is_color_locked && prof.has_2mask;
        if best.as_ref().map_or(true, |(br, _, _, _)| rank < *br) {
            best = Some((rank, pristine, prof.pa_path, usable_2m_unlocked));
            if rank == 0 {
                break;
            }
        }
    }
    best.map(|(_, p, pa, u)| (p, pa, u))
}

fn upsert_mic_texture_params(
    upk: &UnpackedUpk,
    mic_exp_idx: usize,
    desired_params: &[(&str, i32)],
    added_names: &mut Vec<String>,
) -> Option<Vec<u8>> {
    let exp = &upk.exports[mic_exp_idx];
    let mut mic_slice = upk.file_bytes[exp.serial_offset..exp.serial_offset + exp.serial_size].to_vec();
    let props = find_export_properties(upk, exp, &upk.file_bytes);
    let tp = props
        .iter()
        .find(|p| p.name == "TextureParameterValues" && p.tag_type == "ArrayProperty" && p.size >= 4)?;
    let arr_rel = tp.value_offset;
    let mut cnt = read_i32_le(&mic_slice, arr_rel).clamp(0, 64) as usize;
    let mut array_data_len = tp.size;
    let mut rel = 4usize;
    let mut elem_tmpl: Option<Vec<u8>> = None;
    let mut updated_names: HashSet<String> = HashSet::new();
    let mut touched = false;

    for _ in 0..cnt {
        let slice = &mic_slice[arr_rel..arr_rel + array_data_len];
        let (next_rel, elem_props, ended) = parse_property_stream(upk, slice, rel);
        if !ended && elem_props.is_empty() {
            break;
        }
        if elem_tmpl.is_none() && next_rel > rel {
            elem_tmpl = Some(slice[rel..next_rel].to_vec());
        }
        let pname = elem_props
            .iter()
            .find(|ep| ep.name == "ParameterName" && ep.size >= 8)
            .map(|pn| upk.resolve_name(read_i32_le(&mic_slice, arr_rel + pn.value_offset)).to_string())
            .unwrap_or_default();
        if let Some(&(_, target_exp1)) = desired_params.iter().find(|(want, _)| want.eq_ignore_ascii_case(&pname)) {
            if let Some(pv) = elem_props.iter().find(|ep| ep.name == "ParameterValue" && ep.size == 4) {
                write_i32_le(&mut mic_slice, arr_rel + pv.value_offset, target_exp1);
                updated_names.insert(pname.to_lowercase());
                touched = true;
            }
        }
        rel = next_rel;
    }

    let missing: Vec<(&str, i32)> = desired_params
        .iter()
        .copied()
        .filter(|(want, _)| !updated_names.contains(&want.to_lowercase()))
        .collect();

    if !missing.is_empty() {
        if let Some(tmpl) = elem_tmpl {
            let (_, tmpl_props, ok) = parse_property_stream(upk, &tmpl, 0);
            if ok {
                let pn_off = tmpl_props.iter().find(|ep| ep.name == "ParameterName").map(|ep| ep.value_offset);
                let pv_off = tmpl_props.iter().find(|ep| ep.name == "ParameterValue").map(|ep| ep.value_offset);
                let guid_off = tmpl_props.iter().find(|ep| ep.name == "ExpressionGUID" && ep.size == 16).map(|ep| ep.value_offset);

                if let (Some(pn_val_off), Some(pv_val_off)) = (pn_off, pv_off) {
                    let insert_pos = arr_rel + array_data_len;
                    let mut extra = Vec::with_capacity(missing.len() * tmpl.len());
                    for (want_nm, target_exp1) in missing {
                        let nm_idx = resolve_or_add_name(upk, added_names, want_nm);
                        let mut elem = tmpl.clone();
                        write_i32_le(&mut elem, pn_val_off, nm_idx);
                        write_i32_le(&mut elem, pn_val_off + 4, 0);
                        write_i32_le(&mut elem, pv_val_off, target_exp1);
                        if let Some(go) = guid_off {
                            elem[go..go + 16].fill(0);
                        }
                        extra.extend_from_slice(&elem);
                        cnt += 1;
                        array_data_len += elem.len();
                        touched = true;
                    }
                    mic_slice.splice(insert_pos..insert_pos, extra);
                    write_i32_le(&mut mic_slice, arr_rel, cnt as i32);
                    write_i32_le(&mut mic_slice, arr_rel - 8, array_data_len as i32);
                }
            }
        }
    }

    if touched {
        Some(mic_slice)
    } else {
        None
    }
}

fn find_mic_tex_param_exp(upk: &UnpackedUpk, mic_idx: usize, param_name: &str) -> Option<usize> {
    let exp = upk.exports.get(mic_idx)?;
    let props = find_export_properties(upk, exp, &upk.file_bytes);
    let tp = props.iter().find(|p| p.name == "TextureParameterValues" && p.tag_type == "ArrayProperty" && p.size >= 4)?;
    let arr_abs = exp.serial_offset + tp.value_offset;
    let cnt = read_i32_le(&upk.file_bytes, arr_abs).clamp(0, 64) as usize;
    let mut rel = 4usize;
    for _ in 0..cnt {
        let slice = &upk.file_bytes[arr_abs..arr_abs + tp.size];
        let (next_rel, elem_props, ended) = parse_property_stream(upk, slice, rel);
        if !ended && elem_props.is_empty() {
            break;
        }
        let pname = elem_props
            .iter()
            .find(|ep| ep.name == "ParameterName" && ep.size >= 8)
            .map(|pn| upk.resolve_name(read_i32_le(&upk.file_bytes, arr_abs + pn.value_offset)))
            .unwrap_or("");
        if pname.eq_ignore_ascii_case(param_name) {
            if let Some(pv) = elem_props.iter().find(|ep| ep.name == "ParameterValue" && ep.size == 4) {
                let obj_ref = read_i32_le(&upk.file_bytes, arr_abs + pv.value_offset);
                if obj_ref > 0 {
                    let eidx = (obj_ref - 1) as usize;
                    if eidx < upk.exports.len() && upk.export_class_name(&upk.exports[eidx]) == "Texture2D" {
                        return Some(eidx);
                    }
                }
            }
        }
        rel = next_rel;
    }
    None
}

fn composite_legacy_skin_over_diffuse(base: &RgbaImage, skin: &RgbaImage) -> RgbaImage {
    let w = base.width().max(skin.width());
    let h = base.height().max(skin.height());
    let mut out = if base.width() == w && base.height() == h {
        base.clone()
    } else {
        DynamicImage::ImageRgba8(base.clone())
            .resize_exact(w, h, FilterType::Lanczos3)
            .to_rgba8()
    };
    let s_resized = if skin.width() == w && skin.height() == h {
        skin.clone()
    } else {
        DynamicImage::ImageRgba8(skin.clone())
            .resize_exact(w, h, FilterType::Lanczos3)
            .to_rgba8()
    };
    for (dst, src) in out.pixels_mut().zip(s_resized.pixels()) {
        let max_c = src[0].max(src[1]).max(src[2]);
        let strength = ((max_c as f32 / 255.0) * (src[3] as f32 / 255.0)).clamp(0.0, 1.0);
        if strength > 0.06 {
            for c in 0..3 {
                let blended = (dst[c] as f32 * (1.0 - strength) + src[c] as f32 * strength).clamp(0.0, 255.0);
                dst[c] = blended as u8;
            }
        }
    }
    out
}

/// Injects an AlphaConsole decal pack (Diffuse + `1_Diffuse_Skin` + optional `2_Diffuse_Skin_Mask` + Chassis)
/// into any target decal UPK cleanly: natively in-place whenever the target shader matches (`8273`/esports,
/// `leaves`, OR legacy `MIC_Body_Octane` `Diffuse`+`Skin`), and single-pass pristine-donor rewiring otherwise.
pub fn inject_alphaconsole_decal(
    cooked_dir: &Path,
    target_decal_pkg: &str,
    pack: &AlphaConsoleDecalPack,
    preferred_donor_pkg: Option<&str>,
) -> Result<String, String> {
    inject_alphaconsole_decal_at(cooked_dir, target_decal_pkg, None, pack, preferred_donor_pkg)
}

/// Variant of `inject_alphaconsole_decal` allowing an explicit `ProductAsset_Skin_TA` `target_asset_path`
/// for multi-skin packages (e.g. `body_octane_premium_skins_SF.upk`).
pub fn inject_alphaconsole_decal_at(
    cooked_dir: &Path,
    target_decal_pkg: &str,
    target_asset_path_opt: Option<&str>,
    pack: &AlphaConsoleDecalPack,
    preferred_donor_pkg: Option<&str>,
) -> Result<String, String> {
    let target_path = cooked_dir.join(target_decal_pkg);
    if !target_path.exists() {
        return Err(format!("Target decal package not found: {}", target_path.display()));
    }
    if pack.skin_img.is_none() && pack.diffuse_base_img.is_none() && pack.mask_img.is_none() {
        return Err("No decal texture supplied".into());
    }

    ensure_pristine_before_swap(&target_path)?;
    let bak_p = bak_path_for(&target_path);
    let pristine_path = if bak_p.is_file() { bak_p } else { target_path.clone() };
    let (mut upk, mut orig_raw) = unpack_upk_from_disk(&pristine_path)?;

    let collect_skin_mics = |u: &UnpackedUpk| -> Vec<usize> {
        u.exports
            .iter()
            .enumerate()
            .filter(|(idx, exp)| {
                if u.export_class_name(exp) != "MaterialInstanceConstant" {
                    return false;
                }
                let full = u.full_export_path(*idx + 1).to_lowercase();
                let obj = u.resolve_name(exp.object_name).to_lowercase();
                !obj.contains("chassis") && !full.starts_with("body_")
            })
            .map(|(i, _)| i)
            .collect()
    };

    // Detect legacy `Diffuse + Skin` packs (e.g. Violent Octane with SkinID: 0 and no 2_Diffuse_Skin_Mask)
    // vs modern `1_Diffuse_Skin` (+ `2_Diffuse_Skin_Mask`) packs (e.g. Itachi / Blue Lock / SoulyBlossom / PNG).
    let is_legacy_diff_skin = pack.diffuse_base_img.is_some() && pack.skin_img.is_some() && pack.mask_img.is_none();
    let initial_mics = collect_skin_mics(&upk);
    let has_legacy_skin_slot = initial_mics
        .iter()
        .any(|&m| find_mic_tex_param_exp(&upk, m, "Skin").is_some());

    // Never unnecessarily discard/rewire a target UPK whose native shader ALREADY supports what this pack uses:
    //  - Legacy `Diffuse + Skin` pack on native `MIC_Body_*` decal (e.g. Flames/Stripes/Wings) -> inject in-place.
    //  - Modern pack on native `Body_Paintable_Diffuse_Mat` with `1_Diffuse_Skin` (+ `2_Diffuse_Skin_Mask` if needed) -> inject in-place.
    let tgt_prof = inspect_decal_profile(&upk, target_asset_path_opt);
    let has_dmat = tgt_prof.as_ref().map_or(false, |p| p.has_diffuse_mat);
    let has_1s = tgt_prof.as_ref().map_or(false, |p| p.has_1skin);
    let has_2m = tgt_prof.as_ref().map_or(false, |p| p.has_2mask);
    let wants_mask = pack.mask_img.is_some();
    let native_ok = if is_legacy_diff_skin && has_legacy_skin_slot {
        true
    } else {
        has_dmat && has_1s && (!wants_mask || has_2m)
    };
    let needs_upgrade = !native_ok;

    if needs_upgrade {
        let allow_locked_pref = preferred_donor_pkg.is_some();
        if let Some((donor_path, donor_pa, donor_clean_2m)) =
            find_diffuse_donor_for_target(cooked_dir, target_decal_pkg, preferred_donor_pkg, allow_locked_pref)
        {
            let same_file = donor_path
                .file_name()
                .and_then(|n| n.to_str())
                .map_or(false, |n| n.eq_ignore_ascii_case(target_decal_pkg));
            let worth_rewire = !same_file && (!has_dmat || !has_1s || donor_clean_2m || (wants_mask && !has_2m));
            if worth_rewire {
                if let Some(target_asset_path) = tgt_prof.as_ref().map(|p| p.pa_path.clone()) {
                    let target_stem = target_path.file_stem().unwrap_or_default().to_string_lossy().to_string();
                    let (target_chain, target_sf) =
                        extract_target_fnames_from_upk(&pristine_path, &upk.key, &target_asset_path, &target_stem);
                    let target_guid = extract_package_guid(&orig_raw);
                    let donor_stem = donor_path.file_stem().unwrap_or_default().to_string_lossy().to_string();
                    let donor_raw = fs::read(&donor_path).map_err(|e| e.to_string())?;
                    let donor_pfx = parse_upk_prefix(&donor_raw)?;
                    let (_, donor_key, _) = decrypt_upk_header(&donor_path, &donor_raw, &donor_pfx)?;
                    if let Ok(rewired) = build_surgically_rewired_upk_with_guid(
                        &donor_raw,
                        &donor_key,
                        &upk.key,
                        &donor_pa,
                        &donor_stem,
                        &target_chain,
                        &target_sf,
                        target_guid,
                    ) {
                        if let Ok((new_u, new_r)) = unpack_upk_from_raw(rewired, &target_path) {
                            upk = new_u;
                            orig_raw = new_r;
                        }
                    }
                }
            }
        }
    }

    let skin_mics = collect_skin_mics(&upk);

    // Pick clean Skin_* Texture2D template export for any new/cloned inline exports
    let mut tmpl_tex_idx: Option<usize> = None;
    let mut primary_idxs = Vec::new();
    let mut fallback_idxs = Vec::new();
    for (idx, exp) in upk.exports.iter().enumerate() {
        if upk.export_class_name(exp) != "Texture2D" || exp.serial_size < 512 {
            continue;
        }
        let full_low = upk.full_export_path(idx + 1).to_lowercase();
        let obj_low = upk.resolve_name(exp.object_name).to_lowercase();
        if obj_low.ends_with("_n")
            || obj_low.contains("normal")
            || obj_low.contains("curvature")
            || obj_low.contains("blank")
            || obj_low.contains("blackalpha")
            || obj_low.contains("fxmask")
            || obj_low.contains("gradmask")
            || obj_low.contains("chassis_")
        {
            continue;
        }
        let ser = &upk.file_bytes[exp.serial_offset..exp.serial_offset + exp.serial_size];
        let (pe, _, _, _, _) = parse_tex2d_header_props(&upk, ser);
        if pe > 0 && find_tex2d_mip_array_start(ser, pe).is_some() && tmpl_tex_idx.is_none() {
            tmpl_tex_idx = Some(idx);
        }
        if full_low.starts_with("skin_") || full_low.contains(".skin_") {
            primary_idxs.push(idx);
        } else if !full_low.starts_with("body_") && !full_low.starts_with("itemsmode.") && !full_low.starts_with("boost_") {
            fallback_idxs.push(idx);
        }
    }
    let chosen_tex_fallback = if !primary_idxs.is_empty() {
        primary_idxs
    } else {
        fallback_idxs
    };
    if tmpl_tex_idx.is_none() {
        tmpl_tex_idx = chosen_tex_fallback.first().copied();
    }

    let mut replacements: Vec<(usize, Vec<u8>)> = Vec::new();
    let mut replaced_exp_set: HashSet<usize> = HashSet::new();
    let mut added_exports: Vec<(String, usize, Vec<u8>)> = Vec::new();
    let mut added_names: Vec<String> = Vec::new();

    let encode_tex_for = |tmpl_i: usize, img: &RgbaImage, force_dxt5: bool, names_out: &mut Vec<String>| -> Result<Vec<u8>, String> {
        let orig_ser = &upk.file_bytes[upk.exports[tmpl_i].serial_offset..upk.exports[tmpl_i].serial_offset + upk.exports[tmpl_i].serial_size];
        let (_, fmt, _, _, _) = parse_tex2d_header_props(&upk, orig_ser);
        let max_d = if img.width().max(img.height()) >= 1024 { 1024 } else { 512 };
        let dim = if !force_dxt5 && fmt == "PF_A8R8G8B8" { max_d.min(512) } else { max_d };
        Ok(build_inline_texture2d_serial_with_names(&upk, orig_ser, img, dim, dim, force_dxt5, names_out)?.0)
    };

    let is_diffuse_mat = !skin_mics.is_empty() && tmpl_tex_idx.is_some() && upk.find_name_index("1_Diffuse_Skin").is_some();
    let is_legacy_mic = !is_diffuse_mat && !skin_mics.is_empty() && tmpl_tex_idx.is_some() && has_legacy_skin_slot;

    if is_diffuse_mat || is_legacy_mic {
        let tmpl_i = tmpl_tex_idx.unwrap();
        let mut mic_param_targets: Vec<(&str, i32)> = Vec::new();
        let primary_slot = if is_diffuse_mat { "1_Diffuse_Skin" } else { "Skin" };

        let effective_primary: Option<RgbaImage> = if is_diffuse_mat && is_legacy_diff_skin {
            match (&pack.diffuse_base_img, &pack.skin_img) {
                (Some(d), Some(s)) if d.as_raw() != s.as_raw() => Some(composite_legacy_skin_over_diffuse(d, s)),
                (Some(d), _) => Some(d.clone()),
                (_, s) => s.clone(),
            }
        } else {
            pack.skin_img.clone()
        };

        // Synthesize a clean alpha/primary mask when injecting onto a 2_Diffuse_Skin_Mask shader without
        // a mask image so the donor's original RGB pattern never paints over the user's custom decal.
        let synthesized_mask: Option<RgbaImage> = if is_diffuse_mat && pack.mask_img.is_none() {
            effective_primary.as_ref().and_then(|s| {
                let has_mask_slot = skin_mics
                    .iter()
                    .any(|&m| find_mic_tex_param_exp(&upk, m, "2_Diffuse_Skin_Mask").is_some());
                if has_mask_slot {
                    let mut auto_m = RgbaImage::new(s.width(), s.height());
                    for (px, out_px) in s.pixels().zip(auto_m.pixels_mut()) {
                        let a = px.0[3];
                        out_px.0 = [255 - a, 0, 0, a];
                    }
                    Some(auto_m)
                } else {
                    None
                }
            })
        } else {
            None
        };
        let effective_mask = if is_diffuse_mat {
            pack.mask_img.as_ref().or(synthesized_mask.as_ref())
        } else {
            None
        };

        let mut bind_slot = |param_name: &'static str,
                             img: &RgbaImage,
                             added_name: &str,
                             force_dxt5: bool,
                             reuse_exp1: Option<i32>|
         -> Result<i32, String> {
            if let Some(ei) = reuse_exp1 {
                mic_param_targets.push((param_name, ei));
                return Ok(ei);
            }
            let existing = skin_mics.iter().find_map(|&m| find_mic_tex_param_exp(&upk, m, param_name));
            let usable = existing.filter(|&ei| {
                if param_name == "2_Diffuse_Skin_Mask" {
                    !replaced_exp_set.contains(&ei)
                        && !upk.resolve_name(upk.exports[ei].object_name).to_lowercase().contains("blackalpha")
                } else {
                    param_name == primary_slot || !replaced_exp_set.contains(&ei)
                }
            });
            let target_idx = if param_name == primary_slot {
                usable.or_else(|| chosen_tex_fallback.first().copied())
            } else {
                usable
            };
            let exp1 = if let Some(ei) = target_idx {
                if replaced_exp_set.insert(ei) {
                    let ser = encode_tex_for(ei, img, force_dxt5, &mut added_names)?;
                    replacements.push((ei, ser));
                }
                (ei + 1) as i32
            } else {
                let idx1 = (upk.exports.len() + added_exports.len() + 1) as i32;
                let ser = encode_tex_for(tmpl_i, img, force_dxt5, &mut added_names)?;
                added_exports.push((added_name.into(), tmpl_i, ser));
                idx1
            };
            mic_param_targets.push((param_name, exp1));
            Ok(exp1)
        };

        let mut primary_exp1 = None;
        if let Some(ref p_img) = effective_primary {
            primary_exp1 = Some(bind_slot(primary_slot, p_img, "RLSwapper_AC_Skin", is_diffuse_mat, None)?);
        }
        if let Some(ref d_img) = pack.diffuse_base_img {
            let reuse = primary_exp1
                .filter(|_| effective_primary.as_ref().map_or(false, |s| s.as_raw() == d_img.as_raw()));
            let _ = bind_slot("Diffuse", d_img, "RLSwapper_AC_Diffuse", false, reuse)?;
        }
        if let Some(m_img) = effective_mask {
            let _ = bind_slot("2_Diffuse_Skin_Mask", m_img, "RLSwapper_AC_Mask", true, None)?;
        }

        if !mic_param_targets.is_empty() {
            for &mic_i in &skin_mics {
                if let Some(new_mic) = upsert_mic_texture_params(&upk, mic_i, &mic_param_targets, &mut added_names) {
                    if replaced_exp_set.insert(mic_i) {
                        replacements.push((mic_i, new_mic));
                    }
                }
            }
        }
    }

    if let Some(ref ch_img) = pack.chassis_diffuse_img {
        for (idx, exp) in upk.exports.iter().enumerate() {
            if upk.export_class_name(exp) != "Texture2D" || replaced_exp_set.contains(&idx) {
                continue;
            }
            let full = upk.full_export_path(idx + 1).to_lowercase();
            if full.contains("chassis_") && (full.ends_with("_d") || full.contains("diffuse")) {
                if let Ok(ser) = encode_tex_for(idx, ch_img, false, &mut added_names) {
                    replaced_exp_set.insert(idx);
                    replacements.push((idx, ser));
                }
            }
        }
    }

    // Legacy single-image decals (texture-only package without MIC parameters)
    if replacements.is_empty() && added_exports.is_empty() {
        if let Some(ref s_img) = pack.skin_img.as_ref().or(pack.diffuse_base_img.as_ref()) {
            let target_dim = if s_img.width() >= 768 { 1024 } else { 512 };
            for idx in &chosen_tex_fallback {
                let exp = &upk.exports[*idx];
                let orig_serial = &upk.file_bytes[exp.serial_offset..exp.serial_offset + exp.serial_size];
                let (_, fmt, _, _, _) = parse_tex2d_header_props(&upk, orig_serial);
                let dim = if fmt == "PF_A8R8G8B8" { target_dim.min(512) } else { target_dim };
                if let Ok((new_serial, _, _)) =
                    build_inline_texture2d_serial_with_names(&upk, orig_serial, s_img, dim, dim, false, &mut added_names)
                {
                    replacements.push((*idx, new_serial));
                }
            }
        }
    }

    if replacements.is_empty() && added_exports.is_empty() {
        let fallback_img = pack.skin_img.as_ref().or(pack.diffuse_base_img.as_ref()).ok_or("No decal image")?;
        let mut mod_bytes = upk.file_bytes.clone();
        let p = patch_any_inline_texture_mips(&upk, &mut mod_bytes, fallback_img);
        if p == 0 {
            return Err(format!("No injectable decal Texture2D found inside {}", target_decal_pkg));
        }
        rebuild_upk_selective_chunks(&orig_raw, &upk, &mod_bytes, &upk.key, &target_path)?;
        clear_rl_texture_cache();
        return Ok(format!("Injected '{}' into {} ({} mips)", pack.name, target_decal_pkg, p));
    }

    let built = rebuild_upk_with_mod_resized_added_and_names_to_vec(
        &orig_raw,
        &upk,
        &upk.file_bytes,
        &replacements,
        &added_exports,
        &added_names,
        &upk.key,
    )?;
    fs::write(&target_path, built).map_err(|e| e.to_string())?;
    clear_rl_texture_cache();

    let layer_cnt = [pack.skin_img.is_some(), pack.mask_img.is_some(), pack.diffuse_base_img.is_some()]
        .into_iter()
        .filter(|&b| b)
        .count();
    Ok(format!(
        "Injected '{}' ({} layer(s)) into {}",
        pack.name, layer_cnt, target_decal_pkg
    ))
}

/// Injects `user_decal_img` directly into `target_decal_pkg` (convenience wrapper over `inject_alphaconsole_decal`).
pub fn inject_custom_decal(
    cooked_dir: &Path,
    target_decal_pkg: &str,
    user_decal_img: &RgbaImage,
) -> Result<String, String> {
    let pack = AlphaConsoleDecalPack {
        name: "Custom Decal".into(),
        body_id: None,
        skin_id: None,
        diffuse_base_img: None,
        skin_img: Some(user_decal_img.clone()),
        mask_img: None,
        chassis_diffuse_img: None,
        source_label: "Custom PNG".into(),
    };
    inject_alphaconsole_decal(cooked_dir, target_decal_pkg, &pack, None)
}

fn patch_any_inline_texture_mips(
    upk: &UnpackedUpk,
    buf: &mut [u8],
    user_img: &RgbaImage,
) -> usize {
    let mut patched = 0usize;
    for exp in &upk.exports {
        if upk.export_class_name(exp) != "Texture2D" || exp.serial_size < 2048 {
            continue;
        }
        let obj_nm = upk.resolve_name(exp.object_name).to_lowercase();
        if obj_nm.ends_with("_n") || obj_nm.contains("curvature") || obj_nm.contains("blank") {
            continue;
        }
        patched += patch_texture2d_export_mips(buf, exp.serial_offset, exp.serial_size, user_img, 32);
    }
    patched
}

/// Scans a `Texture2D` export serial slice byte-by-byte for inline mip blocks
/// (`(sz, sz) + 4B + pixels + (sx, sy)`) and replaces them with `user_img` in `BGRA8` or `DXT5`.
pub fn patch_texture2d_export_mips(
    buf: &mut [u8],
    serial_offset: usize,
    serial_size: usize,
    user_img: &RgbaImage,
    min_dim: u32,
) -> usize {
    let end = (serial_offset + serial_size).min(buf.len());
    let mut patched = 0usize;
    let mut pos = serial_offset;

    while pos + 20 <= end {
        let sz1 = read_u32_le(buf, pos) as usize;
        let sz2 = read_u32_le(buf, pos + 4) as usize;
        if sz1 == sz2 && (1024..=16_777_216).contains(&sz1) && (sz1 & (sz1 - 1)) == 0 {
            let mut matched_pad = None;
            // Inline FByteBulkData header has elem+disk = 8 bytes (pos += 8 to reach pixels); fallback 12/16
            for pad in [8usize, 12, 16] {
                if pos + pad + sz1 + 8 <= end {
                    let sx = read_u32_le(buf, pos + pad + sz1);
                    let sy = read_u32_le(buf, pos + pad + sz1 + 4);
                    if sx >= min_dim && sy >= min_dim && sx <= 4096 && sy <= 4096 {
                        let px_cnt = (sx as usize) * (sy as usize);
                        let dxt5_sz = (((sx as usize) + 3) / 4) * (((sy as usize) + 3) / 4) * 16;
                        if sz1 == px_cnt * 4 || sz1 == dxt5_sz {
                            matched_pad = Some((pad, sx, sy, sz1 == px_cnt * 4));
                            break;
                        }
                    }
                }
            }
            if let Some((pad, sx, sy, is_bgra)) = matched_pad {
                let resized = DynamicImage::ImageRgba8(user_img.clone())
                    .resize_exact(sx, sy, FilterType::Lanczos3)
                    .to_rgba8();
                let encoded = if is_bgra {
                    encode_bgra8(&resized)
                } else {
                    encode_dxt5(&resized)
                };
                if encoded.len() == sz1 {
                    let dst = pos + pad;
                    buf[dst..dst + sz1].copy_from_slice(&encoded);
                    patched += 1;
                    pos = dst + sz1 + 8;
                    continue;
                }
            }
        }
        pos += 1;
    }
    patched
}

// ── 4. Custom Match Ball Texture & AlphaConsole Injector (`GameInfo_Soccar_SF.upk`) ──

/// Injects an AlphaConsole Match Ball pack (`Diffuse` -> `Ball_Default00_D`, optional `Normal` ->
/// `Ball_Default00_N`, optional `Masks` -> `Ball_Default00_RGB`) into `GameInfo_Soccar_SF.upk`.
pub fn inject_alphaconsole_ball(
    cooked_dir: &Path,
    pack: &AlphaConsoleBallPack,
) -> Result<String, String> {
    let target_path = cooked_dir.join("GameInfo_Soccar_SF.upk");
    if !target_path.exists() {
        return Err(format!("GameInfo_Soccar_SF.upk not found in {}", cooked_dir.display()));
    }

    ensure_pristine_before_swap(&target_path)?;
    let bak_p = bak_path_for(&target_path);
    let pristine = if bak_p.is_file() { bak_p } else { target_path.clone() };
    let (upk, orig_raw) = unpack_upk_from_disk(&pristine)?;

    let find_ball_tex = |suffix: &str| -> Option<usize> {
        upk.exports.iter().position(|e| {
            upk.export_class_name(e) == "Texture2D"
                && upk.full_export_path(e.index).eq_ignore_ascii_case(suffix)
        })
    };

    let diff_idx = find_ball_tex("Ball_Default_Textures.Textures.Ball_Default00_D")
        .ok_or("Could not locate Ball_Default00_D export in GameInfo_Soccar_SF.upk")?;

    let mut replacements: Vec<(usize, Vec<u8>)> = Vec::new();
    let mut added_names: Vec<String> = Vec::new();
    let d_dim = if pack.diffuse_img.width().max(pack.diffuse_img.height()) >= 1024 { 1024 } else { 512 };
    let diff_ser = &upk.file_bytes[upk.exports[diff_idx].serial_offset..upk.exports[diff_idx].serial_offset + upk.exports[diff_idx].serial_size];
    let (new_diff, d_mips, _) =
        build_inline_texture2d_serial_with_names(&upk, diff_ser, &pack.diffuse_img, d_dim, d_dim, false, &mut added_names)?;
    replacements.push((diff_idx, new_diff));

    let mut injected_layers = vec!["Diffuse"];
    if let Some(ref norm_img) = pack.normal_img {
        if let Some(norm_idx) = find_ball_tex("Ball_Default_Textures.Textures.Ball_Default00_N") {
            let n_dim = if norm_img.width().max(norm_img.height()) >= 1024 { 1024 } else { 512 };
            let n_ser = &upk.file_bytes[upk.exports[norm_idx].serial_offset..upk.exports[norm_idx].serial_offset + upk.exports[norm_idx].serial_size];
            if let Ok((new_norm, _, _)) =
                build_inline_texture2d_serial_with_names(&upk, n_ser, norm_img, n_dim, n_dim, false, &mut added_names)
            {
                replacements.push((norm_idx, new_norm));
                injected_layers.push("Normal");
            }
        }
    }
    if let Some(ref mask_img) = pack.mask_img {
        if let Some(mask_idx) = find_ball_tex("Ball_Default_Textures.Textures.Ball_Default00_RGB") {
            let m_dim = if mask_img.width().max(mask_img.height()) >= 1024 { 1024 } else { 512 };
            let m_ser = &upk.file_bytes[upk.exports[mask_idx].serial_offset..upk.exports[mask_idx].serial_offset + upk.exports[mask_idx].serial_size];
            if let Ok((new_mask, _, _)) =
                build_inline_texture2d_serial_with_names(&upk, m_ser, mask_img, m_dim, m_dim, false, &mut added_names)
            {
                replacements.push((mask_idx, new_mask));
                injected_layers.push("Masks");
            }
        }
    }

    let built = rebuild_upk_with_mod_resized_added_and_names_to_vec(
        &orig_raw,
        &upk,
        &upk.file_bytes,
        &replacements,
        &[],
        &added_names,
        &upk.key,
    )?;
    fs::write(&target_path, built).map_err(|e| e.to_string())?;
    clear_rl_texture_cache();
    Ok(format!(
        "Injected '{}' Ball ({}, {} mips) into GameInfo_Soccar_SF.upk",
        pack.name,
        injected_layers.join(" + "),
        d_mips
    ))
}

/// Injects a single custom Diffuse Ball image into `GameInfo_Soccar_SF.upk`.
pub fn inject_custom_ball_texture(
    cooked_dir: &Path,
    user_ball_img: &RgbaImage,
) -> Result<String, String> {
    let pack = AlphaConsoleBallPack {
        name: "Custom Ball".into(),
        group: String::new(),
        diffuse_img: user_ball_img.clone(),
        normal_img: None,
        mask_img: None,
        source_label: "Custom PNG".into(),
    };
    inject_alphaconsole_ball(cooked_dir, &pack)
}

// ── 5. Universal Diffuse Texture Tint Fallback (For Texture-Only Items) ──────

fn scale_pow2_capped(sx: u32, sy: u32, max_dim: u32) -> (u32, u32) {
    let mut w = sx.max(1).next_power_of_two();
    let mut h = sy.max(1).next_power_of_two();
    while w > max_dim || h > max_dim {
        w = (w / 2).max(1);
        h = (h / 2).max(1);
    }
    (w, h)
}

fn is_non_diffuse_tex_name(full_low: &str) -> bool {
    full_low.ends_with("_n")
        || full_low.contains("_n.")
        || full_low.contains("normal")
        || full_low.contains("mask")
        || full_low.contains("blank")
        || full_low.contains("thumb")
        || full_low.contains("_orm")
        || full_low.contains("_spec")
        || full_low.contains("_rough")
        || full_low.contains("_metal")
        || full_low.contains("_ao")
}

/// Luminance-preserving tint toward target sRGB color `[r, g, b]` while preserving alpha and dark
/// shadows (like rubber tires) so texture-only wheels, toppers, antennas, and decals stay shaded cleanly.
pub fn tint_rgba_luminance(src: &RgbaImage, target_rgb: [u8; 3]) -> RgbaImage {
    let mut out = src.clone();
    let tr = (target_rgb[0] as f32).max(18.0) / 255.0;
    let tg = (target_rgb[1] as f32).max(18.0) / 255.0;
    let tb = (target_rgb[2] as f32).max(18.0) / 255.0;
    for px in out.pixels_mut() {
        let [r, g, b, a] = px.0;
        let lum = (0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32) / 255.0;
        let shade = lum.powf(0.82);
        let weight = ((lum - 0.08) / 0.16).clamp(0.35, 1.0);
        let nr = ((r as f32 * (1.0 - weight)) + (tr * shade * 255.0 * weight)).clamp(0.0, 255.0) as u8;
        let ng = ((g as f32 * (1.0 - weight)) + (tg * shade * 255.0 * weight)).clamp(0.0, 255.0) as u8;
        let nb = ((b as f32 * (1.0 - weight)) + (tb * shade * 255.0 * weight)).clamp(0.0, 255.0) as u8;
        px.0 = [nr, ng, nb, a];
    }
    out
}

/// Decodes diffuse `Texture2D` exports (`.tfc` or inline) of an item package, applies `color` tinting,
/// and returns `(replacements, added_names)` using `NeverStream=1` + alpha-safe `PF_DXT5` inline blobs.
pub fn build_tinted_texture_replacements_with_names(
    upk: &UnpackedUpk,
    cooked_dir: &Path,
    color: &PaintColorSpec,
) -> (Vec<(usize, Vec<u8>)>, Vec<String>) {
    let mut preferred = Vec::new();
    let mut secondary = Vec::new();

    for (idx, exp) in upk.exports.iter().enumerate() {
        if upk.export_class_name(exp) != "Texture2D" || exp.serial_offset + exp.serial_size > upk.file_bytes.len() {
            continue;
        }
        let full = upk.full_export_path(idx + 1).to_ascii_lowercase();
        if is_non_diffuse_tex_name(&full) {
            continue;
        }
        let orig_serial = &upk.file_bytes[exp.serial_offset..exp.serial_offset + exp.serial_size];
        let (_, _, _, sx, sy) = parse_tex2d_header_props(upk, orig_serial);
        if sx < 16 || sy < 16 {
            continue;
        }
        let explicit_diff = full.ends_with("_d")
            || full.contains("_d.")
            || full.contains("diff")
            || full.contains("_rgb")
            || full.contains("albedo")
            || full.contains("color");
        if explicit_diff {
            preferred.push(idx);
        } else {
            secondary.push(idx);
        }
    }

    let targets = if !preferred.is_empty() { preferred } else { secondary };
    let target_rgb = color.rgb8();
    let mut reps = Vec::new();
    let mut added_names = Vec::new();
    for idx in targets {
        let Some(img) = decode_texture2d_export(upk, idx, Some(cooked_dir)) else {
            continue;
        };
        let exp = &upk.exports[idx];
        let orig_serial = &upk.file_bytes[exp.serial_offset..exp.serial_offset + exp.serial_size];
        let tinted = tint_rgba_luminance(&img, target_rgb);
        let (tw, th) = scale_pow2_capped(img.width(), img.height(), 512);
        if let Ok((new_ser, _, _)) =
            build_inline_texture2d_serial_with_names(upk, orig_serial, &tinted, tw, th, false, &mut added_names)
        {
            reps.push((idx, new_ser));
        }
    }
    (reps, added_names)
}

/// Convenience wrapper returning only replacement `(exp_idx, serial_bytes)` pairs.
pub fn build_tinted_texture_replacements(
    upk: &UnpackedUpk,
    cooked_dir: &Path,
    color: &PaintColorSpec,
) -> Vec<(usize, Vec<u8>)> {
    build_tinted_texture_replacements_with_names(upk, cooked_dir, color).0
}
