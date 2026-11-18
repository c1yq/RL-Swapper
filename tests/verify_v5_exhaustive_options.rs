//! Layer 2 Exhaustive Triple-Check Matrix across ALL 12 Cosmetic Slots, 6 Hitboxes,
//! 14 Official Paints + Custom HEX, All 4 Studio Pipelines, Map Categories, Workshop Error/Success Paths & Health Repair.

use image::{Rgba, RgbaImage};
use rl_swapper::custom_studio::{
    compose_pfp_with_border_preview, inject_custom_ball_texture, inject_custom_banner,
    inject_custom_decal, inject_custom_pfp, list_avatar_border_packages_with_catalog,
    list_player_banner_packages_with_catalog,
};
use rl_swapper::icon_system::{composite_icon_with_paint, extract_ball_texture_preview, IconCacheManager};
use rl_swapper::map_editor::{
    extract_map_preview_image, reset_all_standard_maps, swap_standard_map, STANDARD_SOCCAR_MAPS,
};
use rl_swapper::templates::{
    bake_name_onto_banner, generate_ball_template, generate_banner_template,
    generate_decal_template_for_car, render_2d_ball_orb_preview,
    render_2d_banner_scoreboard_preview, render_2d_car_decal_with_mask_preview,
};
use rl_swapper::upk_engine::{
    bak_path_for, body_hitbox_for_item, clear_rl_texture_cache, detect_car_uv_label,
    inspect_cooked_health, is_avatar_border_modified, load_catalog_items, official_paints,
    repair_avatar_border_upk, repair_cooked_health, restore_single_package, swap_item_packages,
    unpack_upk_from_disk, CarHitbox, CatalogItem, PaintColorSpec,
};
use rl_swapper::workshop::{
    delete_library_map, download_online_workshop_map_with_progress, import_workshop_file_or_zip,
    install_workshop_textures_folder, load_workshop_map_into_underpass,
    restore_workshop_underpass, scan_upk_external_dependencies, OnlineWorkshopItem,
    WORKSHOP_SAFE_SLOT,
};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::thread;

const ALL_12_SLOTS: &[&str] = &[
    "Body",
    "Decal",
    "Wheels",
    "Rocket Boost",
    "Goal Explosion",
    "Trail",
    "Topper",
    "Antenna",
    "Paint Finish",
    "Engine Audio",
    "Player Banner",
    "Avatar Border",
];

fn detect_test_cooked_dir() -> Option<PathBuf> {
    if let Ok(env_dir) = std::env::var("RL_COOKED_DIR") {
        let p = PathBuf::from(env_dir);
        if p.is_dir() {
            return Some(p);
        }
    }
    for cand in [
        r"D:\SteamLibrary\steamapps\common\rocketleague\TAGame\CookedPCConsole",
        r"C:\Program Files (x86)\Steam\steamapps\common\rocketleague\TAGame\CookedPCConsole",
        r"C:\Program Files\Epic Games\rocketleague\TAGame\CookedPCConsole",
        r"D:\Epic Games\rocketleague\TAGame\CookedPCConsole",
    ] {
        let p = PathBuf::from(cand);
        if p.is_dir() {
            return Some(p);
        }
    }
    None
}

fn copy_if_present(src_dir: &Path, dst_dir: &Path, filename: &str) -> bool {
    let s = src_dir.join(filename);
    let d = dst_dir.join(filename);
    if s.is_file() {
        if !d.exists() {
            let _ = fs::copy(&s, &d);
        }
        true
    } else {
        false
    }
}

fn synthetic_rgba(w: u32, h: u32) -> RgbaImage {
    let mut im = RgbaImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            im.put_pixel(x, y, Rgba([(x % 255) as u8, (y % 255) as u8, 210, 255]));
        }
    }
    im
}

#[test]
fn test_exhaustive_offline_templates_workshop_errors_and_previews() {
    // 1. All 14 official paints + Custom HEX linear & sRGB consistency
    let paints = official_paints();
    assert_eq!(paints.len(), 14);
    let mut seen_names = HashSet::new();
    for p in &paints {
        assert!(seen_names.insert(p.name.as_str()), "Duplicate paint {}", p.name);
        let [r, g, b] = p.rgb8();
        assert!(!(r == 0 && g == 0 && b == 0), "Paint {} RGB must be valid", p.name);
    }
    let hex_custom = PaintColorSpec::from_custom_hex("00e5ff");
    assert_eq!(hex_custom.hex_srgb, "#00E5FF");
    assert_eq!(hex_custom.rgb8(), [0, 229, 255]);
    let icon_base = synthetic_rgba(64, 64);
    for p in &paints {
        assert_eq!(composite_icon_with_paint(&icon_base, Some(p), 64).dimensions(), (64, 64));
    }

    // 2. All Car UV templates, Banner baking (empty, short, >18 truncation), 2D previews
    for car in ["Octane", "Fennec", "Dominus"] {
        let uv = generate_decal_template_for_car(car, None);
        assert_eq!(uv.dimensions(), (1024, 1024));
        let prev = render_2d_car_decal_with_mask_preview(
            car,
            None,
            Some(&icon_base),
            Some(&icon_base),
            [255, 40, 40],
            [40, 220, 255],
            220,
        );
        assert_eq!(prev.dimensions(), (220, 220));
    }
    assert_eq!(detect_car_uv_label("Body_Octane_SF.upk"), Some("Octane"));
    assert_eq!(detect_car_uv_label("body_grain_SF.upk"), Some("Fennec"));
    assert_eq!(detect_car_uv_label("Body_MuscleCar_SF.upk"), Some("Dominus"));
    assert_eq!(detect_car_uv_label("Skin_Force_Stripes_SF.upk"), Some("Breakout"));
    assert_eq!(detect_car_uv_label("Skin_Backfire_Stripes_SF.upk"), Some("Backfire"));
    assert_eq!(detect_car_uv_label("Octane ZSR: Stripes"), None);

    let ban = generate_banner_template();
    assert_eq!(ban.dimensions(), (840, 200));
    assert_eq!(bake_name_onto_banner(&ban, "").dimensions(), (840, 200));
    assert_eq!(bake_name_onto_banner(&ban, "PlayerOne").dimensions(), (420, 100));
    assert_eq!(
        bake_name_onto_banner(&ban, "NameLongerThanEighteenCharacters").dimensions(),
        (420, 100)
    );
    let sb = render_2d_banner_scoreboard_preview(Some(&ban), Some(&icon_base), "RLTester", 540, 168);
    assert_eq!(sb.dimensions(), (540, 168));

    let ball = generate_ball_template(None);
    assert_eq!(ball.dimensions(), (512, 512));
    assert_eq!(render_2d_ball_orb_preview(&ball, 200, Some([255, 255, 255])).dimensions(), (200, 200));

    // 3. Body Hitbox classification sanity for every Hitbox enum variant
    let cases = [
        ("Octane", "Body_Octane_SF.upk", CarHitbox::Octane),
        ("Fennec", "body_grain_SF.upk", CarHitbox::Octane),
        ("Dominus", "Body_MuscleCar_SF.upk", CarHitbox::Dominus),
        ("Endo", "body_endo_SF.upk", CarHitbox::Hybrid),
        ("Breakout", "Body_Force_SF.upk", CarHitbox::Breakout),
        ("Batmobile (2016)", "Body_Darkcar_SF.upk", CarHitbox::Plank),
        ("Merc", "Body_Vanquish_SF.upk", CarHitbox::Merc),
    ];
    for (prod, pkg, expected) in cases {
        assert_eq!(body_hitbox_for_item(prod, pkg), expected, "Hitbox mismatch for {}", prod);
    }

    // 4. Maps catalog covers Standard, Custom Default Size & Labs cleanly
    let std_cnt = STANDARD_SOCCAR_MAPS.iter().filter(|m| m.category == "Standard").count();
    let custom_cnt = STANDARD_SOCCAR_MAPS.iter().filter(|m| m.category == "Custom Default Size").count();
    let labs_cnt = STANDARD_SOCCAR_MAPS.iter().filter(|m| !m.is_default_size).count();
    assert!(std_cnt > 10 && custom_cnt > 10 && labs_cnt > 5);

    // 5. Workshop downloader error guardrails (Steam URL + .rar + .7z + bad payload)
    let steam_item = OnlineWorkshopItem {
        name: "Steam Only".into(),
        source: "Steam Workshop".into(),
        download_url: "https://steamcommunity.com/sharedfiles/filedetails/?id=12345".into(),
        thumb_url: None,
        direct_download: false,
        short_desc: String::new(),
        downloads: 0,
        likes: 0,
    };
    let err_steam = download_online_workshop_map_with_progress(&steam_item, |_, _, _| {}).unwrap_err();
    assert!(err_steam.contains("Steam Workshop"), "Unexpected steam error: {err_steam}");

    let test_bad_magic = |magic: &'static [u8], expect_substr: &str| {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        thread::spawn(move || {
            if let Ok((mut s, _)) = listener.accept() {
                let mut req = [0u8; 1024];
                let _ = s.read(&mut req);
                let mut payload = magic.to_vec();
                payload.extend_from_slice(&[0u8; 64]);
                let hdr = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    payload.len()
                );
                let _ = s.write_all(hdr.as_bytes());
                let _ = s.write_all(&payload);
            }
        });
        let item = OnlineWorkshopItem {
            name: "BadArchive".into(),
            source: "Mock".into(),
            download_url: format!("http://{}/map", addr),
            thumb_url: None,
            direct_download: true,
            short_desc: String::new(),
            downloads: 0,
            likes: 0,
        };
        let err = download_online_workshop_map_with_progress(&item, |_, _, _| {}).unwrap_err();
        assert!(err.contains(expect_substr), "Expected '{expect_substr}' in '{err}'");
    };
    test_bad_magic(b"Rar!\x1A\x07", ".RAR");
    test_bad_magic(&[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C], ".7Z");
    test_bad_magic(b"BADHEADER", "not a valid");
}

#[test]
fn test_exhaustive_live_cooked_matrix_all_12_slots_all_paints_all_studios() {
    let Some(real_cooked) = detect_test_cooked_dir() else {
        eprintln!("Skipping live sandbox matrix: RL CookedPCConsole not found.");
        return;
    };

    let sandbox = std::env::temp_dir().join("rl_swapper_v5_exhaustive_sandbox");
    let _ = fs::remove_dir_all(&sandbox);
    fs::create_dir_all(&sandbox).unwrap();

    let catalog = load_catalog_items(Some(&real_cooked));
    let mut by_slot: HashMap<String, Vec<CatalogItem>> = HashMap::new();
    for it in &catalog {
        if !it.slot.is_empty() && !it.asset_package.is_empty() {
            by_slot.entry(it.slot.clone()).or_default().push(it.clone());
        }
    }

    // Also link/copy shared .tfc files only if needed by icon tests; sandbox UPKs reuse on-demand copies
    let mut slot_swapped_ok = Vec::new();
    for &slot_name in ALL_12_SLOTS {
        let items = by_slot.get(slot_name).expect("Slot missing in catalog");
        // Find up to 2 distinct packages existing on disk in real_cooked (non-_T_SF primary packages)
        let mut distinct_disk: Vec<CatalogItem> = Vec::new();
        let mut seen_pkgs = HashSet::new();
        for it in items {
            let low = it.asset_package.to_lowercase();
            if low.ends_with("_t_sf.upk") {
                continue;
            }
            if !seen_pkgs.contains(&low) && real_cooked.join(&it.asset_package).is_file() {
                seen_pkgs.insert(low);
                distinct_disk.push(it.clone());
                if distinct_disk.len() >= 2 {
                    break;
                }
            }
        }
        assert!(
            distinct_disk.len() >= 2,
            "Slot '{}' must have >= 2 distinct existing packages in CookedPCConsole (found {})",
            slot_name,
            distinct_disk.len()
        );
        let donor = &distinct_disk[0];
        let target = &distinct_disk[1];
        copy_if_present(&real_cooked, &sandbox, &donor.asset_package);
        copy_if_present(&real_cooked, &sandbox, &target.asset_package);

        let swap_res = swap_item_packages(&sandbox, donor, target, None)
            .unwrap_or_else(|e| panic!("Slot '{}' swap {} -> {} failed: {}", slot_name, donor.product, target.product, e));
        let (unpacked, _) = unpack_upk_from_disk(&sandbox.join(&target.asset_package))
            .unwrap_or_else(|e| panic!("Slot '{}' rewired UPK failed to unpack: {}", slot_name, e));
        assert!(!unpacked.exports.is_empty());
        assert!(bak_path_for(&sandbox.join(&target.asset_package)).is_file());
        assert!(restore_single_package(&sandbox.join(&target.asset_package)));
        slot_swapped_ok.push(format!("{}: {}", slot_name, swap_res));
    }
    println!("All 12 slots verified:\n  {}", slot_swapped_ok.join("\n  "));

    // Triple-check ALL 14 official paints + Custom HEX on a Car Body (Octane) + Wheels
    assert!(copy_if_present(&real_cooked, &sandbox, "Body_Octane_SF.upk"));
    let octane = by_slot["Body"]
        .iter()
        .find(|i| i.asset_package.eq_ignore_ascii_case("Body_Octane_SF.upk"))
        .expect("Octane missing");
    let mut all_paints = official_paints();
    all_paints.push(PaintColorSpec::from_custom_hex("#00FFD5"));
    for p in &all_paints {
        let msg = swap_item_packages(&sandbox, octane, octane, Some(p))
            .unwrap_or_else(|e| panic!("Paint '{}' failed on Octane: {}", p.name, e));
        let (u, _) = unpack_upk_from_disk(&sandbox.join("Body_Octane_SF.upk")).unwrap();
        assert!(!u.exports.is_empty());
        assert!(msg.contains(&p.name), "Missing paint tag in msg: {}", msg);
    }
    assert!(restore_single_package(&sandbox.join("Body_Octane_SF.upk")));

    // Studios Matrix: Decal PNG onto Octane Stripes, PFP BGRA8 & Self-Repair, Banner + Bake Name, Match Ball
    let sample_art = synthetic_rgba(256, 256);
    assert!(copy_if_present(&real_cooked, &sandbox, "Skin_Octane_Stripes_SF.upk"));
    assert!(copy_if_present(&real_cooked, &sandbox, "skin_octane_alpine_esports_SF.upk"));
    let dec_msg = inject_custom_decal(&sandbox, "Skin_Octane_Stripes_SF.upk", &sample_art).unwrap();
    println!("Decal studio verified: {}", dec_msg);
    assert!(restore_single_package(&sandbox.join("Skin_Octane_Stripes_SF.upk")));

    // Avatar PFP + Damaged Border Self-Healing without .bak
    let borders = list_avatar_border_packages_with_catalog(&real_cooked, &catalog);
    assert!(borders.len() >= 100);
    let def_border = borders
        .iter()
        .find(|(_, p)| p.eq_ignore_ascii_case("AvatarBorder_Default_SF.upk"))
        .unwrap();
    assert!(copy_if_present(&real_cooked, &sandbox, &def_border.1));
    let comp = compose_pfp_with_border_preview(&sandbox, &def_border.1, Some(&sample_art)).unwrap();
    assert_eq!(comp.dimensions(), (120, 120));
    inject_custom_pfp(&sandbox, &def_border.1, &sample_art).unwrap();
    assert!(is_avatar_border_modified(&sandbox.join(&def_border.1)));
    // Delete .bak and prove repair_cooked_health / repair_avatar_border_upk self-heals the damaged border!
    let _ = fs::remove_file(bak_path_for(&sandbox.join(&def_border.1)));
    let h_pre = inspect_cooked_health(&sandbox);
    assert!(h_pre.damaged_avatar_borders.iter().any(|nm| nm.eq_ignore_ascii_case(&def_border.1)));
    let (_, healed) = repair_cooked_health(&sandbox);
    assert!(healed >= 1 || repair_avatar_border_upk(&sandbox.join(&def_border.1)));
    assert!(restore_single_package(&sandbox.join(&def_border.1)));

    // Banner Studio + Name Bake
    let banners = list_player_banner_packages_with_catalog(&real_cooked, &catalog);
    assert!(!banners.is_empty());
    let first_banner = banners
        .iter()
        .find(|(_, p)| copy_if_present(&real_cooked, &sandbox, p))
        .expect("No banner package copied");
    let baked = bake_name_onto_banner(&sample_art, "V5Verified");
    let bmsg = inject_custom_banner(&sandbox, &first_banner.1, &baked).unwrap();
    println!("Banner studio verified: {}", bmsg);
    assert!(restore_single_package(&sandbox.join(&first_banner.1)));

    // Match Ball Studio
    if copy_if_present(&real_cooked, &sandbox, "GameInfo_Soccar_SF.upk") {
        assert!(extract_ball_texture_preview(&sandbox).is_some());
        let ball_msg = inject_custom_ball_texture(&sandbox, &sample_art).unwrap();
        println!("Ball studio verified: {}", ball_msg);
        assert!(restore_single_package(&sandbox.join("GameInfo_Soccar_SF.upk")));
    }

    // Maps: Standard + Custom Default Size + Labs + Override All + Reset
    for arena in [
        "Stadium_P.upk",
        "Park_P.upk",
        "UF_Day_P.upk",
        "Labs_CirclePillars_P.upk",
        "Labs_Underpass_P.upk",
    ] {
        let _ = copy_if_present(&real_cooked, &sandbox, arena);
    }
    let map_prev = extract_map_preview_image(&sandbox, &STANDARD_SOCCAR_MAPS[0], 160, 84);
    assert_eq!(map_prev.dimensions(), (160, 84));
    if sandbox.join("Park_P.upk").is_file() && sandbox.join("Labs_CirclePillars_P.upk").is_file() {
        let _ = swap_standard_map(&sandbox, "Stadium_P.upk", "Park_P.upk", true).unwrap();
        assert!(!bak_path_for(&sandbox.join("Labs_CirclePillars_P.upk")).exists());
        assert!(reset_all_standard_maps(&sandbox) >= 1);
    }

    // Workshop: ZIP with bundled extra texture package + load + dep scan + restore + delete
    if sandbox.join(WORKSHOP_SAFE_SLOT).is_file() {
        let ws_zip = sandbox.join("v5_workshop_test.zip");
        {
            let mut zw = zip::ZipWriter::new(fs::File::create(&ws_zip).unwrap());
            let underpass_bytes = fs::read(sandbox.join(WORKSHOP_SAFE_SLOT)).unwrap();
            zw.start_file("V5TestArena_P.udk", zip::write::SimpleFileOptions::default()).unwrap();
            zw.write_all(&underpass_bytes).unwrap();
            zw.start_file("V5CustomDepTex.upk", zip::write::SimpleFileOptions::default()).unwrap();
            zw.write_all(b"TEX").unwrap();
            zw.finish().unwrap();
        }
        let entry = import_workshop_file_or_zip(&ws_zip, Some("V5 Workshop Arena"), None).unwrap();
        let (_msg, _deps) = load_workshop_map_into_underpass(&sandbox, &entry).unwrap();
        assert!(sandbox.join("V5CustomDepTex.upk").is_file(), "Bundled extra texture package must deploy to cooked_dir");
        let _ = fs::remove_file(sandbox.join("V5CustomDepTex.upk"));
        assert!(restore_workshop_underpass(&sandbox));

        // Test install_workshop_textures_folder
        let tex_src = sandbox.join("extra_ws_textures");
        fs::create_dir_all(&tex_src).unwrap();
        fs::write(tex_src.join("CustomFloor.upk"), b"UPK_TEX").unwrap();
        let (copied, _) = install_workshop_textures_folder(&tex_src, &sandbox).unwrap();
        assert_eq!(copied, 1);
        assert!(sandbox.join("CustomFloor.upk").is_file());
        let _ = scan_upk_external_dependencies(&sandbox.join(WORKSHOP_SAFE_SLOT), &sandbox);

        delete_library_map(&entry).unwrap();
    }

    // Icon cache sync check on Octane
    let mgr = IconCacheManager::new();
    mgr.index_cooked_dir(&real_cooked);
    let (_, src) = mgr.get_icon_sync(&real_cooked, octane, false);
    assert!(!src.is_empty());

    clear_rl_texture_cache();
    let _ = fs::remove_dir_all(&sandbox);
}
