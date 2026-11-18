//! Native Rust Item & Stadium Icon Extraction Engine (`*_T_SF.upk`, `.tfc`, DXT1/DXT5/BGRA8)
//! and Fixed MediaWiki `pageimages` + Search Online Icon Scraper.

use crate::upk_engine::{
    decompress_all_upk_chunks, decompress_chunk_payload, get_appdata_dir, read_i32_le, read_i64_le,
    read_u32_le, unpack_upk_from_disk, CatalogItem, PaintColorSpec, PACKAGE_FILE_TAG,
};
use image::{imageops, imageops::FilterType, Rgba, RgbaImage};
use rayon::prelude::*;
use std::collections::{HashMap, VecDeque};
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime};

fn shared_wiki_client() -> Option<&'static reqwest::blocking::Client> {
    static CLIENT: OnceLock<Option<reqwest::blocking::Client>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::blocking::Client::builder()
                .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36")
                .timeout(Duration::from_secs(6))
                .build()
                .ok()
        })
        .as_ref()
}

// ── Pure-Rust DXT1 (BC1), DXT5 (BC3), and BGRA8 Decoder & Encoder ────────────

#[inline]
fn rgb565_to_rgb8(val: u16) -> [u8; 3] {
    let r = ((val >> 11) & 0x1F) as u32;
    let g = ((val >> 5) & 0x3F) as u32;
    let b = (val & 0x1F) as u32;
    [
        ((r * 255 + 15) / 31) as u8,
        ((g * 255 + 31) / 63) as u8,
        ((b * 255 + 15) / 31) as u8,
    ]
}

#[inline]
fn rgb8_to_rgb565(r: u8, g: u8, b: u8) -> u16 {
    let r5 = ((r as u16) >> 3) & 0x1F;
    let g6 = ((g as u16) >> 2) & 0x3F;
    let b5 = ((b as u16) >> 3) & 0x1F;
    (r5 << 11) | (g6 << 5) | b5
}

pub fn decode_bgra8(width: u32, height: u32, data: &[u8]) -> Option<RgbaImage> {
    let needed = (width as usize) * (height as usize) * 4;
    if data.len() < needed {
        return None;
    }
    let mut out = vec![0u8; needed];
    for i in 0..(width as usize * height as usize) {
        out[i * 4] = data[i * 4 + 2];
        out[i * 4 + 1] = data[i * 4 + 1];
        out[i * 4 + 2] = data[i * 4];
        out[i * 4 + 3] = data[i * 4 + 3];
    }
    RgbaImage::from_raw(width, height, out)
}

pub fn encode_bgra8(img: &RgbaImage) -> Vec<u8> {
    let raw = img.as_raw();
    let mut out = vec![0u8; raw.len()];
    for i in 0..(raw.len() / 4) {
        out[i * 4] = raw[i * 4 + 2];
        out[i * 4 + 1] = raw[i * 4 + 1];
        out[i * 4 + 2] = raw[i * 4];
        out[i * 4 + 3] = raw[i * 4 + 3];
    }
    out
}

pub fn decode_dxt1(width: u32, height: u32, data: &[u8]) -> Option<RgbaImage> {
    let bw = ((width + 3) / 4) as usize;
    let bh = ((height + 3) / 4) as usize;
    if data.len() < bw * bh * 8 {
        return None;
    }
    let mut img = RgbaImage::new(width, height);
    let mut off = 0usize;
    for by in 0..bh {
        for bx in 0..bw {
            let c0 = u16::from_le_bytes([data[off], data[off + 1]]);
            let c1 = u16::from_le_bytes([data[off + 2], data[off + 3]]);
            let bits = u32::from_le_bytes([data[off + 4], data[off + 5], data[off + 6], data[off + 7]]);
            off += 8;

            let rgb0 = rgb565_to_rgb8(c0);
            let rgb1 = rgb565_to_rgb8(c1);
            let mut palette = [[0u8; 4]; 4];
            palette[0] = [rgb0[0], rgb0[1], rgb0[2], 255];
            palette[1] = [rgb1[0], rgb1[1], rgb1[2], 255];
            if c0 > c1 {
                for ch in 0..3 {
                    palette[2][ch] = ((2 * rgb0[ch] as u16 + rgb1[ch] as u16) / 3) as u8;
                    palette[3][ch] = ((rgb0[ch] as u16 + 2 * rgb1[ch] as u16) / 3) as u8;
                }
                palette[2][3] = 255;
                palette[3][3] = 255;
            } else {
                for ch in 0..3 {
                    palette[2][ch] = ((rgb0[ch] as u16 + rgb1[ch] as u16) / 2) as u8;
                }
                palette[2][3] = 255;
                palette[3] = [0, 0, 0, 0];
            }

            for py in 0..4 {
                let y = (by * 4 + py) as u32;
                if y >= height {
                    continue;
                }
                for px in 0..4 {
                    let x = (bx * 4 + px) as u32;
                    if x >= width {
                        continue;
                    }
                    let idx = ((bits >> ((py * 4 + px) * 2)) & 0x3) as usize;
                    img.put_pixel(x, y, Rgba(palette[idx]));
                }
            }
        }
    }
    Some(img)
}

pub fn decode_dxt5(width: u32, height: u32, data: &[u8]) -> Option<RgbaImage> {
    let bw = ((width + 3) / 4) as usize;
    let bh = ((height + 3) / 4) as usize;
    if data.len() < bw * bh * 16 {
        return None;
    }
    let mut img = RgbaImage::new(width, height);
    let mut off = 0usize;
    for by in 0..bh {
        for bx in 0..bw {
            let a0 = data[off];
            let a1 = data[off + 1];
            let mut a_bits = 0u64;
            for i in 0..6 {
                a_bits |= (data[off + 2 + i] as u64) << (i * 8);
            }
            let mut alphas = [0u8; 8];
            alphas[0] = a0;
            alphas[1] = a1;
            if a0 > a1 {
                for i in 1..=6 {
                    alphas[i + 1] = (((7 - i as u16) * a0 as u16 + (i as u16) * a1 as u16) / 7) as u8;
                }
            } else {
                for i in 1..=4 {
                    alphas[i + 1] = (((5 - i as u16) * a0 as u16 + (i as u16) * a1 as u16) / 5) as u8;
                }
                alphas[6] = 0;
                alphas[7] = 255;
            }

            let c0 = u16::from_le_bytes([data[off + 8], data[off + 9]]);
            let c1 = u16::from_le_bytes([data[off + 10], data[off + 11]]);
            let c_bits = u32::from_le_bytes([
                data[off + 12],
                data[off + 13],
                data[off + 14],
                data[off + 15],
            ]);
            off += 16;

            let rgb0 = rgb565_to_rgb8(c0);
            let rgb1 = rgb565_to_rgb8(c1);
            let mut colors = [[0u8; 3]; 4];
            colors[0] = rgb0;
            colors[1] = rgb1;
            for ch in 0..3 {
                colors[2][ch] = ((2 * rgb0[ch] as u16 + rgb1[ch] as u16) / 3) as u8;
                colors[3][ch] = ((rgb0[ch] as u16 + 2 * rgb1[ch] as u16) / 3) as u8;
            }

            for py in 0..4 {
                let y = (by * 4 + py) as u32;
                if y >= height {
                    continue;
                }
                for px in 0..4 {
                    let x = (bx * 4 + px) as u32;
                    if x >= width {
                        continue;
                    }
                    let p_idx = py * 4 + px;
                    let a_idx = ((a_bits >> (p_idx * 3)) & 0x7) as usize;
                    let c_idx = ((c_bits >> (p_idx * 2)) & 0x3) as usize;
                    let c = colors[c_idx];
                    img.put_pixel(x, y, Rgba([c[0], c[1], c[2], alphas[a_idx]]));
                }
            }
        }
    }
    Some(img)
}

#[inline]
fn encode_dxt1_color_block(block: &[[u8; 4]; 16], min_rgb: [u8; 3], max_rgb: [u8; 3]) -> [u8; 8] {
    let mut out = [0u8; 8];
    let mut c0 = rgb8_to_rgb565(max_rgb[0], max_rgb[1], max_rgb[2]);
    let mut c1 = rgb8_to_rgb565(min_rgb[0], min_rgb[1], min_rgb[2]);
    if c0 < c1 {
        std::mem::swap(&mut c0, &mut c1);
    } else if c0 == c1 {
        if c0 < 0xFFFF {
            c0 += 1;
        } else {
            c1 = c1.saturating_sub(1);
        }
    }
    out[0..2].copy_from_slice(&c0.to_le_bytes());
    out[2..4].copy_from_slice(&c1.to_le_bytes());

    let rgb0 = rgb565_to_rgb8(c0);
    let rgb1 = rgb565_to_rgb8(c1);
    let pal = [
        rgb0,
        rgb1,
        [
            ((2 * rgb0[0] as u16 + rgb1[0] as u16) / 3) as u8,
            ((2 * rgb0[1] as u16 + rgb1[1] as u16) / 3) as u8,
            ((2 * rgb0[2] as u16 + rgb1[2] as u16) / 3) as u8,
        ],
        [
            ((rgb0[0] as u16 + 2 * rgb1[0] as u16) / 3) as u8,
            ((rgb0[1] as u16 + 2 * rgb1[1] as u16) / 3) as u8,
            ((rgb0[2] as u16 + 2 * rgb1[2] as u16) / 3) as u8,
        ],
    ];
    let mut c_bits = 0u32;
    for i in 0..16 {
        let p = block[i];
        let mut best_idx = 0u32;
        let mut best_dist = i32::MAX;
        for (pi, pc) in pal.iter().enumerate() {
            let dr = p[0] as i32 - pc[0] as i32;
            let dg = p[1] as i32 - pc[1] as i32;
            let db = p[2] as i32 - pc[2] as i32;
            let d = dr * dr + dg * dg + db * db;
            if d < best_dist {
                best_dist = d;
                best_idx = pi as u32;
            }
        }
        c_bits |= best_idx << (i * 2);
    }
    out[4..8].copy_from_slice(&c_bits.to_le_bytes());
    out
}

#[inline]
fn sample_block_4x4(img: &RgbaImage, bx: usize, by: usize) -> ([[u8; 4]; 16], [u8; 3], [u8; 3], u8, u8) {
    let width = img.width();
    let height = img.height();
    let mut block = [[0u8; 4]; 16];
    let mut min_rgb = [255u8; 3];
    let mut max_rgb = [0u8; 3];
    let mut min_a = 255u8;
    let mut max_a = 0u8;
    for py in 0..4 {
        let y = ((by * 4 + py) as u32).min(height.saturating_sub(1));
        for px in 0..4 {
            let x = ((bx * 4 + px) as u32).min(width.saturating_sub(1));
            let p = img.get_pixel(x, y).0;
            block[py * 4 + px] = p;
            for c in 0..3 {
                min_rgb[c] = min_rgb[c].min(p[c]);
                max_rgb[c] = max_rgb[c].max(p[c]);
            }
            min_a = min_a.min(p[3]);
            max_a = max_a.max(p[3]);
        }
    }
    (block, min_rgb, max_rgb, min_a, max_a)
}

/// Fast pure-Rust BC1 (`DXT1`) block compressor for Custom Ball & opaque UPK mipmaps.
pub fn encode_dxt1(img: &RgbaImage) -> Vec<u8> {
    let bw = ((img.width() + 3) / 4) as usize;
    let bh = ((img.height() + 3) / 4) as usize;
    let mut out = vec![0u8; bw * bh * 8];
    for by in 0..bh {
        for bx in 0..bw {
            let (block, min_rgb, max_rgb, _, _) = sample_block_4x4(img, bx, by);
            let off = (by * bw + bx) * 8;
            let enc = encode_dxt1_color_block(&block, min_rgb, max_rgb);
            out[off..off + 8].copy_from_slice(&enc);
        }
    }
    out
}

/// Fast pure-Rust BC3 (`DXT5`) block compressor for Custom Decal & Custom Stadium Pitch mipmaps.
pub fn encode_dxt5(img: &RgbaImage) -> Vec<u8> {
    let bw = ((img.width() + 3) / 4) as usize;
    let bh = ((img.height() + 3) / 4) as usize;
    let mut out = vec![0u8; bw * bh * 16];

    for by in 0..bh {
        for bx in 0..bw {
            let (block, min_rgb, max_rgb, min_a, max_a) = sample_block_4x4(img, bx, by);
            let off = (by * bw + bx) * 16;
            out[off] = max_a;
            out[off + 1] = min_a;
            let mut a_bits = 0u64;
            if max_a > min_a {
                let span = (max_a - min_a) as i32;
                for i in 0..16 {
                    let a = block[i][3] as i32;
                    let step = ((max_a as i32 - a) * 7 + span / 2) / span;
                    let idx: u64 = match step.clamp(0, 7) {
                        0 => 0,
                        7 => 1,
                        s => (s + 1) as u64,
                    };
                    a_bits |= idx << (i * 3);
                }
            }
            for i in 0..6 {
                out[off + 2 + i] = ((a_bits >> (i * 8)) & 0xFF) as u8;
            }
            let c_blk = encode_dxt1_color_block(&block, min_rgb, max_rgb);
            out[off + 8..off + 16].copy_from_slice(&c_blk);
        }
    }
    out
}

// ── Local UPK & External .TFC Texture2D Extractor ────────────────────────────

fn extract_tfc_mip(uncomp: &[u8], cooked_dir: &Path) -> Option<RgbaImage> {
    if uncomp.len() < 32 {
        return None;
    }
    let mut best_cand: Option<(usize, usize, u64, u32, u32)> = None;
    for pos in 0..uncomp.len().saturating_sub(28) {
        let flags = read_u32_le(uncomp, pos);
        if ((flags & 0x20) == 0 && (flags & 0x03) != 0x03) || (flags & !0x0001_00FF) != 0 {
            continue;
        }
        let elem_cnt = read_i32_le(uncomp, pos + 4) as usize;
        if !(2048..=16_777_216).contains(&elem_cnt) || (elem_cnt & (elem_cnt - 1)) != 0 {
            continue;
        }
        let disk_sz = read_i32_le(uncomp, pos + 8) as usize;
        let tfc_off = read_i64_le(uncomp, pos + 12);
        let sx = read_u32_le(uncomp, pos + 20);
        let sy = read_u32_le(uncomp, pos + 24);
        if !(64..=1024).contains(&sx) || !(64..=1024).contains(&sy) {
            continue;
        }
        if (sx & (sx - 1)) != 0 || (sy & (sy - 1)) != 0 {
            continue;
        }
        let px_cnt = (sx as usize) * (sy as usize);
        if elem_cnt != px_cnt && elem_cnt * 2 != px_cnt && elem_cnt != px_cnt * 4 {
            continue;
        }
        if (256..=elem_cnt * 2).contains(&disk_sz) && (64..=2_000_000_000).contains(&tfc_off) {
            if best_cand.map_or(true, |b| sx > b.3) {
                best_cand = Some((elem_cnt, disk_sz, tfc_off as u64, sx, sy));
            }
        }
    }

    let (elem_cnt, disk_sz, tfc_off, sx, sy) = best_cand?;
    let entries = fs::read_dir(cooked_dir).ok()?;
    for entry in entries.flatten() {
        let p = entry.path();
        if p.extension().and_then(|e| e.to_str()).map(|s| s.to_lowercase()) != Some("tfc".into()) {
            continue;
        }
        let Ok(meta) = p.metadata() else { continue };
        if meta.len() < tfc_off + (disk_sz as u64) {
            continue;
        }
        let Ok(mut f) = File::open(&p) else { continue };
        if f.seek(SeekFrom::Start(tfc_off)).is_err() {
            continue;
        }
        let mut hdr = [0u8; 16];
        if f.read_exact(&mut hdr).is_err() {
            continue;
        }
        let magic = read_u32_le(&hdr, 0);
        let bsz = read_i32_le(&hdr, 4);
        let c_comp = read_i32_le(&hdr, 8) as usize;
        let c_uncomp = read_i32_le(&hdr, 12) as usize;
        if magic == PACKAGE_FILE_TAG
            && bsz == 131072
            && c_uncomp == elem_cnt
            && (c_comp + 16..=c_comp + 256).contains(&disk_sz)
        {
            if f.seek(SeekFrom::Start(tfc_off)).is_err() {
                continue;
            }
            let mut chunk_buf = vec![0u8; disk_sz];
            if f.read_exact(&mut chunk_buf).is_ok() {
                if let Ok(raw_pix) = decompress_chunk_payload(&chunk_buf) {
                    let px_cnt = (sx as usize) * (sy as usize);
                    if elem_cnt == px_cnt * 4 {
                        return decode_bgra8(sx, sy, &raw_pix);
                    } else if elem_cnt * 2 == px_cnt {
                        return decode_dxt1(sx, sy, &raw_pix);
                    } else {
                        return decode_dxt5(sx, sy, &raw_pix);
                    }
                }
            }
        }
    }
    None
}

pub fn extract_texture_from_upk(upk_path: &Path, cooked_dir: Option<&Path>) -> Option<RgbaImage> {
    let eff_cooked = cooked_dir.or_else(|| upk_path.parent());
    if let Ok((upk, _)) = unpack_upk_from_disk(upk_path) {
        let mut cands: Vec<(u8, usize)> = upk
            .exports
            .iter()
            .enumerate()
            .filter_map(|(idx, exp)| {
                if upk.export_class_name(exp) != "Texture2D" {
                    return None;
                }
                let nm = upk.full_export_path(idx + 1).to_ascii_lowercase();
                let obj = upk.resolve_name(exp.object_name).to_ascii_lowercase();
                if obj.ends_with("_n")
                    || obj.contains("normal")
                    || obj.contains("curvature")
                    || obj.contains("blackalpha")
                    || obj.contains("fxmask")
                    || obj.contains("gradmask")
                {
                    return None;
                }
                let prio = if nm.contains("thumb") || nm.contains("icon") {
                    0u8
                } else if !obj.contains("mask") && !nm.contains("chassis") {
                    1u8
                } else {
                    2u8
                };
                Some((prio, idx))
            })
            .collect();
        cands.sort_by_key(|&(p, i)| (p, i));

        let mut best_thumb: Option<(u32, RgbaImage)> = None;
        let mut best_any: Option<(u32, RgbaImage)> = None;
        for (prio, idx) in cands {
            if prio > 0 && best_thumb.is_some() {
                break;
            }
            if let Some(im) = crate::custom_studio::decode_texture2d_export(&upk, idx, eff_cooked) {
                let area = im.width().saturating_mul(im.height());
                if prio == 0 {
                    if best_thumb.as_ref().map_or(true, |(ba, _)| area >= *ba) {
                        best_thumb = Some((area, im));
                    }
                } else if best_any.as_ref().map_or(true, |(ba, _)| area >= *ba) {
                    best_any = Some((area, im));
                }
            }
        }
        if let Some((_, im)) = best_thumb.or(best_any) {
            return Some(im);
        }
    }

    let chunks = decompress_all_upk_chunks(upk_path).ok()?;
    let mut sorted: Vec<&Vec<u8>> = chunks.iter().collect();
    sorted.sort_by(|a, b| b.len().cmp(&a.len()));

    // Pass 1: Scan first 4096 bytes for inline Mip0 (>= 64px)
    for &uncomp in &sorted {
        let l = uncomp.len();
        let limit = l.saturating_sub(16).min(4096);
        for idx in 0..limit {
            let sz1 = read_u32_le(uncomp, idx) as usize;
            let sz2 = read_u32_le(uncomp, idx + 4) as usize;
            if sz1 != sz2 || sz1 < 2048 || idx + 8 + sz1 + 8 > l {
                continue;
            }
            for pad in [8usize, 12, 16] {
                if idx + pad + sz1 + 8 > l {
                    continue;
                }
                let sx = read_u32_le(uncomp, idx + pad + sz1);
                let sy = read_u32_le(uncomp, idx + pad + sz1 + 4);
                if !(64..=4096).contains(&sx) || !(64..=4096).contains(&sy) {
                    continue;
                }
                let pixels = &uncomp[idx + pad..idx + pad + sz1];
                let px_cnt = (sx as usize) * (sy as usize);
                if sz1 == px_cnt * 4 {
                    return decode_bgra8(sx, sy, pixels);
                } else if sz1 * 2 == px_cnt {
                    return decode_dxt1(sx, sy, pixels);
                } else if sz1 == px_cnt {
                    return decode_dxt5(sx, sy, pixels);
                }
            }
        }
    }

    // Pass 2: Standard bulk sizes anywhere in chunk
    const STD_SIZES: [usize; 12] = [
        8388608, 4194304, 2097152, 1048576, 524288, 262144, 168000, 140800, 131072, 65536, 57600,
        32768,
    ];
    for &uncomp in &sorted {
        let l = uncomp.len();
        for &cand_sz in &STD_SIZES {
            if cand_sz + 16 > l {
                continue;
            }
            let mut m = [0u8; 8];
            m[0..4].copy_from_slice(&(cand_sz as u32).to_le_bytes());
            m[4..8].copy_from_slice(&(cand_sz as u32).to_le_bytes());
            let mut start = 0usize;
            while start + 8 + cand_sz + 8 <= l {
                let Some(rel) = uncomp[start..].windows(8).position(|w| w == m) else {
                    break;
                };
                let fpos = start + rel;
                for pad in [8usize, 12, 16] {
                    if fpos + pad + cand_sz + 8 <= l {
                        let sx = read_u32_le(uncomp, fpos + pad + cand_sz);
                        let sy = read_u32_le(uncomp, fpos + pad + cand_sz + 4);
                        if (64..=4096).contains(&sx) && (64..=4096).contains(&sy) {
                            let pixels = &uncomp[fpos + pad..fpos + pad + cand_sz];
                            let px_cnt = (sx as usize) * (sy as usize);
                            if cand_sz == px_cnt * 4 {
                                return decode_bgra8(sx, sy, pixels);
                            } else if cand_sz * 2 == px_cnt {
                                return decode_dxt1(sx, sy, pixels);
                            } else if cand_sz == px_cnt {
                                return decode_dxt5(sx, sy, pixels);
                            }
                        }
                    }
                }
                start = fpos + 4;
            }
        }
    }

    // Pass 3: External TextureFileCache (.tfc) lookup
    let cdir = cooked_dir.or_else(|| upk_path.parent())?;
    if let Some(last) = chunks.last() {
        if let Some(im) = extract_tfc_mip(last, cdir) {
            return Some(im);
        }
    }

    None
}

// ── Fixed Online Rocket League Wiki MediaWiki `pageimages` + Search Scraper ──

pub fn scrape_item_icon_online(product_name: &str, slot_name: &str) -> Option<RgbaImage> {
    let clean_name = product_name.trim();
    if clean_name.is_empty() {
        return None;
    }

    let client = shared_wiki_client()?;

    // Candidate page titles to query via prop=pageimages&redirects=1
    let mut titles_to_try = vec![
        clean_name.to_string(),
        format!("{} ({})", clean_name, slot_name),
        format!("{} (Alpha Reward)", clean_name),
    ];
    titles_to_try.dedup();
    let joined_titles = titles_to_try.join("|");

    let api_url = "https://rocketleague.fandom.com/api.php";
    if let Ok(resp) = client
        .get(api_url)
        .query(&[
            ("action", "query"),
            ("titles", joined_titles.as_str()),
            ("prop", "pageimages"),
            ("piprop", "original|thumbnail"),
            ("pithumbsize", "256"),
            ("redirects", "1"),
            ("format", "json"),
        ])
        .send()
    {
        if let Ok(json) = resp.json::<serde_json::Value>() {
            if let Some(img) = extract_image_from_pageimages_json(client, &json, &titles_to_try) {
                return Some(img);
            }
        }
    }

    // Fallback 2: MediaWiki Search API (`list=search`) for items whose wiki title differs slightly
    let search_q = format!("{} {}", clean_name, slot_name);
    if let Ok(resp) = client
        .get(api_url)
        .query(&[
            ("action", "query"),
            ("list", "search"),
            ("srsearch", search_q.as_str()),
            ("srlimit", "3"),
            ("format", "json"),
        ])
        .send()
    {
        if let Ok(json) = resp.json::<serde_json::Value>() {
            if let Some(arr) = json
                .get("query")
                .and_then(|q| q.get("search"))
                .and_then(|s| s.as_array())
            {
                let hit_titles: Vec<String> = arr
                    .iter()
                    .filter_map(|v| v.get("title").and_then(|t| t.as_str()).map(|s| s.to_string()))
                    .collect();
                if !hit_titles.is_empty() {
                    let joined = hit_titles.join("|");
                    if let Ok(r2) = client
                        .get(api_url)
                        .query(&[
                            ("action", "query"),
                            ("titles", joined.as_str()),
                            ("prop", "pageimages"),
                            ("piprop", "original|thumbnail"),
                            ("pithumbsize", "256"),
                            ("redirects", "1"),
                            ("format", "json"),
                        ])
                        .send()
                    {
                        if let Ok(j2) = r2.json::<serde_json::Value>() {
                            if let Some(img) = extract_image_from_pageimages_json(client, &j2, &hit_titles) {
                                return Some(img);
                            }
                        }
                    }
                }
            }
        }
    }

    None
}

fn fit_icon_dims(src_w: u32, src_h: u32, max_dim: u32) -> (u32, u32) {
    let sw = src_w.max(1);
    let sh = src_h.max(1);
    let md = max_dim.max(1);
    if sw >= sh {
        let nw = md;
        let nh = ((sh as u64 * md as u64) / sw as u64).max(1) as u32;
        (nw, nh.min(md))
    } else {
        let nh = md;
        let nw = ((sw as u64 * md as u64) / sh as u64).max(1) as u32;
        (nw.min(md), nh)
    }
}

fn extract_image_from_pageimages_json(
    client: &reqwest::blocking::Client,
    json: &serde_json::Value,
    preferred_titles: &[String],
) -> Option<RgbaImage> {
    let pages = json.get("query")?.get("pages")?.as_object()?;
    let mut ordered_pages: Vec<(usize, &serde_json::Value)> = pages
        .values()
        .map(|pinfo| {
            let title = pinfo.get("title").and_then(|t| t.as_str()).unwrap_or("");
            let prio = preferred_titles
                .iter()
                .position(|t| t.eq_ignore_ascii_case(title))
                .unwrap_or(usize::MAX);
            (prio, pinfo)
        })
        .collect();
    ordered_pages.sort_by_key(|&(prio, _)| prio);

    for (_prio, pinfo) in ordered_pages {
        let thumb_url = pinfo.get("thumbnail").and_then(|t| t.get("source")).and_then(|s| s.as_str());
        let orig_url = pinfo.get("original").and_then(|o| o.get("source")).and_then(|s| s.as_str());
        let direct_orig = orig_url.map(|u| u.split("/revision/").next().unwrap_or(u));
        for url in [thumb_url, orig_url, direct_orig].into_iter().flatten() {
            if let Ok(img_resp) = client
                .get(url)
                .header("Referer", "https://rocketleague.fandom.com/")
                .send()
            {
                if let Ok(bytes) = img_resp.bytes() {
                    if let Ok(dyn_im) = image::load_from_memory(&bytes) {
                        let rgba = dyn_im.to_rgba8();
                        let (nw, nh) = fit_icon_dims(rgba.width(), rgba.height(), 256);
                        let resized = if (rgba.width(), rgba.height()) == (nw, nh) {
                            rgba
                        } else {
                            imageops::resize(&rgba, nw, nh, FilterType::Triangle)
                        };
                        let mut canvas = RgbaImage::from_pixel(256, 256, Rgba([0, 0, 0, 0]));
                        let ox = (256 - resized.width()) / 2;
                        let oy = (256 - resized.height()) / 2;
                        imageops::overlay(&mut canvas, &resized, ox as i64, oy as i64);
                        return Some(canvas);
                    }
                }
            }
        }
    }
    None
}

// ── Procedural High-DPI Vector Icon Card Fallback ────────────────────────────

pub fn render_procedural_icon(product_name: &str, slot_name: &str, size: u32) -> RgbaImage {
    let size = size.max(8);
    let mut img = RgbaImage::from_pixel(size, size, Rgba([12, 12, 16, 255]));
    let accent: [u8; 3] = match slot_name {
        "Body" => [59, 130, 246],
        "Decal" => [168, 85, 247],
        "Wheels" => [16, 185, 129],
        "Rocket Boost" => [249, 115, 22],
        "Goal Explosion" => [236, 72, 153],
        "Trail" => [6, 182, 212],
        "Topper" => [234, 179, 8],
        "Antenna" => [99, 102, 241],
        "Paint Finish" => [20, 184, 166],
        "Engine Audio" => [244, 63, 94],
        "Player Banner" => [139, 92, 246],
        "Avatar Border" => [56, 189, 248],
        _ => [161, 161, 170],
    };

    let cx = (size / 2) as i32;
    let cy = (size / 2) as i32;
    let r_outer = (size as i32) * 32 / 100;
    let r_inner = (size as i32) * 18 / 100;

    for y in 0..size {
        for x in 0..size {
            let dx = x as i32 - cx;
            let dy = y as i32 - cy;
            let d2 = dx * dx + dy * dy;
            // Subtle border & emblem
            if x < 2 || y < 2 || x >= size - 2 || y >= size - 2 {
                img.put_pixel(x, y, Rgba([accent[0] / 2, accent[1] / 2, accent[2] / 2, 220]));
            } else if (d2 - r_outer * r_outer).abs() < (size as i32 * 2) {
                img.put_pixel(x, y, Rgba([accent[0], accent[1], accent[2], 240]));
            } else if d2 < r_inner * r_inner {
                let hash = product_name.bytes().fold(0u8, |a, b| a.wrapping_add(b));
                img.put_pixel(
                    x,
                    y,
                    Rgba([
                        accent[0].saturating_add(hash % 25),
                        accent[1].saturating_add((hash >> 2) % 25),
                        accent[2],
                        195,
                    ]),
                );
            }
        }
    }
    img
}

/// Composites an item icon with an optional paint color glow and corner swatch badge.
pub fn composite_icon_with_paint(
    base: &RgbaImage,
    paint: Option<&PaintColorSpec>,
    target_size: u32,
) -> RgbaImage {
    let target_size = target_size.max(16);
    let inner_sz = target_size.saturating_sub(8).max(8);
    let filt = if target_size <= 64 { FilterType::Triangle } else { FilterType::Lanczos3 };
    let (nw, nh) = fit_icon_dims(base.width(), base.height(), inner_sz);
    let resized = if base.width() == nw && base.height() == nh {
        base.clone()
    } else {
        imageops::resize(base, nw, nh, filt)
    };

    let mut canvas = RgbaImage::from_pixel(target_size, target_size, Rgba([10, 10, 13, 255]));
    if let Some(p) = paint {
        let [pr, pg, pb] = p.rgb8();
        let cx = (target_size / 2) as f32;
        let cy = (target_size / 2) as f32;
        let max_r = (target_size as f32) * 0.46;
        for y in 0..target_size {
            for x in 0..target_size {
                let dist = ((x as f32 - cx).powi(2) + (y as f32 - cy).powi(2)).sqrt();
                if dist < max_r {
                    let t = (1.0 - dist / max_r).powi(2) * 0.35;
                    canvas.put_pixel(
                        x,
                        y,
                        Rgba([
                            (10.0 + pr as f32 * t) as u8,
                            (10.0 + pg as f32 * t) as u8,
                            (13.0 + pb as f32 * t) as u8,
                            255,
                        ]),
                    );
                }
            }
        }
    }

    let ox = (target_size - resized.width()) / 2;
    let oy = (target_size - resized.height()) / 2;
    image::imageops::overlay(&mut canvas, &resized, ox as i64, oy as i64);

    // Draw border & corner swatch dot if painted
    if let Some(p) = paint {
        let [pr, pg, pb] = p.rgb8();
        for i in 0..target_size {
            canvas.put_pixel(i, 0, Rgba([pr, pg, pb, 220]));
            canvas.put_pixel(i, target_size - 1, Rgba([pr, pg, pb, 220]));
            canvas.put_pixel(0, i, Rgba([pr, pg, pb, 220]));
            canvas.put_pixel(target_size - 1, i, Rgba([pr, pg, pb, 220]));
        }
        let dot_x = target_size as i32 - 11;
        let dot_y = 11i32;
        for y in (dot_y - 6)..=(dot_y + 6) {
            for x in (dot_x - 6)..=(dot_x + 6) {
                if x >= 0 && y >= 0 && (x as u32) < target_size && (y as u32) < target_size {
                    let d2 = (x - dot_x).pow(2) + (y - dot_y).pow(2);
                    if d2 <= 25 {
                        canvas.put_pixel(x as u32, y as u32, Rgba([pr, pg, pb, 255]));
                    } else if d2 <= 36 {
                        canvas.put_pixel(x as u32, y as u32, Rgba([0, 0, 0, 240]));
                    }
                }
            }
        }
    }
    canvas
}

// ── Thread-Safe Icon Cache & Resolver ────────────────────────────────────────

const MAX_MEM_ICONS: usize = 256;

pub struct IconCacheManager {
    pub cache_dir: PathBuf,
    pub cooked_index: Arc<Mutex<HashMap<String, PathBuf>>>,
    pub mem_cache: Arc<Mutex<HashMap<String, RgbaImage>>>,
    mem_order: Arc<Mutex<VecDeque<String>>>,
}

impl IconCacheManager {
    pub fn new() -> Self {
        let cache_dir = get_appdata_dir().join("Cache").join("Icons");
        let _ = fs::create_dir_all(&cache_dir);
        Self {
            cache_dir,
            cooked_index: Arc::new(Mutex::new(HashMap::new())),
            mem_cache: Arc::new(Mutex::new(HashMap::new())),
            mem_order: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    pub fn index_cooked_dir(&self, cooked_dir: &Path) {
        if !cooked_dir.is_dir() {
            return;
        }
        let mut idx = HashMap::new();
        if let Ok(entries) = fs::read_dir(cooked_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if let Some(nm) = p.file_name().and_then(|n| n.to_str()) {
                    if nm.to_lowercase().ends_with(".upk") {
                        idx.insert(nm.to_lowercase(), p);
                    }
                }
            }
        }
        if let Ok(mut guard) = self.cooked_index.lock() {
            *guard = idx;
        }
    }

    pub fn resolve_thumbnail_upk(&self, cooked_dir: &Path, asset_pkg: &str, asset_path: &str) -> Option<PathBuf> {
        {
            let guard = self.cooked_index.lock().ok()?;
            if guard.is_empty() {
                drop(guard);
                self.index_cooked_dir(cooked_dir);
            }
        }
        let guard = self.cooked_index.lock().ok()?;
        let stem = asset_pkg
            .strip_suffix(".upk")
            .or_else(|| asset_pkg.strip_suffix(".UPK"))
            .unwrap_or(asset_pkg);
        let base = stem
            .strip_suffix("_SF")
            .or_else(|| stem.strip_suffix("_sf"))
            .unwrap_or(stem);

        let mut cands = vec![
            format!("{}_t_sf.upk", base.to_lowercase()),
            format!("{}_t_sf.upk", stem.to_lowercase()),
            format!("shopitem_{}_t_sf.upk", base.to_lowercase()),
        ];
        if let Some((ap_pkg, ap_obj)) = asset_path.split_once('.') {
            cands.push(format!("{}_t_sf.upk", ap_pkg.to_lowercase()));
            cands.push(format!("{}_t_sf.upk", ap_obj.to_lowercase()));
        }
        for c in cands {
            if let Some(p) = guard.get(&c) {
                return Some(p.clone());
            }
        }

        let pkg_low = format!("{}.upk", stem.to_lowercase());
        if guard.contains_key(&pkg_low) {
            return guard.get(&pkg_low).cloned();
        }
        None
    }

    pub fn cache_key(item: &CatalogItem) -> String {
        let mut safe: String = item
            .asset_package
            .to_lowercase()
            .replace(".upk", "")
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        safe.truncate(40);
        let h = item
            .asset_path
            .bytes()
            .fold(5381u64, |acc, b| acc.wrapping_mul(33).wrapping_add(b as u64));
        format!("{}_{:08x}", safe, h as u32)
    }

    fn insert_mem_cache(&self, key: String, img: RgbaImage) {
        if let (Ok(mut guard), Ok(mut order)) = (self.mem_cache.lock(), self.mem_order.lock()) {
            if !guard.contains_key(&key) {
                while guard.len() >= MAX_MEM_ICONS {
                    if let Some(old) = order.pop_front() {
                        guard.remove(&old);
                    } else {
                        guard.clear();
                        break;
                    }
                }
                order.push_back(key.clone());
            }
            guard.insert(key, img);
        }
    }

    /// Resolves an item's icon (`Memory -> Disk Cache -> Local *_T_SF.upk / .tfc -> Online Wiki Scraper -> Procedural`).
    pub fn get_icon_sync(&self, cooked_dir: &Path, item: &CatalogItem, allow_online: bool) -> (RgbaImage, &'static str) {
        let key = Self::cache_key(item);
        if let Ok(guard) = self.mem_cache.lock() {
            if let Some(im) = guard.get(&key) {
                return (im.clone(), "cache");
            }
        }

        let disk_path = self.cache_dir.join(format!("{}.png", key));
        if disk_path.exists() {
            if let Ok(im) = image::open(&disk_path) {
                let rgba = im.to_rgba8();
                let (nw, nh) = fit_icon_dims(rgba.width(), rgba.height(), 128);
                let resized = if (rgba.width(), rgba.height()) == (nw, nh) {
                    rgba
                } else {
                    imageops::resize(&rgba, nw, nh, FilterType::Triangle)
                };
                self.insert_mem_cache(key, resized.clone());
                return (resized, "disk");
            }
        }

        // 1. Local *_T_SF.upk + .tfc extraction
        if let Some(t_upk) = self.resolve_thumbnail_upk(cooked_dir, &item.asset_package, &item.asset_path) {
            if let Some(extracted) = extract_texture_from_upk(&t_upk, Some(cooked_dir)) {
                let (nw, nh) = fit_icon_dims(extracted.width(), extracted.height(), 128);
                let thumb = if (extracted.width(), extracted.height()) == (nw, nh) {
                    extracted
                } else {
                    imageops::resize(&extracted, nw, nh, FilterType::Lanczos3)
                };
                let _ = thumb.save(&disk_path);
                self.insert_mem_cache(key, thumb.clone());
                return (thumb, "upk");
            }
        }

        // 2. Online Rocket League Wiki MediaWiki PageImages + Search scraper (with 7-day negative cache)
        if allow_online {
            let miss_path = self.cache_dir.join(format!("{}.miss", key));
            let miss_fresh = miss_path
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| SystemTime::now().duration_since(t).ok())
                .map_or(false, |age| age < Duration::from_secs(7 * 86_400));
            if !miss_fresh {
                if let Some(scraped) = scrape_item_icon_online(&item.product, &item.slot) {
                    let (nw, nh) = fit_icon_dims(scraped.width(), scraped.height(), 128);
                    let thumb = if (scraped.width(), scraped.height()) == (nw, nh) {
                        scraped
                    } else {
                        imageops::resize(&scraped, nw, nh, FilterType::Lanczos3)
                    };
                    let _ = thumb.save(&disk_path);
                    let _ = fs::remove_file(&miss_path);
                    self.insert_mem_cache(key, thumb.clone());
                    return (thumb, "web");
                }
                let _ = fs::write(&miss_path, b"");
            }
        }

        // 3. Procedural high-DPI vector fallback
        let proc_img = render_procedural_icon(&item.product, &item.slot, 128);
        self.insert_mem_cache(key, proc_img.clone());
        (proc_img, "procedural")
    }

    /// Parallel bulk icon extractor & scraper (bounded worker concurrency for Wiki politeness).
    pub fn bulk_sync_category(
        &self,
        cooked_dir: &Path,
        items: &[CatalogItem],
        allow_online: bool,
    ) -> (usize, usize, usize) {
        self.index_cooked_dir(cooked_dir);
        let run = || -> Vec<&'static str> {
            items
                .par_iter()
                .map(|it| {
                    let (_im, src) = self.get_icon_sync(cooked_dir, it, allow_online);
                    src
                })
                .collect()
        };
        let results: Vec<&'static str> = if allow_online {
            rayon::ThreadPoolBuilder::new()
                .num_threads(4)
                .build()
                .map(|pool| pool.install(run))
                .unwrap_or_else(|_| run())
        } else {
            run()
        };

        let local = results.iter().filter(|&&s| s == "upk" || s == "disk" || s == "cache").count();
        let web = results.iter().filter(|&&s| s == "web").count();
        let proc_cnt = results.iter().filter(|&&s| s == "procedural").count();
        (local, web, proc_cnt)
    }
}

/// Extracts the live match soccer ball texture (`Ball_Default00_D`) from `GameInfo_Soccar_SF.upk`
/// (either inline Mips if customized or `.tfc` from `Textures2.tfc`).
pub fn extract_ball_texture_preview(cooked_dir: &Path) -> Option<RgbaImage> {
    let soccar_path = cooked_dir.join("GameInfo_Soccar_SF.upk");
    let (upk, _) = unpack_upk_from_disk(&soccar_path).ok()?;
    let exp_idx = upk.exports.iter().position(|e| {
        upk.export_class_name(e) == "Texture2D"
            && upk.full_export_path(e.index).eq_ignore_ascii_case("Ball_Default_Textures.Textures.Ball_Default00_D")
    })?;
    if let Some(im) = crate::custom_studio::decode_texture2d_export(&upk, exp_idx, Some(cooked_dir)) {
        return Some(im);
    }
    let exp = &upk.exports[exp_idx];
    let end = (exp.serial_offset + exp.serial_size).min(upk.file_bytes.len());
    extract_tfc_mip(&upk.file_bytes[exp.serial_offset..end], cooked_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mem_cache_evicts_oldest_single_entry_without_clearing_all() {
        let mgr = IconCacheManager::new();
        let sample = RgbaImage::from_pixel(4, 4, Rgba([10, 20, 30, 255]));
        for i in 0..=MAX_MEM_ICONS {
            mgr.insert_mem_cache(format!("k_{}", i), sample.clone());
        }
        let guard = mgr.mem_cache.lock().unwrap();
        assert_eq!(guard.len(), MAX_MEM_ICONS);
        assert!(!guard.contains_key("k_0"), "oldest entry must be evicted");
        assert!(guard.contains_key(&format!("k_{}", MAX_MEM_ICONS)));
        assert!(guard.contains_key("k_1"));
    }

    #[test]
    fn composite_and_procedural_icon_handle_small_sizes_safely() {
        let p = render_procedural_icon("Octane", "Body", 2);
        assert_eq!(p.dimensions(), (8, 8));
        let comp = composite_icon_with_paint(&p, None, 4);
        assert_eq!(comp.dimensions(), (16, 16));
    }
}
