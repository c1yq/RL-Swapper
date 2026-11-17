//! Native Rust UPK Binary Parser, AES-256 (ECB & CTR) Crypto, Zlib Chunk Engine,
//! Surgical ExportTable Swapper, Binary Paint Color Injector, and 3-Tier Pristine Backup Vault.

use aes::cipher::{BlockDecrypt, BlockEncrypt, KeyInit, KeyIvInit, StreamCipher};
use aes::Aes256;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use image::{Rgba, RgbaImage};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

type Aes256Ctr = ctr::Ctr128BE<Aes256>;

pub const PACKAGE_FILE_TAG: u32 = 0x9E2A83C1;
pub const DEFAULT_KEY: [u8; 32] = [
    0xC7, 0xDF, 0x6B, 0x13, 0x25, 0x2A, 0xCC, 0x71, 0x47, 0xBB, 0x51, 0xC9, 0x8A, 0xD7, 0xE3, 0x4B,
    0x7F, 0xE5, 0x00, 0xB7, 0x7C, 0x89, 0x23, 0xFA, 0x6B, 0x53, 0x6D, 0x1E, 0x5B, 0x9A, 0x9C, 0x52,
];

pub static EMBEDDED_ITEMS_JSON: &str = include_str!("../items.json");
pub static EMBEDDED_KEYS_MAP_JSON: &str = include_str!("../keys_map.json");
pub static EMBEDDED_KEYS_TXT: &str = include_str!("../keys.txt");

// ── Byte Helpers ─────────────────────────────────────────────────────────────

#[inline]
pub fn read_u32_le(buf: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(buf[off..off + 4].try_into().unwrap_or([0; 4]))
}

#[inline]
pub fn read_i32_le(buf: &[u8], off: usize) -> i32 {
    i32::from_le_bytes(buf[off..off + 4].try_into().unwrap_or([0; 4]))
}

#[inline]
pub fn read_i64_le(buf: &[u8], off: usize) -> i64 {
    i64::from_le_bytes(buf[off..off + 8].try_into().unwrap_or([0; 8]))
}

#[inline]
pub fn read_f32_le(buf: &[u8], off: usize) -> f32 {
    f32::from_le_bytes(buf[off..off + 4].try_into().unwrap_or([0; 4]))
}

#[inline]
pub fn write_u32_le(buf: &mut [u8], off: usize, val: u32) {
    buf[off..off + 4].copy_from_slice(&val.to_le_bytes());
}

#[inline]
pub fn write_i32_le(buf: &mut [u8], off: usize, val: i32) {
    buf[off..off + 4].copy_from_slice(&val.to_le_bytes());
}

#[inline]
pub fn write_i64_le(buf: &mut [u8], off: usize, val: i64) {
    buf[off..off + 8].copy_from_slice(&val.to_le_bytes());
}

#[inline]
pub fn write_f32_le(buf: &mut [u8], off: usize, val: f32) {
    buf[off..off + 4].copy_from_slice(&val.to_le_bytes());
}

pub fn read_fstring_at(buf: &[u8], offset: usize) -> Option<(String, usize)> {
    if offset + 4 > buf.len() {
        return None;
    }
    let raw_len = read_i32_le(buf, offset);
    let pos = offset + 4;
    if raw_len == 0 {
        return Some((String::new(), pos));
    }
    if raw_len > 0 {
        let len = raw_len as usize;
        if len > 4096 || pos + len > buf.len() {
            return None;
        }
        let slice = &buf[pos..pos + len];
        let trimmed = slice.strip_suffix(&[0]).unwrap_or(slice);
        let s = String::from_utf8_lossy(trimmed).into_owned();
        Some((s, pos + len))
    } else {
        let char_count = (-raw_len) as usize;
        let byte_len = char_count * 2;
        if char_count > 4096 || pos + byte_len > buf.len() {
            return None;
        }
        let mut u16_buf = Vec::with_capacity(char_count);
        for i in 0..char_count {
            let ch = u16::from_le_bytes([buf[pos + i * 2], buf[pos + i * 2 + 1]]);
            if ch != 0 {
                u16_buf.push(ch);
            }
        }
        Some((String::from_utf16_lossy(&u16_buf), pos + byte_len))
    }
}

// ── AES-256 ECB & CTR Crypto ─────────────────────────────────────────────────

pub fn aes_ecb_decrypt(key: &[u8; 32], data: &[u8]) -> Vec<u8> {
    let cipher = Aes256::new_from_slice(key).expect("32-byte key");
    let aligned_len = data.len() & !15;
    let mut out = data[..aligned_len].to_vec();
    for chunk in out.chunks_exact_mut(16) {
        let block = aes::Block::from_mut_slice(chunk);
        cipher.decrypt_block(block);
    }
    out
}

pub fn aes_ecb_encrypt(key: &[u8; 32], data: &[u8]) -> Vec<u8> {
    let cipher = Aes256::new_from_slice(key).expect("32-byte key");
    let padded_len = (data.len() + 15) & !15;
    let mut out = vec![0u8; padded_len];
    out[..data.len()].copy_from_slice(data);
    for chunk in out.chunks_exact_mut(16) {
        let block = aes::Block::from_mut_slice(chunk);
        cipher.encrypt_block(block);
    }
    out
}

pub fn aes_ctr_crypt(key: &[u8; 32], nonce_12: &[u8], data: &[u8]) -> Vec<u8> {
    let mut iv = [0u8; 16];
    let copy_len = nonce_12.len().min(12);
    iv[..copy_len].copy_from_slice(&nonce_12[..copy_len]);
    let mut cipher = Aes256Ctr::new(key.into(), &iv.into());
    let mut out = data.to_vec();
    cipher.apply_keystream(&mut out);
    out
}

// ── Keys & Catalog Management ────────────────────────────────────────────────

pub struct KeyDatabase {
    pub keys_list: Vec<[u8; 32]>,
    pub pkg_to_key: HashMap<String, [u8; 32]>,
}

static GLOBAL_KEY_DB: OnceLock<KeyDatabase> = OnceLock::new();

pub fn get_key_db() -> &'static KeyDatabase {
    GLOBAL_KEY_DB.get_or_init(|| {
        let mut keys_list = Vec::new();
        keys_list.push(DEFAULT_KEY);
        for line in EMBEDDED_KEYS_TXT.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if let Ok(decoded) = BASE64.decode(trimmed) {
                if decoded.len() == 32 {
                    let mut arr = [0u8; 32];
                    arr.copy_from_slice(&decoded);
                    keys_list.push(arr);
                }
            }
        }

        let mut pkg_to_key = HashMap::new();
        if let Ok(raw_map) = serde_json::from_str::<HashMap<String, serde_json::Value>>(EMBEDDED_KEYS_MAP_JSON) {
            for (k, v) in raw_map {
                let low = k.to_lowercase();
                if let Some(idx) = v.as_u64() {
                    let i = idx as usize;
                    // keys_map.json indices are 0-based into keys.txt lines (which is keys_list[i + 1])
                    if i + 1 < keys_list.len() {
                        pkg_to_key.insert(low, keys_list[i + 1]);
                    }
                } else if let Some(b64) = v.as_str() {
                    if let Ok(dec) = BASE64.decode(b64.trim()) {
                        if dec.len() == 32 {
                            let mut arr = [0u8; 32];
                            arr.copy_from_slice(&dec);
                            pkg_to_key.insert(low, arr);
                        }
                    }
                }
            }
        }

        KeyDatabase { keys_list, pkg_to_key }
    })
}

pub fn lookup_package_key(upk_path: &Path) -> Option<[u8; 32]> {
    let db = get_key_db();
    let file_name = upk_path.file_name()?.to_string_lossy().to_lowercase();
    let stem = file_name
        .strip_suffix(".bak")
        .unwrap_or(&file_name)
        .strip_suffix(".upk")
        .unwrap_or(&file_name)
        .strip_suffix(".udk")
        .unwrap_or(&file_name);

    let base_no_sf = stem.strip_suffix("_sf").unwrap_or(stem);
    let base_no_t = base_no_sf.strip_suffix("_t").unwrap_or(base_no_sf);
    let base_no_mapimg = base_no_sf.strip_suffix("_mapimage").unwrap_or(base_no_sf);

    for cand in [
        stem.to_string(),
        format!("{}_sf", base_no_sf),
        base_no_sf.to_string(),
        format!("{}_sf", base_no_t),
        base_no_t.to_string(),
        format!("{}_sf", base_no_mapimg),
        base_no_mapimg.to_string(),
    ] {
        if let Some(k) = db.pkg_to_key.get(&cand) {
            return Some(*k);
        }
    }
    None
}

// ── UPK Prefix & Header Structures ───────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct UpkPrefix {
    pub file_version: u16,
    pub licensee_version: u16,
    pub total_header_size: usize,
    pub folder_name: String,
    pub package_flags: u32,
    pub package_flags_off: usize,
    pub name_count: usize,
    pub name_offset: usize,
    pub export_count: usize,
    pub export_offset: usize,
    pub import_count: usize,
    pub import_offset: usize,
    pub depends_offset: usize,
    pub garbage_size: usize,
    pub compressed_chunks_offset: usize,
    pub ctr_nonce: Option<[u8; 12]>,
    // Summary byte offsets for patching
    pub total_header_size_off: usize,
    pub name_count_off: usize,
    pub name_offset_off: usize,
    pub export_count_off: usize,
    pub export_offset_off: usize,
    pub import_count_off: usize,
    pub import_offset_off: usize,
    pub depends_offset_off: usize,
    pub import_export_guids_offset_off: usize,
    pub thumbnail_table_offset_off: usize,
    pub generations_count: usize,
    pub generations_entries_off: usize,
    pub compression_flags_off: usize,
    pub texture_allocations_off: usize,
    pub compression_meta_off: usize,
}

pub fn parse_upk_prefix(raw: &[u8]) -> Result<UpkPrefix, String> {
    if raw.len() < 64 {
        return Err("UPK file too small".into());
    }
    if read_u32_le(raw, 0) != PACKAGE_FILE_TAG {
        return Err("Invalid UPK package magic tag".into());
    }
    let fv = u16::from_le_bytes([raw[4], raw[5]]);
    let lv = u16::from_le_bytes([raw[6], raw[7]]);
    let total_header_size = read_i32_le(raw, 8).max(0) as usize;
    let (folder_name, mut pos) = read_fstring_at(raw, 12).ok_or("Invalid UPK folder FString")?;
    if pos + 64 > raw.len() {
        return Err("Truncated UPK summary header".into());
    }
    let package_flags_off = pos;
    let package_flags = read_u32_le(raw, pos);
    pos += 4;
    let name_count_off = pos;
    let name_count = read_i32_le(raw, pos).max(0) as usize;
    let name_offset_off = pos + 4;
    let name_offset = read_i32_le(raw, pos + 4).max(0) as usize;
    let export_count_off = pos + 8;
    let export_count = read_i32_le(raw, pos + 8).max(0) as usize;
    let export_offset_off = pos + 12;
    let export_offset = read_i32_le(raw, pos + 12).max(0) as usize;
    let import_count_off = pos + 16;
    let import_count = read_i32_le(raw, pos + 16).max(0) as usize;
    let import_offset_off = pos + 20;
    let import_offset = read_i32_le(raw, pos + 20).max(0) as usize;
    let depends_offset_off = pos + 24;
    let depends_offset = read_i32_le(raw, pos + 24).max(0) as usize;
    pos += 28;

    let import_export_guids_offset_off = pos;
    let thumbnail_table_offset_off = pos + 12;
    // Skip 4 ints + 16-byte GUID
    pos += 32;
    let generations_count = read_i32_le(raw, pos).max(0) as usize;
    let generations_entries_off = pos + 4;
    pos += 4 + generations_count * 12;
    // EngineVersion(4), CookerVersion(4), CompressionFlags(4)
    let compression_flags_off = pos + 8;
    pos += 12;
    if pos + 4 > raw.len() {
        return Err("Truncated UPK summary at outerchunks".into());
    }
    let outer_chunks = read_i32_le(raw, pos).max(0) as usize;
    pos += 4 + outer_chunks * 16;
    // PackageSource + AdditionalPackagesToCook
    pos += 4;
    if let Some((_, next_p)) = read_fstring_array_skip(raw, pos) {
        pos = next_p;
    }
    // TextureAllocations skip: each FTextureType has 5 i32s (20 bytes) + TArray<i32>
    let texture_allocations_off = pos;
    if pos + 4 <= raw.len() {
        let ta_count = read_i32_le(raw, pos).max(0) as usize;
        if ta_count <= 256 {
            let mut tpos = pos + 4;
            let mut ok = true;
            for _ in 0..ta_count {
                if tpos + 24 > raw.len() {
                    ok = false;
                    break;
                }
                let exp_len = read_i32_le(raw, tpos + 20).max(0) as usize;
                tpos += 24 + exp_len * 4;
            }
            if ok && tpos <= name_offset {
                pos = tpos;
            }
        }
    }

    let compression_meta_off = pos;
    let mut garbage_size = 0usize;
    let mut compressed_chunks_offset = 0usize;
    let mut ctr_nonce = None;
    if lv >= 22 && pos + 12 <= name_offset && name_offset <= raw.len() {
        let g = read_i32_le(raw, pos);
        let c = read_i32_le(raw, pos + 4);
        if g >= 0 && c >= 0 {
            garbage_size = g as usize;
            compressed_chunks_offset = c as usize;
        }
        if (package_flags & 0x800) != 0 && pos + 24 <= name_offset {
            let mut n = [0u8; 12];
            n.copy_from_slice(&raw[pos + 12..pos + 24]);
            ctr_nonce = Some(n);
        }
    }

    Ok(UpkPrefix {
        file_version: fv,
        licensee_version: lv,
        total_header_size,
        folder_name,
        package_flags,
        package_flags_off,
        name_count,
        name_offset,
        export_count,
        export_offset,
        import_count,
        import_offset,
        depends_offset,
        garbage_size,
        compressed_chunks_offset,
        ctr_nonce,
        total_header_size_off: 8,
        name_count_off,
        name_offset_off,
        export_count_off,
        export_offset_off,
        import_count_off,
        import_offset_off,
        depends_offset_off,
        import_export_guids_offset_off,
        thumbnail_table_offset_off,
        generations_count,
        generations_entries_off,
        compression_flags_off,
        texture_allocations_off,
        compression_meta_off,
    })
}

fn read_fstring_array_skip(raw: &[u8], offset: usize) -> Option<(usize, usize)> {
    if offset + 4 > raw.len() {
        return None;
    }
    let cnt = read_i32_le(raw, offset);
    if cnt < 0 || cnt > 512 {
        return None;
    }
    let mut pos = offset + 4;
    for _ in 0..cnt {
        let (_, next) = read_fstring_at(raw, pos)?;
        pos = next;
    }
    Some((cnt as usize, pos))
}

/// Validates whether `plain` decrypted header bytes are valid for `pfx`.
pub fn is_valid_decrypted_header(plain: &[u8], pfx: &UpkPrefix) -> bool {
    let c_off = pfx.compressed_chunks_offset;
    if c_off + 8 <= plain.len() && c_off > 0 {
        let num_c = read_i32_le(plain, c_off);
        let u0 = read_i32_le(plain, c_off + 4) as usize;
        return (1..=512).contains(&num_c) && u0 == pfx.depends_offset;
    }
    if let Some((first_name, next_pos)) = read_fstring_at(plain, 0) {
        if !first_name.is_empty()
            && first_name.len() <= 128
            && next_pos > 4
            && plain[next_pos - 1] == 0
            && first_name.chars().all(|c| c.is_ascii_graphic() || c == ' ')
        {
            return true;
        }
    }
    false
}

/// Decrypts the encrypted header table of a UPK and returns `(decrypted_header_bytes, key, is_ctr)`.
pub fn decrypt_upk_header(upk_path: &Path, raw: &[u8], pfx: &UpkPrefix) -> Result<(Vec<u8>, [u8; 32], bool), String> {
    let enc_size = pfx
        .total_header_size
        .saturating_sub(pfx.garbage_size)
        .saturating_sub(pfx.name_offset);
    let enc_aligned = (enc_size + 15) & !15;
    if pfx.name_offset + enc_aligned > raw.len() || enc_size == 0 {
        return Err("Invalid encrypted header bounds".into());
    }
    let enc_block = &raw[pfx.name_offset..pfx.name_offset + enc_aligned];

    // 1. Try exact lookup key first
    if let Some(k) = lookup_package_key(upk_path) {
        if let Some(nonce) = pfx.ctr_nonce {
            let ctr = aes_ctr_crypt(&k, &nonce, enc_block);
            if is_valid_decrypted_header(&ctr, pfx) {
                return Ok((ctr, k, true));
            }
        }
        let ecb = aes_ecb_decrypt(&k, enc_block);
        if is_valid_decrypted_header(&ecb, pfx) {
            return Ok((ecb, k, false));
        }
    }

    // 2. Parallel probe across all 711 keys using rayon
    let db = get_key_db();
    let found = db.keys_list.par_iter().find_map_first(|k| {
        if let Some(nonce) = pfx.ctr_nonce {
            let ctr = aes_ctr_crypt(k, &nonce, enc_block);
            if is_valid_decrypted_header(&ctr, pfx) {
                return Some((ctr, *k, true));
            }
        } else {
            let ecb = aes_ecb_decrypt(k, enc_block);
            if is_valid_decrypted_header(&ecb, pfx) {
                return Some((ecb, *k, false));
            }
        }
        None
    });

    found.ok_or_else(|| format!("Could not find AES key for {}", upk_path.display()))
}

// ── Zlib Chunk Decompression & Compression ───────────────────────────────────

#[derive(Debug, Clone)]
pub struct CompressedChunkEntry {
    pub uncompressed_offset: usize,
    pub uncompressed_size: usize,
    pub compressed_offset: usize,
    pub compressed_size: usize,
    pub chunk_nonce: Option<[u8; 12]>,
}

pub fn parse_chunk_table(plain_hdr: &[u8], pfx: &UpkPrefix, is_ctr: bool) -> Vec<CompressedChunkEntry> {
    let mut out = Vec::new();
    let c_off = pfx.compressed_chunks_offset;
    if c_off + 4 > plain_hdr.len() {
        return out;
    }
    let count = read_i32_le(plain_hdr, c_off);
    if count <= 0 || count > 512 {
        return out;
    }
    let has_nonce = pfx.licensee_version >= 33 || is_ctr;
    let stride = if has_nonce { 36 } else { 24 };
    for i in 0..(count as usize) {
        let off = c_off + 4 + i * stride;
        if off + stride > plain_hdr.len() {
            break;
        }
        let u_off = read_i64_le(plain_hdr, off).max(0) as usize;
        let u_sz = read_i32_le(plain_hdr, off + 8).max(0) as usize;
        let c_off_val = read_i64_le(plain_hdr, off + 12).max(0) as usize;
        let c_sz = read_i32_le(plain_hdr, off + 20).max(0) as usize;
        if u_sz > 256 * 1024 * 1024 || u_off > 512 * 1024 * 1024 {
            break;
        }
        let nonce = if has_nonce {
            let mut n = [0u8; 12];
            n.copy_from_slice(&plain_hdr[off + 24..off + 36]);
            Some(n)
        } else {
            None
        };
        out.push(CompressedChunkEntry {
            uncompressed_offset: u_off,
            uncompressed_size: u_sz,
            compressed_offset: c_off_val,
            compressed_size: c_sz,
            chunk_nonce: nonce,
        });
    }
    out
}

pub fn serialize_chunk_table(chunks: &[CompressedChunkEntry], has_nonce: bool) -> Vec<u8> {
    let stride = if has_nonce { 36 } else { 24 };
    let mut buf = vec![0u8; 4 + chunks.len() * stride];
    write_i32_le(&mut buf, 0, chunks.len() as i32);
    for (i, ch) in chunks.iter().enumerate() {
        let off = 4 + i * stride;
        write_i64_le(&mut buf, off, ch.uncompressed_offset as i64);
        write_i32_le(&mut buf, off + 8, ch.uncompressed_size as i32);
        write_i64_le(&mut buf, off + 12, ch.compressed_offset as i64);
        write_i32_le(&mut buf, off + 20, ch.compressed_size as i32);
        if has_nonce {
            if let Some(n) = ch.chunk_nonce {
                buf[off + 24..off + 36].copy_from_slice(&n);
            }
        }
    }
    buf
}

pub fn decompress_chunk_payload(payload: &[u8]) -> Result<Vec<u8>, String> {
    if payload.len() < 16 {
        return Err("Chunk payload too short".into());
    }
    let magic = read_u32_le(payload, 0);
    let block_size = read_i32_le(payload, 4) as usize;
    let total_uncomp = read_i32_le(payload, 12) as usize;
    if magic != PACKAGE_FILE_TAG || block_size == 0 || total_uncomp == 0 {
        return Err("Invalid chunk payload header".into());
    }
    let num_blocks = (total_uncomp + block_size - 1) / block_size;
    let mut hdr_off = 16usize;
    let mut blocks = Vec::with_capacity(num_blocks);
    for _ in 0..num_blocks {
        if hdr_off + 8 > payload.len() {
            return Err("Truncated chunk block header".into());
        }
        let b_comp = read_i32_le(payload, hdr_off) as usize;
        let b_uncomp = read_i32_le(payload, hdr_off + 4) as usize;
        blocks.push((b_comp, b_uncomp));
        hdr_off += 8;
    }
    let mut data_off = hdr_off;
    let mut out = Vec::with_capacity(total_uncomp);
    for (b_comp, _b_uncomp) in blocks {
        if data_off + b_comp > payload.len() {
            return Err("Truncated zlib block slice".into());
        }
        let mut decoder = ZlibDecoder::new(&payload[data_off..data_off + b_comp]);
        decoder
            .read_to_end(&mut out)
            .map_err(|e| format!("Zlib decompress error: {e}"))?;
        data_off += b_comp;
    }
    Ok(out)
}

pub fn compress_chunk_payload(uncompressed: &[u8]) -> Vec<u8> {
    let block_size = 131072usize;
    let total_uncomp = uncompressed.len();
    let num_blocks = (total_uncomp + block_size - 1) / block_size.max(1);
    let mut comp_blocks: Vec<(Vec<u8>, usize)> = Vec::with_capacity(num_blocks);

    for slice in uncompressed.chunks(block_size) {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(slice).unwrap();
        let comp = encoder.finish().unwrap();
        comp_blocks.push((comp, slice.len()));
    }

    let total_comp: usize = comp_blocks.iter().map(|(c, _)| c.len()).sum();
    let mut out = vec![0u8; 16 + num_blocks * 8 + total_comp];
    write_u32_le(&mut out, 0, PACKAGE_FILE_TAG);
    write_i32_le(&mut out, 4, block_size as i32);
    write_i32_le(&mut out, 8, total_comp as i32);
    write_i32_le(&mut out, 12, total_uncomp as i32);

    let mut hdr_off = 16usize;
    let mut data_off = 16 + num_blocks * 8;
    for (c, u_len) in comp_blocks {
        write_i32_le(&mut out, hdr_off, c.len() as i32);
        write_i32_le(&mut out, hdr_off + 4, u_len as i32);
        hdr_off += 8;
        out[data_off..data_off + c.len()].copy_from_slice(&c);
        data_off += c.len();
    }
    out
}

/// Decompresses all chunks in a UPK (fast path for thumbnail & map image extraction).
pub fn decompress_all_upk_chunks(upk_path: &Path) -> Result<Vec<Vec<u8>>, String> {
    let raw = fs::read(upk_path).map_err(|e| e.to_string())?;
    let pfx = parse_upk_prefix(&raw)?;
    if let Ok((plain_hdr, key, is_ctr)) = decrypt_upk_header(upk_path, &raw, &pfx) {
        let chunks = parse_chunk_table(&plain_hdr, &pfx, is_ctr);
        if !chunks.is_empty() {
            let mut out = Vec::with_capacity(chunks.len());
            for ch in chunks {
                if ch.compressed_offset + ch.compressed_size <= raw.len() {
                    let slice = &raw[ch.compressed_offset..ch.compressed_offset + ch.compressed_size];
                    let payload = if is_ctr {
                        let nonce = ch.chunk_nonce.unwrap_or([0u8; 12]);
                        aes_ctr_crypt(&key, &nonce, slice)
                    } else {
                        slice.to_vec()
                    };
                    if let Ok(uncomp) = decompress_chunk_payload(&payload) {
                        out.push(uncomp);
                    }
                }
            }
            if !out.is_empty() {
                return Ok(out);
            }
        }
    }

    // Fallback scan for unencrypted zlib chunk headers
    let mut out = Vec::new();
    let mut idx = 4usize.max(pfx.total_header_size);
    let magic_bytes = PACKAGE_FILE_TAG.to_le_bytes();
    while idx + 16 <= raw.len() {
        if let Some(rel) = raw[idx..].windows(4).position(|w| w == magic_bytes) {
            let pos = idx + rel;
            if pos + 16 <= raw.len() {
                let bsz = read_i32_le(&raw, pos + 4);
                let c_comp = read_i32_le(&raw, pos + 8);
                let c_uncomp = read_i32_le(&raw, pos + 12);
                if bsz == 131072 && c_comp > 0 && c_uncomp > 0 {
                    let n_blocks = ((c_uncomp as usize) + 131071) / 131072;
                    let full_len = 16 + n_blocks * 8 + (c_comp as usize);
                    if pos + full_len <= raw.len() {
                        if let Ok(uncomp) = decompress_chunk_payload(&raw[pos..pos + full_len]) {
                            out.push(uncomp);
                            idx = pos + full_len;
                            continue;
                        }
                    }
                }
            }
            idx = pos + 4;
        } else {
            break;
        }
    }
    if out.is_empty() {
        Err(format!("No valid chunks found in {}", upk_path.display()))
    } else {
        Ok(out)
    }
}

// ── Unpacked UPK Representation (Names, Imports, Exports) ────────────────────

#[derive(Debug, Clone)]
pub struct NameEntry {
    pub name: String,
    pub flags: u64,
    pub raw_offset: usize,
    pub raw_size: usize,
}

#[derive(Debug, Clone)]
pub struct ImportEntry {
    pub index: i32, // 1-based positive index (referred as -index in object refs)
    pub class_package: i32,
    pub class_name: i32,
    pub outer_index: i32,
    pub object_name: i32,
    pub object_name_num: i32,
    pub raw_offset: usize,
}

#[derive(Debug, Clone)]
pub struct ExportEntry {
    pub index: usize, // 1-based
    pub class_index: i32,
    pub super_index: i32,
    pub outer_index: i32,
    pub object_name: i32,
    pub object_name_num: i32,
    pub archetype_index: i32,
    pub object_flags: u64,
    pub serial_size: usize,
    pub serial_offset: usize,
    pub raw_offset: usize,
    pub raw_size: usize,
}

#[derive(Debug, Clone)]
pub struct UnpackedUpk {
    pub pfx: UpkPrefix,
    pub key: [u8; 32],
    pub is_ctr: bool,
    pub chunks: Vec<CompressedChunkEntry>,
    pub names: Vec<NameEntry>,
    pub imports: Vec<ImportEntry>,
    pub exports: Vec<ExportEntry>,
    pub file_bytes: Vec<u8>,
}

impl UnpackedUpk {
    pub fn resolve_name(&self, name_idx: i32) -> &str {
        if name_idx >= 0 && (name_idx as usize) < self.names.len() {
            &self.names[name_idx as usize].name
        } else {
            ""
        }
    }

    pub fn resolve_name_with_num(&self, name_idx: i32, num: i32) -> String {
        let base = self.resolve_name(name_idx);
        if num > 0 {
            format!("{}_{}", base, num - 1)
        } else {
            base.to_string()
        }
    }

    pub fn export_class_name(&self, exp: &ExportEntry) -> &str {
        if exp.class_index < 0 {
            let imp_idx = (-exp.class_index - 1) as usize;
            if imp_idx < self.imports.len() {
                return self.resolve_name(self.imports[imp_idx].object_name);
            }
        } else if exp.class_index > 0 {
            let e_idx = (exp.class_index - 1) as usize;
            if e_idx < self.exports.len() {
                return self.resolve_name(self.exports[e_idx].object_name);
            }
        }
        "Class"
    }

    pub fn full_export_path(&self, exp_index_1based: usize) -> String {
        let mut parts = Vec::new();
        let mut cur = exp_index_1based as i32;
        let mut depth = 0;
        while cur > 0 && (cur as usize) <= self.exports.len() && depth < 16 {
            let e = &self.exports[(cur - 1) as usize];
            parts.push(self.resolve_name_with_num(e.object_name, e.object_name_num));
            cur = e.outer_index;
            depth += 1;
        }
        parts.reverse();
        parts.join(".")
    }

    pub fn find_name_index(&self, target: &str) -> Option<i32> {
        let low = target.to_lowercase();
        self.names
            .iter()
            .position(|n| n.name.to_lowercase() == low)
            .map(|i| i as i32)
    }
}

pub fn unpack_upk_from_disk(upk_path: &Path) -> Result<(UnpackedUpk, Vec<u8>), String> {
    let orig_raw = fs::read(upk_path).map_err(|e| e.to_string())?;
    unpack_upk_from_raw(orig_raw, upk_path)
}

pub fn unpack_upk_from_raw(orig_raw: Vec<u8>, upk_path: &Path) -> Result<(UnpackedUpk, Vec<u8>), String> {
    let pfx = parse_upk_prefix(&orig_raw)?;
    let (plain_hdr, key, is_ctr) = decrypt_upk_header(upk_path, &orig_raw, &pfx)?;
    let chunks = parse_chunk_table(&plain_hdr, &pfx, is_ctr);
    if chunks.is_empty() {
        return Err(format!("No compressed chunks found in {}", upk_path.display()));
    }

    let total_uncomp_end = chunks
        .iter()
        .map(|c| c.uncompressed_offset + c.uncompressed_size)
        .max()
        .unwrap_or(pfx.depends_offset);

    let mut file_bytes = vec![0u8; total_uncomp_end];
    file_bytes[..pfx.name_offset].copy_from_slice(&orig_raw[..pfx.name_offset]);
    let hdr_copy_end = pfx.depends_offset.min(pfx.name_offset + plain_hdr.len());
    let hdr_slice_len = hdr_copy_end.saturating_sub(pfx.name_offset);
    file_bytes[pfx.name_offset..pfx.name_offset + hdr_slice_len]
        .copy_from_slice(&plain_hdr[..hdr_slice_len]);

    // Decompress chunks in parallel with rayon!
    let decompressed_slices: Result<Vec<(usize, Vec<u8>)>, String> = chunks
        .par_iter()
        .map(|ch| {
            if ch.compressed_offset + ch.compressed_size > orig_raw.len() {
                return Err("Chunk offset out of bounds".to_string());
            }
            let raw_slice = &orig_raw[ch.compressed_offset..ch.compressed_offset + ch.compressed_size];
            let payload = if is_ctr {
                let nonce = ch.chunk_nonce.unwrap_or([0u8; 12]);
                aes_ctr_crypt(&key, &nonce, raw_slice)
            } else {
                raw_slice.to_vec()
            };
            let uncomp = decompress_chunk_payload(&payload)?;
            Ok((ch.uncompressed_offset, uncomp))
        })
        .collect();

    for (u_off, uncomp) in decompressed_slices? {
        let end = (u_off + uncomp.len()).min(file_bytes.len());
        file_bytes[u_off..end].copy_from_slice(&uncomp[..end - u_off]);
    }

    // Parse NameTable
    let mut names = Vec::with_capacity(pfx.name_count);
    let mut npos = pfx.name_offset;
    for _ in 0..pfx.name_count {
        let start = npos;
        let (s, next_p) = read_fstring_at(&file_bytes, npos).ok_or("Failed reading NameEntry")?;
        if next_p + 8 > file_bytes.len() {
            return Err("Truncated NameEntry flags".into());
        }
        let flags = read_i64_le(&file_bytes, next_p) as u64;
        npos = next_p + 8;
        names.push(NameEntry {
            name: s,
            flags,
            raw_offset: start,
            raw_size: npos - start,
        });
    }

    // Parse ImportTable
    let mut imports = Vec::with_capacity(pfx.import_count);
    let mut ipos = pfx.import_offset;
    for i in 0..pfx.import_count {
        if ipos + 28 > file_bytes.len() {
            break;
        }
        imports.push(ImportEntry {
            index: (i + 1) as i32,
            class_package: read_i32_le(&file_bytes, ipos),
            class_name: read_i32_le(&file_bytes, ipos + 8),
            outer_index: read_i32_le(&file_bytes, ipos + 16),
            object_name: read_i32_le(&file_bytes, ipos + 20),
            object_name_num: read_i32_le(&file_bytes, ipos + 24),
            raw_offset: ipos,
        });
        ipos += 28;
    }

    // Parse ExportTable
    let mut exports = Vec::with_capacity(pfx.export_count);
    let mut epos = pfx.export_offset;
    for i in 0..pfx.export_count {
        if epos + 68 > file_bytes.len() {
            break;
        }
        let cls_i = read_i32_le(&file_bytes, epos);
        let sup_i = read_i32_le(&file_bytes, epos + 4);
        let out_i = read_i32_le(&file_bytes, epos + 8);
        let obj_nm = read_i32_le(&file_bytes, epos + 12);
        let obj_num = read_i32_le(&file_bytes, epos + 16);
        let arch_i = read_i32_le(&file_bytes, epos + 20);
        let flags = read_i64_le(&file_bytes, epos + 24) as u64;
        let ser_sz = read_i32_le(&file_bytes, epos + 32).max(0) as usize;
        let ser_off = read_i64_le(&file_bytes, epos + 36).max(0) as usize;
        let net_cnt = read_i32_le(&file_bytes, epos + 48).max(0) as usize;
        let entry_sz = 52 + net_cnt * 4 + 20;
        exports.push(ExportEntry {
            index: i + 1,
            class_index: cls_i,
            super_index: sup_i,
            outer_index: out_i,
            object_name: obj_nm,
            object_name_num: obj_num,
            archetype_index: arch_i,
            object_flags: flags,
            serial_size: ser_sz,
            serial_offset: ser_off,
            raw_offset: epos,
            raw_size: entry_sz,
        });
        epos += entry_sz;
    }

    Ok((
        UnpackedUpk {
            pfx,
            key,
            is_ctr,
            chunks,
            names,
            imports,
            exports,
            file_bytes,
        },
        orig_raw,
    ))
}

/// High-speed selective chunk rebuilder returning the built `.upk` bytes in memory.
pub fn rebuild_upk_selective_chunks_to_vec(
    orig_raw: &[u8],
    orig_unpacked: &UnpackedUpk,
    modified_file_bytes: &[u8],
    output_key: &[u8; 32],
) -> Result<Vec<u8>, String> {
    let pfx = &orig_unpacked.pfx;
    let is_ctr = orig_unpacked.is_ctr;
    let donor_key = &orig_unpacked.key;

    let enc_size = pfx
        .total_header_size
        .saturating_sub(pfx.garbage_size)
        .saturating_sub(pfx.name_offset);
    let enc_aligned = (enc_size + 15) & !15;
    let enc_block = &orig_raw[pfx.name_offset..pfx.name_offset + enc_aligned];

    let mut plain_hdr = if is_ctr {
        let nonce = pfx.ctr_nonce.unwrap_or([0u8; 12]);
        aes_ctr_crypt(donor_key, &nonce, enc_block)
    } else {
        aes_ecb_decrypt(donor_key, enc_block)
    };

    // Copy any header table edits (e.g. ExportTable rewiring) from modified_file_bytes into plain_hdr
    let table_len = pfx.depends_offset.saturating_sub(pfx.name_offset).min(plain_hdr.len());
    plain_hdr[..table_len].copy_from_slice(&modified_file_bytes[pfx.name_offset..pfx.name_offset + table_len]);

    // Process chunks in parallel using rayon!
    let payloads_and_sizes: Vec<(Vec<u8>, usize, Option<[u8; 12]>)> = orig_unpacked
        .chunks
        .par_iter()
        .map(|ch| {
            let start = ch.uncompressed_offset;
            let end = (start + ch.uncompressed_size).min(modified_file_bytes.len());
            let orig_slice = &orig_unpacked.file_bytes[start..end];
            let mod_slice = &modified_file_bytes[start..end];

            if orig_slice == mod_slice && modified_file_bytes.len() == orig_unpacked.file_bytes.len() {
                let raw_chunk = &orig_raw[ch.compressed_offset..ch.compressed_offset + ch.compressed_size];
                let payload = if is_ctr && output_key != donor_key {
                    let nonce = ch.chunk_nonce.unwrap_or([0u8; 12]);
                    let dec = aes_ctr_crypt(donor_key, &nonce, raw_chunk);
                    aes_ctr_crypt(output_key, &nonce, &dec)
                } else {
                    raw_chunk.to_vec()
                };
                (payload, end - start, ch.chunk_nonce)
            } else {
                let comp = compress_chunk_payload(mod_slice);
                let payload = if is_ctr {
                    let nonce = ch.chunk_nonce.unwrap_or([0u8; 12]);
                    aes_ctr_crypt(output_key, &nonce, &comp)
                } else {
                    comp
                };
                (payload, end - start, ch.chunk_nonce)
            }
        })
        .collect();

    let mut rebuilt_chunks = Vec::with_capacity(orig_unpacked.chunks.len());
    let mut cur_comp_off = orig_unpacked.chunks[0].compressed_offset;
    for (i, (payload, u_sz, nonce)) in payloads_and_sizes.iter().enumerate() {
        rebuilt_chunks.push(CompressedChunkEntry {
            uncompressed_offset: orig_unpacked.chunks[i].uncompressed_offset,
            uncompressed_size: *u_sz,
            compressed_offset: cur_comp_off,
            compressed_size: payload.len(),
            chunk_nonce: *nonce,
        });
        cur_comp_off += payload.len();
    }

    let has_nonce = pfx.licensee_version >= 33 || is_ctr;
    let chunk_table_bytes = serialize_chunk_table(&rebuilt_chunks, has_nonce);
    let c_off = pfx.compressed_chunks_offset;
    if c_off + chunk_table_bytes.len() <= plain_hdr.len() {
        plain_hdr[c_off..c_off + chunk_table_bytes.len()].copy_from_slice(&chunk_table_bytes);
    }

    let new_enc_hdr = if is_ctr {
        let nonce = pfx.ctr_nonce.unwrap_or([0u8; 12]);
        aes_ctr_crypt(output_key, &nonce, &plain_hdr)
    } else {
        aes_ecb_encrypt(output_key, &plain_hdr[..enc_aligned])
    };

    let gap_start = pfx.name_offset + enc_aligned;
    let gap_end = orig_unpacked.chunks[0].compressed_offset;
    let total_out_len = gap_end + payloads_and_sizes.iter().map(|(p, _, _)| p.len()).sum::<usize>();
    let mut out_buf = Vec::with_capacity(total_out_len);
    out_buf.extend_from_slice(&orig_raw[..pfx.name_offset]);
    out_buf.extend_from_slice(&new_enc_hdr[..enc_aligned]);
    out_buf.extend_from_slice(&orig_raw[gap_start..gap_end]);
    for (p, _, _) in payloads_and_sizes {
        out_buf.extend_from_slice(&p);
    }
    Ok(out_buf)
}

/// High-speed selective chunk rebuilder:
/// Only recompresses chunks whose uncompressed slice actually changed, reusing unmodified compressed chunks verbatim!
pub fn rebuild_upk_selective_chunks(
    orig_raw: &[u8],
    orig_unpacked: &UnpackedUpk,
    modified_file_bytes: &[u8],
    output_key: &[u8; 32],
    output_path: &Path,
) -> Result<(), String> {
    let out_buf = rebuild_upk_selective_chunks_to_vec(orig_raw, orig_unpacked, modified_file_bytes, output_key)?;
    if let Some(parent) = output_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(output_path, &out_buf).map_err(|e| e.to_string())
}

/// Replaces one export's serialized payload (even if larger/smaller), shifts downstream ExportTable offsets
/// and chunk boundaries cleanly, reuses untouched chunks verbatim, and writes the rebuilt `.upk`.
fn extract_inline_tex2d_alloc_info(
    upk: &UnpackedUpk,
    added_names: &[String],
    serial: &[u8],
) -> Option<(i32, i32, i32, i32)> {
    let limit = serial.len().min(32);
    let mut end_off = 0usize;
    let mut sx = 0i32;
    let mut sy = 0i32;
    let mut fmt_name = String::new();
    for st in (0..limit).step_by(4).chain((0..limit).filter(|x| x % 4 != 0)) {
        let (eo, props, ok) = parse_property_stream(upk, serial, st);
        if ok && !props.is_empty() {
            end_off = eo;
            for p in props {
                match p.name.as_str() {
                    "SizeX" if p.size == 4 => sx = read_i32_le(serial, p.value_offset),
                    "SizeY" if p.size == 4 => sy = read_i32_le(serial, p.value_offset),
                    "Format" if p.size >= 8 => {
                        let idx = read_i32_le(serial, p.value_offset);
                        if idx >= 0 && (idx as usize) < upk.names.len() {
                            fmt_name = upk.resolve_name(idx).to_string();
                        } else if idx >= upk.names.len() as i32 {
                            let ai = (idx as usize) - upk.names.len();
                            if let Some(s) = added_names.get(ai) {
                                fmt_name = s.clone();
                            }
                        }
                    }
                    _ => {}
                }
            }
            break;
        }
    }
    if end_off == 0 || sx <= 0 || sy <= 0 {
        return None;
    }
    let fmt_code = match fmt_name.as_str() {
        "PF_A8R8G8B8" => 2,
        "PF_G8" => 3,
        "PF_DXT1" => 5,
        "PF_DXT3" => 6,
        "PF_DXT5" => 7,
        _ => return None,
    };
    let mut mip_count = 1i32;
    if end_off + 16 <= serial.len() {
        let sa_flags = read_u32_le(serial, end_off);
        let sa_len = if (sa_flags & 0x21) != 0 { 16 } else { 12 };
        if end_off + sa_len + 4 <= serial.len() {
            let mc = read_i32_le(serial, end_off + sa_len);
            if (1..=16).contains(&mc) {
                mip_count = mc;
            }
        }
    }
    Some((sx, sy, mip_count, fmt_code))
}

fn build_updated_texture_allocations_bytes(
    orig_raw: &[u8],
    pfx: &UpkPrefix,
    updates: &HashMap<i32, (i32, i32, i32, i32)>,
) -> Option<(Vec<u8>, usize)> {
    if updates.is_empty() || pfx.texture_allocations_off + 4 > pfx.compression_meta_off {
        return None;
    }
    let old_len = pfx.compression_meta_off - pfx.texture_allocations_off;
    let ta_count = read_i32_le(orig_raw, pfx.texture_allocations_off).max(0) as usize;
    if ta_count > 256 {
        return None;
    }
    let mut tpos = pfx.texture_allocations_off + 4;
    struct TaEntry {
        sx: i32,
        sy: i32,
        mips: i32,
        fmt: i32,
        flags: i32,
        exps: Vec<i32>,
    }
    let mut kept: Vec<TaEntry> = Vec::with_capacity(ta_count + updates.len());
    for _ in 0..ta_count {
        if tpos + 24 > pfx.compression_meta_off {
            return None;
        }
        let sx = read_i32_le(orig_raw, tpos);
        let sy = read_i32_le(orig_raw, tpos + 4);
        let mips = read_i32_le(orig_raw, tpos + 8);
        let fmt = read_i32_le(orig_raw, tpos + 12);
        let flags = read_i32_le(orig_raw, tpos + 16);
        let exp_len = read_i32_le(orig_raw, tpos + 20).max(0) as usize;
        if tpos + 24 + exp_len * 4 > pfx.compression_meta_off {
            return None;
        }
        let mut exps = Vec::with_capacity(exp_len);
        for k in 0..exp_len {
            let id = read_i32_le(orig_raw, tpos + 24 + k * 4);
            if !updates.contains_key(&id) && !exps.contains(&id) {
                exps.push(id);
            }
        }
        if !exps.is_empty() {
            kept.push(TaEntry { sx, sy, mips, fmt, flags, exps });
        }
        tpos += 24 + exp_len * 4;
    }
    if tpos != pfx.compression_meta_off {
        return None;
    }

    let mut sorted_upd: Vec<(i32, (i32, i32, i32, i32))> = updates.iter().map(|(&k, &v)| (k, v)).collect();
    sorted_upd.sort_by_key(|(id, _)| *id);
    for (exp_id, (sx, sy, mips, fmt_code)) in sorted_upd {
        if let Some(existing) = kept
            .iter_mut()
            .find(|e| e.sx == sx && e.sy == sy && e.mips == mips && e.fmt == fmt_code && e.flags == 0)
        {
            if !existing.exps.contains(&exp_id) {
                existing.exps.push(exp_id);
            }
        } else {
            kept.push(TaEntry {
                sx,
                sy,
                mips,
                fmt: fmt_code,
                flags: 0,
                exps: vec![exp_id],
            });
        }
    }

    let mut out = Vec::with_capacity(4 + kept.len() * 28);
    out.extend_from_slice(&(kept.len() as i32).to_le_bytes());
    for e in &kept {
        out.extend_from_slice(&e.sx.to_le_bytes());
        out.extend_from_slice(&e.sy.to_le_bytes());
        out.extend_from_slice(&e.mips.to_le_bytes());
        out.extend_from_slice(&e.fmt.to_le_bytes());
        out.extend_from_slice(&e.flags.to_le_bytes());
        out.extend_from_slice(&(e.exps.len() as i32).to_le_bytes());
        for &id in &e.exps {
            out.extend_from_slice(&id.to_le_bytes());
        }
    }
    Some((out, old_len))
}

/// Replaces zero or more exports with resized serial blobs on top of `base_mod_bytes`, optionally
/// appends brand-new top-level exports (`(name, template_exp_idx, serial_bytes)`), shifts header
/// & chunk tables cleanly, and returns encrypted UPK bytes.
pub fn rebuild_upk_with_mod_resized_added_and_names_to_vec(
    orig_raw: &[u8],
    orig_unpacked: &UnpackedUpk,
    base_mod_bytes: &[u8],
    replacements: &[(usize, Vec<u8>)],
    added_exports: &[(String, usize, Vec<u8>)],
    added_names: &[String],
    output_key: &[u8; 32],
) -> Result<Vec<u8>, String> {
    if replacements.is_empty() && added_exports.is_empty() && added_names.is_empty() {
        return rebuild_upk_selective_chunks_to_vec(orig_raw, orig_unpacked, base_mod_bytes, output_key);
    }
    if base_mod_bytes.len() != orig_unpacked.file_bytes.len() {
        return Err("base_mod_bytes length mismatch".into());
    }

    #[derive(Clone, Copy)]
    struct RepSpan {
        exp_idx: usize,
        old_off: usize,
        old_end: usize,
        delta: isize,
        new_len: usize,
    }

    let mut sorted_idx: Vec<usize> = (0..replacements.len()).collect();
    sorted_idx.sort_by_key(|&ri| orig_unpacked.exports[replacements[ri].0].serial_offset);

    let mut spans = Vec::with_capacity(replacements.len());
    let mut prev_end = 0usize;
    let mut total_delta = 0isize;
    for ri in sorted_idx {
        let (exp_idx, ref new_bytes) = replacements[ri];
        if exp_idx >= orig_unpacked.exports.len() {
            return Err("Export index out of range".into());
        }
        let exp = &orig_unpacked.exports[exp_idx];
        let old_off = exp.serial_offset;
        let old_end = old_off + exp.serial_size;
        if old_off < prev_end || old_end > base_mod_bytes.len() {
            return Err("Invalid or overlapping export serial bounds".into());
        }
        let delta = new_bytes.len() as isize - exp.serial_size as isize;
        total_delta += delta;
        prev_end = old_end;
        spans.push((RepSpan { exp_idx, old_off, old_end, delta, new_len: new_bytes.len() }, new_bytes.as_slice()));
    }

    let added_serial_len: usize = added_exports.iter().map(|(_, _, b)| b.len()).sum();
    let mut mod_bytes = Vec::with_capacity(((base_mod_bytes.len() as isize + total_delta).max(0) as usize) + added_serial_len);
    let mut cursor = 0usize;
    for (sp, new_bytes) in &spans {
        mod_bytes.extend_from_slice(&base_mod_bytes[cursor..sp.old_off]);
        mod_bytes.extend_from_slice(new_bytes);
        cursor = sp.old_end;
    }
    mod_bytes.extend_from_slice(&base_mod_bytes[cursor..]);

    // Append new export payloads cleanly at the end of mod_bytes (expanding only the final chunk)
    let mut added_locs: Vec<(usize, usize)> = Vec::with_capacity(added_exports.len());
    for (_, _, new_blob) in added_exports {
        let off = mod_bytes.len();
        mod_bytes.extend_from_slice(new_blob);
        added_locs.push((off, new_blob.len()));
    }

    // Update replaced export serial_sizes and shift every existing export's serial_offset accurately
    for (sp, _) in &spans {
        let raw_off = orig_unpacked.exports[sp.exp_idx].raw_offset;
        write_i32_le(&mut mod_bytes, raw_off + 32, sp.new_len as i32);
    }
    let mut exp_shifts = vec![0isize; orig_unpacked.exports.len()];
    for (idx, exp) in orig_unpacked.exports.iter().enumerate() {
        let mut shift = 0isize;
        for (sp, _) in &spans {
            if idx != sp.exp_idx && sp.old_off < exp.serial_offset {
                shift += sp.delta;
            }
        }
        exp_shifts[idx] = shift;
        if shift != 0 {
            write_i64_le(&mut mod_bytes, exp.raw_offset + 36, (exp.serial_offset as isize + shift) as i64);
        }
    }

    let pfx = &orig_unpacked.pfx;
    let is_ctr = orig_unpacked.is_ctr;
    let donor_key = &orig_unpacked.key;
    let enc_size = pfx
        .total_header_size
        .saturating_sub(pfx.garbage_size)
        .saturating_sub(pfx.name_offset);
    let enc_aligned = (enc_size + 15) & !15;
    let enc_block = &orig_raw[pfx.name_offset..pfx.name_offset + enc_aligned];

    let mut plain_hdr = if is_ctr {
        let nonce = pfx.ctr_nonce.unwrap_or([0u8; 12]);
        aes_ctr_crypt(donor_key, &nonce, enc_block)
    } else {
        aes_ecb_decrypt(donor_key, enc_block)
    };
    let table_len = pfx.depends_offset.saturating_sub(pfx.name_offset).min(plain_hdr.len());
    plain_hdr[..table_len].copy_from_slice(&mod_bytes[pfx.name_offset..pfx.name_offset + table_len]);
    let depends_rel = pfx.depends_offset.saturating_sub(pfx.name_offset);
    if enc_size > depends_rel && enc_size <= plain_hdr.len() {
        plain_hdr.truncate(enc_size);
    }

    // Expand NameTable & ExportTable when added_names or added_exports is non-empty
    let mut extra_nt = Vec::new();
    let mut extra_exp = Vec::new();
    let mut new_name_count = pfx.name_count;
    let mut new_export_count = pfx.export_count;
    let mut new_import_rel = pfx.import_offset.saturating_sub(pfx.name_offset);
    let mut new_export_rel = pfx.export_offset.saturating_sub(pfx.name_offset);
    let mut new_depends_rel = depends_rel;

    if !added_exports.is_empty() || !added_names.is_empty() {
        let mut npos = 0usize;
        let mut names_list = Vec::with_capacity(pfx.name_count + added_names.len() + added_exports.len());
        let def_flags = 0x0007_0010_0000_0000u64;
        for _ in 0..pfx.name_count {
            let Some((nm, next_p)) = read_fstring_at(&plain_hdr, npos) else {
                return Err("NameTable truncated while adding exports".into());
            };
            names_list.push(nm);
            npos = next_p + 8;
        }
        let nt_end = npos;
        let nt_tail = &plain_hdr[nt_end..new_import_rel];

        for req_nm in added_names {
            if !names_list.iter().any(|n| n.eq_ignore_ascii_case(req_nm)) {
                extra_nt.extend_from_slice(&serialize_name_entry(req_nm, def_flags));
                names_list.push(req_nm.clone());
            }
        }

        for (i, (new_nm, tmpl_idx, _)) in added_exports.iter().enumerate() {
            let nm_low = new_nm.to_ascii_lowercase();
            let nm_idx = if let Some(p) = names_list.iter().position(|n| n.to_ascii_lowercase() == nm_low) {
                p
            } else {
                extra_nt.extend_from_slice(&serialize_name_entry(new_nm, def_flags));
                names_list.push(new_nm.clone());
                names_list.len() - 1
            };
            let tmpl_exp = orig_unpacked.exports.get(*tmpl_idx).ok_or("Template export index out of range")?;
            let tmpl_rel = tmpl_exp.raw_offset - pfx.name_offset;
            let net_cnt = read_i32_le(&plain_hdr, tmpl_rel + 48).max(0) as usize;
            let entry_len = 52 + net_cnt * 4 + 20;
            let mut raw_e = plain_hdr[tmpl_rel..tmpl_rel + entry_len].to_vec();
            write_i32_le(&mut raw_e, 8, tmpl_exp.outer_index);
            write_i32_le(&mut raw_e, 12, nm_idx as i32);
            write_i32_le(&mut raw_e, 16, 0); // Instance = 0
            write_i32_le(&mut raw_e, 32, added_locs[i].1 as i32);
            write_i64_le(&mut raw_e, 36, added_locs[i].0 as i64); // will shift by header_delta below
            extra_exp.extend_from_slice(&raw_e);
        }

        new_name_count = names_list.len();
        new_export_count = if added_exports.is_empty() {
            pfx.export_count
        } else {
            orig_unpacked.exports.len() + added_exports.len()
        };
        let nt_delta = extra_nt.len();
        new_import_rel += nt_delta;
        new_export_rel += nt_delta;
        new_depends_rel = depends_rel + nt_delta + extra_exp.len();
        let header_delta = (new_depends_rel as isize) - (depends_rel as isize);

        // Build expanded plain_hdr before CompressedChunks
        let imp_blob = &plain_hdr[pfx.import_offset.saturating_sub(pfx.name_offset)..pfx.export_offset.saturating_sub(pfx.name_offset)];
        let exp_blob = &plain_hdr[pfx.export_offset.saturating_sub(pfx.name_offset)..depends_rel];
        let beyond = &plain_hdr[depends_rel..];

        let mut expanded_hdr = Vec::with_capacity(new_depends_rel + beyond.len());
        expanded_hdr.extend_from_slice(&plain_hdr[..nt_end]);
        expanded_hdr.extend_from_slice(&extra_nt);
        expanded_hdr.extend_from_slice(nt_tail);
        expanded_hdr.extend_from_slice(imp_blob);
        expanded_hdr.extend_from_slice(exp_blob);
        expanded_hdr.extend_from_slice(&extra_exp);
        expanded_hdr.extend_from_slice(beyond);

        // Shift ALL ExportEntry serial_offsets in expanded_hdr by +header_delta
        let mut ep_cursor = new_export_rel;
        for _ in 0..new_export_count {
            let net_cnt = read_i32_le(&expanded_hdr, ep_cursor + 48).max(0) as usize;
            let entry_len = 52 + net_cnt * 4 + 20;
            let cur_off = read_i64_le(&expanded_hdr, ep_cursor + 36);
            if cur_off > 0 {
                write_i64_le(&mut expanded_hdr, ep_cursor + 36, cur_off + header_delta as i64);
            }
            ep_cursor += entry_len;
        }
        plain_hdr = expanded_hdr;
    }

    let mut ta_updates: HashMap<i32, (i32, i32, i32, i32)> = HashMap::new();
    for (sp, new_bytes) in &spans {
        if orig_unpacked.export_class_name(&orig_unpacked.exports[sp.exp_idx]) == "Texture2D" {
            if let Some(info) = extract_inline_tex2d_alloc_info(orig_unpacked, added_names, new_bytes) {
                ta_updates.insert(sp.exp_idx as i32, info);
            }
        }
    }
    for (i, (_, tmpl_idx, new_blob)) in added_exports.iter().enumerate() {
        if let Some(tmpl_exp) = orig_unpacked.exports.get(*tmpl_idx) {
            if orig_unpacked.export_class_name(tmpl_exp) == "Texture2D" {
                if let Some(info) = extract_inline_tex2d_alloc_info(orig_unpacked, added_names, new_blob) {
                    ta_updates.insert((orig_unpacked.exports.len() + i) as i32, info);
                }
            }
        }
    }
    let updated_ta = build_updated_texture_allocations_bytes(orig_raw, pfx, &ta_updates);
    let ta_delta: isize = updated_ta
        .as_ref()
        .map(|(new_ta, old_ta_len)| new_ta.len() as isize - *old_ta_len as isize)
        .unwrap_or(0);

    let header_delta = new_depends_rel as isize - depends_rel as isize;
    if ta_delta != 0 {
        // `added_exports` / `added_names` already shifted by +header_delta above; apply +ta_delta to all exports.
        let mut ep_cursor = new_export_rel;
        for _ in 0..new_export_count {
            let net_cnt = read_i32_le(&plain_hdr, ep_cursor + 48).max(0) as usize;
            let entry_len = 52 + net_cnt * 4 + 20;
            let cur_off = read_i64_le(&plain_hdr, ep_cursor + 36);
            if cur_off > 0 {
                write_i64_le(&mut plain_hdr, ep_cursor + 36, cur_off + ta_delta as i64);
            }
            ep_cursor += entry_len;
        }
    }

    let map_bound = |x: usize| -> usize {
        let mut cum = 0isize;
        for (sp, _) in &spans {
            if x <= sp.old_off {
                return (x as isize + cum) as usize;
            } else if x < sp.old_end {
                return (sp.old_end as isize + cum + sp.delta) as usize;
            }
            cum += sp.delta;
        }
        (x as isize + cum) as usize
    };

    let last_ch_idx = orig_unpacked.chunks.len().saturating_sub(1);
    let payloads_and_bounds: Vec<(Vec<u8>, usize, usize, Option<[u8; 12]>)> = orig_unpacked
        .chunks
        .par_iter()
        .enumerate()
        .map(|(c_idx, ch)| {
            let old_s = ch.uncompressed_offset;
            let old_e = (old_s + ch.uncompressed_size).min(orig_unpacked.file_bytes.len());
            let new_s = map_bound(old_s);
            let mut new_e = map_bound(old_e).min(mod_bytes.len());
            if c_idx == last_ch_idx && !added_exports.is_empty() {
                new_e = mod_bytes.len();
            }
            let overlaps = spans.iter().any(|(sp, _)| old_e > sp.old_off && old_s < sp.old_end)
                || (c_idx == last_ch_idx && !added_exports.is_empty());

            if !overlaps
                && (new_e - new_s) == (old_e - old_s)
                && &orig_unpacked.file_bytes[old_s..old_e] == &mod_bytes[new_s..new_e]
            {
                let raw_chunk = &orig_raw[ch.compressed_offset..ch.compressed_offset + ch.compressed_size];
                let payload = if is_ctr && output_key != donor_key {
                    let nonce = ch.chunk_nonce.unwrap_or([0u8; 12]);
                    let dec = aes_ctr_crypt(donor_key, &nonce, raw_chunk);
                    aes_ctr_crypt(output_key, &nonce, &dec)
                } else {
                    raw_chunk.to_vec()
                };
                (payload, (new_s as isize + header_delta + ta_delta) as usize, new_e - new_s, ch.chunk_nonce)
            } else {
                let slice = &mod_bytes[new_s..new_e];
                let comp = compress_chunk_payload(slice);
                let payload = if is_ctr {
                    let nonce = ch.chunk_nonce.unwrap_or([0u8; 12]);
                    aes_ctr_crypt(output_key, &nonce, &comp)
                } else {
                    comp
                };
                (payload, (new_s as isize + header_delta + ta_delta) as usize, new_e - new_s, ch.chunk_nonce)
            }
        })
        .collect();

    let new_enc_aligned = (plain_hdr.len() + 15) & !15;
    let size_growth = new_enc_aligned as isize - enc_aligned as isize;
    let avail_gap = (pfx.total_header_size.saturating_sub(pfx.name_offset)) as isize - enc_aligned as isize;
    let needed_space = (size_growth + ta_delta - avail_gap).max(header_delta + ta_delta - pfx.garbage_size as isize);
    let extra_file_shift: isize = if needed_space > 0 {
        ((needed_space as usize + 255) & !15) as isize
    } else {
        0
    };

    let mut rebuilt_chunks = Vec::with_capacity(orig_unpacked.chunks.len());
    let mut cur_comp_off = (orig_unpacked.chunks[0].compressed_offset as isize + extra_file_shift) as usize;
    for (payload, u_off, u_sz, nonce) in &payloads_and_bounds {
        rebuilt_chunks.push(CompressedChunkEntry {
            uncompressed_offset: *u_off,
            uncompressed_size: *u_sz,
            compressed_offset: cur_comp_off,
            compressed_size: payload.len(),
            chunk_nonce: *nonce,
        });
        cur_comp_off += payload.len();
    }

    let has_nonce = pfx.licensee_version >= 33 || is_ctr;
    let chunk_table_bytes = serialize_chunk_table(&rebuilt_chunks, has_nonce);
    let c_off = (pfx.compressed_chunks_offset as isize + header_delta).max(0) as usize;
    if c_off + chunk_table_bytes.len() <= plain_hdr.len() {
        plain_hdr[c_off..c_off + chunk_table_bytes.len()].copy_from_slice(&chunk_table_bytes);
    }
    plain_hdr.resize(new_enc_aligned, 0);

    let new_enc_hdr = if is_ctr {
        let nonce = pfx.ctr_nonce.unwrap_or([0u8; 12]);
        aes_ctr_crypt(output_key, &nonce, &plain_hdr)
    } else {
        aes_ecb_encrypt(output_key, &plain_hdr[..new_enc_aligned])
    };

    let old_gap_start = pfx.name_offset + enc_aligned;
    let old_gap_end = orig_unpacked.chunks[0].compressed_offset;
    let mut out_prefix = if let Some((ref new_ta, _)) = updated_ta {
        let mut v = Vec::with_capacity((pfx.name_offset as isize + ta_delta).max(0) as usize);
        v.extend_from_slice(&orig_raw[..pfx.texture_allocations_off]);
        v.extend_from_slice(new_ta);
        v.extend_from_slice(&orig_raw[pfx.compression_meta_off..pfx.name_offset]);
        v
    } else {
        orig_raw[..pfx.name_offset].to_vec()
    };
    if header_delta != 0
        || ta_delta != 0
        || extra_file_shift != 0
        || new_name_count != pfx.name_count
        || new_export_count != pfx.export_count
    {
        if extra_file_shift != 0 {
            write_i32_le(&mut out_prefix, pfx.total_header_size_off, (pfx.total_header_size as isize + extra_file_shift) as i32);
        }
        if ta_delta != 0 {
            write_i32_le(&mut out_prefix, pfx.name_offset_off, (pfx.name_offset as isize + ta_delta) as i32);
        }
        write_i32_le(&mut out_prefix, pfx.name_count_off, new_name_count as i32);
        write_i32_le(&mut out_prefix, pfx.export_count_off, new_export_count as i32);
        write_i32_le(&mut out_prefix, pfx.import_offset_off, (pfx.name_offset as isize + ta_delta + new_import_rel as isize) as i32);
        write_i32_le(&mut out_prefix, pfx.export_offset_off, (pfx.name_offset as isize + ta_delta + new_export_rel as isize) as i32);
        write_i32_le(&mut out_prefix, pfx.depends_offset_off, (pfx.name_offset as isize + ta_delta + new_depends_rel as isize) as i32);
        let old_guids_off = read_i32_le(&out_prefix, pfx.import_export_guids_offset_off);
        if old_guids_off > 0 {
            write_i32_le(&mut out_prefix, pfx.import_export_guids_offset_off, (old_guids_off as isize + header_delta + ta_delta) as i32);
        }
        let old_thumb_off = read_i32_le(&out_prefix, pfx.thumbnail_table_offset_off);
        if old_thumb_off > 0 {
            write_i32_le(&mut out_prefix, pfx.thumbnail_table_offset_off, (old_thumb_off as isize + header_delta + ta_delta) as i32);
        }
        if pfx.generations_count > 0 {
            let last_gen = pfx.generations_entries_off + (pfx.generations_count - 1) * 12;
            if last_gen + 8 <= out_prefix.len() {
                write_i32_le(&mut out_prefix, last_gen, new_export_count as i32);
                write_i32_le(&mut out_prefix, last_gen + 4, new_name_count as i32);
            }
        }
        let new_comp_meta = (pfx.compression_meta_off as isize + ta_delta) as usize;
        if new_comp_meta + 8 <= out_prefix.len() {
            write_i32_le(&mut out_prefix, new_comp_meta, (pfx.garbage_size as isize + extra_file_shift - header_delta - ta_delta) as i32);
            write_i32_le(&mut out_prefix, new_comp_meta + 4, c_off as i32);
        }
    }

    let new_gap_target_len = (old_gap_end as isize - old_gap_start as isize + extra_file_shift - size_growth - ta_delta).max(0) as usize;
    let total_out_len = out_prefix.len() + new_enc_aligned + new_gap_target_len + payloads_and_bounds.iter().map(|(p, _, _, _)| p.len()).sum::<usize>();
    let mut out_buf = Vec::with_capacity(total_out_len);
    out_buf.extend_from_slice(&out_prefix);
    out_buf.extend_from_slice(&new_enc_hdr[..new_enc_aligned]);
    let existing_gap = &orig_raw[old_gap_start..old_gap_end.min(orig_raw.len())];
    if new_gap_target_len <= existing_gap.len() {
        out_buf.extend_from_slice(&existing_gap[..new_gap_target_len]);
    } else {
        out_buf.extend_from_slice(existing_gap);
        out_buf.resize(out_buf.len() + (new_gap_target_len - existing_gap.len()), 0);
    }
    for (p, _, _, _) in payloads_and_bounds {
        out_buf.extend_from_slice(&p);
    }

    Ok(out_buf)
}

/// Replaces exports and/or appends new exports on top of `base_mod_bytes`.
pub fn rebuild_upk_with_mod_resized_and_added_exports_to_vec(
    orig_raw: &[u8],
    orig_unpacked: &UnpackedUpk,
    base_mod_bytes: &[u8],
    replacements: &[(usize, Vec<u8>)],
    added_exports: &[(String, usize, Vec<u8>)],
    output_key: &[u8; 32],
) -> Result<Vec<u8>, String> {
    rebuild_upk_with_mod_resized_added_and_names_to_vec(
        orig_raw,
        orig_unpacked,
        base_mod_bytes,
        replacements,
        added_exports,
        &[],
        output_key,
    )
}

/// Replaces zero or more exports with resized serial blobs on top of `base_mod_bytes`.
pub fn rebuild_upk_with_mod_and_resized_exports_to_vec(
    orig_raw: &[u8],
    orig_unpacked: &UnpackedUpk,
    base_mod_bytes: &[u8],
    replacements: &[(usize, Vec<u8>)],
    output_key: &[u8; 32],
) -> Result<Vec<u8>, String> {
    rebuild_upk_with_mod_resized_and_added_exports_to_vec(
        orig_raw,
        orig_unpacked,
        base_mod_bytes,
        replacements,
        &[],
        output_key,
    )
}

/// Replaces zero or more exports with resized serial blobs on top of `base_mod_bytes` and writes the UPK.
pub fn rebuild_upk_with_mod_and_resized_exports(
    orig_raw: &[u8],
    orig_unpacked: &UnpackedUpk,
    base_mod_bytes: &[u8],
    replacements: &[(usize, Vec<u8>)],
    output_key: &[u8; 32],
    output_path: &Path,
) -> Result<(), String> {
    let out_buf = rebuild_upk_with_mod_and_resized_exports_to_vec(
        orig_raw,
        orig_unpacked,
        base_mod_bytes,
        replacements,
        output_key,
    )?;
    if let Some(parent) = output_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(output_path, &out_buf).map_err(|e| e.to_string())
}

pub fn rebuild_upk_resized_export(
    orig_raw: &[u8],
    orig_unpacked: &UnpackedUpk,
    target_export_idx: usize,
    new_serial: &[u8],
    output_key: &[u8; 32],
    output_path: &Path,
) -> Result<(), String> {
    rebuild_upk_with_mod_and_resized_exports(
        orig_raw,
        orig_unpacked,
        &orig_unpacked.file_bytes,
        &[(target_export_idx, new_serial.to_vec())],
        output_key,
        output_path,
    )
}

// ── 3-Tier Pristine Backup Vault & Self-Healing Avatar Cutout Engine ─────────

pub fn get_appdata_dir() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".to_string());
    let p = PathBuf::from(base).join("RLSwapper");
    let _ = fs::create_dir_all(&p);
    p
}

pub fn get_pristine_vault_dir() -> PathBuf {
    let p = get_appdata_dir().join("pristine_backups");
    let _ = fs::create_dir_all(&p);
    p
}

pub fn vault_backup_path(upk_path: &Path) -> PathBuf {
    let name = upk_path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let clean = name.strip_suffix(".bak").unwrap_or(&name);
    get_pristine_vault_dir().join(clean)
}

pub fn bak_path_for(upk_path: &Path) -> PathBuf {
    let s = upk_path.to_string_lossy();
    if s.to_lowercase().ends_with(".bak") {
        upk_path.to_path_buf()
    } else {
        PathBuf::from(format!("{}.bak", s))
    }
}

/// Returns true if an `AvatarBorder_*_SF.upk` has opaque custom PFP pixels in its center `(60, 60)`.
pub fn is_avatar_border_modified(upk_path: &Path) -> bool {
    let fname = upk_path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    if !(fname.starts_with("avatarborder_") || fname == "paca6f52bcd3aad77dd19_sf.upk")
        || fname.ends_with("_t_sf.upk")
        || !upk_path.exists()
    {
        return false;
    }
    if let Ok(chunks) = decompress_all_upk_chunks(upk_path) {
        let marker = [0x00, 0xE1, 0x00, 0x00, 0x00, 0xE1, 0x00, 0x00]; // (57600, 57600)
        for ch in &chunks {
            let mut search_from = 0usize;
            while search_from + 8 <= ch.len() {
                if let Some(rel) = ch[search_from..].windows(8).position(|w| w == marker) {
                    let pos = search_from + rel;
                    let center_off = pos + 8 + (60 * 120 + 60) * 4;
                    if center_off + 4 <= ch.len() && ch[center_off + 3] > 16 {
                        return true;
                    }
                    search_from = pos + 8;
                } else {
                    break;
                }
            }
        }
    }
    false
}

const SHADOW_ALPHAS: [u8; 9] = [150, 84, 68, 55, 40, 27, 17, 9, 3];

pub fn sanitize_avatar_border_cutout(img: &mut RgbaImage) {
    if img.width() != 120 || img.height() != 120 {
        return;
    }
    for y in 21..=98 {
        for x in 21..=98 {
            let d = (x - 21).min(98 - x).min(y - 21).min(98 - y) as usize;
            let px = if d == 0 {
                Rgba([40, 40, 40, 150])
            } else if d < SHADOW_ALPHAS.len() {
                Rgba([0, 0, 0, SHADOW_ALPHAS[d]])
            } else {
                Rgba([0, 0, 0, 0])
            };
            img.put_pixel(x, y, px);
        }
    }
}

/// Surgically repairs an `AvatarBorder_*_SF.upk` in-place by restoring its transparent 78x78 cutout.
pub fn repair_avatar_border_upk(upk_path: &Path) -> bool {
    let Ok((unpacked, orig_raw)) = unpack_upk_from_disk(upk_path) else {
        return false;
    };
    let mut mod_bytes = unpacked.file_bytes.clone();
    let marker = [0x00, 0xE1, 0x00, 0x00, 0x00, 0xE1, 0x00, 0x00]; // (57600, 57600)
    let mut patched_any = false;
    let mut search_from = 0usize;
    while search_from + 8 <= mod_bytes.len() {
        let Some(rel) = mod_bytes[search_from..].windows(8).position(|w| w == marker) else {
            break;
        };
        let pos = search_from + rel;
        let pix_start = pos + 8;
        if pix_start + 57600 <= mod_bytes.len() {
            let mut img = RgbaImage::new(120, 120);
            for i in 0..(120 * 120) {
                let b = mod_bytes[pix_start + i * 4];
                let g = mod_bytes[pix_start + i * 4 + 1];
                let r = mod_bytes[pix_start + i * 4 + 2];
                let a = mod_bytes[pix_start + i * 4 + 3];
                img.put_pixel((i % 120) as u32, (i / 120) as u32, Rgba([r, g, b, a]));
            }
            sanitize_avatar_border_cutout(&mut img);
            for i in 0..(120 * 120) {
                let p = img.get_pixel((i % 120) as u32, (i / 120) as u32);
                mod_bytes[pix_start + i * 4] = p[2];
                mod_bytes[pix_start + i * 4 + 1] = p[1];
                mod_bytes[pix_start + i * 4 + 2] = p[0];
                mod_bytes[pix_start + i * 4 + 3] = p[3];
            }
            patched_any = true;
            search_from = pix_start + 57600;
        } else {
            break;
        }
    }
    if !patched_any {
        return false;
    }
    rebuild_upk_selective_chunks(&orig_raw, &unpacked, &mod_bytes, &unpacked.key, upk_path).is_ok()
}

/// Ensures that a pristine backup exists in both `<pkg>.bak` and `%LOCALAPPDATA%\RLSwapper\pristine_backups\<pkg>`,
/// and restores `<pkg>` to pristine before applying a new modification.
pub fn ensure_pristine_before_swap(target_path: &Path) -> Result<PathBuf, String> {
    let bak = bak_path_for(target_path);
    let vpath = vault_backup_path(target_path);

    if !vpath.exists() {
        let src = if bak.exists() { &bak } else { target_path };
        if src.exists() {
            if !is_avatar_border_modified(src) {
                let _ = fs::copy(src, &vpath);
            }
        }
    }

    if bak.exists() {
        let restore_src = if is_avatar_border_modified(&bak) && vpath.exists() {
            &vpath
        } else {
            &bak
        };
        fs::copy(restore_src, target_path).map_err(|e| e.to_string())?;
    } else if target_path.exists() {
        if is_avatar_border_modified(target_path) {
            if vpath.exists() && !is_avatar_border_modified(&vpath) {
                let _ = fs::copy(&vpath, target_path);
            } else {
                repair_avatar_border_upk(target_path);
            }
        }
        fs::copy(target_path, &bak).map_err(|e| e.to_string())?;
        if !vpath.exists() {
            let _ = fs::copy(target_path, &vpath);
        }
    }

    Ok(bak)
}

/// Restores a single modified package using the 3-Tier system (`.bak` -> Pristine Vault -> Self-Healing Cutout Repair).
pub fn restore_single_package(upk_path: &Path) -> bool {
    let bak = bak_path_for(upk_path);
    let vpath = vault_backup_path(upk_path);
    let mut restored = false;

    let use_src = if bak.exists() {
        if is_avatar_border_modified(&bak) && vpath.exists() {
            Some(&vpath)
        } else {
            Some(&bak)
        }
    } else if vpath.exists() {
        Some(&vpath)
    } else {
        None
    };

    if let Some(src) = use_src {
        if fs::copy(src, upk_path).is_ok() {
            restored = true;
        }
    }
    if bak.exists() {
        let _ = fs::remove_file(&bak);
    }

    if upk_path.exists() && is_avatar_border_modified(upk_path) {
        if repair_avatar_border_upk(upk_path) {
            restored = true;
        }
    }
    restored
}

// ── Catalog Items & Paint Color Specs ────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogItem {
    #[serde(alias = "ID", default)]
    pub id: i64,
    #[serde(alias = "Product", default)]
    pub product: String,
    #[serde(alias = "Slot", default)]
    pub slot: String,
    #[serde(alias = "AssetPackage", default)]
    pub asset_package: String,
    #[serde(alias = "AssetPath", default)]
    pub asset_path: String,
    #[serde(alias = "Quality", default)]
    pub quality: String,
}

#[derive(Deserialize)]
struct RawCatalogFile {
    #[serde(alias = "Items", alias = "items")]
    items: Vec<CatalogItem>,
}

pub fn load_catalog_items(cooked_dir: Option<&Path>) -> Vec<CatalogItem> {
    let mut list: Vec<CatalogItem> = serde_json::from_str::<RawCatalogFile>(EMBEDDED_ITEMS_JSON)
        .map(|f| f.items)
        .unwrap_or_default();

    for it in &mut list {
        if it.slot.eq_ignore_ascii_case("PlayerAvatarBorder") {
            it.slot = "Avatar Border".to_string();
        }
        if !it.asset_package.to_lowercase().ends_with(".upk") && !it.asset_package.is_empty() {
            it.asset_package = format!("{}.upk", it.asset_package);
        }
    }

    // Filter to packages present on disk if cooked_dir exists
    if let Some(dir) = cooked_dir {
        if dir.is_dir() {
            if let Ok(entries) = fs::read_dir(dir) {
                let disk_map: HashMap<String, String> = entries
                    .filter_map(|e| e.ok())
                    .map(|e| {
                        let nm = e.file_name().to_string_lossy().to_string();
                        (nm.to_lowercase(), nm)
                    })
                    .collect();

                for it in &mut list {
                    let low = it.asset_package.to_lowercase();
                    if let Some(real_nm) = disk_map.get(&low) {
                        it.asset_package = real_nm.clone();
                    }
                }
            }
        }
    }
    list
}

#[derive(Debug, Clone, PartialEq)]
pub struct PaintColorSpec {
    pub name: String,
    pub hex_srgb: String,
    pub linear_rgb: (f32, f32, f32),
    pub is_custom: bool,
}

impl PaintColorSpec {
    pub fn srgb_hex_to_linear(hex: &str) -> (String, (f32, f32, f32)) {
        let h = hex.trim().trim_start_matches('#');
        let (r8, g8, b8) = if h.len() == 6 {
            (
                u8::from_str_radix(&h[0..2], 16).unwrap_or(255),
                u8::from_str_radix(&h[2..4], 16).unwrap_or(255),
                u8::from_str_radix(&h[4..6], 16).unwrap_or(255),
            )
        } else {
            (255, 255, 255)
        };
        let to_lin = |c: u8| -> f32 {
            let s = (c as f32) / 255.0;
            if s <= 0.04045 {
                s / 12.92
            } else {
                ((s + 0.055) / 1.055).powf(2.4)
            }
        };
        (
            format!("#{:02X}{:02X}{:02X}", r8, g8, b8),
            (to_lin(r8), to_lin(g8), to_lin(b8)),
        )
    }

    pub fn from_custom_hex(hex: &str) -> Self {
        let (norm, lin) = Self::srgb_hex_to_linear(hex);
        Self {
            name: format!("Custom {}", norm),
            hex_srgb: norm,
            linear_rgb: lin,
            is_custom: true,
        }
    }

    pub fn rgb8(&self) -> [u8; 3] {
        let h = self.hex_srgb.trim_start_matches('#');
        if h.len() == 6 {
            [
                u8::from_str_radix(&h[0..2], 16).unwrap_or(255),
                u8::from_str_radix(&h[2..4], 16).unwrap_or(255),
                u8::from_str_radix(&h[4..6], 16).unwrap_or(255),
            ]
        } else {
            [255, 255, 255]
        }
    }
}

pub fn official_paints() -> Vec<PaintColorSpec> {
    vec![
        PaintColorSpec { name: "Titanium White".into(), hex_srgb: "#F5F5F7".into(), linear_rgb: (0.880, 0.880, 0.885), is_custom: false },
        PaintColorSpec { name: "Black".into(),          hex_srgb: "#1C1C1F".into(), linear_rgb: (0.012, 0.012, 0.013), is_custom: false },
        PaintColorSpec { name: "Gold".into(),           hex_srgb: "#E5B842".into(), linear_rgb: (0.760, 0.460, 0.055), is_custom: false },
        PaintColorSpec { name: "Crimson".into(),        hex_srgb: "#E02424".into(), linear_rgb: (0.680, 0.018, 0.018), is_custom: false },
        PaintColorSpec { name: "Sky Blue".into(),       hex_srgb: "#29B6F6".into(), linear_rgb: (0.035, 0.450, 0.880), is_custom: false },
        PaintColorSpec { name: "Cobalt".into(),         hex_srgb: "#2558E8".into(), linear_rgb: (0.025, 0.100, 0.720), is_custom: false },
        PaintColorSpec { name: "Purple".into(),         hex_srgb: "#8E24AA".into(), linear_rgb: (0.260, 0.022, 0.580), is_custom: false },
        PaintColorSpec { name: "Pink".into(),           hex_srgb: "#F472B6".into(), linear_rgb: (0.850, 0.160, 0.380), is_custom: false },
        PaintColorSpec { name: "Forest Green".into(),   hex_srgb: "#1B8A3D".into(), linear_rgb: (0.018, 0.300, 0.035), is_custom: false },
        PaintColorSpec { name: "Lime".into(),           hex_srgb: "#84E119".into(), linear_rgb: (0.240, 0.820, 0.015), is_custom: false },
        PaintColorSpec { name: "Saffron".into(),        hex_srgb: "#FACC15".into(), linear_rgb: (0.880, 0.620, 0.015), is_custom: false },
        PaintColorSpec { name: "Orange".into(),         hex_srgb: "#F97316".into(), linear_rgb: (0.880, 0.200, 0.008), is_custom: false },
        PaintColorSpec { name: "Burnt Sienna".into(),   hex_srgb: "#7C3A16".into(), linear_rgb: (0.180, 0.032, 0.006), is_custom: false },
        PaintColorSpec { name: "Grey".into(),           hex_srgb: "#7A7F87".into(), linear_rgb: (0.190, 0.195, 0.205), is_custom: false },
    ]
}

// ── UObject Serialized Property Scanner & Binary Color Injector ──────────────

#[derive(Debug, Clone)]
pub struct ParsedProperty {
    pub name: String,
    pub tag_type: String,
    pub struct_name: String,
    pub size: usize,
    pub value_offset: usize, // relative to start of buffer passed in
}

pub fn parse_property_stream(upk: &UnpackedUpk, raw: &[u8], start_pos: usize) -> (usize, Vec<ParsedProperty>, bool) {
    let mut props = Vec::new();
    let mut pos = start_pos;
    for _ in 0..256 {
        if pos + 8 > raw.len() {
            return (pos, props, false);
        }
        let name_idx = read_i32_le(raw, pos);
        let name_num = read_i32_le(raw, pos + 4);
        let name = upk.resolve_name(name_idx);
        if name.is_empty() || name_num < 0 || name_num > 2048 {
            return (pos, props, false);
        }
        if name == "None" {
            return (pos + 8, props, true);
        }
        if pos + 24 > raw.len() {
            return (pos, props, false);
        }
        let type_idx = read_i32_le(raw, pos + 8);
        let type_num = read_i32_le(raw, pos + 12);
        let prop_size = read_i32_le(raw, pos + 16);
        let arr_idx = read_i32_le(raw, pos + 20);
        let tag_type = upk.resolve_name(type_idx);
        if type_num != 0 || prop_size < 0 || arr_idx < 0 || arr_idx > 1024 {
            return (pos, props, false);
        }
        let sz = prop_size as usize;
        let mut hdr_end = pos + 24;
        let mut struct_name = String::new();

        match tag_type {
            "StructProperty" => {
                if hdr_end + 8 > raw.len() {
                    return (pos, props, false);
                }
                let sn_idx = read_i32_le(raw, hdr_end);
                struct_name = upk.resolve_name(sn_idx).to_string();
                hdr_end += 8;
            }
            "BoolProperty" => {
                if hdr_end + 1 > raw.len() {
                    return (pos, props, false);
                }
                props.push(ParsedProperty {
                    name: name.to_string(),
                    tag_type: tag_type.to_string(),
                    struct_name,
                    size: 1,
                    value_offset: hdr_end,
                });
                pos = hdr_end + 1;
                continue;
            }
            "ByteProperty" => {
                if hdr_end + 8 > raw.len() {
                    return (pos, props, false);
                }
                hdr_end += 8;
            }
            "IntProperty" | "FloatProperty" | "NameProperty" | "ObjectProperty" | "ComponentProperty"
            | "InterfaceProperty" | "ArrayProperty" | "StrProperty" | "DelegateProperty" => {}
            _ => return (pos, props, false),
        }

        if hdr_end + sz > raw.len() {
            return (pos, props, false);
        }
        props.push(ParsedProperty {
            name: name.to_string(),
            tag_type: tag_type.to_string(),
            struct_name,
            size: sz,
            value_offset: hdr_end,
        });
        pos = hdr_end + sz;
    }
    (pos, props, false)
}

pub fn find_export_properties(upk: &UnpackedUpk, exp: &ExportEntry, buf: &[u8]) -> Vec<ParsedProperty> {
    if exp.serial_size < 8 || exp.serial_offset + exp.serial_size > buf.len() {
        return Vec::new();
    }
    let slice = &buf[exp.serial_offset..exp.serial_offset + exp.serial_size];
    let limit = slice.len().min(64);
    for start in (0..limit).step_by(4) {
        let (_, props, ended) = parse_property_stream(upk, slice, start);
        if ended && !props.is_empty() {
            return props;
        }
    }
    // Fallback for Actors/WorldInfo (whose serialized NetIndex + Object header lands at e.g. byte 26)
    for start in 0..limit {
        if start % 4 == 0 {
            continue;
        }
        let (_, props, ended) = parse_property_stream(upk, slice, start);
        if ended && !props.is_empty() {
            return props;
        }
    }
    Vec::new()
}

/// Returns `true` when a UE3 shader/material parameter name represents spatial coordinates, UV tiling,
/// offsets, speeds, scalar brightness multipliers, or scaling — NEVER an RGB color channel.
pub fn is_spatial_vector_param_name(name: &str) -> bool {
    let low = name.to_ascii_lowercase();
    [
        "uv", "tiling", "position", "offset", "scale", "rotation", "speed", "direction",
        "parallax", "bounds", "size", "coord", "width", "height", "radius", "velocity",
        "accel", "gravity", "pivot", "normal", "panning", "distort", "brightness",
        "intensity", "opacity", "alpha",
    ]
    .iter()
    .any(|kw| low.contains(kw))
}

/// Writes 3 linear RGB `f32` channels at `abs`, scaling by any pre-existing HDR multiplier `(1.0..=6.0)`
/// when the target color is non-black so bright neon glows retain their intensity.
pub fn write_rgb_f32_preserving_hdr(buf: &mut [u8], abs: usize, r: f32, g: f32, b: f32) {
    if abs + 12 > buf.len() {
        return;
    }
    let cr = read_f32_le(buf, abs).abs();
    let cg = read_f32_le(buf, abs + 4).abs();
    let cb = read_f32_le(buf, abs + 8).abs();
    let orig_max = if cr.is_finite() && cg.is_finite() && cb.is_finite() {
        cr.max(cg).max(cb)
    } else {
        1.0
    };
    let target_max = r.max(g).max(b);
    let hdr = if orig_max > 1.15 && target_max > 0.05 {
        orig_max.min(6.0)
    } else {
        1.0
    };
    write_f32_le(buf, abs, r * hdr);
    write_f32_le(buf, abs + 4, g * hdr);
    write_f32_le(buf, abs + 8, b * hdr);
}

fn patch_raw_distribution_lookup(
    upk: &UnpackedUpk,
    buf: &mut [u8],
    prop_abs: usize,
    prop_size: usize,
    r: f32,
    g: f32,
    b: f32,
) -> usize {
    if prop_abs + prop_size > buf.len() || prop_size < 16 {
        return 0;
    }
    let sub = &buf[prop_abs..prop_abs + prop_size];
    let (_, sub_props, _) = parse_property_stream(upk, sub, 0);
    let mut hits = 0usize;
    for sp in sub_props {
        if sp.name == "LookupTable" && sp.tag_type == "ArrayProperty" && sp.size == 36 {
            let arr_abs = prop_abs + sp.value_offset;
            if arr_abs + 36 <= buf.len() && read_i32_le(buf, arr_abs) == 8 {
                let f_off = arr_abs + 4;
                // UE3 constant LookupTable stores 8 floats: [min, max, r0, g0, b0, r1, g1, b1]
                write_rgb_f32_preserving_hdr(buf, f_off + 8, r, g, b);
                write_rgb_f32_preserving_hdr(buf, f_off + 20, r, g, b);
                let nr = read_f32_le(buf, f_off + 8);
                let ng = read_f32_le(buf, f_off + 12);
                let nb = read_f32_le(buf, f_off + 16);
                write_f32_le(buf, f_off, nr.min(ng).min(nb));
                write_f32_le(buf, f_off + 4, nr.max(ng).max(nb));
                hits += 1;
            }
        }
    }
    hits
}

/// Injects `PaintColorSpec` linear RGB values into a decrypted `UnpackedUpk`'s byte buffer
/// across shaders, `MaterialInstanceConstant`, `FXActor` boosts, and `ParticleModuleColor` distributions.
fn is_car_trim_param_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("trimcolor") || n.contains("paintcolor") || n == "tintcolor" || n == "paint"
}

fn patch_skin_params_trim_in_place(upk: &UnpackedUpk, buf: &mut [u8], exp: &ExportEntry, props: &[ParsedProperty], r: f32, g: f32, b: f32) -> usize {
    let mut edits = 0usize;
    for p in props {
        if p.name == "SkinParameters" && p.tag_type == "StructProperty" && p.size >= 8 {
            let sp_abs = exp.serial_offset + p.value_offset;
            let (_, sp_subs, _) = parse_property_stream(upk, &buf[sp_abs..sp_abs + p.size], 0);
            for sub in sp_subs {
                if sub.name == "VectorParameters" && sub.tag_type == "ArrayProperty" && sub.size >= 4 {
                    let vp_abs = sp_abs + sub.value_offset;
                    let cnt = read_i32_le(buf, vp_abs).clamp(0, 32) as usize;
                    let mut rel = 4usize;
                    for _ in 0..cnt {
                        let slice = &buf[vp_abs..vp_abs + sub.size];
                        let (nr, ep, ok) = parse_property_stream(upk, slice, rel);
                        if !ok && ep.is_empty() {
                            break;
                        }
                        let nm = ep
                            .iter()
                            .find(|x| x.name == "Name" && x.size >= 8)
                            .map(|n| upk.resolve_name(read_i32_le(buf, vp_abs + n.value_offset)))
                            .unwrap_or("");
                        if is_car_trim_param_name(nm) {
                            if let Some(vp) = ep.iter().find(|x| x.name == "Value" && x.size == 16) {
                                write_rgb_f32_preserving_hdr(buf, vp_abs + vp.value_offset, r, g, b);
                                edits += 1;
                            }
                        }
                        rel = nr;
                    }
                }
            }
        }
    }
    edits
}

/// Injects custom Paint Color (`Color` / `CustomColor` / `Tint` / `TrimColor`) into shader parameters inside UPK.
pub fn inject_paint_color_into_upk(upk: &UnpackedUpk, buf: &mut [u8], color: &PaintColorSpec) -> usize {
    let (r, g, b) = color.linear_rgb;
    let is_body_pkg = upk.exports.iter().any(|e| upk.export_class_name(e) == "ProductAsset_Body_TA");
    let mut edits = 0usize;
    let mut scale_over_life_fallback: Vec<(usize, usize)> = Vec::new();

    for exp in &upk.exports {
        if exp.serial_size < 16 || exp.serial_offset + exp.serial_size > buf.len() {
            continue;
        }
        let cls = upk.export_class_name(exp);
        let full_low = upk.full_export_path(exp.index).to_ascii_lowercase();

        // Neutralize ProductAttribute_PaintSettings_TA so sacrifice item paint doesn't override
        // without falsely counting neutralization as an actual color edit.
        if cls == "ProductAttribute_PaintSettings_TA" {
            let props = find_export_properties(upk, exp, buf);
            if let Some(none_idx) = upk.find_name_index("None") {
                for p in props {
                    let abs = exp.serial_offset + p.value_offset;
                    if p.tag_type == "NameProperty" && p.size == 8 && abs + 8 <= buf.len() {
                        write_i32_le(buf, abs, none_idx);
                        write_i32_le(buf, abs + 4, 0);
                    }
                }
            }
            continue;
        }

        let props = find_export_properties(upk, exp, buf);

        // Activate Painted* archetype / FX / Mesh when ProductAsset_* has a separate painted variant
        // (e.g. Paper Dragon: GoalExplosionFX -> PaintedGoalExplosionFX) and patch existing TrimColor in SkinParameters.
        if cls.starts_with("ProductAsset_") {
            for p in &props {
                if p.tag_type == "ObjectProperty" && p.size == 4 && !p.name.starts_with("Painted") {
                    let target_nm = format!("Painted{}", p.name);
                    if let Some(pp) = props.iter().find(|x| {
                        x.tag_type == "ObjectProperty"
                            && x.size == 4
                            && x.name.eq_ignore_ascii_case(&target_nm)
                    }) {
                        let painted_ref = read_i32_le(buf, exp.serial_offset + pp.value_offset);
                        if painted_ref > 0 {
                            write_i32_le(buf, exp.serial_offset + p.value_offset, painted_ref);
                            edits += 1;
                        }
                    }
                }
            }
            if cls == "ProductAsset_Body_TA" {
                edits += patch_skin_params_trim_in_place(upk, buf, exp, &props, r, g, b);
            }
            continue;
        }

        if cls.starts_with("ProductOverride_") {
            for p in &props {
                if p.name == "ParameterOverrides" && p.tag_type == "ArrayProperty" && p.size >= 4 {
                    let arr_abs = exp.serial_offset + p.value_offset;
                    let cnt = read_i32_le(buf, arr_abs).clamp(0, 64) as usize;
                    let mut rel = 4usize;
                    for _ in 0..cnt {
                        let slice = &buf[arr_abs..arr_abs + p.size];
                        let (next_rel, elem_props, ended) = parse_property_stream(upk, slice, rel);
                        if !ended && elem_props.is_empty() {
                            break;
                        }
                        let key_nm = elem_props
                            .iter()
                            .find(|ep| matches!(ep.name.as_str(), "PaintParameterName" | "Key" | "ParameterName") && ep.size >= 8)
                            .map(|kn| upk.resolve_name(read_i32_le(buf, arr_abs + kn.value_offset)))
                            .unwrap_or("");
                        if !is_spatial_vector_param_name(key_nm) {
                            for ep in &elem_props {
                                if (ep.struct_name == "LinearColor" && ep.size == 16)
                                    || (ep.struct_name == "Vector" && ep.size == 12 && matches!(ep.name.as_str(), "CustomColor" | "Value"))
                                {
                                    write_rgb_f32_preserving_hdr(buf, arr_abs + ep.value_offset, r, g, b);
                                    edits += 1;
                                }
                            }
                        }
                        rel = next_rel;
                    }
                }
            }
        }

        if cls.starts_with("DistributionVector") {
            let param_nm = props
                .iter()
                .find(|p| p.name == "ParameterName" && p.size >= 8)
                .map(|p| upk.resolve_name(read_i32_le(buf, exp.serial_offset + p.value_offset)))
                .unwrap_or("");
            let is_color_dist = full_low.contains("particlemodulecolor")
                || param_nm.to_ascii_lowercase().contains("color")
                || param_nm.to_ascii_lowercase().contains("team")
                || param_nm.to_ascii_lowercase().contains("tint");
            if is_color_dist && !is_spatial_vector_param_name(param_nm) {
                for p in &props {
                    if p.struct_name == "Vector"
                        && p.size == 12
                        && matches!(p.name.as_str(), "Constant" | "MaxOutput" | "Min" | "Max")
                    {
                        write_rgb_f32_preserving_hdr(buf, exp.serial_offset + p.value_offset, r, g, b);
                        edits += 1;
                    }
                }
            }
            continue;
        }

        if cls == "ParticleModuleColorScaleOverLife" {
            for p in &props {
                if p.name == "ColorScaleOverLife" && p.tag_type == "StructProperty" {
                    scale_over_life_fallback.push((exp.serial_offset + p.value_offset, p.size));
                }
            }
            continue;
        }

        if cls.starts_with("ParticleModuleColor") {
            for p in &props {
                if matches!(p.name.as_str(), "StartColor" | "ColorOverLife") && p.tag_type == "StructProperty" {
                    edits += patch_raw_distribution_lookup(
                        upk,
                        buf,
                        exp.serial_offset + p.value_offset,
                        p.size,
                        r,
                        g,
                        b,
                    );
                }
            }
            continue;
        }

        // Skip MaterialExpressionVectorParameter / Constant3Vector with spatial parameter names
        let expr_param_nm = if cls.starts_with("MaterialExpression") {
            props
                .iter()
                .find(|p| p.name == "ParameterName" && p.size >= 8)
                .map(|p| upk.resolve_name(read_i32_le(buf, exp.serial_offset + p.value_offset)))
                .unwrap_or("")
        } else {
            ""
        };
        if !expr_param_nm.is_empty() && is_spatial_vector_param_name(expr_param_nm) {
            continue;
        }
        if is_body_pkg && cls.starts_with("MaterialExpression") && !is_car_trim_param_name(expr_param_nm) {
            continue;
        }

        for p in props {
            if is_spatial_vector_param_name(&p.name) {
                continue;
            }
            if !is_body_pkg && ((p.struct_name == "LinearColor" && p.size == 16) || (p.name == "CarBoostGlowColor" && p.size == 16)) {
                write_rgb_f32_preserving_hdr(buf, exp.serial_offset + p.value_offset, r, g, b);
                edits += 1;
            } else if p.name == "VectorParameterValues" && p.tag_type == "ArrayProperty" && p.size >= 4 {
                let arr_abs = exp.serial_offset + p.value_offset;
                let cnt = read_i32_le(buf, arr_abs).clamp(0, 64) as usize;
                let mut rel = 4usize;
                for _ in 0..cnt {
                    let slice = &buf[arr_abs..arr_abs + p.size];
                    let (next_rel, elem_props, ended) = parse_property_stream(upk, slice, rel);
                    if !ended && elem_props.is_empty() {
                        break;
                    }
                    let pname = elem_props
                        .iter()
                        .find(|ep| ep.name == "ParameterName" && ep.size >= 8)
                        .map(|pn| upk.resolve_name(read_i32_le(buf, arr_abs + pn.value_offset)))
                        .unwrap_or("");
                    let allow = if is_body_pkg {
                        is_car_trim_param_name(pname)
                    } else {
                        !is_spatial_vector_param_name(pname)
                    };
                    if allow {
                        if let Some(pv) = elem_props.iter().find(|ep| ep.name == "ParameterValue" && ep.size == 16) {
                            write_rgb_f32_preserving_hdr(buf, arr_abs + pv.value_offset, r, g, b);
                            edits += 1;
                        }
                    }
                    rel = next_rel;
                }
            }
        }
    }

    // Apply ParticleModuleColorScaleOverLife only when no primary particle/shader color existed
    if edits == 0 && !is_body_pkg {
        for (abs, sz) in scale_over_life_fallback {
            edits += patch_raw_distribution_lookup(upk, buf, abs, sz, r, g, b);
        }
    }
    edits
}

/// Inserts or updates `"TrimColor"` directly inside Car Body `ProductAsset_Body_TA.SkinParameters.VectorParameters`
/// (which `CarMeshComponentBase_TA.SetMaterialParameters` binds to both base body and equipped decals) as well as
/// separate Trim `MaterialInstanceConstant`s (`MIC_Vehicle_Trim_*` such as Dominus).
pub fn build_body_trim_resized_replacements(
    upk: &UnpackedUpk,
    mod_bytes: &[u8],
    color: &PaintColorSpec,
) -> (Vec<(usize, Vec<u8>)>, Vec<String>, usize) {
    if !upk.exports.iter().any(|e| upk.export_class_name(e) == "ProductAsset_Body_TA") {
        return (Vec::new(), Vec::new(), 0);
    }

    let (r, g, b) = color.linear_rgb;
    let mut added_names: Vec<String> = Vec::new();
    let mut ensure_idx = |nm: &str| -> i32 {
        if let Some(id) = upk.find_name_index(nm) {
            id
        } else if let Some(pos) = added_names.iter().position(|n| n.eq_ignore_ascii_case(nm)) {
            (upk.names.len() + pos) as i32
        } else {
            added_names.push(nm.to_string());
            (upk.names.len() + added_names.len() - 1) as i32
        }
    };

    let push_fn = |out: &mut Vec<u8>, idx: i32| {
        out.extend_from_slice(&idx.to_le_bytes());
        out.extend_from_slice(&0i32.to_le_bytes());
    };
    let push_tag = |out: &mut Vec<u8>, prop_nm: i32, type_nm: i32, sz: i32| {
        push_fn(out, prop_nm);
        push_fn(out, type_nm);
        out.extend_from_slice(&sz.to_le_bytes());
        out.extend_from_slice(&0i32.to_le_bytes());
    };
    let push_linear = |out: &mut Vec<u8>, lin_r: f32, lin_g: f32, lin_b: f32| {
        out.extend_from_slice(&lin_r.to_le_bytes());
        out.extend_from_slice(&lin_g.to_le_bytes());
        out.extend_from_slice(&lin_b.to_le_bytes());
        out.extend_from_slice(&1.0f32.to_le_bytes());
    };

    let trim_i = ensure_idx("TrimColor");
    let none_i = ensure_idx("None");
    let name_p = ensure_idx("Name");
    let name_t = ensure_idx("NameProperty");
    let val_p = ensure_idx("Value");
    let struct_t = ensure_idx("StructProperty");
    let arr_t = ensure_idx("ArrayProperty");
    let lin_s = ensure_idx("LinearColor");
    let pname_p = ensure_idx("ParameterName");
    let pval_p = ensure_idx("ParameterValue");
    let guid_p = ensure_idx("ExpressionGUID");
    let guid_s = ensure_idx("Guid");
    let vpv_p = ensure_idx("VectorParameterValues");

    let build_skin_vec_elem = || -> Vec<u8> {
        let mut v = Vec::with_capacity(88);
        push_tag(&mut v, name_p, name_t, 8);
        push_fn(&mut v, trim_i);
        push_tag(&mut v, val_p, struct_t, 16);
        push_fn(&mut v, lin_s);
        push_linear(&mut v, r, g, b);
        push_fn(&mut v, none_i);
        v
    };
    let build_mic_vpv_elem = || -> Vec<u8> {
        let mut v = Vec::with_capacity(136);
        push_tag(&mut v, pname_p, name_t, 8);
        push_fn(&mut v, trim_i);
        push_tag(&mut v, pval_p, struct_t, 16);
        push_fn(&mut v, lin_s);
        push_linear(&mut v, r, g, b);
        push_tag(&mut v, guid_p, struct_t, 16);
        push_fn(&mut v, guid_s);
        v.extend_from_slice(&[0u8; 16]);
        push_fn(&mut v, none_i);
        v
    };

    let mut reps = Vec::new();
    let mut edits = 0usize;

    for (exp_idx, exp) in upk.exports.iter().enumerate() {
        if exp.serial_size < 16 || exp.serial_offset + exp.serial_size > mod_bytes.len() {
            continue;
        }
        let cls = upk.export_class_name(exp);
        let full_low = upk.full_export_path(exp.index).to_ascii_lowercase();
        let orig_ser = &mod_bytes[exp.serial_offset..exp.serial_offset + exp.serial_size];

        if cls == "ProductAsset_Body_TA" {
            let props = find_export_properties(upk, exp, mod_bytes);
            if let Some(sp) = props.iter().find(|p| p.name == "SkinParameters" && p.tag_type == "StructProperty" && p.size >= 8) {
                let sp_abs = sp.value_offset;
                if sp_abs >= 16 && read_i32_le(orig_ser, sp_abs - 16) == sp.size as i32 {
                    let (_, sp_subs, _) = parse_property_stream(upk, &orig_ser[sp_abs..sp_abs + sp.size], 0);
                    if let Some(vp) = sp_subs.iter().find(|s| s.name == "VectorParameters" && s.tag_type == "ArrayProperty" && s.size >= 4) {
                        let vp_rel = sp_abs + vp.value_offset;
                        if vp_rel >= 8 && read_i32_le(orig_ser, vp_rel - 8) == vp.size as i32 {
                            let cnt = read_i32_le(orig_ser, vp_rel).clamp(0, 32) as usize;
                            let mut rel = 4usize;
                            let mut has_trim = false;
                            for _ in 0..cnt {
                                let (nr, ep, ok) = parse_property_stream(upk, &orig_ser[vp_rel..vp_rel + vp.size], rel);
                                if !ok && ep.is_empty() {
                                    break;
                                }
                                if ep.iter().any(|x| x.name == "Name" && x.size >= 8 && is_car_trim_param_name(upk.resolve_name(read_i32_le(orig_ser, vp_rel + x.value_offset)))) {
                                    has_trim = true;
                                }
                                rel = nr;
                            }
                            if !has_trim {
                                let elem = build_skin_vec_elem();
                                let mut new_ser = orig_ser.to_vec();
                                write_i32_le(&mut new_ser, vp_rel, (cnt + 1) as i32);
                                write_i32_le(&mut new_ser, vp_rel - 8, (vp.size + elem.len()) as i32);
                                write_i32_le(&mut new_ser, sp_abs - 16, (sp.size + elem.len()) as i32);
                                new_ser.splice((vp_rel + 4)..(vp_rel + 4), elem);
                                reps.push((exp_idx, new_ser));
                                edits += 1;
                            }
                        }
                    }
                }
            }
        } else if cls == "MaterialInstanceConstant" && full_low.contains("trim") {
            let mut props_end = 0usize;
            let mut props = Vec::new();
            let limit = orig_ser.len().min(32);
            for st in (0..limit).step_by(4).chain((0..limit).filter(|x| x % 4 != 0)) {
                let (end_off, parsed, ok) = parse_property_stream(upk, orig_ser, st);
                if ok && !parsed.is_empty() {
                    props_end = end_off;
                    props = parsed;
                    break;
                }
            }
            if let Some(vpv) = props.iter().find(|p| p.name == "VectorParameterValues" && p.tag_type == "ArrayProperty" && p.size >= 4) {
                let vrel = vpv.value_offset;
                if vrel >= 8 && read_i32_le(orig_ser, vrel - 8) == vpv.size as i32 {
                    let cnt = read_i32_le(orig_ser, vrel).clamp(0, 64) as usize;
                    let mut rel = 4usize;
                    let mut has_trim = false;
                    for _ in 0..cnt {
                        let (nr, ep, ok) = parse_property_stream(upk, &orig_ser[vrel..vrel + vpv.size], rel);
                        if !ok && ep.is_empty() {
                            break;
                        }
                        if ep.iter().any(|x| x.name == "ParameterName" && x.size >= 8 && is_car_trim_param_name(upk.resolve_name(read_i32_le(orig_ser, vrel + x.value_offset)))) {
                            has_trim = true;
                        }
                        rel = nr;
                    }
                    if !has_trim {
                        let elem = build_mic_vpv_elem();
                        let mut new_ser = orig_ser.to_vec();
                        write_i32_le(&mut new_ser, vrel, (cnt + 1) as i32);
                        write_i32_le(&mut new_ser, vrel - 8, (vpv.size + elem.len()) as i32);
                        new_ser.splice((vrel + 4)..(vrel + 4), elem);
                        reps.push((exp_idx, new_ser));
                        edits += 1;
                    }
                }
            } else if props_end >= 8 && upk.resolve_name(read_i32_le(orig_ser, props_end - 8)).eq_ignore_ascii_case("None") {
                let elem = build_mic_vpv_elem();
                let mut arr_prop = Vec::with_capacity(28 + elem.len());
                push_tag(&mut arr_prop, vpv_p, arr_t, (4 + elem.len()) as i32);
                arr_prop.extend_from_slice(&1i32.to_le_bytes());
                arr_prop.extend_from_slice(&elem);
                let mut new_ser = orig_ser.to_vec();
                new_ser.splice((props_end - 8)..(props_end - 8), arr_prop);
                reps.push((exp_idx, new_ser));
                edits += 1;
            }
        }
    }

    if reps.is_empty() {
        return (Vec::new(), Vec::new(), 0);
    }
    (reps, added_names, edits)
}

// ── Universal Surgical ExportTable Rewiring & Item Package Swapper ─────────

fn ue3_split_fname(text: &str) -> (String, i32) {
    if let Some((base, suffix)) = text.rsplit_once('_') {
        if !base.is_empty()
            && suffix.chars().all(|c| c.is_ascii_digit())
            && (suffix == "0" || !suffix.starts_with('0'))
        {
            if let Ok(num) = suffix.parse::<i32>() {
                return (base.to_string(), num + 1);
            }
        }
    }
    (text.to_string(), 0)
}

fn serialize_name_entry(name: &str, flags: u64) -> Vec<u8> {
    let raw = name.as_bytes();
    let mut out = Vec::with_capacity(4 + raw.len() + 1 + 8);
    out.extend_from_slice(&((raw.len() as i32) + 1).to_le_bytes());
    out.extend_from_slice(raw);
    out.push(0);
    out.extend_from_slice(&flags.to_le_bytes());
    out
}

/// Reads the target UPK's own decrypted ExportTable so every segment's exact `(base_name, stored_instance)`
/// matches Rocket League's cooked asset path bit-for-bit.
pub fn extract_target_fnames_from_upk(
    target_pristine_path: &Path,
    target_key: &[u8; 32],
    target_asset_path: &str,
    target_pkg_stem: &str,
) -> (Vec<(String, i32)>, (String, i32)) {
    let fallback_chain: Vec<(String, i32)> = target_asset_path
        .split('.')
        .filter(|s| !s.is_empty())
        .map(ue3_split_fname)
        .collect();
    let fallback_sf = ue3_split_fname(target_pkg_stem);

    let Ok(raw) = fs::read(target_pristine_path) else {
        return (fallback_chain, fallback_sf);
    };
    let Ok(pfx) = parse_upk_prefix(&raw) else {
        return (fallback_chain, fallback_sf);
    };
    let enc_sz = pfx
        .total_header_size
        .saturating_sub(pfx.garbage_size)
        .saturating_sub(pfx.name_offset);
    let enc_al = (enc_sz + 15) & !15;
    if enc_sz == 0 || pfx.name_offset + enc_al > raw.len() {
        return (fallback_chain, fallback_sf);
    }
    let eb = &raw[pfx.name_offset..pfx.name_offset + enc_al];
    let plain = if let Some(nonce) = pfx.ctr_nonce {
        aes_ctr_crypt(target_key, &nonce, eb)
    } else {
        aes_ecb_decrypt(target_key, eb)
    };
    let hdr = &plain[..enc_sz.min(plain.len())];
    if pfx.compressed_chunks_offset + 8 > hdr.len() {
        return (fallback_chain, fallback_sf);
    }
    let num_c = read_i32_le(hdr, pfx.compressed_chunks_offset);
    let u0 = read_i32_le(hdr, pfx.compressed_chunks_offset + 4) as usize;
    if num_c < 1 || u0 != pfx.depends_offset {
        return (fallback_chain, fallback_sf);
    }

    // Walk NameTable
    let mut names_list = Vec::with_capacity(pfx.name_count);
    let mut npos = 0usize;
    for _ in 0..pfx.name_count {
        let Some((nm, next_p)) = read_fstring_at(hdr, npos) else {
            return (fallback_chain, fallback_sf);
        };
        if next_p + 8 > hdr.len() {
            return (fallback_chain, fallback_sf);
        }
        names_list.push(nm);
        npos = next_p + 8;
    }

    // Walk ImportTable
    let ci = pfx.import_offset.saturating_sub(pfx.name_offset);
    let ce = pfx.export_offset.saturating_sub(pfx.name_offset);
    let cd = pfx.depends_offset.saturating_sub(pfx.name_offset);
    if ci > ce || ce > cd || cd > hdr.len() {
        return (fallback_chain, fallback_sf);
    }
    let imp_cnt = (ce - ci) / 28;
    let mut imp_names = Vec::with_capacity(imp_cnt);
    for i in 0..imp_cnt {
        let off = ci + i * 28;
        let on_i = read_i32_le(hdr, off + 20) as usize;
        imp_names.push(names_list.get(on_i).cloned().unwrap_or_default());
    }

    // Walk ExportTable
    struct LiteExp {
        cls_name: String,
        outer: i32,
        base: String,
        inst: i32,
        fmt: String,
    }
    let mut exps = Vec::with_capacity(pfx.export_count);
    let mut epos = ce;
    for _ in 0..pfx.export_count {
        if epos + 52 > cd {
            break;
        }
        let cls_i = read_i32_le(hdr, epos);
        let out_i = read_i32_le(hdr, epos + 8);
        let nm_i = read_i32_le(hdr, epos + 12) as usize;
        let inst_i = read_i32_le(hdr, epos + 16);
        let nc = read_i32_le(hdr, epos + 48).max(0) as usize;
        let cname = if cls_i < 0 {
            imp_names.get((-cls_i - 1) as usize).cloned().unwrap_or_default()
        } else {
            "Class".to_string()
        };
        let base = names_list.get(nm_i).cloned().unwrap_or_default();
        let fmt = if inst_i > 0 {
            format!("{}_{}", base, inst_i - 1)
        } else {
            base.clone()
        };
        exps.push(LiteExp { cls_name: cname, outer: out_i, base, inst: inst_i, fmt });
        epos += 52 + nc * 4 + 20;
    }

    let path_and_chain = |idx1: usize| -> (String, Vec<(String, i32)>) {
        let mut pts_s = Vec::new();
        let mut pts_f = Vec::new();
        let mut cur = idx1 as i32;
        for _ in 0..16 {
            if cur <= 0 || (cur as usize) > exps.len() {
                break;
            }
            let e = &exps[(cur - 1) as usize];
            pts_s.push(e.fmt.clone());
            pts_f.push((e.base.clone(), e.inst));
            cur = e.outer;
        }
        pts_s.reverse();
        pts_f.reverse();
        (pts_s.join("."), pts_f)
    };

    let t_low = target_asset_path.to_lowercase();
    let mut matched = None;
    for i in 0..exps.len() {
        let (fp, ch) = path_and_chain(i + 1);
        let fpl = fp.to_lowercase();
        if fpl == t_low && !fpl.starts_with("productslots.") {
            matched = Some(ch);
            break;
        }
    }
    if matched.is_none() {
        for i in 0..exps.len() {
            let (fp, ch) = path_and_chain(i + 1);
            if fp.to_lowercase() == t_low {
                matched = Some(ch);
                break;
            }
        }
    }
    if matched.is_none() {
        let cands: Vec<Vec<(String, i32)>> = exps
            .iter()
            .enumerate()
            .filter_map(|(i, e)| {
                let c = e.cls_name.to_lowercase();
                if c.starts_with("productasset_") && !c.starts_with("productassetreference") {
                    let (fp, ch) = path_and_chain(i + 1);
                    if !fp.to_lowercase().starts_with("productslots.") {
                        return Some(ch);
                    }
                }
                None
            })
            .collect();
        if cands.len() == 1 {
            matched = Some(cands.into_iter().next().unwrap());
        }
    }

    let mut sf_fname = fallback_sf;
    let stem_low = target_pkg_stem.to_lowercase();
    for e in &exps {
        if e.cls_name.eq_ignore_ascii_case("package") && e.outer == 0 && e.fmt.to_lowercase() == stem_low {
            sf_fname = (e.base.clone(), e.inst);
            break;
        }
    }

    (matched.unwrap_or(fallback_chain), sf_fname)
}

#[derive(Clone)]
struct RewireExp {
    idx1: i32,
    cls_i: i32,
    out_i: i32,
    nm_i: usize,
    inst_i: i32,
    obj_flags: u64,
    serial_sz: i32,
    serial_off: i64,
    exp_flags: i32,
    raw: Vec<u8>,
}

/// Surgically rewires donor's `ProductAsset_*_TA` export & `Core.Package` outer chain inside decrypted `header`
/// to match `target_chain` and `target_sf_fname`.
fn rewire_header_surgical(
    header: &[u8],
    import_off: usize,
    export_off: usize,
    depends_off: usize,
    name_count: usize,
    export_count: usize,
    compressed_chunks_offset: usize,
    donor_asset_path: &str,
    donor_pkg_stem: &str,
    target_chain: &[(String, i32)],
    target_sf_fname: &(String, i32),
    chunk_stride: usize,
) -> Result<(Vec<u8>, isize, usize, usize, usize, usize, usize), String> {
    if target_chain.is_empty() {
        return Err("Target AssetPath cannot be empty".into());
    }

    // Walk existing NameTable
    let mut names_list = Vec::with_capacity(name_count + 8);
    let mut name_flags_list = Vec::with_capacity(name_count + 8);
    let mut npos = 0usize;
    for _ in 0..name_count {
        let Some((nm, next_p)) = read_fstring_at(header, npos) else {
            return Err("NameTable truncated during surgical rewire".into());
        };
        if next_p + 8 > import_off {
            return Err("NameTable overran ImportTable".into());
        }
        names_list.push(nm);
        name_flags_list.push(read_i64_le(header, next_p) as u64);
        npos = next_p + 8;
    }
    let nt_end = npos;
    let nt_tail = &header[nt_end..import_off];
    let mut extra_nt = Vec::new();

    let imp_blob = &header[import_off..export_off];
    let imp_count = imp_blob.len() / 28;
    let mut imp_obj_names = Vec::with_capacity(imp_count);
    let mut pkg_import_cls_idx: Option<i32> = None;
    for i in 0..imp_count {
        let off = i * 28;
        let cn_n = read_i32_le(imp_blob, off + 8) as usize;
        let on_n = read_i32_le(imp_blob, off + 20) as usize;
        let on_i = read_i32_le(imp_blob, off + 24);
        let on_base = names_list.get(on_n).map(|s| s.as_str()).unwrap_or("");
        let cn_base = names_list.get(cn_n).map(|s| s.as_str()).unwrap_or("");
        let on_fmt = if on_i > 0 {
            format!("{}_{}", on_base, on_i - 1)
        } else {
            on_base.to_string()
        };
        imp_obj_names.push(on_fmt);
        if pkg_import_cls_idx.is_none()
            && on_base.eq_ignore_ascii_case("package")
            && cn_base.eq_ignore_ascii_case("class")
        {
            pkg_import_cls_idx = Some(-((i + 1) as i32));
        }
    }

    // Parse ExportTable
    let mut exps = Vec::with_capacity(export_count + 4);
    let mut epos = export_off;
    for idx in 0..export_count {
        if epos + 52 > depends_off {
            return Err("ExportTable truncated before DependsOffset".into());
        }
        let cls_i = read_i32_le(header, epos);
        let out_i = read_i32_le(header, epos + 8);
        let nm_i = read_i32_le(header, epos + 12) as usize;
        let inst_i = read_i32_le(header, epos + 16);
        let obj_flags = read_i64_le(header, epos + 24) as u64;
        let serial_sz = read_i32_le(header, epos + 32);
        let serial_off = read_i64_le(header, epos + 36);
        let exp_flags = read_i32_le(header, epos + 44);
        let net_cnt = read_i32_le(header, epos + 48).max(0) as usize;
        let entry_len = 52 + net_cnt * 4 + 20;
        if epos + entry_len > depends_off {
            return Err("ExportTable entry overran DependsOffset".into());
        }
        exps.push(RewireExp {
            idx1: (idx + 1) as i32,
            cls_i,
            out_i,
            nm_i,
            inst_i,
            obj_flags,
            serial_sz,
            serial_off,
            exp_flags,
            raw: header[epos..epos + entry_len].to_vec(),
        });
        epos += entry_len;
    }
    let exp_tail = &header[epos..depends_off];

    let fmt_exp_name = |names: &[String], nm_i: usize, inst_i: i32| -> String {
        let base = names.get(nm_i).map(|s| s.as_str()).unwrap_or("");
        if inst_i > 0 {
            format!("{}_{}", base, inst_i - 1)
        } else {
            base.to_string()
        }
    };
    let cls_name_of = |exps: &[RewireExp], names: &[String], cls_i: i32| -> String {
        if cls_i < 0 {
            imp_obj_names.get((-cls_i - 1) as usize).cloned().unwrap_or_default()
        } else if cls_i == 0 {
            "Class".to_string()
        } else if let Some(e) = exps.get((cls_i - 1) as usize) {
            fmt_exp_name(names, e.nm_i, e.inst_i)
        } else {
            String::new()
        }
    };
    let full_exp_path = |exps: &[RewireExp], names: &[String], idx1: i32| -> String {
        let mut pts = Vec::new();
        let mut cur = idx1;
        for _ in 0..16 {
            if cur <= 0 || (cur as usize) > exps.len() {
                break;
            }
            let e = &exps[(cur - 1) as usize];
            pts.push(fmt_exp_name(names, e.nm_i, e.inst_i));
            cur = e.out_i;
        }
        pts.reverse();
        pts.join(".")
    };

    // Locate donor ProductAsset_*_TA export
    let donor_low = donor_asset_path.to_lowercase();
    let mut donor_pa_idx: Option<usize> = None;
    for i in 0..exps.len() {
        let fp = full_exp_path(&exps, &names_list, exps[i].idx1).to_lowercase();
        if fp == donor_low && !fp.starts_with("productslots.") {
            donor_pa_idx = Some(i);
            break;
        }
    }
    if donor_pa_idx.is_none() {
        for i in 0..exps.len() {
            if full_exp_path(&exps, &names_list, exps[i].idx1).to_lowercase() == donor_low {
                donor_pa_idx = Some(i);
                break;
            }
        }
    }
    if donor_pa_idx.is_none() {
        let donor_leaf = donor_asset_path.split('.').last().unwrap_or("").to_lowercase();
        let pa_cands: Vec<usize> = (0..exps.len())
            .filter(|&i| {
                let cn = cls_name_of(&exps, &names_list, exps[i].cls_i).to_lowercase();
                cn.starts_with("productasset_")
                    && !cn.starts_with("productassetreference")
                    && !full_exp_path(&exps, &names_list, exps[i].idx1)
                        .to_lowercase()
                        .starts_with("productslots.")
            })
            .collect();
        donor_pa_idx = pa_cands
            .iter()
            .copied()
            .find(|&i| fmt_exp_name(&names_list, exps[i].nm_i, exps[i].inst_i).to_lowercase() == donor_leaf)
            .or_else(|| pa_cands.first().copied());
    }
    let Some(donor_pa_pos) = donor_pa_idx else {
        return Err(format!("Could not locate donor ProductAsset export '{}'", donor_asset_path));
    };

    let default_name_flags = name_flags_list
        .get(exps[donor_pa_pos].nm_i)
        .copied()
        .unwrap_or(0x0007_0010_0000_0000);

    fn ensure_fname(
        names_list: &mut Vec<String>,
        extra_nt: &mut Vec<u8>,
        default_name_flags: u64,
        base_str: &str,
        stored_inst: i32,
    ) -> (usize, i32) {
        let low = base_str.to_lowercase();
        if let Some(pos) = names_list.iter().position(|n| n.to_lowercase() == low) {
            return (pos, stored_inst);
        }
        extra_nt.extend_from_slice(&serialize_name_entry(base_str, default_name_flags));
        names_list.push(base_str.to_string());
        (names_list.len() - 1, stored_inst)
    }

    // Template Core.Package export (12-byte None stub)
    let donor_out = exps[donor_pa_pos].out_i;
    let tpl_pkg = if donor_out > 0 && (donor_out as usize) <= exps.len() {
        let c = &exps[(donor_out - 1) as usize];
        if cls_name_of(&exps, &names_list, c.cls_i).eq_ignore_ascii_case("package")
            && c.serial_sz == 12
            && c.serial_off > 0
        {
            Some((c.cls_i, c.obj_flags, c.serial_sz, c.serial_off, c.exp_flags))
        } else {
            None
        }
    } else {
        None
    }
    .or_else(|| {
        exps.iter().find_map(|c| {
            if cls_name_of(&exps, &names_list, c.cls_i).eq_ignore_ascii_case("package")
                && c.serial_sz == 12
                && c.serial_off > 0
            {
                Some((c.cls_i, c.obj_flags, c.serial_sz, c.serial_off, c.exp_flags))
            } else {
                None
            }
        })
    });

    // Walk donor PA's root Package export (`out_i == 0`) so single-depth content packages rename
    // cleanly in place instead of spawning a second overlapping Package export at the same offset.
    let mut donor_root_pos: Option<usize> = None;
    {
        let mut cur = exps[donor_pa_pos].out_i;
        for _ in 0..8 {
            if cur <= 0 || (cur as usize) > exps.len() {
                break;
            }
            let pos = (cur - 1) as usize;
            if exps[pos].out_i == 0 && cls_name_of(&exps, &names_list, exps[pos].cls_i).eq_ignore_ascii_case("package") {
                let d_root_nm = fmt_exp_name(&names_list, exps[pos].nm_i, exps[pos].inst_i).to_lowercase();
                let d_stem_base = donor_pkg_stem
                    .to_lowercase()
                    .trim_end_matches("_sf")
                    .to_string();
                let d_pa_top = donor_asset_path
                    .split('.')
                    .next()
                    .unwrap_or("")
                    .to_lowercase();
                if (d_root_nm == d_stem_base || d_root_nm == d_pa_top) && !d_root_nm.is_empty() {
                    donor_root_pos = Some(pos);
                }
                break;
            }
            cur = exps[pos].out_i;
        }
    }

    // Build/reuse Core.Package outer chain
    let mut cur_outer = 0i32;
    for (seg_idx, (seg_base, seg_inst)) in target_chain[..target_chain.len() - 1].iter().enumerate() {
        let found = exps.iter().find_map(|e| {
            if cls_name_of(&exps, &names_list, e.cls_i).eq_ignore_ascii_case("package")
                && e.out_i == cur_outer
                && names_list.get(e.nm_i).map(|s| s.to_lowercase()) == Some(seg_base.to_lowercase())
                && e.inst_i == *seg_inst
            {
                Some(e.idx1)
            } else {
                None
            }
        });
        if let Some(existing_idx1) = found {
            cur_outer = existing_idx1;
        } else if seg_idx == 0
            && donor_root_pos.is_some()
            && !seg_base.eq_ignore_ascii_case("archetypes")
            && !seg_base.eq_ignore_ascii_case("engine")
        {
            let rpos = donor_root_pos.unwrap();
            let (seg_nm_i, seg_inst_i) = ensure_fname(&mut names_list, &mut extra_nt, default_name_flags, seg_base, *seg_inst);
            exps[rpos].nm_i = seg_nm_i;
            exps[rpos].inst_i = seg_inst_i;
            write_i32_le(&mut exps[rpos].raw, 12, seg_nm_i as i32);
            write_i32_le(&mut exps[rpos].raw, 16, seg_inst_i);
            cur_outer = exps[rpos].idx1;
        } else {
            let (seg_nm_i, seg_inst_i) = ensure_fname(&mut names_list, &mut extra_nt, default_name_flags, seg_base, *seg_inst);
            let (p_cls, p_oflags, p_ssz, p_soff, p_eflags) = tpl_pkg.unwrap_or((
                pkg_import_cls_idx.unwrap_or(-1),
                0x0007_0004_0000_0000,
                0,
                0,
                1,
            ));
            let p_pflags: i32 = if cur_outer == 0 { 0x2000_0005 } else { 0 };
            let mut raw_pkg = vec![0u8; 72];
            write_i32_le(&mut raw_pkg, 0, p_cls);
            write_i32_le(&mut raw_pkg, 8, cur_outer);
            write_i32_le(&mut raw_pkg, 12, seg_nm_i as i32);
            write_i32_le(&mut raw_pkg, 16, seg_inst_i);
            raw_pkg[24..32].copy_from_slice(&p_oflags.to_le_bytes());
            write_i32_le(&mut raw_pkg, 32, p_ssz);
            write_i64_le(&mut raw_pkg, 36, p_soff);
            write_i32_le(&mut raw_pkg, 44, p_eflags);
            write_i32_le(&mut raw_pkg, 68, p_pflags);
            let new_idx1 = (exps.len() + 1) as i32;
            exps.push(RewireExp {
                idx1: new_idx1,
                cls_i: p_cls,
                out_i: cur_outer,
                nm_i: seg_nm_i,
                inst_i: seg_inst_i,
                obj_flags: p_oflags,
                serial_sz: p_ssz,
                serial_off: p_soff,
                exp_flags: p_eflags,
                raw: raw_pkg,
            });
            cur_outer = new_idx1;
        }
    }

    // Evict sibling shadow exports matching target leaf path
    let (leaf_base, leaf_inst) = &target_chain[target_chain.len() - 1];
    let (leaf_nm_i, leaf_inst_i) = ensure_fname(&mut names_list, &mut extra_nt, default_name_flags, leaf_base, *leaf_inst);
    let donor_pa_idx1 = exps[donor_pa_pos].idx1;
    for i in 0..exps.len() {
        if exps[i].idx1 != donor_pa_idx1
            && exps[i].out_i == cur_outer
            && names_list.get(exps[i].nm_i).map(|s| s.to_lowercase()) == Some(leaf_base.to_lowercase())
            && exps[i].inst_i == leaf_inst_i
        {
            let ev_str = format!("__swapped_shadow_{}", exps[i].idx1);
            let (ev_nm_i, ev_inst_i) = ensure_fname(&mut names_list, &mut extra_nt, default_name_flags, &ev_str, 0);
            exps[i].nm_i = ev_nm_i;
            exps[i].inst_i = ev_inst_i;
            write_i32_le(&mut exps[i].raw, 12, ev_nm_i as i32);
            write_i32_le(&mut exps[i].raw, 16, ev_inst_i);
        }
    }

    // Rewire donor ProductAsset_*_TA export
    exps[donor_pa_pos].out_i = cur_outer;
    exps[donor_pa_pos].nm_i = leaf_nm_i;
    exps[donor_pa_pos].inst_i = leaf_inst_i;
    write_i32_le(&mut exps[donor_pa_pos].raw, 8, cur_outer);
    write_i32_le(&mut exps[donor_pa_pos].raw, 12, leaf_nm_i as i32);
    write_i32_le(&mut exps[donor_pa_pos].raw, 16, leaf_inst_i);

    // Rewire donor root _SF Package export if target root _SF Package isn't already present
    let (sf_base, sf_inst) = target_sf_fname;
    let sf_exists = exps.iter().any(|e| {
        cls_name_of(&exps, &names_list, e.cls_i).eq_ignore_ascii_case("package")
            && e.out_i == 0
            && names_list.get(e.nm_i).map(|s| s.to_lowercase()) == Some(sf_base.to_lowercase())
            && e.inst_i == *sf_inst
    });
    if !sf_exists {
        let dstem_low = donor_pkg_stem.to_lowercase();
        for i in 0..exps.len() {
            if cls_name_of(&exps, &names_list, exps[i].cls_i).eq_ignore_ascii_case("package")
                && exps[i].out_i == 0
                && fmt_exp_name(&names_list, exps[i].nm_i, exps[i].inst_i).to_lowercase() == dstem_low
            {
                let (sf_nm_i, sf_inst_i) = ensure_fname(&mut names_list, &mut extra_nt, default_name_flags, sf_base, *sf_inst);
                exps[i].nm_i = sf_nm_i;
                exps[i].inst_i = sf_inst_i;
                write_i32_le(&mut exps[i].raw, 12, sf_nm_i as i32);
                write_i32_le(&mut exps[i].raw, 16, sf_inst_i);
                break;
            }
        }
    }

    let new_nt_len = nt_end + extra_nt.len() + nt_tail.len();
    let new_import_rel = new_nt_len;
    let new_export_rel = new_import_rel + imp_blob.len();
    let exp_bytes_len: usize = exps.iter().map(|e| e.raw.len()).sum::<usize>() + exp_tail.len();
    let new_depends_rel = new_export_rel + exp_bytes_len;
    let header_delta = new_depends_rel as isize - depends_off as isize;

    if header_delta != 0 {
        for e in &mut exps {
            if e.serial_off > 0 {
                let updated = (e.serial_off as isize + header_delta) as i64;
                e.serial_off = updated;
                write_i64_le(&mut e.raw, 36, updated);
            }
        }
    }

    let mut beyond = header[depends_off..].to_vec();
    if header_delta != 0 {
        let c_rel_is = compressed_chunks_offset as isize - depends_off as isize;
        if c_rel_is >= 0 {
            let c_rel = c_rel_is as usize;
            if c_rel + 4 <= beyond.len() {
                let num_chunks = read_i32_le(&beyond, c_rel).clamp(0, 512) as usize;
                for i in 0..num_chunks {
                    let e_off = c_rel + 4 + i * chunk_stride;
                    if e_off + chunk_stride > beyond.len() {
                        break;
                    }
                    let u_off = read_i64_le(&beyond, e_off);
                    write_i64_le(&mut beyond, e_off, (u_off as isize + header_delta) as i64);
                }
            }
        }
    }

    let mut new_hdr = Vec::with_capacity(new_depends_rel + beyond.len());
    new_hdr.extend_from_slice(&header[..nt_end]);
    new_hdr.extend_from_slice(&extra_nt);
    new_hdr.extend_from_slice(nt_tail);
    new_hdr.extend_from_slice(imp_blob);
    for e in &exps {
        new_hdr.extend_from_slice(&e.raw);
    }
    new_hdr.extend_from_slice(exp_tail);
    new_hdr.extend_from_slice(&beyond);

    Ok((
        new_hdr,
        header_delta,
        names_list.len(),
        exps.len(),
        new_import_rel,
        new_export_rel,
        new_depends_rel,
    ))
}

/// Applies surgical ExportTable rewiring + header capacity expansion + AES key re-encryption onto `donor_data`.
/// Reads the 16-byte `PackageGuid` from the unencrypted UPK prefix header if present.
pub fn extract_package_guid(raw: &[u8]) -> Option<[u8; 16]> {
    let pfx = parse_upk_prefix(raw).ok()?;
    let goff = pfx.import_export_guids_offset_off + 16;
    if goff + 16 <= raw.len() {
        let mut g = [0u8; 16];
        g.copy_from_slice(&raw[goff..goff + 16]);
        Some(g)
    } else {
        None
    }
}

/// Surgically rewires donor UPK to target `ProductAsset` + root packages while optionally adopting `target_guid`.
pub fn build_surgically_rewired_upk_with_guid(
    donor_data: &[u8],
    donor_key: &[u8; 32],
    output_key: &[u8; 32],
    donor_asset_path: &str,
    donor_pkg_stem: &str,
    target_chain: &[(String, i32)],
    target_sf_fname: &(String, i32),
    target_guid: Option<[u8; 16]>,
) -> Result<Vec<u8>, String> {
    let pfx = parse_upk_prefix(donor_data)?;
    let is_ctr = pfx.ctr_nonce.is_some();
    let chunk_stride = if pfx.licensee_version >= 33 || is_ctr { 36 } else { 24 };

    let name_off = pfx.name_offset;
    let enc_size = pfx
        .total_header_size
        .saturating_sub(pfx.garbage_size)
        .saturating_sub(name_off);
    let enc_aligned = (enc_size + 15) & !15;
    if enc_size == 0 || name_off + enc_aligned > donor_data.len() {
        return Err("Invalid UPK header bounds in donor package".into());
    }
    let enc_block = &donor_data[name_off..name_off + enc_aligned];
    let plain_full = if let Some(nonce) = pfx.ctr_nonce {
        aes_ctr_crypt(donor_key, &nonce, enc_block)
    } else {
        aes_ecb_decrypt(donor_key, enc_block)
    };
    let header_plain = &plain_full[..enc_size];

    let import_rel = pfx.import_offset.saturating_sub(name_off);
    let export_rel = pfx.export_offset.saturating_sub(name_off);
    let depends_rel = pfx.depends_offset.saturating_sub(name_off);

    let (
        mut new_hdr,
        header_delta,
        new_name_count,
        new_export_count,
        new_import_rel,
        new_export_rel,
        new_depends_rel,
    ) = rewire_header_surgical(
        header_plain,
        import_rel,
        export_rel,
        depends_rel,
        pfx.name_count,
        pfx.export_count,
        pfx.compressed_chunks_offset,
        donor_asset_path,
        donor_pkg_stem,
        target_chain,
        target_sf_fname,
        chunk_stride,
    )?;

    let new_enc_aligned = (new_hdr.len() + 15) & !15;
    let size_growth = new_enc_aligned as isize - enc_aligned as isize;
    let avail_gap = (pfx.total_header_size.saturating_sub(name_off)) as isize - enc_aligned as isize;

    let extra_file_shift: isize = if size_growth > avail_gap || header_delta > pfx.garbage_size as isize {
        let needed = (size_growth - avail_gap).max(header_delta - pfx.garbage_size as isize).max(0) as usize;
        let shift = ((needed + 255) & !15) as isize;
        let c_off_hdr = (pfx.compressed_chunks_offset as isize + header_delta).max(0) as usize;
        if c_off_hdr + 4 <= new_hdr.len() {
            let num_chunks = read_i32_le(&new_hdr, c_off_hdr).clamp(0, 512) as usize;
            for i in 0..num_chunks {
                let e_off = c_off_hdr + 4 + i * chunk_stride;
                if e_off + chunk_stride > new_hdr.len() {
                    break;
                }
                let comp_off = read_i64_le(&new_hdr, e_off + 12);
                write_i64_le(&mut new_hdr, e_off + 12, (comp_off as isize + shift) as i64);
            }
        }
        shift
    } else {
        0
    };

    let c_off_final = (pfx.compressed_chunks_offset as isize + header_delta).max(0) as usize;
    let padded_len = new_enc_aligned;
    new_hdr.resize(padded_len, 0);

    let new_enc_block = if let Some(nonce) = pfx.ctr_nonce {
        aes_ctr_crypt(output_key, &nonce, &new_hdr)
    } else {
        aes_ecb_encrypt(output_key, &new_hdr)
    };

    // Splice prefix + new_enc_block + gap adjustment + chunk payloads
    let mut output = Vec::with_capacity((donor_data.len() as isize + extra_file_shift).max(0) as usize);
    let mut prefix = donor_data[..name_off].to_vec();

    if header_delta != 0
        || extra_file_shift != 0
        || new_name_count != pfx.name_count
        || new_export_count != pfx.export_count
    {
        if extra_file_shift != 0 {
            write_i32_le(&mut prefix, pfx.total_header_size_off, (pfx.total_header_size as isize + extra_file_shift) as i32);
        }
        write_i32_le(&mut prefix, pfx.name_count_off, new_name_count as i32);
        write_i32_le(&mut prefix, pfx.export_count_off, new_export_count as i32);
        write_i32_le(&mut prefix, pfx.import_offset_off, (name_off + new_import_rel) as i32);
        write_i32_le(&mut prefix, pfx.export_offset_off, (name_off + new_export_rel) as i32);
        write_i32_le(&mut prefix, pfx.depends_offset_off, (name_off + new_depends_rel) as i32);
        write_i32_le(&mut prefix, pfx.import_export_guids_offset_off, (name_off + new_depends_rel) as i32);
        if pfx.generations_count > 0 {
            let last_gen = pfx.generations_entries_off + (pfx.generations_count - 1) * 12;
            if last_gen + 8 <= prefix.len() {
                write_i32_le(&mut prefix, last_gen, new_export_count as i32);
                write_i32_le(&mut prefix, last_gen + 4, new_name_count as i32);
            }
        }
        let new_comp_meta = pfx.compression_meta_off;
        if new_comp_meta + 8 <= prefix.len() {
            write_i32_le(
                &mut prefix,
                new_comp_meta,
                (pfx.garbage_size as isize + extra_file_shift - header_delta) as i32,
            );
            write_i32_le(
                &mut prefix,
                new_comp_meta + 4,
                c_off_final as i32,
            );
        }
    }
    if let Some(guid) = target_guid {
        let goff = pfx.import_export_guids_offset_off + 16;
        if goff + 16 <= prefix.len() {
            prefix[goff..goff + 16].copy_from_slice(&guid);
        }
    }

    output.extend_from_slice(&prefix);
    output.extend_from_slice(&new_enc_block[..new_enc_aligned]);

    let old_gap_start = name_off + enc_aligned;
    let old_gap_end = pfx.total_header_size;
    let new_gap_target_len = (old_gap_end as isize - old_gap_start as isize + (extra_file_shift - size_growth)).max(0) as usize;
    let existing_gap = &donor_data[old_gap_start..old_gap_end.min(donor_data.len())];
    if new_gap_target_len <= existing_gap.len() {
        output.extend_from_slice(&existing_gap[..new_gap_target_len]);
    } else {
        output.extend_from_slice(existing_gap);
        output.resize(output.len() + (new_gap_target_len - existing_gap.len()), 0);
    }

    output.extend_from_slice(&donor_data[old_gap_end..]);

    // Re-encrypt CTR chunks if needed
    if pfx.ctr_nonce.is_some() && output_key != donor_key && c_off_final + 4 <= new_hdr.len() {
        let num_chunks = read_i32_le(&new_hdr, c_off_final).clamp(0, 512) as usize;
        for i in 0..num_chunks {
            let e_off = c_off_final + 4 + i * 36;
            if e_off + 36 > new_hdr.len() {
                break;
            }
            let comp_off = read_i64_le(&new_hdr, e_off + 12).max(0) as usize;
            let comp_sz = read_i32_le(&new_hdr, e_off + 20).max(0) as usize;
            if comp_sz > 0 && comp_off + comp_sz <= output.len() {
                let mut chunk_nonce = [0u8; 12];
                chunk_nonce.copy_from_slice(&new_hdr[e_off + 24..e_off + 36]);
                let plain_ch = aes_ctr_crypt(donor_key, &chunk_nonce, &output[comp_off..comp_off + comp_sz]);
                let re_enc = aes_ctr_crypt(output_key, &chunk_nonce, &plain_ch);
                output[comp_off..comp_off + comp_sz].copy_from_slice(&re_enc);
            }
        }
    }

    Ok(output)
}

/// Surgically rewires donor UPK to target `ProductAsset` + root packages.
pub fn build_surgically_rewired_upk(
    donor_data: &[u8],
    donor_key: &[u8; 32],
    output_key: &[u8; 32],
    donor_asset_path: &str,
    donor_pkg_stem: &str,
    target_chain: &[(String, i32)],
    target_sf_fname: &(String, i32),
) -> Result<Vec<u8>, String> {
    build_surgically_rewired_upk_with_guid(
        donor_data,
        donor_key,
        output_key,
        donor_asset_path,
        donor_pkg_stem,
        target_chain,
        target_sf_fname,
        None,
    )
}

/// Swaps `donor_item` into `target_item` inside `cooked_dir` using surgical ExportTable rewiring,
/// header expansion, and optional binary Paint Color Shader injection.
pub fn swap_item_packages(
    cooked_dir: &Path,
    donor_item: &CatalogItem,
    target_item: &CatalogItem,
    paint_color: Option<&PaintColorSpec>,
) -> Result<String, String> {
    let target_path = cooked_dir.join(&target_item.asset_package);
    let donor_raw_path = cooked_dir.join(&donor_item.asset_package);
    if !target_path.exists() {
        return Err(format!("Target package not found: {}", target_path.display()));
    }
    if !donor_raw_path.exists() {
        return Err(format!("Donor package not found: {}", donor_raw_path.display()));
    }

    // 1. Ensure pristine target backup in both .bak and Vault
    ensure_pristine_before_swap(&target_path)?;
    let target_pristine = {
        let b = bak_path_for(&target_path);
        if b.exists() { b } else { target_path.clone() }
    };
    let target_raw = fs::read(&target_pristine).map_err(|e| e.to_string())?;
    let target_pfx = parse_upk_prefix(&target_raw)?;
    let (_, target_key, _) = decrypt_upk_header(&target_pristine, &target_raw, &target_pfx)?;

    // 2. Read pristine donor (and bake Paint Color shader edits if chosen)
    let donor_bak = bak_path_for(&donor_raw_path);
    let donor_path = if donor_bak.exists() { &donor_bak } else { &donor_raw_path };

    let mut color_edits = 0usize;
    let mut tex_tints = 0usize;
    let (donor_data, donor_key) = if let Some(col) = paint_color {
        let (donor_upk, donor_orig_raw) = unpack_upk_from_disk(donor_path)?;
        let mut mod_bytes = donor_upk.file_bytes.clone();
        color_edits = inject_paint_color_into_upk(&donor_upk, &mut mod_bytes, col);
        let (body_reps, added_names, body_edits) =
            build_body_trim_resized_replacements(&donor_upk, &mod_bytes, col);
        color_edits += body_edits;
        let built = if !body_reps.is_empty() || !added_names.is_empty() {
            rebuild_upk_with_mod_resized_added_and_names_to_vec(
                &donor_orig_raw,
                &donor_upk,
                &mod_bytes,
                &body_reps,
                &[],
                &added_names,
                &donor_upk.key,
            )?
        } else if color_edits == 0 {
            let (tex_reps, tex_added_names) =
                crate::custom_studio::build_tinted_texture_replacements_with_names(&donor_upk, cooked_dir, col);
            tex_tints = tex_reps.len();
            if tex_reps.is_empty() {
                rebuild_upk_selective_chunks_to_vec(&donor_orig_raw, &donor_upk, &mod_bytes, &donor_upk.key)?
            } else {
                rebuild_upk_with_mod_resized_added_and_names_to_vec(
                    &donor_orig_raw,
                    &donor_upk,
                    &mod_bytes,
                    &tex_reps,
                    &[],
                    &tex_added_names,
                    &donor_upk.key,
                )?
            }
        } else {
            rebuild_upk_selective_chunks_to_vec(&donor_orig_raw, &donor_upk, &mod_bytes, &donor_upk.key)?
        };
        (built, donor_upk.key)
    } else {
        let raw = fs::read(donor_path).map_err(|e| e.to_string())?;
        let pfx = parse_upk_prefix(&raw)?;
        let (_, k, _) = decrypt_upk_header(donor_path, &raw, &pfx)?;
        (raw, k)
    };

    // 3. Extract target exact ExportTable FName chain & apply surgical rewiring
    let target_stem = target_path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let donor_stem = donor_raw_path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let (target_chain, target_sf_fname) = extract_target_fnames_from_upk(
        &target_pristine,
        &target_key,
        &target_item.asset_path,
        &target_stem,
    );

    let target_guid = fs::read(&target_pristine)
        .ok()
        .and_then(|b| extract_package_guid(&b));
    let final_bytes = build_surgically_rewired_upk_with_guid(
        &donor_data,
        &donor_key,
        &target_key,
        &donor_item.asset_path,
        &donor_stem,
        &target_chain,
        &target_sf_fname,
        target_guid,
    )?;

    fs::write(&target_path, &final_bytes).map_err(|e| e.to_string())?;
    clear_rl_texture_cache();

    let color_note = paint_color
        .map(|c| {
            if tex_tints > 0 {
                format!(" [{} · {} tinted textures]", c.name, tex_tints)
            } else {
                format!(" [{} · {} shader edits]", c.name, color_edits)
            }
        })
        .unwrap_or_default();
    Ok(format!(
        "Swapped {}{} -> {} ({})",
        donor_item.product, color_note, target_item.product, target_item.asset_package
    ))
}

// ── Car Body Hitbox Compatibility Tags ───────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CarHitbox {
    Octane,
    Dominus,
    Hybrid,
    Breakout,
    Plank,
    Merc,
}

impl CarHitbox {
    pub fn label(&self) -> &'static str {
        match self {
            CarHitbox::Octane => "Octane",
            CarHitbox::Dominus => "Dominus",
            CarHitbox::Hybrid => "Hybrid",
            CarHitbox::Breakout => "Breakout",
            CarHitbox::Plank => "Plank",
            CarHitbox::Merc => "Merc",
        }
    }
}

/// Resolves Rocket League's official hitbox family for any Body `CatalogItem`.
pub fn body_hitbox_for_item(product: &str, asset_package: &str) -> CarHitbox {
    let low_p = product.to_lowercase();
    let low_f = asset_package.to_lowercase();

    // Merc Hitbox
    if low_p.contains("merc")
        || low_p.contains("battle bus")
        || low_p.contains("nomad")
        || low_p.contains("sweet tooth")
        || low_p.contains("patty wagon")
        || low_p.contains("turtle van")
        || low_p.contains("behemoth")
        || low_p.contains("gummi ship")
        || low_p.contains("homer")
        || low_p.contains("defender")
        || low_f.contains("vanquish")
    {
        return CarHitbox::Merc;
    }

    // Plank Hitbox
    if low_p.contains("batmobile (2016)")
        || low_p == "mantis"
        || low_p == "paladin"
        || low_p == "sentinel"
        || low_p.contains("twin mill")
        || low_p.starts_with("artemis")
        || low_p.contains("formula 1")
        || low_p.contains("mclaren")
        || low_p.contains("1989")
    {
        return CarHitbox::Plank;
    }

    // Breakout Hitbox
    if low_p.starts_with("breakout")
        || low_p.contains("animus")
        || low_p == "centio"
        || low_p == "cyclone"
        || low_p == "komodo"
        || low_p == "samurai"
        || low_p == "whiplash"
        || low_p == "quadra"
        || low_p.contains("ferrari f40")
        || low_p.contains("azura")
        || low_p.contains("redline")
        || low_f.contains("body_force")
        || low_f.contains("body_carcar")
    {
        return CarHitbox::Breakout;
    }

    // Hybrid Hitbox
    if low_p == "endo"
        || low_p == "esper"
        || low_p.contains("ger 619")
        || low_p == "nimbus"
        || low_p.starts_with("r3mx")
        || low_p == "tygris"
        || low_p == "venom"
        || low_p.starts_with("x-devil")
        || low_p == "insidio"
        || low_p.contains("skyline")
        || low_p.contains("nissan")
        || low_p.contains("rx-7")
        || low_p.contains("ioniq")
        || low_p.contains("fiero")
        || low_p.contains("cybertruck")
        || low_p == "diesel"
        || low_p.contains("fuse")
        || low_p.contains("shokunin")
        || low_p.contains("primo")
        || low_p.contains("corlay")
    {
        return CarHitbox::Hybrid;
    }

    // Dominus Hitbox
    if low_p.starts_with("dominus")
        || low_p == "aftershock"
        || low_p.contains("time machine")
        || low_p.contains("charger")
        || low_p.contains("diestro")
        || low_p.contains("viper")
        || low_p == "ecto-1"
        || low_p.starts_with("emperor")
        || low_p.contains("mustang")
        || low_p.contains("bmw")
        || low_p.contains("gazella")
        || low_p.starts_with("guardian")
        || low_p == "hotshot"
        || low_p.contains("imperator")
        || low_p == "ripper"
        || low_p.starts_with("ronin")
        || low_p.starts_with("maverick")
        || low_p.contains("lamborghini")
        || low_p.contains("porsche")
        || low_p.contains("nascar")
        || low_p.contains("ferrari")
        || low_p.contains("mercedes")
        || low_p.contains("corvette")
        || low_p.contains("cadillac")
        || low_p.contains("aston martin")
        || low_p.contains("bumblebee")
        || low_p.contains("maestro")
        || low_p.contains("chikara")
        || low_p == "masamune"
        || low_p.contains("firebird")
        || low_p.contains("maserati")
        || low_p.contains("k.i.t.t.")
        || low_p == "nemesis"
        || low_p.contains("nexus")
        || low_p.contains("peregrine")
        || low_p.contains("lockjaw")
        || low_p.contains("tyranno")
        || low_p.contains("batmobile (2022)")
    {
        return CarHitbox::Dominus;
    }

    // Octane Hitbox (standard & compact SUVs/cars)
    CarHitbox::Octane
}

// ── Active Swaps Scanner & Game Integrity Health Checker ─────────────────────

#[derive(Debug, Clone)]
pub struct CookedHealthReport {
    pub cooked_exists: bool,
    pub total_bak_files: usize,
    pub missing_vault_backups: Vec<String>,
    pub damaged_avatar_borders: Vec<String>,
    pub cache_file_count: usize,
    pub underpass_swapped: bool,
    pub bak_filenames: Vec<String>,
}

/// Scans `cooked_dir`, `%LOCALAPPDATA%\RLSwapper\pristine_backups`, and the RL texture cache to report integrity.
pub fn inspect_cooked_health(cooked_dir: &Path) -> CookedHealthReport {
    let mut rep = CookedHealthReport {
        cooked_exists: cooked_dir.is_dir(),
        total_bak_files: 0,
        missing_vault_backups: Vec::new(),
        damaged_avatar_borders: Vec::new(),
        cache_file_count: 0,
        underpass_swapped: false,
        bak_filenames: Vec::new(),
    };

    if let Ok(entries) = fs::read_dir(cooked_dir) {
        for entry in entries.flatten() {
            let nm = entry.file_name().to_string_lossy().to_string();
            let low = nm.to_lowercase();
            if low.ends_with(".upk.bak") {
                rep.total_bak_files += 1;
                let base_pkg = nm[..nm.len() - 4].to_string();
                if base_pkg.eq_ignore_ascii_case("Labs_Underpass_P.upk") {
                    rep.underpass_swapped = true;
                }
                rep.bak_filenames.push(base_pkg.clone());
                let vpath = vault_backup_path(Path::new(&base_pkg));
                if !vpath.exists() {
                    rep.missing_vault_backups.push(base_pkg);
                }
            } else if low.starts_with("avatarborder_") && !low.ends_with("_t_sf.upk") && low.ends_with("_sf.upk") {
                let bak = bak_path_for(&entry.path());
                if !bak.exists() && is_avatar_border_modified(&entry.path()) {
                    rep.damaged_avatar_borders.push(nm);
                }
            }
        }
    }

    if let Ok(userprofile) = std::env::var("USERPROFILE") {
        let cache_dir = PathBuf::from(userprofile)
            .join("Documents")
            .join("My Games")
            .join("Rocket League")
            .join("TAGame")
            .join("Cache");
        if let Ok(entries) = fs::read_dir(&cache_dir) {
            rep.cache_file_count = entries.flatten().filter(|e| e.path().is_file()).count();
        }
    }
    rep.bak_filenames.sort();
    rep
}

/// Repairs any orphaned/modified `AvatarBorder_*_SF.upk` files and syncs missing `.bak` files to the Pristine Vault.
pub fn repair_cooked_health(cooked_dir: &Path) -> (usize, usize) {
    let mut synced_vault = 0usize;
    let mut repaired_borders = 0usize;
    let rep = inspect_cooked_health(cooked_dir);
    for pkg in &rep.missing_vault_backups {
        let bak = bak_path_for(&cooked_dir.join(pkg));
        if bak.exists() && !is_avatar_border_modified(&bak) {
            if fs::copy(&bak, vault_backup_path(Path::new(pkg))).is_ok() {
                synced_vault += 1;
            }
        }
    }
    for nm in &rep.damaged_avatar_borders {
        if repair_avatar_border_upk(&cooked_dir.join(nm)) {
            repaired_borders += 1;
        }
    }
    clear_rl_texture_cache();
    (synced_vault, repaired_borders)
}

/// Clears Rocket League's local cooked cache so swapped items & maps appear immediately.
pub fn clear_rl_texture_cache() {
    if let Ok(userprofile) = std::env::var("USERPROFILE") {
        let cache_dir = PathBuf::from(userprofile)
            .join("Documents")
            .join("My Games")
            .join("Rocket League")
            .join("TAGame")
            .join("Cache");
        if cache_dir.is_dir() {
            if let Ok(entries) = fs::read_dir(&cache_dir) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.is_file() {
                        let _ = fs::remove_file(p);
                    }
                }
            }
        }
    }
}

/// Checks if `RocketLeague.exe` is currently running on Windows.
pub fn is_rocket_league_running() -> bool {
    #[cfg(target_os = "windows")]
    unsafe {
        use winapi::um::handleapi::{CloseHandle, INVALID_HANDLE_VALUE};
        use winapi::um::tlhelp32::{
            CreateToolhelp32Snapshot, Process32First, Process32Next, PROCESSENTRY32, TH32CS_SNAPPROCESS,
        };
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return false;
        }
        let mut entry: PROCESSENTRY32 = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32>() as u32;
        let mut found = false;
        if Process32First(snap, &mut entry) != 0 {
            loop {
                let name = std::ffi::CStr::from_ptr(entry.szExeFile.as_ptr())
                    .to_string_lossy()
                    .to_lowercase();
                if name == "rocketleague.exe" {
                    found = true;
                    break;
                }
                if Process32Next(snap, &mut entry) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snap);
        found
    }
    #[cfg(not(target_os = "windows"))]
    {
        false
    }
}

/// Finds an item by its official Rocket League Product ID (used by AlphaConsole BodyID / SkinID presets).
pub fn find_catalog_item_by_id(catalog: &[CatalogItem], id: i64) -> Option<CatalogItem> {
    catalog.iter().find(|it| it.id == id).cloned()
}

/// Maps a car body or decal package/product name to the canonical UV label (`Octane`, `Fennec`, `Dominus`, etc.).
pub fn detect_car_uv_label(pkg_or_name: &str) -> Option<&'static str> {
    let low = pkg_or_name.to_lowercase();
    if low.contains("grain") || low.contains("fennec") {
        Some("Fennec")
    } else if (low.contains("musclecar") || low.contains("dominus")) && !low.contains("dominus gt") && !low.contains("musclecar2") {
        Some("Dominus")
    } else if (low.contains("octane") || low.contains("pepe_")) && !low.contains("zsr") {
        Some("Octane")
    } else if low.contains("skin_force_") || low.contains("body_force") || low.contains("breakout") {
        Some("Breakout")
    } else if low.contains("skin_backfire_") || low.contains("backfire") {
        Some("Backfire")
    } else {
        None
    }
}
