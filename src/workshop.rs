//! Epic Games Steam Workshop Map Loader (`Labs_Underpass_P.upk`), Local Workshop Library
//! (.UDK / .UPK / .ZIP), `workshopTextures` Dependency Scanner & Installer, and Online
//! Community Workshop Catalog (`lethamyr.com` API).

use crate::upk_engine::{
    clear_rl_texture_cache, ensure_pristine_before_swap, get_appdata_dir, parse_upk_prefix,
    read_fstring_at, read_i32_le, read_u32_le, restore_single_package, unpack_upk_from_disk,
    PACKAGE_FILE_TAG,
};
use image::RgbaImage;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime};
use zip::ZipArchive;

fn shared_http_client() -> Option<&'static reqwest::blocking::Client> {
    static CLIENT: OnceLock<Option<reqwest::blocking::Client>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(15))
                .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) RLSwapper/4.1")
                .build()
                .ok()
        })
        .as_ref()
}

pub const WORKSHOP_SAFE_SLOT: &str = "Labs_Underpass_P.upk";
pub const WORKSHOP_TEXTURES_PAGE_URL: &str = "https://bakkesplugins.com/maps/96";
pub const WORKSHOP_TEXTURES_CDN_FALLBACK_URL: &str =
    "https://cdn.bakkesplugins.com/uploads/bda9556fe79d4359afb8e000ac7ec63e-96_1.0.0_6858564db4de4cba8ce31032563a0c44.zip";

pub fn get_workshop_library_dir() -> PathBuf {
    let p = get_appdata_dir().join("Workshop Maps");
    let _ = fs::create_dir_all(&p);
    p
}

pub fn get_workshop_textures_dir() -> PathBuf {
    let p = get_appdata_dir().join("Workshop Textures");
    let _ = fs::create_dir_all(&p);
    p
}

pub fn get_workshop_thumb_cache_dir() -> PathBuf {
    let p = get_appdata_dir().join("Cache").join("Workshop Thumbs");
    let _ = fs::create_dir_all(&p);
    p
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkshopMapEntry {
    pub id: String,
    pub name: String,
    pub description: String,
    pub map_file_rel: String,
    pub thumb_file_rel: Option<String>,
    pub extra_files_rel: Vec<String>,
    pub source_url: Option<String>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnlineWorkshopItem {
    pub name: String,
    pub short_desc: String,
    pub download_url: String,
    pub thumb_url: Option<String>,
    pub downloads: u64,
    pub likes: u64,
    #[serde(default)]
    pub source: String,
    #[serde(default = "default_true")]
    pub direct_download: bool,
}

#[derive(Debug, Clone)]
pub struct OnlineWorkshopPage {
    pub page: usize,
    pub last_page: usize,
    pub total: usize,
    pub items: Vec<OnlineWorkshopItem>,
}

// ── UE3 / UDK External Dependency & `workshopTextures` Scanner ───────────────

const BUILTIN_ENGINE_PACKAGES: &[&str] = &[
    "core",
    "engine",
    "enginematerials",
    "engineresources",
    "engine_materialfunctions02",
    "tagame",
    "projectx",
    "akaudio",
    "gfxui",
    "gfxuieditor",
    "ipdrv",
    "windrv",
    "onlinesubsystempc",
    "onlinesubsystemps4",
    "onlinesubsystemdurango",
    "onlinesubsystemsteamworks",
    "onlinesubsystemepic",
    "physicalmaterials",
    "postprocess",
    "fxactorevents",
    "customcolors",
    "psy_materialfunctions",
    "editormeshes",
    "editorresources",
    "unrealed",
    "temp",
    "transient",
];

/// Parses the `ImportTable` of an unencrypted `.udk` or encrypted `.upk` map and returns any root
/// `Package` dependencies that are missing from `cooked_dir`.
pub fn scan_upk_external_dependencies(map_path: &Path, cooked_dir: &Path) -> Vec<String> {
    let mut missing = Vec::new();
    let Ok(mut f) = File::open(map_path) else {
        return missing;
    };
    let file_len = f.metadata().map(|m| m.len() as usize).unwrap_or(0);
    if file_len < 64 {
        return missing;
    }
    let probe_len = file_len.min(512 * 1024);
    let mut raw = vec![0u8; probe_len];
    if f.read_exact(&mut raw).is_err() {
        return missing;
    }
    let Ok(pfx) = parse_upk_prefix(&raw) else {
        return missing;
    };

    // Build case-insensitive set of packages present in CookedPCConsole
    let mut existing_pkgs: HashSet<String> = HashSet::new();
    if let Ok(entries) = fs::read_dir(cooked_dir) {
        for e in entries.flatten() {
            let nm = e.file_name().to_string_lossy().to_lowercase();
            if let Some(stem) = nm.strip_suffix(".upk") {
                existing_pkgs.insert(stem.to_string());
            }
        }
    }

    let mut pkg_names: Vec<String> = Vec::new();
    let comp_flags = if pfx.compression_flags_off + 4 <= raw.len() {
        read_u32_le(&raw, pfx.compression_flags_off)
    } else {
        0
    };
    let needed_hdr_end = pfx.import_offset.saturating_add(pfx.import_count.saturating_mul(28));
    let is_plain_udk = (pfx.package_flags & 0x800) == 0
        && comp_flags == 0
        && pfx.name_offset > 0
        && needed_hdr_end <= file_len
        && needed_hdr_end <= 32 * 1024 * 1024;

    if is_plain_udk {
        if needed_hdr_end > raw.len() {
            let extra = needed_hdr_end - raw.len();
            let old_len = raw.len();
            raw.resize(needed_hdr_end, 0);
            if f.read_exact(&mut raw[old_len..old_len + extra]).is_err() {
                raw.truncate(old_len);
            }
        }
    }

    if is_plain_udk && needed_hdr_end <= raw.len() {
        // Walk plaintext NameTable directly
        let mut names = Vec::with_capacity(pfx.name_count);
        let mut npos = pfx.name_offset;
        let mut ok = true;
        for _ in 0..pfx.name_count {
            if let Some((nm, next_p)) = read_fstring_at(&raw, npos) {
                if next_p + 8 <= raw.len() {
                    names.push(nm);
                    npos = next_p + 8;
                    continue;
                }
            }
            ok = false;
            break;
        }
        if ok && !names.is_empty() {
            for i in 0..pfx.import_count {
                let off = pfx.import_offset + i * 28;
                let outer = read_i32_le(&raw, off + 16);
                let cn_idx = read_i32_le(&raw, off + 8) as usize;
                let on_idx = read_i32_le(&raw, off + 20) as usize;
                if outer == 0 {
                    let cname = names.get(cn_idx).map(|s| s.as_str()).unwrap_or("");
                    if cname.eq_ignore_ascii_case("Package") {
                        if let Some(pnm) = names.get(on_idx) {
                            pkg_names.push(pnm.clone());
                        }
                    }
                }
            }
        }
    }

    if pkg_names.is_empty() {
        if let Ok((upk, _)) = unpack_upk_from_disk(map_path) {
            for imp in &upk.imports {
                if imp.outer_index == 0 && upk.resolve_name(imp.class_name).eq_ignore_ascii_case("Package") {
                    pkg_names.push(upk.resolve_name(imp.object_name).to_string());
                }
            }
        }
    }

    let self_stem = map_path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();

    for pnm in pkg_names {
        let low = pnm.to_lowercase();
        if low.is_empty()
            || low == "none"
            || low == self_stem
            || BUILTIN_ENGINE_PACKAGES.contains(&low.as_str())
        {
            continue;
        }
        if !existing_pkgs.contains(&low) && !missing.iter().any(|m: &String| m.eq_ignore_ascii_case(&pnm)) {
            missing.push(format!("{}.upk", pnm));
        }
    }
    missing
}

/// Copies shared custom `workshopTextures` (`.upk` / `.tfc`) packages from `src_dir` into `cooked_dir`
/// without overwriting existing vanilla Rocket League packages. Returns `(copied, skipped_existing)`.
pub fn install_workshop_textures_folder(src_dir: &Path, cooked_dir: &Path) -> Result<(usize, usize), String> {
    if !src_dir.is_dir() || !cooked_dir.is_dir() {
        return Err("Invalid source workshopTextures or CookedPCConsole folder".into());
    }
    let mut copied = 0usize;
    let mut skipped = 0usize;
    for entry in walkdir::WalkDir::new(src_dir).max_depth(3).into_iter().flatten() {
        if !entry.file_type().is_file() {
            continue;
        }
        let p = entry.path();
        let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        if ext != "upk" && ext != "tfc" {
            continue;
        }
        let Some(fname) = p.file_name() else { continue };
        let dst = cooked_dir.join(fname);
        if dst.exists() {
            skipped += 1;
        } else if fs::copy(p, &dst).is_ok() {
            copied += 1;
        }
    }
    Ok((copied, skipped))
}

// ── Local Workshop Library Management ────────────────────────────────────────

pub fn sanitize_slug(name: &str) -> String {
    let clean: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
        .collect();
    clean.trim_matches('_').to_string()
}

/// Deduplicates legacy timestamped folders (`<slug>_18651`, `<slug>_20300`) inside `Workshop Maps\`
/// so each map has a single canonical folder and entry.
pub fn deduplicate_workshop_library() -> usize {
    let root = get_workshop_library_dir();
    let Ok(entries) = fs::read_dir(&root) else {
        return 0;
    };
    let mut by_key: std::collections::HashMap<(String, String), Vec<(PathBuf, WorkshopMapEntry, SystemTime)>> =
        std::collections::HashMap::new();

    for e in entries.flatten() {
        let dir = e.path();
        if !dir.is_dir() {
            continue;
        }
        let meta_p = dir.join("workshop.json");
        if !meta_p.is_file() {
            continue;
        }
        let Ok(txt) = fs::read_to_string(&meta_p) else {
            continue;
        };
        let Ok(m) = serde_json::from_str::<WorkshopMapEntry>(&txt) else {
            continue;
        };
        let mtime = meta_p
            .metadata()
            .and_then(|md| md.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        let key = (sanitize_slug(&m.name), m.map_file_rel.to_lowercase());
        by_key.entry(key).or_default().push((dir, m, mtime));
    }

    let mut removed = 0usize;
    for ((slug, _), mut group) in by_key {
        if group.len() <= 1 {
            continue;
        }
        // Prefer exact canonical slug folder if present; otherwise keep newest modified entry
        group.sort_by(|a, b| {
            let a_exact = a.1.id == slug;
            let b_exact = b.1.id == slug;
            b_exact.cmp(&a_exact).then_with(|| b.2.cmp(&a.2))
        });
        for (dup_dir, _, _) in group.into_iter().skip(1) {
            if fs::remove_dir_all(&dup_dir).is_ok() {
                removed += 1;
            }
        }
    }
    removed
}

pub fn list_library_maps() -> Vec<WorkshopMapEntry> {
    let _ = deduplicate_workshop_library();
    let root = get_workshop_library_dir();
    let mut list = Vec::new();
    let Ok(entries) = fs::read_dir(&root) else {
        return list;
    };
    for e in entries.flatten() {
        let meta_p = e.path().join("workshop.json");
        if meta_p.is_file() {
            if let Ok(txt) = fs::read_to_string(&meta_p) {
                if let Ok(m) = serde_json::from_str::<WorkshopMapEntry>(&txt) {
                    list.push(m);
                }
            }
        }
    }
    list.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    list
}

/// Finds a library map by exact ID or by matching its canonical slug prefix (for backwards-compatibility with legacy `_timestamp` IDs).
pub fn find_library_map_by_id_or_slug(id_or_slug: &str) -> Option<WorkshopMapEntry> {
    let maps = list_library_maps();
    if let Some(exact) = maps.iter().find(|e| e.id.eq_ignore_ascii_case(id_or_slug)) {
        return Some(exact.clone());
    }
    let q_slug = sanitize_slug(id_or_slug);
    maps.into_iter().find(|e| {
        let es = sanitize_slug(&e.name);
        !es.is_empty() && (es == q_slug || q_slug.starts_with(&format!("{}_", es)) || e.id.starts_with(&format!("{}_", q_slug)))
    })
}

pub fn delete_library_map(entry: &WorkshopMapEntry) -> Result<(), String> {
    if entry.id.is_empty()
        || entry.id == "."
        || entry.id.contains('/')
        || entry.id.contains('\\')
        || entry.id.contains("..")
    {
        return Err("Invalid workshop map ID".into());
    }
    let dir = get_workshop_library_dir().join(&entry.id);
    if dir.exists() {
        fs::remove_dir_all(dir).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Imports a `.udk`, `.upk`, or `.zip` Workshop map archive into `Downloads\RL Swapper\Workshop Maps\`,
/// using a deterministic slug folder and deduplicating any legacy timestamped copies of the same map.
pub fn import_workshop_file_or_zip(src_path: &Path, display_name_override: Option<&str>, source_url: Option<&str>) -> Result<WorkshopMapEntry, String> {
    if !src_path.is_file() {
        return Err(format!("File not found: {}", src_path.display()));
    }
    let ext = src_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let raw_title = display_name_override
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            src_path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .replace('_', " ")
        });
    let lib_root = get_workshop_library_dir();
    let base_slug = {
        let s = sanitize_slug(&raw_title);
        if s.is_empty() { "workshop_map".to_string() } else { s }
    };

    // Remove any legacy timestamped duplicate folders for this same map title before writing canonical folder
    if let Ok(entries) = fs::read_dir(&lib_root) {
        for e in entries.flatten() {
            let meta_p = e.path().join("workshop.json");
            if let Ok(txt) = fs::read_to_string(&meta_p) {
                if let Ok(existing) = serde_json::from_str::<WorkshopMapEntry>(&txt) {
                    if sanitize_slug(&existing.name) == base_slug && existing.id != base_slug {
                        let _ = fs::remove_dir_all(e.path());
                    }
                }
            }
        }
    }

    let id = base_slug;
    let dest_dir = lib_root.join(&id);
    fs::create_dir_all(&dest_dir).map_err(|e| e.to_string())?;

    let mut map_file_rel: Option<String> = None;
    let mut best_map_size = 0u64;
    let mut thumb_file_rel: Option<String> = None;
    let mut extra_files_rel: Vec<String> = Vec::new();
    let mut description = String::from("Imported Workshop Map (.UDK / .UPK)");

    if ext == "zip" {
        let f = File::open(src_path).map_err(|e| e.to_string())?;
        let mut archive = ZipArchive::new(f).map_err(|e| format!("Invalid ZIP archive: {e}"))?;
        for i in 0..archive.len() {
            let mut zf = archive.by_index(i).map_err(|e| e.to_string())?;
            if !zf.is_file() {
                continue;
            }
            let zname = zf.name().to_string();
            let base_nm = Path::new(&zname)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            if base_nm.is_empty() || base_nm.starts_with('.') {
                continue;
            }
            let f_ext = Path::new(&base_nm)
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_lowercase();

            if f_ext == "udk" || f_ext == "upk" || f_ext == "tfc" {
                let out_file = dest_dir.join(&base_nm);
                let sz = {
                    let mut out = File::create(&out_file).map_err(|e| e.to_string())?;
                    io::copy(&mut zf, &mut out).map_err(|e| e.to_string())?
                };

                if (f_ext == "udk" || (f_ext == "upk" && base_nm.to_lowercase().ends_with("_p.upk")))
                    && sz > best_map_size
                {
                    if let Some(prev) = map_file_rel.replace(base_nm.clone()) {
                        extra_files_rel.push(prev);
                    }
                    best_map_size = sz;
                } else if f_ext == "upk" && map_file_rel.is_none() && sz > best_map_size {
                    best_map_size = sz;
                    map_file_rel = Some(base_nm.clone());
                } else {
                    extra_files_rel.push(base_nm.clone());
                }
            } else if matches!(f_ext.as_str(), "png" | "jpg" | "jpeg" | "webp") && thumb_file_rel.is_none() {
                let out_img = dest_dir.join(&base_nm);
                if let Ok(mut out) = File::create(&out_img) {
                    if io::copy(&mut zf, &mut out).is_ok() {
                        thumb_file_rel = Some(base_nm);
                    }
                }
            } else if f_ext == "json" && base_nm.to_lowercase().ends_with(".json") {
                let mut s = String::new();
                if zf.take(1024 * 1024).read_to_string(&mut s).is_ok() {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) {
                        if let Some(d) = v.get("desc").or_else(|| v.get("description")).and_then(|x| x.as_str()) {
                            description = d.to_string();
                        }
                    }
                }
            }
        }
    } else if ext == "udk" || ext == "upk" {
        let base_nm = src_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let mut hdr = [0u8; 64];
        let hdr_ok = File::open(src_path)
            .and_then(|mut f| f.read_exact(&mut hdr))
            .is_ok()
            && read_u32_le(&hdr, 0) == PACKAGE_FILE_TAG;
        if !hdr_ok {
            let _ = fs::remove_dir_all(&dest_dir);
            return Err("File does not have a valid UE3 UPK/UDK header magic.".into());
        }
        fs::copy(src_path, dest_dir.join(&base_nm)).map_err(|e| e.to_string())?;
        map_file_rel = Some(base_nm);
    } else {
        let _ = fs::remove_dir_all(&dest_dir);
        return Err("Unsupported format. Select a .UDK, .UPK, or .ZIP Workshop archive.".into());
    }

    let Some(map_rel) = map_file_rel else {
        let _ = fs::remove_dir_all(&dest_dir);
        return Err("No .UDK or .UPK map package found inside archive.".into());
    };

    let entry = WorkshopMapEntry {
        id: id.clone(),
        name: raw_title,
        description,
        map_file_rel: map_rel,
        thumb_file_rel,
        extra_files_rel,
        source_url: source_url.map(|s| s.to_string()),
    };
    fs::write(
        dest_dir.join("workshop.json"),
        serde_json::to_string_pretty(&entry).unwrap(),
    )
    .map_err(|e| e.to_string())?;
    Ok(entry)
}

/// Loads a Workshop `.udk`/`.upk` map into the safe offline Free Play slot (`Labs_Underpass_P.upk`)
/// inside `cooked_dir`, automatically installing any bundled `.upk`/`.tfc` extra files and warning
/// about any remaining missing external texture dependencies.
pub fn load_workshop_map_into_underpass(
    cooked_dir: &Path,
    entry: &WorkshopMapEntry,
) -> Result<(String, Vec<String>), String> {
    let target_path = cooked_dir.join(WORKSHOP_SAFE_SLOT);
    if !target_path.exists() {
        return Err(format!("Safe Workshop slot {} not found in {}", WORKSHOP_SAFE_SLOT, cooked_dir.display()));
    }

    let map_dir = get_workshop_library_dir().join(&entry.id);
    let src_map_path = map_dir.join(&entry.map_file_rel);
    if !src_map_path.is_file() {
        return Err(format!("Workshop map file missing: {}", src_map_path.display()));
    }

    let mut hdr_buf = [0u8; 4096];
    let hdr_n = {
        let mut f = File::open(&src_map_path).map_err(|e| e.to_string())?;
        f.read(&mut hdr_buf).map_err(|e| e.to_string())?
    };
    if hdr_n < 64 || read_u32_le(&hdr_buf[..hdr_n], 0) != PACKAGE_FILE_TAG {
        return Err("Invalid UPK/UDK package magic".into());
    }
    // IMPORTANT: Do NOT set PKG_COOKED (0x8) on uncooked .udk maps — Epic/Steam RL loads uncooked
    // .udk files natively when copied unmodified, whereas forcing 0x8 triggers Bad name index crashes.
    let _ = parse_upk_prefix(&hdr_buf[..hdr_n]).map_err(|e| format!("Corrupt UPK/UDK header: {e}"))?;

    // Copy any non-conflicting bundled extra texture/dependency UPKs shipped in the zip
    let mut copied_extras = 0usize;
    for extra in &entry.extra_files_rel {
        let ep = map_dir.join(extra);
        let dst = cooked_dir.join(extra);
        if ep.is_file() && !dst.exists() {
            if fs::copy(&ep, &dst).is_ok() {
                copied_extras += 1;
            }
        }
    }

    // Also auto-install any cached shared Workshop Textures from `Downloads\RL Swapper\Workshop Textures`
    let tex_dir = get_workshop_textures_dir();
    if let Ok((c, _)) = install_workshop_textures_folder(&tex_dir, cooked_dir) {
        copied_extras += c;
    }

    let missing_deps = scan_upk_external_dependencies(&src_map_path, cooked_dir);

    ensure_pristine_before_swap(&target_path)?;
    fs::copy(&src_map_path, &target_path).map_err(|e| e.to_string())?;
    clear_rl_texture_cache();

    let extra_note = if copied_extras > 0 {
        format!(" (+{} bundled texture packs)", copied_extras)
    } else {
        String::new()
    };
    Ok((
        format!(
            "Loaded '{}' into Free Play ({}){} — launch Play > Training > Free Play > Underpass!",
            entry.name, WORKSHOP_SAFE_SLOT, extra_note
        ),
        missing_deps,
    ))
}

/// Restores `Labs_Underpass_P.upk` back to its pristine Rocket League original.
pub fn restore_workshop_underpass(cooked_dir: &Path) -> bool {
    let p = cooked_dir.join(WORKSHOP_SAFE_SLOT);
    let ok = restore_single_package(&p);
    clear_rl_texture_cache();
    ok
}

// ── Online Community Workshop Browser (`lethamyr.com/api/v1/maps`) ───────────

pub fn fetch_lethamyr_catalog_page(page: usize) -> Result<OnlineWorkshopPage, String> {
    let client = shared_http_client().ok_or_else(|| "Failed to initialize HTTP client".to_string())?;

    let url = format!("https://lethamyr.com/api/v1/maps?page={}", page.max(1));
    let val: serde_json::Value = client
        .get(&url)
        .send()
        .map_err(|e| format!("Network error: {e}"))?
        .error_for_status()
        .map_err(|e| format!("API error: {e}"))?
        .json()
        .map_err(|e| format!("Invalid JSON: {e}"))?;

    let mut items = Vec::new();
    if let Some(arr) = val.get("data").and_then(|d| d.as_array()) {
        for obj in arr {
            let name = obj.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let dl = obj.get("download_url").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if name.is_empty() || dl.is_empty() {
                continue;
            }
            let short_desc = obj
                .get("description_short")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let thumb_url = obj
                .get("media")
                .and_then(|m| m.as_array())
                .and_then(|a| a.first())
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let downloads = obj
                .get("counts")
                .and_then(|c| c.get("downloads"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let likes = obj
                .get("counts")
                .and_then(|c| c.get("likes"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            items.push(OnlineWorkshopItem {
                name,
                short_desc,
                download_url: dl,
                thumb_url,
                downloads,
                likes,
                source: "Lethamyr".into(),
                direct_download: true,
            });
        }
    }

    let meta = val.get("meta");
    let cur_p = meta.and_then(|m| m.get("current_page")).and_then(|v| v.as_u64()).unwrap_or(1) as usize;
    let last_p = meta.and_then(|m| m.get("last_page")).and_then(|v| v.as_u64()).unwrap_or(1) as usize;
    let total = meta.and_then(|m| m.get("total")).and_then(|v| v.as_u64()).unwrap_or(items.len() as u64) as usize;

    Ok(OnlineWorkshopPage {
        page: cur_p,
        last_page: last_p,
        total,
        items,
    })
}

fn unescape_html_basic(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&#x27;", "'")
        .replace("&#39;", "'")
        .replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
        .trim()
        .to_string()
}

fn strip_html_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    unescape_html_basic(&out)
}

/// Parses one HTML page of `https://bakkesplugins.com/maps?page=N`, returning `(max_page, items)`.
pub fn parse_bakkesplugins_maps_html(html: &str) -> (usize, Vec<OnlineWorkshopItem>) {
    let mut max_page = 1usize;
    let mut scan = html;
    while let Some(pos) = scan.find("page=") {
        let tail = &scan[pos + 5..];
        let digits: String = tail.chars().take_while(|c| c.is_ascii_digit()).collect();
        if let Ok(p) = digits.parse::<usize>() {
            if p > max_page && p <= 500 {
                max_page = p;
            }
        }
        scan = &scan[pos + 5..];
    }

    let mut items = Vec::new();
    let mut seen_ids = HashSet::new();
    let mut cursor = 0usize;
    while let Some(rel) = html[cursor..].find("href=\"/maps/") {
        let tag_start = cursor + rel;
        let id_tail = &html[tag_start + 12..];
        let id_str: String = id_tail.chars().take_while(|c| c.is_ascii_digit()).collect();
        if id_str.is_empty() {
            cursor = tag_start + 12;
            continue;
        }
        let next_card = html[tag_start + 12..]
            .find("href=\"/maps/")
            .map(|n| tag_start + 12 + n)
            .unwrap_or_else(|| (tag_start + 4096).min(html.len()));
        let card = &html[tag_start..next_card];
        cursor = tag_start + 12 + id_str.len();

        if !seen_ids.insert(id_str.clone()) {
            continue;
        }

        // Extract title from <h2>...</h2>
        let name = if let Some(h2) = card.find("<h2") {
            let after_h2 = &card[h2..];
            if let Some(gt) = after_h2.find('>') {
                if let Some(end_h2) = after_h2[gt + 1..].find("</h2>") {
                    strip_html_tags(&after_h2[gt + 1..gt + 1 + end_h2])
                } else {
                    String::new()
                }
            } else {
                String::new()
            }
        } else {
            String::new()
        };
        if name.is_empty() {
            continue;
        }

        // Preview thumbnail on cdn.bakkesplugins.com/uploads/
        let thumb_url = card.find("https://cdn.bakkesplugins.com/uploads/").and_then(|u0| {
            let sub = &card[u0..];
            sub.find('"').map(|q| sub[..q].to_string())
        });

        // Short description inside <p ...>...</p>
        let short_desc = if let Some(p0) = card.find("<p ") {
            let after_p = &card[p0..];
            if let Some(gt) = after_p.find('>') {
                if let Some(end_p) = after_p[gt + 1..].find("</p>") {
                    strip_html_tags(&after_p[gt + 1..gt + 1 + end_p])
                        .lines()
                        .next()
                        .unwrap_or("")
                        .chars()
                        .take(160)
                        .collect()
                } else {
                    String::new()
                }
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        items.push(OnlineWorkshopItem {
            name,
            short_desc,
            download_url: format!("https://bakkesplugins.com/maps/{}", id_str),
            thumb_url,
            downloads: 0,
            likes: 0,
            source: "BakkesPlugins".into(),
            direct_download: true,
        });
    }

    (max_page, items)
}

/// Extracts a direct CDN archive link (`https://cdn.bakkesplugins.com/uploads/...zip/.udk/.upk`)
/// or fallback Drive/map download URL from a BakkesPlugins map detail page.
pub fn extract_bakkesplugins_download_url(html: &str) -> Option<String> {
    let has_archive_ext = |url: &str| {
        let base = url.split(['?', '#']).next().unwrap_or(url).to_lowercase();
        base.ends_with(".zip") || base.ends_with(".udk") || base.ends_with(".upk")
    };
    let mut scan = html;
    while let Some(pos) = scan.find("https://cdn.bakkesplugins.com/uploads/") {
        let sub = &scan[pos..];
        let end = sub
            .find(|c: char| matches!(c, '"' | '\'' | '<' | '>' | ' ' | '\\'))
            .unwrap_or(sub.len());
        let url = &sub[..end];
        if has_archive_ext(url) {
            return Some(url.to_string());
        }
        scan = &scan[pos + end.max(1)..];
    }
    for prefix in ["https://drive.google.com/file/d/", "https://drive.google.com/uc?"] {
        if let Some(pos) = html.find(prefix) {
            let sub = &html[pos..];
            let end = sub
                .find(|c: char| matches!(c, '"' | '\'' | '<' | '>' | ' ' | '\\'))
                .unwrap_or(sub.len());
            return Some(sub[..end].to_string());
        }
    }
    None
}

/// Queries Steam Workshop Browse pages + Valve's public `GetPublishedFileDetails/v1/` API
/// for top Rocket League Workshop maps (`appid=252950`), returning metadata + thumbnails + direct links where hosted.
fn fetch_steam_workshop_catalog_items(
    client: &reqwest::blocking::Client,
    max_pages: usize,
) -> Vec<OnlineWorkshopItem> {
    let pages: Vec<usize> = (1..=max_pages).collect();
    let per_page_ids: Vec<Vec<String>> = pages
        .into_par_iter()
        .filter_map(|p| {
            let url = format!(
                "https://steamcommunity.com/workshop/browse/?appid=252950&browsesort=totaluniquesubscribers&section=readytouseitems&p={}",
                p
            );
            let html = client.get(&url).send().ok()?.text().ok()?;
            let mut ids = Vec::new();
            let mut scan = html.as_str();
            while let Some(pos) = scan.find("sharedfiles/filedetails/?id=") {
                let tail = &scan[pos + 28..];
                let id: String = tail.chars().take_while(|c| c.is_ascii_digit()).collect();
                if id.len() >= 6 && id != "813629808" {
                    ids.push(id);
                }
                scan = &scan[pos + 28..];
            }
            Some(ids)
        })
        .collect();

    let mut seen_ids = HashSet::new();
    let mut unique_ids = Vec::new();
    for list in per_page_ids {
        for id in list {
            if seen_ids.insert(id.clone()) {
                unique_ids.push(id);
            }
        }
    }

    let chunks: Vec<Vec<String>> = unique_ids.chunks(100).map(|c| c.to_vec()).collect();
    let fetched: Vec<Vec<OnlineWorkshopItem>> = chunks
        .into_par_iter()
        .filter_map(|chunk| {
            let mut params: Vec<(String, String)> = Vec::with_capacity(chunk.len() + 1);
            params.push(("itemcount".into(), chunk.len().to_string()));
            for (i, id) in chunk.iter().enumerate() {
                params.push((format!("publishedfileids[{}]", i), id.clone()));
            }
            let val: serde_json::Value = client
                .post("https://api.steampowered.com/ISteamRemoteStorage/GetPublishedFileDetails/v1/")
                .form(&params)
                .send()
                .ok()?
                .json()
                .ok()?;
            let arr = val
                .get("response")
                .and_then(|r| r.get("publishedfiledetails"))
                .and_then(|d| d.as_array())?;
            let mut out = Vec::new();
            for obj in arr {
                if obj.get("result").and_then(|v| v.as_u64()).unwrap_or(0) != 1 {
                    continue;
                }
                let id = obj.get("publishedfileid").and_then(|v| v.as_str()).unwrap_or("");
                let name = obj.get("title").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
                if id.is_empty() || name.is_empty() {
                    continue;
                }
                let raw_desc = obj.get("description").and_then(|v| v.as_str()).unwrap_or("");
                let short_desc: String = raw_desc
                    .replace('\r', " ")
                    .replace('\n', " ")
                    .chars()
                    .take(150)
                    .collect::<String>()
                    .trim()
                    .to_string();
                let preview = obj
                    .get("preview_url")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string());
                let file_url = obj
                    .get("file_url")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                let subs = obj.get("subscriptions").and_then(|v| v.as_u64()).unwrap_or(0);
                let likes = obj.get("favorited").and_then(|v| v.as_u64()).unwrap_or(0);
                let has_direct = file_url.starts_with("http");
                out.push(OnlineWorkshopItem {
                    name,
                    short_desc,
                    download_url: if has_direct {
                        file_url
                    } else {
                        format!("https://steamcommunity.com/sharedfiles/filedetails/?id={}", id)
                    },
                    thumb_url: preview,
                    downloads: subs,
                    likes,
                    source: if has_direct { "Steam Direct" } else { "Steam Workshop" }.into(),
                    direct_download: has_direct,
                });
            }
            Some(out)
        })
        .collect();

    fetched.into_iter().flatten().collect()
}

/// Fetches ALL available pages from `lethamyr.com/api/v1/maps`, `bakkesplugins.com/maps` (+ utilities),
/// and top Steam Workshop Rocket League maps (`appid=252950`), deduplicates & caches for 24h (1,000+ maps).
pub fn fetch_all_online_workshop_maps(force_refresh: bool) -> Result<Vec<OnlineWorkshopItem>, String> {
    let cache_path = get_workshop_library_dir().join("_online_catalog_cache_v2.json");
    if !force_refresh && cache_path.is_file() {
        if let Ok(meta) = fs::metadata(&cache_path) {
            if let Ok(mod_t) = meta.modified() {
                if SystemTime::now().duration_since(mod_t).unwrap_or(Duration::MAX)
                    < Duration::from_secs(24 * 3600)
                {
                    if let Ok(raw) = fs::read_to_string(&cache_path) {
                        if let Ok(cached) = serde_json::from_str::<Vec<OnlineWorkshopItem>>(&raw) {
                            if cached.len() > 400 {
                                return Ok(cached);
                            }
                        }
                    }
                }
            }
        }
    }

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) RLSwapper/4.1")
        .build()
        .map_err(|e| e.to_string())?;

    // 1. Fetch Lethamyr API page 1 + remaining pages in parallel
    let mut leth_items = Vec::new();
    if let Ok(first) = fetch_lethamyr_catalog_page(1) {
        let last_p = first.last_page.clamp(1, 50);
        leth_items.extend(first.items);
        if last_p > 1 {
            let pages: Vec<usize> = (2..=last_p).collect();
            let extra: Vec<Vec<OnlineWorkshopItem>> = pages
                .into_par_iter()
                .filter_map(|p| fetch_lethamyr_catalog_page(p).ok().map(|pg| pg.items))
                .collect();
            for list in extra {
                leth_items.extend(list);
            }
        }
    }

    // 2. Fetch BakkesPlugins Maps (all pages + utilities category) in parallel
    let mut bp_items = Vec::new();
    if let Ok(resp) = client.get("https://bakkesplugins.com/maps?page=1").send() {
        if let Ok(html1) = resp.text() {
            let (max_p, items1) = parse_bakkesplugins_maps_html(&html1);
            bp_items.extend(items1);
            let last_bp = max_p.clamp(1, 60);
            if last_bp > 1 {
                let pages: Vec<usize> = (2..=last_bp).collect();
                let extra: Vec<Vec<OnlineWorkshopItem>> = pages
                    .into_par_iter()
                    .filter_map(|p| {
                        let url = format!("https://bakkesplugins.com/maps?page={}", p);
                        let html = client.get(&url).send().ok()?.text().ok()?;
                        Some(parse_bakkesplugins_maps_html(&html).1)
                    })
                    .collect();
                for list in extra {
                    bp_items.extend(list);
                }
            }
        }
    }
    if let Ok(resp_u) = client.get("https://bakkesplugins.com/maps?category=utilities").send() {
        if let Ok(hu) = resp_u.text() {
            bp_items.extend(parse_bakkesplugins_maps_html(&hu).1);
        }
    }

    // 3. Fetch top ~750 Steam Workshop items (provides hundreds more searchable custom maps!)
    let steam_items = fetch_steam_workshop_catalog_items(&client, 25);

    if leth_items.is_empty() && bp_items.is_empty() && steam_items.is_empty() {
        return Err("Unable to reach online workshop map catalogs.".into());
    }

    // 4. Deduplicate cleanly by normalized alphanumeric slug (Lethamyr -> BakkesPlugins -> Steam Workshop)
    //    so any map hosted on a 1-click direct CDN mirror always wins over Steam-only listings.
    let mut merged = Vec::with_capacity(leth_items.len() + bp_items.len() + steam_items.len());
    let mut seen_slugs = HashSet::new();
    for item in leth_items.into_iter().chain(bp_items.into_iter()).chain(steam_items.into_iter()) {
        let slug: String = item
            .name
            .to_lowercase()
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect();
        if slug.is_empty() || !seen_slugs.insert(slug) {
            continue;
        }
        merged.push(item);
    }

    if let Ok(json) = serde_json::to_string(&merged) {
        let _ = fs::write(&cache_path, json);
    }
    Ok(merged)
}

pub fn thumb_cache_filename(url: &str) -> String {
    let mut safe = sanitize_slug(url);
    safe.truncate(44);
    let h = url
        .bytes()
        .fold(5381u64, |acc, b| acc.wrapping_mul(33).wrapping_add(b as u64));
    format!("{}_{:08x}.png", safe, h as u32)
}

pub fn fetch_or_cache_thumb(url: &str) -> Option<RgbaImage> {
    let cache_p = get_workshop_thumb_cache_dir().join(thumb_cache_filename(url));
    if cache_p.is_file() {
        if let Ok(im) = image::open(&cache_p) {
            return Some(im.to_rgba8());
        }
    }
    let client = shared_http_client()?;
    let bytes = client.get(url).send().ok()?.bytes().ok()?;
    let im = image::load_from_memory(&bytes).ok()?.thumbnail(220, 124).to_rgba8();
    let _ = im.save(&cache_p);
    Some(im)
}

fn normalize_workshop_url(url: &str) -> String {
    // Convert https://drive.google.com/file/d/<ID>/view... to direct uc download URL
    if let Some(pos) = url.find("drive.google.com/file/d/") {
        let rest = &url[pos + "drive.google.com/file/d/".len()..];
        let id = rest.split(['/', '?', '#']).next().unwrap_or("");
        if !id.is_empty() {
            return format!("https://drive.google.com/uc?export=download&id={}", id);
        }
    }
    url.to_string()
}

fn extract_attr(tag: &str, attr: &str) -> Option<String> {
    for quote in ['"', '\''] {
        let pat = format!("{}={}", attr, quote);
        if let Some(p) = tag.find(&pat) {
            let s = &tag[p + pat.len()..];
            if let Some(end) = s.find(quote) {
                return Some(s[..end].replace("&amp;", "&"));
            }
        }
    }
    None
}

fn url_encode_param(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

/// Parses Google Drive's large-file virus-scan confirmation `<form id="download-form" action="...">`
/// HTML page into a direct binary download URL (`https://drive.usercontent.google.com/download?...`).
pub fn extract_gdrive_confirm_url(html: &str) -> Option<String> {
    let form_pos = html
        .find("id=\"download-form\"")
        .or_else(|| html.find("drive.usercontent.google.com/download"))?;
    let search_start = html[..form_pos].rfind("<form").unwrap_or(form_pos.saturating_sub(120));
    let form_tail = &html[search_start..];
    let form_open_end = form_tail.find('>')?;
    let action = extract_attr(&form_tail[..form_open_end], "action")?;
    let form_close = form_tail.find("</form>").unwrap_or(form_tail.len());
    let body = &form_tail[form_open_end + 1..form_close];

    let mut params = Vec::new();
    let mut cur = body;
    while let Some(i_pos) = cur.find("<input") {
        let after = &cur[i_pos..];
        let Some(end) = after.find('>') else { break };
        let tag = &after[..end + 1];
        if !tag.contains("type=\"submit\"") {
            if let Some(nm) = extract_attr(tag, "name") {
                let vl = extract_attr(tag, "value").unwrap_or_default();
                params.push(format!("{}={}", url_encode_param(&nm), url_encode_param(&vl)));
            }
        }
        cur = &after[end + 1..];
    }
    if params.is_empty() {
        return None;
    }
    let sep = if action.contains('?') { '&' } else { '?' };
    Some(format!("{}{}{}", action, sep, params.join("&")))
}

/// Downloads a map ZIP/UDK/UPK from `download_url` into a temporary file (automatically following
/// Google Drive / BakkesPlugins confirmation forms), reporting streaming byte progress, and imports it.
pub fn download_online_workshop_map_with_progress<F>(
    item: &OnlineWorkshopItem,
    mut on_progress: F,
) -> Result<WorkshopMapEntry, String>
where
    F: FnMut(u64, Option<u64>, &str),
{
    if !item.direct_download || item.download_url.contains("steamcommunity.com/sharedfiles") {
        return Err(format!(
            "'{}' is hosted on Steam Workshop without a direct CDN link. Click 'Steam' to subscribe/download and then 'Import Map'.",
            item.name
        ));
    }
    on_progress(0, None, "Connecting...");
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(600))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) RLSwapper/4.1")
        .build()
        .map_err(|e| e.to_string())?;

    let parse_resp_total = |r: &reqwest::blocking::Response| -> Option<u64> {
        r.headers()
            .get(reqwest::header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.trim().parse::<u64>().ok())
            .or_else(|| r.content_length())
            .filter(|&n| n > 0)
    };

    let mut target_url = normalize_workshop_url(&item.download_url);
    let mut resp = client
        .get(&target_url)
        .send()
        .map_err(|e| format!("Download failed: {e}"))?
        .error_for_status()
        .map_err(|e| format!("Download HTTP error: {e}"))?;

    let mut total_bytes = parse_resp_total(&resp);
    let mut ctype = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_lowercase();

    // Read small initial prefix or HTML page to detect Google Drive virus-scan interstitial
    let mut head_buf = vec![0u8; 64];
    let mut head_len = 0usize;
    while head_len < 64 {
        match resp.read(&mut head_buf[head_len..]) {
            Ok(0) => break,
            Ok(n) => head_len += n,
            Err(e) => return Err(format!("Download read error: {e}")),
        }
    }
    head_buf.truncate(head_len);

    // Follow up to 2 HTML intermediate hops (e.g. bakkesplugins.com/maps/<id> -> cdn .zip,
    // or drive.google.com viewer -> virus-scan confirmation URL).
    for hop in 0..3 {
        let looks_html = ctype.contains("text/html")
            || head_buf.starts_with(b"<!DOCTYPE")
            || head_buf.starts_with(b"<html")
            || head_buf.starts_with(b"<HTML");
        if !looks_html {
            break;
        }
        if hop == 2 {
            return Err(format!(
                "Mirror still requires interactive browser download for '{}'. Click 'Browser' then Import.",
                item.name
            ));
        }
        on_progress(0, None, "Resolving download mirror...");
        let mut html_bytes = head_buf.clone();
        let _ = resp.take(512 * 1024).read_to_end(&mut html_bytes);
        let html_str = String::from_utf8_lossy(&html_bytes);
        let next_url = extract_bakkesplugins_download_url(&html_str)
            .or_else(|| extract_gdrive_confirm_url(&html_str));
        let Some(confirm_url) = next_url else {
            return Err(format!(
                "Host returned HTML page without a direct download link for '{}'. Click 'Browser' to download and import manually.",
                item.name
            ));
        };
        target_url = normalize_workshop_url(&confirm_url);
        resp = client
            .get(&target_url)
            .send()
            .map_err(|e| format!("Confirmed download failed: {e}"))?
            .error_for_status()
            .map_err(|e| format!("Confirmed download HTTP error: {e}"))?;
        total_bytes = parse_resp_total(&resp);
        ctype = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_lowercase();
        head_buf = vec![0u8; 64];
        head_len = 0;
        while head_len < 64 {
            match resp.read(&mut head_buf[head_len..]) {
                Ok(0) => break,
                Ok(n) => head_len += n,
                Err(e) => return Err(format!("Download read error: {e}")),
            }
        }
        head_buf.truncate(head_len);
    }

    let tmp_ext = if head_buf.starts_with(b"PK\x03\x04") {
        "zip"
    } else if head_buf.len() >= 4 && read_u32_le(&head_buf, 0) == PACKAGE_FILE_TAG {
        "udk"
    } else if head_buf.starts_with(b"Rar!") {
        return Err(format!(
            "'{}' is packaged as a .RAR archive. Click 'Browser' to extract the .UDK and import it.",
            item.name
        ));
    } else if head_buf.starts_with(&[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C]) {
        return Err(format!(
            "'{}' is packaged as a .7Z archive. Click 'Browser' to extract the .UDK and import it.",
            item.name
        ));
    } else {
        return Err("Downloaded file is not a valid .ZIP, .UDK, or .UPK map package.".into());
    };

    let mut downloaded = head_buf.len() as u64;
    on_progress(downloaded, total_bytes, "Downloading...");

    let tmp_path = get_workshop_library_dir().join(format!("_dl_{}.{}", sanitize_slug(&item.name), tmp_ext));
    {
        let mut out_f = File::create(&tmp_path).map_err(|e| e.to_string())?;
        io::Write::write_all(&mut out_f, &head_buf).map_err(|e| e.to_string())?;
        let mut buf = [0u8; 65536];
        loop {
            match resp.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    io::Write::write_all(&mut out_f, &buf[..n])
                        .map_err(|e| format!("Stream write failed: {e}"))?;
                    downloaded += n as u64;
                    on_progress(downloaded, total_bytes, "Downloading...");
                }
                Err(e) => {
                    let _ = fs::remove_file(&tmp_path);
                    return Err(format!("Stream download failed: {e}"));
                }
            }
        }
    }

    on_progress(downloaded, total_bytes, "Extracting & importing map...");
    let res = import_workshop_file_or_zip(&tmp_path, Some(&item.name), Some(&item.download_url));
    let _ = fs::remove_file(&tmp_path);

    if let (Ok(mut entry), Some(ref turl)) = (res.as_ref().map(|e| e.clone()), item.thumb_url.as_ref()) {
        if entry.thumb_file_rel.is_none() {
            if let Some(th) = fetch_or_cache_thumb(turl) {
                let tdest = get_workshop_library_dir().join(&entry.id).join("thumb.png");
                if th.save(&tdest).is_ok() {
                    entry.thumb_file_rel = Some("thumb.png".into());
                    let _ = fs::write(
                        get_workshop_library_dir().join(&entry.id).join("workshop.json"),
                        serde_json::to_string_pretty(&entry).unwrap(),
                    );
                }
            }
        }
        return Ok(entry);
    }
    res
}

/// Downloads a map ZIP/UDK/UPK from `download_url` into a temporary file and imports it.
pub fn download_online_workshop_map(item: &OnlineWorkshopItem) -> Result<WorkshopMapEntry, String> {
    download_online_workshop_map_with_progress(item, |_, _, _| {})
}

/// Returns true if an online catalog entry represents the shared Workshop Textures pack rather than a single map.
pub fn is_workshop_textures_item(item: &OnlineWorkshopItem) -> bool {
    let low_nm = item.name.trim().to_lowercase();
    let low_url = item.download_url.to_lowercase();
    low_nm == "workshop textures"
        || low_nm.contains("workshop textures")
        || low_url.ends_with("/maps/96")
}

/// Returns how many `.upk` / `.tfc` files are currently cached inside `Downloads\RL Swapper\Workshop Textures\`.
pub fn count_cached_workshop_textures() -> usize {
    let tex_dir = get_workshop_textures_dir();
    let Ok(entries) = fs::read_dir(&tex_dir) else {
        return 0;
    };
    entries
        .flatten()
        .filter(|e| {
            let p = e.path();
            p.is_file()
                && matches!(
                    p.extension().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase().as_str(),
                    "upk" | "tfc"
                )
        })
        .count()
}

/// Directly downloads the official ~44.5 MB `Workshop Textures` pack (`https://bakkesplugins.com/maps/96` / CDN fallback),
/// caches all `.upk` and `.tfc` files inside `Downloads\RL Swapper\Workshop Textures\`, and installs missing ones
/// into `cooked_dir`. Returns `(copied_to_cooked, skipped_existing, total_cached)`.
pub fn download_and_install_workshop_textures_with_progress<F>(
    cooked_dir: &Path,
    force_redownload: bool,
    mut on_progress: F,
) -> Result<(usize, usize, usize), String>
where
    F: FnMut(u64, Option<u64>, &str),
{
    let tex_dir = get_workshop_textures_dir();
    let existing_cached = count_cached_workshop_textures();
    if !force_redownload && existing_cached >= 5 {
        on_progress(1, Some(1), "Installing cached Workshop Textures...");
        let (copied, skipped) = install_workshop_textures_folder(&tex_dir, cooked_dir)?;
        return Ok((copied, skipped, existing_cached));
    }

    on_progress(0, None, "Resolving Workshop Textures mirror...");
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(600))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) RLSwapper/4.1")
        .build()
        .map_err(|e| e.to_string())?;

    let direct_url = client
        .get(WORKSHOP_TEXTURES_PAGE_URL)
        .send()
        .ok()
        .and_then(|r| r.text().ok())
        .and_then(|html| extract_bakkesplugins_download_url(&html))
        .unwrap_or_else(|| WORKSHOP_TEXTURES_CDN_FALLBACK_URL.to_string());

    let mut resp = client
        .get(&direct_url)
        .send()
        .map_err(|e| format!("Workshop Textures download failed: {e}"))?
        .error_for_status()
        .map_err(|e| format!("Workshop Textures HTTP error: {e}"))?;

    let total_bytes = resp
        .headers()
        .get(reqwest::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.trim().parse::<u64>().ok())
        .or_else(|| resp.content_length())
        .filter(|&n| n > 0);

    let tmp_zip = tex_dir.join("_workshop_textures_dl.zip");
    let mut downloaded = 0u64;
    on_progress(0, total_bytes, "Downloading Workshop Textures...");
    {
        let mut out_f = File::create(&tmp_zip).map_err(|e| e.to_string())?;
        let mut buf = [0u8; 65536];
        loop {
            match resp.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    io::Write::write_all(&mut out_f, &buf[..n])
                        .map_err(|e| format!("Failed writing textures archive: {e}"))?;
                    downloaded += n as u64;
                    on_progress(downloaded, total_bytes, "Downloading Workshop Textures...");
                }
                Err(e) => {
                    let _ = fs::remove_file(&tmp_zip);
                    return Err(format!("Workshop Textures stream error: {e}"));
                }
            }
        }
    }

    on_progress(downloaded, total_bytes, "Extracting Workshop Textures...");
    let extracted_res: Result<usize, String> = (|| {
        let f = File::open(&tmp_zip).map_err(|e| e.to_string())?;
        let mut archive = ZipArchive::new(f).map_err(|e| format!("Invalid textures ZIP: {e}"))?;
        let mut extracted = 0usize;
        for i in 0..archive.len() {
            let mut zf = archive.by_index(i).map_err(|e| e.to_string())?;
            if !zf.is_file() {
                continue;
            }
            let base_nm = Path::new(zf.name())
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            if base_nm.is_empty() || base_nm.starts_with('.') {
                continue;
            }
            let ext = Path::new(&base_nm)
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if ext == "upk" || ext == "tfc" {
                let dst = tex_dir.join(&base_nm);
                let mut out = File::create(&dst).map_err(|e| e.to_string())?;
                io::copy(&mut zf, &mut out).map_err(|e| e.to_string())?;
                extracted += 1;
            }
        }
        Ok(extracted)
    })();
    let _ = fs::remove_file(&tmp_zip);
    let extracted_count = extracted_res?;
    if extracted_count == 0 {
        return Err("No .UPK or .TFC texture packages found in downloaded archive.".into());
    }

    on_progress(downloaded, total_bytes, "Installing into CookedPCConsole...");
    let (copied, skipped) = install_workshop_textures_folder(&tex_dir, cooked_dir)?;
    Ok((copied, skipped, count_cached_workshop_textures()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thumb_cache_filename_avoids_steam_cdn_prefix_collisions() {
        let a = thumb_cache_filename("https://steamuserimages-a.akamaihd.net/ugc/1111111111111111111/AAAA/");
        let b = thumb_cache_filename("https://steamuserimages-a.akamaihd.net/ugc/1111111111111111111/BBBB/");
        assert_ne!(a, b, "Steam UGC URLs sharing >44 prefix chars must hash differently");
    }

    #[test]
    fn bakkesplugins_download_url_supports_query_and_gdrive() {
        let html_q = r#"<a href="https://cdn.bakkesplugins.com/uploads/maps/Rings.zip?v=2">DL</a>"#;
        assert_eq!(
            extract_bakkesplugins_download_url(html_q).as_deref(),
            Some("https://cdn.bakkesplugins.com/uploads/maps/Rings.zip?v=2")
        );
        let html_gd = r#"<a href="https://drive.google.com/file/d/1AbCdEfG/view?usp=sharing">Drive</a>"#;
        assert_eq!(
            extract_bakkesplugins_download_url(html_gd).as_deref(),
            Some("https://drive.google.com/file/d/1AbCdEfG/view?usp=sharing")
        );
    }

    #[test]
    fn delete_library_map_rejects_traversal() {
        let bad = WorkshopMapEntry {
            id: "../outside".into(),
            name: "Bad".into(),
            description: String::new(),
            map_file_rel: "x.udk".into(),
            thumb_file_rel: None,
            extra_files_rel: vec![],
            source_url: None,
        };
        assert!(delete_library_map(&bad).is_err());
    }

    #[test]
    fn detects_workshop_textures_utility_entry() {
        let item = OnlineWorkshopItem {
            name: "Workshop Textures".into(),
            short_desc: "Missing textures fix".into(),
            download_url: "https://bakkesplugins.com/maps/96".into(),
            thumb_url: None,
            downloads: 0,
            likes: 0,
            source: "BakkesPlugins".into(),
            direct_download: true,
        };
        assert!(is_workshop_textures_item(&item));
    }
}
